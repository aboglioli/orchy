use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::edge::{
    Edge, EdgeId, EdgeStore, RelationDirection, RelationType, RestoreEdge, TraversalDirection,
    TraversalHop,
};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::resource_ref::{ResourceKind, ResourceRef};

use crate::SqliteBackend;

fn str_err(e: impl ToString) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
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

fn build_rel_clause(rel_types: &[RelationType], prefix: &str) -> String {
    if rel_types.is_empty() {
        return String::new();
    }
    let list = rel_types
        .iter()
        .map(|r| format!("'{r}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" AND {prefix}rel_type IN ({list})")
}

#[async_trait]
impl EdgeStore for SqliteBackend {
    async fn save(&self, edge: &mut Edge) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at, created_by, source_kind, source_id, valid_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                edge.id().to_string(),
                edge.org_id().to_string(),
                edge.from_kind().to_string(),
                edge.from_id(),
                edge.to_kind().to_string(),
                edge.to_id(),
                edge.rel_type().to_string(),
                edge.created_at().to_rfc3339(),
                edge.created_by().map(|a| a.to_string()),
                edge.source_kind().map(|k| k.to_string()),
                edge.source_id(),
                edge.valid_until().map(|dt| dt.to_rfc3339()),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = edge.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
                 created_at, created_by, source_kind, source_id, valid_until \
                 FROM edges WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_row(rusqlite::params![id.to_string()], row_to_edge)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))
    }

    async fn delete(&self, id: &EdgeId) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "DELETE FROM edges WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
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
        let rel_clause = build_rel_clause(rel_types, "");
        let sql = format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
             created_at, created_by, source_kind, source_id, valid_until \
             FROM edges WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3{rel_clause}{time_clause} \
             ORDER BY created_at ASC"
        );
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let edges = stmt
            .query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(edges)
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
        let rel_clause = build_rel_clause(rel_types, "");
        let sql = format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, \
             created_at, created_by, source_kind, source_id, valid_until \
             FROM edges WHERE org_id = ?1 AND to_kind = ?2 AND to_id = ?3{rel_clause}{time_clause} \
             ORDER BY created_at ASC"
        );
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let edges = stmt
            .query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(edges)
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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges
                 WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3
                   AND to_kind = ?4 AND to_id = ?5 AND rel_type = ?6
                   AND valid_until IS NULL",
                rusqlite::params![
                    org.to_string(),
                    from_kind.to_string(),
                    from_id,
                    to_kind.to_string(),
                    to_id,
                    rel_type.to_string(),
                ],
                |row| row.get(0),
            )
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
        let active_clause = if let Some(ts) = as_of.as_ref() {
            let ts_str = ts.to_rfc3339();
            format!(
                " AND created_at <= '{ts_str}' AND (valid_until IS NULL OR valid_until > '{ts_str}')"
            )
        } else if only_active {
            " AND valid_until IS NULL".to_string()
        } else {
            String::new()
        };
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let fetch_limit = (page.limit as i64) + 1;

        let mut edges = if let Some(rt) = rel_type {
            if let Some(ref cursor) = page.after {
                if let Some(decoded) = decode_cursor(cursor) {
                    let sql = format!(
                        "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at, created_by, source_kind, source_id, valid_until
                         FROM edges WHERE org_id = ?1 AND rel_type = ?2 AND id > ?3{active_clause}
                         ORDER BY created_at ASC LIMIT ?4"
                    );
                    let mut stmt = conn
                        .prepare(&sql)
                        .map_err(|e| Error::Store(e.to_string()))?;
                    stmt.query_map(
                        rusqlite::params![org.to_string(), rt.to_string(), decoded, fetch_limit],
                        row_to_edge,
                    )
                    .map_err(|e| Error::Store(e.to_string()))?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| Error::Store(e.to_string()))?
                } else {
                    vec![]
                }
            } else {
                let sql = format!(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at, created_by, source_kind, source_id, valid_until
                     FROM edges WHERE org_id = ?1 AND rel_type = ?2{active_clause}
                     ORDER BY created_at ASC LIMIT ?3"
                );
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| Error::Store(e.to_string()))?;
                stmt.query_map(
                    rusqlite::params![org.to_string(), rt.to_string(), fetch_limit],
                    row_to_edge,
                )
                .map_err(|e| Error::Store(e.to_string()))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::Store(e.to_string()))?
            }
        } else if let Some(ref cursor) = page.after {
            if let Some(decoded) = decode_cursor(cursor) {
                let sql = format!(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at, created_by, source_kind, source_id, valid_until
                     FROM edges WHERE org_id = ?1 AND id > ?2{active_clause}
                     ORDER BY created_at ASC LIMIT ?3"
                );
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| Error::Store(e.to_string()))?;
                stmt.query_map(
                    rusqlite::params![org.to_string(), decoded, fetch_limit],
                    row_to_edge,
                )
                .map_err(|e| Error::Store(e.to_string()))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::Store(e.to_string()))?
            } else {
                vec![]
            }
        } else {
            let sql = format!(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at, created_by, source_kind, source_id, valid_until
                 FROM edges WHERE org_id = ?1{active_clause}
                 ORDER BY created_at ASC LIMIT ?2"
            );
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| Error::Store(e.to_string()))?;
            stmt.query_map(rusqlite::params![org.to_string(), fetch_limit], row_to_edge)
                .map_err(|e| Error::Store(e.to_string()))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::Store(e.to_string()))?
        };

        let has_more = edges.len() > page.limit as usize;
        if has_more {
            edges.pop();
        }
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
        let sql = build_find_neighbors_sql(rel_types, target_kinds, direction, &as_of);
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let hops = stmt
            .query_map(
                rusqlite::params![
                    org.to_string(),
                    kind.to_string(),
                    id,
                    max_depth as i64,
                    limit as i64,
                ],
                row_to_traversal_hop,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(hops)
    }

    async fn delete_all_for(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "DELETE FROM edges WHERE org_id = ?1 AND ((from_kind = ?2 AND from_id = ?3) OR (to_kind = ?2 AND to_id = ?3))",
            rusqlite::params![org.to_string(), kind.to_string(), id],
        )
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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "DELETE FROM edges
             WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3
               AND to_kind = ?4 AND to_id = ?5 AND rel_type = ?6",
            rusqlite::params![
                org.to_string(),
                from_kind.to_string(),
                from_id,
                to_kind.to_string(),
                to_id,
                rel_type.to_string(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}

fn build_find_neighbors_sql(
    rel_types: &[RelationType],
    target_kinds: &[ResourceKind],
    direction: TraversalDirection,
    as_of: &Option<DateTime<Utc>>,
) -> String {
    let rel_clause = build_rel_clause(rel_types, "e.");

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
            format!("(from_kind = ?2 AND from_id = ?3){rel_clause}{time_clause}"),
            "'outgoing'".to_string(),
            "e.to_kind".to_string(),
            "e.to_id".to_string(),
            "INNER JOIN traversal t ON e.org_id = t.org_id AND e.from_kind = t.peer_kind AND e.from_id = t.peer_id".to_string(),
            "t.direction".to_string(),
            "e.to_kind".to_string(),
            "e.to_id".to_string(),
        ),
        TraversalDirection::Incoming => (
            format!("(to_kind = ?2 AND to_id = ?3){rel_clause}{time_clause}"),
            "'incoming'".to_string(),
            "e.from_kind".to_string(),
            "e.from_id".to_string(),
            "INNER JOIN traversal t ON e.org_id = t.org_id AND e.to_kind = t.peer_kind AND e.to_id = t.peer_id".to_string(),
            "t.direction".to_string(),
            "e.from_kind".to_string(),
            "e.from_id".to_string(),
        ),
        TraversalDirection::Both => (
            format!("((from_kind = ?2 AND from_id = ?3) OR (to_kind = ?2 AND to_id = ?3)){rel_clause}{time_clause}"),
            "CASE WHEN e.from_kind = ?2 AND e.from_id = ?3 THEN 'outgoing' ELSE 'incoming' END".to_string(),
            "CASE WHEN e.from_kind = ?2 AND e.from_id = ?3 THEN e.to_kind ELSE e.from_kind END".to_string(),
            "CASE WHEN e.from_kind = ?2 AND e.from_id = ?3 THEN e.to_id ELSE e.from_id END".to_string(),
            "INNER JOIN traversal t ON e.org_id = t.org_id AND ((e.from_kind = t.peer_kind AND e.from_id = t.peer_id) OR (e.to_kind = t.peer_kind AND e.to_id = t.peer_id))".to_string(),
            "CASE WHEN e.from_kind = t.peer_kind AND e.from_id = t.peer_id THEN 'outgoing' ELSE 'incoming' END".to_string(),
            "CASE WHEN e.from_kind = t.peer_kind AND e.from_id = t.peer_id THEN e.to_kind ELSE e.from_kind END".to_string(),
            "CASE WHEN e.from_kind = t.peer_kind AND e.from_id = t.peer_id THEN e.to_id ELSE e.from_id END".to_string(),
        ),
    };

    format!(
        "WITH RECURSIVE traversal(
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
                   NULL AS via_kind,
                   NULL AS via_id,
                   ',' || CAST(e.id AS TEXT) || ',' AS visited
            FROM edges e
            WHERE e.org_id = ?1 AND {anchor_match}

            UNION ALL

            SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type,
                   e.created_at, e.created_by, e.source_kind, e.source_id, e.valid_until,
                   t.depth + 1,
                   {rec_direction},
                   {rec_peer_kind},
                   {rec_peer_id},
                   t.peer_kind,
                   t.peer_id,
                   t.visited || CAST(e.id AS TEXT) || ','
            FROM edges e
            {recursive_join}
            WHERE t.depth < ?4
              AND INSTR(t.visited, ',' || CAST(e.id AS TEXT) || ',') = 0
              {rec_time}
              {rel_clause}
        )
        SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type,
               created_at, created_by, source_kind, source_id, valid_until,
               depth, direction, via_kind, via_id
        FROM (
            SELECT *, ROW_NUMBER() OVER (PARTITION BY id ORDER BY depth ASC) AS rn
            FROM traversal
            {target_filter}
        ) WHERE rn = 1
        ORDER BY depth ASC
        LIMIT ?5"
    )
}

