use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{EntityKind, EntityRef, EventLog, Id, Result, Task, TaskQuery, TaskStore};

use crate::codec;
use crate::transaction::atomically;
use crate::vault::{Precondition, Vault};

pub struct VaultTaskStore {
    vault: Arc<Vault>,
    log: Arc<dyn EventLog>,
}

impl VaultTaskStore {
    pub fn new(vault: Arc<Vault>, log: Arc<dyn EventLog>) -> Self {
        Self { vault, log }
    }

    /// Keeps a parent's `subtasks` field in step with the children that name it (D43 stores
    /// the hierarchy on the child; this is the rendered inverse).
    async fn list_under(&self, parent: Option<&Id>, child: &Task, present: bool) -> Result<()> {
        let Some(parent) = parent else {
            return Ok(());
        };
        let reference = EntityRef::task(child.id().clone()).to_string();
        self.vault
            .amend_refs(parent, "subtasks", move |refs| {
                refs.retain(|r| r != &reference);
                if present {
                    refs.push(reference.clone());
                }
            })
            .await
            .map(drop)
    }

    /// The child names its parent and the parent lists the child; both land together.
    async fn save_now(&self, task: &mut Task) -> Result<()> {
        let events = task.drain_events();
        let (carried, previous_parent) = match self.vault.peek_by_id(task.id()).await? {
            Some((_, file)) => (
                codec::carried_frontmatter(&file),
                codec::task_from_markdown(&file)
                    .ok()
                    .and_then(|t| t.parent().cloned()),
            ),
            None => (Default::default(), None),
        };

        let key = self.vault.layout().task_key(task.id(), task.status());
        let file = codec::task_to_markdown(task, carried)?;
        self.vault
            .write_if(
                &key,
                &file,
                task.id(),
                EntityKind::Task,
                Precondition::Unchanged,
            )
            .await?;
        if previous_parent.as_ref() != task.parent() {
            self.list_under(previous_parent.as_ref(), task, false)
                .await?;
            self.list_under(task.parent(), task, true).await?;
        }
        self.log.append(&events).await
    }

    async fn all(&self) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        for (_, file) in self.vault.load_all(EntityKind::Task).await? {
            if let Ok(decoded) = codec::task_from_markdown(&file) {
                tasks.push(decoded);
            }
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
        codec::task_from_markdown(&file)
            .map_err(codec::at(&key))
            .map(Some)
    }

    async fn matching(&self, query: &TaskQuery) -> Result<Vec<Task>> {
        Ok(self
            .all()
            .await?
            .into_iter()
            .filter(|t| query.matches(t))
            .collect())
    }

    /// What a parent's status is derived from, so the children found are guarded: a sibling
    /// finishing at the same moment makes one of the two derivations run again.
    async fn children_of(&self, parent: &Id) -> Result<Vec<Task>> {
        let mut children = Vec::new();
        for (_, located, file) in self.vault.scan().await?.entries {
            if located.kind != EntityKind::Task {
                continue;
            }
            let Ok(task) = codec::task_from_markdown(&file) else {
                continue;
            };
            if task.parent() == Some(parent) {
                self.vault.guard(&located.key, located.seen);
                children.push(task);
            }
        }
        children.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(children)
    }

    async fn save(&self, task: &mut Task) -> Result<()> {
        atomically(
            &self.vault,
            Some(self.log.as_ref()),
            Box::pin(self.save_now(task)),
        )
        .await
    }
}
