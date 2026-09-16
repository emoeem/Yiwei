//! Tests for the relocation pipeline and the anchor merge rules.

use reader_model::{Locator, TextQuote};

use crate::merge::{AnchorMergeRule, merge_anchor};
use crate::relocate::{
    BlockText, RelocationStep, RelocationWarning, ResolvedPosition, SectionText, TextIndex, relocate,
};
use crate::types::{AnchorScheme, AnchorState, AnnotationAnchor, AnnotationType};

fn block(id: &str, ordinal: u32, text: &str) -> BlockText {
    BlockText {
        block_id: id.to_string(),
        ordinal,
        text: text.to_string(),
    }
}

fn section(id: &str, spine: u32, fingerprint: &str, blocks: Vec<BlockText>) -> SectionText {
    SectionText {
        section_id: id.to_string(),
        spine_index: Some(spine),
        doc_fingerprint: Some(fingerprint.to_string()),
        blocks,
    }
}

struct AnchorSpec<'a> {
    section_id: &'a str,
    spine: u32,
    cfi: Option<&'a str>,
    block_id: &'a str,
    offsets: Option<(u32, u32)>,
    exact: &'a str,
    prefix: &'a str,
    suffix: &'a str,
    fingerprint: &'a str,
    confidence: f64,
    state: AnchorState,
    updated_at: Option<&'a str>,
}

impl<'a> AnchorSpec<'a> {
    fn new(section_id: &'a str, spine: u32, block_id: &'a str, exact: &'a str) -> Self {
        Self {
            section_id,
            spine,
            cfi: None,
            block_id,
            offsets: None,
            exact,
            prefix: "",
            suffix: "",
            fingerprint: "fp-1",
            confidence: 1.0,
            state: AnchorState::Anchored,
            updated_at: None,
        }
    }

    fn build(&self) -> AnnotationAnchor {
        let itemref = self.spine * 2 + 2;
        let cfi = self
            .cfi
            .map(str::to_string)
            .or_else(|| Some(format!("epubcfi(/6/{itemref}!/4/2:0)")));
        let locator = Locator {
            book_id: "book".into(),
            edition_id: Some("edition".into()),
            section_id: Some(self.section_id.to_string()),
            cfi,
            block_id: Some(self.block_id.to_string()),
            start_offset: self.offsets.map(|(start, _)| start),
            end_offset: self.offsets.map(|(_, end)| end),
            ..Locator::default()
        };
        let mut anchor = AnnotationAnchor::new(AnchorScheme::EpubCfi, locator);
        anchor.section_id = Some(self.section_id.to_string());
        anchor.block_id = Some(self.block_id.to_string());
        anchor.start_offset = self.offsets.map(|(start, _)| start);
        anchor.end_offset = self.offsets.map(|(_, end)| end);
        anchor.doc_fingerprint = Some(self.fingerprint.to_string());
        anchor.quote = Some(TextQuote::new(self.exact, self.prefix, self.suffix));
        anchor.confidence = self.confidence;
        anchor.state = self.state;
        anchor.updated_at = self.updated_at.map(str::to_string);
        anchor
    }
}

fn single_section_index(text: &str) -> TextIndex {
    TextIndex::new([section("s1", 0, "fp-1", vec![block("b1", 0, text)])])
}

/// Set the character offsets of a built anchor.
fn set_offsets(anchor: &mut AnnotationAnchor, start: u32, end: u32) {
    anchor.start_offset = Some(start);
    anchor.end_offset = Some(end);
    anchor.primary_locator.start_offset = Some(start);
    anchor.primary_locator.end_offset = Some(end);
}

/// Replace the text quote of a built anchor.
fn set_quote(anchor: &mut AnnotationAnchor, exact: &str, prefix: &str, suffix: &str) {
    anchor.quote = Some(TextQuote::new(exact, prefix, suffix));
}

#[test]
fn step_one_keeps_an_unchanged_cfi_anchor() {
    let index = single_section_index("Hello brave new world.");
    let anchor = AnchorSpec::new("s1", 0, "b1", "brave").build();
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::ExactCfi);
    assert_eq!(result.anchor.confidence, 1.0);
    assert_eq!(result.anchor.state, AnchorState::Anchored);
    assert_eq!(
        result.resolved,
        Some(ResolvedPosition {
            section_id: "s1".into(),
            block_id: Some("b1".into()),
            start: 6,
            end: 11,
        })
    );
    assert!(result.warnings.is_empty());
}

