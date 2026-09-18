use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Blocked,
    Claimed,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub const ALL: [Self; 7] = [
        Self::Pending,
        Self::Blocked,
        Self::Claimed,
        Self::InProgress,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn is_claimable(&self) -> bool {
        matches!(self, Self::Pending)
    }

    pub fn can_transition_to(&self, target: Self) -> bool {
        use TaskStatus::*;
        matches!(
            (self, target),
            (Pending, Blocked)
                | (Pending, Claimed)
                | (Pending, Cancelled)
                | (Blocked, Pending)
                | (Blocked, Cancelled)
                | (Claimed, Pending)
                | (Claimed, Blocked)
                | (Claimed, InProgress)
                | (Claimed, Completed)
                | (Claimed, Failed)
                | (Claimed, Cancelled)
                | (InProgress, Pending)
                | (InProgress, Blocked)
                | (InProgress, Completed)
                | (InProgress, Failed)
                | (InProgress, Cancelled)
        )
    }

    pub fn transition_to(&self, target: Self) -> Result<Self> {
        if self.can_transition_to(target) {
            return Ok(target);
        }
        Err(DomainError::invalid_transition(self, target))
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Blocked => "blocked",
            Self::Claimed => "claimed",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == s)
            .ok_or_else(|| DomainError::validation(format!("unknown task status: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::TaskStatus::*;
    use super::*;

    #[test]
    fn all_three_terminal_statuses_are_absorbing() {
        for terminal in [Completed, Failed, Cancelled] {
            assert!(terminal.is_terminal());
            for target in TaskStatus::ALL {
                assert!(
                    !terminal.can_transition_to(target),
                    "{terminal} must not reopen into {target}"
                );
            }
        }
    }

    #[test]
    fn a_blocked_task_must_be_unblocked_before_it_can_be_claimed() {
        assert!(!Blocked.can_transition_to(Claimed));
        assert!(Blocked.can_transition_to(Pending));
        assert!(Pending.can_transition_to(Claimed));
    }

    #[test]
    fn claiming_is_not_a_self_transition() {
        assert!(!Claimed.can_transition_to(Claimed));
    }

    #[test]
    fn release_returns_a_claimed_task_to_the_pool() {
        assert!(Claimed.can_transition_to(Pending));
        assert!(InProgress.can_transition_to(Pending));
    }

    #[test]
    fn work_can_finish_from_claimed_or_in_progress() {
        for from in [Claimed, InProgress] {
            for target in [Completed, Failed, Cancelled] {
                assert!(from.can_transition_to(target), "{from} -> {target}");
            }
        }
    }

    #[test]
    fn pending_cannot_jump_straight_to_completed() {
        assert!(!Pending.can_transition_to(Completed));
        assert!(!Pending.can_transition_to(InProgress));
    }

    #[test]
    fn transition_to_reports_both_ends_of_a_refused_move() {
        let err = Pending.transition_to(Completed).unwrap_err();
        assert_eq!(
            err,
            DomainError::InvalidTransition {
                from: "pending".to_owned(),
                to: "completed".to_owned(),
            }
        );
    }

    #[test]
    fn only_pending_tasks_are_claimable() {
        for status in TaskStatus::ALL {
            assert_eq!(status.is_claimable(), status == Pending, "{status}");
        }
    }

    #[test]
    fn round_trips_through_its_wire_name() {
        for status in TaskStatus::ALL {
            assert_eq!(status.as_str().parse::<TaskStatus>().unwrap(), status);
        }
        assert_eq!(InProgress.as_str(), "in_progress");
        assert!("in-progress".parse::<TaskStatus>().is_err());
    }
}
