//! Anchor merge rules for sync (design report §21.4 `mergeAnnotationAnchor`).
//!
//! The rules are deliberately conservative:
//!
//! 1. both sides anchored → merge field by field, keeping the more confident
//!    position as the primary one,
//! 2. one side orphaned → keep the anchored side as primary *and* keep the
//!    orphaned record as an alternative, so nothing is lost,
//! 3. both orphaned → keep the newer one as primary and the other as an
//!    alternative, and ask the user to review.

use serde::{Deserialize, Serialize};

use crate::types::AnnotationAnchor;

/// Which branch of the merge rule was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorMergeRule {
    /// Both sides resolved to a position.
    BothAnchored,
    /// One side resolved, the other is orphaned.
    AnchoredWins,
    /// Neither side resolved.
    BothOrphaned,
}

/// Result of merging two anchors.
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorMerge {
    /// The position the annotation should use.
    pub primary: AnnotationAnchor,
    /// A candidate that was not discarded.
    pub alternative: Option<AnnotationAnchor>,
    /// Whether the user must confirm the position.
    pub requires_review: bool,
    /// Which rule was applied.
    pub rule: AnchorMergeRule,
}

/// Merge two anchors for the same annotation.
pub fn merge_anchor(local: &AnnotationAnchor, remote: &AnnotationAnchor) -> AnchorMerge {
    match (local.is_usable(), remote.is_usable()) {
        (true, true) => {
            let (primary, secondary) = if more_confident(local, remote) {
                (local, remote)
            } else {
                (remote, local)
            };
            AnchorMerge {
                primary: merge_fields(primary, secondary),
                alternative: None,
                requires_review: false,
                rule: AnchorMergeRule::BothAnchored,
            }
        }
        (true, false) => AnchorMerge {
            primary: merge_fields(local, remote),
            alternative: Some(remote.clone()),
            requires_review: false,
            rule: AnchorMergeRule::AnchoredWins,
        },
        (false, true) => AnchorMerge {
            primary: merge_fields(remote, local),
            alternative: Some(local.clone()),
            requires_review: false,
            rule: AnchorMergeRule::AnchoredWins,
        },
        (false, false) => {
            let (primary, secondary) = if newer(local, remote) {
                (local, remote)
            } else {
                (remote, local)
            };
            AnchorMerge {
                primary: primary.clone(),
                alternative: Some(secondary.clone()),
                requires_review: true,
                rule: AnchorMergeRule::BothOrphaned,
            }
        }
    }
}

/// Prefer the more confident anchor; ties are broken deterministically so both
/// devices pick the same primary.
fn more_confident(left: &AnnotationAnchor, right: &AnnotationAnchor) -> bool {
    match left.confidence.partial_cmp(&right.confidence) {
        Some(std::cmp::Ordering::Greater) => true,
        Some(std::cmp::Ordering::Less) => false,
        _ => sort_key(left) >= sort_key(right),
    }
}

/// Prefer the newer anchor by RFC 3339 timestamp, then deterministically.
fn newer(left: &AnnotationAnchor, right: &AnnotationAnchor) -> bool {
    match (left.updated_at.as_deref(), right.updated_at.as_deref()) {
        (Some(left_at), Some(right_at)) if left_at != right_at => left_at > right_at,
        _ => sort_key(left) >= sort_key(right),
    }
}

fn sort_key(anchor: &AnnotationAnchor) -> String {
    serde_json::to_string(anchor).unwrap_or_default()
}

/// Fill in missing evidence from the secondary anchor without overwriting what
/// the primary already knows.
fn merge_fields(primary: &AnnotationAnchor, secondary: &AnnotationAnchor) -> AnnotationAnchor {
    let mut merged = primary.clone();
    if merged.section_id.is_none() {
        merged.section_id = secondary.section_id.clone();
    }
    if merged.block_id.is_none() {
        merged.block_id = secondary.block_id.clone();
    }
    if merged.quote.is_none() {
        merged.quote = secondary.quote.clone();
    }
    if merged.doc_fingerprint.is_none() {
        merged.doc_fingerprint = secondary.doc_fingerprint.clone();
    }
    if merged.rects.is_none() {
        merged.rects = secondary.rects.clone();
    }
    if merged.page.is_none() {
        merged.page = secondary.page;
    }
    if merged.start_offset.is_none() {
        merged.start_offset = secondary.start_offset;
    }
    if merged.end_offset.is_none() {
        merged.end_offset = secondary.end_offset;
    }
    merged
}

