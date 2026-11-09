#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass, field
import json
import os
from pathlib import Path
import re
from typing import NotRequired, TypedDict
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


JSONValue = str | int | float | bool | None | list["JSONValue"] | dict[str, "JSONValue"]


class Case(TypedDict):
    id: str
    text: str | list[str]
    from_: str
    to: str
    html: NotRequired[bool]
    required_numbers: NotRequired[list[str]]
    per_item_numbers: NotRequired[list[list[str]]]


@dataclass(frozen=True, slots=True)
class Result:
    id: str
    status: int | None
    input: str | list[str]
    output: str | list[str] | None
    raw_response: str
    checks: dict[str, bool] = field(default_factory=dict)
    failures: list[str] = field(default_factory=list)


def has_number(text: str, literal: str) -> bool:
    """Match a complete numeric literal, so 3600 cannot match inside 36000."""
    pattern = rf"(?<![0-9])(?<![0-9][.,]){re.escape(literal)}(?![0-9]|[.,][0-9])"
    return re.search(pattern, text) is not None


def numeric_failures(case: Case, outputs: list[str]) -> tuple[dict[str, bool], list[str]]:
    checks: dict[str, bool] = {}
    failures: list[str] = []
    required = case.get("required_numbers", [])
    joined = "\n".join(outputs)
    for literal in required:
        passed = has_number(joined, literal)
        checks[f"number:{literal}"] = passed
        if not passed:
            failures.append(f"missing_number:{literal}")
    per_item = case.get("per_item_numbers", [])
    if per_item:
        checks["batch_order"] = len(outputs) == len(per_item)
        if not checks["batch_order"]:
            failures.append("batch_output_count_mismatch")
        else:
            for index, (output, literals) in enumerate(zip(outputs, per_item, strict=True)):
                for literal in literals:
                    passed = has_number(output, literal)
                    checks[f"item:{index}:number:{literal}"] = passed
                    if not passed:
                        failures.append(f"missing_item_number:{index}:{literal}")
    return checks, failures


def response_text(engine: str, body: JSONValue, batch: bool) -> str | list[str] | None:
    if not isinstance(body, dict):
        return None
    value = body.get("text" if engine == "lingua" else "results" if batch else "result")
    if isinstance(value, str) and not batch:
        return value
    if batch and isinstance(value, list) and all(isinstance(item, str) for item in value):
        return [item for item in value if isinstance(item, str)]
    return None


