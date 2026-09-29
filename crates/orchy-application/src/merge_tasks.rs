use std::sync::Arc;

use orchy_core::{
    ActorId, Clock, DomainError, Edge, EdgeStore, EntityRef, Id, Relation, TaskStore,
};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;

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
    edges: Arc<dyn EdgeStore>,
    rollup: Arc<RollupAncestors>,
    clock: Arc<dyn Clock>,
}

impl MergeTasks {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
        rollup: Arc<RollupAncestors>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            tasks,
            edges,
            rollup,
            clock,
        }
    }

    pub async fn execute(&self, cmd: MergeTasksCommand) -> ApplicationResult<MergeTasksResponse> {
        cmd.actor.parse::<ActorId>()?;
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
        let mut merged = Vec::new();
        let mut moved = Vec::new();
        for other_id in &other_ids {
            let mut other = self.tasks.require(other_id).await?;
            other.supersede(
                vec![keep_id.clone()],
                Some(format!("merged into {keep_id}")),
                &*self.clock,
            )?;
            self.tasks.save(&mut other).await?;

            keep.retag(other.tags().to_vec(), &[], &*self.clock);
            for dependency in other.depends_on() {
                if dependency != &keep_id && !other_ids.contains(dependency) {
                    keep.add_dependency(dependency.clone(), &*self.clock)?;
                }
            }
            if keep.parent() == Some(other_id) {
                match other.parent() {
                    Some(grandparent) => keep.attach_to(grandparent.clone(), &*self.clock)?,
                    None => keep.detach(&*self.clock),
                }
            }

            for mut child in self.tasks.children_of(other_id).await? {
                if child.id() == &keep_id {
                    continue;
                }
                child.attach_to(keep_id.clone(), &*self.clock)?;
                self.tasks.save(&mut child).await?;
                moved.push(TaskDto::from(&child));
            }

            merged.push(other);
        }
        // links live in the kept task's file, so it is saved before they are added
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
