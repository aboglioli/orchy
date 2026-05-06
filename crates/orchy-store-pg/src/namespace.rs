use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};

use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::organization::OrganizationId;

pub struct PgNamespaceStore {
    pool: PgPool,
}

impl PgNamespaceStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NamespaceStore for PgNamespaceStore {
    async fn register(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO namespaces (organization_id, project, namespace, created_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (organization_id, project, namespace) DO NOTHING",
        )
        .bind(org.to_string())
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn list(&self, org: &OrganizationId, project: &ProjectId) -> Result<Vec<Namespace>> {
        let rows = sqlx::query(
            "SELECT namespace FROM namespaces \
             WHERE organization_id = $1 AND project = $2 \
             ORDER BY namespace LIMIT 1000",
        )
        .bind(org.to_string())
        .bind(project.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            let ns_str: String = row.get("namespace");
            let ns = Namespace::try_from(ns_str.as_str())
                .map_err(|e| Error::Store(format!("invalid namespace in database: {e}")))?;
            result.push(ns);
        }
        Ok(result)
    }
}
