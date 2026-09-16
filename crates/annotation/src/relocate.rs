//! Anchor relocation pipeline (design report §20.3).
//!
//! The pipeline tries, in order: exact CFI with an unchanged fingerprint, a CFI
//! that still resolves structurally, block identity plus character offsets, the
//! text quote, a fuzzy comparison, then a search across the rest of the
//! edition. Every step reports the confidence from §20.3, and nothing is ever
//! deleted: an annotation that cannot be located becomes `orphaned` and stays
//! in the database for the user to resolve.
//!
//! The pipeline works on the derived text index, not on the DOM, so it does
//! **not** mint a new CFI — only the renderer can do that. It returns the new
//! section/block/offset, which the caller combines with a fresh CFI from the
//! renderer; the original locator is kept untouched.

use std::collections::BTreeMap;

use reader_model::Cfi;
use serde::{Deserialize, Serialize};

use crate::text::{
    MAX_FUZZY_NEEDLE_CHARS, MAX_FUZZY_TEXT_CHARS, normalize_text, normalize_with_map, similarity,
};
use crate::types::{AnchorState, AnnotationAnchor};

/// Confidence at or above which a position counts as anchored.
pub const ANCHORED_THRESHOLD: f64 = 0.9;
/// Confidence at or above which a position must be confirmed by the user.
pub const REVIEW_THRESHOLD: f64 = 0.7;
/// Confidence at or above which a position is kept as a fuzzy match.
pub const FUZZY_THRESHOLD: f64 = 0.5;

/// A block of text inside one section of the derived index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockText {
    /// Deterministic block ID.
    pub block_id: String,
    /// Ordinal inside the section.
    pub ordinal: u32,
    /// Block text.
    pub text: String,
}

/// One section (spine item) of the derived index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionText {
    /// Section identity, e.g. the spine href.
    pub section_id: String,
    /// Spine index, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spine_index: Option<u32>,
    /// Fingerprint of the section text when the index was built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_fingerprint: Option<String>,
    /// Text blocks in reading order.
    pub blocks: Vec<BlockText>,
}

impl SectionText {
    /// Find a block by ID.
    pub fn block(&self, block_id: &str) -> Option<&BlockText> {
        self.blocks.iter().find(|block| block.block_id == block_id)
    }
}

/// The searchable text index of one edition.
#[derive(Debug, Clone, Default)]
pub struct TextIndex {
    sections: BTreeMap<String, SectionText>,
    by_spine: BTreeMap<u32, String>,
}

impl TextIndex {
    /// Build an index from sections.
    pub fn new(sections: impl IntoIterator<Item = SectionText>) -> Self {
        let mut index = Self::default();
        for section in sections {
            if let Some(spine) = section.spine_index {
                index.by_spine.insert(spine, section.section_id.clone());
            }
            index.sections.insert(section.section_id.clone(), section);
        }
        index
    }

    /// Look up a section by ID.
    pub fn section(&self, section_id: &str) -> Option<&SectionText> {
        self.sections.get(section_id)
    }

    /// Look up a section by spine index.
    pub fn section_for_spine(&self, spine_index: u32) -> Option<&SectionText> {
        self.by_spine
            .get(&spine_index)
            .and_then(|id| self.sections.get(id))
    }

    /// All sections.
    pub fn sections(&self) -> impl Iterator<Item = &SectionText> {
        self.sections.values()
    }
}

/// Which step of §20.3 produced the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelocationStep {
    /// Exact CFI with an unchanged document fingerprint.
    ExactCfi,
    /// CFI that still resolves structurally (changed fingerprint or spelling).
    NormalizedCfi,
    /// Block identity plus character offsets.
    BlockOffset,
    /// Text quote match inside the anchor's section.
    TextQuote,
    /// Fuzzy comparison inside the anchor's section.
    FuzzyMatch,
    /// High-confidence candidate found in another section of the same edition.
    GlobalSearch,
    /// Nothing could be located.
    Unresolved,
}

/// A position in the derived index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPosition {
    /// Section the text was found in.
    pub section_id: String,
    /// Block the text was found in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    /// Start character offset inside the block.
    pub start: u32,
    /// End character offset inside the block.
    pub end: u32,
}

/// A possible position, with its confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Position.
    #[serde(flatten)]
    pub position: ResolvedPosition,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
}

/// Something the caller must surface or record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelocationWarning {
    /// The stored CFI could not be parsed at all.
    CfiUnparsable {
        /// The offending CFI.
        cfi: String,
    },
    /// A fuzzy comparison was skipped because the block is too large.
    FuzzyComparisonSkipped {
        /// Block that was skipped.
        block_id: String,
        /// Why it was skipped.
        reason: String,
    },
    /// The text was found somewhere else in the edition.
    MovedToAnotherSection {
        /// Section the anchor used to point at.
        from: String,
        /// Section the text is in now.
        to: String,
    },
}

