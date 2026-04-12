mod agent;
mod context;
mod memory;
mod message;
mod skill;
mod store_impl;
mod task;

use rusqlite::Connection;
use std::sync::Mutex;

use orchy_core::error::{Error, Result};

pub struct SqliteBackend {
    conn: Mutex<Connection>,
}

impl SqliteBackend {
    pub fn new(path: &str, embedding_dimensions: Option<u32>) -> Result<Self> {
        // Register sqlite-vec extension BEFORE opening connection
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

        // journal_mode returns a result row, so use pragma_update_and_check
        let _: String = conn
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
            .map_err(|e| Error::Store(e.to_string()))?;

        Self::init_schema(&conn, embedding_dimensions)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn init_schema(conn: &Connection, embedding_dimensions: Option<u32>) -> Result<()> {
        let ddl_statements = [
            "CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                namespace TEXT NOT NULL,
                roles TEXT NOT NULL DEFAULT '[]',
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'online',
                last_heartbeat TEXT NOT NULL,
                connected_at TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}'
            )",
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                namespace TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                priority TEXT NOT NULL DEFAULT 'normal',
                assigned_roles TEXT NOT NULL DEFAULT '[]',
                claimed_by TEXT,
                claimed_at TEXT,
                depends_on TEXT NOT NULL DEFAULT '[]',
                result_summary TEXT,
                created_by TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE TABLE IF NOT EXISTS memory (
                namespace TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                embedding BLOB,
                embedding_model TEXT,
                embedding_dimensions INTEGER,
                written_by TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (namespace, key)
            )",
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                namespace TEXT NOT NULL,
                from_agent TEXT NOT NULL,
                to_target TEXT NOT NULL,
                body TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL
            )",
            "CREATE TABLE IF NOT EXISTS contexts (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                namespace TEXT NOT NULL,
                summary TEXT NOT NULL,
                embedding BLOB,
                embedding_model TEXT,
                embedding_dimensions INTEGER,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            )",
            "CREATE TABLE IF NOT EXISTS skills (
                namespace TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL,
                content TEXT NOT NULL,
                written_by TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (namespace, name)
            )",
        ];

        for ddl in &ddl_statements {
            conn.execute_batch(ddl)
                .map_err(|e| Error::Store(format!("{e}: {ddl}")))?;
        }

        // FTS5 virtual tables - use prepare+execute to handle quirks
        let fts_statements = [
            "CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(namespace, key, value, content='memory', content_rowid='rowid')",
            "CREATE VIRTUAL TABLE IF NOT EXISTS contexts_fts USING fts5(namespace, summary, content='contexts', content_rowid='rowid')",
        ];
        for fts in &fts_statements {
            let mut stmt = conn.prepare(fts).map_err(|e| Error::Store(e.to_string()))?;
            // FTS5 CREATE may return rows; drain them
            let _ = stmt.raw_execute();
        }

        if let Some(dims) = embedding_dimensions {
            conn.execute_batch(
                &format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(
                        rowid INTEGER PRIMARY KEY,
                        embedding FLOAT[{dims}]
                    )"
                ),
            )
            .map_err(|e| Error::Store(e.to_string()))?;

            conn.execute_batch(
                &format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS contexts_vec USING vec0(
                        rowid INTEGER PRIMARY KEY,
                        embedding FLOAT[{dims}]
                    )"
                ),
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        Ok(())
    }
}

/// Convert f32 slice to little-endian bytes for sqlite-vec.
pub(crate) fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert little-endian bytes back to f32 vec.
pub(crate) fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

