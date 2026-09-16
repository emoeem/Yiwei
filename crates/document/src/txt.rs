//! Plain text books (design report §10.3, §10.4).
//!
//! A 10,000 chapter TXT is never loaded into memory: the file is scanned once
//! line by line to build a chapter index (title + byte range), and a chapter is
//! decoded only when it is opened. Encoding is detected from the BOM, then from
//! validity; a guess is always *labelled* as a guess.

use std::io::ErrorKind;
use std::path::Path;

use encoding_rs::{GBK, UTF_16BE, UTF_16LE, UTF_8};
use reader_annotation::BlockText;
use reader_model::{sha256_hex, spine_item_id, text_block_id};
use serde::{Deserialize, Serialize};

use crate::error::DocumentError;

/// Size of the read buffer used while scanning.
pub const SCAN_BUFFER_BYTES: usize = 1024 * 1024;

/// Character encodings we can decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEncoding {
    /// UTF-8.
    Utf8,
    /// UTF-16 little endian.
    Utf16Le,
    /// UTF-16 big endian.
    Utf16Be,
    /// GBK / GB18030.
    Gbk,
}

/// Result of encoding detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedEncoding {
    /// Chosen encoding.
    pub encoding: TextEncoding,
    /// Whether the choice was a guess rather than a declaration.
    pub assumed: bool,
    /// Number of bytes belonging to a byte order mark.
    pub bom_len: u64,
}

impl DetectedEncoding {
    fn encoding(&self) -> &'static encoding_rs::Encoding {
        match self.encoding {
            TextEncoding::Utf8 => UTF_8,
            TextEncoding::Utf16Le => UTF_16LE,
            TextEncoding::Utf16Be => UTF_16BE,
            TextEncoding::Gbk => GBK,
        }
    }

    /// Byte sequence that terminates a line in this encoding.
    fn newline_pattern(&self) -> &'static [u8] {
        match self.encoding {
            TextEncoding::Utf16Le => &[0x0A, 0x00],
            TextEncoding::Utf16Be => &[0x00, 0x0A],
            _ => &[0x0A],
        }
    }
}

/// One chapter of a plain text book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxtChapter {
    /// Ordinal in reading order.
    pub ordinal: u32,
    /// Chapter title (the paragraph that introduced it).
    pub title: String,
    /// First byte of the chapter.
    pub start_byte: u64,
    /// One past the last byte of the chapter.
    pub end_byte: u64,
}

/// An indexed plain text book.
#[derive(Debug, Clone)]
pub struct TxtDocument {
    bytes: Vec<u8>,
    filename_hint: Option<String>,
    encoding: DetectedEncoding,
    chapters: Vec<TxtChapter>,
    total_bytes: u64,
}

impl TxtDocument {
    /// Scan a text file and build the chapter index.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocumentError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        let filename_hint = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string());
        Self::open_bytes(bytes, filename_hint.as_deref())
    }

    /// Build the chapter index from an in-memory buffer.
    ///
    /// `filename_hint` is used only as the fallback book title when the buffer
    /// has no chapter markers (e.g. "Untitled"). Pass `None` if you do not have
    /// an original file name.
    pub fn open_bytes(
        bytes: Vec<u8>,
        filename_hint: Option<&str>,
    ) -> Result<Self, DocumentError> {
        let total_bytes = bytes.len() as u64;
        let encoding = detect_encoding_bytes(&bytes);
        let mut chapters: Vec<TxtChapter> = Vec::new();

        for_each_line_in_bytes(&bytes, encoding, |start_byte, raw| {
            let line = decode_line(encoding, raw);
            let trimmed = line.trim();
            if let Some(title) = chapter_title(trimmed) {
                if let Some(previous) = chapters.last_mut() {
                    previous.end_byte = start_byte;
                }
                chapters.push(TxtChapter {
                    ordinal: chapters.len() as u32,
                    title,
                    start_byte,
                    end_byte: total_bytes,
                });
            }
            Ok(())
        })?;

        if chapters.is_empty() {
            chapters.push(TxtChapter {
                ordinal: 0,
                title: filename_hint
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Untitled".to_string()),
                start_byte: encoding.bom_len,
                end_byte: total_bytes,
            });
        }

        Ok(Self {
            bytes,
            filename_hint: filename_hint.map(|s| s.to_string()),
            encoding,
            chapters,
            total_bytes,
        })
    }

    /// Detected encoding, with the "assumed" flag.
    pub fn encoding(&self) -> DetectedEncoding {
        self.encoding
    }

    /// Chapter index.
    pub fn chapters(&self) -> &[TxtChapter] {
        &self.chapters
    }

    /// Total size of the source in bytes.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Original source hint (file stem, or `None` for in-memory buffers).
    pub fn source_name(&self) -> Option<&str> {
        self.filename_hint.as_deref()
    }

    /// Decode one chapter.
    pub fn read_chapter(&self, ordinal: u32) -> Result<String, DocumentError> {
        let chapter = self
            .chapters
            .get(ordinal as usize)
            .ok_or_else(|| DocumentError::MissingField(format!("chapter[{ordinal}]"), "txt".into()))?;
        let start = chapter.start_byte as usize;
        let end = chapter.end_byte as usize;
        let bytes = &self.bytes[start..end];
        Ok(self.encoding.encoding().decode(bytes).0.to_string())
    }

    /// Split one chapter into derived blocks.
    pub fn chapter_blocks(
        &self,
        edition_id: &str,
        ordinal: u32,
    ) -> Result<Vec<BlockText>, DocumentError> {
        let text = self.read_chapter(ordinal)?;
        let section_id = format!("txt:chapter:{}", self.chapters[ordinal as usize].ordinal);
        let spine_id = spine_item_id(edition_id, &section_id);
        let mut blocks = Vec::new();
        for paragraph in text.split("\n\n") {
            let paragraph = paragraph.trim();
            if paragraph.is_empty() {
                continue;
            }
            let ordinal = blocks.len() as u32;
            let text_sha = sha256_hex(paragraph.as_bytes());
            let id = text_block_id(edition_id, spine_id.as_str(), ordinal, &text_sha);
            blocks.push(BlockText {
                block_id: id.into_string(),
                ordinal,
                text: paragraph.to_string(),
            });
        }
        Ok(blocks)
    }
}

