use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore};
use orchy_core::resource_ref::ResourceRef;
use orchy_core::task::{Task, TaskId, TaskStore};

pub struct AddTaskNoteCommand {
    pub task_id: String,
    pub body: String,
    pub author: Option<String>,
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
}

pub struct AddTaskNote {
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
}

impl AddTaskNote {
    pub fn new(tasks: Arc<dyn TaskStore>, knowledge: Arc<dyn KnowledgeStore>) -> Self {
        Self { tasks, knowledge }
    }

    pub async fn execute(&self, cmd: AddTaskNoteCommand) -> Result<Task> {
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

        let namespace = crate::parse_namespace(cmd.namespace.as_deref())?;
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

        task.add_ref(
            ResourceRef::knowledge(entry.id().to_string()).with_display("task note".to_string()),
        );
        self.tasks.save(&mut task).await?;

        Ok(task)
    }
}
