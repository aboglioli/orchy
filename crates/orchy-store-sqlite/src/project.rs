use rusqlite::{Error as RusqliteError, Row as RusqliteRow, types::Type as RusqliteType};
use std::collections::HashMap;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result, StoreError};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore, RestoreProject};

use crate::{SqliteConn, blocking, blocking_tx, decode_json, events};

pub struct SqliteProjectStore {
    conn: SqliteConn,
}

impl SqliteProjectStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl ProjectStore for SqliteProjectStore {
    async fn save(&self, project: &mut Project) -> Result<()> {
        let org_id = project.org_id().to_string();
        let id = project.id().to_string();
        let description = project.description().to_owned();
        let metadata = serde_json::to_string(project.metadata())
            .map_err(|e| Error::Store(StoreError::Serialization(format!("metadata: {e}"))))?;
        let created_at = project.created_at().to_rfc3339();
        let updated_at = project.updated_at().to_rfc3339();
        let drained = project.drain_events();
        blocking_tx(&self.conn, move |tx| {
            tx.execute(
                "INSERT OR REPLACE INTO projects (organization_id, name, description, metadata, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![org_id, id, description, metadata, created_at, updated_at],
            )
            .map_err(crate::error::store_err)?;
            events::write_events_in_tx(tx, &drained)?;
            Ok(())
        })
        .await
    }

    async fn find_by_id(&self, org: &OrganizationId, id: &ProjectId) -> Result<Option<Project>> {
        let org = org.to_string();
        let id = id.to_string();
        blocking(&self.conn, move |conn| {
            conn.query_row(
                "SELECT organization_id, name, description, metadata, created_at, updated_at
                 FROM projects WHERE organization_id = ?1 AND name = ?2",
                rusqlite::params![org, id],
                row_to_project,
            )
            .optional()
            .map_err(crate::error::store_err)
        })
        .await
    }
}

fn row_to_project(row: &RusqliteRow) -> rusqlite::Result<Project> {
    let org_id_str: String = row.get(0)?;
    let name_str: String = row.get(1)?;
    let description: String = row.get(2)?;
    let metadata_str: String = row.get(3)?;
    let created_at_str: String = row.get(4)?;
    let updated_at_str: String = row.get(5)?;

    let org_id = OrganizationId::new(&org_id_str).map_err(|e| {
        RusqliteError::FromSqlConversionFailure(
            0,
            RusqliteType::Text,
            Box::new(IoError::new(IoErrorKind::InvalidData, e.to_string())),
        )
    })?;
    let id = ProjectId::try_from(name_str).map_err(|e| {
        RusqliteError::FromSqlConversionFailure(
            1,
            RusqliteType::Text,
            Box::new(IoError::new(IoErrorKind::InvalidData, e)),
        )
    })?;
    let metadata: HashMap<String, String> = decode_json(&metadata_str, "metadata")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RusqliteError::FromSqlConversionFailure(4, RusqliteType::Text, Box::new(e)))?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RusqliteError::FromSqlConversionFailure(5, RusqliteType::Text, Box::new(e)))?;

    Ok(Project::restore(RestoreProject {
        id,
        org_id,
        description,
        metadata,
        created_at,
        updated_at,
    }))
}