#[test]
fn step_two_uses_the_structure_when_the_fingerprint_changed() {
    let index = single_section_index("Hello brave new world.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave").build();
    anchor.doc_fingerprint = Some("outdated-fingerprint".into());
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::NormalizedCfi);
    assert_eq!(result.anchor.confidence, 0.95);
    assert!(result.anchor.is_usable());
}

#[test]
fn step_three_uses_block_offsets_when_there_is_no_cfi() {
    let index = single_section_index("Hello brave new world.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 6, 11);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::BlockOffset);
    assert_eq!(result.anchor.confidence, 0.92);
    assert_eq!(result.anchor.state, AnchorState::Anchored);
}

#[test]
fn block_offsets_pointing_at_different_text_fall_through_to_the_quote() {
    let index = single_section_index("Hello brave new world.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 0, 5);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::TextQuote);
    assert_eq!(result.resolved.expect("resolved").start, 6);
    assert!(result.anchor.confidence >= 0.9);
}

#[test]
fn step_four_finds_a_quote_that_moved_to_another_block() {
    let index = TextIndex::new([section(
        "s1",
        0,
        "fp-1",
        vec![
            block("b1", 0, "Chapter one."),
            block("b2", 1, "The brave sailor returned home."),
        ],
    )]);
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave sailor").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 0, 4);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::TextQuote);
    let resolved = result.resolved.expect("resolved");
    assert_eq!(resolved.block_id, Some("b2".into()));
    assert_eq!(resolved.start, 4);
    assert_eq!(result.anchor.state, AnchorState::Anchored);
}

#[test]
fn repeated_text_is_disambiguated_by_context_and_needs_review() {
    let index = single_section_index("the cat sat. the cat ran. the cat slept.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "the cat").build();
    anchor.primary_locator.cfi = None;
    // The stored offsets point at stale text, so the pipeline must fall through
    // to the quote step and use the surrounding context to pick a candidate.
    set_offsets(&mut anchor, 0, 3);
    set_quote(&mut anchor, "the cat", "sat. ", " ran");
    let result = relocate(&anchor, &index);

    let resolved = result.resolved.expect("resolved");
    assert_eq!(resolved.start, 13, "context selects the second occurrence");
    assert_eq!(
        result.anchor.state,
        AnchorState::NeedsReview,
        "a repeated match must be confirmed by the user"
    );
    assert!(result.anchor.confidence >= 0.7 && result.anchor.confidence < 0.9);
    assert!(result.candidates.len() >= 3);
}

#[test]
fn whitespace_and_punctuation_changes_still_count_as_a_quote_match() {
    let index = single_section_index("Hello   brave\u{2019}s new world.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave's new").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 0, 5);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::TextQuote);
    let resolved = result.resolved.expect("resolved");
    assert_eq!(resolved.start, 8);
}

#[test]
fn step_five_reports_a_fuzzy_match_with_low_confidence() {
    let index = single_section_index("Hello brxve sailor of the new world.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave sailor").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 0, 5);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::FuzzyMatch);
    assert_eq!(result.anchor.state, AnchorState::Fuzzy);
    assert!(result.anchor.confidence >= 0.5 && result.anchor.confidence < 0.7);
}

#[test]
fn missing_text_leaves_the_annotation_orphaned() {
    let index = single_section_index("Completely different content.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave sailor").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 0, 5);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::Unresolved);
    assert_eq!(result.anchor.state, AnchorState::Orphaned);
    assert_eq!(result.anchor.confidence, 0.0);
    assert_eq!(result.resolved, None);
    assert_eq!(result.original.quote, anchor.quote, "the original is preserved");
}

#[test]
fn step_six_finds_the_text_in_another_section() {
    let index = TextIndex::new([
        section("s1", 0, "fp-1", vec![block("b1", 0, "Chapter one.")]),
        section("s2", 1, "fp-2", vec![block("b2", 0, "The brave sailor returned.")]),
    ]);
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave sailor").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 0, 5);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::GlobalSearch);
    assert_eq!(result.resolved.expect("resolved").section_id, "s2");
    assert!(matches!(
        result.warnings.as_slice(),
        [RelocationWarning::MovedToAnotherSection { .. }]
    ));
}

