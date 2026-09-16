//! Note and reading-progress merge rules (design report §21.4, §21.5).
//!
//! Notes are merged as text: `base`/`local`/`remote` are compared line by line
//! and non-overlapping edits are combined. Anything that cannot be merged
//! automatically is reported as a conflict so that the caller can create a
//! conflict copy — no side is ever silently discarded.
//!
//! Progress is merged in two parts: the *current* position is last-write-wins,
//! while the *furthest* position follows reading order so that a late sync from
//! an old device can never move the reader backwards.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use reader_model::{Locator, ReadingOrder, compare_reading_order};
use serde::{Deserialize, Serialize};

use crate::hlc::Hlc;

/// Maximum number of lines per side for an automatic three-way merge.
pub const MAX_MERGE_LINES: usize = 4_000;

/// Maximum LCS table size (lines × lines) for an automatic three-way merge.
pub const MAX_MERGE_CELLS: usize = 4_000_000;

/// Which side (or what combination) produced the merged text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeSource {
    /// Both sides had the same text.
    Identical,
    /// The local side had not changed since `base`.
    Remote,
    /// The remote side had not changed since `base`.
    Local,
    /// Both sides changed and the edits combined cleanly.
    ThreeWay,
}

/// Outcome of merging one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteMerge {
    /// The note merged deterministically.
    Merged {
        /// Merged text.
        content: String,
        /// How the merge was reached.
        source: MergeSource,
    },
    /// Both sides edited the same region; the caller must create a conflict copy.
    Conflict {
        /// Common ancestor text.
        base: String,
        /// Local text.
        local: String,
        /// Remote text.
        remote: String,
    },
    /// Merging was refused because the input exceeds the supported size.
    Unmergeable {
        /// Human readable reason, shown to the user instead of faking success.
        reason: String,
        /// Common ancestor text.
        base: String,
        /// Local text.
        local: String,
        /// Remote text.
        remote: String,
    },
}

/// Result of a raw line-based three-way merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeWayMerge {
    /// Merged text.
    Merged(String),
    /// Overlapping edits.
    Conflict,
    /// Input too large for the automatic merge.
    TooLarge {
        /// Line count of the base text.
        base_lines: usize,
        /// Line count of the local text.
        local_lines: usize,
        /// Line count of the remote text.
        remote_lines: usize,
    },
}

/// Merge a note written concurrently on two devices.
pub fn merge_note(base: &str, local: &str, remote: &str) -> NoteMerge {
    match three_way_merge(base, local, remote) {
        ThreeWayMerge::Merged(content) => {
            let source = if content == local && content == remote {
                MergeSource::Identical
            } else if content == local {
                MergeSource::Local
            } else if content == remote {
                MergeSource::Remote
            } else {
                MergeSource::ThreeWay
            };
            NoteMerge::Merged { content, source }
        }
        ThreeWayMerge::Conflict => NoteMerge::Conflict {
            base: base.to_string(),
            local: local.to_string(),
            remote: remote.to_string(),
        },
        ThreeWayMerge::TooLarge {
            base_lines,
            local_lines,
            remote_lines,
        } => NoteMerge::Unmergeable {
            reason: format!(
                "note too large for automatic merge ({base_lines}/{local_lines}/{remote_lines} lines, limit {MAX_MERGE_LINES})"
            ),
            base: base.to_string(),
            local: local.to_string(),
            remote: remote.to_string(),
        },
    }
}

/// Line-based three-way merge of `base`, `local` and `remote`.
pub fn three_way_merge(base: &str, local: &str, remote: &str) -> ThreeWayMerge {
    if local == remote {
        return ThreeWayMerge::Merged(local.to_string());
    }
    if local == base {
        return ThreeWayMerge::Merged(remote.to_string());
    }
    if remote == base {
        return ThreeWayMerge::Merged(local.to_string());
    }

    let base_lines = split_lines(base);
    let local_lines = split_lines(local);
    let remote_lines = split_lines(remote);
    let too_large = ThreeWayMerge::TooLarge {
        base_lines: base_lines.len(),
        local_lines: local_lines.len(),
        remote_lines: remote_lines.len(),
    };
    if base_lines.len() > MAX_MERGE_LINES
        || local_lines.len() > MAX_MERGE_LINES
        || remote_lines.len() > MAX_MERGE_LINES
    {
        return too_large;
    }

    let Some(local_hunks) = diff_hunks(&base_lines, &local_lines) else {
        return too_large;
    };
    let Some(remote_hunks) = diff_hunks(&base_lines, &remote_lines) else {
        return too_large;
    };

    match combine_hunks(&base_lines, &local_hunks, &remote_hunks) {
        Some(lines) => ThreeWayMerge::Merged(join_lines(&lines)),
        None => ThreeWayMerge::Conflict,
    }
}

