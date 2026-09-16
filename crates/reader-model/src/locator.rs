//! The cross-scheme reading [`Locator`] (design report §19.3).
//!
//! A locator never stores a bare page number: after a font, window or edition
//! change a page number is meaningless. Instead it carries an edition, section
//! and anchor evidence plus a `progression` value for cheap sorting and for
//! showing progress in the UI.
//!
//! Unknown fields coming from a newer client are preserved through
//! [`Locator::extra`] so that a round trip through an older build cannot drop
//! data (the sync protocol requires additive-only changes).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cfi::{Cfi, CfiError};

/// A normalized rectangle (0.0..=1.0 relative to the page or image).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// Text quote selector evidence (W3C style `exact` + `prefix` + `suffix`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TextQuote {
    /// The selected text.
    pub exact: String,
    /// Text preceding the selection, used to disambiguate repeats.
    #[serde(default)]
    pub prefix: String,
    /// Text following the selection, used to disambiguate repeats.
    #[serde(default)]
    pub suffix: String,
}

impl TextQuote {
    /// Build a quote with surrounding context.
    pub fn new(exact: impl Into<String>, prefix: impl Into<String>, suffix: impl Into<String>) -> Self {
        Self {
            exact: exact.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
        }
    }
}

/// A position inside a book edition.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Locator {
    /// Publication ID (required: a locator without a book is meaningless).
    pub book_id: String,
    /// Edition ID. Required as soon as a progression is stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition_id: Option<String>,
    /// Spine item / section identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    /// EPUB CFI, the primary structural anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cfi: Option<String>,
    /// XPointer anchor, used for KOReader interoperability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xpointer: Option<String>,
    /// Deterministic text block ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    /// Character offset of the start inside the block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<u32>,
    /// Character offset of the end inside the block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<u32>,
    /// Text quote evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_quote: Option<TextQuote>,
    /// Fingerprint of the section text at the time the locator was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_fingerprint: Option<String>,
    /// Normalized reading progression within the edition, 0.0..=1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progression: Option<f64>,
    /// Page number, only meaningful for fixed-layout documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Normalized rectangles for image / fixed-layout anchors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rects: Option<Vec<Rect>>,
    /// Last update time, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields written by a newer client, preserved verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Why a locator was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LocatorError {
    /// No anchor evidence at all.
    #[error("locator for book {0:?} has no anchor (cfi, xpointer, block_id or text_quote required)")]
    NoAnchor(String),
    /// Start offset after end offset.
    #[error("start_offset {start} must not be greater than end_offset {end}")]
    OffsetRange {
        /// Start offset.
        start: u32,
        /// End offset.
        end: u32,
    },
    /// Progression outside the valid range.
    #[error("progression {0} must be within 0.0..=1.0")]
    ProgressionOutOfRange(f64),
    /// Progressive position without an edition.
    #[error("a locator with progression must carry edition_id so it survives re-layout")]
    ProgressionWithoutEdition,
    /// The CFI failed to parse.
    #[error("cfi is not valid: {0}")]
    InvalidCfi(#[from] CfiError),
    /// A rectangle was not normalized.
    #[error("rect {index} must be normalized to 0.0..=1.0 (got x={x}, y={y}, width={width}, height={height})")]
    UnnormalizedRect {
        /// Index of the offending rectangle.
        index: usize,
        /// Left edge.
        x: f64,
        /// Top edge.
        y: f64,
        /// Width.
        width: f64,
        /// Height.
        height: f64,
    },
}

impl Locator {
    /// Create a minimal locator for a publication.
    pub fn for_book(book_id: impl Into<String>) -> Self {
        Self {
            book_id: book_id.into(),
            ..Self::default()
        }
    }

    /// Whether the locator carries any anchor evidence.
    pub fn has_anchor(&self) -> bool {
        self.cfi.is_some()
            || self.xpointer.is_some()
            || self.block_id.is_some()
            || self.text_quote.is_some()
    }

    /// Parse the CFI, if one is present.
    pub fn parsed_cfi(&self) -> Option<Result<Cfi, CfiError>> {
        self.cfi.as_deref().map(Cfi::parse)
    }

