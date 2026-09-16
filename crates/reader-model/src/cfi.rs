//! EPUB Canonical Fragment Identifiers (EPUB CFI 1.1).
//!
//! CFI is a *structural* locator: it depends on the DOM shape of the package
//! document and the content document. The design report (§10.5, §20.3) requires
//! that we never rely on CFI alone, so this module deliberately exposes only
//! what the anchoring pipeline needs:
//!
//! * parse / render (round-trip safe),
//! * [`Cfi::normalize`] and [`Cfi::structurally_equivalent`] so that cosmetically
//!   different spellings (`/4,,/20` vs `/4/20`) compare equal,
//! * [`Cfi::spine_index`] to order positions inside one edition.
//!
//! The parser is strict about malformed input: a CFI that cannot be understood
//! is reported as an error instead of being silently coerced into a wrong
//! location.

use std::fmt;

/// Failure modes when parsing a CFI string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CfiError {
    /// The string was not wrapped in `epubcfi(...)`.
    #[error("CFI must be wrapped in `epubcfi(...)`")]
    MissingWrapper,
    /// The wrapper contained no path.
    #[error("CFI contains no path")]
    Empty,
    /// A path step or offset was empty where a value was required.
    #[error("empty path component at offset {0}")]
    EmptyComponent(usize),
    /// An unexpected character was found.
    #[error("unexpected character {ch:?} at offset {pos}")]
    UnexpectedChar { ch: char, pos: usize },
    /// An assertion (`[...]`) was opened but never closed.
    #[error("unterminated assertion starting at offset {0}")]
    UnterminatedAssertion(usize),
    /// A numeric field could not be parsed.
    #[error("invalid number {text:?}")]
    InvalidNumber {
        /// The offending text.
        text: String,
    },
    /// More than three comma-separated paths were provided.
    #[error("expected at most a parent, start and end path in a CFI")]
    TooManyPaths,
    /// The same offset selector appeared twice.
    #[error("duplicate offset selector {0:?}")]
    DuplicateOffset(char),
    /// Input remained after the CFI path was complete.
    #[error("trailing input after CFI path: {rest:?}")]
    TrailingInput {
        /// The unparsed remainder.
        rest: String,
    },
}

/// Which side of a boundary a position refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Before the boundary.
    Before,
    /// After the boundary.
    After,
}

/// Offsets that refine a path position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Offset {
    /// Character offset inside the referenced text node.
    pub text: Option<i64>,
    /// Temporal offset (audio/video).
    pub temporal: Option<f64>,
    /// Horizontal spatial offset in the referenced rectangle.
    pub spatial_x: Option<f64>,
    /// Vertical spatial offset in the referenced rectangle.
    pub spatial_y: Option<f64>,
    /// Side bias for the offset.
    pub side: Option<Side>,
}

impl Offset {
    /// Whether any offset component is present.
    pub fn is_empty(&self) -> bool {
        self.text.is_none()
            && self.temporal.is_none()
            && self.spatial_x.is_none()
            && self.spatial_y.is_none()
            && self.side.is_none()
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(text) = self.text {
            write!(f, ":{text}")?;
        }
        if let Some(temporal) = self.temporal {
            write!(f, "~{}", format_number(temporal))?;
        }
        if let Some(x) = self.spatial_x {
            write!(f, "@{}", format_number(x))?;
            if let Some(y) = self.spatial_y {
                write!(f, ":{}", format_number(y))?;
            }
        } else if self.spatial_y.is_some() {
            // A vertical offset without a horizontal one has no meaning; the
            // parser cannot produce this state, but be explicit rather than
            // silently dropping the value.
            write!(f, "@:{}", format_number(self.spatial_y.unwrap_or_default()))?;
        }
        if let Some(side) = self.side {
            match side {
                Side::Before => write!(f, "@b")?,
                Side::After => write!(f, "@a")?,
            }
        }
        Ok(())
    }
}

/// One step in a CFI path: a child index plus an optional assertion.
///
/// A step with `index == None` is the "null step" spelling (`/4,,/20`), which
/// [`Cfi::normalize`] removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Zero-based child index, `None` for a null step.
    pub index: Option<u32>,
    /// Optional ID assertion (`[chap01]`), already unescaped.
    pub assertion: Option<String>,
}

