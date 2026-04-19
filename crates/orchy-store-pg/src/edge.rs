use std::str::FromStr;

use async_trait::async_trait;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::edge::{
    Edge, EdgeId, EdgeStore, RelationType, RestoreEdge, TraversalDirection, TraversalEdge,
};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::resource_ref::ResourceKind;

use crate::PgBackend;

#[async_trait]
impl EdgeStore for PgBackend {
    async fn save(&self, edge: &Edge) -> Result<()> {
        sqlx::query(
            "INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE SET
                org_id = EXCLUDED.org_id,
                from_kind = EXCLUDED.from_kind,
                from_id = EXCLUDED.from_id,
                to_kind = EXCLUDED.to_kind,
                to_id = EXCLUDED.to_id,
                rel_type = EXCLUDED.rel_type,
                display = EXCLUDED.display,
                created_at = EXCLUDED.created_at,
                created_by = EXCLUDED.created_by",
        )
        .bind(edge.id().as_uuid())
        .bind(edge.org_id().to_string())
        .bind(edge.from_kind().to_string())
        .bind(edge.from_id())
        .bind(edge.to_kind().to_string())
        .bind(edge.to_id())
        .bind(edge.rel_type().to_string())
        .bind(edge.display())
        .bind(edge.created_at())
        .bind(edge.created_by().map(|a| *a.as_uuid()))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        let row = sqlx::query(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
             FROM edges WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_edge(&r)).transpose()
    }

    async fn delete(&self, id: &EdgeId) -> Result<()> {
        sqlx::query("DELETE FROM edges WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_from(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_type: Option<&RelationType>,
    ) -> Result<Vec<Edge>> {
        let rows = if let Some(rt) = rel_type {
            sqlx::query(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                 FROM edges WHERE org_id = $1 AND from_kind = $2 AND from_id = $3 AND rel_type = $4
                 ORDER BY created_at ASC",
            )
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .bind(rt.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                 FROM edges WHERE org_id = $1 AND from_kind = $2 AND from_id = $3
                 ORDER BY created_at ASC",
            )
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        };
        rows.iter().map(row_to_edge).collect()
    }

    async fn find_to(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_type: Option<&RelationType>,
    ) -> Result<Vec<Edge>> {
        let rows = if let Some(rt) = rel_type {
            sqlx::query(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                 FROM edges WHERE org_id = $1 AND to_kind = $2 AND to_id = $3 AND rel_type = $4
                 ORDER BY created_at ASC",
            )
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .bind(rt.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                 FROM edges WHERE org_id = $1 AND to_kind = $2 AND to_id = $3
                 ORDER BY created_at ASC",
            )
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        };
        rows.iter().map(row_to_edge).collect()
    }

    async fn exists_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM edges
             WHERE org_id = $1 AND from_kind = $2 AND from_id = $3
               AND to_kind = $4 AND to_id = $5 AND rel_type = $6",
        )
        .bind(org.to_string())
        .bind(from_kind.to_string())
        .bind(from_id)
        .bind(to_kind.to_string())
        .bind(to_id)
        .bind(rel_type.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(count > 0)
    }

    async fn list_by_org(
        &self,
        org: &OrganizationId,
        rel_type: Option<&RelationType>,
        page: PageParams,
    ) -> Result<Page<Edge>> {
        let fetch_limit = (page.limit as i64) + 1;

        let mut rows = if let Some(rt) = rel_type {
            if let Some(ref cursor) = page.after {
                if let Some(decoded) = decode_cursor(cursor) {
                    sqlx::query(
                        "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                         FROM edges WHERE org_id = $1 AND rel_type = $2 AND id > $3
                         ORDER BY created_at ASC LIMIT $4",
                    )
                    .bind(org.to_string()).bind(rt.to_string()).bind(decoded).bind(fetch_limit)
                    .fetch_all(&self.pool).await.map_err(|e| Error::Store(e.to_string()))?
                } else {
                    vec![]
                }
            } else {
                sqlx::query(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                     FROM edges WHERE org_id = $1 AND rel_type = $2
                     ORDER BY created_at ASC LIMIT $3",
                )
                .bind(org.to_string()).bind(rt.to_string()).bind(fetch_limit)
                .fetch_all(&self.pool).await.map_err(|e| Error::Store(e.to_string()))?
            }
        } else if let Some(ref cursor) = page.after {
            if let Some(decoded) = decode_cursor(cursor) {
                sqlx::query(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                     FROM edges WHERE org_id = $1 AND id > $2
                     ORDER BY created_at ASC LIMIT $3",
                )
                .bind(org.to_string()).bind(decoded).bind(fetch_limit)
                .fetch_all(&self.pool).await.map_err(|e| Error::Store(e.to_string()))?
            } else {
                vec![]
            }
        } else {
            sqlx::query(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                 FROM edges WHERE org_id = $1
                 ORDER BY created_at ASC LIMIT $2",
            )
            .bind(org.to_string()).bind(fetch_limit)
            .fetch_all(&self.pool).await.map_err(|e| Error::Store(e.to_string()))?
        };

        let has_more = rows.len() > page.limit as usize;
        if has_more {
            rows.pop();
        }
        let edges: Vec<Edge> = rows
            .iter()
            .map(row_to_edge)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let next_cursor = if has_more {
            edges.last().map(|e| encode_cursor(&e.id().to_string()))
        } else {
            None
        };
        Ok(Page::new(edges, next_cursor))
    }

