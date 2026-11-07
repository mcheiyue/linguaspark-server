#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

# Run: python qa/contract_check.py --base-url http://127.0.0.1:8080 --output qa/contract-report.json

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass, field
import json
from pathlib import Path
from typing import TypedDict
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

from quality import structural_errors


class Case(TypedDict, total=False):
    name: str
    method: str
    path: str
    body: dict[str, str | bool | list[str]]
    status: int
    checks: list[str]
    preserve: list[str]
    error: str


@dataclass(frozen=True, slots=True)
class CheckResult:
    name: str
    status: int | None
    input: str | dict[str, str | bool | list[str]] | None
    output: str | dict[str, str] | list[str] | None
    response_text: str
    checks: dict[str, bool] = field(default_factory=dict)
    errors: list[str] = field(default_factory=list)


def check_response(case: Case, status: int, raw: str) -> CheckResult:
    """Evaluate one observed HTTP response against its declared contract."""
    body = case.get("body")
    checks: dict[str, bool] = {"status": status == case["status"]}
    errors = [] if checks["status"] else [f"expected_http_{case['status']}_got_{status}"]
    output: str | dict[str, str] | list[str] | None = None
    if status == 200:
        try:
            decoded = json.loads(raw)
        except json.JSONDecodeError as error:
            errors.append(f"invalid_json:{error.msg}")
        else:
            if case.get("path") == "/languages":
                valid = isinstance(decoded, dict) and all(isinstance(key, str) and isinstance(value, list) and all(isinstance(item, str) for item in value) for key, value in decoded.items())
                checks["languages_shape"] = valid
                if not valid:
                    errors.append("invalid_languages_shape")
            elif isinstance(decoded, dict) and isinstance(decoded.get("text"), (str, list)):
                output = decoded["text"]
                expected_text = body.get("text") if body else None
                expected_batch = isinstance(expected_text, list)
                checks["response_shape"] = isinstance(decoded.get("from"), str) and isinstance(decoded.get("to"), str) and isinstance(decoded.get("text"), list if expected_batch else str)
                checks["target_language"] = bool(body) and decoded.get("to") == body.get("to")
                expected_source = None
                if body:
                    requested_source = body.get("from")
                    if isinstance(requested_source, str) and requested_source != "auto":
                        expected_source = requested_source
                    elif expected_batch:
                        expected_source = "mul"
                    elif isinstance(expected_text, str) and not expected_text.strip():
                        expected_source = "und"
                checks["source_language"] = isinstance(decoded.get("from"), str) and (expected_source is None or decoded.get("from") == expected_source)
                if not all(checks[key] for key in ("response_shape", "target_language", "source_language")):
                    errors.append("invalid_native_response_shape")
            else:
                errors.append("invalid_native_response_shape")
    elif status >= 400:
        try:
            decoded_error = json.loads(raw)
        except json.JSONDecodeError:
            decoded_error = {}
        if isinstance(decoded_error, dict):
            error_value = decoded_error.get("error", "")
            output = {"error": error_value} if isinstance(error_value, str) else {"error": ""}
        if "error" in case:
            checks["error_detail"] = case["error"].casefold() in raw.casefold()
            if not checks["error_detail"]:
                errors.append("expected_error_detail_missing")
    return CheckResult(case["name"], status, body, output, raw, checks, errors)


