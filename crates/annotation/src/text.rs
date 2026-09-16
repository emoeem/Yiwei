//! Text normalization and similarity used by the relocation pipeline (§20.3).
//!
//! Comparison happens on a normalized form (collapsed whitespace, folded
//! punctuation, no zero-width characters) so that a re-typeset chapter with
//! different spacing still matches. Offsets reported back to the caller are
//! *character* offsets into the original text, and [`normalize_with_map`]
//! keeps the correspondence between the two.
//!
//! Unicode composition differences (NFC versus NFD) are deliberately *not*
//! rewritten: rewriting them would break the offset mapping. They are absorbed
//! by the similarity comparison instead.

/// Maximum number of characters compared by the fuzzy matcher.
pub const MAX_FUZZY_TEXT_CHARS: usize = 8_192;

/// Maximum length of the quoted text compared by the fuzzy matcher.
pub const MAX_FUZZY_NEEDLE_CHARS: usize = 512;

/// Normalize text for comparison without changing its meaning.
pub fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if is_invisible(ch) {
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(fold_punctuation(ch));
    }
    out
}

fn is_invisible(ch: char) -> bool {
    matches!(ch, '\u{200b}'..='\u{200f}' | '\u{feff}')
}

/// Map visually equivalent punctuation onto one form so that a chapter
/// re-typeset with different quotes still matches.
fn fold_punctuation(ch: char) -> char {
    match ch {
        '\u{2018}' | '\u{2019}' | '\u{201b}' => '\'',
        '\u{201c}' | '\u{201d}' | '\u{201f}' => '"',
        '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
        '\u{2026}' => '.',
        '\u{3000}' => ' ',
        other => other,
    }
}

/// Character offset of every normalized character in the original text.
///
/// `normalize_with_map(text).0` is the normalized string and `.1[start..end]`
/// gives the original character offsets covered by a normalized range.
pub fn normalize_with_map(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map: Vec<usize> = Vec::with_capacity(text.len());
    let mut pending_space = false;

    for (index, ch) in text.chars().enumerate() {
        if is_invisible(ch) {
            continue;
        }
        if ch.is_whitespace() {
            if !out.is_empty() {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            out.push(' ');
            map.push(index);
            pending_space = false;
        }
        out.push(fold_punctuation(ch));
        map.push(index);
    }
    (out, map)
}

/// Similarity ratio in `0.0..=1.0` based on character-level edit distance.
///
/// Returns `None` when the inputs exceed the fuzzy matching budget, so callers
/// can report "could not compare" instead of pretending a match.
pub fn similarity(left: &str, right: &str) -> Option<f64> {
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    if left_chars.len() > MAX_FUZZY_TEXT_CHARS || right_chars.len() > MAX_FUZZY_NEEDLE_CHARS {
        return None;
    }
    let distance = levenshtein(&left_chars, &right_chars);
    let longest = left_chars.len().max(right_chars.len());
    if longest == 0 {
        return Some(1.0);
    }
    Some(1.0 - (distance as f64 / longest as f64))
}

/// Levenshtein distance with two rolling rows.
pub fn levenshtein(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (i, left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_collapses_whitespace_and_folds_punctuation() {
        assert_eq!(normalize_text("  hello   world \n"), "hello world");
        assert_eq!(normalize_text("“quoted”"), "\"quoted\"");
        assert_eq!(normalize_text("café"), "café");
        assert_eq!(normalize_text("a\u{200b}b"), "ab");
    }

    #[test]
    fn normalization_map_points_back_into_the_original() {
        let text = "  a  b ";
        let (normalized, map) = normalize_with_map(text);
        assert_eq!(normalized, "a b");
        assert_eq!(map.len(), normalized.chars().count());
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(chars[map[0]], 'a');
        assert_eq!(chars[map[2]], 'b');
    }

    #[test]
    fn similarity_is_one_for_equal_text_and_zero_for_disjoint() {
        assert_eq!(similarity("abcd", "abcd"), Some(1.0));
        assert_eq!(similarity("abcd", ""), Some(0.0));
        let near = similarity("hello world", "hello wrold").expect("similar");
        assert!(near > 0.8 && near < 1.0, "got {near}");
    }

    #[test]
    fn oversized_input_is_reported_rather_than_compared() {
        let long: String = std::iter::repeat_n('x', MAX_FUZZY_TEXT_CHARS + 1).collect();
        assert_eq!(similarity(&long, "x"), None);
    }
}
