use crate::{AppError, AppState, pipeline::Document};
use isolang::Language;
use std::{collections::BTreeMap, sync::Arc};
use whichlang::Lang;

pub fn parse_language_code(code: &str) -> Result<Language, AppError> {
    let lowered = code.to_ascii_lowercase();
    if lowered.starts_with("zh-")
        && !matches!(
            lowered.as_str(),
            "zh-cn" | "zh-sg" | "zh-hans" | "zh-hans-cn" | "zh-hans-sg"
        )
    {
        return Err(AppError::TranslationError(
            "Only simplified Chinese is supported by the loaded zh models".into(),
        ));
    }
    let normalized = lowered.split('-').next().unwrap_or(&lowered);
    Language::from_639_1(normalized).ok_or_else(|| {
        AppError::TranslationError("Invalid language code. Please use ISO 639-1 format.".into())
    })
}

fn parse_source_language_code(code: &str) -> Result<Language, AppError> {
    let normalized = code.to_ascii_lowercase();
    Language::from_639_1(normalized.split('-').next().unwrap_or(&normalized)).ok_or_else(|| {
        AppError::TranslationError(
            "Invalid source language code. Please use ISO 639-1 format.".into(),
        )
    })
}

pub fn language_code(language: &Language) -> &'static str {
    language.to_639_1().expect("language has no ISO 639-1 code")
}

pub fn detect_language_code(text: &str) -> Result<&'static str, AppError> {
    Ok(language_code(&detect_language(text)?))
}

pub fn normalize_language_code(code: &str) -> Result<&'static str, AppError> {
    Ok(language_code(&parse_language_code(code)?))
}

fn from_whichlang(language: Lang) -> Language {
    match language {
        Lang::Ara => Language::Ara,
        // whichlang uses "cmn" for Chinese, but only Zho has the ISO 639-1 code "zh"
        Lang::Cmn => Language::Zho,
        Lang::Deu => Language::Deu,
        Lang::Eng => Language::Eng,
        Lang::Fra => Language::Fra,
        Lang::Hin => Language::Hin,
        Lang::Ita => Language::Ita,
        Lang::Jpn => Language::Jpn,
        Lang::Kor => Language::Kor,
        Lang::Nld => Language::Nld,
        Lang::Por => Language::Por,
        Lang::Rus => Language::Rus,
        Lang::Spa => Language::Spa,
        Lang::Swe => Language::Swe,
        Lang::Tur => Language::Tur,
        Lang::Vie => Language::Vie,
    }
}

#[expect(clippy::unnecessary_wraps, reason = "detector may become fallible")]
fn detect_language(text: &str) -> Result<Language, AppError> {
    Ok(from_whichlang(whichlang::detect_language(text)))
}

fn resolve_source_language(text: &str, from_lang: Option<&str>) -> Result<Language, AppError> {
    let explicit = from_lang.filter(|code| !code.is_empty() && !code.eq_ignore_ascii_case("auto"));

    match explicit {
        Some(code) => parse_source_language_code(code),
        None => detect_language(text),
    }
}

fn detected_document_language(
    document: &Document,
    from_lang: Option<&str>,
) -> Result<Option<Language>, AppError> {
    let detected_text = document.segments().map(String::as_str).collect::<String>();
    if detected_text.trim().is_empty() {
        return Ok(None);
    }
    resolve_source_language(&detected_text, from_lang).map(Some)
}

pub async fn perform_translation(
    state: &Arc<AppState>,
    text: &str,
    from_lang: Option<String>,
    to_lang: &str,
) -> Result<(String, String, String), AppError> {
    let mut results =
        perform_batch_translation(state, vec![text.into()], from_lang, to_lang).await?;
    let (text, from) = results
        .pop()
        .ok_or_else(|| AppError::InferenceError("Missing single translation".into()))?;
    Ok((text, from, normalize_language_code(to_lang)?.into()))
}

pub async fn perform_batch_translation(
    state: &Arc<AppState>,
    texts: Vec<String>,
    from_lang: Option<String>,
    to_lang: &str,
) -> Result<Vec<(String, String)>, AppError> {
    perform_formatted_batch(
        state,
        texts,
        TranslationOptions {
            from: from_lang.as_deref(),
            to: to_lang,
            html: false,
        },
    )
    .await
}

pub struct TranslationOptions<'a> {
    pub from: Option<&'a str>,
    pub to: &'a str,
    pub html: bool,
}

