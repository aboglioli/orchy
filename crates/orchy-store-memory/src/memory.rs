use orchy_core::error::{Error, Result};
use orchy_core::memory::{MemoryEntry, MemoryFilter, MemoryStore};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{MemoryBackend, cosine_similarity};

impl MemoryStore for MemoryBackend {
    async fn save(&self, entry: &mut MemoryEntry) -> Result<()> {
        {
            let key = (
                entry.project().to_string(),
                entry.namespace().to_string(),
                entry.key().to_string(),
            );

            let mut store = self
                .memory
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;

            store.insert(key, entry.clone());
        }

        let events = entry.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_key(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> Result<Option<MemoryEntry>> {
        let store = self
            .memory
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        let composite = (project.to_string(), namespace.to_string(), key.to_string());
        Ok(store.get(&composite).cloned())
    }

    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        let store = self
            .memory
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let results: Vec<MemoryEntry> = store
            .values()
            .filter(|entry| {
                if let Some(ref ns) = filter.namespace {
                    if !entry.namespace().starts_with(ns) {
                        return false;
                    }
                }
                if let Some(ref project) = filter.project {
                    if entry.project() != project {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        Ok(results)
    }

    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let store = self
            .memory
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut scored: Vec<(f32, &MemoryEntry)> = store
            .values()
            .filter(|entry| {
                if let Some(ns) = namespace {
                    if !entry.namespace().starts_with(ns) {
                        return false;
                    }
                }
                true
            })
            .filter_map(|entry| {
                let text_match = entry.value().contains(query);

                let sim = embedding
                    .and_then(|emb| entry.embedding().map(|e_emb| cosine_similarity(emb, e_emb)));

                if text_match || sim.map(|s| s > 0.0).unwrap_or(false) {
                    let score = if text_match { 1.0 } else { 0.0 } + sim.unwrap_or(0.0);
                    Some((score, entry))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, entry)| entry.clone()).collect())
    }

    async fn delete(&self, project: &ProjectId, namespace: &Namespace, key: &str) -> Result<()> {
        let mut store = self
            .memory
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        store.retain(|k, _| {
            !(k.0 == project.to_string() && k.1 == namespace.to_string() && k.2 == key)
        });
        Ok(())
    }
}
