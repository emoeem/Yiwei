//! Anchor data model (design report §20.1, §20.2).
//!
//! An annotation never stores a page number as its only position. It stores a
//! primary locator plus the evidence needed to find the text again after the
//! layout, the font or even the edition changed: section fingerprint, block ID,
//! character offsets and a text quote with context.

use reader_model::{Locator, Rect, TextQuote};
use serde::{Deserialize, Serialize};

/// Kinds of annotation the reader can create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationType {
    /// Highlight with a colour.
    Highlight,
    /// Underline.
    Underline,
    /// Strikethrough.
    Strikethrough,
    /// Bookmark (a position without a range).
    Bookmark,
    /// Free note attached to a position.
    Note,
    /// Comment on a selection.
    Comment,
    /// Quoted excerpt.
    Quote,
    /// Tag applied to a book or range.
    Tag,
    /// Screenshot.
    Screenshot,
    /// Annotation drawn on an image page.
    ImageAnnotation,
    /// Machine generated translation note.
    Translation,
}

/// Which addressing scheme the primary locator uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorScheme {
    /// EPUB CFI.
    EpubCfi,
    /// XPointer, used for KOReader interoperability.
    XPointer,
    /// DOM range.
    DomRange,
    /// PDF page + rectangles.
    Pdf,
    /// Text only.
    Text,
}

/// Where an anchor stands after relocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorState {
    /// Located with high confidence.
    Anchored,
    /// Located approximately; the UI should mark it.
    Fuzzy,
    /// Not located; kept and listed for the user, never deleted.
    Orphaned,
    /// A candidate was found but the user must confirm it.
    NeedsReview,
}

/// Who produced the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnchorOrigin {
    /// Created by the user.
    #[default]
    User,
    /// Moved from the original edition to a translation by machine.
    MachineMigrated,
    /// Produced by AI or OCR.
    MachineGenerated,
}

/// A persisted anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationAnchor {
    /// Addressing scheme of `primary_locator`.
    pub scheme: AnchorScheme,
    /// Primary position.
    pub primary_locator: Locator,
    /// Document identity inside the edition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<String>,
    /// Section / spine item identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    /// Deterministic text block identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    /// Text quote evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<TextQuote>,
    /// Character offset of the start inside the block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<u32>,
    /// Character offset of the end inside the block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<u32>,
    /// Fingerprint of the section text when the anchor was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_fingerprint: Option<String>,
    /// Normalized rectangles for fixed layout documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rects: Option<Vec<Rect>>,
    /// Page number for PDF / scanned content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Confidence in the current position, 0.0..=1.0.
    pub confidence: f64,
    /// Current state.
    pub state: AnchorState,
    /// Who produced the anchor.
    #[serde(default)]
    pub origin: AnchorOrigin,
    /// Last update time, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl AnnotationAnchor {
    /// A user anchor at a locator, before any relocation happened.
    pub fn new(scheme: AnchorScheme, primary_locator: Locator) -> Self {
        Self {
            scheme,
            primary_locator,
            doc_id: None,
            section_id: None,
            block_id: None,
            quote: None,
            start_offset: None,
            end_offset: None,
            doc_fingerprint: None,
            rects: None,
            page: None,
            confidence: 1.0,
            state: AnchorState::Anchored,
            origin: AnchorOrigin::User,
            updated_at: None,
        }
    }

    /// The quoted text, if any.
    pub fn exact_text(&self) -> Option<&str> {
        self.quote.as_ref().map(|quote| quote.exact.as_str())
    }

    /// Whether the anchor currently resolves to a usable position.
    pub fn is_usable(&self) -> bool {
        matches!(self.state, AnchorState::Anchored | AnchorState::Fuzzy)
    }
}

