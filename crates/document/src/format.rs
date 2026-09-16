//! Book format detection.
//!
//! Detection uses the file signature, not the extension: a `.txt` file that is
//! actually an EPUB must be opened as an EPUB (§ the report's "no fake
//! implementation" rule — guessing from the name would silently mis-parse).

use std::io::Read;
use std::path::Path;

use crate::error::DocumentError;

/// Formats supported by this crate today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookFormat {
    /// EPUB (also covers FB2/CBZ containers only when explicitly requested).
    Epub,
    /// Plain text.
    Txt,
    /// HTML.
    Html,
    /// PDF, recognised but not parsed by this crate yet.
    Pdf,
}

/// Detect the format of a file.
pub fn detect_format(path: impl AsRef<Path>) -> Result<BookFormat, DocumentError> {
    let mut file = std::fs::File::open(path.as_ref())?;
    // Enough bytes to recognise a doctype or an XML declaration, not just a
    // magic number: format detection must not depend on the file extension.
    let mut head = [0u8; 64];
    let read = file.read(&mut head)?;
    let head = &head[..read];

    if head.starts_with(b"%PDF-") {
        return Ok(BookFormat::Pdf);
    }
    if head.starts_with(&[0x50, 0x4B, 0x03, 0x04])
        || head.starts_with(&[0x50, 0x4B, 0x05, 0x06])
    {
        return Ok(BookFormat::Epub);
    }
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let rest = &head[3..];
        if looks_like_html(rest) {
            return Ok(BookFormat::Html);
        }
        return Ok(BookFormat::Txt);
    }
    if looks_like_html(head) {
        return Ok(BookFormat::Html);
    }
    Ok(BookFormat::Txt)
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes).trim_start().to_ascii_lowercase();
    text.starts_with("<!doctype html")
        || text.starts_with("<html")
        || text.starts_with("<?xml")
        || text.starts_with("<body")
}