    /// Full structural validation used before persisting or syncing.
    pub fn validate(&self) -> Result<(), LocatorError> {
        if !self.has_anchor() {
            return Err(LocatorError::NoAnchor(self.book_id.clone()));
        }
        if let (Some(start), Some(end)) = (self.start_offset, self.end_offset)
            && start > end
        {
            return Err(LocatorError::OffsetRange { start, end });
        }
        if let Some(progression) = self.progression {
            if !(0.0..=1.0).contains(&progression) || progression.is_nan() {
                return Err(LocatorError::ProgressionOutOfRange(progression));
            }
            if self.edition_id.is_none() {
                return Err(LocatorError::ProgressionWithoutEdition);
            }
        }
        if let Some(rects) = &self.rects {
            for (index, rect) in rects.iter().enumerate() {
                let normalized = (0.0..=1.0).contains(&rect.x)
                    && (0.0..=1.0).contains(&rect.y)
                    && rect.width >= 0.0
                    && rect.height >= 0.0
                    && rect.x + rect.width <= 1.0 + f64::EPSILON
                    && rect.y + rect.height <= 1.0 + f64::EPSILON;
                if !normalized {
                    return Err(LocatorError::UnnormalizedRect {
                        index,
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    });
                }
            }
        }
        if let Some(result) = self.parsed_cfi() {
            result?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor_locator() -> Locator {
        Locator {
            book_id: "book-1".into(),
            edition_id: Some("edition-1".into()),
            section_id: Some("text/chapter1.xhtml".into()),
            cfi: Some("epubcfi(/6/24!/4/20/1:58)".into()),
            block_id: Some("block-abc".into()),
            start_offset: Some(58),
            end_offset: Some(72),
            text_quote: Some(TextQuote::new("selected text", "before ", " after")),
            doc_fingerprint: Some("sha256-deadbeef".into()),
            progression: Some(0.64),
            ..Locator::default()
        }
    }

    #[test]
    fn accepts_a_well_formed_locator() {
        assert!(anchor_locator().validate().is_ok());
    }

    #[test]
    fn rejects_locator_without_anchor() {
        let locator = Locator::for_book("book-1");
        assert_eq!(
            locator.validate().unwrap_err(),
            LocatorError::NoAnchor("book-1".into())
        );
    }

    #[test]
    fn rejects_reversed_offsets() {
        let mut locator = anchor_locator();
        locator.start_offset = Some(90);
        locator.end_offset = Some(10);
        assert!(matches!(
            locator.validate().unwrap_err(),
            LocatorError::OffsetRange { .. }
        ));
    }

    #[test]
    fn rejects_progression_without_edition_or_out_of_range() {
        let mut locator = anchor_locator();
        locator.edition_id = None;
        assert_eq!(
            locator.validate().unwrap_err(),
            LocatorError::ProgressionWithoutEdition
        );

        let mut locator = anchor_locator();
        locator.progression = Some(1.4);
        assert_eq!(
            locator.validate().unwrap_err(),
            LocatorError::ProgressionOutOfRange(1.4)
        );
    }

    #[test]
    fn rejects_broken_cfi_and_unnormalized_rects() {
        let mut locator = anchor_locator();
        locator.cfi = Some("epubcfi(/6/4[broken)".into());
        assert!(matches!(
            locator.validate().unwrap_err(),
            LocatorError::InvalidCfi(_)
        ));

        let mut locator = anchor_locator();
        locator.rects = Some(vec![Rect {
            x: 0.9,
            y: 0.0,
            width: 0.5,
            height: 0.5,
        }]);
        assert!(matches!(
            locator.validate().unwrap_err(),
            LocatorError::UnnormalizedRect { index: 0, .. }
        ));
    }

    #[test]
    fn preserves_unknown_fields_from_newer_clients() {
        let json = serde_json::json!({
            "book_id": "book-1",
            "block_id": "block-1",
            "future_field": { "nested": [1, 2, 3] }
        });
        let locator: Locator = serde_json::from_value(json.clone()).expect("deserialize");
        assert_eq!(
            locator.extra.get("future_field"),
            Some(&serde_json::json!({ "nested": [1, 2, 3] }))
        );
        let round_tripped = serde_json::to_value(&locator).expect("serialize");
        assert_eq!(
            round_tripped.get("future_field"),
            json.get("future_field")
        );
        assert!(round_tripped.get("page").is_none(), "absent optionals stay absent");
    }

    #[test]
    fn matches_the_documented_json_shape() {
        let json = serde_json::json!({
            "book_id": "pub-1",
            "edition_id": "ed-1",
            "section_id": "text/ch1.xhtml",
            "cfi": "epubcfi(/6/24!/4/20/1:58)",
            "xpointer": null,
            "block_id": "block-1",
            "start_offset": 58,
            "end_offset": 72,
            "text_quote": { "exact": "hello", "prefix": "a ", "suffix": " b" },
            "doc_fingerprint": "sha256-abc",
            "progression": 0.64,
            "page": null,
            "rects": null,
            "updated_at": "2026-09-15T10:00:00Z"
        });
        let locator: Locator = serde_json::from_value(json).expect("deserialize documented shape");
        assert!(locator.validate().is_ok());
        assert_eq!(locator.start_offset, Some(58));
        assert_eq!(
            locator.parsed_cfi().expect("cfi present").expect("valid"),
            Cfi::parse("epubcfi(/6/24!/4/20/1:58)").expect("valid")
        );
    }
}
