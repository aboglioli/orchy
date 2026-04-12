use crate::entities::Task;
use crate::error::{Error, Result};
use crate::value_objects::{AgentId, TaskStatus};

pub struct TaskAggregate;

impl TaskAggregate {
    pub fn claim(task: &mut Task, agent: AgentId) -> Result<()> {
        if task.status != TaskStatus::Pending {
            return Err(Error::InvalidTransition {
                from: task.status.to_string(),
                to: TaskStatus::Claimed.to_string(),
            });
        }
        task.status = task.status.transition_to(TaskStatus::Claimed)?;
        task.claimed_by = Some(agent);
        task.claimed_at = Some(chrono::Utc::now());
        Ok(())
    }

    pub fn start(task: &mut Task, agent: &AgentId) -> Result<()> {
        if task.claimed_by != Some(*agent) {
            return Err(Error::InvalidInput(format!(
                "task {} is not claimed by agent {}",
                task.id, agent
            )));
        }
        task.status = task.status.transition_to(TaskStatus::InProgress)?;
        Ok(())
    }

    pub fn complete(task: &mut Task, summary: Option<String>) -> Result<()> {
        task.status = task.status.transition_to(TaskStatus::Completed)?;
        task.result_summary = summary;
        Ok(())
    }

    pub fn fail(task: &mut Task, reason: Option<String>) -> Result<()> {
        if !matches!(task.status, TaskStatus::InProgress | TaskStatus::Claimed) {
            return Err(Error::InvalidTransition {
                from: task.status.to_string(),
                to: TaskStatus::Failed.to_string(),
            });
        }
        task.status = task.status.transition_to(TaskStatus::Failed)?;
        task.result_summary = reason;
        Ok(())
    }

    pub fn release(task: &mut Task) -> Result<()> {
        if !matches!(task.status, TaskStatus::Claimed | TaskStatus::InProgress) {
            return Err(Error::InvalidTransition {
                from: task.status.to_string(),
                to: TaskStatus::Pending.to_string(),
            });
        }
        task.status = task.status.transition_to(TaskStatus::Pending)?;
        task.claimed_by = None;
        task.claimed_at = None;
        Ok(())
    }
}
