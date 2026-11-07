# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
import json
from pathlib import Path
import sys


def main() -> None:
    source = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    cases = [{"id": row["id"], "text": row["text"], "from": "en", "to": "zh"}
             for row in source["samples"]]
    cases.extend([
        {"id": "abbreviations", "text": "Dr. Smith lives in the U.S. He paid $19.99, not $29.99. The value is 3.14, e.g. in the first example."},
        {"id": "layout", "text": "  The first request succeeded.\n\nThe second request failed.\r\n\r\nPlease do not retry it.  "},
        {"id": "placeholders", "text": "Hello {{user_name}}, use ${region} and %s to open https://example.com/a?q=42&x=1. The price is $19.99, not $29.99.", "preserve": ["{{user_name}}", "${region}", "%s", "https://example.com/a?q=42&x=1", "$19.99", "$29.99"]},
        {"id": "code", "text": "Run `npm install` before starting.\n```python\nprint('do not translate')\n```\nThe command must succeed.", "preserve": ["`npm install`", "```python\nprint('do not translate')\n```"]},
        {"id": "empty", "text": "", "exact": ""},
        {"id": "spaces", "text": " \t\r\n \n", "exact": " \t\r\n \n"},
        {"id": "protected_only", "text": "{{name}} $19.99 `API_KEY`", "exact": "{{name}} $19.99 `API_KEY`"},
        {"id": "unicode", "text": "The café opens at 09:30. Guests must not share passwords. Prices remain unchanged."},
        {"id": "long_sentence", "text": "When the service is unavailable " + "the administrator must inspect the logs and preserve all user data before restarting the server, " * 16 + "because deleting the data is not an acceptable recovery strategy."},
        {"id": "unbroken", "text": "abcdefghijklmnopqrstuvwxyz" * 40},
        {"id": "batch", "text": ["The first request succeeded.", "Do not delete the backup.", "The price has not changed."]},
        {"id": "html", "html": True, "text": "<!--keep--><p class='lead'>Please open <a href='/docs?id=42&amp;x=1'>the documentation</a> before paying $19.99.</p><pre>do not translate</pre><code>API_KEY</code><script>const keep='original';</script><style>.original{color:red}</style><span translate='no'>Keep Me</span><span class='notranslate'>Original Name</span>", "preserve": ["$19.99", "do not translate", "API_KEY", "const keep='original';", ".original{color:red}", "Keep Me", "Original Name"]},
        {"id": "html_entities", "html": True, "text": "Root text &amp; more.<p>Tom &amp; Jerry read &lt;three&gt; books.</p><p>Do not erase&nbsp;the backup.</p>"},
        {"id": "same_language", "text": "你好，世界。不要删除备份。", "from": "zh", "exact": "你好，世界。不要删除备份。"},
    ])
    burst = ["Save your changes before leaving the page.", "The backup completed successfully.",
             "This update does not delete your files.", "Check the connection and try again.",
             "Your account settings have been updated.", "The document is ready for download.",
             "The meeting starts in ten minutes.", "You can cancel the request at any time.",
             "Do not share this password with anyone.", "Please select the correct language.",
             "The server is temporarily unavailable.", "This feature is available to all users.",
             "You must restart the application to continue.", "The payment has not been processed.",
             "All changes are saved automatically.", "The results are sorted by creation date.",
             "This file cannot be opened in the browser.", "We will notify you when the task is complete.",
             "The original document remains unchanged.", "Use the search box to find a specific record."]
    cases.extend({"id": f"burst_{i + 1:02}", "text": text, "group": "burst"} for i, text in enumerate(burst))
    for case in cases:
        case.setdefault("from", "en")
        case.setdefault("to", "zh")
    destination = Path(__file__).with_name("corpus.json")
    if destination.exists():
        raise SystemExit("语料已固定，不覆盖已有文件")
    destination.write_text(json.dumps({"source_sha256": source["source_sha256"], "cases": cases}, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"{len(cases)} cases -> {destination}")


if __name__ == "__main__":
    main()