#[test]
fn oversized_blocks_skip_fuzzy_matching_with_a_warning() {
    let long_text: String = std::iter::repeat_n('x', crate::MAX_FUZZY_TEXT_CHARS + 10).collect();
    let index = single_section_index(&long_text);
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave sailor").build();
    anchor.primary_locator.cfi = None;
    set_offsets(&mut anchor, 0, 5);
    let result = relocate(&anchor, &index);

    assert_eq!(result.step, RelocationStep::Unresolved);
    assert!(matches!(
        result.warnings.as_slice(),
        [RelocationWarning::FuzzyComparisonSkipped { .. }]
    ));
}

#[test]
fn machine_origin_survives_relocation() {
    let index = single_section_index("Hello brave new world.");
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave").build();
    anchor.origin = crate::AnchorOrigin::MachineMigrated;
    let result = relocate(&anchor, &index);
    assert_eq!(result.anchor.origin, crate::AnchorOrigin::MachineMigrated);
}

fn merged_anchor(state: AnchorState, confidence: f64, updated_at: Option<&str>) -> AnnotationAnchor {
    let mut anchor = AnchorSpec::new("s1", 0, "b1", "brave").build();
    anchor.state = state;
    anchor.confidence = confidence;
    anchor.updated_at = updated_at.map(str::to_string);
    anchor
}

#[test]
fn both_anchored_keeps_the_more_confident_position() {
    let local = merged_anchor(AnchorState::Anchored, 0.95, Some("2026-09-14T10:00:00Z"));
    let mut remote = merged_anchor(AnchorState::Anchored, 0.99, Some("2026-09-15T10:00:00Z"));
    remote.quote = None;

    let merge = merge_anchor(&local, &remote);
    assert_eq!(merge.rule, AnchorMergeRule::BothAnchored);
    assert_eq!(merge.primary.confidence, 0.99);
    assert_eq!(
        merge.primary.quote, local.quote,
        "missing evidence is filled in from the other side"
    );
    assert!(merge.alternative.is_none());
    assert!(!merge.requires_review);
}

#[test]
fn anchored_side_wins_but_the_orphan_is_kept() {
    let anchored = merged_anchor(AnchorState::Anchored, 0.92, Some("2026-09-15T10:00:00Z"));
    let orphaned = merged_anchor(AnchorState::Orphaned, 0.0, Some("2026-09-16T10:00:00Z"));

    let merge = merge_anchor(&anchored, &orphaned);
    assert_eq!(merge.rule, AnchorMergeRule::AnchoredWins);
    assert_eq!(merge.primary.state, AnchorState::Anchored);
    assert_eq!(merge.alternative.expect("orphan kept").state, AnchorState::Orphaned);
    assert!(!merge.requires_review);

    let reversed = merge_anchor(&orphaned, &anchored);
    assert_eq!(reversed.primary.state, AnchorState::Anchored);
    assert!(reversed.alternative.is_some());
}

#[test]
fn both_orphaned_keeps_the_newer_one_and_asks_for_review() {
    let older = merged_anchor(AnchorState::Orphaned, 0.0, Some("2026-09-14T10:00:00Z"));
    let newer = merged_anchor(AnchorState::Orphaned, 0.0, Some("2026-09-16T10:00:00Z"));

    let merge = merge_anchor(&older, &newer);
    assert_eq!(merge.rule, AnchorMergeRule::BothOrphaned);
    assert_eq!(merge.primary.updated_at, newer.updated_at);
    assert!(merge.alternative.is_some());
    assert!(merge.requires_review);

    let reversed = merge_anchor(&newer, &older);
    assert_eq!(
        reversed.primary.updated_at, merge.primary.updated_at,
        "the rule is order independent"
    );
}

#[test]
fn annotation_types_and_anchor_states_round_trip_through_json() {
    let anchor = merged_anchor(AnchorState::NeedsReview, 0.8, None);
    let json = serde_json::to_value(&anchor).expect("serialize");
    assert_eq!(json["state"], serde_json::json!("needs_review"));
    assert_eq!(json["origin"], serde_json::json!("user"));
    let back: AnnotationAnchor = serde_json::from_value(json).expect("deserialize");
    assert_eq!(back, anchor);
    assert_eq!(
        serde_json::to_value(AnnotationType::Highlight).expect("serialize"),
        serde_json::json!("highlight")
    );
}
