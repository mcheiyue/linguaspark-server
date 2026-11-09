use regex::Regex;
use std::sync::LazyLock;

static EMOJI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\A(?:\p{RI}\p{RI}|\p{Extended_Pictographic}(?:\u{FE0F}|\p{Emoji_Modifier})?(?:\u{200D}\p{Extended_Pictographic}(?:\u{FE0F}|\p{Emoji_Modifier})?)*)",
    )
    .expect("emoji protection regex is a compile-time constant")
});

pub fn end(text: &str) -> Option<usize> {
    if let Some(rest) = text.strip_prefix('{') {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest.as_bytes().get(digits) == Some(&b'}') {
            return Some(digits + 2);
        }
    }
    if let Some(rest) = text.strip_prefix("[p") {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest.as_bytes().get(digits) == Some(&b']') {
            return Some(digits + 3);
        }
    }
    if let Some(rest) = text.strip_prefix('[') {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest.as_bytes().get(digits) == Some(&b']') {
            return Some(digits + 2);
        }
    }
    if text.starts_with("```") || text.starts_with("~~~") {
        let marker = text.as_bytes()[0];
        let count = text.bytes().take_while(|&byte| byte == marker).count();
        let delimiter = &text[..count];
        return Some(
            text[count..]
                .find(delimiter)
                .map_or(text.len(), |at| count + at + count),
        );
    }
    EMOJI.find(text).map(|matched| matched.end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_mtran_boundaries_and_rejects_unrelated_literals() {
        for literal in [
            "{12}",
            "[345]",
            "[p0]",
            "[p123]",
            "```py\nx=1\n```",
            "🇺🇸",
            "©",
            "®",
            "👩🏽‍💻",
        ]
        {
            assert_eq!(
                end(&format!("{literal} remaining")),
                Some(literal.len()),
                "{literal}"
            );
        }
        for text in [
            "normal",
            "{name}",
            "{12x}",
            "[12x]",
            "[P1]",
            "[p]",
            "[p1x]",
            "3.14",
            "`npm install`",
            "$19.99",
            "https://example.com",
            "←",
        ] {
            assert_eq!(end(text), None, "{text}");
        }
    }

    #[test]
    fn keeps_unclosed_fence_verbatim() {
        assert_eq!(end("```unfinished"), Some("```unfinished".len()));
    }
}