/// A replacement of `base[base_start..base_end]` with new lines.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk {
    base_start: usize,
    base_end: usize,
    replacement: Vec<String>,
}

fn split_lines(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_string).collect()
}

fn join_lines(lines: &[String]) -> String {
    lines.join("\n")
}

/// Longest-common-subsequence diff, returning `None` when the input is too
/// large to diff within the memory budget.
fn diff_hunks(base: &[String], other: &[String]) -> Option<Vec<Hunk>> {
    let rows = base.len();
    let cols = other.len();
    if rows.saturating_mul(cols) > MAX_MERGE_CELLS {
        return None;
    }

    let width = cols + 1;
    let mut lcs = vec![0u32; (rows + 1) * width];
    for i in (0..rows).rev() {
        for j in (0..cols).rev() {
            lcs[i * width + j] = if base[i] == other[j] {
                lcs[(i + 1) * width + (j + 1)] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + (j + 1)])
            };
        }
    }

    let mut hunks = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < rows || j < cols {
        if i < rows && j < cols && base[i] == other[j] {
            i += 1;
            j += 1;
            continue;
        }
        let base_start = i;
        let replacement_start = j;
        while i < rows || j < cols {
            if i < rows && j < cols && base[i] == other[j] {
                break;
            }
            let skip_base = if i < rows {
                lcs[(i + 1) * width + j]
            } else {
                0
            };
            let take_other = if j < cols { lcs[i * width + (j + 1)] } else { 0 };
            if j < cols && (i >= rows || take_other >= skip_base) {
                j += 1;
            } else {
                i += 1;
            }
        }
        hunks.push(Hunk {
            base_start,
            base_end: i,
            replacement: other[replacement_start..j].to_vec(),
        });
    }
    Some(hunks)
}

/// Combine both sides' hunks; `None` means a genuine conflict.
fn combine_hunks(base: &[String], local: &[Hunk], remote: &[Hunk]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut local_index = 0usize;
    let mut remote_index = 0usize;
    let mut position = 0usize;

    while local_index < local.len() || remote_index < remote.len() {
        let next_local = local.get(local_index).map(|h| h.base_start);
        let next_remote = remote.get(remote_index).map(|h| h.base_start);
        let start = match (next_local, next_remote) {
            (Some(left), Some(right)) => left.min(right),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => break,
        };
        if start > position {
            out.extend_from_slice(&base[position..start]);
        }

        // Grow the region so that overlapping hunks from either side are merged
        // as one unit instead of being interleaved incorrectly.
        let mut end = start;
        let mut local_group: Vec<&Hunk> = Vec::new();
        let mut remote_group: Vec<&Hunk> = Vec::new();
        loop {
            let mut advanced = false;
            if let Some(hunk) = local.get(local_index)
                && hunk.base_start <= end
            {
                end = end.max(hunk.base_end);
                local_group.push(hunk);
                local_index += 1;
                advanced = true;
            }
            if let Some(hunk) = remote.get(remote_index)
                && hunk.base_start <= end
            {
                end = end.max(hunk.base_end);
                remote_group.push(hunk);
                remote_index += 1;
                advanced = true;
            }
            if !advanced {
                break;
            }
        }

        match (local_group.is_empty(), remote_group.is_empty()) {
            (false, false) => {
                let local_text = apply_hunks(base, &local_group, start, end);
                let remote_text = apply_hunks(base, &remote_group, start, end);
                if local_text == remote_text {
                    out.extend(local_text);
                } else {
                    return None;
                }
            }
            (false, true) => out.extend(apply_hunks(base, &local_group, start, end)),
            (true, false) => out.extend(apply_hunks(base, &remote_group, start, end)),
            (true, true) => return None,
        }
        position = end;
    }

    out.extend_from_slice(&base[position..]);
    Some(out)
}

