//! Versioned schema migrations (design report §18.1, §18.3).
//!
//! Rules from the report that this module enforces:
//!
//! * every change is a numbered migration applied inside a transaction,
//! * migrations are recorded, so applying them twice is a no-op,
//! * a database written by a newer build is refused instead of being corrupted,
//! * derived tables (search index) are separate from the tables that hold user
//!   data, so the index can always be rebuilt.

use rusqlite::Connection;

use crate::error::StorageError;

/// Highest schema version this build understands.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// One migration step.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Monotonic version number.
    pub version: u32,
    /// Human readable name, recorded in the database.
    pub name: &'static str,
    /// SQL to apply.
    pub sql: &'static str,
}

/// All migrations, in order.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial schema",
    sql: INITIAL_SCHEMA,
}];

const INITIAL_SCHEMA: &str = r#"
-- Books and files
CREATE TABLE IF NOT EXISTS publication(
  id TEXT PRIMARY KEY, uuid TEXT UNIQUE, title TEXT, author_json TEXT,
  language TEXT, metadata_json TEXT, created_at INTEGER, updated_at INTEGER,
  deleted_at INTEGER, rev INTEGER, device_id TEXT);
CREATE TABLE IF NOT EXISTS book_file(
  id TEXT PRIMARY KEY, publication_id TEXT REFERENCES publication(id), sha256 TEXT,
  partial_md5 TEXT, byte_size INTEGER, format TEXT, local_path TEXT, blob_id TEXT,
  imported_at INTEGER, source_json TEXT);
CREATE TABLE IF NOT EXISTS edition(
  id TEXT PRIMARY KEY, publication_id TEXT REFERENCES publication(id), kind TEXT,
  language TEXT, source_edition_id TEXT, file_id TEXT, title TEXT,
  metadata_json TEXT, created_at INTEGER, updated_at INTEGER);

-- Derived structure index (rebuildable, never written back into the book)
CREATE TABLE IF NOT EXISTS spine_item(
  id TEXT PRIMARY KEY, edition_id TEXT REFERENCES edition(id), ordinal INTEGER,
  href TEXT, media_type TEXT, linear INTEGER, properties_json TEXT);
CREATE TABLE IF NOT EXISTS nav_node(
  id TEXT PRIMARY KEY, edition_id TEXT REFERENCES edition(id), parent_id TEXT,
  ordinal INTEGER, label TEXT, href TEXT, fragment TEXT);
CREATE TABLE IF NOT EXISTS text_block(
  id TEXT PRIMARY KEY, edition_id TEXT REFERENCES edition(id),
  spine_item_id TEXT, ordinal INTEGER, block_kind TEXT, text TEXT,
  text_sha256 TEXT, doc_fingerprint TEXT);

-- Reading state
CREATE TABLE IF NOT EXISTS reading_state(
  publication_id TEXT, edition_id TEXT, device_id TEXT, locator_json TEXT,
  progress REAL, furthest_json TEXT, updated_at INTEGER, hlc TEXT,
  PRIMARY KEY(publication_id, edition_id, device_id));

-- Annotations
CREATE TABLE IF NOT EXISTS annotation(
  id TEXT PRIMARY KEY, publication_id TEXT REFERENCES publication(id),
  edition_id TEXT, type TEXT, color TEXT, style TEXT, note TEXT,
  selected_text TEXT, tags_json TEXT, global INTEGER DEFAULT 0,
  created_at INTEGER, updated_at INTEGER, deleted_at INTEGER, rev INTEGER,
  device_id TEXT);
CREATE TABLE IF NOT EXISTS annotation_anchor(
  annotation_id TEXT PRIMARY KEY REFERENCES annotation(id), scheme TEXT,
  primary_locator TEXT, doc_id TEXT, section_id TEXT, block_id TEXT,
  exact_text TEXT, prefix_text TEXT, suffix_text TEXT, start_offset INTEGER,
  end_offset INTEGER, doc_fingerprint TEXT, rects_json TEXT, page INTEGER,
  confidence REAL, anchor_state TEXT, updated_at INTEGER);
CREATE TABLE IF NOT EXISTS note_revision(
  id TEXT PRIMARY KEY, annotation_id TEXT REFERENCES annotation(id),
  base_rev INTEGER, content TEXT, created_at INTEGER, device_id TEXT);

-- Sources and plugins
CREATE TABLE IF NOT EXISTS source(
  id TEXT PRIMARY KEY, kind TEXT, name TEXT, enabled INTEGER, config_json TEXT,
  secrets_ref TEXT, created_at INTEGER, updated_at INTEGER, deleted_at INTEGER,
  rev INTEGER);