    async fn traverse(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        max_depth: u32,
        rel_types: Option<&[RelationType]>,
        direction: TraversalDirection,
    ) -> Result<Vec<TraversalEdge>> {
        let rel_filter = rel_types.map(|rts| {
            rts.iter()
                .map(|rt| format!("'{}'", rt))
                .collect::<Vec<_>>()
                .join(", ")
        });

        let sql = match direction {
            TraversalDirection::Outgoing => {
                build_traverse_sql(TraversalSide::Outgoing, rel_filter.as_deref())
            }
            TraversalDirection::Incoming => {
                build_traverse_sql(TraversalSide::Incoming, rel_filter.as_deref())
            }
            TraversalDirection::Both => {
                build_traverse_sql(TraversalSide::Both, rel_filter.as_deref())
            }
        };

        let rows = sqlx::query(&sql)
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .bind(max_depth as i32)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_traversal_edge).collect()
    }

    async fn delete_all_for(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM edges WHERE org_id = $1 AND ((from_kind = $2 AND from_id = $3) OR (to_kind = $2 AND to_id = $3))",
        )
        .bind(org.to_string())
        .bind(kind.to_string())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn delete_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM edges
             WHERE org_id = $1 AND from_kind = $2 AND from_id = $3
               AND to_kind = $4 AND to_id = $5 AND rel_type = $6",
        )
        .bind(org.to_string())
        .bind(from_kind.to_string())
        .bind(from_id)
        .bind(to_kind.to_string())
        .bind(to_id)
        .bind(rel_type.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}

enum TraversalSide {
    Outgoing,
    Incoming,
    Both,
}

fn build_traverse_sql(side: TraversalSide, rel_filter: Option<&str>) -> String {
    let rel_clause = rel_filter
        .map(|rts| format!(" AND rel_type IN ({rts})"))
        .unwrap_or_default();

    let anchor = match side {
        TraversalSide::Outgoing => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, ARRAY[id::text] AS path, 1 AS depth
             FROM edges
             WHERE org_id = $1 AND from_kind = $2 AND from_id = $3{rel_clause}"
        ),
        TraversalSide::Incoming => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, ARRAY[id::text] AS path, 1 AS depth
             FROM edges
             WHERE org_id = $1 AND to_kind = $2 AND to_id = $3{rel_clause}"
        ),
        TraversalSide::Both => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, ARRAY[id::text] AS path, 1 AS depth
             FROM edges
             WHERE org_id = $1 AND ((from_kind = $2 AND from_id = $3) OR (to_kind = $2 AND to_id = $3)){rel_clause}"
        ),
    };

    let recursive = match side {
        TraversalSide::Outgoing => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.path || e.id::text, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND e.from_kind = t.to_kind AND e.from_id = t.to_id
             WHERE t.depth < $4 AND NOT (e.id::text = ANY(t.path)){rel_clause}"
        ),
        TraversalSide::Incoming => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.path || e.id::text, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND e.to_kind = t.from_kind AND e.to_id = t.from_id
             WHERE t.depth < $4 AND NOT (e.id::text = ANY(t.path)){rel_clause}"
        ),
        TraversalSide::Both => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.path || e.id::text, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND (
                 (e.from_kind = t.from_kind AND e.from_id = t.from_id) OR
                 (e.to_kind = t.from_kind AND e.to_id = t.from_id) OR
                 (e.from_kind = t.to_kind AND e.from_id = t.to_id) OR
                 (e.to_kind = t.to_kind AND e.to_id = t.to_id)
             )
             WHERE t.depth < $4 AND NOT (e.id::text = ANY(t.path)){rel_clause}"
        ),
    };

    format!(
        "WITH RECURSIVE traversal(id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, path, depth) AS (
             {anchor}
             UNION ALL
             {recursive}
         )
         SELECT DISTINCT ON (id) id, from_kind, from_id, to_kind, to_id, rel_type, display, depth
         FROM traversal
         ORDER BY id, depth ASC"
    )
}

