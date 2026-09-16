//! Repositories over the local schema.
//!
//! Everything here works on explicit SQL — no ORM — so that the same statements
//! can be reviewed against the Postgres variant later (§18.3), and so that
//! derived data (search index, block text) is clearly separated from user data.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use reader_model::Locator;
use reader_sync::Op;

use crate::error::StorageError;

/// A row of `publication`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicationRow {
    /// Publication ID (UUIDv7).
    pub id: String,
    /// Optional stable UUID from the book metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// Title.
    pub title: String,
    /// Authors, stored as a JSON array.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Language tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Remaining metadata, kept verbatim so unknown fields survive.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Creation time (unix seconds).
    pub created_at: i64,
    /// Last update time.
    pub updated_at: i64,
    /// Tombstone time, when the book was removed from the shelf.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    /// Revision counter used for conflict detection.
    pub rev: i64,
    /// Device that last wrote the row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// A row of `annotation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationRow {
    /// Annotation ID.
    pub id: String,
    /// Owning publication.
    pub publication_id: String,
    /// Edition the annotation belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition_id: Option<String>,
    /// Annotation type, e.g. `highlight`.
    pub annotation_type: String,
    /// Colour, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Style, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Note body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Selected text, denormalised for listing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_text: Option<String>,
    /// Tags, stored as a JSON array.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether the annotation is visible across editions.
    pub global: bool,
    /// Creation time.
    pub created_at: i64,
    /// Last update time.
    pub updated_at: i64,
    /// Tombstone time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    /// Revision counter.
    pub rev: i64,
    /// Device that last wrote the row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// A row of `annotation_anchor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorRow {
    /// Owning annotation.
    pub annotation_id: String,
    /// Addressing scheme.
    pub scheme: String,
    /// Primary locator, stored as JSON.
    pub primary_locator: Locator,
    /// Document ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<String>,
    /// Section ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    /// Block ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    /// Exact quoted text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_text: Option<String>,
    /// Text before the quote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_text: Option<String>,
    /// Text after the quote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix_text: Option<String>,
    /// Start character offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<u32>,
    /// End character offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<u32>,
    /// Section fingerprint at anchor time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_fingerprint: Option<String>,
    /// Normalized rectangles, stored as JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rects: Option<serde_json::Value>,
    /// Page number for fixed layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Confidence in the current position.
    pub confidence: f64,
    /// Anchor state.
    pub anchor_state: String,
    /// Last update time.
    pub updated_at: i64,
}

/// A row of `note_revision`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteRevisionRow {
    /// Revision ID.
    pub id: String,
    /// Owning annotation.
    pub annotation_id: String,
    /// Revision the edit was based on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_rev: Option<i64>,
    /// Note body at this revision.
    pub content: String,
    /// Creation time.
    pub created_at: i64,
    /// Device that created it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// A row of `reading_state`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingStateRow {
    /// Publication ID.
    pub publication_id: String,
    /// Edition ID.
    pub edition_id: String,
    /// Device ID.
    pub device_id: String,
    /// Current locator.
    pub locator: Locator,
    /// Normalized progression.
    pub progress: f64,
    /// Furthest locator, stored as JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub furthest: Option<Locator>,
    /// Last update time.
    pub updated_at: i64,
    /// Clock of the last write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hlc: Option<String>,
}

/// A row of `setting`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingRow {
    /// Setting key.
    pub key: String,
    /// Setting value.
    pub value: serde_json::Value,
    /// Clock of the last write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hlc: Option<String>,
    /// Device that last wrote the setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// A queued sync operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxItem {
    /// The operation to push.
    pub op: Op,
    /// How many attempts were made.
    pub attempts: i64,
    /// When the operation was queued.
    pub created_at: i64,
    /// Last error, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// A full-text search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Block ID.
    pub block_id: String,
    /// Matched text.
    pub text: String,
}

/// A row of `edition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditionRow {
    pub id: String,
    pub publication_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_edition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A row of `book_file`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookFileRow {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_md5: Option<String>,
    pub byte_size: i64,
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<i64>,
    #[serde(default)]
    pub source: serde_json::Value,
}

/// A row of `spine_item`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpineItemRow {
    pub id: String,
    pub edition_id: String,
    pub ordinal: i64,
    pub href: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub linear: bool,
    #[serde(default)]
    pub properties: serde_json::Value,
}

/// A row of `nav_node`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavNodeRow {
    pub id: String,
    pub edition_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub ordinal: i64,
    pub label: String,
    pub href: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
}

/// A row of `text_block`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlockRow {
    pub id: String,
    pub edition_id: String,
    pub spine_item_id: String,
    pub ordinal: i64,
    pub block_kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_fingerprint: Option<String>,
}

/// A row of `job`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRow {
    pub id: String,
    pub kind: String,
    pub state: String,
    #[serde(default)]
    pub input: serde_json::Value,
    pub progress: f64,
    #[serde(default)]
    pub output: serde_json::Value,
    #[serde(default)]
    pub error: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

/// A row of `reading_session`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingSessionRow {
    pub id: String,
    pub publication_id: String,
    pub edition_id: String,
    pub device_id: String,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(default)]
    pub duration_seconds: i64,
    #[serde(default)]
    pub pages: i64,
    #[serde(default)]
    pub words: i64,
    #[serde(default)]
    pub locator: serde_json::Value,
}

/// A row of `stat_daily`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatDailyRow {
    pub id: String,
    pub device_id: String,
    pub day: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_id: Option<String>,
    pub duration_seconds: i64,
    pub pages: i64,
    pub words: i64,
    pub sessions: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hlc: Option<String>,
}

/// A row of `glossary_term`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlossaryTermRow {
    pub id: String,
    pub publication_id: String,
    pub edition_id: String,
    pub source_term: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_term: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hlc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
}

