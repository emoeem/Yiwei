//! Annotation model, relocation and merge rules (design report §20 and §21.4).
//!
//! This crate owns everything about *where an annotation is*:
//!
//! * [`types`] — the persisted anchor model,
//! * [`text`] — normalization and similarity used for matching,
//! * [`relocate`] — the seven-step relocation pipeline with the confidence
//!   table from §20.3,
//! * [`merge`] — the anchor merge rules from §21.4.
//!
//! It deliberately does not talk to the database, the renderer or the network:
//! it takes a text index and returns positions, which keeps it testable and
//! keeps the DOM-dependent parts (CFI creation) in the renderer.

pub mod merge;
pub mod relocate;
pub mod text;
pub mod types;

#[cfg(test)]
mod tests;

pub use merge::{AnchorMerge, AnchorMergeRule, merge_anchor};
pub use relocate::{
    ANCHORED_THRESHOLD, BlockText, Candidate, FUZZY_THRESHOLD, REVIEW_THRESHOLD, Relocation,
    RelocationStep, RelocationWarning, ResolvedPosition, SectionText, TextIndex, relocate,
    state_for_confidence,
};
pub use text::{MAX_FUZZY_NEEDLE_CHARS, MAX_FUZZY_TEXT_CHARS, normalize_text, similarity};
pub use types::{
    AnchorOrigin, AnchorScheme, AnchorState, AnnotationAnchor, AnnotationType,
};

