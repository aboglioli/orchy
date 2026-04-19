use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStatus, TaskStore};

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
        for task in &sources {
            for dep in task.depends_on() {
                if !source_ids.contains(dep) {
                    deps_set.insert(*dep);
                }
            }
        }
        let depends_on: Vec<TaskId> = deps_set.into_iter().collect();

        let parent_id = {
            let first_parent = sources[0].parent_id();
            if sources.iter().all(|t| t.parent_id() == first_parent) {
                first_parent
            } else {
                None
            }
        };

        let namespace = sources[0].namespace().clone();
        let is_blocked = !depends_on.is_empty() && !self.all_deps_completed(&depends_on).await?;

        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut merged = Task::new(
            org_id,
            sources[0].project().clone(),
            namespace,
            parent_id,
            cmd.title,
            cmd.description,
            cmd.acceptance_criteria,
            priority,
            assigned_roles,
            depends_on,
            created_by.clone(),
            is_blocked,
        )?;

        self.tasks.save(&mut merged).await?;

        let mut cancelled = Vec::with_capacity(sources.len());
        for mut task in sources {
            task.cancel(Some(format!("merged into {}", merged.id())))?;
            self.tasks.save(&mut task).await?;
            cancelled.push(task);
        }

        for source in &cancelled {
            let mut edge = match Edge::new(
                merged.org_id().clone(),
                ResourceKind::Task,
                merged.id().to_string(),
                ResourceKind::Task,
                source.id().to_string(),
                RelationType::MergedFrom,
                None,
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
            let children = self
                .tasks
                .list(
                    TaskFilter {
                        parent_id: Some(*source_id),
                        ..Default::default()
                    },
                    PageParams::unbounded(),
                )
                .await?
                .items;

            for mut child in children {
                child.set_parent_id(Some(merged.id()))?;
                self.tasks.save(&mut child).await?;
            }
        }

        for status in [
            TaskStatus::Pending,
            TaskStatus::Blocked,
            TaskStatus::Claimed,
        ] {
            let tasks = self
                .tasks
                .list(
                    TaskFilter {
                        project: Some(merged.project().clone()),
                        status: Some(status),
                        ..Default::default()
                    },
                    PageParams::unbounded(),
                )
                .await?
                .items;

            for mut task in tasks {
                if source_ids.contains(&task.id()) || task.id() == merged.id() {
                    continue;
                }

                let mut changed = false;
                for source_id in &source_ids {
                    if task.depends_on().contains(source_id) {
                        task.replace_dependency(source_id, merged.id())?;
                        changed = true;
                    }
                }

                if changed {
                    self.tasks.save(&mut task).await?;
                }
            }
        }

        Ok((
            TaskResponse::from(&merged),
            cancelled.iter().map(TaskResponse::from).collect(),
        ))
    }

    async fn all_deps_completed(&self, deps: &[TaskId]) -> Result<bool> {
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