/// A row of `ai_run`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiRunRow {
    pub id: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
    #[serde(default)]
    pub usage: serde_json::Value,
    pub created_at: i64,
}

/// A row of `source`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets_ref: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    pub rev: i64,
}

/// A row of `source_rule`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRuleRow {
    pub source_id: String,
    pub interface_: String,
    #[serde(default)]
    pub rule: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compat_level: Option<String>,
}

/// A row of `plugin`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginRow {
    pub id: String,
    pub version: String,
    pub runtime: String,
    #[serde(default)]
    pub manifest: serde_json::Value,
    #[serde(default)]
    pub permissions: serde_json::Value,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub installed_at: i64,
}

/// Access to `publication`.
#[derive(Debug)]
pub struct Publications<'a> {
    connection: &'a Connection,
}

impl<'a> Publications<'a> {
    /// Wrap a connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Insert or update a publication row.
    pub fn save(&self, row: &PublicationRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO publication(id, uuid, title, author_json, language, metadata_json,
                                     created_at, updated_at, deleted_at, rev, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
               uuid = excluded.uuid,
               title = excluded.title,
               author_json = excluded.author_json,
               language = excluded.language,
               metadata_json = excluded.metadata_json,
               updated_at = excluded.updated_at,
               deleted_at = excluded.deleted_at,
               rev = excluded.rev,
               device_id = excluded.device_id",
            params![
                row.id,
                row.uuid,
                row.title,
                serde_json::to_string(&row.authors)?,
                row.language,
                serde_json::to_string(&row.metadata)?,
                row.created_at,
                row.updated_at,
                row.deleted_at,
                row.rev,
                row.device_id,
            ],
        )?;
        Ok(())
    }

    /// Fetch one publication.
    pub fn get(&self, id: &str) -> Result<Option<PublicationRow>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, uuid, title, author_json, language, metadata_json,
                        created_at, updated_at, deleted_at, rev, device_id
                 FROM publication WHERE id = ?1",
                params![id],
                map_publication,
            )
            .optional()?;
        Ok(row)
    }

    /// List publications, newest first.
    pub fn list(&self, include_deleted: bool) -> Result<Vec<PublicationRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, uuid, title, author_json, language, metadata_json,
                    created_at, updated_at, deleted_at, rev, device_id
             FROM publication
             WHERE (?1 = 1) OR deleted_at IS NULL
             ORDER BY updated_at DESC, id ASC",
        )?;
        let rows = statement.query_map(params![include_deleted], map_publication)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Soft delete a publication (tombstone, never a hard delete).
    pub fn soft_delete(&self, id: &str, deleted_at: i64) -> Result<usize, StorageError> {
        let changed = self.connection.execute(
            "UPDATE publication SET deleted_at = ?2, updated_at = ?2, rev = rev + 1 WHERE id = ?1",
            params![id, deleted_at],
        )?;
        Ok(changed)
    }

    /// Number of rows, including tombstones.
    pub fn count(&self) -> Result<i64, StorageError> {
        let count =
            self.connection
                .query_row("SELECT COUNT(*) FROM publication", [], |row| row.get(0))?;
        Ok(count)
    }
}

/// Access to `annotation`, `annotation_anchor` and `note_revision`.
#[derive(Debug)]
pub struct Annotations<'a> {
    connection: &'a Connection,
}

impl<'a> Annotations<'a> {
    /// Wrap a connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Insert or update an annotation.
    pub fn save(&self, row: &AnnotationRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO annotation(id, publication_id, edition_id, type, color, style, note,
                                    selected_text, tags_json, global, created_at, updated_at,
                                    deleted_at, rev, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
               edition_id = excluded.edition_id,
               type = excluded.type,
               color = excluded.color,
               style = excluded.style,
               note = excluded.note,
               selected_text = excluded.selected_text,
               tags_json = excluded.tags_json,
               global = excluded.global,
               updated_at = excluded.updated_at,
               deleted_at = excluded.deleted_at,
               rev = excluded.rev,
               device_id = excluded.device_id",
            params![
                row.id,
                row.publication_id,
                row.edition_id,
                row.annotation_type,
                row.color,
                row.style,
                row.note,
                row.selected_text,
                serde_json::to_string(&row.tags)?,
                i64::from(row.global),
                row.created_at,
                row.updated_at,
                row.deleted_at,
                row.rev,
                row.device_id,
            ],
        )?;
        Ok(())
    }

    /// Save the anchor of an annotation.
    pub fn save_anchor(&self, row: &AnchorRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO annotation_anchor(annotation_id, scheme, primary_locator, doc_id,
                                           section_id, block_id, exact_text, prefix_text,
                                           suffix_text, start_offset, end_offset,
                                           doc_fingerprint, rects_json, page, confidence,
                                           anchor_state, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT(annotation_id) DO UPDATE SET
               scheme = excluded.scheme,
               primary_locator = excluded.primary_locator,
               doc_id = excluded.doc_id,
               section_id = excluded.section_id,
               block_id = excluded.block_id,
               exact_text = excluded.exact_text,
               prefix_text = excluded.prefix_text,
               suffix_text = excluded.suffix_text,
               start_offset = excluded.start_offset,
               end_offset = excluded.end_offset,
               doc_fingerprint = excluded.doc_fingerprint,
               rects_json = excluded.rects_json,
               page = excluded.page,
               confidence = excluded.confidence,
               anchor_state = excluded.anchor_state,
               updated_at = excluded.updated_at",
            params![
                row.annotation_id,
                row.scheme,
                serde_json::to_string(&row.primary_locator)?,
                row.doc_id,
                row.section_id,
                row.block_id,
                row.exact_text,
                row.prefix_text,
                row.suffix_text,
                row.start_offset,
                row.end_offset,
                row.doc_fingerprint,
                row.rects.as_ref().map(serde_json::to_string).transpose()?,
                row.page,
                row.confidence,
                row.anchor_state,
                row.updated_at,
            ],
        )?;
        Ok(())
    }

