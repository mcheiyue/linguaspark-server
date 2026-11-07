# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import statistics
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

from quality import structural_errors


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", choices=("lingua", "mtran"), required=True)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, default=Path(__file__).with_name("corpus.json"))
    parser.add_argument("--token-env")
    parser.add_argument("--html", action="store_true")
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--concurrency", type=int, default=2)
    args = parser.parse_args()
    cases = json.loads(args.corpus.read_text(encoding="utf-8"))["cases"]
    headers = {"Content-Type": "application/json"}
    if args.token_env:
        headers["Authorization"] = "Bearer " + os.environ[args.token_env]

    def execute(case):
        mtran_batch = args.engine == "mtran" and isinstance(case["text"], list)
        body = {"texts" if mtran_batch else "text": case["text"], "from": case["from"], "to": case["to"]}
        if case.get("html") and not args.html:
            return {"id": case["id"], "status": "unsupported_html"}
        if case.get("html"):
            body["html"] = True
        started = time.perf_counter()
        try:
            path = "/translate/batch" if mtran_batch else "/translate"
            with urlopen(Request(args.base_url.rstrip("/") + path, data=json.dumps(body).encode(), headers=headers), timeout=90) as response:
                status = response.status
                result = json.load(response)
        except HTTPError as error:
            return {"id": case["id"], "status": error.code, "error": error.read().decode("utf-8", errors="replace"), "elapsed_ms": round((time.perf_counter() - started) * 1000, 3)}
        except (URLError, TimeoutError, ConnectionError) as error:
            return {"id": case["id"], "status": "transport_error", "error": str(error), "elapsed_ms": round((time.perf_counter() - started) * 1000, 3)}
        elapsed = (time.perf_counter() - started) * 1000
        translated = result.get("text" if args.engine == "lingua" else "results" if mtran_batch else "result")
        originals = case["text"] if isinstance(case["text"], list) else [case["text"]]
        outputs = translated if isinstance(translated, list) else [translated]
        errors = []
        if len(outputs) != len(originals) or not all(isinstance(text, str) for text in outputs):
            errors.append("invalid_output_shape")
        else:
            for original, output in zip(originals, outputs, strict=True):
                errors.extend(structural_errors(original, output, case.get("preserve", []), case.get("html", False)))
                if original.strip() and case["from"] != case["to"] and original == output and "exact" not in case:
                    errors.append("untranslated_input")
            if "exact" in case and translated != case["exact"]:
                errors.append("passthrough_changed")
        return {"id": case["id"], "status": status, "elapsed_ms": round(elapsed, 3), "input": case["text"], "output": translated, "structural_errors": errors, "semantic_review": "manual_required"}

    rows = [execute(case) for case in cases if case.get("group") != "burst"]
    short = [case for case in cases if case["id"] in ("ui", "web", "attention", "technical", "reference")]
    hot = [execute(case) for _ in range(args.repetitions) for case in short]
    burst_cases = [case for case in cases if case.get("group") == "burst"]
    started = time.perf_counter()
    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        burst = list(pool.map(execute, burst_cases))
    burst_ms = (time.perf_counter() - started) * 1000
    times = sorted(row["elapsed_ms"] for row in hot if row["status"] == 200)
    report = {"label": args.label, "engine": args.engine, "created_at": datetime.now(timezone.utc).isoformat(),
              "rows": rows, "hot": hot, "burst": burst, "concurrency": args.concurrency,
              "hot_p50_ms": statistics.median(times) if times else None,
              "hot_p95_ms": times[min(len(times) - 1, int(len(times) * 0.95))] if times else None,
              "burst_wall_ms": round(burst_ms, 3), "cold_start": "not_measured", "semantic_verdict": "not_automated"}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"label": args.label, "hot_p50_ms": report["hot_p50_ms"], "burst_wall_ms": report["burst_wall_ms"],
                      "structural_failed": [row["id"] for row in rows if row.get("structural_errors")],
                      "http_failed": [row["id"] for row in rows if row["status"] != 200]}, ensure_ascii=True))


if __name__ == "__main__":
    main()
