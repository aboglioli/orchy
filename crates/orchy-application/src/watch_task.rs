use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::task::{TaskId, TaskStore, TaskWatcher, WatcherStore};

use crate::parse_namespace;

pub struct WatchTaskCommand {
    pub task_id: String,
    pub agent_id: String,
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
}

pub struct WatchTask {
    tasks: Arc<dyn TaskStore>,
    watchers: Arc<dyn WatcherStore>,
}

impl WatchTask {
    pub fn new(tasks: Arc<dyn TaskStore>, watchers: Arc<dyn WatcherStore>) -> Self {
        Self { tasks, watchers }
    }

    pub async fn execute(&self, cmd: WatchTaskCommand) -> Result<TaskWatcher> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let agent_id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        self.tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        let mut watcher = TaskWatcher::new(task_id, agent_id, org_id, project, namespace)?;
        self.watchers.save(&mut watcher).await?;
        Ok(watcher)
    }
}
