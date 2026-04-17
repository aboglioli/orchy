use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock, RestoreResourceLock};

use crate::{PgBackend, parse_namespace, parse_project_id};

#[async_trait]
impl LockStore for PgBackend {
    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO resource_locks (organization_id, project, namespace, name, holder, acquired_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (project, namespace, name) DO UPDATE SET
                organization_id = EXCLUDED.organization_id,
                holder = EXCLUDED.holder,
                acquired_at = EXCLUDED.acquired_at,
                expires_at = EXCLUDED.expires_at",
        )
        .bind(lock.org_id().to_string())
        .bind(lock.project().to_string())
        .bind(lock.namespace().to_string())
        .bind(lock.name())
        .bind(lock.holder().as_uuid())
        .bind(lock.acquired_at())
        .bind(lock.expires_at())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = lock.drain_events();
        crate::write_events_in_tx(&mut tx, &events).await?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let row = sqlx::query(
            "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
             FROM resource_locks WHERE organization_id = $1 AND project = $2 AND namespace = $3 AND name = $4",
        )
        .bind(org.to_string())
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_resource_lock(&r)).transpose()
    }

    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM resource_locks WHERE organization_id = $1 AND project = $2 AND namespace = $3 AND name = $4",
        )
        .bind(org.to_string())
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
            "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
             FROM resource_locks WHERE holder = $1",
        )
        .bind(holder.as_uuid())
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
    let org_id_str: String = row.get("organization_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let name: String = row.get("name");
    let holder: Uuid = row.get("holder");
    let acquired_at: DateTime<Utc> = row.get("acquired_at");
    let expires_at: DateTime<Utc> = row.get("expires_at");

    Ok(ResourceLock::restore(RestoreResourceLock {
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid resource_locks.organization_id: {e}")))?,
        project: parse_project_id(project, "resource_locks", "project")?,
        namespace: parse_namespace(namespace, "resource_locks", "namespace")?,
        name,
        holder: AgentId::from_uuid(holder),
        acquired_at,
        expires_at,
    }))
}
