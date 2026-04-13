#![allow(clippy::collapsible_if)]

mod agent;
mod context;
mod memory;
mod message;
mod namespace;
mod project;
mod skill;
mod task;

use rusqlite::Connection;
use std::sync::Mutex;

use orchy_core::error::{Error, Result};

pub struct SqliteBackend {
    conn: Mutex<Connection>,
}

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

        Self::init_virtual_tables(&conn, embedding_dimensions)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn apply_schema(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute_batch(include_str!(
            "../../../migrations/sqlite/20260412-160000_initial_schema.sql"
        ))
        .map_err(|e| Error::Store(e.to_string()))
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
