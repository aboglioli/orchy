use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::graph::{Edge, EdgeStore, RelationType};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskResponse;

pub struct MergeTasksCommand {
    pub org_id: String,
    pub task_ids: Vec<String>,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub created_by: Option<String>,
}

pub struct MergeTasks {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl MergeTasks {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(
        &self,
        cmd: MergeTasksCommand,
    ) -> Result<(TaskResponse, Vec<TaskResponse>)> {
        let task_ids = cmd
            .task_ids
            .iter()
            .map(|s| s.parse::<TaskId>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        if task_ids.len() < 2 {
            return Err(Error::InvalidInput(
                "merge requires at least 2 tasks".into(),
            ));
        }

        let created_by = cmd.created_by.map(|s| AgentId::from_str(&s)).transpose()?;

        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut sources = Vec::with_capacity(task_ids.len());
        for id in &task_ids {
            let task = self
                .tasks
                .find_by_id(id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("task {id}")))?;
            sources.push(task);
        }

        let first_project = sources[0].project().clone();
        for task in &sources {
            if *task.project() != first_project {
                return Err(Error::InvalidInput(format!(
                    "task {} belongs to project {}, expected {}",
                    task.id(),
                    task.project(),
                    first_project
                )));
            }
            if !task.status().is_mergeable() {
                return Err(Error::InvalidInput(format!(
                    "task {} has status {} which cannot be merged",
                    task.id(),
                    task.status()
                )));
            }
        }

        let source_ids: HashSet<TaskId> = task_ids.iter().copied().collect();

        let priority = sources
            .iter()
            .map(|t| t.priority())
            .max()
            .unwrap_or_default();

        let mut roles_set = HashSet::new();
        for task in &sources {
            for role in task.assigned_roles() {
                roles_set.insert(role.clone());
            }
        }
        let assigned_roles: Vec<String> = roles_set.into_iter().collect();

        let mut deps_set = HashSet::new();
        for source_id in &source_ids {
            let dep_edges = self
                .edges
                .find_from(
                    &org_id,
                    &ResourceKind::Task,
                    &source_id.to_string(),
                    &[RelationType::DependsOn],
                    None,
                )
                .await?;
            for edge in dep_edges {
                if let Ok(dep_id) = edge.to_id().parse::<TaskId>()
                    && !source_ids.contains(&dep_id)
                {
                    deps_set.insert(dep_id);
                }
            }
        }

        let namespace = sources[0].namespace().clone();
        let is_blocked = !deps_set.is_empty() && !self.all_deps_completed(&deps_set).await?;

        let mut merged = Task::new(
            org_id.clone(),
            sources[0].project().clone(),
            namespace,
            cmd.title,
            cmd.description,
            cmd.acceptance_criteria,
            priority,
            assigned_roles,
            created_by.clone(),
            is_blocked,
        )?;

        self.tasks.save(&mut merged).await?;

        for dep_id in &deps_set {
            let mut dep_edge = Edge::new(
                org_id.clone(),
                ResourceKind::Task,
                merged.id().to_string(),
                ResourceKind::Task,
                dep_id.to_string(),
                RelationType::DependsOn,
                created_by.clone(),
            )?;
            self.edges.save(&mut dep_edge).await?;
        }

        let mut cancelled = Vec::with_capacity(sources.len());
        for mut task in sources {
            task.cancel(Some(format!("merged into {}", merged.id())))?;
            self.tasks.save(&mut task).await?;
            cancelled.push(task);
        }

        for source in &cancelled {
            let mut edge = match Edge::new(
                org_id.clone(),
                ResourceKind::Task,
                merged.id().to_string(),
                ResourceKind::Task,
                source.id().to_string(),
                RelationType::MergedFrom,
                created_by.clone(),
            ) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("failed to create edge: {e}");
                    continue;
                }
            };
            if let Err(e) = self.edges.save(&mut edge).await {
                tracing::warn!("failed to create merged_from edge for {}: {e}", source.id());
            }
        }

        for source_id in &source_ids {
            let child_edges = self
                .edges
                .find_from(
                    &org_id,
                    &ResourceKind::Task,
                    &source_id.to_string(),
                    &[RelationType::Spawns],
                    None,
                )
                .await?;
            for child_edge in child_edges {
                let child_id = child_edge.to_id().to_string();
                let mut new_edge = Edge::new(
                    org_id.clone(),
                    ResourceKind::Task,
                    merged.id().to_string(),
                    ResourceKind::Task,
                    child_id,
                    RelationType::Spawns,
                    created_by.clone(),
                )?;
                self.edges.save(&mut new_edge).await?;
            }
        }

        Ok((
            TaskResponse::from(&merged),
            cancelled.iter().map(TaskResponse::from).collect(),
        ))
    }

    async fn all_deps_completed(&self, deps: &HashSet<TaskId>) -> Result<bool> {
        for dep_id in deps {
            let dep = self
                .tasks
                .find_by_id(dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            if dep.status() != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
