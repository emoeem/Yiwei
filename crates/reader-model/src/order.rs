//! Reading-order comparison for progress merging (design report §21.5).
//!
//! The "furthest position" of a book must never move backwards because an old
//! device synced late, so it is merged by reading order rather than by
//! timestamp. Ordering is derived from the CFI (`/6/<even>` gives the spine
//! index) and the character offset; when a locator has no usable CFI we fall
//! back to `progression`, and if that is missing too the two locators are
//! reported as [`ReadingOrder::Incomparable`] instead of guessing.

use std::cmp::Ordering;

use crate::locator::Locator;

/// Relative order of two positions inside one edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingOrder {
    /// `a` comes before `b`.
    Before,
    /// Both locators address the same position.
    Same,
    /// `a` comes after `b`.
    After,
    /// The two positions cannot be compared (different editions, or no usable
    /// ordering evidence); callers must not merge them by position.
    Incomparable,
}

/// Compare two locators by reading order.
pub fn compare_reading_order(a: &Locator, b: &Locator) -> ReadingOrder {
    if let (Some(left), Some(right)) = (a.edition_id.as_deref(), b.edition_id.as_deref())
        && left != right
    {
        return ReadingOrder::Incomparable;
    }

    match (reading_key(a), reading_key(b)) {
        (Some(left), Some(right)) => match left.cmp(&right) {
            Ordering::Less => ReadingOrder::Before,
            Ordering::Greater => ReadingOrder::After,
            Ordering::Equal => compare_progression(a, b),
        },
        _ => match (a.progression, b.progression) {
            (Some(left), Some(right)) => compare_floats(left, right),
            _ => ReadingOrder::Incomparable,
        },
    }
}

/// Absolute position inside the document stream, when derivable.
fn reading_key(locator: &Locator) -> Option<(u32, i64)> {
    let cfi = locator.parsed_cfi()?.ok()?;
    let spine = cfi.spine_index()?;
    Some((spine, cfi.text_offset().unwrap_or(0)))
}

fn compare_progression(a: &Locator, b: &Locator) -> ReadingOrder {
    match (a.progression, b.progression) {
        (Some(left), Some(right)) => compare_floats(left, right),
        _ => ReadingOrder::Same,
    }
}

fn compare_floats(left: f64, right: f64) -> ReadingOrder {
    match left.partial_cmp(&right) {
        Some(Ordering::Less) => ReadingOrder::Before,
        Some(Ordering::Greater) => ReadingOrder::After,
        Some(Ordering::Equal) => ReadingOrder::Same,
        None => ReadingOrder::Incomparable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locator(spine: u32, offset: i64, progression: f64) -> Locator {
        let itemref = spine * 2 + 2;
        Locator {
            book_id: "book".into(),
            edition_id: Some("ed".into()),
            cfi: Some(format!("epubcfi(/6/{itemref}!/4/2:{offset})")),
            progression: Some(progression),
            ..Locator::default()
        }
    }

    #[test]
    fn orders_positions_inside_one_section() {
        let a = locator(3, 10, 0.1);
        let b = locator(3, 90, 0.2);
        assert_eq!(compare_reading_order(&a, &b), ReadingOrder::Before);
        assert_eq!(compare_reading_order(&b, &a), ReadingOrder::After);
        assert_eq!(compare_reading_order(&a, &a), ReadingOrder::Same);
    }

    #[test]
    fn orders_across_sections() {
        let earlier = locator(2, 500, 0.9);
        let later = locator(3, 0, 0.1);
        assert_eq!(
            compare_reading_order(&earlier, &later),
            ReadingOrder::Before,
            "spine order dominates offsets"
        );
    }

    #[test]
    fn different_editions_are_incomparable() {
        let mut a = locator(1, 0, 0.5);
        let mut b = locator(1, 0, 0.5);
        a.edition_id = Some("original".into());
        b.edition_id = Some("translated".into());
        assert_eq!(compare_reading_order(&a, &b), ReadingOrder::Incomparable);
    }

    #[test]
    fn falls_back_to_progression_without_cfi() {
        let mut a = locator(1, 0, 0.3);
        let mut b = locator(1, 0, 0.7);
        a.cfi = None;
        b.cfi = None;
        assert_eq!(compare_reading_order(&a, &b), ReadingOrder::Before);

        let mut c = locator(1, 0, 0.2);
        c.cfi = None;
        c.progression = None;
        assert_eq!(compare_reading_order(&c, &b), ReadingOrder::Incomparable);
    }

    #[test]
    fn equal_structural_position_uses_progression_breakdown() {
        let a = locator(4, 12, 0.4);
        let b = locator(4, 12, 0.6);
        assert_eq!(compare_reading_order(&a, &b), ReadingOrder::Before);
        assert_eq!(compare_reading_order(&a, &a), ReadingOrder::Same);
    }
}