/// Result of relocating one anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relocation {
    /// The anchor as it was.
    pub original: AnnotationAnchor,
    /// The anchor with updated state and confidence.
    pub anchor: AnnotationAnchor,
    /// Step that produced the result.
    pub step: RelocationStep,
    /// New position, when one was found.
    pub resolved: Option<ResolvedPosition>,
    /// Other candidates, ordered by descending confidence.
    pub candidates: Vec<Candidate>,
    /// Warnings to record and show.
    pub warnings: Vec<RelocationWarning>,
}

/// Relocate one anchor against the current text index.
pub fn relocate(anchor: &AnnotationAnchor, index: &TextIndex) -> Relocation {
    let mut warnings = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let anchor_section = anchor
        .section_id
        .as_deref()
        .and_then(|id| index.section(id));

    // Steps 1 and 2: structural CFI.
    let mut cfi_section: Option<(&SectionText, bool)> = None;
    if let Some(cfi_text) = anchor.primary_locator.cfi.as_deref() {
        match Cfi::parse(cfi_text) {
            Ok(cfi) => {
                if let Some(section) = cfi.spine_index().and_then(|i| index.section_for_spine(i)) {
                    let fingerprint_matches = match (
                        anchor.doc_fingerprint.as_deref(),
                        section.doc_fingerprint.as_deref(),
                    ) {
                        (Some(expected), Some(actual)) => expected == actual,
                        _ => false,
                    };
                    cfi_section = Some((section, fingerprint_matches));
                }
            }
            Err(_) => warnings.push(RelocationWarning::CfiUnparsable {
                cfi: cfi_text.to_string(),
            }),
        }
    }

    if let Some((section, true)) = cfi_section {
        let resolved = anchor
            .block_id
            .as_deref()
            .and_then(|block_id| offset_position(section, block_id, anchor))
            .or_else(|| best_candidate(match_quote(anchor, section)));
        return finish(
            anchor,
            RelocationStep::ExactCfi,
            resolved,
            1.0,
            candidates,
            warnings,
        );
    }

    if let Some((section, false)) = cfi_section {
        let resolved = anchor
            .block_id
            .as_deref()
            .and_then(|block_id| offset_position(section, block_id, anchor))
            .or_else(|| best_candidate(match_quote(anchor, section)));
        if let Some(position) = resolved {
            return finish(
                anchor,
                RelocationStep::NormalizedCfi,
                Some(position),
                0.95,
                candidates,
                warnings,
            );
        }
    }

    // Step 3: block identity plus character offsets.
    if let (Some(section), Some(block_id)) = (anchor_section, anchor.block_id.as_deref())
        && let Some(position) = offset_position(section, block_id, anchor)
    {
        let confidence = match anchor.exact_text() {
            None => 0.90,
            Some(exact) => match text_at(section, block_id, position.start, position.end) {
                Some(found) if found == exact => 0.92,
                Some(_) => 0.60,
                None => 0.75,
            },
        };
        candidates.push(Candidate {
            position: position.clone(),
            confidence,
        });
        if confidence >= REVIEW_THRESHOLD {
            return finish(
                anchor,
                RelocationStep::BlockOffset,
                Some(position),
                confidence,
                candidates,
                warnings,
            );
        }
    }

    // Steps 4 and 5: quote match, then fuzzy match, inside the same section.
    if let Some(section) = anchor_section {
        let quote_matches = match_quote(anchor, section);
        candidates.extend(quote_matches.iter().cloned());
        if let Some(position) = best_candidate(quote_matches) {
            let confidence = confidence_of(&candidates, &position);
            return finish(
                anchor,
                RelocationStep::TextQuote,
                Some(position),
                confidence,
                candidates,
                warnings,
            );
        }

        let fuzzy_matches = fuzzy_match(anchor, section, &mut warnings);
        candidates.extend(fuzzy_matches.iter().cloned());
        if let Some(position) = best_candidate(fuzzy_matches) {
            let confidence = confidence_of(&candidates, &position);
            return finish(
                anchor,
                RelocationStep::FuzzyMatch,
                Some(position),
                confidence,
                candidates,
                warnings,
            );
        }
    }

    // Step 6: search the rest of the edition.
    if let Some(anchor_section) = anchor_section {
        for section in index.sections() {
            if section.section_id == anchor_section.section_id {
                continue;
            }
            let matches = match_quote(anchor, section);
            if matches.is_empty() {
                continue;
            }
            let confidence = matches
                .iter()
                .map(|candidate| candidate.confidence)
                .fold(0.0f64, f64::max)
                .min(0.90);
            let position = matches[0].position.clone();
            candidates.push(Candidate {
                position: position.clone(),
                confidence,
            });
            warnings.push(RelocationWarning::MovedToAnotherSection {
                from: anchor_section.section_id.clone(),
                to: section.section_id.clone(),
            });
            return finish(
                anchor,
                RelocationStep::GlobalSearch,
                Some(position),
                confidence,
                candidates,
                warnings,
            );
        }
    }

    // Step 7: keep the annotation and mark it orphaned.
    sort_candidates(&mut candidates);
    finish(
        anchor,
        RelocationStep::Unresolved,
        None,
        0.0,
        candidates,
        warnings,
    )
}

