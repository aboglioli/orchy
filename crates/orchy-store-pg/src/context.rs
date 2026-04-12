use std::collections::HashMap;

use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::entities::{ContextSnapshot, CreateSnapshot};
use orchy_core::error::{Error, Result};
use orchy_core::store::ContextStore;
use orchy_core::value_objects::{AgentId, Namespace, SnapshotId};

use crate::PgBackend;

impl ContextStore for PgBackend {
    async fn save(&self, cmd: CreateSnapshot) -> Result<ContextSnapshot> {
        let snapshot = ContextSnapshot {
            id: SnapshotId::new(),
            agent_id: cmd.agent_id,
            namespace: cmd.namespace,
            summary: cmd.summary,
            embedding: cmd.embedding,
            embedding_model: cmd.embedding_model,
            embedding_dimensions: cmd.embedding_dimensions,
            metadata: cmd.metadata,
            created_at: Utc::now(),
        };

        let vec_binding = snapshot.embedding.as_ref().map(|e| Vector::from(e.clone()));
        let metadata_json = serde_json::to_value(&snapshot.metadata).unwrap();

        sqlx::query(
            "INSERT INTO contexts (id, agent_id, namespace, summary, embedding, embedding_model, embedding_dimensions, metadata, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(snapshot.id.as_uuid())
        .bind(snapshot.agent_id.as_uuid())
        .bind(snapshot.namespace.to_string())
        .bind(&snapshot.summary)
        .bind(vec_binding.as_ref())
        .bind(&snapshot.embedding_model)
        .bind(snapshot.embedding_dimensions.map(|d| d as i32))
        .bind(&metadata_json)
        .bind(snapshot.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(snapshot)
    }

    async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        let row = sqlx::query(
            "SELECT id, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at
             FROM contexts WHERE agent_id = $1
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(agent.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_context(&r)))
    }

    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        let mut sql = String::from(
            "SELECT id, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at
             FROM contexts WHERE (namespace = $1 OR namespace LIKE $1 || '/%')",
        );
        let param_idx = 2u32;
        let mut agent_uuid: Option<Uuid> = None;
        let ns_str = namespace.to_string();

        if let Some(a) = agent {
            sql.push_str(&format!(" AND agent_id = ${param_idx}"));
            agent_uuid = Some(*a.as_uuid());
        }

        let mut query = sqlx::query(&sql);
        query = query.bind(&ns_str);
        if let Some(ref id) = agent_uuid {
            query = query.bind(id);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_context).collect())
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        let mut sql = String::from(
            "SELECT id, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at
             FROM contexts
             WHERE to_tsvector('english', summary) @@ plainto_tsquery('english', $1)
               AND (namespace = $2 OR namespace LIKE $2 || '/%')",
        );
        let mut param_idx = 3u32;
        let ns_str = namespace.to_string();
        let mut agent_uuid: Option<Uuid> = None;

        if let Some(a) = agent_id {
            sql.push_str(&format!(" AND agent_id = ${param_idx}"));
            agent_uuid = Some(*a.as_uuid());
            param_idx += 1;
        }

        sql.push_str(&format!(
            " ORDER BY ts_rank(to_tsvector('english', summary), plainto_tsquery('english', $1)) DESC LIMIT ${param_idx}"
        ));

        let mut q = sqlx::query(&sql);
        q = q.bind(query);
        q = q.bind(&ns_str);
        if let Some(ref id) = agent_uuid {
            q = q.bind(id);
        }
        q = q.bind(limit as i64);

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_context).collect())
    }
}

fn row_to_context(row: &sqlx::postgres::PgRow) -> ContextSnapshot {
    let id: Uuid = row.get("id");
    let agent_id: Uuid = row.get("agent_id");
    let namespace: String = row.get("namespace");
    let summary: String = row.get("summary");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let metadata: serde_json::Value = row.get("metadata");
    let created_at: DateTime<Utc> = row.get("created_at");

    ContextSnapshot {
        id: SnapshotId::from_uuid(id),
        agent_id: AgentId::from_uuid(agent_id),
        namespace: Namespace::try_from(namespace).expect("invalid namespace in database"),
        summary,
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        metadata: serde_json::from_value(metadata).unwrap_or_else(|_| HashMap::new()),
        created_at,
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
