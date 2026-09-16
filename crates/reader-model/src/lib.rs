//! Document model primitives shared by every other crate.
//!
//! Scope (see design report §19 and ADR-0002):
//!
//! * [`id`] — deterministic IDs for derived entities, UUIDv7 for book entities.
//! * [`cfi`] — EPUB Canonical Fragment Identifier parsing and normalization.
//! * [`locator`] — the cross-scheme reading [`locator::Locator`] persisted in the database.
//! * [`order`] — reading-order comparison used by progress merging (§21.5).
//!
//! Nothing in this crate touches the original book file. It only describes
//! positions and derived identifiers that can be rebuilt at any time.

pub mod cfi;
pub mod id;
pub mod locator;
pub mod order;

pub use cfi::{Cfi, CfiError};
pub use id::{
    Id, IdError, content_hash, deterministic_id, new_entity_id, sha256_hex, spine_item_id,
    text_block_id,
};
pub use locator::{Locator, LocatorError, Rect, TextQuote};
pub use order::{ReadingOrder, compare_reading_order};

