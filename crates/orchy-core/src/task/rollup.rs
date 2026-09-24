use super::status::TaskStatus;

pub const MAX_DEPTH: usize = 64;

pub fn resolve(children: &[TaskStatus]) -> Option<TaskStatus> {
    if children.is_empty() || children.iter().any(|s| !s.is_terminal()) {
        return None;
    }
    if children.contains(&TaskStatus::Failed) {
        return Some(TaskStatus::Failed);
    }
    if children.contains(&TaskStatus::Completed) {
        return Some(TaskStatus::Completed);
    }
    if children.iter().all(|s| *s == TaskStatus::Superseded) {
        return Some(TaskStatus::Superseded);
    }
    Some(TaskStatus::Cancelled)
}

#[cfg(test)]
mod tests {
    use super::TaskStatus::*;
    use super::*;

    #[test]
    fn a_task_with_no_children_never_rolls_up() {
        assert_eq!(resolve(&[]), None, "a leaf is not a parent");
    }

    #[test]
    fn rollup_is_none_while_any_child_is_open() {
        assert_eq!(resolve(&[Completed, Pending]), None);
        assert_eq!(resolve(&[Completed, InProgress]), None);
        assert_eq!(resolve(&[Completed, Claimed]), None);
        assert_eq!(resolve(&[Failed, Blocked]), None);
    }

    #[test]
    fn rollup_completes_when_every_child_completed() {
        assert_eq!(resolve(&[Completed]), Some(Completed));
        assert_eq!(resolve(&[Completed, Completed, Completed]), Some(Completed));
    }

    #[test]
    fn rollup_fails_when_any_child_failed() {
        assert_eq!(resolve(&[Completed, Failed]), Some(Failed));
        assert_eq!(resolve(&[Cancelled, Failed]), Some(Failed));
        assert_eq!(resolve(&[Failed, Completed, Cancelled]), Some(Failed));
    }

    #[test]
    fn rollup_cancels_only_when_every_child_cancelled() {
        assert_eq!(resolve(&[Cancelled]), Some(Cancelled));
        assert_eq!(resolve(&[Cancelled, Cancelled]), Some(Cancelled));
        assert_eq!(
            resolve(&[Cancelled, Completed]),
            Some(Completed),
            "one real completion outweighs abandoned siblings"
        );
    }

    #[test]
    fn rollup_completes_when_children_mix_completed_and_cancelled() {
        assert_eq!(resolve(&[Completed, Cancelled, Completed]), Some(Completed));
    }

    #[test]
    fn failure_outranks_cancellation() {
        assert_eq!(resolve(&[Cancelled, Cancelled, Failed]), Some(Failed));
    }

    #[test]
    fn a_superseded_child_carries_no_verdict_of_its_own() {
        assert_eq!(
            resolve(&[Completed, Superseded]),
            Some(Completed),
            "work that moved elsewhere does not spoil a real completion"
        );
        assert_eq!(resolve(&[Failed, Superseded]), Some(Failed));
        assert_eq!(resolve(&[Cancelled, Superseded]), Some(Cancelled));
    }

    #[test]
    fn a_parent_whose_children_were_all_replaced_is_itself_superseded() {
        assert_eq!(resolve(&[Superseded]), Some(Superseded));
        assert_eq!(resolve(&[Superseded, Superseded]), Some(Superseded));
    }

    #[test]
    fn a_superseded_child_still_blocks_nothing_while_siblings_are_open() {
        assert_eq!(resolve(&[Superseded, Pending]), None);
    }

    #[test]
    fn rollup_does_not_depend_on_the_order_children_finished() {
        let mut children = vec![Completed, Cancelled, Failed, Superseded];
        let expected = resolve(&children);
        children.reverse();
        assert_eq!(resolve(&children), expected);
        children.swap(0, 1);
        assert_eq!(resolve(&children), expected);
    }

    #[test]
    fn rollup_is_idempotent_under_replay() {
        let children = [Completed, Completed];
        let first = resolve(&children);
        assert_eq!(resolve(&children), first);
        assert_eq!(resolve(&children), first);
    }
}
