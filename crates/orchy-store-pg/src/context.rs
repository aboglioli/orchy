use std::collections::HashMap;

use chrono::{DateTime, Utc};
use pgvector::Vector;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{ContextSnapshot, ContextStore, RestoreContextSnapshot, SnapshotId};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{PgBackend, parse_pg_vector_text};

impl ContextStore for PgBackend {
    async fn save(&self, snapshot: &ContextSnapshot) -> Result<()> {
        let vec_binding = snapshot.embedding().map(|e| Vector::from(e.to_vec()));
        let metadata_json = serde_json::to_value(snapshot.metadata()).unwrap();

        sqlx::query(
            "INSERT INTO contexts (id, project, agent_id, namespace, summary, embedding, embedding_model, embedding_dimensions, metadata, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE
             SET summary = EXCLUDED.summary,
                 embedding = EXCLUDED.embedding,
                 embedding_model = EXCLUDED.embedding_model,
                 embedding_dimensions = EXCLUDED.embedding_dimensions,
                 metadata = EXCLUDED.metadata",
        )
        .bind(snapshot.id().as_uuid())
        .bind(snapshot.project().to_string())
        .bind(snapshot.agent_id().as_uuid())
        .bind(snapshot.namespace().to_string())
        .bind(snapshot.summary())
        .bind(vec_binding.as_ref())
        .bind(snapshot.embedding_model())
        .bind(snapshot.embedding_dimensions().map(|d| d as i32))
        .bind(&metadata_json)
        .bind(snapshot.created_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_latest(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        let row = sqlx::query(
            "SELECT id, project, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at
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
            "SELECT id, project, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at
             FROM contexts WHERE 1=1",
        );
        let mut param_idx = 1u32;
        let mut agent_uuid: Option<Uuid> = None;
        let ns_str = namespace.to_string();
        let mut bind_ns = false;

        if !namespace.is_root() {
            sql.push_str(&format!(
                " AND (namespace = ${param_idx} OR namespace LIKE ${param_idx} || '/%')"
            ));
            bind_ns = true;
            param_idx += 1;
        }

        if let Some(a) = agent {
            sql.push_str(&format!(" AND agent_id = ${param_idx}"));
            agent_uuid = Some(*a.as_uuid());
        }

        let mut query = sqlx::query(&sql);
        if bind_ns {
            query = query.bind(&ns_str);
        }
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
            "SELECT id, project, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at
             FROM contexts
             WHERE to_tsvector('english', summary) @@ plainto_tsquery('english', $1)",
        );
        let mut param_idx = 2u32;
        let ns_str = namespace.to_string();
        let mut agent_uuid: Option<Uuid> = None;
        let mut bind_ns = false;

        if !namespace.is_root() {
            sql.push_str(&format!(
                " AND (namespace = ${param_idx} OR namespace LIKE ${param_idx} || '/%')"
            ));
            bind_ns = true;
            param_idx += 1;
        }

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
        if bind_ns {
            q = q.bind(&ns_str);
        }
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
    let project: String = row.get("project");
    let agent_id: Uuid = row.get("agent_id");
    let namespace: String = row.get("namespace");
    let summary: String = row.get("summary");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let metadata: serde_json::Value = row.get("metadata");
    let created_at: DateTime<Utc> = row.get("created_at");

    ContextSnapshot::restore(RestoreContextSnapshot {
        id: SnapshotId::from_uuid(id),
        project: ProjectId::try_from(project).expect("invalid project in database"),
        agent_id: AgentId::from_uuid(agent_id),
        namespace: Namespace::try_from(namespace).expect("invalid namespace in database"),
        summary,
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        metadata: serde_json::from_value(metadata).unwrap_or_else(|_| HashMap::new()),
        created_at,
    })
}