fn finish(
    anchor: &AnnotationAnchor,
    step: RelocationStep,
    resolved: Option<ResolvedPosition>,
    confidence: f64,
    candidates: Vec<Candidate>,
    warnings: Vec<RelocationWarning>,
) -> Relocation {
    let mut updated = anchor.clone();
    updated.confidence = confidence;
    updated.state = state_for_confidence(confidence);
    if let Some(position) = &resolved {
        updated.section_id = Some(position.section_id.clone());
        updated.block_id = position.block_id.clone();
        updated.start_offset = Some(position.start);
        updated.end_offset = Some(position.end);
    }
    let mut candidates = candidates;
    sort_candidates(&mut candidates);
    Relocation {
        original: anchor.clone(),
        anchor: updated,
        step,
        resolved,
        candidates,
        warnings,
    }
}

fn sort_candidates(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Map a confidence onto the state machine of §20.2.
pub fn state_for_confidence(confidence: f64) -> AnchorState {
    if confidence >= ANCHORED_THRESHOLD {
        AnchorState::Anchored
    } else if confidence >= REVIEW_THRESHOLD {
        AnchorState::NeedsReview
    } else if confidence >= FUZZY_THRESHOLD {
        AnchorState::Fuzzy
    } else {
        AnchorState::Orphaned
    }
}

/// Position from explicit character offsets, when both ends are known.
fn offset_position(
    section: &SectionText,
    block_id: &str,
    anchor: &AnnotationAnchor,
) -> Option<ResolvedPosition> {
    let block = section.block(block_id)?;
    let length = block.text.chars().count() as u32;
    let start = anchor.start_offset?.min(length);
    let end = anchor.end_offset?.min(length).max(start);
    Some(ResolvedPosition {
        section_id: section.section_id.clone(),
        block_id: Some(block_id.to_string()),
        start,
        end,
    })
}

fn text_at(section: &SectionText, block_id: &str, start: u32, end: u32) -> Option<String> {
    let block = section.block(block_id)?;
    let chars: Vec<char> = block.text.chars().collect();
    let start = start as usize;
    let end = (end as usize).min(chars.len());
    if start >= end {
        return None;
    }
    Some(chars[start..end].iter().collect())
}

fn best_candidate(candidates: Vec<Candidate>) -> Option<ResolvedPosition> {
    candidates
        .into_iter()
        .filter(|candidate| candidate.confidence >= FUZZY_THRESHOLD)
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|candidate| candidate.position)
}

fn confidence_of(candidates: &[Candidate], position: &ResolvedPosition) -> f64 {
    candidates
        .iter()
        .filter(|candidate| &candidate.position == position)
        .map(|candidate| candidate.confidence)
        .fold(0.0f64, f64::max)
}

/// Step 4: exact (or whitespace-insensitive) quote matching inside one section.
fn match_quote(anchor: &AnnotationAnchor, section: &SectionText) -> Vec<Candidate> {
    let Some(quote) = anchor.quote.as_ref() else {
        return Vec::new();
    };
    let exact = quote.exact.trim();
    if exact.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for block in &section.blocks {
        let raw_matches = find_exact(&block.text, exact);
        let matches = if raw_matches.is_empty() {
            find_exact_normalized(&block.text, exact)
        } else {
            raw_matches
        };
        if matches.is_empty() {
            continue;
        }
        let base: f64 = if matches.len() == 1 { 0.92 } else { 0.80 };
        let cap: f64 = if matches.len() == 1 { 0.99 } else { 0.90 };
        let chars: Vec<char> = block.text.chars().collect();
        for (start, end) in matches {
            let mut confidence = base;
            let before: String = chars[..start.min(chars.len())].iter().collect();
            let after: String = chars[end.min(chars.len())..].iter().collect();
            if context_matches(&before, &quote.prefix) {
                confidence += 0.03;
            }
            if context_matches(&after, &quote.suffix) {
                confidence += 0.03;
            }
            out.push(Candidate {
                position: ResolvedPosition {
                    section_id: section.section_id.clone(),
                    block_id: Some(block.block_id.clone()),
                    start: start as u32,
                    end: end as u32,
                },
                confidence: confidence.min(cap),
            });
        }
    }
    out
}

