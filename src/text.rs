use crate::{AppError, protected};
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_SEGMENT_CHARS: usize = 400;

enum Part {
    Literal(String),
    Translation(usize),
}

pub struct TextPlan {
    parts: Vec<Part>,
    segments: Vec<String>,
}

impl TextPlan {
    pub fn new(input: &str) -> Self {
        let mut plan = Self {
            parts: Vec::new(),
            segments: Vec::new(),
        };
        if is_standalone_version(input) {
            plan.literal(input);
        } else {
            plan.prose(input);
        }
        plan
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    pub fn render(&self, translations: &[String]) -> Result<String, AppError> {
        if translations.len() != self.segments.len()
            || translations.iter().any(|s| s.trim().is_empty())
        {
            return Err(AppError::InferenceError(
                "Missing or extra translated segments".into(),
            ));
        }
        let mut output = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => output.push_str(text),
                Part::Translation(index) => output.push_str(translations[*index].trim()),
            }
        }
        Ok(output)
    }

    fn literal(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(Part::Literal(previous)) = self.parts.last_mut() {
            previous.push_str(text);
        } else {
            self.parts.push(Part::Literal(text.into()));
        }
    }

    fn piece(&mut self, text: &str) {
        let content = text.trim();
        if !content.chars().any(char::is_alphabetic) {
            self.literal(text);
            return;
        }
        let leading = text.len() - text.trim_start().len();
        self.literal(&text[..leading]);
        self.parts.push(Part::Translation(self.segments.len()));
        self.segments.push(content.into());
        self.literal(&text[leading + content.len()..]);
    }

    fn prose(&mut self, text: &str) {
        let mut at = 0;
        while at < text.len() {
            if let Some(length) = protected::end(&text[at..]) {
                self.literal(&text[at..at + length]);
                at += length;
                continue;
            }
            let first_len = text[at..].chars().next().map_or(0, char::len_utf8);
            let next_protected = text[at + first_len..]
                .char_indices()
                .find_map(|(offset, _)| {
                    let start = at + first_len + offset;
                    protected::end(&text[start..]).map(|_| start)
                });
            let end = next_protected.unwrap_or(text.len());
            self.prose_run(&text[at..end]);
            at = end;
        }
    }

    fn prose_run(&mut self, text: &str) {
        let mut line_start = 0;
        for (at, character) in text.char_indices() {
            if matches!(character, '\r' | '\n') {
                self.line(&text[line_start..at]);
                let end = at + character.len_utf8();
                self.literal(&text[at..end]);
                line_start = end;
            }
        }
        self.line(&text[line_start..]);
    }

    fn line(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut sentence = String::new();
        for next in text.split_sentence_bounds() {
            sentence.push_str(next);
            if abbreviation(&sentence) {
                continue;
            }
            self.bounded(&sentence);
            sentence.clear();
        }
        self.bounded(&sentence);
    }

    fn bounded(&mut self, mut text: &str) {
        let mut chars = text.char_indices();
        while let Some((limit, _)) = chars.nth(MAX_SEGMENT_CHARS) {
            let split = text
                .split_word_bounds()
                .scan(0, |offset, word| {
                    *offset += word.len();
                    Some(*offset)
                })
                .take_while(|&offset| offset <= limit)
                .last()
                .filter(|&offset| offset > 0)
                .unwrap_or(limit);
            self.piece(&text[..split]);
            text = &text[split..];
            chars = text.char_indices();
        }
        self.piece(text);
    }
}