CREATE TABLE IF NOT EXISTS source_rule(
  source_id TEXT, interface TEXT, rule_json TEXT, compat_level TEXT,
  PRIMARY KEY(source_id, interface));
CREATE TABLE IF NOT EXISTS plugin(
  id TEXT PRIMARY KEY, version TEXT, runtime TEXT, manifest_json TEXT,
  permissions_json TEXT, enabled INTEGER, signature TEXT, installed_at INTEGER);

-- Jobs and statistics
CREATE TABLE IF NOT EXISTS job(
  id TEXT PRIMARY KEY, kind TEXT, state TEXT, input_json TEXT, progress REAL,
  output_json TEXT, error_json TEXT, created_at INTEGER, updated_at INTEGER,
  started_at INTEGER, finished_at INTEGER);
CREATE TABLE IF NOT EXISTS reading_session(
  id TEXT PRIMARY KEY, publication_id TEXT, edition_id TEXT, device_id TEXT,
  started_at INTEGER, ended_at INTEGER, duration_seconds INTEGER, pages INTEGER,
  words INTEGER, locator_json TEXT);
CREATE TABLE IF NOT EXISTS stat_daily(
  id TEXT PRIMARY KEY, device_id TEXT, day TEXT, publication_id TEXT,
  duration_seconds INTEGER, pages INTEGER, words INTEGER, sessions INTEGER,
  hlc TEXT);

-- AI and glossary
CREATE TABLE IF NOT EXISTS glossary_term(
  id TEXT PRIMARY KEY, publication_id TEXT, edition_id TEXT, source_term TEXT,
  target_term TEXT, term_type TEXT, note TEXT, status TEXT, hlc TEXT,
  deleted_at INTEGER);
CREATE TABLE IF NOT EXISTS ai_run(
  id TEXT PRIMARY KEY, capability TEXT, provider_id TEXT, model TEXT,
  input_hash TEXT, cache_key TEXT, usage_json TEXT, created_at INTEGER);

-- Settings and sync bookkeeping
CREATE TABLE IF NOT EXISTS setting(
  key TEXT PRIMARY KEY, value_json TEXT, hlc TEXT, device_id TEXT);
CREATE TABLE IF NOT EXISTS sync_outbox(
  op_id TEXT PRIMARY KEY, entity TEXT, entity_id TEXT, op TEXT,
  payload_json TEXT, base_revision INTEGER, hlc TEXT, attempts INTEGER,
  created_at INTEGER, last_error TEXT);
CREATE TABLE IF NOT EXISTS sync_state(
  peer_id TEXT PRIMARY KEY, cursor TEXT, last_pull_at INTEGER,
  last_push_at INTEGER, protocol_version INTEGER);

-- Recommended indexes (§18.1)
CREATE INDEX IF NOT EXISTS idx_annotation_publication ON annotation(publication_id, deleted_at);
CREATE INDEX IF NOT EXISTS idx_annotation_updated ON annotation(updated_at);
CREATE INDEX IF NOT EXISTS idx_reading_state_publication ON reading_state(publication_id);
CREATE INDEX IF NOT EXISTS idx_sync_outbox_created ON sync_outbox(created_at);
CREATE INDEX IF NOT EXISTS idx_text_block_edition ON text_block(edition_id, spine_item_id, ordinal);
CREATE INDEX IF NOT EXISTS idx_job_state ON job(state, updated_at);
CREATE INDEX IF NOT EXISTS idx_book_file_hash ON book_file(sha256);
CREATE INDEX IF NOT EXISTS idx_spine_item_edition ON spine_item(edition_id, ordinal);
"#;

/// Apply every migration that has not been applied yet.
///
/// Returns the schema version after the call.
pub fn apply(connection: &mut Connection) -> Result<u32, StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migration(
           version INTEGER PRIMARY KEY,
           name TEXT NOT NULL,
           applied_at INTEGER NOT NULL
         );",
    )?;

    let applied: u32 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migration",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if applied > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::SchemaTooNew {
            found: applied,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    for migration in MIGRATIONS {
        if migration.version <= applied {
            continue;
        }
        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migration(version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                migration.version,
                migration.name,
                now_seconds()
            ],
        )?;
        transaction.commit()?;
    }
    Ok(CURRENT_SCHEMA_VERSION)
}

/// Unix seconds; the schema stores times as INTEGER (§18.3).
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

