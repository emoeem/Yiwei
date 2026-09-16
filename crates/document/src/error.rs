//! Document errors.

/// Anything that can go wrong while reading a book file.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    /// The file could not be read.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The ZIP container was malformed.
    #[error("zip error: {0}")]
    Zip(#[from] crate::zip::ZipError),
    /// The XML was malformed.
    #[error("xml error in {context}: {source}")]
    Xml {
        /// Which document failed to parse.
        context: String,
        /// The underlying parser error.
        source: quick_xml::Error,
    },
    /// A required file was missing from the container.
    #[error("required file {0:?} is missing")]
    MissingFile(String),
    /// A required XML element or attribute was missing.
    #[error("missing element/attribute {0} in {1}")]
    MissingField(String, String),
    /// The book uses a feature we do not support.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

