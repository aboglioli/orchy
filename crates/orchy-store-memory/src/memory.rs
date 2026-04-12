use chrono::Utc;

use orchy_core::entities::{MemoryEntry, MemoryFilter, WriteMemory};
use orchy_core::error::{Error, Result};
use orchy_core::store::MemoryStore;
use orchy_core::value_objects::{Namespace, Version};

use crate::{MemoryBackend, cosine_similarity};

impl MemoryStore for MemoryBackend {
    async fn write(&self, cmd: WriteMemory) -> Result<MemoryEntry> {
        let now = Utc::now();
        let key = (cmd.namespace.to_string(), cmd.key.clone());

        let mut store = self
            .memory
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;

        let entry = if let Some(existing) = store.get(&key) {
            // Update existing entry
            if let Some(expected) = cmd.expected_version {
                if existing.version != expected {
                    return Err(Error::VersionMismatch {
                        expected: expected.as_u64(),
                        actual: existing.version.as_u64(),
                    });
                }
            }

            MemoryEntry {
                namespace: existing.namespace.clone(),
                key: existing.key.clone(),
                value: cmd.value,
                version: existing.version.next(),
                embedding: cmd.embedding.or_else(|| existing.embedding.clone()),
                embedding_model: cmd
                    .embedding_model
                    .or_else(|| existing.embedding_model.clone()),
                embedding_dimensions: cmd.embedding_dimensions.or(existing.embedding_dimensions),
                written_by: cmd.written_by.or(existing.written_by),
                created_at: existing.created_at,
                updated_at: now,
            }
        } else {
            // Create new entry
            if let Some(expected) = cmd.expected_version {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: 0,
                });
            }

            MemoryEntry {
                namespace: cmd.namespace,
                key: cmd.key,
                value: cmd.value,
                version: Version::initial(),
                embedding: cmd.embedding,
                embedding_model: cmd.embedding_model,
                embedding_dimensions: cmd.embedding_dimensions,
                written_by: cmd.written_by,
                created_at: now,
                updated_at: now,
            }
        };

        store.insert(key, entry.clone());
        Ok(entry)
    }

    async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        let store = self
            .memory
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        let composite = (namespace.to_string(), key.to_string());
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
                    entry.namespace.starts_with(ns)
                } else {
                    true
                }
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
                    if !entry.namespace.starts_with(ns) {
                        return false;
                    }
                }
                true
            })
            .filter_map(|entry| {
                let text_match = entry.value.contains(query);

                let sim = embedding.and_then(|emb| {
                    entry
                        .embedding
                        .as_ref()
                        .map(|e_emb| cosine_similarity(emb, e_emb))
                });

                // Include if text matches or has positive similarity
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

    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()> {
        let mut store = self
            .memory
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let composite = (namespace.to_string(), key.to_string());
        store.remove(&composite);
        Ok(())
    }
}
