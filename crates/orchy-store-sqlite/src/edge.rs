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
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
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
            "INSERT OR REPLACE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
                edge.source_kind().map(|k| k.to_string()),
                edge.source_id(),
                edge.valid_until().map(|dt| dt.to_rfc3339()),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
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
        only_active: bool,
    ) -> Result<Vec<Edge>> {
        let active_clause = if only_active {
            " AND valid_until IS NULL"
        } else {
            ""
        };
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let edges = if let Some(rt) = rel_type {
            let sql = format!(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
                 FROM edges WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3 AND rel_type = ?4{active_clause}
                 ORDER BY created_at ASC"
            );
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| Error::Store(e.to_string()))?;
            stmt.query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id, rt.to_string()],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            let sql = format!(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
                 FROM edges WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3{active_clause}
                 ORDER BY created_at ASC"
            );
            let mut stmt = conn
                .prepare(&sql)
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
        only_active: bool,
    ) -> Result<Vec<Edge>> {
        let active_clause = if only_active {
            " AND valid_until IS NULL"
        } else {
            ""
        };
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let edges = if let Some(rt) = rel_type {
            let sql = format!(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
                 FROM edges WHERE org_id = ?1 AND to_kind = ?2 AND to_id = ?3 AND rel_type = ?4{active_clause}
                 ORDER BY created_at ASC"
            );
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| Error::Store(e.to_string()))?;
            stmt.query_map(
                rusqlite::params![org.to_string(), kind.to_string(), id, rt.to_string()],
                row_to_edge,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            let sql = format!(
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
                 FROM edges WHERE org_id = ?1 AND to_kind = ?2 AND to_id = ?3{active_clause}
                 ORDER BY created_at ASC"
            );
            let mut stmt = conn
                .prepare(&sql)
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
                   AND to_kind = ?4 AND to_id = ?5 AND rel_type = ?6",
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
    ) -> Result<Page<Edge>> {
        let active_clause = if only_active {
            " AND valid_until IS NULL"
        } else {
            ""
        };
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let fetch_limit = (page.limit as i64) + 1;

        let mut edges = if let Some(rt) = rel_type {
            if let Some(ref cursor) = page.after {
                if let Some(decoded) = decode_cursor(cursor) {
                    let sql = format!(
                        "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
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
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
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
                    "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
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
                "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at, created_by, source_kind, source_id, valid_until
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

    async fn traverse(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        max_depth: u32,
        rel_types: Option<&[RelationType]>,
        direction: TraversalDirection,
        only_active: bool,
    ) -> Result<Vec<TraversalEdge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let rel_filter = rel_types.map(|rts| {
            rts.iter()
                .map(|rt| format!("'{}'", rt))
                .collect::<Vec<_>>()
                .join(", ")
        });

        let sql = match direction {
            TraversalDirection::Outgoing => {
                build_traverse_sql(TraversalSide::Outgoing, rel_filter.as_deref(), only_active)
            }
            TraversalDirection::Incoming => {
                build_traverse_sql(TraversalSide::Incoming, rel_filter.as_deref(), only_active)
            }
            TraversalDirection::Both => {
                build_traverse_sql(TraversalSide::Both, rel_filter.as_deref(), only_active)
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

enum TraversalSide {
    Outgoing,
    Incoming,
    Both,
}

fn build_traverse_sql(side: TraversalSide, rel_filter: Option<&str>, only_active: bool) -> String {
    let rel_clause = rel_filter
        .map(|rts| format!(" AND rel_type IN ({rts})"))
        .unwrap_or_default();

    let active_anchor = if only_active {
        " AND valid_until IS NULL"
    } else {
        ""
    };
    let active_recursive = if only_active {
        " AND e.valid_until IS NULL"
    } else {
        ""
    };

    let anchor = match side {
        TraversalSide::Outgoing => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, 1 AS depth
             FROM edges
             WHERE org_id = ?1 AND from_kind = ?2 AND from_id = ?3{rel_clause}{active_anchor}"
        ),
        TraversalSide::Incoming => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, 1 AS depth
             FROM edges
             WHERE org_id = ?1 AND to_kind = ?2 AND to_id = ?3{rel_clause}{active_anchor}"
        ),
        TraversalSide::Both => format!(
            "SELECT id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, 1 AS depth
             FROM edges
             WHERE org_id = ?1 AND ((from_kind = ?2 AND from_id = ?3) OR (to_kind = ?2 AND to_id = ?3)){rel_clause}{active_anchor}"
        ),
    };

    let recursive = match side {
        TraversalSide::Outgoing => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND e.from_kind = t.to_kind AND e.from_id = t.to_id
             WHERE t.depth < ?4{rel_clause}{active_recursive}"
        ),
        TraversalSide::Incoming => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND e.to_kind = t.from_kind AND e.to_id = t.from_id
             WHERE t.depth < ?4{rel_clause}{active_recursive}"
        ),
        TraversalSide::Both => format!(
            "SELECT e.id, e.org_id, e.from_kind, e.from_id, e.to_kind, e.to_id, e.rel_type, e.display, t.depth + 1
             FROM edges e
             INNER JOIN traversal t ON e.org_id = t.org_id AND (
                 (e.from_kind = t.from_kind AND e.from_id = t.from_id) OR
                 (e.to_kind = t.from_kind AND e.to_id = t.from_id) OR
                 (e.from_kind = t.to_kind AND e.from_id = t.to_id) OR
                 (e.to_kind = t.to_kind AND e.to_id = t.to_id)
             )
             WHERE t.depth < ?4{rel_clause}{active_recursive}"
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
    let source_kind_str: Option<String> = row.get(10).ok().flatten();
    let source_id: Option<String> = row.get(11).ok().flatten();
    let valid_until_str: Option<String> = row.get(12).ok().flatten();

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
    let source_kind = source_kind_str.and_then(|s| s.parse::<ResourceKind>().ok());
    let valid_until = valid_until_str
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        12,
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
        display,
        created_at,
        created_by,
        source_kind,
        source_id,
        valid_until,
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