    /// List annotations of one publication, newest first.
    pub fn list_for_publication(
        &self,
        publication_id: &str,
        include_deleted: bool,
    ) -> Result<Vec<AnnotationRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, publication_id, edition_id, type, color, style, note, selected_text,
                    tags_json, global, created_at, updated_at, deleted_at, rev, device_id
             FROM annotation
             WHERE publication_id = ?1 AND ((?2 = 1) OR deleted_at IS NULL)
             ORDER BY updated_at DESC, id ASC",
        )?;
        let rows = statement.query_map(params![publication_id, include_deleted], map_annotation)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Fetch one annotation.
    pub fn get(&self, id: &str) -> Result<Option<AnnotationRow>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, publication_id, edition_id, type, color, style, note, selected_text,
                        tags_json, global, created_at, updated_at, deleted_at, rev, device_id
                 FROM annotation WHERE id = ?1",
                params![id],
                map_annotation,
            )
            .optional()?;
        Ok(row)
    }

    /// Fetch the anchor of an annotation.
    pub fn anchor(&self, annotation_id: &str) -> Result<Option<AnchorRow>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT annotation_id, scheme, primary_locator, doc_id, section_id, block_id,
                        exact_text, prefix_text, suffix_text, start_offset, end_offset,
                        doc_fingerprint, rects_json, page, confidence, anchor_state, updated_at
                 FROM annotation_anchor WHERE annotation_id = ?1",
                params![annotation_id],
                map_anchor,
            )
            .optional()?;
        Ok(row)
    }

    /// Soft delete an annotation.
    pub fn soft_delete(&self, id: &str, deleted_at: i64) -> Result<usize, StorageError> {
        let changed = self.connection.execute(
            "UPDATE annotation SET deleted_at = ?2, updated_at = ?2, rev = rev + 1 WHERE id = ?1",
            params![id, deleted_at],
        )?;
        Ok(changed)
    }

    /// Append a note revision (§20.4 keeps the history of note bodies).
    pub fn add_note_revision(&self, row: &NoteRevisionRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO note_revision(id, annotation_id, base_rev, content, created_at, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.id,
                row.annotation_id,
                row.base_rev,
                row.content,
                row.created_at,
                row.device_id
            ],
        )?;
        Ok(())
    }

    /// Note revisions of one annotation, oldest first.
    pub fn note_revisions(&self, annotation_id: &str) -> Result<Vec<NoteRevisionRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, annotation_id, base_rev, content, created_at, device_id
             FROM note_revision WHERE annotation_id = ?1 ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map(params![annotation_id], |row| {
            Ok(NoteRevisionRow {
                id: row.get(0)?,
                annotation_id: row.get(1)?,
                base_rev: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                device_id: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

/// Access to `reading_state`.
#[derive(Debug)]
pub struct ReadingStates<'a> {
    connection: &'a Connection,
}

impl<'a> ReadingStates<'a> {
    /// Wrap a connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Insert or update the reading state of one device.
    pub fn put(&self, row: &ReadingStateRow) -> Result<(), StorageError> {
        let furthest = row
            .furthest
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.connection.execute(
            "INSERT INTO reading_state(publication_id, edition_id, device_id, locator_json,
                                       progress, furthest_json, updated_at, hlc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(publication_id, edition_id, device_id) DO UPDATE SET
               locator_json = excluded.locator_json,
               progress = excluded.progress,
               furthest_json = excluded.furthest_json,
               updated_at = excluded.updated_at,
               hlc = excluded.hlc",
            params![
                row.publication_id,
                row.edition_id,
                row.device_id,
                serde_json::to_string(&row.locator)?,
                row.progress,
                furthest,
                row.updated_at,
                row.hlc,
            ],
        )?;
        Ok(())
    }

    /// Fetch the reading state of one device.
    pub fn get(
        &self,
        publication_id: &str,
        edition_id: &str,
        device_id: &str,
    ) -> Result<Option<ReadingStateRow>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT publication_id, edition_id, device_id, locator_json, progress,
                        furthest_json, updated_at, hlc
                 FROM reading_state
                 WHERE publication_id = ?1 AND edition_id = ?2 AND device_id = ?3",
                params![publication_id, edition_id, device_id],
                map_reading_state,
            )
            .optional()?;
        Ok(row)
    }

    /// All device states for one book, most recently updated first.
    pub fn list_for_publication(
        &self,
        publication_id: &str,
    ) -> Result<Vec<ReadingStateRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT publication_id, edition_id, device_id, locator_json, progress,
                    furthest_json, updated_at, hlc
             FROM reading_state WHERE publication_id = ?1
             ORDER BY updated_at DESC, device_id ASC",
        )?;
        let rows = statement.query_map(params![publication_id], map_reading_state)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

/// Access to `setting`.
#[derive(Debug)]
pub struct Settings<'a> {
    connection: &'a Connection,
}

