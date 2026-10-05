use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{Result, UnitOfWork, Work};

use crate::{
    MemoryActorStore, MemoryDocumentStore, MemoryEdgeStore, MemoryEventLog, MemoryMessageStore,
    MemorySkillStore, MemoryTaskStore,
};

pub struct MemoryUnitOfWork {
    pub(crate) documents: Arc<MemoryDocumentStore>,
    pub(crate) tasks: Arc<MemoryTaskStore>,
    pub(crate) messages: Arc<MemoryMessageStore>,
    pub(crate) skills: Arc<MemorySkillStore>,
    pub(crate) edges: Arc<MemoryEdgeStore>,
    pub(crate) actors: Arc<MemoryActorStore>,
    pub(crate) log: Arc<MemoryEventLog>,
}

#[async_trait]
impl UnitOfWork for MemoryUnitOfWork {
    async fn run<'a>(&self, work: Work<'a>) -> Result<()> {
        let documents = self.documents.state();
        let tasks = self.tasks.state();
        let messages = self.messages.state();
        let skills = self.skills.state();
        let edges = self.edges.state();
        let actors = self.actors.state();
        let log = self.log.state();

        let result = work.await;
        if result.is_err() {
            self.documents.restore_state(documents);
            self.tasks.restore_state(tasks);
            self.messages.restore_state(messages);
            self.skills.restore_state(skills);
            self.edges.restore_state(edges);
            self.actors.restore_state(actors);
            self.log.restore_state(log);
        }
        result
    }
}
