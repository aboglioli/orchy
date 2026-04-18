use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::edge::{
    Edge, EdgeId, EdgeStore, RelationType, RestoreEdge, TraversalDirection, TraversalEdge,
};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::SqliteBackend;

fn str_err(e: impl ToString) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
}

#[async_trait]
impl EdgeStore for SqliteBackend {
    async fn save(&self, edge: &Edge) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                edge.id().to_string(),
                edge.org_id().to_string(),
                edge.from_kind().to_string(),
                edge.from_id(),
                edge.to_kind().to_string(),
                edge.to_id(),
                edge.rel_type().to_string(),
                edge.display(),
                edge.created_at().to_rfc3339(),
                edge.created_by().map(|a| a.to_string()),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
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
        rel_type: Option<&RelationType>,
    ) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let edges = if let Some(rt) = rel_type {
            let mut stmt = conn
                .prepare(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                     FROM edges WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3 AND rel_type = ?4
                     ORDER BY created_at ASC",
                )
                .map_err(|e| Error::Store(e.to_string()))?;
            stmt.query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id, rt.to_string()],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                     FROM edges WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3
                     ORDER BY created_at ASC",
                )
                .map_err(|e| Error::Store(e.to_string()))?;
            stmt.query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?
        };
        Ok(edges)
    }

    async fn find_to(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_type: Option<&RelationType>,
    ) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let edges = if let Some(rt) = rel_type {
            let mut stmt = conn
                .prepare(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                     FROM edges WHERE org_id = ?1 AND to_kind = ?2 AND to_id = ?3 AND rel_type = ?4
                     ORDER BY created_at ASC",
                )
                .map_err(|e| Error::Store(e.to_string()))?;
            stmt.query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id, rt.to_string()],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by
                     FROM edges WHERE org_id = ?1 AND to_kind = ?2 AND to_id = ?3
                     ORDER BY created_at ASC",
                )
                .map_err(|e| Error::Store(e.to_string()))?;
            stmt.query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?
        };
        Ok(edges)
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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let rel_filter = rel_types.map(|rts| {
            rts.iter()
                .map(|rt| format!("'{}'", rt))
                .collect::<Vec<_>>()
                .join(", ")
        });

        // Build the recursive CTE. The anchor selects direct edges from/to the starting node.
        // Each recursive step follows edges from the frontier, deduplicating by (from_kind, from_id, to_kind, to_id).
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

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;

        let edges = stmt
            .query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id, max_depth as i64,],
                row_to_traversal_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(edges)
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
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, 1 AS depth
             FROM edges
             WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3{rel_clause}"
        ),
        TraversalSide::Incoming => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, 1 AS depth
             FROM edges
             WHERE org_id = ?1 AND to_kind = ?2 AND to_id = ?3{rel_clause}"
        ),
        TraversalSide::Both => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, 1 AS depth
             FROM edges
             WHERE org_id = ?1 AND ((from_kind = ?2 AND from_id = ?3) OR (to_kind = ?2 AND to_id = ?3)){rel_clause}"
        ),
    };

    let recursive = match side {
        TraversalSide::Outgoing => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND e.from_kind = t.to_kind AND e.from_id = t.to_id
             WHERE t.depth < ?4{rel_clause}"
        ),
        TraversalSide::Incoming => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND e.to_kind = t.from_kind AND e.to_id = t.from_id
             WHERE t.depth < ?4{rel_clause}"
        ),
        TraversalSide::Both => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND (
                 (e.from_kind = t.to_kind AND e.from_id = t.to_id) OR
                 (e.to_kind = t.from_kind AND e.to_id = t.from_id)
             )
             WHERE t.depth < ?4{rel_clause}"
        ),
    };

    format!(
        "WITH RECURSIVE traversal(id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, depth) AS (
             {anchor}
             UNION ALL
             {recursive}
         )
         SELECT id, from_kind, from_id, to_kind, to_id, rel_type, display, MIN(depth) AS depth
         FROM traversal
         GROUP BY id
         ORDER BY depth ASC, from_kind ASC, from_id ASC"
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
    let display: Option<String> = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let created_by_str: Option<String> = row.get(9)?;

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
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, str_err(e))
        })?;
    let created_by = created_by_str
        .map(|s| AgentId::from_str(&s))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, str_err(e))
        })?;

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

fn row_to_traversal_edge(row: &rusqlite::Row) -> rusqlite::Result<TraversalEdge> {
    let id_str: String = row.get(0)?;
    let from_kind_str: String = row.get(1)?;
    let from_id: String = row.get(2)?;
    let to_kind_str: String = row.get(3)?;
    let to_id: String = row.get(4)?;
    let rel_type_str: String = row.get(5)?;
    let display: Option<String> = row.get(6)?;
    let depth: i64 = row.get(7)?;

    let id = EdgeId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, str_err(e))
    })?;
    let from_kind = ResourceKind::from_str(&from_kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, str_err(e))
    })?;
    let to_kind = ResourceKind::from_str(&to_kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, str_err(e))
    })?;
    let rel_type = RelationType::from_str(&rel_type_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, str_err(e))
    })?;

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
