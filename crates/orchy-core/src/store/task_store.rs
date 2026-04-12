use crate::entities::{CreateTask, Task, TaskFilter};
use crate::error::Result;
use crate::value_objects::{AgentId, TaskId, TaskStatus};

pub trait TaskStore: Send + Sync {
    async fn create(&self, task: CreateTask) -> Result<Task>;
    async fn get(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>>;
    async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task>;
    async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task>;
    async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task>;
    async fn release(&self, id: &TaskId) -> Result<Task>;
    async fn update(&self, task: &Task) -> Result<Task>;
    async fn update_status(&self, id: &TaskId, status: TaskStatus) -> Result<()>;
}