impl<'a> Settings<'a> {
    /// Wrap a connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Store a setting value.
    pub fn put(&self, row: &SettingRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO setting(key, value_json, hlc, device_id) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET
               value_json = excluded.value_json,
               hlc = excluded.hlc,
               device_id = excluded.device_id",
            params![
                row.key,
                serde_json::to_string(&row.value)?,
                row.hlc,
                row.device_id
            ],
        )?;
        Ok(())
    }

    /// Read one setting.
    pub fn get(&self, key: &str) -> Result<Option<SettingRow>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT key, value_json, hlc, device_id FROM setting WHERE key = ?1",
                params![key],
                |row| {
                    let value: String = row.get(1)?;
                    Ok(SettingRow {
                        key: row.get(0)?,
                        value: serde_json::from_str(&value).unwrap_or(serde_json::Value::Null),
                        hlc: row.get(2)?,
                        device_id: row.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// All settings, ordered by key.
    pub fn all(&self) -> Result<Vec<SettingRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT key, value_json, hlc, device_id FROM setting ORDER BY key ASC",
        )?;
        let rows = statement.query_map([], |row| {
            let value: String = row.get(1)?;
            Ok(SettingRow {
                key: row.get(0)?,
                value: serde_json::from_str(&value).unwrap_or(serde_json::Value::Null),
                hlc: row.get(2)?,
                device_id: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

/// Access to `sync_outbox`.
#[derive(Debug)]
pub struct Outbox<'a> {
    connection: &'a Connection,
}

impl<'a> Outbox<'a> {
    /// Wrap a connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Queue an operation for push.
    pub fn enqueue(&self, op: &Op, created_at: i64) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO sync_outbox(op_id, entity, entity_id, op, payload_json,
                                               base_revision, hlc, attempts, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            params![
                op.op_id,
                serde_json::to_value(op.entity)?.as_str().unwrap_or_default(),
                op.entity_id,
                serde_json::to_value(op.kind)?.as_str().unwrap_or_default(),
                serde_json::to_string(op)?,
                op.base_revision,
                op.hlc.to_string(),
                created_at,
            ],
        )?;
        Ok(())
    }

    /// Pending operations, oldest first.
    pub fn pending(&self, limit: usize) -> Result<Vec<OutboxItem>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT payload_json, attempts, created_at, last_error
             FROM sync_outbox ORDER BY created_at ASC, op_id ASC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], |row| {
            let payload: String = row.get(0)?;
            Ok((payload, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, Option<String>>(3)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (payload, attempts, created_at, last_error) = row?;
            out.push(OutboxItem {
                op: serde_json::from_str(&payload)?,
                attempts,
                created_at,
                last_error,
            });
        }
        Ok(out)
    }

    /// Remove an operation once the server accepted it.
    pub fn mark_pushed(&self, op_id: &str) -> Result<usize, StorageError> {
        let changed = self
            .connection
            .execute("DELETE FROM sync_outbox WHERE op_id = ?1", params![op_id])?;
        Ok(changed)
    }

    /// Record a failed attempt.
    pub fn record_failure(&self, op_id: &str, error: &str) -> Result<usize, StorageError> {
        let changed = self.connection.execute(
            "UPDATE sync_outbox SET attempts = attempts + 1, last_error = ?2 WHERE op_id = ?1",
            params![op_id, error],
        )?;
        Ok(changed)
    }

    /// Number of queued operations.
    pub fn count(&self) -> Result<i64, StorageError> {
        let count =
            self.connection
                .query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))?;
        Ok(count)
    }
}

/// Full-text search over the derived `text_block` index.
#[derive(Debug)]
pub struct Search<'a> {
    connection: &'a Connection,
    fts5: bool,
}

impl<'a> Search<'a> {
    /// Wrap a connection and the probed capability.
    pub fn new(connection: &'a Connection, fts5: bool) -> Self {
        Self { connection, fts5 }
    }

    /// Index one section's blocks.
    pub fn index_section(
        &self,
        edition_id: &str,
        spine_item_id: &str,
        blocks: &[(String, String)],
    ) -> Result<usize, StorageError> {
        if !self.fts5 {
            return Err(StorageError::MissingCapability("fts5".into()));
        }
        let transaction_started = self.connection.is_autocommit();
        if transaction_started {
            self.connection.execute_batch("BEGIN")?;
        }
        let result = (|| -> Result<usize, StorageError> {
            let mut inserted = 0usize;
            {
                let mut statement = self.connection.prepare(
                    "INSERT INTO text_fts(text, block_id, edition_id, spine_item_id)
                     VALUES (?1, ?2, ?3, ?4)",
                )?;
                for (block_id, text) in blocks {
                    statement.execute(params![text, block_id, edition_id, spine_item_id])?;
                    inserted += 1;
                }
            }
            Ok(inserted)
        })();
        if transaction_started {
            match &result {
                Ok(_) => self.connection.execute_batch("COMMIT")?,
                Err(_) => self.connection.execute_batch("ROLLBACK")?,
            }
        }
        result
    }

    /// Query the index of one edition.
    pub fn query(
        &self,
        edition_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, StorageError> {
        if !self.fts5 {
            return Err(StorageError::MissingCapability("fts5".into()));
        }
        let mut statement = self.connection.prepare(
            "SELECT block_id, text FROM text_fts
             WHERE text_fts MATCH ?1 AND edition_id = ?2
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = statement.query_map(params![query, edition_id, limit as i64], |row| {
            Ok(SearchHit {
                block_id: row.get(0)?,
                text: row.get(1)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn conversion_error(index: usize, error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(error),
    )
}

fn map_publication(row: &rusqlite::Row<'_>) -> rusqlite::Result<PublicationRow> {
    let authors_json: String = row.get(3)?;
    let metadata_json: String = row.get(5)?;
    Ok(PublicationRow {
        id: row.get(0)?,
        uuid: row.get(1)?,
        title: row.get(2)?,
        authors: serde_json::from_str(&authors_json).map_err(|e| conversion_error(3, e))?,
        language: row.get(4)?,
        metadata: serde_json::from_str(&metadata_json).map_err(|e| conversion_error(5, e))?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        deleted_at: row.get(8)?,
        rev: row.get(9)?,
        device_id: row.get(10)?,
    })
}

fn map_annotation(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnnotationRow> {
    let tags_json: String = row.get(8)?;
    Ok(AnnotationRow {
        id: row.get(0)?,
        publication_id: row.get(1)?,
        edition_id: row.get(2)?,
        annotation_type: row.get(3)?,
        color: row.get(4)?,
        style: row.get(5)?,
        note: row.get(6)?,
        selected_text: row.get(7)?,
        tags: serde_json::from_str(&tags_json).map_err(|e| conversion_error(8, e))?,
        global: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        deleted_at: row.get(12)?,
        rev: row.get(13)?,
        device_id: row.get(14)?,
    })
}

fn map_anchor(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnchorRow> {
    let locator_json: String = row.get(2)?;
    let rects_json: Option<String> = row.get(12)?;
    Ok(AnchorRow {
        annotation_id: row.get(0)?,
        scheme: row.get(1)?,
        primary_locator: serde_json::from_str(&locator_json).map_err(|e| conversion_error(2, e))?,
        doc_id: row.get(3)?,
        section_id: row.get(4)?,
        block_id: row.get(5)?,
        exact_text: row.get(6)?,
        prefix_text: row.get(7)?,
        suffix_text: row.get(8)?,
        start_offset: row.get(9)?,
        end_offset: row.get(10)?,
        doc_fingerprint: row.get(11)?,
        rects: rects_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|e| conversion_error(12, e))?,
        page: row.get(13)?,
        confidence: row.get(14)?,
        anchor_state: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn map_reading_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReadingStateRow> {
    let locator_json: String = row.get(3)?;
    let furthest_json: Option<String> = row.get(5)?;
    Ok(ReadingStateRow {
        publication_id: row.get(0)?,
        edition_id: row.get(1)?,
        device_id: row.get(2)?,
        locator: serde_json::from_str(&locator_json).map_err(|e| conversion_error(3, e))?,
        progress: row.get(4)?,
        furthest: furthest_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|e| conversion_error(5, e))?,
        updated_at: row.get(6)?,
        hlc: row.get(7)?,
    })
}

// ---------------------------------------------------------------------------
// Edition / BookFile / SpineItem / NavNode / TextBlock repositories
// ---------------------------------------------------------------------------

/// Access to `edition`.
#[derive(Debug)]
pub struct Editions<'a> {
    connection: &'a Connection,
}

impl<'a> Editions<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &EditionRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO edition(
               id, publication_id, kind, language, source_edition_id, file_id,
               title, metadata_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               kind=excluded.kind, language=excluded.language,
               source_edition_id=excluded.source_edition_id,
               file_id=excluded.file_id, title=excluded.title,
               metadata_json=excluded.metadata_json, updated_at=excluded.updated_at",
            params![
                row.id, row.publication_id, row.kind, row.language,
                row.source_edition_id, row.file_id, row.title,
                serde_json::to_string(&row.metadata).map_err(StorageError::Json)?,
                row.created_at, row.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<EditionRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, publication_id, kind, language, source_edition_id, file_id,
                    title, metadata_json, created_at, updated_at
             FROM edition WHERE id = ?1",
        )?;
        Ok(stmt.query_row([id], map_edition).optional()?)
    }

    pub fn list_for_publication(&self, publication_id: &str) -> Result<Vec<EditionRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, publication_id, kind, language, source_edition_id, file_id,
                    title, metadata_json, created_at, updated_at
             FROM edition WHERE publication_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map([publication_id], map_edition)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

/// Access to `book_file`.
#[derive(Debug)]
pub struct BookFiles<'a> {
    connection: &'a Connection,
}

impl<'a> BookFiles<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &BookFileRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO book_file(
               id, publication_id, sha256, partial_md5, byte_size, format,
               local_path, blob_id, imported_at, source_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               publication_id=excluded.publication_id, sha256=excluded.sha256,
               partial_md5=excluded.partial_md5, byte_size=excluded.byte_size,
               format=excluded.format, local_path=excluded.local_path,
               blob_id=excluded.blob_id, imported_at=excluded.imported_at,
               source_json=excluded.source_json",
            params![
                row.id, row.publication_id, row.sha256, row.partial_md5,
                row.byte_size, row.format, row.local_path, row.blob_id, row.imported_at,
                serde_json::to_string(&row.source).map_err(StorageError::Json)?,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<BookFileRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, publication_id, sha256, partial_md5, byte_size, format,
                    local_path, blob_id, imported_at, source_json
             FROM book_file WHERE id = ?1",
        )?;
        Ok(stmt.query_row([id], map_book_file).optional()?)
    }

    pub fn find_by_sha256(&self, sha256: &str) -> Result<Option<BookFileRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, publication_id, sha256, partial_md5, byte_size, format,
                    local_path, blob_id, imported_at, source_json
             FROM book_file WHERE sha256 = ?1",
        )?;
        Ok(stmt.query_row([sha256], map_book_file).optional()?)
    }
}

/// Access to `spine_item`.
#[derive(Debug)]
pub struct SpineItems<'a> {
    connection: &'a Connection,
}

impl<'a> SpineItems<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &SpineItemRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO spine_item(
               id, edition_id, ordinal, href, media_type, linear, properties_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               edition_id=excluded.edition_id, ordinal=excluded.ordinal,
               href=excluded.href, media_type=excluded.media_type,
               linear=excluded.linear, properties_json=excluded.properties_json",
            params![
                row.id, row.edition_id, row.ordinal, row.href, row.media_type,
                row.linear,
                serde_json::to_string(&row.properties).map_err(StorageError::Json)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_for_edition(&self, edition_id: &str) -> Result<Vec<SpineItemRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, edition_id, ordinal, href, media_type, linear, properties_json
             FROM spine_item WHERE edition_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map([edition_id], map_spine_item)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }

    pub fn delete_for_edition(&self, edition_id: &str) -> Result<usize, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM spine_item WHERE edition_id = ?1", [edition_id])?)
    }
}

/// Access to `nav_node`.
#[derive(Debug)]
pub struct NavNodes<'a> {
    connection: &'a Connection,
}

