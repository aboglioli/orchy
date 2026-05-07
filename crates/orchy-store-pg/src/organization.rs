use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row, postgres::PgRow};

use orchy_core::error::{Error, Result, StoreError};
use orchy_core::organization::{
    Organization, OrganizationId, OrganizationStore, RestoreOrganization,
};
use orchy_events::io::Writer;

use crate::events::PgEventWriter;

pub struct PgOrganizationStore {
    pool: PgPool,
}

impl PgOrganizationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrganizationStore for PgOrganizationStore {
    async fn save(&self, org: &mut Organization) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(crate::error::store_err)?;

        sqlx::query(
            "INSERT INTO organizations (id, name, created_at, updated_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(org.id().as_str())
        .bind(org.name())
        .bind(org.created_at())
        .bind(org.updated_at())
        .execute(&mut *tx)
        .await
        .map_err(crate::error::store_err)?;

        let events = org.drain_events();
        PgEventWriter::new_tx(&mut tx).write_all(&events).await?;

        tx.commit().await.map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let row =
            sqlx::query("SELECT id, name, created_at, updated_at FROM organizations WHERE id = $1")
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(crate::error::store_err)?;

        row.map(|r| build_org(&r)).transpose()
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let rows = sqlx::query(
            "SELECT id, name, created_at, updated_at FROM organizations ORDER BY created_at LIMIT 1000",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        rows.iter().map(build_org).collect()
    }
}

fn build_org(row: &PgRow) -> Result<Organization> {
    let id_str: String = row.get("id");
    let name: String = row.get("name");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    let id = OrganizationId::new(&id_str).map_err(|e| {
        Error::Store(StoreError::Decode {
            table: "organizations".to_string(),
            column: "id".to_string(),
            cause: e.to_string(),
        })
    })?;

    Ok(Organization::restore(RestoreOrganization {
        id,
        name,
        created_at,
        updated_at,
    }))
}
