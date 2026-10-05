use std::sync::Arc;

use orchy_core::{
    ActorId, Clock, DomainError, Edge, EdgeStore, EntityRef, Id, Relation, TaskStore, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;
use crate::task_graph::TaskGraph;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MergeTasksCommand {
    pub keep: String,
    pub others: Vec<String>,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeTasksResponse {
    pub kept: TaskDto,
    pub merged: Vec<TaskDto>,
    pub moved: Vec<TaskDto>,
}

/// Duplicates collapse into `keep`: each other task is superseded by it, and its subtasks,
/// tags and dependencies move over, so nothing filed under a duplicate is lost.
pub struct MergeTasks {
    tasks: Arc<dyn TaskStore>,
    graph: Arc<TaskGraph>,
    edges: Arc<dyn EdgeStore>,
    rollup: Arc<RollupAncestors>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl MergeTasks {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        graph: Arc<TaskGraph>,
        edges: Arc<dyn EdgeStore>,
        rollup: Arc<RollupAncestors>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            graph,
            edges,
            rollup,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: MergeTasksCommand) -> ApplicationResult<MergeTasksResponse> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: MergeTasksCommand) -> ApplicationResult<MergeTasksResponse> {
        let actor: ActorId = cmd.actor.parse()?;
        let keep_id = Id::new(&cmd.keep)?;
        let mut other_ids = Vec::new();
        for raw in &cmd.others {
            let id = Id::new(raw)?;
            if id == keep_id {
                return Err(DomainError::validation("a task cannot be merged into itself").into());
            }
            if !other_ids.contains(&id) {
                other_ids.push(id);
            }
        }
        if other_ids.is_empty() {
            return Err(DomainError::validation("name at least one task to merge").into());
        }

        let mut keep = self.tasks.require(&keep_id).await?;
        keep.ensure_open()?;
        let mut others = Vec::new();
        for id in &other_ids {
            others.push(self.tasks.require(id).await?);
        }

        let mut adding: Vec<(Id, Id)> = Vec::new();
        let mut removing: Vec<(Id, Id)> = Vec::new();

        // waiting on a duplicate is waiting on itself once they are one task
        for other in &others {
            if keep.depends_on().contains(other.id()) {
                keep.remove_dependency(other.id(), &*self.clock);
                removing.push((keep_id.clone(), other.id().clone()));
            }
        }

        // beneath a duplicate, the kept task takes its place under the first ancestor that stays
        let mut above = keep.parent().cloned();
        while let Some(parent) = above.clone().filter(|p| other_ids.contains(p)) {
            above = others
                .iter()
                .find(|o| o.id() == &parent)
                .and_then(|o| o.parent().cloned());
        }
        if above.as_ref() != keep.parent() {
            if let Some(previous) = keep.parent() {
                removing.push((previous.clone(), keep_id.clone()));
            }
            match &above {
                Some(grandparent) => {
                    let grandparent = self.tasks.require(grandparent).await?;
                    adding.push((grandparent.id().clone(), keep_id.clone()));
                    keep.attach_to(&grandparent, &*self.clock)?;
                }
                None => keep.detach(&*self.clock),
            }
        }

        let mut children = Vec::new();
        for other in &others {
            keep.retag(other.tags().to_vec(), &[], &*self.clock);
            for dependency in other.depends_on() {
                if dependency != &keep_id && !other_ids.contains(dependency) {
                    keep.add_dependency(dependency.clone(), &*self.clock)?;
                    adding.push((keep_id.clone(), dependency.clone()));
                }
            }
            for child in self.tasks.children_of(other.id()).await? {
                if child.id() == &keep_id || other_ids.contains(child.id()) {
                    continue;
                }
                removing.push((other.id().clone(), child.id().clone()));
                adding.push((keep_id.clone(), child.id().clone()));
                children.push(child);
            }
            adding.push((other.id().clone(), keep_id.clone()));
        }
        self.graph.ensure_no_loop(&adding, &removing).await?;

        let mut moved = Vec::new();
        for mut child in children {
            child.attach_to(&keep, &*self.clock)?;
            self.tasks.save(&mut child).await?;
            moved.push(TaskDto::from(&child));
        }
        let mut merged = Vec::new();
        for mut other in others {
            other.supersede(
                &actor,
                vec![keep_id.clone()],
                Some(format!("merged into {keep_id}")),
                &*self.clock,
            )?;
            self.tasks.save(&mut other).await?;
            merged.push(other);
        }
        self.tasks.save(&mut keep).await?;
        for other_id in &other_ids {
            let (from, to) = (
                EntityRef::task(keep_id.clone()),
                EntityRef::task(other_id.clone()),
            );
            self.edges
                .add(&Edge::new(from.clone(), to.clone(), Relation::MergedFrom)?)
                .await?;
            self.edges
                .add(&Edge::new(from, to, Relation::Supersedes)?)
                .await?;
        }

        for other in &merged {
            self.rollup.execute(other.id()).await?;
        }
        self.rollup.from_parent(&keep_id).await?;
        let kept = self.tasks.require(&keep_id).await?;

        Ok(MergeTasksResponse {
            kept: TaskDto::from(&kept),
            merged: merged.iter().map(TaskDto::from).collect(),
            moved,
        })
    }
}
