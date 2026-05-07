use async_trait::async_trait;

use orchy_core::error::{Error, Result, StoreError};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::SqliteConn;

pub struct SqliteNamespaceStore {
    conn: SqliteConn,
}

impl SqliteNamespaceStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl NamespaceStore for SqliteNamespaceStore {
    async fn register(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        conn.execute(
            "INSERT OR IGNORE INTO namespaces (organization_id, project, namespace, created_at) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                org.to_string(),
                project.to_string(),
                namespace.to_string(),
                chrono::Utc::now().to_rfc3339(),
            ],
        )
        .map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn list(&self, org: &OrganizationId, project: &ProjectId) -> Result<Vec<Namespace>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let mut stmt = conn
            .prepare(
                "SELECT namespace FROM namespaces \
                 WHERE organization_id = ?1 AND project = ?2 \
                 ORDER BY namespace",
            )
            .map_err(crate::error::store_err)?;

        let rows = stmt
            .query_map(
                rusqlite::params![org.to_string(), project.to_string()],
                |row| {
                    let ns: String = row.get(0)?;
                    Ok(ns)
                },
            )
            .map_err(crate::error::store_err)?;

        let mut result = Vec::new();
        for row in rows {
            let ns_str = row.map_err(crate::error::store_err)?;
            let ns = Namespace::try_from(ns_str.as_str()).map_err(|e| {
                Error::Store(StoreError::Other(format!(
                    "invalid namespace in database: {e}"
                )))
            })?;
            result.push(ns);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rusqlite::Connection;

    use super::*;

    fn open_in_memory() -> SqliteConn {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE namespaces (\
                organization_id TEXT NOT NULL DEFAULT 'default',\
                project TEXT NOT NULL,\
                namespace TEXT NOT NULL,\
                created_at TEXT NOT NULL,\
                PRIMARY KEY (organization_id, project, namespace)\
            );",
        )
        .unwrap();
        Arc::new(Mutex::new(conn))
    }

    #[tokio::test]
    async fn isolates_orgs() {
        let conn = open_in_memory();
        let store = SqliteNamespaceStore::new(conn);

        let org_a = OrganizationId::new("org-a").unwrap();
        let org_b = OrganizationId::new("org-b").unwrap();
        let project = ProjectId::try_from("demo").unwrap();
        let ns = Namespace::try_from("/backend").unwrap();

        store.register(&org_a, &project, &ns).await.unwrap();

        let in_a = store.list(&org_a, &project).await.unwrap();
        let in_b = store.list(&org_b, &project).await.unwrap();

        assert_eq!(in_a.len(), 1);
        assert_eq!(in_a[0].as_str(), "/backend");
        assert!(in_b.is_empty(), "org-b must not see org-a's namespace");
    }

    #[tokio::test]
    async fn register_is_idempotent_per_org() {
        let conn = open_in_memory();
        let store = SqliteNamespaceStore::new(conn);

        let org = OrganizationId::new("org-a").unwrap();
        let project = ProjectId::try_from("demo").unwrap();
        let ns = Namespace::try_from("/backend").unwrap();

        store.register(&org, &project, &ns).await.unwrap();
        store.register(&org, &project, &ns).await.unwrap();

        let listed = store.list(&org, &project).await.unwrap();
        assert_eq!(listed.len(), 1);
    }
}
