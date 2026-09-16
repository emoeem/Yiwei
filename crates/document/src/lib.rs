//! Reading book files without touching them.
//!
//! Per ADR-0002 the original file is immutable and everything here is a
//! *derived index* that can be rebuilt:
//!
//! * [`zip`] — a ZIP reader with the safety limits from §23.2 (entry count,
//!   size, compression ratio, path traversal),
//! * [`zip_writer`] — a small writer used by tests, the corpus generator and
//!   the export path,
//! * [`xhtml`] — text extraction into deterministic blocks,
//! * [`epub`] — container/OPF/NAV/NCX parsing and lazy section indexing,
//! * [`txt`] — chapter indexing for large plain text books,
//! * [`format`] — format detection by content rather than by file extension.

pub mod epub;
pub mod error;
pub mod format;
pub mod txt;
pub mod xhtml;
pub mod zip;
pub mod zip_writer;

#[cfg(test)]
mod tests;

pub use epub::{
    CONTAINER_PATH, Epub, EpubMetadata, EpubPackage, ManifestItem, NavNode, NavSource, SpineItem,
    container_rootfile, parse_nav_document, parse_ncx_document, resolve_href,
};
pub use error::DocumentError;
pub use format::{BookFormat, detect_format};
pub use txt::{DetectedEncoding, TextEncoding, TxtChapter, TxtDocument, chapter_title};
pub use xhtml::{BlockKind, TextBlock, extract_blocks};
pub use zip::{Method, ZipArchive, ZipEntry, ZipError, ZipLimits};
pub use zip_writer::ZipWriter;

