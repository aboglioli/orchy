use sqlx::Row;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};

use crate::PgBackend;

impl NamespaceStore for PgBackend {
    async fn register(&self, project: &ProjectId, namespace: &Namespace) -> Result<()> {
        sqlx::query(
            "INSERT INTO namespaces (project, namespace) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(project.to_string())
        .bind(namespace.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn list(&self, project: &ProjectId) -> Result<Vec<Namespace>> {
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
