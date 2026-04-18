use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct AddTaskNoteCommand {
    pub task_id: String,
    pub body: String,
    pub author: Option<String>,
}

pub struct AddTaskNote {
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
}

impl AddTaskNote {
    pub fn new(tasks: Arc<dyn TaskStore>, knowledge: Arc<dyn KnowledgeStore>) -> Self {
        Self { tasks, knowledge }
    }

    pub async fn execute(&self, cmd: AddTaskNoteCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let author = cmd
            .author
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        let namespace = task.namespace().clone();
        let title = if cmd.body.len() > 80 {
            cmd.body[..80].to_string()
        } else {
            cmd.body.clone()
        };

        let mut entry = Knowledge::new(
            task.org_id().clone(),
            Some(task.project().clone()),
            namespace,
            format!("tasks/{}/notes/{}", task_id, uuid::Uuid::now_v7()),
            KnowledgeKind::Note,
            title,
            cmd.body,
            vec![format!("task:{task_id}")],
            author,
            HashMap::new(),
        )?;
        self.knowledge.save(&mut entry).await?;
        self.tasks.save(&mut task).await?;

        Ok(TaskResponse::from(&task))
    }
}
