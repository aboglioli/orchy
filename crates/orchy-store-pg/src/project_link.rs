use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::project_link::{
    ProjectLink, ProjectLinkId, ProjectLinkStore, RestoreProjectLink, SharedResourceType,
};

use crate::{PgBackend, decode_json_value, parse_project_id};

impl ProjectLinkStore for PgBackend {
    async fn save(&self, link: &mut ProjectLink) -> Result<()> {
        let types_json = serde_json::to_value(link.resource_types()).map_err(|e| {
            Error::Store(format!(
                "failed to serialize project_links.resource_types: {e}"
            ))
        })?;

        sqlx::query(
            "INSERT INTO project_links (id, source_project, target_project, resource_types, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
                resource_types = EXCLUDED.resource_types",
        )
        .bind(*link.id().as_uuid())
        .bind(link.source_project().to_string())
        .bind(link.target_project().to_string())
        .bind(&types_json)
        .bind(link.created_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = link.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn delete(&self, id: &ProjectLinkId) -> Result<()> {
        sqlx::query("DELETE FROM project_links WHERE id = $1")
            .bind(*id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &ProjectLinkId) -> Result<Option<ProjectLink>> {
        let row = sqlx::query(
            "SELECT id, source_project, target_project, resource_types, created_at
             FROM project_links WHERE id = $1",
        )
        .bind(*id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_link(&r)).transpose()
    }

    async fn list_by_target(&self, target: &ProjectId) -> Result<Vec<ProjectLink>> {
        let rows = sqlx::query(
            "SELECT id, source_project, target_project, resource_types, created_at
             FROM project_links WHERE target_project = $1",
        )
        .bind(target.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_link).collect()
    }

    async fn find_link(
        &self,
        source: &ProjectId,
        target: &ProjectId,
    ) -> Result<Option<ProjectLink>> {
        let row = sqlx::query(
            "SELECT id, source_project, target_project, resource_types, created_at
             FROM project_links WHERE source_project = $1 AND target_project = $2",
        )
        .bind(source.to_string())
        .bind(target.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_link(&r)).transpose()
    }
}

fn row_to_link(row: &sqlx::postgres::PgRow) -> Result<ProjectLink> {
    let id: Uuid = row.get("id");
    let source: String = row.get("source_project");
    let target: String = row.get("target_project");
    let types_json: serde_json::Value = row.get("resource_types");
    let created_at: DateTime<Utc> = row.get("created_at");

    let resource_types: Vec<SharedResourceType> =
        decode_json_value(types_json, "project_links", "resource_types")?;

    Ok(ProjectLink::restore(RestoreProjectLink {
        id: ProjectLinkId::from_uuid(id),
        source_project: parse_project_id(source, "project_links", "source_project")?,
        target_project: parse_project_id(target, "project_links", "target_project")?,
        resource_types,
        created_at,
    }))
}