pub async fn perform_formatted_batch(
    state: &Arc<AppState>,
    texts: Vec<String>,
    options: TranslationOptions<'_>,
) -> Result<Vec<(String, String)>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    if texts.len() > 128 || texts.iter().map(String::len).sum::<usize>() > 1024 * 1024 {
        return Err(AppError::InputTooLarge(
            "Maximum 128 texts and 1 MiB per request".into(),
        ));
    }
    let target_lang = parse_language_code(options.to)?;
    let documents = texts
        .iter()
        .map(|text| Document::new(text, options.html))
        .collect::<Result<Vec<_>, _>>()?;
    let explicit_source = options
        .from
        .filter(|source| !source.is_empty() && !source.eq_ignore_ascii_case("auto"));
    let detected_sources = documents
        .iter()
        .map(|document| detected_document_language(document, options.from))
        .collect::<Result<Vec<_>, _>>()?;
    let explicit_source_code = explicit_source
        .map(parse_source_language_code)
        .transpose()?
        .map(|language| language_code(&language).to_string());
    let source_code = |source: Option<Language>| {
        source.map_or_else(
            || {
                explicit_source_code
                    .clone()
                    .unwrap_or_else(|| "und".to_string())
            },
            |language| language_code(&language).to_string(),
        )
    };
    let mut translations = vec![Vec::new(); documents.len()];
    let mut language_groups = BTreeMap::<Language, Vec<(usize, String)>>::new();
    let mut unchanged_documents = vec![false; documents.len()];
    for (index, document) in documents.iter().enumerate() {
        let Some(source_lang) = detected_sources[index] else {
            unchanged_documents[index] = true;
            continue;
        };
        if source_lang == target_lang {
            unchanged_documents[index] = true;
        } else {
            language_groups
                .entry(source_lang)
                .or_default()
                .extend(document.segments().cloned().map(|segment| (index, segment)));
        }
    }

    let supported_pairs = state.inference.language_pairs();
    for &source_lang in language_groups.keys() {
        if !supported_pairs
            .get(&source_lang)
            .is_some_and(|targets| targets.contains(&target_lang))
        {
            return Err(AppError::TranslationError(format!(
                "Translation from '{}' to '{}' is not supported",
                language_code(&source_lang),
                language_code(&target_lang)
            )));
        }
    }
    for (&source_lang, group) in &language_groups {
        let mut start = 0;
        while start < group.len() {
            let mut end = start;
            let mut chars = 0;
            while end < group.len() && end - start < 16 {
                let next_chars = group[end].1.chars().count();
                if end > start && chars + next_chars > 1600 {
                    break;
                }
                chars += next_chars;
                end += 1;
            }
            let batch = group[start..end]
                .iter()
                .map(|(_, text)| text.clone())
                .collect::<Vec<_>>();
            let batch_results = state
                .inference
                .translate_batch(source_lang, target_lang, batch)
                .await?;
            for ((document_index, _), translation) in group[start..end].iter().zip(batch_results) {
                translations[*document_index].push(translation);
            }
            start = end;
        }
    }

    documents
        .iter()
        .enumerate()
        .map(|(index, document)| {
            let result = if unchanged_documents[index] {
                texts[index].clone()
            } else {
                document.render(&translations[index])?
            };
            Ok((result, source_code(detected_sources[index])))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_and_detected_languages_are_honored() {
        let japanese = "これは日本語の文章です。英語として翻訳しないでください。";
        assert_eq!(
            resolve_source_language(japanese, None).unwrap(),
            Language::Jpn
        );
        assert_eq!(
            resolve_source_language(japanese, Some("en")).unwrap(),
            Language::Eng
        );
        assert_eq!(parse_language_code("zh-Hans").unwrap(), Language::Zho);
        assert!(parse_language_code("zh-TW").is_err());
        assert_eq!(parse_source_language_code("zh-TW").unwrap(), Language::Zho);
    }

    #[test]
    fn detects_each_document_from_translatable_content_only() {
        let documents = [
            Document::new("<b>Hello world.</b>", true).unwrap(),
            Document::new("これは日本語です。", false).unwrap(),
            Document::new("<code>{12}</code>", true).unwrap(),
        ];

        assert_eq!(
            detected_document_language(&documents[0], None).unwrap(),
            Some(Language::Eng)
        );
        assert_eq!(
            detected_document_language(&documents[1], None).unwrap(),
            Some(Language::Jpn)
        );
        assert_eq!(
            detected_document_language(&documents[2], None).unwrap(),
            None
        );
    }
}
