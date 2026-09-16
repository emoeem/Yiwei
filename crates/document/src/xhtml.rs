//! XHTML text extraction for the derived index (§10.2).
//!
//! The extracted text is *derived*: it is never written back into the book and
//! the renderer still renders the original XHTML and CSS. Blocks exist so that
//! search, AI and annotation anchoring have stable, deterministic units.

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::error::DocumentError;

/// Kind of text block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    /// Paragraph or generic `div`.
    Paragraph,
    /// Heading (`h1`..`h6`).
    Heading,
    /// List item.
    ListItem,
    /// Block quote.
    Quote,
    /// Preformatted text.
    Preformatted,
    /// Table cell.
    Cell,
    /// Anything else that produced text.
    Other,
}

/// A block of extracted text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    /// Kind of block.
    pub kind: BlockKind,
    /// Extracted text with whitespace collapsed.
    pub text: String,
}

/// Elements whose text belongs to the navigation chrome rather than the book.
const SKIPPED_ELEMENTS: [&str; 6] = ["script", "style", "head", "audio", "video", "svg"];

/// Extract text blocks from an XHTML document.
pub fn extract_blocks(xhtml: &str) -> Result<Vec<TextBlock>, DocumentError> {
    let mut reader = Reader::from_str(xhtml);
    reader.config_mut().trim_text(false);

    let mut blocks: Vec<TextBlock> = Vec::new();
    let mut buffer = String::new();
    let mut current_kind = BlockKind::Paragraph;
    let mut skip_depth = 0usize;
    let mut buffer_open = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if SKIPPED_ELEMENTS.contains(&name.as_str()) {
                    skip_depth += 1;
                    continue;
                }
                if skip_depth > 0 {
                    continue;
                }
                if let Some(kind) = block_kind(&name) {
                    flush(&mut blocks, &mut buffer, current_kind);
                    current_kind = kind;
                    buffer_open = true;
                }
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if SKIPPED_ELEMENTS.contains(&name.as_str()) {
                    skip_depth = skip_depth.saturating_sub(1);
                    continue;
                }
                if skip_depth > 0 {
                    continue;
                }
                if block_kind(&name).is_some() {
                    flush(&mut blocks, &mut buffer, current_kind);
                    buffer_open = false;
                    current_kind = BlockKind::Paragraph;
                }
            }
            Ok(Event::Empty(event)) => {
                if skip_depth > 0 {
                    continue;
                }
                let name = local_name(event.name().as_ref());
                if name == "br" || name == "hr" {
                    buffer.push(' ');
                }
            }
            Ok(Event::Text(event)) => {
                if skip_depth > 0 {
                    continue;
                }
                let decoded = event.decode().map_err(|error| DocumentError::Xml {
                    context: "xhtml text".into(),
                    source: quick_xml::Error::from(error),
                })?;
                let text = quick_xml::escape::unescape(&decoded).map_err(|error| {
                    DocumentError::Xml {
                        context: "xhtml entities".into(),
                        source: quick_xml::Error::from(error),
                    }
                })?;
                buffer.push_str(&text);
                buffer_open = true;
            }
            Ok(Event::CData(event)) => {
                if skip_depth > 0 {
                    continue;
                }
                if let Ok(text) = std::str::from_utf8(&event) {
                    buffer.push_str(text);
                    buffer_open = true;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(DocumentError::Xml {
                    context: "xhtml".into(),
                    source: error,
                });
            }
        }
    }
    if buffer_open || !buffer.trim().is_empty() {
        flush(&mut blocks, &mut buffer, current_kind);
    }
    Ok(blocks)
}

fn flush(blocks: &mut Vec<TextBlock>, buffer: &mut String, kind: BlockKind) {
    let text = collapse_whitespace(buffer);
    buffer.clear();
    if !text.is_empty() {
        blocks.push(TextBlock { kind, text });
    }
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

fn local_name(raw: &[u8]) -> String {
    let name = String::from_utf8_lossy(raw);
    let name = name.rsplit(':').next().unwrap_or(&name);
    name.to_ascii_lowercase()
}

fn block_kind(name: &str) -> Option<BlockKind> {
    match name {
        "p" | "div" | "section" | "article" | "figcaption" | "dd" | "dt" | "caption" => {
            Some(BlockKind::Paragraph)
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "title" => Some(BlockKind::Heading),
        "li" => Some(BlockKind::ListItem),
        "blockquote" | "q" => Some(BlockKind::Quote),
        "pre" | "code" => Some(BlockKind::Preformatted),
        "td" | "th" => Some(BlockKind::Cell),
        "body" | "main" => Some(BlockKind::Other),
        _ => None,
    }
}