impl Step {
    /// Whether this is a null step.
    pub fn is_null(&self) -> bool {
        self.index.is_none() && self.assertion.is_none()
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(index) = self.index {
            write!(f, "{index}")?;
        }
        if let Some(assertion) = &self.assertion {
            write!(f, "[{}]", escape_assertion(assertion))?;
        }
        Ok(())
    }
}

/// A `/`-separated run of steps with a trailing offset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathPart {
    /// Steps, in document order.
    pub steps: Vec<Step>,
    /// Offsets applied to the final step.
    pub offset: Offset,
}

impl fmt::Display for PathPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for step in &self.steps {
            write!(f, "/{step}")?;
        }
        write!(f, "{}", self.offset)
    }
}

/// One path of a CFI: `!`-separated parts (package document, then each document
/// reached through indirection).
#[derive(Debug, Clone, PartialEq)]
pub struct StepRef {
    /// Path parts in order.
    pub parts: Vec<PathPart>,
    /// Whether the path ends with a trailing `!` (used by parent references).
    pub open_indirection: bool,
}

impl StepRef {
    /// Build a step reference from a single part.
    pub fn from_part(part: PathPart) -> Self {
        Self {
            parts: vec![part],
            open_indirection: false,
        }
    }

    /// The offset of the deepest part.
    pub fn offset(&self) -> Offset {
        self.parts.last().map(|p| p.offset).unwrap_or_default()
    }
}

impl fmt::Display for StepRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, part) in self.parts.iter().enumerate() {
            if index > 0 {
                f.write_str("!")?;
            }
            write!(f, "{part}")?;
        }
        if self.open_indirection {
            f.write_str("!")?;
        }
        Ok(())
    }
}

/// A parsed CFI: an optional parent reference plus a single position or a range.
#[derive(Debug, Clone, PartialEq)]
pub struct Cfi {
    /// Parent reference for the range form.
    pub parent: Option<StepRef>,
    /// Start position.
    pub start: Option<StepRef>,
    /// End position for a range.
    pub end: Option<StepRef>,
}

impl Cfi {
    /// Parse a CFI string such as `epubcfi(/6/24!/4/20/1:58)`.
    pub fn parse(input: &str) -> Result<Self, CfiError> {
        let trimmed = input.trim();
        let inner = trimmed
            .strip_prefix("epubcfi(")
            .and_then(|rest| rest.strip_suffix(')'))
            .ok_or(CfiError::MissingWrapper)?;
        if inner.trim().is_empty() {
            return Err(CfiError::Empty);
        }

        let path_strings = split_paths(inner)?;
        let mut parsed = Vec::with_capacity(path_strings.len());
        for path in &path_strings {
            parsed.push(parse_step_ref(path)?);
        }

        match parsed.len() {
            1 => Ok(Cfi {
                parent: None,
                start: Some(parsed.remove(0)),
                end: None,
            }),
            2 => {
                let mut it = parsed.into_iter();
                Ok(Cfi {
                    parent: None,
                    start: it.next(),
                    end: it.next(),
                })
            }
            3 => {
                let mut it = parsed.into_iter();
                Ok(Cfi {
                    parent: it.next(),
                    start: it.next(),
                    end: it.next(),
                })
            }
            _ => Err(CfiError::TooManyPaths),
        }
    }

    /// Canonical spelling: null steps removed, offsets in canonical order.
    pub fn normalize(&self) -> Cfi {
        Cfi {
            parent: self.parent.as_ref().map(normalize_step_ref),
            start: self.start.as_ref().map(normalize_step_ref),
            end: self.end.as_ref().map(normalize_step_ref),
        }
    }

    /// Whether two CFIs address the same structure, ignoring assertions.
    ///
    /// This is the comparison used by the relocation pipeline (§20.3 step 2):
    /// `/4,,/20` and `/4/20` must be treated as the same position.
    pub fn structurally_equivalent(&self, other: &Cfi) -> bool {
        let left = self.normalize();
        let right = other.normalize();
        equivalent_ref(left.parent.as_ref(), right.parent.as_ref())
            && equivalent_ref(left.start.as_ref(), right.start.as_ref())
            && equivalent_ref(left.end.as_ref(), right.end.as_ref())
    }

