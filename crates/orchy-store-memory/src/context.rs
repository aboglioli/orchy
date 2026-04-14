use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{ContextSnapshot, ContextStore};
use orchy_core::namespace::Namespace;

use crate::{MemoryBackend, cosine_similarity};

impl ContextStore for MemoryBackend {
    async fn save(&self, snapshot: &mut ContextSnapshot) -> Result<()> {
        {
            let mut contexts = self
                .contexts
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            contexts.insert(snapshot.id(), snapshot.clone());
        }

        let events = snapshot.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_latest(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        let contexts = self
            .contexts
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(contexts
            .values()
            .filter(|s| s.agent_id() == *agent)
            .max_by_key(|s| s.created_at())
            .cloned())
    }

    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        let contexts = self
            .contexts
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(contexts
            .values()
            .filter(|s| {
                if let Some(a) = agent {
                    if s.agent_id() != *a {
                        return false;
                    }
                }
                s.namespace().starts_with(namespace)
            })
            .cloned()
            .collect())
    }

    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        let contexts = self
            .contexts
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut scored: Vec<(f32, &ContextSnapshot)> = contexts
            .values()
            .filter(|s| {
                if let Some(a) = agent_id {
                    if s.agent_id() != *a {
                        return false;
                    }
                }
                s.namespace().starts_with(namespace)
            })
            .filter_map(|s| {
                let text_match = s.summary().contains(query);

                let sim = embedding
                    .and_then(|emb| s.embedding().map(|s_emb| cosine_similarity(emb, s_emb)));

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