impl<'a> NavNodes<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &NavNodeRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO nav_node(
               id, edition_id, parent_id, ordinal, label, href, fragment
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               parent_id=excluded.parent_id, ordinal=excluded.ordinal,
               label=excluded.label, href=excluded.href, fragment=excluded.fragment",
            params![
                row.id, row.edition_id, row.parent_id, row.ordinal, row.label, row.href, row.fragment,
            ],
        )?;
        Ok(())
    }

    pub fn list_for_edition(&self, edition_id: &str) -> Result<Vec<NavNodeRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, edition_id, parent_id, ordinal, label, href, fragment
             FROM nav_node WHERE edition_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map([edition_id], map_nav_node)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }

    pub fn delete_for_edition(&self, edition_id: &str) -> Result<usize, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM nav_node WHERE edition_id = ?1", [edition_id])?)
    }
}

/// Access to `text_block`.
#[derive(Debug)]
pub struct TextBlocks<'a> {
    connection: &'a Connection,
}

impl<'a> TextBlocks<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &TextBlockRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO text_block(
               id, edition_id, spine_item_id, ordinal, block_kind, text,
               text_sha256, doc_fingerprint
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               spine_item_id=excluded.spine_item_id, ordinal=excluded.ordinal,
               block_kind=excluded.block_kind, text=excluded.text,
               text_sha256=excluded.text_sha256,
               doc_fingerprint=excluded.doc_fingerprint",
            params![
                row.id, row.edition_id, row.spine_item_id, row.ordinal,
                row.block_kind, row.text, row.text_sha256, row.doc_fingerprint,
            ],
        )?;
        Ok(())
    }

    pub fn list_for_spine_item(&self, spine_item_id: &str) -> Result<Vec<TextBlockRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, edition_id, spine_item_id, ordinal, block_kind, text,
                    text_sha256, doc_fingerprint
             FROM text_block WHERE spine_item_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map([spine_item_id], map_text_block)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }

    pub fn list_for_edition(&self, edition_id: &str) -> Result<Vec<TextBlockRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, edition_id, spine_item_id, ordinal, block_kind, text,
                    text_sha256, doc_fingerprint
             FROM text_block WHERE edition_id = ?1 ORDER BY spine_item_id, ordinal",
        )?;
        let rows = stmt.query_map([edition_id], map_text_block)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }

    pub fn delete_for_edition(&self, edition_id: &str) -> Result<usize, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM text_block WHERE edition_id = ?1", [edition_id])?)
    }
}

