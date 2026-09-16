//! Storage errors.

/// Anything that can go wrong while talking to the local database.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// SQLite reported an error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored JSON document could not be decoded.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// The database was written by a newer build.
    #[error("database schema is at version {found}, this build supports up to {supported}")]
    SchemaTooNew {
        /// Version found on disk.
        found: u32,
        /// Highest version this build knows.
        supported: u32,
    },
    /// A required SQLite capability is missing.
    #[error("sqlite is missing the required capability {0}")]
    MissingCapability(String),
    /// A row was expected but not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// The caller passed an invalid value.
    #[error("invalid value: {0}")]
    Invalid(String),
}

