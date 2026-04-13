mod agent;
mod context;
mod memory;
mod message;
mod project;
mod skill;
mod task;

use rusqlite::Connection;
use std::sync::Mutex;

use orchy_core::error::{Error, Result};

pub struct SqliteBackend {
    conn: Mutex<Connection>,
}

struct Migration {
    version: &'static str,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "20260412-160000",
        name: "initial_schema",
        sql: include_str!("../../../migrations/sqlite/20260412-160000_initial_schema.sql"),
    },
    Migration {
        version: "20260412-235000",
        name: "notes_projects_reconnect",
        sql: include_str!(
            "../../../migrations/sqlite/20260412-235000_notes_projects_reconnect.sql"
        ),
    },
];

impl SqliteBackend {
    pub fn new(path: &str, embedding_dimensions: Option<u32>) -> Result<Self> {
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

        Self::run_migrations(&conn)?;
        Self::init_virtual_tables(&conn, embedding_dimensions)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            )",
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        for migration in MIGRATIONS {
            let applied: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM schema_migrations WHERE version = ?1",
                    rusqlite::params![migration.version],
                    |row| row.get(0),
                )
                .map_err(|e| Error::Store(e.to_string()))?;

            if !applied {
                conn.execute_batch(migration.sql).map_err(|e| {
                    Error::Store(format!("migration {} failed: {e}", migration.name))
                })?;

                conn.execute(
                    "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        migration.version,
                        migration.name,
                        chrono::Utc::now().to_rfc3339(),
                    ],
                )
                .map_err(|e| Error::Store(e.to_string()))?;
            }
        }

        Ok(())
    }

    fn init_virtual_tables(conn: &Connection, embedding_dimensions: Option<u32>) -> Result<()> {
        let fts_statements = [
            "CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(namespace, key, value, content='memory', content_rowid='rowid')",
            "CREATE VIRTUAL TABLE IF NOT EXISTS contexts_fts USING fts5(namespace, summary, content='contexts', content_rowid='rowid')",
        ];
        for fts in &fts_statements {
            let mut stmt = conn.prepare(fts).map_err(|e| Error::Store(e.to_string()))?;
            let _ = stmt.raw_execute();
        }

        if let Some(dims) = embedding_dimensions {
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(
                    rowid INTEGER PRIMARY KEY,
                    embedding FLOAT[{dims}]
                )"
            ))
            .map_err(|e| Error::Store(e.to_string()))?;

            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS contexts_vec USING vec0(
                    rowid INTEGER PRIMARY KEY,
                    embedding FLOAT[{dims}]
                )"
            ))
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        Ok(())
    }
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