fn apply_hunks(base: &[String], hunks: &[&Hunk], start: usize, end: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut position = start;
    for hunk in hunks {
        if hunk.base_start > position {
            out.extend_from_slice(&base[position..hunk.base_start]);
        }
        out.extend_from_slice(&hunk.replacement);
        position = hunk.base_end;
    }
    if position < end {
        out.extend_from_slice(&base[position..end]);
    }
    out
}

/// A device's reading progress for one book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressState {
    /// Where the reader currently is.
    pub current: Locator,
    /// Clock of the current position.
    pub current_hlc: Hlc,
    /// The furthest position reached.
    pub furthest: Locator,
    /// Clock of the furthest position.
    pub furthest_hlc: Hlc,
}

/// Which progress value a warning refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressField {
    /// The current position.
    Current,
    /// The furthest position.
    Furthest,
}

/// Something the UI has to surface after a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressWarning {
    /// The kept locator could not be validated and needs re-anchoring (§21.5).
    NeedsReanchor {
        /// Which value.
        field: ProgressField,
        /// Device the locator came from.
        device: String,
        /// Why validation failed.
        reason: String,
    },
    /// The two furthest positions belong to different editions or carry no
    /// usable ordering evidence; the newer clock was kept.
    IncomparableFurthest {
        /// Device whose value was kept.
        kept: String,
        /// Device whose value was discarded.
        discarded: String,
    },
}

/// Result of merging two devices' progress.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressMerge {
    /// The merged progress.
    pub merged: ProgressState,
    /// Warnings to store and show to the user.
    pub warnings: Vec<ProgressWarning>,
}

/// Merge two devices' progress for the same book (§21.5).
pub fn merge_progress(local: &ProgressState, remote: &ProgressState) -> ProgressMerge {
    let mut warnings = Vec::new();

    let current = pick_by_clock(
        (&local.current, &local.current_hlc),
        (&remote.current, &remote.current_hlc),
    );
    let (furthest, furthest_hlc) = match compare_reading_order(&local.furthest, &remote.furthest) {
        ReadingOrder::Before => (&remote.furthest, &remote.furthest_hlc),
        ReadingOrder::After => (&local.furthest, &local.furthest_hlc),
        ReadingOrder::Same => pick_by_clock(
            (&local.furthest, &local.furthest_hlc),
            (&remote.furthest, &remote.furthest_hlc),
        ),
        ReadingOrder::Incomparable => {
            let (kept, hlc) = pick_by_clock(
                (&local.furthest, &local.furthest_hlc),
                (&remote.furthest, &remote.furthest_hlc),
            );
            let discarded = if std::ptr::eq(hlc, &local.furthest_hlc) {
                &remote.furthest_hlc
            } else {
                &local.furthest_hlc
            };
            warnings.push(ProgressWarning::IncomparableFurthest {
                kept: hlc.device().to_string(),
                discarded: discarded.device().to_string(),
            });
            (kept, hlc)
        }
    };

    for (field, locator, hlc) in [
        (ProgressField::Current, current.0, current.1),
        (ProgressField::Furthest, furthest, furthest_hlc),
    ] {
        if let Err(error) = locator.validate() {
            warnings.push(ProgressWarning::NeedsReanchor {
                field,
                device: hlc.device().to_string(),
                reason: error.to_string(),
            });
        }
    }

    ProgressMerge {
        merged: ProgressState {
            current: current.0.clone(),
            current_hlc: current.1.clone(),
            furthest: furthest.clone(),
            furthest_hlc: furthest_hlc.clone(),
        },
        warnings,
    }
}

/// Pick the value with the greater clock, breaking exact ties by content so the
/// result does not depend on argument order.
fn pick_by_clock<'a>(
    left: (&'a Locator, &'a Hlc),
    right: (&'a Locator, &'a Hlc),
) -> (&'a Locator, &'a Hlc) {
    let ordering = left
        .1
        .cmp(right.1)
        .then_with(|| locator_sort_key(left.0).cmp(&locator_sort_key(right.0)));
    if ordering == Ordering::Greater {
        left
    } else {
        right
    }
}

fn locator_sort_key(locator: &Locator) -> String {
    serde_json::to_string(locator).unwrap_or_default()
}

/// Per-chapter progress entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterProgress {
    /// Where the reader is inside this chapter.
    pub locator: Locator,
    /// Clock of the entry.
    pub hlc: Hlc,
}

