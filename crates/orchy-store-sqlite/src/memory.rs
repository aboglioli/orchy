use std::str::FromStr;

use chrono::{DateTime, Utc};

use orchy_core::entities::{MemoryEntry, MemoryFilter, WriteMemory};
use orchy_core::error::{Error, Result};
use orchy_core::store::MemoryStore;
use orchy_core::value_objects::{AgentId, Namespace, Version};

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};

impl MemoryStore for SqliteBackend {
    async fn write(&self, cmd: WriteMemory) -> Result<MemoryEntry> {
        let now = Utc::now();
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        // Check if entry exists
        let existing: Option<(i64, i64, Option<Vec<u8>>, Option<String>, Option<u32>, Option<String>, String)> = conn
            .query_row(
                "SELECT rowid, version, embedding, embedding_model, embedding_dimensions, written_by, created_at
                 FROM memory WHERE namespace = ?1 AND key = ?2",
                rusqlite::params![cmd.namespace.to_string(), cmd.key],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        let entry = if let Some((
            rowid,
            version,
            existing_emb,
            existing_model,
            existing_dims,
            existing_writer,
            created_at_str,
        )) = existing
        {
            let current_version = Version::from(version as u64);

            if let Some(expected) = cmd.expected_version {
                if current_version != expected {
                    return Err(Error::VersionMismatch {
                        expected: expected.as_u64(),
                        actual: current_version.as_u64(),
                    });
                }
            }

            let new_version = current_version.next();
            let embedding = cmd
                .embedding
                .or_else(|| existing_emb.as_ref().map(|b| bytes_to_embedding(b)));
            let embedding_model = cmd.embedding_model.or(existing_model);
            let embedding_dimensions = cmd.embedding_dimensions.or(existing_dims);
            let written_by = cmd
                .written_by
                .or_else(|| existing_writer.and_then(|s| AgentId::from_str(&s).ok()));
            let embedding_bytes = embedding.as_ref().map(|e| embedding_to_bytes(e));

            // Delete old FTS entry
            conn.execute(
                "INSERT INTO memory_fts(memory_fts, rowid, namespace, key, value) VALUES('delete', ?1, ?2, ?3, (SELECT value FROM memory WHERE namespace = ?2 AND key = ?3))",
                rusqlite::params![rowid, cmd.namespace.to_string(), cmd.key],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

            conn.execute(
                "UPDATE memory SET value = ?1, version = ?2, embedding = ?3, embedding_model = ?4, embedding_dimensions = ?5, written_by = ?6, updated_at = ?7
                 WHERE namespace = ?8 AND key = ?9",
                rusqlite::params![
                    cmd.value,
                    new_version.as_u64() as i64,
                    embedding_bytes,
                    embedding_model,
                    embedding_dimensions.map(|d| d as i64),
                    written_by.map(|a| a.to_string()),
                    now.to_rfc3339(),
                    cmd.namespace.to_string(),
                    cmd.key,
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

            // Insert new FTS entry
            conn.execute(
                "INSERT INTO memory_fts(rowid, namespace, key, value) VALUES(?1, ?2, ?3, ?4)",
                rusqlite::params![rowid, cmd.namespace.to_string(), cmd.key, cmd.value],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

            // Update vec table if embedding provided
            if let Some(ref emb_bytes) = embedding_bytes {
                // Try to update vec table; ignore errors if table doesn't exist
                let _ = conn.execute(
                    "DELETE FROM memory_vec WHERE rowid = ?1",
                    rusqlite::params![rowid],
                );
                let _ = conn.execute(
                    "INSERT INTO memory_vec(rowid, embedding) VALUES(?1, ?2)",
                    rusqlite::params![rowid, emb_bytes],
                );
            }

            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| Error::Store(e.to_string()))?;

            MemoryEntry {
                namespace: cmd.namespace,
                key: cmd.key,
                value: cmd.value,
                version: new_version,
                embedding,
                embedding_model,
                embedding_dimensions,
                written_by,
                created_at,
                updated_at: now,
            }
        } else {
            // New entry
            if let Some(expected) = cmd.expected_version {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: 0,
                });
            }

            let version = Version::initial();
            let embedding_bytes = cmd.embedding.as_ref().map(|e| embedding_to_bytes(e));

            conn.execute(
                "INSERT INTO memory (namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    cmd.namespace.to_string(),
                    cmd.key,
                    cmd.value,
                    version.as_u64() as i64,
                    embedding_bytes,
                    cmd.embedding_model,
                    cmd.embedding_dimensions.map(|d| d as i64),
                    cmd.written_by.map(|a| a.to_string()),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

            let rowid = conn.last_insert_rowid();

            // Insert FTS entry
            conn.execute(
                "INSERT INTO memory_fts(rowid, namespace, key, value) VALUES(?1, ?2, ?3, ?4)",
                rusqlite::params![rowid, cmd.namespace.to_string(), cmd.key, cmd.value],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

            // Insert vec entry if embedding provided
            if let Some(ref emb_bytes) = embedding_bytes {
                let _ = conn.execute(
                    "INSERT INTO memory_vec(rowid, embedding) VALUES(?1, ?2)",
                    rusqlite::params![rowid, emb_bytes],
                );
            }

            MemoryEntry {
                namespace: cmd.namespace,
                key: cmd.key,
                value: cmd.value,
                version,
                embedding: cmd.embedding,
                embedding_model: cmd.embedding_model,
                embedding_dimensions: cmd.embedding_dimensions,
                written_by: cmd.written_by,
                created_at: now,
                updated_at: now,
            }
        };

        Ok(entry)
    }

    async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at
                 FROM memory WHERE namespace = ?1 AND key = ?2",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![namespace.to_string(), key], row_to_memory)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = "SELECT namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at FROM memory WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }
        if let Some(ref project) = filter.project {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(project.to_string()));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let entries = stmt
            .query_map(param_refs.as_slice(), row_to_memory)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(entries)
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        // Use FTS5 for keyword search
        let mut sql = String::from(
            "SELECT m.namespace, m.key, m.value, m.version, m.embedding, m.embedding_model, m.embedding_dimensions, m.written_by, m.created_at, m.updated_at
             FROM memory m
             JOIN memory_fts ON memory_fts.rowid = m.rowid
             WHERE memory_fts MATCH ?1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        // Escape FTS5 special chars and create a simple query
        let fts_query = sanitize_fts_query(query);
        params.push(Box::new(fts_query));
        let mut idx = 2;

        if let Some(ns) = namespace {
            sql.push_str(&format!(
                " AND (m.namespace = ?{idx} OR m.namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }

        sql.push_str(&format!(" ORDER BY rank LIMIT ?{idx}"));
        params.push(Box::new(limit as i64));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let entries = stmt
            .query_map(param_refs.as_slice(), row_to_memory)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(entries)
    }

    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        // Get rowid for FTS/vec cleanup
        let rowid: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM memory WHERE namespace = ?1 AND key = ?2",
                rusqlite::params![namespace.to_string(), key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        if let Some(rowid) = rowid {
            // Delete FTS entry
            let _ = conn.execute(
                "INSERT INTO memory_fts(memory_fts, rowid, namespace, key, value) VALUES('delete', ?1, ?2, ?3, (SELECT value FROM memory WHERE namespace = ?2 AND key = ?3))",
                rusqlite::params![rowid, namespace.to_string(), key],
            );

            // Delete vec entry
            let _ = conn.execute(
                "DELETE FROM memory_vec WHERE rowid = ?1",
                rusqlite::params![rowid],
            );
        }

        conn.execute(
            "DELETE FROM memory WHERE namespace = ?1 AND key = ?2",
            rusqlite::params![namespace.to_string(), key],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryEntry> {
    let namespace_str: String = row.get(0)?;
    let key: String = row.get(1)?;
    let value: String = row.get(2)?;
    let version: i64 = row.get(3)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(4)?;
    let embedding_model: Option<String> = row.get(5)?;
    let embedding_dimensions: Option<i64> = row.get(6)?;
    let written_by_str: Option<String> = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let updated_at_str: String = row.get(9)?;

    Ok(MemoryEntry {
        namespace: Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        key,
        value,
        version: Version::from(version as u64),
        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        written_by: written_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
    })
}

/// Sanitize a user query for FTS5 by quoting it to avoid syntax errors.
fn sanitize_fts_query(query: &str) -> String {
    // Wrap each word in quotes so FTS5 treats them as literals
    query
        .split_whitespace()
        .map(|word| {
            // Escape any double quotes within the word
            let escaped = word.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

use rusqlite::OptionalExtension;