fn is_standalone_version(text: &str) -> bool {
    let Some(version) = text.trim().strip_prefix('v') else {
        return false;
    };
    !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn abbreviation(text: &str) -> bool {
    let last = text.split_whitespace().last().unwrap_or("");
    matches!(
        last,
        "Dr." | "Mr." | "Mrs." | "Ms." | "Prof." | "e.g." | "i.e." | "vs."
    ) || (last.len() == 2
        && last.ends_with('.')
        && last.starts_with(|c: char| c.is_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_layout_and_supported_literals_around_translations() {
        let source = "  First {12} sentence.\r\n\r\nSecond [345] sentence 🇺🇸 👩‍💻.  ";
        let plan = TextPlan::new(source);
        assert!(plan.segments.len() >= 2);
        let output = plan.render(plan.segments()).unwrap();
        assert_eq!(output, source);
    }

    #[test]
    fn splits_sentences_with_short_single_sentence_fast_path() {
        let plan = TextPlan::new("Hello world. Save your changes.");
        assert_eq!(plan.segments, ["Hello world.", "Save your changes."]);
        assert_eq!(TextPlan::new("One short sentence.").segments.len(), 1);
    }

    #[test]
    fn translates_each_sentence_in_a_long_paragraph_and_reconstructs_order() {
        let sentences = (0..20)
            .map(|index| format!("Sentence {index:02} has distinct meaning."))
            .collect::<Vec<_>>();
        let source = sentences.join(" ");
        let plan = TextPlan::new(&source);
        let translations = (0..sentences.len())
            .map(|index| format!("译文{index:02}。"))
            .collect::<Vec<_>>();

        assert_eq!(plan.segments, sentences);
        assert_eq!(plan.render(&translations).unwrap(), translations.join(" "));
    }

    #[test]
    fn keeps_words_whole_when_a_boundary_precedes_the_character_limit() {
        let prefix = format!("{}complete ", "a ".repeat(190));
        let word = format!("{}tail", "x".repeat(20));
        let source = format!("{prefix}{word}.");
        let plan = TextPlan::new(&source);

        assert!(source.chars().count() > MAX_SEGMENT_CHARS);
        assert_eq!(
            plan.segments,
            [prefix.trim_end().to_owned(), format!("{word}.")]
        );
        assert_eq!(plan.render(plan.segments()).unwrap(), source);
    }

    #[test]
    fn splits_long_unicode_text_without_losing_any_bytes() {
        for source in [
            "The request succeeded. ".repeat(100),
            "没有空格但不能丢失任何文字。".repeat(100),
            "abcdefghij".repeat(200),
        ] {
            let plan = TextPlan::new(&source);
            assert!(plan.segments.len() > 1);
            assert!(
                plan.segments
                    .iter()
                    .all(|text| text.chars().count() <= MAX_SEGMENT_CHARS)
            );
            assert_eq!(plan.render(plan.segments()).unwrap(), source);
        }
    }

    #[test]
    fn fenced_code_is_literal_but_inline_code_money_and_urls_are_model_text() {
        let source = "Translate `API_KEY` and $19.99 at https://example.com.\n```py\nprint(1)\n```";
        let plan = TextPlan::new(source);
        assert_eq!(plan.segments.len(), 1);
        assert!(plan.segments[0].contains("`API_KEY`"));
        assert!(plan.segments[0].contains("$19.99"));
        assert!(plan.segments[0].contains("https://example.com"));
        assert_eq!(plan.render(plan.segments()).unwrap(), source);
    }

    #[test]
    fn protects_empty_and_code_only_text_without_inference() {
        for source in [
            "",
            " \t\r\n",
            "{12} [p0] [p123] 👩‍💻",
            "[p0][p123]",
            "```python\nprint('hello')\n```",
        ] {
            let plan = TextPlan::new(source);
            assert!(plan.segments.is_empty());
            assert_eq!(plan.render(&[]).unwrap(), source);
        }
    }

    #[test]
    fn keeps_adjacent_citations_literal_and_translates_natural_sentences_independently() {
        let source = "First [p0][p1] sentence. Second [p2] sentence.";
        let plan = TextPlan::new(source);
        assert_eq!(
            plan.segments,
            ["First", "sentence.", "Second", "sentence."].map(str::to_owned)
        );
        let translations = ["第一", "句子。", "第二", "句子。"].map(str::to_owned);
        assert_eq!(
            plan.render(&translations).unwrap(),
            "第一 [p0][p1] 句子。 第二 [p2] 句子。"
        );
    }

    #[test]
    fn preserves_standalone_versions_with_original_whitespace() {
        for source in ["v1", "  v1.2.3  "] {
            let plan = TextPlan::new(source);
            assert!(plan.segments.is_empty());
            assert_eq!(plan.render(&[]).unwrap(), source);
        }
        for source in [
            "v1x",
            "v1.",
            "version v1",
            "https://example.com/v1",
            "/tmp/v1",
            "en",
            "Blog",
            "Audio",
            "DL",
            "Released on 2026-11-09.",
            "Alice",
        ] {
            assert!(!TextPlan::new(source).segments.is_empty(), "{source}");
        }
    }

    #[test]
    fn refuses_incomplete_or_extra_results() {
        let plan = TextPlan::new("Translate this.");
        for outputs in [
            vec![],
            vec![String::new()],
            vec!["译文".into(), "多余".into()],
        ] {
            assert!(plan.render(&outputs).is_err());
        }
    }
}
