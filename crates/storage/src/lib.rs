//! Local SQLite storage (design report §18, ADR-0004).
//!
//! The local database is the source of truth. This crate owns its schema
//! (migrations), the repositories that read and write it, and the honest
//! reporting of capabilities: if the linked SQLite has no FTS5, full-text
//! search reports that it is unavailable instead of returning empty results.
//!
//! Not implemented here (and deliberately not faked): the Postgres server-side
//! schema, migrations and merge functions, which belong to `server/`.

pub mod db;
pub mod error;
pub mod migrations;
pub mod repos;

#[cfg(test)]
mod tests;

pub use db::{Capabilities, Database};
pub use error::StorageError;
pub use migrations::{CURRENT_SCHEMA_VERSION, Migration, MIGRATIONS, apply, now_seconds};
pub use repos::{
    AiRunRow, AiRuns, AnchorRow, Annotations, BookFileRow, BookFiles, EditionRow, Editions,
    GlossaryTermRow, GlossaryTerms, JobRow, Jobs, NavNodeRow, NavNodes, NoteRevisionRow, Outbox,
    OutboxItem, PluginRow, Plugins, PublicationRow, Publications, ReadingSessionRow, ReadingSessions, ReadingStateRow, ReadingStates, Search, SearchHit, SettingRow,
    Settings, SourceRow, SourceRuleRow, SourceRules, Sources, SpineItemRow, SpineItems,
    StatDailyRow, StatDailies, TextBlockRow, TextBlocks,
};
