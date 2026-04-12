use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{MemoryEntry, MemoryFilter, MemoryStore, Version, WriteMemory};
use orchy_core::namespace::Namespace;

use crate::PgBackend;

impl MemoryStore for PgBackend {
    async fn write(&self, cmd: WriteMemory) -> Result<MemoryEntry> {
        let now = Utc::now();

        // Check if entry exists
        let existing = sqlx::query(
            "SELECT version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at
             FROM memory WHERE namespace = $1 AND key = $2",
        )
        .bind(cmd.namespace.to_string())
        .bind(&cmd.key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let entry = if let Some(row) = existing {
            let version: i64 = row.get("version");
            let existing_emb_str: Option<String> = row.get("embedding");
            let existing_model: Option<String> = row.get("embedding_model");
            let existing_dims: Option<i32> = row.get("embedding_dimensions");
            let existing_writer: Option<Uuid> = row.get("written_by");
            let created_at: DateTime<Utc> = row.get("created_at");

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

            // Parse existing embedding from text representation if needed
            let existing_embedding = existing_emb_str.and_then(|s| parse_pg_vector_text(&s));

            let embedding = cmd.embedding.or(existing_embedding);
            let embedding_model = cmd.embedding_model.or(existing_model);
            let embedding_dimensions = cmd.embedding_dimensions.or(existing_dims.map(|d| d as u32));
            let written_by = cmd
                .written_by
                .or_else(|| existing_writer.map(AgentId::from_uuid));

            let vec_binding = embedding.as_ref().map(|e| Vector::from(e.clone()));

            sqlx::query(
                "UPDATE memory SET value = $1, version = $2, embedding = $3, embedding_model = $4, embedding_dimensions = $5, written_by = $6, updated_at = $7
                 WHERE namespace = $8 AND key = $9",
            )
            .bind(&cmd.value)
            .bind(new_version.as_u64() as i64)
            .bind(vec_binding.as_ref())
            .bind(&embedding_model)
            .bind(embedding_dimensions.map(|d| d as i32))
            .bind(written_by.map(|a| *a.as_uuid()))
            .bind(now)
            .bind(cmd.namespace.to_string())
            .bind(&cmd.key)
            .execute(&self.pool)
            .await
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
            let vec_binding = cmd.embedding.as_ref().map(|e| Vector::from(e.clone()));

            sqlx::query(
                "INSERT INTO memory (namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(cmd.namespace.to_string())
            .bind(&cmd.key)
            .bind(&cmd.value)
            .bind(version.as_u64() as i64)
            .bind(vec_binding.as_ref())
            .bind(&cmd.embedding_model)
            .bind(cmd.embedding_dimensions.map(|d| d as i32))
            .bind(cmd.written_by.map(|a| *a.as_uuid()))
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

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
        let row = sqlx::query(
            "SELECT namespace, key, value, version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at, updated_at
             FROM memory WHERE namespace = $1 AND key = $2",
        )
        .bind(namespace.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_memory(&r)))
    }

    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        let mut sql = "SELECT namespace, key, value, version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at, updated_at FROM memory WHERE 1=1".to_string();
        let mut params: Vec<String> = Vec::new();
        let mut idx = 1u32;

        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(
                " AND (namespace = ${idx} OR namespace LIKE ${idx} || '/%')"
            ));
            params.push(ns.to_string());
            idx += 1;
        }
        if let Some(ref project) = filter.project {
            sql.push_str(&format!(
                " AND (namespace = ${idx} OR namespace LIKE ${idx} || '/%')"
            ));
            params.push(project.to_string());
        }

        let mut query = sqlx::query(&sql);
        for p in &params {
            query = query.bind(p);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_memory).collect())
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let rows = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT namespace, key, value, version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at, updated_at
                 FROM memory
                 WHERE to_tsvector('english', value) @@ plainto_tsquery('english', $1)
                   AND (namespace = $2 OR namespace LIKE $2 || '/%')
                 ORDER BY ts_rank(to_tsvector('english', value), plainto_tsquery('english', $1)) DESC
                 LIMIT $3",
            )
            .bind(query)
            .bind(ns.to_string())
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT namespace, key, value, version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at, updated_at
                 FROM memory
                 WHERE to_tsvector('english', value) @@ plainto_tsquery('english', $1)
                 ORDER BY ts_rank(to_tsvector('english', value), plainto_tsquery('english', $1)) DESC
                 LIMIT $2",
            )
            .bind(query)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        };

        Ok(rows.iter().map(row_to_memory).collect())
    }

    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM memory WHERE namespace = $1 AND key = $2")
            .bind(namespace.to_string())
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_memory(row: &sqlx::postgres::PgRow) -> MemoryEntry {
    let namespace: String = row.get("namespace");
    let key: String = row.get("key");
    let value: String = row.get("value");
    let version: i64 = row.get("version");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let written_by: Option<Uuid> = row.get("written_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    MemoryEntry {
        namespace: Namespace::try_from(namespace).unwrap(),
        key,
        value,
        version: Version::from(version as u64),
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        written_by: written_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    }
}

/// Parse pgvector text representation "[1,2,3]" into Vec<f32>
fn parse_pg_vector_text(s: &str) -> Option<Vec<f32>> {
    let trimmed = s.trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return None;
    }
    let result: std::result::Result<Vec<f32>, _> = trimmed
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect();
    result.ok()
}
