use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::note::Note;
use orchy_core::project::{Project, ProjectStore, RestoreProject};

use crate::SqliteBackend;

impl ProjectStore for SqliteBackend {
    async fn save(&self, project: &Project) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO projects (name, description, notes, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                project.id().to_string(),
                project.description(),
                serde_json::to_string(project.notes()).unwrap(),
                serde_json::to_string(project.metadata()).unwrap(),
                project.created_at().to_rfc3339(),
                project.updated_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &ProjectId) -> Result<Option<Project>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT name, description, notes, metadata, created_at, updated_at
                 FROM projects WHERE name = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_project)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }
}

fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<Project> {
    let name_str: String = row.get(0)?;
    let description: String = row.get(1)?;
    let notes_str: String = row.get(2)?;
    let metadata_str: String = row.get(3)?;
    let created_at_str: String = row.get(4)?;
    let updated_at_str: String = row.get(5)?;

    let id = ProjectId::try_from(name_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let notes: Vec<Note> = serde_json::from_str(&notes_str).unwrap_or_default();
    let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str).unwrap_or_default();
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Project::restore(RestoreProject {
        id,
        description,
        notes,
        metadata,
        created_at,
        updated_at,
    }))
}
