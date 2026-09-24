mod documents;
mod edges;
mod eventlog;
mod messages;
mod roster;
mod search;
mod tasks;
mod time;

pub use documents::MemoryDocumentStore;
pub use edges::MemoryEdgeStore;
pub use eventlog::MemoryEventLog;
pub use messages::{MemoryMessageStore, MemoryWatermarks};
pub use roster::{MemoryActorStore, MemoryLeaseStore};
pub use search::MemorySearch;
pub use tasks::MemoryTaskStore;
pub use time::{FixedClock, SeqIdGenerator};

use std::sync::Arc;

#[derive(Clone)]
pub struct MemoryBackend {
    pub documents: Arc<MemoryDocumentStore>,
    pub tasks: Arc<MemoryTaskStore>,
    pub messages: Arc<MemoryMessageStore>,
    pub edges: Arc<MemoryEdgeStore>,
    pub actors: Arc<MemoryActorStore>,
    pub leases: Arc<MemoryLeaseStore>,
    pub watermarks: Arc<MemoryWatermarks>,
    pub search: Arc<MemorySearch>,
    pub log: Arc<MemoryEventLog>,
    pub clock: Arc<FixedClock>,
    pub ids: Arc<SeqIdGenerator>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        let log = Arc::new(MemoryEventLog::new());
        let clock = Arc::new(FixedClock::at(1_700_000_000));
        let documents = Arc::new(MemoryDocumentStore::new(Arc::clone(&log)));
        Self {
            search: Arc::new(MemorySearch::new(Arc::clone(&documents))),
            documents,
            tasks: Arc::new(MemoryTaskStore::new(Arc::clone(&log))),
            messages: Arc::new(MemoryMessageStore::new(Arc::clone(&log))),
            edges: Arc::new(MemoryEdgeStore::new()),
            actors: Arc::new(MemoryActorStore::new()),
            leases: Arc::new(MemoryLeaseStore::new(Arc::clone(&clock))),
            watermarks: Arc::new(MemoryWatermarks::new()),
            log,
            clock,
            ids: Arc::new(SeqIdGenerator::new()),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}
