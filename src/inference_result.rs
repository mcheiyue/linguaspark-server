use crate::AppError;
use linguaspark::{StopReason, Translation};

pub fn collect(results: Vec<Translation>, count: usize) -> Result<Vec<String>, AppError> {
    if results.len() != count {
        return Err(AppError::InferenceError(
            "Inference result count mismatch".into(),
        ));
    }
    results
        .into_iter()
        .map(|result| {
            validate(result.stop_reason, &result.text)?;
            Ok(result.text)
        })
        .collect()
}

fn validate(reason: StopReason, text: &str) -> Result<(), AppError> {
    match reason {
        StopReason::EndOfSentence if !text.trim().is_empty() => Ok(()),
        StopReason::EndOfSentence => Err(AppError::InferenceError(
            "Inference returned empty text".into(),
        )),
        StopReason::LengthLimit => Err(AppError::InferenceError(
            "Translation reached output length limit; partial result rejected".into(),
        )),
        other => Err(AppError::InferenceError(format!(
            "Unsupported inference stop reason: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_and_empty_results() {
        assert!(validate(StopReason::LengthLimit, "partial output").is_err());
        assert!(validate(StopReason::EndOfSentence, " \n").is_err());
        assert!(validate(StopReason::EndOfSentence, "完整译文").is_ok());
        assert!(collect(vec![], 1).is_err());
    }
}
