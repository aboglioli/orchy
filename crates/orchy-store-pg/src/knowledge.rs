use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pgvector::Vector;
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeKind, KnowledgePath, KnowledgeStore,
    RestoreKnowledge, Version,
};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_events::io::Writer;

use crate::{
    PgBackend, decode_json_value, events::PgEventWriter, parse_namespace, parse_pg_vector_text,
    parse_project_id,
};

#[derive(Iden)]
#[allow(dead_code)]
enum KnowledgeEntries {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "path"]
    Path,
    #[iden = "kind"]
    KnowledgeKind,
    #[iden = "title"]
    Title,
    #[iden = "content"]
    Content,
    #[iden = "tags"]
    Tags,
    #[iden = "version"]
    Version,
    #[iden = "metadata"]
    Metadata,
    #[iden = "embedding_model"]
    EmbeddingModel,
    #[iden = "embedding_dimensions"]
    EmbeddingDimensions,
    #[iden = "valid_from"]
    ValidFrom,
    #[iden = "valid_until"]
    ValidUntil,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

const SELECT_COLUMNS: &str = "id, organization_id, project, namespace, path, kind, title, content, tags, version, metadata, embedding::text, embedding_model, embedding_dimensions, valid_from, valid_until, created_at, updated_at";

#[async_trait]
impl KnowledgeStore for PgBackend {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        let vec_binding = entry.embedding().map(|e| Vector::from(e.to_vec()));
        let tags_json = serde_json::to_value(entry.tags()).map_err(|e| {
            Error::Store(format!("failed to serialize knowledge_entries.tags: {e}"))
        })?;
        let metadata_json = serde_json::to_value(entry.metadata()).map_err(|e| {
            Error::Store(format!(
                "failed to serialize knowledge_entries.metadata: {e}"
            ))
        })?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        if let Some(pv) = entry.persisted_version() {
            let result = sqlx::query(
                "UPDATE knowledge_entries SET organization_id = $2, project = $3, namespace = $4, path = $5, kind = $6, title = $7, content = $8, tags = $9, version = $10, metadata = $11, embedding = $12, embedding_model = $13, embedding_dimensions = $14, valid_from = $15, valid_until = $16, updated_at = $17
                 WHERE id = $1 AND version = $18",
            )
            .bind(entry.id().as_uuid())
            .bind(entry.org_id().to_string())
            .bind(entry.project().map(|p| p.to_string()))
            .bind(entry.namespace().to_string())
            .bind(entry.path().as_str())
            .bind(entry.kind().to_string())
            .bind(entry.title())
            .bind(entry.content())
            .bind(&tags_json)
            .bind(entry.version().as_u64() as i64)
            .bind(&metadata_json)
            .bind(vec_binding.as_ref())
            .bind(entry.embedding_model())
            .bind(entry.embedding_dimensions().map(|d| d as i32))
            .bind(entry.valid_from())
            .bind(entry.valid_until())
            .bind(entry.updated_at())
            .bind(pv.as_u64() as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

            if result.rows_affected() == 0 {
                let stored_version: Option<i64> =
                    sqlx::query_scalar("SELECT version FROM knowledge_entries WHERE id = $1")
                        .bind(entry.id().as_uuid())
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(|e| Error::Store(e.to_string()))?;

                return Err(match stored_version {
                    Some(v) => Error::VersionMismatch {
                        expected: pv.as_u64(),
                        actual: v as u64,
                    },
                    None => Error::NotFound(format!("knowledge entry {}", entry.id())),
                });
            }
        } else {
            sqlx::query(
                "INSERT INTO knowledge_entries (id, organization_id, project, namespace, path, kind, title, content, tags, version, metadata, embedding, embedding_model, embedding_dimensions, valid_from, valid_until, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)",
            )
            .bind(entry.id().as_uuid())
            .bind(entry.org_id().to_string())
            .bind(entry.project().map(|p| p.to_string()))
            .bind(entry.namespace().to_string())
            .bind(entry.path().as_str())
            .bind(entry.kind().to_string())
            .bind(entry.title())
            .bind(entry.content())
            .bind(&tags_json)
            .bind(entry.version().as_u64() as i64)
            .bind(&metadata_json)
            .bind(vec_binding.as_ref())
            .bind(entry.embedding_model())
            .bind(entry.embedding_dimensions().map(|d| d as i32))
            .bind(entry.valid_from())
            .bind(entry.valid_until())
            .bind(entry.created_at())
            .bind(entry.updated_at())
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = entry.drain_events();
        PgEventWriter::new_tx(&mut tx)
            .write_all(&events)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;

        entry.mark_persisted();

        Ok(())
    }

    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        let row = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM knowledge_entries WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_entry(&r)).transpose()
    }

    async fn find_by_ids(&self, ids: &[KnowledgeId]) -> Result<Vec<Knowledge>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let uuid_ids: Vec<uuid::Uuid> = ids.iter().map(|id| *id.as_uuid()).collect();
        let rows = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM knowledge_entries WHERE id = ANY($1::uuid[])"
        ))
        .bind(&uuid_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        rows.iter().map(row_to_entry).collect()
    }

    async fn find_by_path(
        &self,
        org: &OrganizationId,
        project: Option<&ProjectId>,
        namespace: &Namespace,
        path: &KnowledgePath,
    ) -> Result<Option<Knowledge>> {
        let row = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM knowledge_entries WHERE organization_id = $1 AND project IS NOT DISTINCT FROM $2 AND namespace = $3 AND path = $4"
        ))
        .bind(org.to_string())
        .bind(project.map(|p| p.to_string()))
        .bind(namespace.to_string())
        .bind(path.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_entry(&r)).transpose()
    }

    async fn list(&self, filter: KnowledgeFilter, page: PageParams) -> Result<Page<Knowledge>> {
        let mut select = Query::select();
        select
            .from(KnowledgeEntries::Table)
            .expr(Expr::cust(SELECT_COLUMNS));

        if let Some(ref project) = filter.project {
            select.and_where(Expr::col(KnowledgeEntries::Project).eq(project.to_string()));
        }
        if let Some(ref ns) = filter.namespace
            && !ns.is_root()
        {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(KnowledgeEntries::Namespace).eq(ns.to_string()))
                    .add(Expr::col(KnowledgeEntries::Namespace).like(format!("{}/%", ns))),
            );
        }
        if let Some(ref kind) = filter.kind {
            select.and_where(Expr::col(KnowledgeEntries::KnowledgeKind).eq(kind.to_string()));
        }
        if let Some(ref tag) = filter.tag {
            select.and_where(Expr::cust_with_values(
                "tags @> to_jsonb(?::text)",
                [sea_query::Value::String(Some(Box::new(tag.clone())))],
            ));
        }
        if let Some(ref prefix) = filter.path_prefix {
            select.and_where(Expr::col(KnowledgeEntries::Path).like(format!("{prefix}%")));
        }
        if !filter.include_expired.unwrap_or(false) {
            select.and_where(Expr::cust("(valid_until IS NULL OR valid_until > NOW())"));
        }
        if let Some(orphaned) = filter.orphaned {
            let exists_sql = "EXISTS (
                SELECT 1 FROM edges e
                WHERE e.org_id = knowledge_entries.organization_id
                  AND e.to_kind = 'knowledge'
                  AND e.to_id = knowledge_entries.id::text
                  AND e.rel_type IN ('produces', 'owned_by')
                  AND e.valid_until IS NULL
            )";
            if orphaned {
                select.and_where(Expr::cust(format!("NOT {exists_sql}")));
            } else {
                select.and_where(Expr::cust(exists_sql));
            }
        }
        if let Some(ref cursor) = page.after
            && let Some(decoded) = decode_cursor(cursor)
            && let Ok(cursor_uuid) = decoded.parse::<Uuid>()
        {
            select.and_where(Expr::col(KnowledgeEntries::Id).lt(cursor_uuid));
        }

        select
            .order_by(KnowledgeEntries::Id, sea_query::Order::Desc)
            .limit((page.limit as u64).saturating_add(1));

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut entries: Vec<Knowledge> =
            rows.iter().map(row_to_entry).collect::<Result<Vec<_>>>()?;
        let has_more = entries.len() > page.limit as usize;
        if has_more {
            entries.truncate(page.limit as usize);
        }
        let next_cursor = if has_more {
            entries.last().map(|e| encode_cursor(&e.id().to_string()))
        } else {
            None
        };

        Ok(Page::new(entries, next_cursor))
    }

    async fn search(
        &self,
        org: &OrganizationId,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<(Knowledge, Option<f32>)>> {
        if let Some(emb) = embedding {
            return search_by_embedding(&self.pool, org, emb, namespace, limit).await;
        }

        let rows = if let Some(ns) = namespace.filter(|ns| !ns.is_root()) {
            let sql = format!(
                "WITH search AS (
                    SELECT {SELECT_COLUMNS}, to_tsvector('english', title || ' ' || content) AS ts
                    FROM knowledge_entries
                    WHERE organization_id = $1
                      AND (namespace = $3 OR namespace LIKE $3 || '/%')
                )
                SELECT {SELECT_COLUMNS} FROM search
                WHERE ts @@ plainto_tsquery('english', $2)
                ORDER BY ts_rank(ts, plainto_tsquery('english', $2)) DESC
                LIMIT $4"
            );
            sqlx::query(&sql)
                .bind(org.to_string())
                .bind(query)
                .bind(ns.to_string())
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?
        } else {
            let sql = format!(
                "WITH search AS (
                    SELECT {SELECT_COLUMNS}, to_tsvector('english', title || ' ' || content) AS ts
                    FROM knowledge_entries
                    WHERE organization_id = $1
                )
                SELECT {SELECT_COLUMNS} FROM search
                WHERE ts @@ plainto_tsquery('english', $2)
                ORDER BY ts_rank(ts, plainto_tsquery('english', $2)) DESC
                LIMIT $3"
            );
            sqlx::query(&sql)
                .bind(org.to_string())
                .bind(query)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?
        };

        rows.iter()
            .map(|r| row_to_entry(r).map(|k| (k, None)))
            .collect()
    }

    async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        sqlx::query("DELETE FROM knowledge_entries WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

async fn search_by_embedding(
    pool: &sqlx::PgPool,
    org: &OrganizationId,
    embedding: &[f32],
    namespace: Option<&Namespace>,
    limit: usize,
) -> Result<Vec<(Knowledge, Option<f32>)>> {
    let vec = Vector::from(embedding.to_vec());

    let mut sql = format!(
        "SELECT {SELECT_COLUMNS}, (1.0 - (embedding <=> $1)) AS score FROM knowledge_entries WHERE embedding IS NOT NULL AND organization_id = $2"
    );
    let mut param_idx = 3u32;

    if let Some(ns) = namespace.filter(|ns| !ns.is_root()) {
        sql.push_str(&format!(
            " AND (namespace = ${param_idx} OR namespace LIKE ${param_idx} || '/%')"
        ));
        param_idx += 1;

        sql.push_str(&format!(" ORDER BY embedding <=> $1 LIMIT ${param_idx}"));

        let rows = sqlx::query(&sql)
            .bind(&vec)
            .bind(org.to_string())
            .bind(ns.to_string())
            .bind(limit as i64)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        return rows
            .iter()
            .map(|r| {
                let knowledge = row_to_entry(r)?;
                let score: Option<f64> = r.try_get("score").ok();
                Ok((knowledge, score.map(|s| s as f32)))
            })
            .collect();
    }

    sql.push_str(&format!(" ORDER BY embedding <=> $1 LIMIT ${param_idx}"));

    let rows = sqlx::query(&sql)
        .bind(&vec)
        .bind(org.to_string())
        .bind(limit as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

    rows.iter()
        .map(|r| {
            let knowledge = row_to_entry(r)?;
            let score: Option<f64> = r.try_get("score").ok();
            Ok((knowledge, score.map(|s| s as f32)))
        })
        .collect()
}

fn row_to_entry(row: &sqlx::postgres::PgRow) -> Result<Knowledge> {
    let id: Uuid = row.get("id");
    let org_id_str: String = row.get("organization_id");
    let project: Option<String> = row.get("project");
    let namespace: String = row.get("namespace");
    let path: String = row.get("path");
    let kind_str: String = row.get("kind");
    let title: String = row.get("title");
    let content: String = row.get("content");
    let tags: serde_json::Value = row.get("tags");
    let version: i64 = row.get("version");
    let metadata: serde_json::Value = row.get("metadata");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let valid_from: Option<DateTime<Utc>> = row.get("valid_from");
    let valid_until: Option<DateTime<Utc>> = row.get("valid_until");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    let kind = KnowledgeKind::from_str(&kind_str).map_err(|e| {
        Error::Store(format!(
            "invalid knowledge_entries.kind value `{kind_str}`: {e}"
        ))
    })?;

    Ok(Knowledge::restore(RestoreKnowledge {
        id: KnowledgeId::from_uuid(id),
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid knowledge_entries.organization_id: {e}")))?,
        project: project
            .map(|p| parse_project_id(p, "knowledge_entries", "project"))
            .transpose()?,
        namespace: parse_namespace(namespace, "knowledge_entries", "namespace")?,
        path,
        kind,
        title,
        content,
        tags: decode_json_value(tags, "knowledge_entries", "tags")?,
        version: Version::new(version as u64),
        metadata: decode_json_value(metadata, "knowledge_entries", "metadata")?,
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        valid_from,
        valid_until,
        created_at,
        updated_at,
    }))
}
