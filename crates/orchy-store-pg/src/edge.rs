use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::edge::{
    Edge, EdgeId, EdgeStore, RelationDirection, RelationType, RestoreEdge, TraversalDirection,
    TraversalHop,
};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::resource_ref::{ResourceKind, ResourceRef};

use crate::PgBackend;

#[async_trait]
impl EdgeStore for PgBackend {
    async fn save(&self, edge: &mut Edge) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at, created_by, source_kind, source_id, valid_until)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
                org_id = EXCLUDED.org_id,
                from_kind = EXCLUDED.from_kind,
                from_id = EXCLUDED.from_id,
                to_kind = EXCLUDED.to_kind,
                to_id = EXCLUDED.to_id,
                rel_type = EXCLUDED.rel_type,
                created_at = EXCLUDED.created_at,
                created_by = EXCLUDED.created_by,
                source_kind = EXCLUDED.source_kind,
                source_id = EXCLUDED.source_id,
                valid_until = EXCLUDED.valid_until",
        )
        .bind(edge.id().as_uuid())
        .bind(edge.org_id().to_string())
        .bind(edge.from_kind().to_string())
        .bind(edge.from_id())
        .bind(edge.to_kind().to_string())
        .bind(edge.to_id())
        .bind(edge.rel_type().to_string())
        .bind(edge.created_at())
        .bind(edge.created_by().map(|a| *a.as_uuid()))
        .bind(edge.source_kind().map(|k| k.to_string()))
        .bind(edge.source_id())
        .bind(edge.valid_until())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = edge.drain_events();
        crate::write_events_in_tx(&mut tx, &events).await?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        let row = sqlx::query(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
             created_at, created_by, source_kind, source_id, valid_until \
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
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        let time_clause = build_time_clause(as_of.as_ref());
        let rel_clause = build_rel_clause(rel_types);
        let sql = format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
             created_at, created_by, source_kind, source_id, valid_until \
             FROM edges WHERE org_id = $1 AND from_kind = $2 AND from_id = $3{rel_clause}{time_clause} \
             ORDER BY created_at ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        rows.iter().map(row_to_edge).collect()
    }

    async fn find_to(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        let time_clause = build_time_clause(as_of.as_ref());
        let rel_clause = build_rel_clause(rel_types);
        let sql = format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
             created_at, created_by, source_kind, source_id, valid_until \
             FROM edges WHERE org_id = $1 AND to_kind = $2 AND to_id = $3{rel_clause}{time_clause} \
             ORDER BY created_at ASC"
        );
        let rows = sqlx::query(&sql)
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
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
               AND to_kind = $4 AND to_id = $5 AND rel_type = $6
               AND valid_until IS NULL",
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
        only_active: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Page<Edge>> {
        let time_clause = if let Some(ts) = as_of.as_ref() {
            let ts_str = ts.to_rfc3339();
            format!(
                " AND created_at <= '{ts_str}' AND (valid_until IS NULL OR valid_until > '{ts_str}')"
            )
        } else if only_active {
            " AND valid_until IS NULL".to_string()
        } else {
            String::new()
        };
        let fetch_limit = (page.limit as i64) + 1;

        let mut rows = if let Some(rt) = rel_type {
            if let Some(ref cursor) = page.after {
                if let Some(decoded) = decode_cursor(cursor) {
                    let sql = format!(
                        "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
                         created_at, created_by, source_kind, source_id, valid_until \
                         FROM edges WHERE org_id = $1 AND rel_type = $2 AND id > $3{time_clause} \
                         ORDER BY created_at ASC LIMIT $4"
                    );
                    sqlx::query(&sql)
                        .bind(org.to_string())
                        .bind(rt.to_string())
                        .bind(decoded)
                        .bind(fetch_limit)
                        .fetch_all(&self.pool)
                        .await
                        .map_err(|e| Error::Store(e.to_string()))?
                } else {
                    vec![]
                }
            } else {
                let sql = format!(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
                     created_at, created_by, source_kind, source_id, valid_until \
                     FROM edges WHERE org_id = $1 AND rel_type = $2{time_clause} \
                     ORDER BY created_at ASC LIMIT $3"
                );
                sqlx::query(&sql)
                    .bind(org.to_string())
                    .bind(rt.to_string())
                    .bind(fetch_limit)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| Error::Store(e.to_string()))?
            }
        } else if let Some(ref cursor) = page.after {
            if let Some(decoded) = decode_cursor(cursor) {
                let sql = format!(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
                     created_at, created_by, source_kind, source_id, valid_until \
                     FROM edges WHERE org_id = $1 AND id > $2{time_clause} \
                     ORDER BY created_at ASC LIMIT $3"
                );
                sqlx::query(&sql)
                    .bind(org.to_string())
                    .bind(decoded)
                    .bind(fetch_limit)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| Error::Store(e.to_string()))?
            } else {
                vec![]
            }
        } else {
            let sql = format!(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
                 created_at, created_by, source_kind, source_id, valid_until \
                 FROM edges WHERE org_id = $1{time_clause} \
                 ORDER BY created_at ASC LIMIT $2"
            );
            sqlx::query(&sql)
                .bind(org.to_string())
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?
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

    async fn find_neighbors(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        target_kinds: &[ResourceKind],
        direction: TraversalDirection,
        max_depth: u32,
        as_of: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<TraversalHop>> {
        let max_depth = max_depth.max(1);
        let sql = build_find_neighbors_sql(rel_types, target_kinds, direction, &as_of, limit);
        let rows = sqlx::query(&sql)
            .bind(org.to_string())
            .bind(kind.to_string())
            .bind(id)
            .bind(max_depth as i32)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        rows.iter().map(row_to_traversal_hop).collect()
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

fn build_time_clause(as_of: Option<&DateTime<Utc>>) -> String {
    if let Some(ts) = as_of {
        let ts_str = ts.to_rfc3339();
        format!(
            " AND created_at <= '{ts_str}' AND (valid_until IS NULL OR valid_until > '{ts_str}')"
        )
    } else {
        " AND valid_until IS NULL".to_string()
    }
}

fn build_rel_clause(rel_types: &[RelationType]) -> String {
    if rel_types.is_empty() {
        return String::new();
    }
    let list = rel_types
        .iter()
        .map(|r| format!("'{r}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" AND rel_type IN ({list})")
}

fn build_find_neighbors_sql(
    rel_types: &[RelationType],
    target_kinds: &[ResourceKind],
    direction: TraversalDirection,
    as_of: &Option<DateTime<Utc>>,
    _limit: u32,
) -> String {
    let rel_clause = build_rel_clause(rel_types);

    let time_clause = if let Some(ts) = as_of {
        let s = ts.to_rfc3339();
        format!(" AND created_at <= '{s}' AND (valid_until IS NULL OR valid_until > '{s}')")
    } else {
        " AND valid_until IS NULL".to_string()
    };

    let target_filter = if target_kinds.is_empty() {
        String::new()
    } else {
        let list = target_kinds
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("WHERE peer_kind IN ({list})")
    };

    let rec_time = if let Some(ts) = as_of {
        let s = ts.to_rfc3339();
        format!(" AND e.created_at <= '{s}' AND (e.valid_until IS NULL OR e.valid_until > '{s}')")
    } else {
        " AND e.valid_until IS NULL".to_string()
    };

    let (anchor_match, base_direction, base_peer_kind, base_peer_id, recursive_join, rec_direction, rec_peer_kind, rec_peer_id) = match direction {
        TraversalDirection::Outgoing => (
            format!("(from_kind = $2 AND from_id = $3){rel_clause}{time_clause}"),
            "'outgoing'".to_string(),
            "e.to_kind".to_string(),
            "e.to_id".to_string(),
            "INNER JOIN traversal t ON e.org_id = t.org_id AND e.from_kind = t.peer_kind AND e.from_id = t.peer_id".to_string(),
            "t.direction".to_string(),
            "e.to_kind".to_string(),
            "e.to_id".to_string(),
        ),
        TraversalDirection::Incoming => (
            format!("(to_kind = $2 AND to_id = $3){rel_clause}{time_clause}"),
            "'incoming'".to_string(),
            "e.from_kind".to_string(),
            "e.from_id".to_string(),
            "INNER JOIN traversal t ON e.org_id = t.org_id AND e.to_kind = t.peer_kind AND e.to_id = t.peer_id".to_string(),
            "t.direction".to_string(),
            "e.from_kind".to_string(),
            "e.from_id".to_string(),
        ),
        TraversalDirection::Both => (
            format!("((from_kind = $2 AND from_id = $3) OR (to_kind = $2 AND to_id = $3)){rel_clause}{time_clause}"),
            "CASE WHEN e.from_kind = $2 AND e.from_id = $3 THEN 'outgoing' ELSE 'incoming' END".to_string(),
            "CASE WHEN e.from_kind = $2 AND e.from_id = $3 THEN e.to_kind ELSE e.from_kind END".to_string(),
            "CASE WHEN e.from_kind = $2 AND e.from_id = $3 THEN e.to_id ELSE e.from_id END".to_string(),
            "INNER JOIN traversal t ON e.org_id = t.org_id AND ((e.from_kind = t.peer_kind AND e.from_id = t.peer_id) OR (e.to_kind = t.peer_kind AND e.to_id = t.peer_id))".to_string(),
            "CASE WHEN e.from_kind = t.peer_kind AND e.from_id = t.peer_id THEN 'outgoing' ELSE 'incoming' END".to_string(),
            "CASE WHEN e.from_kind = t.peer_kind AND e.from_id = t.peer_id THEN e.to_kind ELSE e.from_kind END".to_string(),
            "CASE WHEN e.from_kind = t.peer_kind AND e.from_id = t.peer_id THEN e.to_id ELSE e.from_id END".to_string(),
        ),
    };

    format!(
        r#"WITH RECURSIVE traversal(
            id, org_id, from_kind, from_id, to_kind, to_id, rel_type,
            created_at, created_by, source_kind, source_id, valid_until,
            depth, direction, peer_kind, peer_id, via_kind, via_id, visited
        ) AS (
            SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type,
                   e.created_at, e.created_by, e.source_kind, e.source_id, e.valid_until,
                   1 AS depth,
                   {base_direction} AS direction,
                   {base_peer_kind} AS peer_kind,
                   {base_peer_id} AS peer_id,
                   NULL::text AS via_kind,
                   NULL::text AS via_id,
                   ARRAY[e.id::text] AS visited
            FROM edges e
            WHERE e.org_id = $1 AND {anchor_match}

            UNION ALL

            SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type,
                   e.created_at, e.created_by, e.source_kind, e.source_id, e.valid_until,
                   t.depth + 1,
                   {rec_direction},
                   {rec_peer_kind},
                   {rec_peer_id},
                   t.peer_kind,
                   t.peer_id,
                   t.visited || e.id::text
            FROM edges e
            {recursive_join}
            WHERE t.depth < $4
              AND NOT (e.id::text = ANY(t.visited))
              {rec_time}
              {rel_clause}
        )
        SELECT DISTINCT ON (id)
            id, org_id, from_kind, from_id, to_kind, to_id, rel_type,
            created_at, created_by, source_kind, source_id, valid_until,
            depth, direction, via_kind, via_id
        FROM traversal
        {target_filter}
        ORDER BY id, depth ASC
        LIMIT $5"#
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
    let created_at: chrono::DateTime<chrono::Utc> = row
        .try_get("created_at")
        .map_err(|e| Error::Store(e.to_string()))?;
    let created_by_uuid: Option<Uuid> = row
        .try_get("created_by")
        .map_err(|e| Error::Store(e.to_string()))?;
    let source_kind_str: Option<String> = row.try_get("source_kind").unwrap_or(None);
    let source_id: Option<String> = row.try_get("source_id").unwrap_or(None);
    let valid_until: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("valid_until").unwrap_or(None);

    let id = EdgeId::from_uuid(id_uuid);
    let org_id = OrganizationId::new(&org_id_str).map_err(|e| Error::Store(e.to_string()))?;
    let from_kind = ResourceKind::from_str(&from_kind_str).map_err(Error::Store)?;
    let to_kind = ResourceKind::from_str(&to_kind_str).map_err(Error::Store)?;
    let rel_type = RelationType::from_str(&rel_type_str).map_err(Error::Store)?;
    let created_by = created_by_uuid.map(AgentId::from_uuid);
    let source_kind = source_kind_str.and_then(|s| s.parse::<ResourceKind>().ok());

    Ok(Edge::restore(RestoreEdge {
        id,
        org_id,
        from_kind,
        from_id,
        to_kind,
        to_id,
        rel_type,
        created_at,
        created_by,
        source_kind,
        source_id,
        valid_until,
    }))
}

fn row_to_traversal_hop(row: &sqlx::postgres::PgRow) -> Result<TraversalHop> {
    let edge = row_to_edge(row)?;
    let depth: i32 = row
        .try_get("depth")
        .map_err(|e| Error::Store(e.to_string()))?;
    let direction_str: String = row
        .try_get("direction")
        .map_err(|e| Error::Store(e.to_string()))?;
    let direction = match direction_str.as_str() {
        "outgoing" => RelationDirection::Outgoing,
        _ => RelationDirection::Incoming,
    };
    let via_kind_str: Option<String> = row.try_get("via_kind").unwrap_or(None);
    let via_id_str: Option<String> = row.try_get("via_id").unwrap_or(None);
    let via = via_kind_str.zip(via_id_str).and_then(|(k, id)| {
        let kind = k.parse::<ResourceKind>().ok()?;
        Some(ResourceRef::new(kind, id))
    });
    Ok(TraversalHop {
        edge,
        depth: depth as u32,
        direction,
        via,
    })
}
