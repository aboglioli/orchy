use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::project_link::{
    ProjectLink, ProjectLinkId, ProjectLinkStore, RestoreProjectLink, SharedResourceType,
};

use crate::SqliteBackend;

impl ProjectLinkStore for SqliteBackend {
    async fn save(&self, link: &mut ProjectLink) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO project_links (id, source_project, target_project, resource_types, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                link.id().to_string(),
                link.source_project().to_string(),
                link.target_project().to_string(),
                serde_json::to_string(link.resource_types()).unwrap(),
                link.created_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: &ProjectLinkId) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "DELETE FROM project_links WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &ProjectLinkId) -> Result<Option<ProjectLink>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, source_project, target_project, resource_types, created_at
                 FROM project_links WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_row(rusqlite::params![id.to_string()], row_to_link)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))
    }

    async fn list_by_target(&self, target: &ProjectId) -> Result<Vec<ProjectLink>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, source_project, target_project, resource_types, created_at
                 FROM project_links WHERE target_project = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![target.to_string()], row_to_link)
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut links = Vec::new();
        for row in rows {
            links.push(row.map_err(|e| Error::Store(e.to_string()))?);
        }
        Ok(links)
    }

    async fn find_link(
        &self,
        source: &ProjectId,
        target: &ProjectId,
    ) -> Result<Option<ProjectLink>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, source_project, target_project, resource_types, created_at
                 FROM project_links WHERE source_project = ?1 AND target_project = ?2",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_row(
            rusqlite::params![source.to_string(), target.to_string()],
            row_to_link,
        )
        .optional()
        .map_err(|e| Error::Store(e.to_string()))
    }
}

fn row_to_link(row: &rusqlite::Row) -> rusqlite::Result<ProjectLink> {
    let id_str: String = row.get(0)?;
    let source_str: String = row.get(1)?;
    let target_str: String = row.get(2)?;
    let types_str: String = row.get(3)?;
    let created_at_str: String = row.get(4)?;

    let id: ProjectLinkId = id_str.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{e}"),
            )),
        )
    })?;
    let source_project = ProjectId::try_from(source_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let target_project = ProjectId::try_from(target_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let resource_types: Vec<SharedResourceType> =
        serde_json::from_str(&types_str).unwrap_or_default();
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(ProjectLink::restore(RestoreProjectLink {
        id,
        source_project,
        target_project,
        resource_types,
        created_at,
    }))
}