def evaluate_behavior(case: Case, result: CheckResult) -> CheckResult:
    """Add output assertions that do not depend on generated wording."""
    checks = dict(result.checks)
    errors = list(result.errors)
    body = case.get("body")
    output = result.output
    if result.status != 200 or body is None or not isinstance(output, (str, list)):
        return result
    texts = body.get("text")
    inputs = texts if isinstance(texts, list) else [texts] if isinstance(texts, str) else []
    outputs = output if isinstance(output, list) else [output]
    checks["item_count_and_order"] = bool(inputs) and len(inputs) == len(outputs)
    if checks["item_count_and_order"]:
        checks["item_count_and_order"] = all(isinstance(item, str) for item in outputs)
    if not checks["item_count_and_order"]:
        errors.append("native_batch_shape_or_order_invalid")
        return CheckResult(result.name, result.status, result.input, result.output, result.response_text, checks, errors)
    expectations = case.get("checks", [])
    source = inputs[0] if inputs else ""
    translated = outputs[0] if outputs else ""
    checks["preserved_literals"] = all(
        not structural_errors(input_text, output_text, case.get("preserve", []), body.get("html", False))
        for input_text, output_text in zip(inputs, outputs)
    )
    if not checks["preserved_literals"]:
        errors.append("behavior_check_failed:preserve")
    for expectation in expectations:
        passed = False
        if expectation == "passthrough":
            passed = source == translated
        elif expectation == "whitespace":
            passed = source == translated
        elif expectation == "translated":
            passed = bool(translated.strip()) and source != translated
        elif expectation == "structural":
            passed = not structural_errors(source, translated, [], False)
        elif expectation == "html":
            passed = not structural_errors(source, translated, [], True) and "&lt;span&gt;" in translated
        elif expectation == "fenced_code":
            passed = "```rust\nlet value = 42;\n```" in translated
        elif expectation == "batch_mixed":
            passed = len(outputs) == 3 and outputs[1] == inputs[1] and outputs[2] == inputs[2] and outputs[0] != inputs[0]
        elif expectation == "batch_explicit":
            passed = len(outputs) == len(inputs) and all(
                label in output for label, output in zip(("{1}", "{2}", "{3}"), outputs)
            )
        checks[expectation] = passed
        if not passed:
            errors.append(f"behavior_check_failed:{expectation}")
    return CheckResult(result.name, result.status, result.input, result.output, result.response_text, checks, errors)


def cases() -> list[Case]:
    """Return the native API contract cases for the current model fixture."""
    return [
        {"name": "languages", "method": "GET", "path": "/languages", "status": 200},
        {"name": "empty", "body": {"text": "", "from": "auto", "to": "zh"}, "status": 200, "checks": ["passthrough"]},
        {"name": "whitespace", "body": {"text": "  \n\t ", "from": "auto", "to": "zh"}, "status": 200, "checks": ["whitespace"]},
        {"name": "mixed_auto_batch", "body": {"text": ["This is a clear English sentence.", "这是中文句子。", ""], "from": "auto", "to": "zh"}, "status": 200, "checks": ["batch_mixed"]},
        {"name": "unsupported_pair", "body": {"text": "これは日本語の文章です。", "from": "auto", "to": "zh"}, "status": 400, "error": "ja"},
        {"name": "traditional_chinese_rejected", "body": {"text": "Hello world", "from": "en", "to": "zh-TW"}, "status": 400, "error": "simplified chinese"},
        {"name": "numbers_flags_emoji", "body": {"text": "Version {12} is ready in [345] regions. 🇺🇸 🇯🇵 😀", "from": "en", "to": "zh"}, "status": 200, "preserve": ["{12}", "[345]", "🇺🇸", "🇯🇵", "😀"], "checks": ["structural"]},
        {"name": "fenced_code", "body": {"text": "Please review this change.\n\n```rust\nlet value = 42;\n```\n\nThank you.", "from": "en", "to": "zh"}, "status": 200, "checks": ["structural", "fenced_code"]},
        {"name": "html_exclusions", "body": {"text": "<p title=\"hello\">Read this.</p><script>keepScript()</script><style>.x { color: red }</style><pre>keep pre</pre><code>keep code &lt;span&gt;</code><span translate=\"no\">keep translate</span><div class=\"notranslate\">keep class</div>", "from": "en", "to": "zh", "html": True}, "status": 200, "preserve": ["title=\"hello\"", "keepScript()", ".x { color: red }", "keep pre", "keep code &lt;span&gt;", "keep translate", "keep class"], "checks": ["html"]},
        {"name": "explicit_batch", "body": {"text": ["First {1}.", "Second {2}.", "Third {3}."], "from": "en", "to": "zh"}, "status": 200, "preserve": ["{1}", "{2}", "{3}"], "checks": ["batch_explicit"]},
        {"name": "too_many_items", "body": {"text": ["x"] * 129, "from": "en", "to": "zh"}, "status": 413},
        {"name": "too_many_bytes", "body": {"text": "x" * (1024 * 1024 + 1), "from": "en", "to": "zh"}, "status": 413},
    ]