    /// Spine index of the addressed document, when the CFI follows the standard
    /// package layout (`/6/<even itemref index>`).
    pub fn spine_index(&self) -> Option<u32> {
        let reference = self.start.as_ref().or(self.parent.as_ref())?;
        let steps = &reference.parts.first()?.steps;
        let spine = steps.first()?.index?;
        if spine != SPINE_STEP_INDEX {
            return None;
        }
        let item = steps.get(1)?.index?;
        if item < 2 || item % 2 != 0 {
            return None;
        }
        Some(item / 2 - 1)
    }

    /// Character offset of the deepest path part, if any.
    pub fn text_offset(&self) -> Option<i64> {
        self.end
            .as_ref()
            .or(self.start.as_ref())
            .map(StepRef::offset)
            .and_then(|offset| offset.text)
    }
}

impl fmt::Display for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("epubcfi(")?;
        let mut first = true;
        for reference in [&self.parent, &self.start, &self.end].into_iter().flatten() {
            if !first {
                f.write_str(",")?;
            }
            write!(f, "{reference}")?;
            first = false;
        }
        f.write_str(")")
    }
}

/// Index of the `<spine>` element inside the package document.
const SPINE_STEP_INDEX: u32 = 6;

fn normalize_step_ref(reference: &StepRef) -> StepRef {
    StepRef {
        parts: reference
            .parts
            .iter()
            .map(|part| PathPart {
                steps: part.steps.iter().filter(|s| !s.is_null()).cloned().collect(),
                offset: part.offset,
            })
            .collect(),
        open_indirection: reference.open_indirection,
    }
}

fn equivalent_ref(left: Option<&StepRef>, right: Option<&StepRef>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(l), Some(r)) => {
            l.open_indirection == r.open_indirection
                && l.parts.len() == r.parts.len()
                && l.parts.iter().zip(&r.parts).all(|(a, b)| {
                    a.offset == b.offset
                        && a.steps.len() == b.steps.len()
                        && a.steps
                            .iter()
                            .zip(&b.steps)
                            .all(|(x, y)| x.index == y.index)
                })
        }
        _ => false,
    }
}

/// Split the CFI body on top-level commas.
///
/// Commas inside an assertion belong to the assertion. A doubled comma (`,,`)
/// is the null-step spelling documented in the design report (§20.3) and stays
/// inside the path so that `/4,,/20` normalizes to `/4/20`.
fn split_paths(inner: &str) -> Result<Vec<String>, CfiError> {
    let chars: Vec<(usize, char)> = inner.char_indices().collect();
    let mut paths = Vec::new();
    let mut current = String::new();
    let mut depth: usize = 0;
    let mut escaped = false;
    let mut index = 0usize;

    while index < chars.len() {
        let (pos, ch) = chars[index];
        if escaped {
            current.push(ch);
            escaped = false;
            index += 1;
            continue;
        }
        match ch {
            '^' => {
                current.push(ch);
                escaped = true;
            }
            '[' => {
                depth += 1;
                current.push(ch);
            }
            ']' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                if chars.get(index + 1).is_some_and(|(_, next)| *next == ',') {
                    current.push_str(",,");
                    index += 2;
                    continue;
                }
                if current.trim().is_empty() {
                    return Err(CfiError::EmptyComponent(pos));
                }
                paths.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
        index += 1;
    }

    if current.trim().is_empty() {
        if paths.is_empty() {
            return Err(CfiError::Empty);
        }
        return Err(CfiError::EmptyComponent(inner.len()));
    }
    paths.push(current);
    Ok(paths)
}

struct Scanner<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn is_done(&self) -> bool {
        self.pos >= self.text.len()
    }

}

