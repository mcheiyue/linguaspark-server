# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from html.parser import HTMLParser
import re


class Structure(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.events: list[tuple[str, str]] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.events.append(("open", repr((tag, sorted(attrs)))))

    def handle_endtag(self, tag: str) -> None:
        self.events.append(("close", tag))

    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.events.append(("void", repr((tag, sorted(attrs)))))

    def handle_comment(self, data: str) -> None:
        self.events.append(("comment", data))


def structural_errors(source: str, output: str, preserve: list[str], html: bool) -> list[str]:
    errors = []
    if source.strip() and not output.strip():
        errors.append("empty_output")
    for literal in preserve:
        if source.count(literal) != output.count(literal):
            errors.append("literal_changed:" + literal)
    if html:
        before, after = Structure(), Structure()
        before.feed(source)
        after.feed(output)
        if before.events != after.events:
            errors.append("html_structure_changed")
    else:
        if re.findall(r"\r\n|\r|\n", source) != re.findall(r"\r\n|\r|\n", output):
            errors.append("line_breaks_changed")
        for pattern in (r"^\s*", r"\s*$"):
            if re.search(pattern, source)[0] != re.search(pattern, output)[0]:
                errors.append("outer_whitespace_changed")
    return errors


def self_check() -> None:
    original = "<p id='x'>Open {{name}}</p><!--end-->"
    good = "<p id='x'>打开 {{name}}</p><!--end-->"
    assert structural_errors(original, good, ["{{name}}"], True) == []
    assert structural_errors(original, good.replace("{{name}}", "名字"), ["{{name}}"], True)
    assert structural_errors(original, good.replace("id='x'", "id='y'"), [], True)
    assert structural_errors("  hello\r\n\r\nworld ", "  你好\r\n\r\n世界 ", [], False) == []
    assert structural_errors("hello\nworld", "你好世界", [], False)
    print("quality self-check passed")


if __name__ == "__main__":
    self_check()
