//! Database opening, configuration and capability reporting.

use std::path::Path;

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations;

/// Optional SQLite features we probe instead of assuming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Whether FTS5 is available for local full-text search.
    pub fts5: bool,
}

/// The local database.
#[derive(Debug)]
pub struct Database {
    connection: Connection,
    capabilities: Capabilities,
}

impl Database {
    /// Open (or create) a database file and apply migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    /// Open an in-memory database, used by tests.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, StorageError> {
        configure(&connection)?;
        migrations::apply(&mut connection)?;
        let capabilities = probe_capabilities(&connection)?;
        Ok(Self {
            connection,
            capabilities,
        })
    }

    /// The underlying connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Mutable access, for transactions.
    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    /// Schema version currently stored.
    pub fn schema_version(&self) -> Result<u32, StorageError> {
        let version: u32 = self.connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migration",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }

    /// Probed capabilities of the linked SQLite build.
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
}

fn configure(connection: &Connection) -> Result<(), StorageError> {
    // WAL keeps readers from blocking the writer; foreign keys are enforced
    // explicitly because SQLite does not enable them by default.
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;",
    )?;
    Ok(())
}

/// Create the FTS5 index when the bundled SQLite supports it.
///
/// The result is reported instead of assumed: a build without FTS5 must tell
/// the caller that full-text search is unavailable rather than silently
/// returning no results.
fn probe_capabilities(connection: &Connection) -> Result<Capabilities, StorageError> {
    let fts5 = match connection.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS text_fts USING fts5(
           text, block_id UNINDEXED, edition_id UNINDEXED, spine_item_id UNINDEXED,
           tokenize='unicode61'
         );",
    ) {
        Ok(()) => true,
        Err(error) => {
            let message = error.to_string();
            if message.contains("fts5") || message.contains("no such module") {
                false
            } else {
                return Err(StorageError::Sqlite(error));
            }
        }
    };
    Ok(Capabilities { fts5 })
}
