use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::Row;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::note::Note;
use orchy_core::project::{Project, ProjectStore, RestoreProject};

use crate::PgBackend;

impl ProjectStore for PgBackend {
    async fn save(&self, project: &mut Project) -> Result<()> {
        let notes_json = serde_json::to_value(project.notes()).unwrap();
        let metadata_json = serde_json::to_value(project.metadata()).unwrap();

        sqlx::query(
            "INSERT INTO projects (name, description, notes, metadata, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (name) DO UPDATE SET
                description = EXCLUDED.description,
                notes = EXCLUDED.notes,
                metadata = EXCLUDED.metadata,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(project.id().to_string())
        .bind(project.description())
        .bind(&notes_json)
        .bind(&metadata_json)
        .bind(project.created_at())
        .bind(project.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = project.drain_events();
        for evt in &events {
            if let Ok(serialized) = orchy_events::SerializedEvent::from_event(evt) {
                let id = uuid::Uuid::parse_str(&serialized.id).unwrap();
                let _ = sqlx::query(
                    "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                )
                .bind(id)
                .bind(&serialized.organization)
                .bind(&serialized.namespace)
                .bind(&serialized.topic)
                .bind(&serialized.payload)
                .bind(&serialized.content_type)
                .bind(serde_json::to_value(&serialized.metadata).unwrap())
                .bind(serialized.timestamp)
                .bind(serialized.version as i64)
                .execute(&self.pool)
                .await;
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &ProjectId) -> Result<Option<Project>> {
        let row = sqlx::query(
            "SELECT name, description, notes, metadata, created_at, updated_at
             FROM projects WHERE name = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_project(&r)))
    }
}

fn row_to_project(row: &sqlx::postgres::PgRow) -> Project {
    let name: String = row.get("name");
    let description: String = row.get("description");
    let notes_json: serde_json::Value = row.get("notes");
    let metadata_json: serde_json::Value = row.get("metadata");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    let id = ProjectId::try_from(name).expect("invalid project name in database");
    let notes: Vec<Note> = serde_json::from_value(notes_json).unwrap_or_default();
    let metadata: HashMap<String, String> =
        serde_json::from_value(metadata_json).unwrap_or_default();

    Project::restore(RestoreProject {
        id,
        description,
        notes,
        metadata,
        created_at,
        updated_at,
    })
}
