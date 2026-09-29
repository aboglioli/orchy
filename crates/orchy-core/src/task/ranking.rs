use super::Task;

/// The one order claimable work is handed out in: most urgent first, then oldest, then by id
/// so that ties never depend on the order tasks were read in.
pub fn rank(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        b.priority()
            .cmp(&a.priority())
            .then_with(|| a.created_at().cmp(&b.created_at()))
            .then_with(|| a.id().cmp(b.id()))
    });
}

/// Pending tasks that `ready` accepts, in `rank` order.
pub fn claimable(tasks: Vec<Task>, ready: impl Fn(&Task) -> bool) -> Vec<Task> {
    let mut claimable: Vec<Task> = tasks
        .into_iter()
        .filter(|t| t.status().is_claimable() && ready(t))
        .collect();
    rank(&mut claimable);
    claimable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;
    use crate::id::IdGenerator;
    use crate::namespace::Namespace;
    use crate::priority::Priority;
    use crate::task::tests::ids;
    use crate::title::Title;
    use chrono::{DateTime, Utc};

    struct At(i64);

    impl Clock for At {
        fn now(&self) -> DateTime<Utc> {
            DateTime::from_timestamp(self.0, 0).unwrap()
        }
    }

    fn task(ids: &dyn IdGenerator, title: &str, priority: Priority, at: i64) -> Task {
        let mut task = Task::create(Title::new(title).unwrap(), Namespace::root(), ids, &At(at));
        task.set_priority(priority, &At(at));
        task
    }

    fn titles(tasks: &[Task]) -> Vec<String> {
        tasks.iter().map(|t| t.title().to_string()).collect()
    }

    #[test]
    fn a_newer_urgent_task_outranks_older_normal_ones() {
        let ids = ids();
        let mut tasks: Vec<Task> = (0..20)
            .map(|n| task(&ids, &format!("filler {n}"), Priority::Normal, 100 + n))
            .collect();
        tasks.push(task(&ids, "urgent", Priority::High, 500));
        assert_eq!(titles(&claimable(tasks, |_| true))[0], "urgent");
    }

    #[test]
    fn within_a_priority_the_oldest_comes_first() {
        let ids = ids();
        let tasks = vec![
            task(&ids, "newer", Priority::Normal, 200),
            task(&ids, "older", Priority::Normal, 100),
        ];
        assert_eq!(titles(&claimable(tasks, |_| true)), vec!["older", "newer"]);
    }

    #[test]
    fn a_task_that_is_not_ready_is_never_claimable() {
        let ids = ids();
        let tasks = vec![
            task(&ids, "waiting", Priority::High, 100),
            task(&ids, "free", Priority::Low, 100),
        ];
        let ranked = claimable(tasks, |t| t.title().as_str() != "waiting");
        assert_eq!(titles(&ranked), vec!["free"]);
    }
}
