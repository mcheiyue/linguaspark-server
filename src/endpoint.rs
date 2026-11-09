use crate::{
    AppError, AppState,
    translation::{
        TranslationOptions, detect_language_code, language_code, normalize_language_code,
        perform_batch_translation, perform_formatted_batch, perform_translation,
    },
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::SystemTime,
};

#[derive(Debug, Deserialize)]
pub struct DetectLanguageRequest {
    text: String,
}

#[derive(Debug, Serialize)]
pub struct DetectLanguageResponse {
    language: String,
}

pub async fn detect_language(
    Json(request): Json<DetectLanguageRequest>,
) -> Result<Json<DetectLanguageResponse>, AppError> {
    Ok(Json(DetectLanguageResponse {
        language: detect_language_code(&request.text).await?.to_owned(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TranslationInput {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum TranslationOutput {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct TranslationRequest {
    text: TranslationInput,
    from: Option<String>,
    to: String,
    #[serde(default)]
    html: bool,
}

#[derive(Debug, Serialize)]
pub struct TranslationResponse {
    text: TranslationOutput,
    from: String,
    to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sources: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct LanguagesResponse(BTreeMap<String, BTreeSet<String>>);

fn batch_source_language(sources: impl IntoIterator<Item = String>) -> String {
    let sources = sources
        .into_iter()
        .filter(|source| source != "und")
        .collect::<BTreeSet<_>>();
    if sources.len() == 1 {
        match sources.into_iter().next() {
            Some(source) => source,
            None => "und".to_owned(),
        }
    } else if sources.is_empty() {
        "und".to_owned()
    } else {
        "mul".to_owned()
    }
}

pub async fn translate(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TranslationRequest>,
) -> Result<Json<TranslationResponse>, AppError> {
    let (text, from_lang, to_lang, sources) = match request.text {
        TranslationInput::Single(text) => {
            let to = normalize_language_code(&request.to)?.to_string();
            let mut translated = perform_formatted_batch(
                &state,
                vec![text],
                TranslationOptions {
                    from: request.from.as_deref(),
                    to: &request.to,
                    html: request.html,
                },
            )
            .await?;
            let (text, from) = translated
                .pop()
                .ok_or_else(|| AppError::InferenceError("Missing single translation".into()))?;
            (TranslationOutput::Single(text), from, to, None)
        }
        TranslationInput::Batch(texts) => {
            if texts.is_empty() {
                return Err(AppError::TranslationError(
                    "Translation batch must contain at least one text".to_string(),
                ));
            }
            let to = normalize_language_code(&request.to)?.to_string();
            let translations = perform_formatted_batch(
                &state,
                texts,
                TranslationOptions {
                    from: request.from.as_deref(),
                    to: &request.to,
                    html: request.html,
                },
            )
            .await?;
            let (texts, sources): (Vec<_>, Vec<_>) = translations.into_iter().unzip();
            let from = batch_source_language(sources.iter().cloned());
            (TranslationOutput::Batch(texts), from, to, Some(sources))
        }
    };

    Ok(Json(TranslationResponse {
        text,
        from: from_lang,
        to: to_lang,
        sources,
    }))
}

#[derive(Debug, Deserialize)]
pub struct KissTranslationRequest {
    text: String,
    from: Option<String>,
    to: String,
}

#[derive(Debug, Serialize)]
pub struct KissTranslationResponse {
    text: String,
    from: String,
    to: String,
}

pub async fn translate_kiss(
    State(state): State<Arc<AppState>>,
    Json(request): Json<KissTranslationRequest>,
) -> Result<Json<KissTranslationResponse>, AppError> {
    let (text, from_lang, to_lang) =
        perform_translation(&state, &request.text, request.from, &request.to).await?;

    Ok(Json(KissTranslationResponse {
        text,
        from: from_lang,
        to: to_lang,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ImmersiveTranslationRequest {
    source_lang: Option<String>,
    target_lang: String,
    text_list: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImmersiveTranslationItem {
    detected_source_lang: String,
    text: String,
}

#[derive(Debug, Serialize)]
pub struct ImmersiveTranslationResponse {
    translations: Vec<ImmersiveTranslationItem>,
}

pub async fn translate_immersive(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ImmersiveTranslationRequest>,
) -> Result<Json<ImmersiveTranslationResponse>, AppError> {
    let translations = perform_batch_translation(
        &state,
        request.text_list,
        request.source_lang,
        &request.target_lang,
    )
    .await?
    .into_iter()
    .map(|(text, detected_source_lang)| ImmersiveTranslationItem {
        detected_source_lang,
        text,
    })
    .collect();

    Ok(Json(ImmersiveTranslationResponse { translations }))
}

#[derive(Debug, Deserialize)]
pub struct HcfyTranslationRequest {
    text: String,
    source: Option<String>,
    destination: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct HcfyTranslationResponse {
    text: String,
    from: String,
    to: String,
    result: Vec<String>,
}

pub async fn translate_hcfy(
    State(state): State<Arc<AppState>>,
    Json(request): Json<HcfyTranslationRequest>,
) -> Result<Json<HcfyTranslationResponse>, AppError> {
    const LANGUAGE_CODE_MAP: &[(&str, &str)] =
        &[("中文(简体)", "zh"), ("英语", "en"), ("日语", "ja")];

    fn convert_language_name(lang: &str) -> String {
        LANGUAGE_CODE_MAP
            .iter()
            .find(|&&(name, _)| name == lang)
            .map(|&(_, code)| code)
            .unwrap_or(lang)
            .to_string()
    }

    fn get_language_name(code: &str) -> String {
        LANGUAGE_CODE_MAP
            .iter()
            .find(|&&(_, mapped_code)| mapped_code == code)
            .map(|&(name, _)| name)
            .unwrap_or(code)
            .to_string()
    }

    let source_lang = request.source.as_deref().map(convert_language_name);

    let target_lang = match (
        request.destination.first(),
        source_lang.as_deref(),
        request.destination.get(1),
    ) {
        (None, _, _) => "en".to_string(),
        (Some(first), Some(src), Some(second)) if convert_language_name(first) == src => {
            convert_language_name(second)
        }
        (Some(first), _, _) => convert_language_name(first),
    };

    let (translated_text, detected_source, _) =
        perform_translation(&state, &request.text, source_lang, &target_lang).await?;

    Ok(Json(HcfyTranslationResponse {
        text: request.text,
        from: get_language_name(&detected_source),
        to: get_language_name(&target_lang),
        result: vec![translated_text],
    }))
}

#[derive(Debug, Deserialize)]
pub struct DeeplxTranslationRequest {
    text: String,
    source_lang: Option<String>,
    target_lang: String,
}

#[derive(Debug, Serialize)]
pub struct DeeplxTranslationResponse {
    code: u32,
    id: u128,
    data: String,
    alternatives: Vec<String>,
    source_lang: String,
    target_lang: String,
    method: String,
}

pub async fn translate_deeplx(
    State(state): State<Arc<AppState>>,
    Json(request): Json<DeeplxTranslationRequest>,
) -> Result<Json<DeeplxTranslationResponse>, AppError> {
    let (text, from_lang, to_lang) = perform_translation(
        &state,
        &request.text,
        request.source_lang.map(|lang| lang.to_lowercase()),
        &request.target_lang.to_lowercase(),
    )
    .await?;

    Ok(Json(DeeplxTranslationResponse {
        code: 200,
        id: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        data: text,
        alternatives: vec![],
        source_lang: from_lang.to_uppercase(),
        target_lang: to_lang.to_uppercase(),
        method: "Free".to_owned(),
    }))
}

pub async fn languages(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LanguagesResponse>, AppError> {
    let language_pairs = state.inference.language_pairs();

    let languages = language_pairs
        .iter()
        .map(|(from, to_set)| {
            (
                language_code(from).to_string(),
                to_set
                    .iter()
                    .map(|l| language_code(l).to_string())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect();

    Ok(Json(LanguagesResponse(languages)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_response_omits_batch_sources() {
        let response = TranslationResponse {
            text: TranslationOutput::Single("Hello".into()),
            from: "zh".into(),
            to: "en".into(),
            sources: None,
        };
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({"text": "Hello", "from": "zh", "to": "en"})
        );
    }

    #[test]
    fn batch_source_language_ignores_unknown_sources_when_known_sources_exist() {
        assert_eq!(
            batch_source_language(["und".to_owned(), "en".to_owned()]),
            "en"
        );
        assert_eq!(
            batch_source_language(["en".to_owned(), "ja".to_owned()]),
            "mul"
        );
        assert_eq!(batch_source_language(["und".to_owned()]), "und");
    }

    #[tokio::test]
    async fn detect_endpoint_returns_language_or_bad_request_for_unknown_text() {
        let detected = detect_language(Json(DetectLanguageRequest {
            text: "View Paper".to_owned(),
        }))
        .await
        .unwrap();
        assert_eq!(detected.0.language, "en");

        let error = detect_language(Json(DetectLanguageRequest {
            text: "12345".to_owned(),
        }))
        .await
        .unwrap_err();
        assert!(matches!(error, AppError::TranslationError(_)));
    }
}
