use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};

use crate::SqliteBackend;

impl NamespaceStore for SqliteBackend {
    async fn register(&self, project: &ProjectId, namespace: &Namespace) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR IGNORE INTO namespaces (project, namespace, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                project.to_string(),
                namespace.to_string(),
                chrono::Utc::now().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn list(&self, project: &ProjectId) -> Result<Vec<Namespace>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT namespace FROM namespaces WHERE project = ?1 ORDER BY namespace")
            .map_err(|e| Error::Store(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![project.to_string()], |row| {
                let ns: String = row.get(0)?;
                Ok(ns)
            })
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            let ns_str = row.map_err(|e| Error::Store(e.to_string()))?;
            let ns = Namespace::try_from(ns_str.as_str())
                .map_err(|e| Error::Store(format!("invalid namespace in database: {e}")))?;
            result.push(ns);
        }
        Ok(result)
    }
}
