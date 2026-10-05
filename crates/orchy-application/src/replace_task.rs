use std::sync::Arc;

use orchy_core::task::rollup;
use orchy_core::{
    ActorId, Clock, Edge, EdgeStore, EntityRef, Id, IdGenerator, Relation, Task, TaskStatus,
    TaskStore, Title, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplaceTaskCommand {
    pub task_id: String,
    pub titles: Vec<String>,
    pub reason: Option<String>,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceTaskResponse {
    pub replaced: TaskDto,
    pub created: Vec<TaskDto>,
    pub ancestors: Vec<TaskDto>,
}

/// Succession, not composition: the original is retired. Contrast `SplitTask`, which keeps it
/// as the umbrella its subtasks roll up into.
pub struct ReplaceTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
    rollup: Arc<RollupAncestors>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl ReplaceTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
        rollup: Arc<RollupAncestors>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            edges,
            rollup,
            ids,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: ReplaceTaskCommand) -> ApplicationResult<ReplaceTaskResponse> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: ReplaceTaskCommand) -> ApplicationResult<ReplaceTaskResponse> {
        let original_id = Id::new(&cmd.task_id)?;
        let actor: ActorId = cmd.actor.parse()?;
        let mut original = self.tasks.require(&original_id).await?;
        let children: Vec<TaskStatus> = self
            .tasks
            .children_of(&original_id)
            .await?
            .iter()
            .map(Task::status)
            .collect();
        rollup::ensure_can_finish(&children)?;
        let parent = match original.parent() {
            Some(parent) => Some(self.tasks.require(parent).await?),
            None => None,
        };

        let mut replacements = Vec::new();
        for raw in &cmd.titles {
            let mut replacement = Task::create(
                Title::new(raw)?,
                original.namespace().clone(),
                &*self.ids,
                &*self.clock,
            );
            // the work still belongs under whatever goal the original sat beneath
            if let Some(parent) = &parent {
                replacement.attach_to(parent, &*self.clock)?;
            }
            replacement.set_priority(original.priority(), &*self.clock);
            replacements.push(replacement);
        }

        original.supersede(
            &actor,
            replacements.iter().map(|r| r.id().clone()).collect(),
            cmd.reason,
            &*self.clock,
        )?;
        self.tasks.save(&mut original).await?;

        let mut created = Vec::new();
        for mut replacement in replacements {
            self.tasks.save(&mut replacement).await?;
            self.edges
                .add(&Edge::new(
                    EntityRef::task(replacement.id().clone()),
                    EntityRef::task(original_id.clone()),
                    Relation::Supersedes,
                )?)
                .await?;
            created.push(TaskDto::from(&replacement));
        }

        let ancestors = self.rollup.execute(&original_id).await?;

        Ok(ReplaceTaskResponse {
            replaced: TaskDto::from(&original),
            created,
            ancestors,
        })
    }
}
