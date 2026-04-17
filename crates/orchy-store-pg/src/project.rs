use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore, RestoreProject};

use crate::{PgBackend, decode_json_value, parse_project_id};

#[async_trait]
impl ProjectStore for PgBackend {
    async fn save(&self, project: &mut Project) -> Result<()> {
        let metadata_json = serde_json::to_value(project.metadata())
            .map_err(|e| Error::Store(format!("failed to serialize projects.metadata: {e}")))?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO projects (organization_id, name, description, metadata, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (organization_id, name) DO UPDATE SET
                description = EXCLUDED.description,
                metadata = EXCLUDED.metadata,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(project.org_id().to_string())
        .bind(project.id().to_string())
        .bind(project.description())
        .bind(&metadata_json)
        .bind(project.created_at())
        .bind(project.updated_at())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = project.drain_events();
        crate::write_events_in_tx(&mut tx, &events).await?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, org: &OrganizationId, id: &ProjectId) -> Result<Option<Project>> {
        let row = sqlx::query(
            "SELECT organization_id, name, description, metadata, created_at, updated_at
             FROM projects WHERE organization_id = $1 AND name = $2",
        )
        .bind(org.to_string())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_project(&r)).transpose()
    }
}

fn row_to_project(row: &sqlx::postgres::PgRow) -> Result<Project> {
    let org_id_str: String = row.get("organization_id");
    let name: String = row.get("name");
    let description: String = row.get("description");
    let metadata_json: serde_json::Value = row.get("metadata");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    let id = parse_project_id(name, "projects", "name")?;
    let metadata: HashMap<String, String> =
        decode_json_value(metadata_json, "projects", "metadata")?;

    Ok(Project::restore(RestoreProject {
        id,
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid projects.organization_id: {e}")))?,
        description,
        metadata,
        created_at,
        updated_at,
    }))
}
