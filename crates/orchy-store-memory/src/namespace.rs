use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};

use crate::MemoryBackend;

impl NamespaceStore for MemoryBackend {
    async fn register(&self, project: &ProjectId, namespace: &Namespace) -> Result<()> {
        let mut namespaces = self
            .namespaces
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        namespaces.insert((project.to_string(), namespace.to_string()));
        Ok(())
    }

    async fn list(&self, project: &ProjectId) -> Result<Vec<Namespace>> {
        let namespaces = self
            .namespaces
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let project_str = project.to_string();
        let mut result: Vec<Namespace> = namespaces
            .iter()
            .filter(|(p, _)| *p == project_str)
            .filter_map(|(_, ns)| Namespace::try_from(ns.as_str()).ok())
            .collect();
        result.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
        Ok(result)
    }
}
