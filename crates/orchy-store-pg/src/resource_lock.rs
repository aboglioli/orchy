use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock, RestoreResourceLock};

use crate::{PgBackend, parse_namespace, parse_project_id};

impl LockStore for PgBackend {
    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        sqlx::query(
            "INSERT INTO resource_locks (project, namespace, name, holder, acquired_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (project, namespace, name) DO UPDATE SET
                holder = EXCLUDED.holder,
                acquired_at = EXCLUDED.acquired_at,
                expires_at = EXCLUDED.expires_at",
        )
        .bind(lock.project().to_string())
        .bind(lock.namespace().to_string())
        .bind(lock.name())
        .bind(*lock.holder().as_uuid())
        .bind(lock.acquired_at())
        .bind(lock.expires_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = lock.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find(
        &self,
        _org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let row = sqlx::query(
            "SELECT project, namespace, name, holder, acquired_at, expires_at
             FROM resource_locks WHERE project = $1 AND namespace = $2 AND name = $3",
        )
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_resource_lock(&r)).transpose()
    }

    async fn delete(&self, _org: &OrganizationId, project: &ProjectId, namespace: &Namespace, name: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM resource_locks WHERE project = $1 AND namespace = $2 AND name = $3",
        )
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_holder(&self, holder: &AgentId) -> Result<Vec<ResourceLock>> {
        let rows = sqlx::query(
            "SELECT project, namespace, name, holder, acquired_at, expires_at
             FROM resource_locks WHERE holder = $1",
        )
        .bind(*holder.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_resource_lock).collect()
    }

    async fn delete_expired(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM resource_locks WHERE expires_at < NOW()")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result.rows_affected())
    }
}

fn row_to_resource_lock(row: &sqlx::postgres::PgRow) -> Result<ResourceLock> {
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let name: String = row.get("name");
    let holder: Uuid = row.get("holder");
    let acquired_at: DateTime<Utc> = row.get("acquired_at");
    let expires_at: DateTime<Utc> = row.get("expires_at");

    Ok(ResourceLock::restore(RestoreResourceLock {
        org_id: OrganizationId::new("default").unwrap(),
        project: parse_project_id(project, "resource_locks", "project")?,
        namespace: parse_namespace(namespace, "resource_locks", "namespace")?,
        name,
        holder: AgentId::from_uuid(holder),
        acquired_at,
        expires_at,
    }))
}