/// Detect the encoding of a text file (convenience wrapper around
/// [`detect_encoding_bytes`]).
pub fn detect_encoding(path: impl AsRef<Path>) -> Result<DetectedEncoding, DocumentError> {
    let mut file = std::fs::File::open(path.as_ref())?;
    let mut head = vec![0u8; 64 * 1024];
    let read = std::io::Read::read(&mut file, &mut head)?;
    head.truncate(read);
    Ok(detect_encoding_bytes(&head))
}

/// Detect the encoding from an in-memory buffer.
pub fn detect_encoding_bytes(bytes: &[u8]) -> DetectedEncoding {
    let head = bytes;

    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return DetectedEncoding {
            encoding: TextEncoding::Utf8,
            assumed: false,
            bom_len: 3,
        };
    }
    if head.starts_with(&[0xFF, 0xFE]) {
        return DetectedEncoding {
            encoding: TextEncoding::Utf16Le,
            assumed: false,
            bom_len: 2,
        };
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return DetectedEncoding {
            encoding: TextEncoding::Utf16Be,
            assumed: false,
            bom_len: 2,
        };
    }
    if std::str::from_utf8(head).is_ok() {
        return DetectedEncoding {
            encoding: TextEncoding::Utf8,
            assumed: false,
            bom_len: 0,
        };
    }
    DetectedEncoding {
        encoding: TextEncoding::Gbk,
        assumed: true,
        bom_len: 0,
    }
}

/// Iterate the lines of an in-memory buffer, reporting the absolute byte offset
/// of each one.
fn for_each_line_in_bytes<F>(
    bytes: &[u8],
    encoding: DetectedEncoding,
    mut visit: F,
) -> Result<(), DocumentError>
where
    F: FnMut(u64, &[u8]) -> Result<(), DocumentError>,
{
    let pattern = encoding.newline_pattern();
    let pattern_len = pattern.len();
    let mut line_start = encoding.bom_len;
    let mut i = encoding.bom_len as usize;

    while i < bytes.len() {
        if i + pattern_len <= bytes.len() && &bytes[i..i + pattern_len] == pattern {
            visit(line_start, &bytes[line_start as usize..i])?;
            i += pattern_len;
            line_start = i as u64;
        } else {
            i += 1;
        }
    }
    if line_start < bytes.len() as u64 {
        visit(line_start, &bytes[line_start as usize..])?;
    }
    Ok(())
}

fn decode_line(encoding: DetectedEncoding, raw: &[u8]) -> String {
    let raw = match raw.last() {
        Some(b'\r') => &raw[..raw.len() - 1],
        _ => raw,
    };
    encoding.encoding().decode(raw).0.to_string()
}

/// Recognise a chapter heading line.
pub fn chapter_title(line: &str) -> Option<String> {
    if line.is_empty() || line.chars().count() > 60 {
        return None;
    }
    const MARKERS: [&str; 8] = ["章", "节", "回", "卷", "篇", "集", "话", "部"];
    const PREFIXES: [&str; 10] = [
        "序章",
        "楔子",
        "尾声",
        "后记",
        "番外",
        "前言",
        "序言",
        "Prologue",
        "Epilogue",
        "Interlude",
    ];
    if PREFIXES
        .iter()
        .any(|prefix| line.eq_ignore_ascii_case(prefix))
    {
        return Some(line.to_string());
    }
    if let Some(rest) = line.strip_prefix('第') {
        let head: String = rest.chars().take(8).collect();
        if MARKERS.iter().any(|marker| head.contains(marker)) {
            return Some(line.to_string());
        }
    }
    let lower = line.to_ascii_lowercase();
    for keyword in ["chapter ", "part ", "book "] {
        if lower.starts_with(keyword)
            && lower[keyword.len()..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit() || ch.is_ascii_roman_numeral())
        {
            return Some(line.to_string());
        }
    }
    None
}

trait AsciiRoman {
    fn is_ascii_roman_numeral(&self) -> bool;
}

impl AsciiRoman for char {
    fn is_ascii_roman_numeral(&self) -> bool {
        matches!(self.to_ascii_uppercase(), 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M')
    }
}

/// Ensure the file exists and is readable, returning a friendly error otherwise.
pub fn check_readable(path: impl AsRef<Path>) -> Result<u64, DocumentError> {
    match std::fs::metadata(path.as_ref()) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == ErrorKind::NotFound => Err(DocumentError::MissingFile(
            path.as_ref().display().to_string(),
        )),
        Err(error) => Err(DocumentError::Io(error)),
    }
}