def run_case(case: Case, engine: str, base_url: str, headers: dict[str, str]) -> Result:
    text = case["text"]
    batch = isinstance(text, list)
    body: dict[str, JSONValue] = {"texts" if engine == "mtran" and batch else "text": text,
                                  "from": case["from_"], "to": case["to"]}
    if case.get("html"):
        body["html"] = True
    path = "/translate/batch" if engine == "mtran" and batch else "/translate"
    request = Request(
        base_url.rstrip("/") + path,
        data=json.dumps(body, ensure_ascii=False).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    try:
        with urlopen(request, timeout=90) as response:
            status = response.status
            raw = response.read().decode("utf-8", errors="replace")
    except HTTPError as error:
        raw = error.read().decode("utf-8", errors="replace")
        return Result(case["id"], error.code, text, None, raw, {"http_status": False}, [f"http_status:{error.code}"])
    except (URLError, TimeoutError, ConnectionError) as error:
        return Result(case["id"], None, text, None, "", {"http_status": False}, [f"transport_error:{error}"])

    checks: dict[str, bool] = {"http_status": status == 200}
    failures = [] if checks["http_status"] else [f"http_status:{status}"]
    output: str | list[str] | None = None
    try:
        decoded: JSONValue = json.loads(raw)
    except json.JSONDecodeError as error:
        failures.append(f"invalid_json:{error.msg}")
    else:
        output = response_text(engine, decoded, batch)
        checks["response_shape"] = output is not None
        if output is None:
            failures.append("invalid_response_shape")
        elif isinstance(output, list) and len(output) != len(text):
            checks["batch_order"] = False
            failures.append("batch_output_count_mismatch")
        elif status == 200:
            outputs = output if isinstance(output, list) else [output]
            number_checks, number_failures = numeric_failures(case, outputs)
            checks.update(number_checks)
            failures.extend(number_failures)
    return Result(case["id"], status, text, output, raw, checks, failures)


def load_cases(path: Path) -> list[Case]:
    document: JSONValue = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict) or not isinstance(document.get("cases"), list):
        raise ValueError("corpus must contain a cases array")
    raw_cases = document["cases"]
    if not isinstance(raw_cases, list) or not raw_cases:
        raise ValueError("corpus cases must be nonempty")
    cases: list[Case] = []
    for raw in raw_cases:
        if not isinstance(raw, dict):
            raise ValueError("case must be an object")
        name, source, target, text = raw.get("id"), raw.get("from"), raw.get("to"), raw.get("text")
        if not isinstance(name, str) or not isinstance(source, str) or not isinstance(target, str):
            raise ValueError("case id, from and to must be strings")
        if isinstance(text, str):
            input_text: str | list[str] = text
        elif isinstance(text, list) and text and all(isinstance(item, str) for item in text):
            input_text = [item for item in text if isinstance(item, str)]
        else:
            raise ValueError(f"{name}: invalid input text")
        case: Case = {"id": name, "from_": source, "to": target, "text": input_text}
        html = raw.get("html", False)
        if not isinstance(html, bool):
            raise ValueError(f"{name}: html must be boolean")
        case["html"] = html
        required = raw.get("required_numbers", [])
        if not isinstance(required, list) or not all(isinstance(item, str) and item for item in required):
            raise ValueError(f"{name}: required_numbers must contain nonempty strings")
        case["required_numbers"] = [item for item in required if isinstance(item, str)]
        per_item = raw.get("per_item_numbers", [])
        if not isinstance(per_item, list):
            raise ValueError(f"{name}: per_item_numbers must be an array")
        item_numbers: list[list[str]] = []
        for item in per_item:
            if not isinstance(item, list) or not all(isinstance(number, str) and number for number in item):
                raise ValueError(f"{name}: invalid per-item numbers")
            item_numbers.append([number for number in item if isinstance(number, str)])
        case["per_item_numbers"] = item_numbers
        if not required and not item_numbers:
            raise ValueError(f"{name}: no numeric assertion")
        cases.append(case)
    return cases


def self_check() -> None:
    case: Case = {"id": "check", "text": "value 3600", "from_": "en", "to": "zh", "required_numbers": ["3600"]}
    checks, failures = numeric_failures(case, ["值为 3600"])
    assert checks["number:3600"] and not failures
    checks, failures = numeric_failures(case, ["值为 36000"])
    assert not checks["number:3600"] and failures == ["missing_number:3600"]
    batch: Case = {"id": "batch", "text": ["a 101", "b 202"], "from_": "en", "to": "zh", "required_numbers": ["101", "202"], "per_item_numbers": [["101"], ["202"]]}
    checks, failures = numeric_failures(batch, ["甲 101", "乙 202"])
    assert checks["batch_order"] and not failures
    _, failures = numeric_failures(batch, ["甲 202", "乙 101"])
    assert failures == ["missing_item_number:0:101", "missing_item_number:1:202"]
    assert not has_number("0.3600", "3600")
    assert not has_number("3600.5", "3600")
    assert has_number("3600.", "3600")
    print("numeric regression self-check passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", choices=("lingua", "mtran"))
    parser.add_argument("--base-url")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--corpus", type=Path, default=Path(__file__).with_name("numeric_corpus.json"))
    parser.add_argument("--token-env")
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        self_check()
        return 0
    if args.engine is None or args.base_url is None or args.output is None:
        parser.error("--engine, --base-url and --output are required unless --self-check is used")
    headers = {"Content-Type": "application/json"}
    if args.token_env:
        token = os.environ.get(args.token_env)
        if token is None:
            raise SystemExit(f"missing token environment variable: {args.token_env}")
        headers["Authorization"] = "Bearer " + token
    rows = [run_case(case, args.engine, args.base_url, headers) for case in load_cases(args.corpus)]
    report = {
        "engine": args.engine,
        "base_url": args.base_url,
        "scope": "numeric-literal regression only; semantic translation quality is not asserted",
        "passed": all(not row.failures for row in rows),
        "cases": [asdict(row) for row in rows],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "failed": [row.id for row in rows if row.failures]}, ensure_ascii=True))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
