use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use orchy_core::task::waits::Waits;
use orchy_core::{
    DomainError, Edge, EdgeStore, EntityKind, Id, Relation, Task, TaskQuery, TaskStore,
};

use crate::error::ApplicationResult;

/// Reads who waits on whom across the whole board and refuses a change that would close a
/// loop. Every task the decision rests on is loaded again by id, so a concurrent change to any
/// of them makes this change run again instead of landing on a stale reading.
pub struct TaskGraph {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl TaskGraph {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    /// `adding` and `removing` are the waits the change makes and takes away, each as
    /// `(waiter, waited on)`: a task on a dependency, or a parent on a subtask.
    pub async fn ensure_no_loop(
        &self,
        adding: &[(Id, Id)],
        removing: &[(Id, Id)],
    ) -> ApplicationResult<()> {
        if adding.is_empty() {
            return Ok(());
        }
        let board = self.tasks.matching(&TaskQuery::default()).await?;
        let by_id: HashMap<&Id, &Task> = board.iter().map(|t| (t.id(), t)).collect();

        let mut waits = waits_of(
            &board,
            &self.edges.of_relation(&Relation::Supersedes).await?,
        );
        for (waiter, on) in removing {
            waits.remove(waiter, on);
        }
        waits.ensure_no_loop(adding)?;
        for (waiter, on) in adding {
            waits.add(waiter.clone(), on.clone());
        }

        for id in self.reached(&waits, adding) {
            let Some(scanned) = by_id.get(&id) else {
                continue;
            };
            let current = self.tasks.get(&id).await?;
            let unchanged = current.as_ref().is_some_and(|t| {
                t.parent() == scanned.parent()
                    && t.depends_on() == scanned.depends_on()
                    && t.status() == scanned.status()
            });
            if !unchanged {
                return Err(DomainError::contended(format!(
                    "`{id}` changed while its place in the board was being checked"
                ))
                .into());
            }
        }
        Ok(())
    }

    fn reached(&self, waits: &Waits, adding: &[(Id, Id)]) -> BTreeSet<Id> {
        adding
            .iter()
            .flat_map(|(waiter, on)| [waiter, on])
            .flat_map(|id| waits.reachable(id))
            .collect()
    }
}

/// A task waits on each dependency, a parent on each subtask, and a superseded task on what
/// replaced it.
pub(crate) fn waits_of(board: &[Task], supersedes: &[Edge]) -> Waits {
    let mut waits = Waits::new();
    for task in board {
        for dependency in task.depends_on() {
            waits.add(task.id().clone(), dependency.clone());
        }
        if let Some(parent) = task.parent() {
            waits.add(parent.clone(), task.id().clone());
        }
    }
    for edge in supersedes {
        if edge.from().kind() != EntityKind::Task {
            continue;
        }
        if let Some(replaced) = edge.to().id() {
            waits.add(replaced.clone(), edge.source().clone());
        }
    }
    waits
}
