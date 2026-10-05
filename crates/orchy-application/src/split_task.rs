use std::sync::Arc;

use orchy_core::{Clock, Id, IdGenerator, Task, TaskStore, Title, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

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
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl SplitTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            ids,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: SplitTaskCommand) -> ApplicationResult<SplitTaskResponse> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    /// Splitting the same goal the same way twice at once leaves one subtask per title: both
    /// read the siblings, both add to the parent's `subtasks`, and the one that lands second
    /// finds the parent changed, runs again and sees the sibling already there.
    async fn apply(&self, cmd: SplitTaskCommand) -> ApplicationResult<SplitTaskResponse> {
        let parent_id = Id::new(&cmd.task_id)?;
        let parent = self.tasks.require(&parent_id).await?;
        parent.ensure_open()?;

        let mut existing: Vec<String> = self
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
            let folded = title.as_str().to_lowercase();
            if existing.contains(&folded) {
                skipped.push(title.to_string());
                continue;
            }
            existing.push(folded);
            let mut child =
                Task::create(title, parent.namespace().clone(), &*self.ids, &*self.clock);
            child.attach_to(&parent, &*self.clock)?;
            self.tasks.save(&mut child).await?;
            created.push(TaskDto::from(&child));
        }

        Ok(SplitTaskResponse {
            parent: TaskDto::from(&parent),
            created,
            skipped,
        })
    }
}
