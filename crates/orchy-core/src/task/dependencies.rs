use serde::{Deserialize, Serialize};

use super::status::TaskStatus;

/// Where one dependency, or a task's dependencies as a whole, leaves the dependent task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The work it waited for is done.
    Satisfied,
    /// Still to happen, or unknown (a dependency that no longer exists).
    Pending,
    /// It can never happen: the work failed or was abandoned.
    Doomed,
}

/// One dependency's outcome from its status and, when it was superseded, the outcomes of
/// the tasks that replaced it. A superseded dependency nobody replaced waits until someone
/// re-points the dependency.
pub fn outcome(status: Option<TaskStatus>, replacements: &[Outcome]) -> Outcome {
    match status {
        Some(TaskStatus::Completed) => Outcome::Satisfied,
        Some(TaskStatus::Failed | TaskStatus::Cancelled) => Outcome::Doomed,
        Some(TaskStatus::Superseded) if !replacements.is_empty() => combine(replacements),
        _ => Outcome::Pending,
    }
}

/// A task is ready when every dependency is satisfied, doomed as soon as one is doomed.
pub fn combine(outcomes: &[Outcome]) -> Outcome {
    if outcomes.contains(&Outcome::Doomed) {
        return Outcome::Doomed;
    }
    if outcomes.iter().all(|o| *o == Outcome::Satisfied) {
        return Outcome::Satisfied;
    }
    Outcome::Pending
}

#[cfg(test)]
mod tests {
    use super::Outcome::*;
    use super::*;

    #[test]
    fn only_completion_satisfies_a_dependency() {
        assert_eq!(outcome(Some(TaskStatus::Completed), &[]), Satisfied);
        for open in [
            TaskStatus::Pending,
            TaskStatus::Blocked,
            TaskStatus::Claimed,
            TaskStatus::InProgress,
        ] {
            assert_eq!(outcome(Some(open), &[]), Pending, "{open}");
        }
    }

    #[test]
    fn failed_or_cancelled_work_dooms_whatever_waits_for_it() {
        assert_eq!(outcome(Some(TaskStatus::Failed), &[]), Doomed);
        assert_eq!(outcome(Some(TaskStatus::Cancelled), &[]), Doomed);
    }

    #[test]
    fn a_superseded_dependency_is_satisfied_once_every_replacement_is() {
        let replaced = Some(TaskStatus::Superseded);
        assert_eq!(outcome(replaced, &[Satisfied, Satisfied]), Satisfied);
        assert_eq!(outcome(replaced, &[Satisfied, Pending]), Pending);
        assert_eq!(outcome(replaced, &[Satisfied, Doomed]), Doomed);
    }

    #[test]
    fn a_superseded_dependency_nobody_replaced_waits_to_be_repointed() {
        assert_eq!(outcome(Some(TaskStatus::Superseded), &[]), Pending);
    }

    #[test]
    fn a_missing_dependency_is_never_satisfied() {
        assert_eq!(outcome(None, &[]), Pending);
    }

    #[test]
    fn no_dependencies_means_ready_and_one_doomed_dependency_dooms_the_task() {
        assert_eq!(combine(&[]), Satisfied);
        assert_eq!(combine(&[Satisfied, Pending]), Pending);
        assert_eq!(combine(&[Pending, Doomed]), Doomed);
    }
}
