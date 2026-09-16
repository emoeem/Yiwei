//! Identifier strategy (design report §19.1).
//!
//! * Entities that are referenced across devices (publication, edition, annotation,
//!   job, sync op) use UUIDv7 so that IDs stay unique without a coordinator.
//! * Derived entities (spine item, text block) use deterministic hashes so that
//!   rebuilding the index produces the same IDs, while never being written back
//!   into the book file.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Number of hex characters taken from a SHA-256 digest for deterministic IDs.
pub const DETERMINISTIC_ID_HEX_LEN: usize = 32;

/// Hard upper bound for identifier length, matching the `TEXT` primary keys in §18.1.
const MAX_ID_LEN: usize = 128;

/// An opaque entity identifier.
///
/// IDs are validated once at the boundary so that no whitespace or control
/// characters can ever reach the database or the sync protocol.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

/// Why an identifier was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    /// The identifier was empty.
    #[error("id must not be empty")]
    Empty,
    /// The identifier exceeded [`MAX_ID_LEN`] characters.
    #[error("id must be at most {MAX_ID_LEN} characters, got {0}")]
    TooLong(usize),
    /// The identifier contained whitespace or control characters.
    #[error("id must not contain whitespace or control characters: {0:?}")]
    InvalidCharacters(String),
}

impl Id {
    /// Validate and wrap an identifier string.
    pub fn parse(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdError::Empty);
        }
        let char_count = value.chars().count();
        if char_count > MAX_ID_LEN {
            return Err(IdError::TooLong(char_count));
        }
        if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(IdError::InvalidCharacters(value));
        }
        Ok(Id(value))
    }

    /// Build an ID from an already-validated `kind` and hex digest.
    fn from_digest(kind: &str, digest_hex: &str) -> Self {
        Id(format!(
            "{kind}-{}",
            &digest_hex[..DETERMINISTIC_ID_HEX_LEN]
        ))
    }

    /// Borrow the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the identifier and return the inner string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Generate a new UUIDv7 identifier for a cross-device entity.
pub fn new_entity_id() -> Id {
    Id(uuid::Uuid::now_v7().to_string())
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

/// Lowercase hex SHA-256 of a byte stream, for content addressing large files.
///
/// Prefer this over reading a whole book into memory: the reader feeds the
/// hasher in chunks so that a 100 MB EPUB never becomes a 100 MB allocation.
pub fn content_hash(reader: &mut impl std::io::Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(to_hex(&hasher.finalize()))
}

/// Deterministic identifier from a kind and an ordered list of components.
///
/// Components are length-prefixed before hashing so that `("ab", "c")` and
/// `("a", "bc")` cannot collide.
pub fn deterministic_id(kind: &str, parts: &[&str]) -> Id {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0u8]);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    Id::from_digest(kind, &to_hex(&hasher.finalize()))
}

/// Deterministic spine item ID: `hash(edition_id + href)` (§19.1).
pub fn spine_item_id(edition_id: &str, href: &str) -> Id {
    deterministic_id("spine", &[edition_id, href])
}

/// Deterministic text block ID: `hash(edition_id + spine_item + ordinal + text_sha256)` (§19.1).
pub fn text_block_id(
    edition_id: &str,
    spine_item: &str,
    ordinal: u32,
    text_sha256: &str,
) -> Id {
    deterministic_id(
        "block",
        &[edition_id, spine_item, &ordinal.to_string(), text_sha256],
    )
}

fn to_hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a `String` cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_rejects_invalid_input() {
        assert_eq!(Id::parse(""), Err(IdError::Empty));
        assert_eq!(Id::parse("a b"), Err(IdError::InvalidCharacters("a b".into())));
        assert_eq!(Id::parse("a\nb"), Err(IdError::InvalidCharacters("a\nb".into())));
        assert_eq!(Id::parse("x".repeat(129)), Err(IdError::TooLong(129)));
        assert!(Id::parse("x".repeat(128)).is_ok());
    }

    #[test]
    fn deterministic_ids_are_stable_and_distinct() {
        let a = spine_item_id("edition-1", "text/chapter1.xhtml");
        let b = spine_item_id("edition-1", "text/chapter1.xhtml");
        let c = spine_item_id("edition-2", "text/chapter1.xhtml");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.as_str().starts_with("spine-"));
        assert_eq!(a.as_str().len(), "spine-".len() + DETERMINISTIC_ID_HEX_LEN);
    }

    #[test]
    fn length_prefixing_prevents_component_collisions() {
        assert_ne!(
            deterministic_id("k", &["ab", "c"]),
            deterministic_id("k", &["a", "bc"])
        );
    }

    #[test]
    fn text_block_id_depends_on_every_component() {
        let base = text_block_id("e", "s", 3, "abc");
        assert_ne!(base, text_block_id("e", "s", 4, "abc"));
        assert_ne!(base, text_block_id("e", "s", 3, "abd"));
        assert_ne!(base, text_block_id("e2", "s", 3, "abc"));
    }

    #[test]
    fn entity_ids_are_uuid_v7() {
        let id = new_entity_id();
        let parsed = uuid::Uuid::parse_str(id.as_str()).expect("uuid");
        assert_eq!(parsed.get_version_num(), 7);
        assert_ne!(id, new_entity_id());
    }

    #[test]
    fn content_hash_matches_known_digest() {
        // SHA-256("abc")
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex(b"abc"), expected);
        let mut cursor = std::io::Cursor::new(b"abc".to_vec());
        assert_eq!(content_hash(&mut cursor).expect("hash"), expected);
    }
}
