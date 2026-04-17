use async_trait::async_trait;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::MemoryBackend;

#[async_trait]
impl NamespaceStore for MemoryBackend {
    async fn register(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<()> {
        let mut namespaces = self
            .namespaces
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        namespaces.insert((org.to_string(), project.to_string(), namespace.to_string()));
        Ok(())
    }

    async fn list(&self, org: &OrganizationId, project: &ProjectId) -> Result<Vec<Namespace>> {
        let namespaces = self
            .namespaces
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let org_str = org.to_string();
        let project_str = project.to_string();
        let mut result: Vec<Namespace> = namespaces
            .iter()
            .filter(|(o, p, _)| *o == org_str && *p == project_str)
            .filter_map(|(_, _, ns)| Namespace::try_from(ns.as_str()).ok())
            .collect();
        result.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
        Ok(result)
    }
}