def run_case(base_url: str, case: Case) -> CheckResult:
    """Send one real HTTP request and preserve its raw response."""
    method = case.get("method", "POST")
    body = case.get("body")
    data = None if body is None else json.dumps(body, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json"} if data is not None else {}
    request = Request(base_url.rstrip("/") + case.get("path", "/translate"), data=data, headers=headers, method=method)
    try:
        with urlopen(request, timeout=90) as response:
            status, raw = response.status, response.read().decode("utf-8", errors="replace")
    except HTTPError as error:
        status, raw = error.code, error.read().decode("utf-8", errors="replace")
    except (URLError, TimeoutError, ConnectionError) as error:
        return CheckResult(case["name"], None, body, None, "", {}, [f"transport_error:{error}"])
    return evaluate_behavior(case, check_response(case, status, raw))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    rows = [run_case(args.base_url, case) for case in cases()]
    report = {"base_url": args.base_url, "passed": all(not row.errors for row in rows), "cases": [asdict(row) for row in rows]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "failed": [row.name for row in rows if row.errors]}, ensure_ascii=True))
    return 0 if report["passed"] else 1


def self_check() -> None:
    """Check the response evaluator without opening a network connection."""
    explicit = {"name": "ok", "body": {"text": "hi", "from": "en", "to": "zh"}, "status": 200}
    good = check_response(explicit, 200, '{"text":"你好","from":"en","to":"zh"}')
    assert not evaluate_behavior({**explicit, "checks": ["translated"]}, good).errors
    for raw in ('{"text":"hi","from":"en","to":"zh"}', '{"text":"你好","from":"mul","to":"zh"}'):
        result = evaluate_behavior({**explicit, "checks": ["translated"]}, check_response(explicit, 200, raw))
        assert result.errors

    mixed = {"name": "mixed", "body": {"text": ["English", "中文", ""], "from": "auto", "to": "zh"}, "status": 200}
    mixed_good = check_response(mixed, 200, '{"text":["中文", "中文", ""],"from":"mul","to":"zh"}')
    assert not mixed_good.errors
    mixed_bad = check_response(mixed, 200, '{"text":["中文", "中文", ""],"from":"en","to":"zh"}')
    assert mixed_bad.errors
    mixed_reordered = evaluate_behavior({**mixed, "checks": ["batch_mixed"]}, check_response(mixed, 200, '{"text":["中文", "", "中文"],"from":"mul","to":"zh"}'))
    assert mixed_reordered.errors

    explicit_batch = cases()[-3]
    batch_result = check_response(explicit_batch, 200, '{"text":["一 {1}","二 {2}","三 {3}"],"from":"en","to":"zh"}')
    assert not evaluate_behavior(explicit_batch, batch_result).errors
    missing_number = evaluate_behavior(explicit_batch, check_response(explicit_batch, 200, '{"text":["一 {1}","二","三 {3}"],"from":"en","to":"zh"}'))
    assert missing_number.errors

    blank = {"name": "blank", "body": {"text": "", "from": "auto", "to": "zh"}, "status": 200}
    assert not check_response(blank, 200, '{"text":"","from":"und","to":"zh"}').errors
    empty_batch = {"name": "empty_batch", "body": {"text": [], "from": "en", "to": "zh"}, "status": 200, "checks": ["batch_explicit"]}
    assert evaluate_behavior(empty_batch, check_response(empty_batch, 200, '{"text":[],"from":"en","to":"zh"}')).errors

    fenced = cases()[-5]
    changed_fence = evaluate_behavior(fenced, check_response(fenced, 200, '{"text":"请审阅。\\n\\n```rust\\nlet value = 43;\\n```\\n\\n谢谢。","from":"en","to":"zh"}'))
    assert changed_fence.errors
    html = cases()[-4]
    html_result = evaluate_behavior(html, check_response(html, 200, '{"text":"<p title=\\"hello\\">Read this.</p><script>keepScript()</script><style>.x { color: red }</style><pre>keep pre</pre><code>keep code &lt;span&gt;</code><span translate=\\"no\\">keep translate</span><div class=\\"notranslate\\">keep class</div>","from":"en","to":"zh"}'))
    assert not html_result.errors

    unknown = evaluate_behavior({**explicit, "checks": ["unknown"]}, good)
    assert unknown.errors
    bad = check_response({"name": "bad", "body": {"text": "x", "from": "en", "to": "zh"}, "status": 400, "error": "ja"}, 400, '{"error":"Translation from \'fr\' to \'zh\' is not supported"}')
    assert bad.errors == ["expected_error_detail_missing"]
    print("contract evaluator self-check passed")


if __name__ == "__main__":
    import sys

    if sys.argv[1:] == ["--self-check"]:
        self_check()
    else:
        raise SystemExit(main())