fn row_to_edge(row: &rusqlite::Row) -> rusqlite::Result<Edge> {
    let id_str: String = row.get(0)?;
    let org_id_str: String = row.get(1)?;
    let from_kind_str: String = row.get(2)?;
    let from_id: String = row.get(3)?;
    let to_kind_str: String = row.get(4)?;
    let to_id: String = row.get(5)?;
    let rel_type_str: String = row.get(6)?;
    let created_at_str: String = row.get(7)?;
    let created_by_str: Option<String> = row.get(8)?;
    let source_kind_str: Option<String> = row.get(9).ok().flatten();
    let source_id: Option<String> = row.get(10).ok().flatten();
    let valid_until_str: Option<String> = row.get(11).ok().flatten();

    let id = EdgeId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, str_err(e))
    })?;
    let org_id = OrganizationId::new(&org_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, str_err(e))
    })?;
    let from_kind = ResourceKind::from_str(&from_kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, str_err(e))
    })?;
    let to_kind = ResourceKind::from_str(&to_kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, str_err(e))
    })?;
    let rel_type = RelationType::from_str(&rel_type_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, str_err(e))
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, str_err(e))
        })?;
    let created_by = created_by_str
        .map(|s| AgentId::from_str(&s))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, str_err(e))
        })?;
    let source_kind = source_kind_str.and_then(|s| s.parse::<ResourceKind>().ok());
    let valid_until = valid_until_str
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        11,
                        rusqlite::types::Type::Text,
                        str_err(e),
                    )
                })
        })
        .transpose()?;

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

fn row_to_traversal_hop(row: &rusqlite::Row) -> rusqlite::Result<TraversalHop> {
    let edge = row_to_edge(row)?;
    let depth: i64 = row.get(12)?;
    let direction_str: String = row.get(13)?;
    let direction = match direction_str.as_str() {
        "outgoing" => RelationDirection::Outgoing,
        _ => RelationDirection::Incoming,
    };
    let via_kind_str: Option<String> = row.get(14).ok().flatten();
    let via_id_str: Option<String> = row.get(15).ok().flatten();
    let via = via_kind_str.zip(via_id_str).and_then(|(k, vid)| {
        let kind = k.parse::<ResourceKind>().ok()?;
        Some(ResourceRef::new(kind, vid))
    });
    Ok(TraversalHop {
        edge,
        depth: depth as u32,
        direction,
        via,
    })
}
