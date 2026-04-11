use chrono::Utc;

use orchy_core::entities::{ContextSnapshot, CreateSnapshot};
use orchy_core::error::{Error, Result};
use orchy_core::store::ContextStore;
use orchy_core::value_objects::{AgentId, Namespace, SnapshotId};

use crate::{cosine_similarity, MemoryBackend};

impl ContextStore for MemoryBackend {
    async fn save(&self, cmd: CreateSnapshot) -> Result<ContextSnapshot> {
        let snapshot = ContextSnapshot {
            id: SnapshotId::new(),
            agent_id: cmd.agent_id,
            namespace: cmd.namespace,
            summary: cmd.summary,
            embedding: cmd.embedding,
            embedding_model: cmd.embedding_model,
            embedding_dimensions: cmd.embedding_dimensions,
            metadata: cmd.metadata,
            created_at: Utc::now(),
        };

        let mut contexts = self.contexts.write().map_err(|e| Error::Store(e.to_string()))?;
        contexts.insert(snapshot.id, snapshot.clone());
        Ok(snapshot)
    }

    async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        let contexts = self.contexts.read().map_err(|e| Error::Store(e.to_string()))?;

        Ok(contexts
            .values()
            .filter(|s| s.agent_id == *agent)
            .max_by_key(|s| s.created_at)
            .cloned())
    }

    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<ContextSnapshot>> {
        let contexts = self.contexts.read().map_err(|e| Error::Store(e.to_string()))?;

        Ok(contexts
            .values()
            .filter(|s| {
                if let Some(a) = agent {
                    if s.agent_id != *a {
                        return false;
                    }
                }
                if let Some(ns) = namespace {
                    if let Some(ref s_ns) = s.namespace {
                        if !s_ns.starts_with(ns) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect())
    }

    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        let contexts = self.contexts.read().map_err(|e| Error::Store(e.to_string()))?;

        let mut scored: Vec<(f32, &ContextSnapshot)> = contexts
            .values()
            .filter(|s| {
                if let Some(a) = agent_id {
                    if s.agent_id != *a {
                        return false;
                    }
                }
                if let Some(ns) = namespace {
                    if let Some(ref s_ns) = s.namespace {
                        if !s_ns.starts_with(ns) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                true
            })
            .filter_map(|s| {
                let text_match = s.summary.contains(query);

                let sim = embedding.and_then(|emb| {
                    s.embedding.as_ref().map(|s_emb| cosine_similarity(emb, s_emb))
                });

                if text_match || sim.map(|v| v > 0.0).unwrap_or(false) {
                    let score = if text_match { 1.0 } else { 0.0 } + sim.unwrap_or(0.0);
                    Some((score, s))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, s)| s.clone()).collect())
    }
}
