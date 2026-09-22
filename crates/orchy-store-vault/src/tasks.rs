use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{EntityKind, EventLog, Id, Page, PageRequest, Result, Task, TaskQuery, TaskStore};

use crate::codec;
use crate::vault::{Precondition, Vault};

pub struct VaultTaskStore {
    vault: Arc<Vault>,
    log: Arc<dyn EventLog>,
}

impl VaultTaskStore {
    pub fn new(vault: Arc<Vault>, log: Arc<dyn EventLog>) -> Self {
        Self { vault, log }
    }

    async fn all(&self) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        for (key, file) in self.vault.load_all(EntityKind::Task).await? {
            tasks.push(codec::task_from_markdown(&file, &key)?);
        }
        tasks.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(tasks)
    }
}

#[async_trait]
impl TaskStore for VaultTaskStore {
    async fn get(&self, id: &Id) -> Result<Option<Task>> {
        let Some((key, file)) = self.vault.read_by_id(id).await? else {
            return Ok(None);
        };
        if codec::kind_of(&file) != Some("task") {
            return Ok(None);
        }
        codec::task_from_markdown(&file, &key).map(Some)
    }

    async fn find(&self, query: &TaskQuery, page: PageRequest) -> Result<Page<Task>> {
        let matched: Vec<Task> = self
            .all()
            .await?
            .into_iter()
            .filter(|t| query.matches(t))
            .collect();
        Ok(Page::slice(matched, page))
    }

    async fn children_of(&self, parent: &Id) -> Result<Vec<Task>> {
        Ok(self
            .all()
            .await?
            .into_iter()
            .filter(|t| t.parent() == Some(parent))
            .collect())
    }

    async fn save(&self, task: &mut Task) -> Result<()> {
        let events = task.drain_events();
        let carried = match self.vault.peek_by_id(task.id()).await? {
            Some((_, file)) => codec::carried_frontmatter(&file),
            None => Default::default(),
        };

        let key = self.vault.layout().task_key(task.id(), task.status());
        let file = codec::task_to_markdown(task, carried);
        self.vault
            .write_if(
                &key,
                &file,
                task.id(),
                EntityKind::Task,
                Precondition::Unchanged,
            )
            .await?;
        self.log.append(&events).await
    }

    async fn delete(&self, id: &Id) -> Result<()> {
        self.vault.remove(id).await
    }
}
