use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{MemoryEntry, MemoryFilter, MemoryStore, Version};
use orchy_core::namespace::Namespace;

use crate::PgBackend;

impl MemoryStore for PgBackend {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        let vec_binding = entry.embedding().map(|e| Vector::from(e.to_vec()));

        sqlx::query(
            "INSERT INTO memory (namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (namespace, key) DO UPDATE
             SET value = EXCLUDED.value,
                 version = EXCLUDED.version,
                 embedding = EXCLUDED.embedding,
                 embedding_model = EXCLUDED.embedding_model,
                 embedding_dimensions = EXCLUDED.embedding_dimensions,
                 written_by = EXCLUDED.written_by,
                 updated_at = EXCLUDED.updated_at",
        )
        .bind(entry.namespace().to_string())
        .bind(entry.key())
        .bind(entry.value())
        .bind(entry.version().as_u64() as i64)
        .bind(vec_binding.as_ref())
        .bind(entry.embedding_model())
        .bind(entry.embedding_dimensions().map(|d| d as i32))
        .bind(entry.written_by().map(|a| *a.as_uuid()))
        .bind(entry.created_at())
        .bind(entry.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_key(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
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

    MemoryEntry::restore(
        Namespace::try_from(namespace).unwrap(),
        key,
        value,
        Version::from(version as u64),
        embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions.map(|d| d as u32),
        written_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    )
}

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
