use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::Row;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::project::{Project, ProjectStore, RestoreProject};

use crate::{PgBackend, decode_json_value, parse_project_id};

impl ProjectStore for PgBackend {
    async fn save(&self, project: &mut Project) -> Result<()> {
        let metadata_json = serde_json::to_value(project.metadata())
            .map_err(|e| Error::Store(format!("failed to serialize projects.metadata: {e}")))?;

        sqlx::query(
            "INSERT INTO projects (name, description, metadata, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (name) DO UPDATE SET
                description = EXCLUDED.description,
                metadata = EXCLUDED.metadata,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(project.id().to_string())
        .bind(project.description())
        .bind(&metadata_json)
        .bind(project.created_at())
        .bind(project.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = project.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &ProjectId) -> Result<Option<Project>> {
        let row = sqlx::query(
            "SELECT name, description, metadata, created_at, updated_at
             FROM projects WHERE name = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_project(&r)).transpose()
    }
}

fn row_to_project(row: &sqlx::postgres::PgRow) -> Result<Project> {
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
        description,
        metadata,
        created_at,
        updated_at,
    }))
}
