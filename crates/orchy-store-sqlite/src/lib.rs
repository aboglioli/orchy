#![allow(clippy::collapsible_if)]

mod agent;
mod api_key;
mod edge;
mod events;
mod knowledge;
mod membership;
mod message;
mod namespace;
mod organization;
mod project;
mod resource_lock;
mod task;
mod user;

use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use std::sync::Mutex;

use orchy_core::error::{Error, Result};

pub use agent::SqliteAgentStore;
pub use api_key::SqliteApiKeyStore;
pub use edge::SqliteEdgeStore;
pub use events::{SqliteEventQuery, SqliteEventWriter};
pub use knowledge::SqliteKnowledgeStore;
pub use membership::SqliteOrgMembershipStore;
pub use message::SqliteMessageStore;
pub use namespace::SqliteNamespaceStore;
pub use organization::SqliteOrganizationStore;
pub use project::SqliteProjectStore;
pub use resource_lock::SqliteLockStore;
pub use task::SqliteTaskStore;
pub use user::SqliteUserStore;

pub type SqliteConn = Arc<Mutex<Connection>>;

pub struct SqliteDatabase {
    conn: SqliteConn,
}

impl SqliteDatabase {
    pub fn new(path: &str, embedding_dimensions: Option<u32>) -> Result<Self> {
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = if path == ":memory:" {
            Connection::open_in_memory()
        } else {
            Connection::open(path)
        }
        .map_err(|e| Error::Store(e.to_string()))?;

        let _: String = conn
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
            .map_err(|e| Error::Store(e.to_string()))?;

        Self::init_vec0_table(&conn, embedding_dimensions)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn conn(&self) -> SqliteConn {
        self.conn.clone()
    }

    fn init_vec0_table(conn: &Connection, embedding_dimensions: Option<u32>) -> Result<()> {
        let Some(dims) = embedding_dimensions else {
            return Ok(());
        };
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_vec USING vec0(embedding float[{dims}])"
        ))
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    pub fn run_migrations(&self, dir: &Path) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
            )",
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| Error::Store(format!("cannot read migrations dir: {e}")))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "sql")
                    .unwrap_or(false)
            })
            .collect();
        files.sort_by_key(|e| e.file_name());

        for entry in files {
            let filename = entry.file_name().to_string_lossy().to_string();

            let applied: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM schema_migrations WHERE version = ?1",
                    rusqlite::params![&filename],
                    |row| row.get(0),
                )
                .map_err(|e| Error::Store(e.to_string()))?;

            if applied {
                continue;
            }

            let sql = std::fs::read_to_string(entry.path())
                .map_err(|e| Error::Store(format!("cannot read {filename}: {e}")))?;

            let tx = conn
                .unchecked_transaction()
                .map_err(|e| Error::Store(format!("migration {filename} tx begin: {e}")))?;
            match tx.execute_batch(&sql) {
                Ok(()) => {}
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("duplicate column name")
                        || err_msg.contains("already exists")
                        || err_msg.contains("UNIQUE constraint failed")
                        || err_msg.contains("no such column")
                        || err_msg.contains("no such table")
                    {
                        tx.rollback().map_err(|e2| Error::Store(e2.to_string()))?;
                        conn.execute(
                            "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                            rusqlite::params![&filename, chrono::Utc::now().to_rfc3339()],
                        )
                        .map_err(|e2| Error::Store(e2.to_string()))?;
                        continue;
                    }
                    return Err(Error::Store(format!("migration {filename} failed: {e}")));
                }
            }
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![&filename, chrono::Utc::now().to_rfc3339()],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
            tx.commit()
                .map_err(|e| Error::Store(format!("migration {filename} commit: {e}")))?;
        }

        Ok(())
    }

    pub fn migrations_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/sqlite")
    }
}

pub(crate) fn decode_json<T: serde::de::DeserializeOwned>(
    raw: &str,
    col: &str,
) -> std::result::Result<T, rusqlite::Error> {
    serde_json::from_str(raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("column {col}: {e}"),
            )),
        )
    })
}

pub(crate) fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub(crate) fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_dir_exists() {
        let dir = SqliteDatabase::migrations_dir();
        assert!(dir.exists(), "migrations dir must exist: {}", dir.display());
    }

    #[test]
    fn run_migrations_on_fresh_db() {
        let backend = SqliteDatabase::new(":memory:", None).unwrap();
        backend
            .run_migrations(&SqliteDatabase::migrations_dir())
            .unwrap();
    }
}