fn map_edition(row: &rusqlite::Row<'_>) -> rusqlite::Result<EditionRow> {
    let metadata_json: String = row.get(7)?;
    Ok(EditionRow {
        id: row.get(0)?,
        publication_id: row.get(1)?,
        kind: row.get(2)?,
        language: row.get(3)?,
        source_edition_id: row.get(4)?,
        file_id: row.get(5)?,
        title: row.get(6)?,
        metadata: serde_json::from_str(&metadata_json).map_err(|e| conversion_error(7, e))?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn map_book_file(row: &rusqlite::Row<'_>) -> rusqlite::Result<BookFileRow> {
    let source_json: String = row.get(9)?;
    Ok(BookFileRow {
        id: row.get(0)?,
        publication_id: row.get(1)?,
        sha256: row.get(2)?,
        partial_md5: row.get(3)?,
        byte_size: row.get(4)?,
        format: row.get(5)?,
        local_path: row.get(6)?,
        blob_id: row.get(7)?,
        imported_at: row.get(8)?,
        source: serde_json::from_str(&source_json).map_err(|e| conversion_error(9, e))?,
    })
}

fn map_spine_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpineItemRow> {
    let properties_json: String = row.get(6)?;
    Ok(SpineItemRow {
        id: row.get(0)?,
        edition_id: row.get(1)?,
        ordinal: row.get(2)?,
        href: row.get(3)?,
        media_type: row.get(4)?,
        linear: row.get::<_, i64>(5)? != 0,
        properties: serde_json::from_str(&properties_json).map_err(|e| conversion_error(6, e))?,
    })
}

fn map_nav_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<NavNodeRow> {
    Ok(NavNodeRow {
        id: row.get(0)?,
        edition_id: row.get(1)?,
        parent_id: row.get(2)?,
        ordinal: row.get(3)?,
        label: row.get(4)?,
        href: row.get(5)?,
        fragment: row.get(6)?,
    })
}

fn map_text_block(row: &rusqlite::Row<'_>) -> rusqlite::Result<TextBlockRow> {
    Ok(TextBlockRow {
        id: row.get(0)?,
        edition_id: row.get(1)?,
        spine_item_id: row.get(2)?,
        ordinal: row.get(3)?,
        block_kind: row.get(4)?,
        text: row.get(5)?,
        text_sha256: row.get(6)?,
        doc_fingerprint: row.get(7)?,
    })
}

// ---------------------------------------------------------------------------
// Remaining repositories: job, reading_session, stat_daily, glossary, ai,
// sources, plugins. All marked "表已建仓储未实现" in the progress report.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Jobs<'a> {
    connection: &'a Connection,
}

impl<'a> Jobs<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &JobRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO job(
               id, kind, state, input_json, progress, output_json, error_json,
               created_at, updated_at, started_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
               state=excluded.state, progress=excluded.progress,
               output_json=excluded.output_json, error_json=excluded.error_json,
               updated_at=excluded.updated_at, started_at=excluded.started_at,
               finished_at=excluded.finished_at",
            params![
                row.id, row.kind, row.state,
                serde_json::to_string(&row.input).map_err(StorageError::Json)?,
                row.progress,
                serde_json::to_string(&row.output).map_err(StorageError::Json)?,
                serde_json::to_string(&row.error).map_err(StorageError::Json)?,
                row.created_at, row.updated_at, row.started_at, row.finished_at,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<JobRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, kind, state, input_json, progress, output_json, error_json,
                    created_at, updated_at, started_at, finished_at
             FROM job WHERE id = ?1",
        )?;
        Ok(stmt.query_row([id], map_job).optional()?)
    }

    pub fn list_by_state(&self, state: &str, limit: usize) -> Result<Vec<JobRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, kind, state, input_json, progress, output_json, error_json,
                    created_at, updated_at, started_at, finished_at
             FROM job WHERE state = ?1 ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![state, limit as i64], map_job)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

