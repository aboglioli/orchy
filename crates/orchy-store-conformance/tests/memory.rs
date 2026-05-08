use std::sync::Arc;

use orchy_store_conformance::{Backend, Bundle, conformance_suite};
use orchy_store_memory::{
    MemoryAgentStore, MemoryApiKeyStore, MemoryEdgeStore, MemoryKnowledgeStore, MemoryLockStore,
    MemoryMessageStore, MemoryState, MemoryTaskStore,
};

struct MemoryBackend;

impl Backend for MemoryBackend {
    fn build() -> std::pin::Pin<Box<dyn std::future::Future<Output = Bundle> + Send>> {
        Box::pin(async {
            let state = Arc::new(MemoryState::new());
            Bundle {
                agents: Arc::new(MemoryAgentStore::new(Arc::clone(&state))),
                tasks: Arc::new(MemoryTaskStore::new(Arc::clone(&state))),
                messages: Arc::new(MemoryMessageStore::new(Arc::clone(&state))),
                knowledge: Arc::new(MemoryKnowledgeStore::new(Arc::clone(&state))),
                edges: Arc::new(MemoryEdgeStore::new(Arc::clone(&state))),
                locks: Arc::new(MemoryLockStore::new(Arc::clone(&state))),
                api_keys: Arc::new(MemoryApiKeyStore::new(state)),
            }
        })
    }
}

conformance_suite!(MemoryBackend);
