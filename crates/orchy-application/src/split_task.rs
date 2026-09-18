use std::sync::Arc;

use orchy_core::{Clock, Id, IdGenerator, Task, TaskStore, Title};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

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
}

impl SplitTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self { tasks, ids, clock }
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
            created.push(TaskDto::from(&child));
        }

        Ok(SplitTaskResponse {
            parent: TaskDto::from(&parent),
            created,
            skipped,
        })
    }
}