/// Merge per-chapter progress maps (§21.5: chapters merge independently).
pub fn merge_chapter_progress(
    local: &BTreeMap<String, ChapterProgress>,
    remote: &BTreeMap<String, ChapterProgress>,
) -> BTreeMap<String, ChapterProgress> {
    let mut merged = local.clone();
    for (key, remote_entry) in remote {
        match merged.get(key) {
            Some(local_entry) => {
                let (locator, hlc) = pick_by_clock(
                    (&local_entry.locator, &local_entry.hlc),
                    (&remote_entry.locator, &remote_entry.hlc),
                );
                merged.insert(
                    key.clone(),
                    ChapterProgress {
                        locator: locator.clone(),
                        hlc: hlc.clone(),
                    },
                );
            }
            None => {
                merged.insert(key.clone(), remote_entry.clone());
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::DeviceId;

    const BASE: &str = "line 1\nline 2\nline 3\nline 4";

    #[test]
    fn unchanged_side_is_taken_directly() {
        assert_eq!(
            merge_note(BASE, BASE, "changed"),
            NoteMerge::Merged {
                content: "changed".into(),
                source: MergeSource::Remote
            }
        );
        assert_eq!(
            merge_note(BASE, "changed", BASE),
            NoteMerge::Merged {
                content: "changed".into(),
                source: MergeSource::Local
            }
        );
        assert_eq!(
            merge_note(BASE, "same", "same"),
            NoteMerge::Merged {
                content: "same".into(),
                source: MergeSource::Identical
            }
        );
    }

    #[test]
    fn non_overlapping_edits_are_combined() {
        let local = "local 1\nline 2\nline 3\nline 4";
        let remote = "line 1\nline 2\nline 3\nremote 4";
        assert_eq!(
            merge_note(BASE, local, remote),
            NoteMerge::Merged {
                content: "local 1\nline 2\nline 3\nremote 4".into(),
                source: MergeSource::ThreeWay
            }
        );
    }

    #[test]
    fn identical_edits_merge_once() {
        let edited = "line 1\nshared edit\nline 3\nline 4";
        assert_eq!(
            merge_note(BASE, edited, edited),
            NoteMerge::Merged {
                content: edited.into(),
                source: MergeSource::Identical
            }
        );
    }

    #[test]
    fn overlapping_edits_conflict_and_keep_both_sides() {
        let local = "line 1\nlocal 2\nline 3\nline 4";
        let remote = "line 1\nremote 2\nline 3\nline 4";
        assert_eq!(
            merge_note(BASE, local, remote),
            NoteMerge::Conflict {
                base: BASE.into(),
                local: local.into(),
                remote: remote.into()
            }
        );
    }

    #[test]
    fn deletions_and_insertions_merge() {
        let local = "line 1\nline 3\nline 4"; // removed line 2
        let remote = "line 1\nline 2\nline 3\nline 4\nline 5"; // appended
        assert_eq!(
            merge_note(BASE, local, remote),
            NoteMerge::Merged {
                content: "line 1\nline 3\nline 4\nline 5".into(),
                source: MergeSource::ThreeWay
            }
        );
    }

    #[test]
    fn conflict_is_detected_when_one_side_edits_deleted_text() {
        let local = "line 1\nline 3"; // deleted line 2
        let remote = "line 1\nrewritten 2\nline 3\nline 4";
        assert!(matches!(
            merge_note(BASE, local, remote),
            NoteMerge::Conflict { .. }
        ));
    }

    #[test]
    fn oversized_notes_are_reported_as_unmergeable() {
        let big: String = (0..MAX_MERGE_LINES + 10)
            .map(|index| format!("line {index}\n"))
            .collect();
        let local = format!("{big}local");
        let remote = format!("{big}remote");
        assert!(matches!(
            merge_note(&big, &local, &remote),
            NoteMerge::Unmergeable { .. }
        ));
    }

    #[test]
    fn trailing_newlines_are_preserved() {
        let base = "a\nb\n";
        let local = "a\nb\n";
        let remote = "a\nc\n";
        assert_eq!(
            merge_note(base, local, remote),
            NoteMerge::Merged {
                content: "a\nc\n".into(),
                source: MergeSource::Remote
            }
        );
    }

    fn device(name: &str) -> DeviceId {
        DeviceId::parse(name).expect("device")
    }

    fn hlc(physical_ms: u64, dev: &str) -> Hlc {
        Hlc::new(physical_ms, 0, device(dev))
    }

    fn locator(spine: u32, progression: f64) -> Locator {
        let itemref = spine * 2 + 2;
        Locator {
            book_id: "book".into(),
            edition_id: Some("ed".into()),
            cfi: Some(format!("epubcfi(/6/{itemref}!/4/2:0)")),
            progression: Some(progression),
            ..Locator::default()
        }
    }

    fn progress(spine: u32, current_ms: u64, furthest_spine: u32, dev: &str) -> ProgressState {
        ProgressState {
            current: locator(spine, f64::from(spine) / 100.0),
            current_hlc: hlc(current_ms, dev),
            furthest: locator(furthest_spine, f64::from(furthest_spine) / 100.0),
            furthest_hlc: hlc(current_ms, dev),
        }
    }

    #[test]
    fn current_position_is_last_write_wins() {
        let local = progress(1, 100, 2, "dev-a");
        let remote = progress(5, 200, 5, "dev-b");
        let merged = merge_progress(&local, &remote);
        assert_eq!(merged.merged.current, locator(5, 0.05));
        assert_eq!(merged.merged.current_hlc, hlc(200, "dev-b"));
        assert!(merged.warnings.is_empty());
    }

    #[test]
    fn furthest_position_never_moves_backwards() {
        // The local device synced later but read less far.
        let local = progress(2, 999, 3, "dev-a");
        let remote = progress(1, 100, 20, "dev-b");
        let merged = merge_progress(&local, &remote);
        assert_eq!(
            merged.merged.furthest,
            locator(20, 0.2),
            "furthest progress follows reading order, not timestamps"
        );
        assert_eq!(merged.merged.current, locator(2, 0.02));
    }

    #[test]
    fn merge_progress_is_commutative() {
        let local = progress(2, 999, 3, "dev-a");
        let remote = progress(1, 100, 20, "dev-b");
        assert_eq!(
            merge_progress(&local, &remote).merged,
            merge_progress(&remote, &local).merged
        );
    }

    #[test]
    fn warns_when_the_kept_locator_is_not_resolvable() {
        let mut local = progress(1, 100, 1, "dev-a");
        local.current.cfi = Some("epubcfi(/6/4[broken)".into());
        let remote = progress(2, 50, 2, "dev-b");
        let merged = merge_progress(&local, &remote);
        assert!(matches!(
            merged.warnings.as_slice(),
            [ProgressWarning::NeedsReanchor {
                field: ProgressField::Current,
                ..
            }]
        ));
    }

    #[test]
    fn warns_when_furthest_positions_are_incomparable() {
        let local = progress(3, 100, 30, "dev-a");
        let mut remote = progress(1, 200, 1, "dev-b");
        remote.current.edition_id = Some("translated".into());
        remote.furthest.edition_id = Some("translated".into());

        let merged = merge_progress(&local, &remote);
        assert!(merged.warnings.iter().any(|warning| matches!(
            warning,
            ProgressWarning::IncomparableFurthest { .. }
        )));
        assert_eq!(
            merged.merged.furthest_hlc,
            hlc(200, "dev-b"),
            "the newer clock is kept, with a warning"
        );
    }

    #[test]
    fn chapter_progress_merges_per_chapter() {
        let mut local = BTreeMap::new();
        local.insert(
            "chapter-1".to_string(),
            ChapterProgress {
                locator: locator(1, 0.9),
                hlc: hlc(500, "dev-a"),
            },
        );
        local.insert(
            "chapter-2".to_string(),
            ChapterProgress {
                locator: locator(2, 0.1),
                hlc: hlc(100, "dev-a"),
            },
        );

        let mut remote = BTreeMap::new();
        remote.insert(
            "chapter-1".to_string(),
            ChapterProgress {
                locator: locator(1, 0.2),
                hlc: hlc(300, "dev-b"),
            },
        );
        remote.insert(
            "chapter-3".to_string(),
            ChapterProgress {
                locator: locator(3, 0.5),
                hlc: hlc(400, "dev-b"),
            },
        );

        let merged = merge_chapter_progress(&local, &remote);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged["chapter-1"].hlc, hlc(500, "dev-a"));
        assert_eq!(merged["chapter-2"].hlc, hlc(100, "dev-a"));
        assert_eq!(merged["chapter-3"].hlc, hlc(400, "dev-b"));
        assert_eq!(
            merged,
            merge_chapter_progress(&remote, &local),
            "chapter merge is order independent"
        );
    }
}