#[derive(Debug)]
pub struct ReadingSessions<'a> {
    connection: &'a Connection,
}

impl<'a> ReadingSessions<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &ReadingSessionRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO reading_session(
               id, publication_id, edition_id, device_id, started_at, ended_at,
               duration_seconds, pages, words, locator_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               ended_at=excluded.ended_at, duration_seconds=excluded.duration_seconds,
               pages=excluded.pages, words=excluded.words, locator_json=excluded.locator_json",
            params![
                row.id, row.publication_id, row.edition_id, row.device_id,
                row.started_at, row.ended_at, row.duration_seconds, row.pages, row.words,
                serde_json::to_string(&row.locator).map_err(StorageError::Json)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_for_publication(&self, publication_id: &str, limit: usize) -> Result<Vec<ReadingSessionRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, publication_id, edition_id, device_id, started_at, ended_at,
                    duration_seconds, pages, words, locator_json
             FROM reading_session WHERE publication_id = ?1 ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![publication_id, limit as i64], map_session)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

#[derive(Debug)]
pub struct StatDailies<'a> {
    connection: &'a Connection,
}

impl<'a> StatDailies<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn upsert(&self, row: &StatDailyRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO stat_daily(
               id, device_id, day, publication_id, duration_seconds, pages, words, sessions, hlc
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               duration_seconds=stat_daily.duration_seconds + excluded.duration_seconds,
               pages=stat_daily.pages + excluded.pages,
               words=stat_daily.words + excluded.words,
               sessions=stat_daily.sessions + excluded.sessions",
            params![
                row.id, row.device_id, row.day, row.publication_id,
                row.duration_seconds, row.pages, row.words, row.sessions, row.hlc,
            ],
        )?;
        Ok(())
    }

    pub fn list_for_device(&self, device_id: &str, from_day: &str, to_day: &str) -> Result<Vec<StatDailyRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, device_id, day, publication_id, duration_seconds, pages, words, sessions, hlc
             FROM stat_daily WHERE device_id = ?1 AND day >= ?2 AND day <= ?3 ORDER BY day",
        )?;
        let rows = stmt.query_map(params![device_id, from_day, to_day], map_stat_daily)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

#[derive(Debug)]
pub struct GlossaryTerms<'a> {
    connection: &'a Connection,
}

impl<'a> GlossaryTerms<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &GlossaryTermRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO glossary_term(
               id, publication_id, edition_id, source_term, target_term, term_type,
               note, status, hlc, deleted_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               target_term=excluded.target_term, term_type=excluded.term_type,
               note=excluded.note, status=excluded.status, hlc=excluded.hlc,
               deleted_at=excluded.deleted_at",
            params![
                row.id, row.publication_id, row.edition_id, row.source_term,
                row.target_term, row.term_type, row.note, row.status, row.hlc, row.deleted_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_for_publication(&self, publication_id: &str) -> Result<Vec<GlossaryTermRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, publication_id, edition_id, source_term, target_term, term_type,
                    note, status, hlc, deleted_at
             FROM glossary_term WHERE publication_id = ?1 AND deleted_at IS NULL
             ORDER BY source_term",
        )?;
        let rows = stmt.query_map([publication_id], map_glossary)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

#[derive(Debug)]
pub struct AiRuns<'a> {
    connection: &'a Connection,
}

impl<'a> AiRuns<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &AiRunRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO ai_run(
               id, capability, provider_id, model, input_hash, cache_key,
               usage_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               usage_json=excluded.usage_json",
            params![
                row.id, row.capability, row.provider_id, row.model,
                row.input_hash, row.cache_key,
                serde_json::to_string(&row.usage).map_err(StorageError::Json)?,
                row.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn find_by_cache(&self, cache_key: &str) -> Result<Option<AiRunRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, capability, provider_id, model, input_hash, cache_key, usage_json, created_at
             FROM ai_run WHERE cache_key = ?1",
        )?;
        Ok(stmt.query_row([cache_key], map_ai_run).optional()?)
    }
}

#[derive(Debug)]
pub struct Sources<'a> {
    connection: &'a Connection,
}

impl<'a> Sources<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &SourceRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO source(
               id, kind, name, enabled, config_json, secrets_ref,
               created_at, updated_at, deleted_at, rev
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               kind=excluded.kind, name=excluded.name, enabled=excluded.enabled,
               config_json=excluded.config_json, secrets_ref=excluded.secrets_ref,
               updated_at=excluded.updated_at, deleted_at=excluded.deleted_at,
               rev=excluded.rev",
            params![
                row.id, row.kind, row.name, row.enabled,
                serde_json::to_string(&row.config).map_err(StorageError::Json)?,
                row.secrets_ref, row.created_at, row.updated_at, row.deleted_at, row.rev,
            ],
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<SourceRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, kind, name, enabled, config_json, secrets_ref,
                    created_at, updated_at, deleted_at, rev
             FROM source WHERE deleted_at IS NULL ORDER BY name",
        )?;
        let rows = stmt.query_map([], map_source)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

