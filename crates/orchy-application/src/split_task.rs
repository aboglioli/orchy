use std::sync::Arc;

use orchy_core::task::TaskDeleted;
use orchy_core::{Clock, EventLog, Id, IdGenerator, Task, TaskStore, Title};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

const SETTLE_PASSES: u32 = 4;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SplitTaskCommand {
    pub task_id: String,
    pub titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitTaskResponse {
    pub parent: TaskDto,
    pub created: Vec<TaskDto>,
    pub skipped: Vec<String>,
}

pub struct SplitTask {
    tasks: Arc<dyn TaskStore>,
    log: Arc<dyn EventLog>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl SplitTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        log: Arc<dyn EventLog>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            tasks,
            log,
            ids,
            clock,
        }
    }

    pub async fn execute(&self, cmd: SplitTaskCommand) -> ApplicationResult<SplitTaskResponse> {
        let parent_id = Id::new(&cmd.task_id)?;
        let parent = self.tasks.require(&parent_id).await?;

        let existing: Vec<String> = self
            .tasks
            .children_of(&parent_id)
            .await?
            .iter()
            .map(|t| t.title().as_str().to_lowercase())
            .collect();

        let mut created = Vec::new();
        let mut skipped = Vec::new();

        for raw in &cmd.titles {
            let title = Title::new(raw)?;
            if existing.contains(&title.as_str().to_lowercase()) {
                skipped.push(title.to_string());
                continue;
            }
            let mut child =
                Task::create(title, parent.namespace().clone(), &*self.ids, &*self.clock);
            child.attach_to(parent_id.clone(), &*self.clock)?;
            self.tasks.save(&mut child).await?;
            created.push(child);
        }

        let (kept, dropped) = self.reconcile(&parent_id, created).await?;
        skipped.extend(dropped);

        Ok(SplitTaskResponse {
            parent: TaskDto::from(&parent),
            created: kept,
            skipped,
        })
    }

    /// The title check above reads the siblings before writing any, so two agents splitting a
    /// goal the same way both find it empty. With no transaction to put the writes in, the
    /// duplicate is settled afterwards: ids are time-ordered, so every process agrees which
    /// same-titled sibling came first, and each withdraws only what it wrote itself. Read until
    /// two readings agree, or a process that looked too early sees no duplicate to settle.
    async fn reconcile(
        &self,
        parent_id: &Id,
        created: Vec<Task>,
    ) -> ApplicationResult<(Vec<TaskDto>, Vec<String>)> {
        if created.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut siblings = self.tasks.children_of(parent_id).await?;
        for _ in 1..SETTLE_PASSES {
            let again = self.tasks.children_of(parent_id).await?;
            let settled = again.len() == siblings.len()
                && again.iter().zip(&siblings).all(|(a, b)| a.id() == b.id());
            siblings = again;
            if settled {
                break;
            }
        }

        let mut kept = Vec::new();
        let mut withdrawn = Vec::new();
        for child in created {
            let first = siblings
                .iter()
                .filter(|s| {
                    s.title()
                        .as_str()
                        .eq_ignore_ascii_case(child.title().as_str())
                })
                .all(|s| s.id() >= child.id());
            if first {
                kept.push(TaskDto::from(&child));
                continue;
            }
            self.tasks.delete(child.id()).await?;
            let deleted = TaskDeleted {
                id: child.id().clone(),
                namespace: child.namespace().clone(),
                reason: "a sibling with the same title was split out first".to_owned(),
                at: self.clock.now(),
            };
            self.log.append(&[Box::new(deleted)]).await?;
            withdrawn.push(child.title().to_string());
        }
        Ok((kept, withdrawn))
    }
}
