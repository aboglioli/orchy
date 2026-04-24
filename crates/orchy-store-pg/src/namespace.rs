use async_trait::async_trait;
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
        _org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO namespaces (project, namespace, created_at) VALUES ($1, $2, $3) ON CONFLICT (project, namespace) DO NOTHING",
        )
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn list(&self, _org: &OrganizationId, project: &ProjectId) -> Result<Vec<Namespace>> {
        let rows =
            sqlx::query("SELECT namespace FROM namespaces WHERE project = $1 ORDER BY namespace")
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
