use std::sync::Arc;

use orchy_core::{
    ActorId, Clock, Edge, EdgeStore, EntityRef, Id, IdGenerator, Relation, Task, TaskStore, Title,
};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;

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
}

impl ReplaceTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
        rollup: Arc<RollupAncestors>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            tasks,
            edges,
            rollup,
            ids,
            clock,
        }
    }

    pub async fn execute(&self, cmd: ReplaceTaskCommand) -> ApplicationResult<ReplaceTaskResponse> {
        let original_id = Id::new(&cmd.task_id)?;
        cmd.actor.parse::<ActorId>()?;
        let mut original = self.tasks.require(&original_id).await?;

        let mut created = Vec::new();
        let mut replacements = Vec::new();

        for raw in &cmd.titles {
            let mut replacement = Task::create(
                Title::new(raw)?,
                original.namespace().clone(),
                &*self.ids,
                &*self.clock,
            );
            // the work still belongs under whatever goal the original sat beneath
            if let Some(parent) = original.parent() {
                replacement.attach_to(parent.clone(), &*self.clock)?;
            }
            replacement.set_priority(original.priority(), &*self.clock);
            self.tasks.save(&mut replacement).await?;

            self.edges
                .add(&Edge::new(
                    EntityRef::task(replacement.id().clone()),
                    EntityRef::task(original_id.clone()),
                    Relation::Supersedes,
                )?)
                .await?;

            replacements.push(replacement.id().clone());
            created.push(TaskDto::from(&replacement));
        }

        original.supersede(replacements, cmd.reason, &*self.clock)?;
        self.tasks.save(&mut original).await?;

        let ancestors = self.rollup.execute(&original_id).await?;

        Ok(ReplaceTaskResponse {
            replaced: TaskDto::from(&original),
            created,
            ancestors,
        })
    }
}
