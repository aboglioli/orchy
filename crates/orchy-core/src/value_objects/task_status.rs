use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Blocked,
    Claimed,
    InProgress,
    Completed,
    Failed,
}

impl TaskStatus {
    pub fn can_transition_to(&self, target: &TaskStatus) -> bool {
        use TaskStatus::*;
        matches!(
            (self, target),
            (Pending, Claimed)
                | (Pending, Blocked)
                | (Blocked, Pending)
                | (Claimed, InProgress)
                | (Claimed, Completed)
                | (Claimed, Failed)
                | (Claimed, Pending)
                | (InProgress, Completed)
                | (InProgress, Failed)
                | (InProgress, Pending)
        )
    }

    pub fn transition_to(&self, target: TaskStatus) -> Result<TaskStatus> {
        if self.can_transition_to(&target) {
            Ok(target)
        } else {
            Err(Error::InvalidTransition {
                from: self.to_string(),
                to: target.to_string(),
            })
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Claimed => "claimed",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_transitions() {
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Claimed));
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Blocked));
        assert!(TaskStatus::Blocked.can_transition_to(&TaskStatus::Pending));
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::InProgress));
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::Completed));
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::Failed));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Completed));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Failed));
    }

    #[test]
    fn invalid_transitions() {
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::InProgress));
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Completed));
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Failed));
        assert!(!TaskStatus::Completed.can_transition_to(&TaskStatus::Pending));
        assert!(!TaskStatus::Failed.can_transition_to(&TaskStatus::Pending));
        assert!(!TaskStatus::Blocked.can_transition_to(&TaskStatus::Claimed));
    }

    #[test]
    fn release_transitions() {
        // Claimed and InProgress can both release back to Pending
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::Pending));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Pending));

        let result = TaskStatus::Claimed.transition_to(TaskStatus::Pending);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TaskStatus::Pending);

        let result = TaskStatus::InProgress.transition_to(TaskStatus::Pending);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TaskStatus::Pending);
    }
}