fn parse_step_ref(text: &str) -> Result<StepRef, CfiError> {
    let mut scanner = Scanner::new(text);
    let mut parts = Vec::new();
    let mut open_indirection = false;

    loop {
        let start = scanner.pos;
        while let Some(ch) = scanner.peek() {
            if ch == '!' {
                break;
            }
            scanner.pos += ch.len_utf8();
        }
        let chunk = &text[start..scanner.pos];
        if chunk.is_empty() {
            // A trailing `!` (open indirection) is the only valid empty chunk.
            if scanner.peek() == Some('!') && parts.is_empty() {
                return Err(CfiError::EmptyComponent(start));
            }
            if scanner.peek() != Some('!') {
                return Err(CfiError::EmptyComponent(start));
            }
        } else {
            parts.push(parse_path_part(chunk)?);
        }

        if scanner.eat('!') {
            if scanner.is_done() {
                open_indirection = true;
                break;
            }
            continue;
        }
        break;
    }

    if parts.is_empty() {
        return Err(CfiError::Empty);
    }
    Ok(StepRef {
        parts,
        open_indirection,
    })
}

fn parse_path_part(text: &str) -> Result<PathPart, CfiError> {
    let mut scanner = Scanner::new(text);
    let mut part = PathPart::default();

    while !scanner.is_done() {
        match scanner.peek() {
            Some('/') => {
                scanner.pos += 1;
                part.steps.push(parse_step(&mut scanner)?);
            }
            // Null step spelled with one or two commas (`/4,,/20`).
            Some(',') => {
                scanner.pos += 1;
                if scanner.eat(',') {
                    // consumed the doubled comma
                }
                part.steps.push(Step {
                    index: None,
                    assertion: None,
                });
            }
            Some(':') | Some('~') | Some('@') => {
                parse_offset(&mut scanner, &mut part.offset)?;
            }
            Some(ch) => {
                return Err(CfiError::UnexpectedChar {
                    ch,
                    pos: scanner.pos,
                });
            }
            None => break,
        }
    }

    if part.steps.is_empty() && part.offset.is_empty() {
        return Err(CfiError::EmptyComponent(0));
    }
    Ok(part)
}

fn parse_step(scanner: &mut Scanner<'_>) -> Result<Step, CfiError> {
    let digits_start = scanner.pos;
    while matches!(scanner.peek(), Some(ch) if ch.is_ascii_digit()) {
        scanner.pos += 1;
    }
    let digits = &scanner.text[digits_start..scanner.pos];
    let index = if digits.is_empty() {
        None
    } else {
        Some(digits.parse::<u32>().map_err(|_| CfiError::InvalidNumber {
            text: digits.to_string(),
        })?)
    };

    let assertion = if scanner.eat('[') {
        Some(read_assertion(scanner)?)
    } else {
        None
    };

    if index.is_none() && assertion.is_none() {
        // Null step (`/4,,/20`): explicitly allowed, normalization drops it.
        return Ok(Step {
            index: None,
            assertion: None,
        });
    }
    Ok(Step { index, assertion })
}

fn read_assertion(scanner: &mut Scanner<'_>) -> Result<String, CfiError> {
    let start = scanner.pos;
    let mut value = String::new();
    loop {
        match scanner.next() {
            Some('^') => match scanner.next() {
                Some(escaped) => value.push(escaped),
                None => return Err(CfiError::UnterminatedAssertion(start)),
            },
            Some(']') => return Ok(value),
            Some(ch) => value.push(ch),
            None => return Err(CfiError::UnterminatedAssertion(start)),
        }
    }
}

fn parse_offset(scanner: &mut Scanner<'_>, offset: &mut Offset) -> Result<(), CfiError> {
    match scanner.next() {
        Some(':') => {
            if offset.text.is_some() {
                return Err(CfiError::DuplicateOffset(':'));
            }
            offset.text = Some(parse_integer(scanner)?);
        }
        Some('~') => {
            if offset.temporal.is_some() {
                return Err(CfiError::DuplicateOffset('~'));
            }
            offset.temporal = Some(parse_number(scanner)?);
        }
        Some('@') => {
            if matches!(scanner.peek(), Some('b') | Some('a')) {
                let side = scanner.next().expect("peeked");
                if offset.side.is_some() {
                    return Err(CfiError::DuplicateOffset('@'));
                }
                offset.side = Some(if side == 'a' { Side::After } else { Side::Before });
            } else {
                if offset.spatial_x.is_some() {
                    return Err(CfiError::DuplicateOffset('@'));
                }
                offset.spatial_x = Some(parse_number(scanner)?);
                if scanner.eat(':') {
                    offset.spatial_y = Some(parse_number(scanner)?);
                }
                if scanner.eat('@') {
                    match scanner.next() {
                        Some('b') => offset.side = Some(Side::Before),
                        Some('a') => offset.side = Some(Side::After),
                        Some(ch) => {
                            return Err(CfiError::UnexpectedChar {
                                ch,
                                pos: scanner.pos,
                            });
                        }
                        None => {
                            return Err(CfiError::UnexpectedChar {
                                ch: '@',
                                pos: scanner.pos,
                            });
                        }
                    }
                }
            }
        }
        other => {
            return Err(CfiError::UnexpectedChar {
                ch: other.unwrap_or(' '),
                pos: scanner.pos,
            });
        }
    }
    Ok(())
}