fn row_to_edge(row: &sqlx::postgres::PgRow) -> Result<Edge> {
    let id_uuid: Uuid = row.try_get("id").map_err(|e| Error::Store(e.to_string()))?;
    let org_id_str: String = row
        .try_get("org_id")
        .map_err(|e| Error::Store(e.to_string()))?;
    let from_kind_str: String = row
        .try_get("from_kind")
        .map_err(|e| Error::Store(e.to_string()))?;
    let from_id: String = row
        .try_get("from_id")
        .map_err(|e| Error::Store(e.to_string()))?;
    let to_kind_str: String = row
        .try_get("to_kind")
        .map_err(|e| Error::Store(e.to_string()))?;
    let to_id: String = row
        .try_get("to_id")
        .map_err(|e| Error::Store(e.to_string()))?;
    let rel_type_str: String = row
        .try_get("rel_type")
        .map_err(|e| Error::Store(e.to_string()))?;
    let display: Option<String> = row
        .try_get("display")
        .map_err(|e| Error::Store(e.to_string()))?;
    let created_at: chrono::DateTime<chrono::Utc> = row
        .try_get("created_at")
        .map_err(|e| Error::Store(e.to_string()))?;
    let created_by_uuid: Option<Uuid> = row
        .try_get("created_by")
        .map_err(|e| Error::Store(e.to_string()))?;

    let id = EdgeId::from_uuid(id_uuid);
    let org_id = OrganizationId::new(&org_id_str).map_err(|e| Error::Store(e.to_string()))?;
    let from_kind = ResourceKind::from_str(&from_kind_str).map_err(Error::Store)?;
    let to_kind = ResourceKind::from_str(&to_kind_str).map_err(Error::Store)?;
    let rel_type = RelationType::from_str(&rel_type_str).map_err(Error::Store)?;
    let created_by = created_by_uuid.map(AgentId::from_uuid);

    Ok(Edge::restore(RestoreEdge {
        id,
        org_id,
        from_kind,
        from_id,
        to_kind,
        to_id,
        rel_type,
        display,
        created_at,
        created_by,
    }))
}

fn row_to_traversal_edge(row: &sqlx::postgres::PgRow) -> Result<TraversalEdge> {
    let id_uuid: Uuid = row.try_get("id").map_err(|e| Error::Store(e.to_string()))?;
    let from_kind_str: String = row
        .try_get("from_kind")
        .map_err(|e| Error::Store(e.to_string()))?;
    let from_id: String = row
        .try_get("from_id")
        .map_err(|e| Error::Store(e.to_string()))?;
    let to_kind_str: String = row
        .try_get("to_kind")
        .map_err(|e| Error::Store(e.to_string()))?;
    let to_id: String = row
        .try_get("to_id")
        .map_err(|e| Error::Store(e.to_string()))?;
    let rel_type_str: String = row
        .try_get("rel_type")
        .map_err(|e| Error::Store(e.to_string()))?;
    let display: Option<String> = row
        .try_get("display")
        .map_err(|e| Error::Store(e.to_string()))?;
    let depth: i32 = row
        .try_get("depth")
        .map_err(|e| Error::Store(e.to_string()))?;

    let id = EdgeId::from_uuid(id_uuid);
    let from_kind = ResourceKind::from_str(&from_kind_str).map_err(Error::Store)?;
    let to_kind = ResourceKind::from_str(&to_kind_str).map_err(Error::Store)?;
    let rel_type = RelationType::from_str(&rel_type_str).map_err(Error::Store)?;

    Ok(TraversalEdge {
        id,
        from_kind,
        from_id,
        to_kind,
        to_id,
        rel_type,
        display,
        depth: depth as u32,
    })
}
