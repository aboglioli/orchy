use crate::agent::AgentId;
use crate::error::{Error, Result};

use super::{Task, TaskStatus};

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
        task.status = task.status.transition_to(TaskStatus::Failed)?;
        task.result_summary = reason;
        Ok(())
    }

    pub fn reassign(task: &mut Task, new_agent: AgentId) -> Result<()> {
        if !matches!(task.status, TaskStatus::Claimed | TaskStatus::InProgress) {
            return Err(Error::InvalidInput(format!(
                "task {} cannot be reassigned from status {}",
                task.id, task.status
            )));
        }
        task.status = TaskStatus::Claimed;
        task.claimed_by = Some(new_agent);
        task.claimed_at = Some(chrono::Utc::now());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::Namespace;
    use crate::task::{Priority, TaskId};

    fn make_task(status: TaskStatus, claimed_by: Option<AgentId>) -> Task {
        Task {
            id: TaskId::new(),
            namespace: Namespace::try_from("test".to_string()).unwrap(),
            title: "Test Task".to_string(),
            description: "Test".to_string(),
            status,
            priority: Priority::default(),
            assigned_roles: vec!["tester".to_string()],
            claimed_by,
            claimed_at: None,
            depends_on: vec![],
            result_summary: None,
            created_by: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn claim_succeeds_from_pending() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Pending, None);
        let result = TaskAggregate::claim(&mut task, agent);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Claimed);
        assert_eq!(task.claimed_by, Some(agent));
        assert!(task.claimed_at.is_some());
    }

    #[test]
    fn claim_fails_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        let result = TaskAggregate::claim(&mut task, agent);
        assert!(result.is_err());
    }

    #[test]
    fn claim_fails_from_in_progress() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent));
        let result = TaskAggregate::claim(&mut task, agent);
        assert!(result.is_err());
    }

    #[test]
    fn start_succeeds_when_claimed_by_agent() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        let result = TaskAggregate::start(&mut task, &agent);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::InProgress);
    }

    #[test]
    fn start_fails_when_not_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, None);
        let result = TaskAggregate::start(&mut task, &agent);
        assert!(result.is_err());
    }

    #[test]
    fn start_fails_when_claimed_by_different_agent() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent1));
        let result = TaskAggregate::start(&mut task, &agent2);
        assert!(result.is_err());
    }

    #[test]
    fn complete_succeeds_from_in_progress() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent));
        let result = TaskAggregate::complete(&mut task, Some("done".to_string()));
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.result_summary, Some("done".to_string()));
    }

    #[test]
    fn complete_fails_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        let result = TaskAggregate::complete(&mut task, None);
        assert!(result.is_err());
    }

    #[test]
    fn complete_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        let result = TaskAggregate::complete(&mut task, None);
        assert!(result.is_err());
    }

    #[test]
    fn fail_succeeds_from_in_progress() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent));
        let result = TaskAggregate::fail(&mut task, Some("error".to_string()));
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Failed);
        assert_eq!(task.result_summary, Some("error".to_string()));
    }

    #[test]
    fn fail_succeeds_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        let result = TaskAggregate::fail(&mut task, None);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Failed);
    }

    #[test]
    fn fail_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        let result = TaskAggregate::fail(&mut task, None);
        assert!(result.is_err());
    }

    #[test]
    fn fail_fails_from_completed() {
        let mut task = make_task(TaskStatus::Completed, None);
        let result = TaskAggregate::fail(&mut task, None);
        assert!(result.is_err());
    }

    #[test]
    fn release_succeeds_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        let result = TaskAggregate::release(&mut task);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.claimed_by.is_none());
        assert!(task.claimed_at.is_none());
    }

    #[test]
    fn release_succeeds_from_in_progress() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent));
        let result = TaskAggregate::release(&mut task);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.claimed_by.is_none());
        assert!(task.claimed_at.is_none());
    }

    #[test]
    fn release_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        let result = TaskAggregate::release(&mut task);
        assert!(result.is_err());
    }

    #[test]
    fn release_fails_from_completed() {
        let mut task = make_task(TaskStatus::Completed, None);
        let result = TaskAggregate::release(&mut task);
        assert!(result.is_err());
    }

    #[test]
    fn reassign_succeeds_from_claimed() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent1));
        let result = TaskAggregate::reassign(&mut task, agent2);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Claimed);
        assert_eq!(task.claimed_by, Some(agent2));
        assert!(task.claimed_at.is_some());
    }

    #[test]
    fn reassign_succeeds_from_in_progress() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent1));
        let result = TaskAggregate::reassign(&mut task, agent2);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Claimed);
        assert_eq!(task.claimed_by, Some(agent2));
    }

    #[test]
    fn reassign_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(TaskAggregate::reassign(&mut task, AgentId::new()).is_err());
    }

    #[test]
    fn reassign_fails_from_completed() {
        let mut task = make_task(TaskStatus::Completed, None);
        assert!(TaskAggregate::reassign(&mut task, AgentId::new()).is_err());
    }

    #[test]
    fn reassign_fails_from_failed() {
        let mut task = make_task(TaskStatus::Failed, None);
        assert!(TaskAggregate::reassign(&mut task, AgentId::new()).is_err());
    }
}
