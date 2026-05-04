use std::sync::Arc;

use orchy_store_conformance::{Backend, Bundle, conformance_suite};
use orchy_store_memory::{
    MemoryAgentStore, MemoryApiKeyStore, MemoryEdgeStore, MemoryKnowledgeStore, MemoryLockStore,
    MemoryMessageStore, MemoryState, MemoryTaskStore,
};

struct MemoryBackend;

#[async_trait::async_trait]
impl Backend for MemoryBackend {
    async fn build() -> Bundle {
        let state = Arc::new(MemoryState::new());
        Bundle {
            agents: Arc::new(MemoryAgentStore::new(state.clone())),
            tasks: Arc::new(MemoryTaskStore::new(state.clone())),
            messages: Arc::new(MemoryMessageStore::new(state.clone())),
            knowledge: Arc::new(MemoryKnowledgeStore::new(state.clone())),
            edges: Arc::new(MemoryEdgeStore::new(state.clone())),
            locks: Arc::new(MemoryLockStore::new(state.clone())),
            api_keys: Arc::new(MemoryApiKeyStore::new(state)),
        }
    }
}

conformance_suite!(MemoryBackend);
