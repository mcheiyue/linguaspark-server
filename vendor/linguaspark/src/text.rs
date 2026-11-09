// 本地修改：缓存纯 ASCII 数字词片 ID；来源与许可见本 crate 的 README。
use sentencepiece_rs::{Piece, PieceType, SentencePieceProcessor};

use crate::error::{LoadError, TranslateError};

/// A vocabulary token ID.
pub type TokenId = u32;

/// `SentencePiece` vocabulary hidden behind a small translation-specific API.
///
/// Keeping the third-party processor private prevents its filesystem helpers
/// and usize-based IDs from leaking into the core inference interfaces.
pub(crate) struct Vocabulary {
    processor: SentencePieceProcessor,
    numeric_piece_ids: Vec<TokenId>,
}

impl Vocabulary {
    /// Load a serialized `SentencePiece` model.
    pub(crate) fn load(bytes: Vec<u8>) -> Result<Self, LoadError> {
        let processor = SentencePieceProcessor::from_serialized_model(&bytes)
            .map_err(|err| LoadError::InvalidVocabulary(err.to_string()))?;
        if processor.model().vocab_size() > TokenId::MAX as usize {
            return Err(LoadError::InvalidVocabulary(
                "vocabulary is too large for 32-bit token IDs".into(),
            ));
        }
        let numeric_piece_ids = processor
            .model()
            .pieces()
            .iter()
            .enumerate()
            .map(|(id, piece)| numeric_piece_id(id, piece))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(Self {
            processor,
            numeric_piece_ids,
        })
    }

    #[must_use]
    pub(crate) fn numeric_piece_ids(&self) -> &[TokenId] {
        &self.numeric_piece_ids
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.processor.model().vocab_size()
    }

    #[must_use]
    pub(crate) fn eos_id(&self) -> Option<TokenId> {
        self.processor.eos_id().map(|id| id as TokenId)
    }

    #[must_use]
    pub(crate) fn unk_id(&self) -> TokenId {
        self.processor.unk_id() as TokenId
    }

    /// Encode text into vocabulary IDs.
    pub(crate) fn encode(
        &self,
        input: &str,
        add_eos: bool,
    ) -> Result<Vec<TokenId>, TranslateError> {
        let mut ids = self
            .processor
            .encode_to_ids(input)
            .map_err(|err| TranslateError::Runtime(err.to_string()))?
            .into_iter()
            .map(|id| id as TokenId)
            .collect::<Vec<_>>();
        if add_eos {
            let eos = self.eos_id().ok_or_else(|| {
                TranslateError::Runtime("source vocabulary has no EOS token".into())
            })?;
            ids.push(eos);
        }
        Ok(ids)
    }

    /// Decode vocabulary IDs into text.
    pub(crate) fn decode(&self, ids: &[TokenId]) -> Result<String, TranslateError> {
        let ids = ids.iter().map(|&id| id as usize).collect::<Vec<_>>();
        self.processor
            .decode_ids(&ids)
            .map_err(|err| TranslateError::Runtime(err.to_string()))
    }
}

fn numeric_piece_id(id: usize, piece: &Piece) -> Result<Option<TokenId>, LoadError> {
    if !matches!(piece.kind, PieceType::Normal | PieceType::UserDefined) {
        return Ok(None);
    }
    let piece = piece.piece.strip_prefix('▁').unwrap_or(&piece.piece);
    if piece.is_empty() || !piece.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    TokenId::try_from(id).map(Some).map_err(|_| {
        LoadError::InvalidVocabulary("numeric piece ID exceeds 32-bit token IDs".into())
    })
}

#[cfg(test)]
mod tests {
    use super::{Vocabulary, numeric_piece_id};
    use sentencepiece_rs::{Piece, PieceType};

    #[test]
    fn numeric_piece_ids_include_all_ascii_digit_segmentations() {
        let pieces = [
            Piece {
                piece: "0".into(),
                score: 0.0,
                kind: PieceType::Normal,
            },
            Piece {
                piece: "00".into(),
                score: 0.0,
                kind: PieceType::Normal,
            },
            Piece {
                piece: "▁360".into(),
                score: 0.0,
                kind: PieceType::UserDefined,
            },
            Piece {
                piece: "3600".into(),
                score: 0.0,
                kind: PieceType::Normal,
            },
            Piece {
                piece: "<ctrl>".into(),
                score: 0.0,
                kind: PieceType::Control,
            },
            Piece {
                piece: "unused0".into(),
                score: 0.0,
                kind: PieceType::Unused,
            },
        ];
        let ids = pieces
            .iter()
            .enumerate()
            .filter_map(|(id, piece)| numeric_piece_id(id, piece).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids, [0, 1, 2, 3]);
    }

    #[test]
    fn rejects_invalid_sentencepiece_model() {
        assert!(Vocabulary::load(b"not sentencepiece".to_vec()).is_err());
    }
}