#[derive(Debug)]
pub struct SourceRules<'a> {
    connection: &'a Connection,
}

impl<'a> SourceRules<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &SourceRuleRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO source_rule(source_id, interface, rule_json, compat_level)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source_id, interface) DO UPDATE SET
               rule_json=excluded.rule_json, compat_level=excluded.compat_level",
            params![
                row.source_id, row.interface_,
                serde_json::to_string(&row.rule).map_err(StorageError::Json)?,
                row.compat_level,
            ],
        )?;
        Ok(())
    }

    pub fn list_for_source(&self, source_id: &str) -> Result<Vec<SourceRuleRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT source_id, interface, rule_json, compat_level
             FROM source_rule WHERE source_id = ?1",
        )?;
        let rows = stmt.query_map([source_id], map_source_rule)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

#[derive(Debug)]
pub struct Plugins<'a> {
    connection: &'a Connection,
}

impl<'a> Plugins<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, row: &PluginRow) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO plugin(
               id, version, runtime, manifest_json, permissions_json, enabled,
               signature, installed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               version=excluded.version, runtime=excluded.runtime,
               manifest_json=excluded.manifest_json,
               permissions_json=excluded.permissions_json, enabled=excluded.enabled,
               signature=excluded.signature",
            params![
                row.id, row.version, row.runtime,
                serde_json::to_string(&row.manifest).map_err(StorageError::Json)?,
                serde_json::to_string(&row.permissions).map_err(StorageError::Json)?,
                row.enabled, row.signature, row.installed_at,
            ],
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<PluginRow>, StorageError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, version, runtime, manifest_json, permissions_json, enabled,
                    signature, installed_at
             FROM plugin ORDER BY id",
        )?;
        let rows = stmt.query_map([], map_plugin)?;
        rows.map(|r| r.map_err(StorageError::from)).collect()
    }
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRow> {
    let input_json: String = row.get(3)?;
    let output_json: String = row.get(5)?;
    let error_json: String = row.get(6)?;
    Ok(JobRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        state: row.get(2)?,
        input: serde_json::from_str(&input_json).map_err(|e| conversion_error(3, e))?,
        progress: row.get(4)?,
        output: serde_json::from_str(&output_json).map_err(|e| conversion_error(5, e))?,
        error: serde_json::from_str(&error_json).map_err(|e| conversion_error(6, e))?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
    })
}

fn map_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReadingSessionRow> {
    let locator_json: String = row.get(9)?;
    Ok(ReadingSessionRow {
        id: row.get(0)?,
        publication_id: row.get(1)?,
        edition_id: row.get(2)?,
        device_id: row.get(3)?,
        started_at: row.get(4)?,
        ended_at: row.get(5)?,
        duration_seconds: row.get(6)?,
        pages: row.get(7)?,
        words: row.get(8)?,
        locator: serde_json::from_str(&locator_json).map_err(|e| conversion_error(9, e))?,
    })
}

fn map_stat_daily(row: &rusqlite::Row<'_>) -> rusqlite::Result<StatDailyRow> {
    Ok(StatDailyRow {
        id: row.get(0)?,
        device_id: row.get(1)?,
        day: row.get(2)?,
        publication_id: row.get(3)?,
        duration_seconds: row.get(4)?,
        pages: row.get(5)?,
        words: row.get(6)?,
        sessions: row.get(7)?,
        hlc: row.get(8)?,
    })
}

fn map_glossary(row: &rusqlite::Row<'_>) -> rusqlite::Result<GlossaryTermRow> {
    Ok(GlossaryTermRow {
        id: row.get(0)?,
        publication_id: row.get(1)?,
        edition_id: row.get(2)?,
        source_term: row.get(3)?,
        target_term: row.get(4)?,
        term_type: row.get(5)?,
        note: row.get(6)?,
        status: row.get(7)?,
        hlc: row.get(8)?,
        deleted_at: row.get(9)?,
    })
}

fn map_ai_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiRunRow> {
    let usage_json: String = row.get(6)?;
    Ok(AiRunRow {
        id: row.get(0)?,
        capability: row.get(1)?,
        provider_id: row.get(2)?,
        model: row.get(3)?,
        input_hash: row.get(4)?,
        cache_key: row.get(5)?,
        usage: serde_json::from_str(&usage_json).map_err(|e| conversion_error(6, e))?,
        created_at: row.get(7)?,
    })
}

fn map_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRow> {
    let config_json: String = row.get(4)?;
    Ok(SourceRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        config: serde_json::from_str(&config_json).map_err(|e| conversion_error(4, e))?,
        secrets_ref: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        deleted_at: row.get(8)?,
        rev: row.get(9)?,
    })
}

fn map_source_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRuleRow> {
    let rule_json: String = row.get(2)?;
    Ok(SourceRuleRow {
        source_id: row.get(0)?,
        interface_: row.get(1)?,
        rule: serde_json::from_str(&rule_json).map_err(|e| conversion_error(2, e))?,
        compat_level: row.get(3)?,
    })
}

fn map_plugin(row: &rusqlite::Row<'_>) -> rusqlite::Result<PluginRow> {
    let manifest_json: String = row.get(3)?;
    let perms_json: String = row.get(4)?;
    Ok(PluginRow {
        id: row.get(0)?,
        version: row.get(1)?,
        runtime: row.get(2)?,
        manifest: serde_json::from_str(&manifest_json).map_err(|e| conversion_error(3, e))?,
        permissions: serde_json::from_str(&perms_json).map_err(|e| conversion_error(4, e))?,
        enabled: row.get::<_, i64>(5)? != 0,
        signature: row.get(6)?,
        installed_at: row.get(7)?,
    })
}