fn find_exact(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (byte_offset, _) in text.match_indices(needle) {
        let start = text[..byte_offset].chars().count();
        out.push((start, start + needle.chars().count()));
    }
    out
}

/// Exact match against the normalized text, mapped back to original offsets.
fn find_exact_normalized(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let (normalized, map) = normalize_with_map(text);
    let normalized_needle = normalize_text(needle);
    if normalized_needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (byte_offset, _) in normalized.match_indices(&normalized_needle) {
        let start = normalized[..byte_offset].chars().count();
        let end = start + normalized_needle.chars().count();
        let Some(original_start) = map.get(start).copied() else {
            continue;
        };
        let original_end = map
            .get(end.saturating_sub(1))
            .map(|offset| offset + 1)
            .unwrap_or(original_start + 1);
        out.push((original_start, original_end));
    }
    out
}

/// Whether the text next to a candidate matches the quote's context.
fn context_matches(context: &str, expected: &str) -> bool {
    let expected = normalize_text(expected);
    if expected.is_empty() {
        return false;
    }
    let context = normalize_text(context);
    let context_chars: Vec<char> = context.chars().collect();
    let expected_chars: Vec<char> = expected.chars().collect();
    let take = expected_chars.len().min(context_chars.len());
    if take == 0 {
        return false;
    }
    context_chars[context_chars.len() - take..] == expected_chars[expected_chars.len() - take..]
}

/// Step 5: fuzzy matching inside one section.
fn fuzzy_match(
    anchor: &AnnotationAnchor,
    section: &SectionText,
    warnings: &mut Vec<RelocationWarning>,
) -> Vec<Candidate> {
    let Some(quote) = anchor.quote.as_ref() else {
        return Vec::new();
    };
    let needle = normalize_text(quote.exact.trim());
    let needle_len = needle.chars().count();
    if needle_len == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    for block in &section.blocks {
        let (normalized, map) = normalize_with_map(&block.text);
        let text_len = normalized.chars().count();
        if text_len > MAX_FUZZY_TEXT_CHARS || needle_len > MAX_FUZZY_NEEDLE_CHARS {
            warnings.push(RelocationWarning::FuzzyComparisonSkipped {
                block_id: block.block_id.clone(),
                reason: format!(
                    "block has {text_len} characters (limit {MAX_FUZZY_TEXT_CHARS}), quote has {needle_len} (limit {MAX_FUZZY_NEEDLE_CHARS})"
                ),
            });
            continue;
        }
        if text_len == 0 {
            continue;
        }
        let Some((start, end, score)) = best_window(&normalized, &needle) else {
            continue;
        };
        if score < FUZZY_THRESHOLD {
            continue;
        }
        let original_start = map.get(start).copied().unwrap_or(0);
        let original_end = map
            .get(end.saturating_sub(1))
            .map(|offset| offset + 1)
            .unwrap_or(original_start + 1);
        out.push(Candidate {
            position: ResolvedPosition {
                section_id: section.section_id.clone(),
                block_id: Some(block.block_id.clone()),
                start: original_start as u32,
                end: original_end as u32,
            },
            confidence: 0.5 + (score - 0.5) * 0.4,
        });
    }
    out
}

/// Best matching window of `needle` inside `text`, coarse-to-fine for speed.
fn best_window(text: &str, needle: &str) -> Option<(usize, usize, f64)> {
    let hay: Vec<char> = text.chars().collect();
    let needle_len = needle.chars().count();
    if needle_len == 0 || hay.len() < needle_len {
        return None;
    }

    let coarse_step = (needle_len / 8).max(1);
    let mut best: Option<(usize, f64)> = None;
    let mut start = 0usize;
    while start + needle_len <= hay.len() {
        let window: String = hay[start..start + needle_len].iter().collect();
        let score = similarity(&window, needle)?;
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((start, score));
        }
        start += coarse_step;
    }
    let (coarse_start, _) = best?;

    let last_start = hay.len() - needle_len;
    let refine_from = coarse_start.saturating_sub(coarse_step);
    let refine_to = (coarse_start + coarse_step).min(last_start);
    let mut refined: Option<(usize, f64)> = None;
    for start in refine_from..=refine_to {
        let window: String = hay[start..start + needle_len].iter().collect();
        let score = similarity(&window, needle)?;
        if refined.is_none_or(|(_, best_score)| score > best_score) {
            refined = Some((start, score));
        }
    }
    refined.map(|(start, score)| (start, start + needle_len, score))
}