fn parse_integer(scanner: &mut Scanner<'_>) -> Result<i64, CfiError> {
    let start = scanner.pos;
    let negative = scanner.eat('-');
    while matches!(scanner.peek(), Some(ch) if ch.is_ascii_digit()) {
        scanner.pos += 1;
    }
    let digits = &scanner.text[start + usize::from(negative)..scanner.pos];
    if digits.is_empty() {
        return Err(CfiError::InvalidNumber {
            text: scanner.text[start..scanner.pos].to_string(),
        });
    }
    let magnitude = digits.parse::<i64>().map_err(|_| CfiError::InvalidNumber {
        text: digits.to_string(),
    })?;
    Ok(if negative { -magnitude } else { magnitude })
}

fn parse_number(scanner: &mut Scanner<'_>) -> Result<f64, CfiError> {
    let start = scanner.pos;
    if scanner.eat('-') {
        // Temporal and spatial offsets are non-negative; reject instead of
        // silently storing a value the renderer cannot honour.
        return Err(CfiError::InvalidNumber {
            text: scanner.text[start..scanner.pos].to_string(),
        });
    }
    let mut digits = 0usize;
    while matches!(scanner.peek(), Some(ch) if ch.is_ascii_digit()) {
        scanner.pos += 1;
        digits += 1;
    }
    if scanner.eat('.') {
        while matches!(scanner.peek(), Some(ch) if ch.is_ascii_digit()) {
            scanner.pos += 1;
            digits += 1;
        }
    }
    let text = &scanner.text[start..scanner.pos];
    if digits == 0 {
        return Err(CfiError::InvalidNumber {
            text: text.to_string(),
        });
    }
    text.parse::<f64>().map_err(|_| CfiError::InvalidNumber {
        text: text.to_string(),
    })
}

