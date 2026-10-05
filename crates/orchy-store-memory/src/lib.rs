mod documents;
mod edges;
mod eventlog;
mod integrity;
mod messages;
mod roster;
mod search;
mod skills;
mod tasks;
mod time;
mod unit_of_work;

pub use documents::MemoryDocumentStore;
pub use edges::MemoryEdgeStore;
pub use eventlog::MemoryEventLog;
pub use integrity::MemoryIntegrity;
pub use messages::{MemoryMessageStore, MemoryWatermarks};
pub use roster::{MemoryActorStore, MemoryLeaseStore};
pub use search::MemorySearch;
pub use skills::MemorySkillStore;
pub use tasks::MemoryTaskStore;
pub use time::{FixedClock, SeqIdGenerator};
pub use unit_of_work::MemoryUnitOfWork;

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
    pub skills: Arc<MemorySkillStore>,
    pub search: Arc<MemorySearch>,
    pub integrity: Arc<MemoryIntegrity>,
    pub log: Arc<MemoryEventLog>,
    pub clock: Arc<FixedClock>,
    pub ids: Arc<SeqIdGenerator>,
    pub unit_of_work: Arc<MemoryUnitOfWork>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        let log = Arc::new(MemoryEventLog::new());
        let clock = Arc::new(FixedClock::at(1_700_000_000));
        let documents = Arc::new(MemoryDocumentStore::new(Arc::clone(&log)));
        let skills = Arc::new(MemorySkillStore::new(Arc::clone(&log)));
        let tasks = Arc::new(MemoryTaskStore::new(Arc::clone(&log)));
        let messages = Arc::new(MemoryMessageStore::new(Arc::clone(&log)));
        let edges = Arc::new(MemoryEdgeStore::new(
            Arc::clone(&log) as _,
            Arc::clone(&clock) as _,
        ));
        let actors = Arc::new(MemoryActorStore::new(Arc::clone(&log) as _));
        let unit_of_work = Arc::new(MemoryUnitOfWork {
            documents: Arc::clone(&documents),
            tasks: Arc::clone(&tasks),
            messages: Arc::clone(&messages),
            skills: Arc::clone(&skills),
            edges: Arc::clone(&edges),
            actors: Arc::clone(&actors),
            log: Arc::clone(&log),
        });
        Self {
            search: Arc::new(MemorySearch::new(
                Arc::clone(&documents),
                Arc::clone(&skills),
            )),
            documents,
            tasks,
            messages,
            skills,
            edges,
            actors,
            unit_of_work,
            leases: Arc::new(MemoryLeaseStore::new(Arc::clone(&clock))),
            watermarks: Arc::new(MemoryWatermarks::new()),
            integrity: Arc::new(MemoryIntegrity::new()),
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