fn escape_assertion(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '^' | '[' | ']' | '(' | ')' | ',' | ';') {
            out.push('^');
        }
        out.push(ch);
    }
    out
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(input: &str) {
        let parsed = Cfi::parse(input).unwrap_or_else(|e| panic!("parse {input}: {e}"));
        assert_eq!(parsed.to_string(), input, "round trip for {input}");
    }

    #[test]
    fn parses_plain_positions() {
        round_trip("epubcfi(/6/24!/4/20/1:58)");
        round_trip("epubcfi(/6/4[chap01ref]!/4[body01]/10[para05])");
        round_trip("epubcfi(/6/4)");
        round_trip("epubcfi(/6/4!/4)");
    }

    #[test]
    fn parses_ranges_and_parents() {
        round_trip("epubcfi(/6/4[chap01ref]!/4[body01]/10[para05],/2/1:1,/3:4)");
        round_trip("epubcfi(/6/4!/4/2:1,/6/4!/4/2:9)");

        let range = Cfi::parse("epubcfi(/6/4!/4/2,/1:0,/1:10)").expect("range");
        assert_eq!(range.start.as_ref().expect("start").offset().text, Some(0));
        assert_eq!(range.end.as_ref().expect("end").offset().text, Some(10));
        assert!(range.parent.is_some());
    }

    #[test]
    fn parses_offsets() {
        let temporal = Cfi::parse("epubcfi(/6/4!/4/2~2.5)").expect("temporal");
        assert_eq!(temporal.text_offset(), None);
        assert_eq!(
            temporal.start.as_ref().expect("start").offset().temporal,
            Some(2.5)
        );

        let spatial = Cfi::parse("epubcfi(/6/4!/4/2@0.5:0.75)").expect("spatial");
        let offset = spatial.start.as_ref().expect("start").offset();
        assert_eq!(offset.spatial_x, Some(0.5));
        assert_eq!(offset.spatial_y, Some(0.75));

        let side = Cfi::parse("epubcfi(/6/4!/4/2:3@b)").expect("side");
        let offset = side.start.as_ref().expect("start").offset();
        assert_eq!(offset.text, Some(3));
        assert_eq!(offset.side, Some(Side::Before));

        round_trip("epubcfi(/6/4!/4/2~2.5)");
        round_trip("epubcfi(/6/4!/4/2@0.5:0.75)");
        round_trip("epubcfi(/6/4!/4/2:3@b)");
    }

    #[test]
    fn escapes_assertions() {
        let parsed = Cfi::parse("epubcfi(/6/4[a^]b^[c^,d]/2)").expect("escaped");
        let assertion = parsed.start.as_ref().expect("start").parts[0].steps[1]
            .assertion
            .clone()
            .expect("assertion");
        assert_eq!(assertion, "a]b[c,d");
        assert_eq!(parsed.to_string(), "epubcfi(/6/4[a^]b^[c^,d]/2)");
    }

    #[test]
    fn rejects_malformed_cfi() {
        assert_eq!(Cfi::parse("/6/4").unwrap_err(), CfiError::MissingWrapper);
        assert_eq!(Cfi::parse("epubcfi()").unwrap_err(), CfiError::Empty);
        assert!(matches!(
            Cfi::parse("epubcfi(/6/4[unterminated)"),
            Err(CfiError::UnterminatedAssertion(_))
        ));
        assert!(matches!(
            Cfi::parse("epubcfi(/6/x)"),
            Err(CfiError::UnexpectedChar { .. })
        ));
        assert!(matches!(
            Cfi::parse("epubcfi(/6/4:4:5)"),
            Err(CfiError::DuplicateOffset(':'))
        ));
        assert!(matches!(
            Cfi::parse("epubcfi(/6/4,/2,/4,/6)"),
            Err(CfiError::TooManyPaths)
        ));
        assert_eq!(
            Cfi::parse("epubcfi(/6/4294967296)").unwrap_err(),
            CfiError::InvalidNumber {
                text: "4294967296".into()
            }
        );
    }

    #[test]
    fn normalizes_null_steps_and_ignores_assertions() {
        let with_null = Cfi::parse("epubcfi(/4,,/20)").expect("null step");
        let plain = Cfi::parse("epubcfi(/4/20)").expect("plain");
        assert_eq!(with_null.normalize().to_string(), "epubcfi(/4/20)");
        assert!(with_null.structurally_equivalent(&plain));

        let asserted = Cfi::parse("epubcfi(/6/4[chap01ref]!/4/2)").expect("asserted");
        let bare = Cfi::parse("epubcfi(/6/4!/4/2)").expect("bare");
        assert!(asserted.structurally_equivalent(&bare));

        let other = Cfi::parse("epubcfi(/6/6!/4/2)").expect("other");
        assert!(!asserted.structurally_equivalent(&other));
    }

    #[test]
    fn derives_spine_index() {
        let first = Cfi::parse("epubcfi(/6/2!/4/2:10)").expect("first");
        assert_eq!(first.spine_index(), Some(0));
        let second = Cfi::parse("epubcfi(/6/4!/4/2:10)").expect("second");
        assert_eq!(second.spine_index(), Some(1));
        let deep = Cfi::parse("epubcfi(/6/24!/4/20/1:58)").expect("deep");
        assert_eq!(deep.spine_index(), Some(11));

        let metadata = Cfi::parse("epubcfi(/2/2:0)").expect("metadata");
        assert_eq!(metadata.spine_index(), None);
        let odd = Cfi::parse("epubcfi(/6/3!/4)").expect("odd");
        assert_eq!(odd.spine_index(), None);
    }

    #[test]
    fn reads_text_offset_and_spine_together() {
        let cfi = Cfi::parse("epubcfi(/6/24!/4/20/1:58)").expect("cfi");
        assert_eq!(cfi.spine_index(), Some(11));
        assert_eq!(cfi.text_offset(), Some(58));
    }
}
