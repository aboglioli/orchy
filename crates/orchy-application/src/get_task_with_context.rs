use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::graph::{EdgeStore, RelationType};
use orchy_core::knowledge::{KnowledgeId, KnowledgeKind, KnowledgeStore};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskId, TaskStore, TaskWithContext};

use crate::dto::{KnowledgeDto, TaskDto, TaskWithContextResponse};

pub struct GetTaskWithContextCommand {
    pub task_id: String,
    pub org_id: String,
    pub include_dependencies: bool,
    pub include_knowledge: bool,
    pub knowledge_limit: u32,
    pub knowledge_kind: Option<String>,
    pub knowledge_tag: Option<String>,
    pub knowledge_content_limit: usize,
}

pub struct GetTaskWithContext {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
    knowledge: Arc<dyn KnowledgeStore>,
}

impl GetTaskWithContext {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
        knowledge: Arc<dyn KnowledgeStore>,
    ) -> Self {
        Self {
            tasks,
            edges,
            knowledge,
        }
    }

    pub async fn execute(&self, cmd: GetTaskWithContextCommand) -> Result<TaskWithContextResponse> {
        let id = cmd.task_id.parse::<TaskId>()?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        self.get_with_context(&id, &org_id, cmd).await
    }

    async fn get_with_context(
        &self,
        id: &TaskId,
        org_id: &OrganizationId,
        cmd: GetTaskWithContextCommand,
    ) -> Result<TaskWithContextResponse> {
        let task = self
            .tasks
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let mut ancestors = Vec::new();
        let mut current_id = id.to_string();
        loop {
            let parent_edges = self
                .edges
                .find_to(
                    org_id,
                    &ResourceKind::Task,
                    &current_id,
                    &[RelationType::Spawns],
                    None,
                )
                .await?;
            let Some(parent_edge) = parent_edges.first() else {
                break;
            };
            let parent_id_str = parent_edge.from_id().to_string();
            let parent_task_id: TaskId = match parent_id_str.parse() {
                Ok(tid) => tid,
                Err(_) => break,
            };
            match self.tasks.find_by_id(&parent_task_id).await? {
                Some(parent) => {
                    current_id = parent_task_id.to_string();
                    ancestors.push(parent);
                }
                None => break,
            }
        }

        let child_edges = self
            .edges
            .find_from(
                org_id,
                &ResourceKind::Task,
                &id.to_string(),
                &[RelationType::Spawns],
                None,
            )
            .await?;
        let child_ids: Vec<TaskId> = child_edges
            .iter()
            .filter_map(|e| e.to_id().parse::<TaskId>().ok())
            .collect();
        let children = self.tasks.find_by_ids(&child_ids).await?;

        let mut response = TaskWithContextResponse::from(TaskWithContext {
            task,
            ancestors,
            children,
        });

        if cmd.include_dependencies {
            let dep_edges = self
                .edges
                .find_from(
                    org_id,
                    &ResourceKind::Task,
                    &id.to_string(),
                    &[RelationType::DependsOn],
                    None,
                )
                .await?;
            let dep_ids: Vec<String> = dep_edges.iter().map(|e| e.to_id().to_string()).collect();
            response.dependencies = self.load_dependencies(&dep_ids).await?;
        }

        if cmd.include_knowledge {
            response.knowledge = self
                .load_linked_knowledge(
                    &response.task.org_id,
                    &response.task.id,
                    cmd.knowledge_limit,
                    cmd.knowledge_kind.as_deref(),
                    cmd.knowledge_tag.as_deref(),
                    cmd.knowledge_content_limit,
                )
                .await?;
        }

        Ok(response)
    }

    async fn load_dependencies(&self, dependency_ids: &[String]) -> Result<Vec<TaskDto>> {
        let mut dependencies = Vec::new();
        for dep in dependency_ids {
            let dep_id = dep.parse::<TaskId>()?;
            let Some(task) = self.tasks.find_by_id(&dep_id).await? else {
                continue;
            };
            dependencies.push(TaskDto::from(task));
        }

        Ok(dependencies)
    }

    async fn load_linked_knowledge(
        &self,
        org_id: &str,
        task_id: &str,
        limit: u32,
        kind_filter: Option<&str>,
        tag_filter: Option<&str>,
        content_limit: usize,
    ) -> Result<Vec<KnowledgeDto>> {
        let org = OrganizationId::new(org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut edges = self
            .edges
            .find_from(&org, &ResourceKind::Task, task_id, &[], None)
            .await?;
        edges.extend(
            self.edges
                .find_to(&org, &ResourceKind::Task, task_id, &[], None)
                .await?,
        );

        let expected_kind = kind_filter
            .map(|k| k.parse::<KnowledgeKind>().map_err(Error::InvalidInput))
            .transpose()?;

        let mut results = Vec::new();
        for edge in edges {
            let knowledge_id_str = if edge.from_kind() == &ResourceKind::Knowledge {
                edge.from_id()
            } else if edge.to_kind() == &ResourceKind::Knowledge {
                edge.to_id()
            } else {
                continue;
            };

            let knowledge_id = match knowledge_id_str.parse::<KnowledgeId>() {
                Ok(id) => id,
                Err(_) => continue,
            };

            let Some(entry) = self.knowledge.find_by_id(&knowledge_id).await? else {
                continue;
            };

            if let Some(ref expected) = expected_kind
                && entry.kind() != *expected
            {
                continue;
            }

            if let Some(tag) = tag_filter
                && !entry.tags().iter().any(|t| t == tag)
            {
                continue;
            }

            let mut response = KnowledgeDto::from(&entry);
            if content_limit == 0 {
                response.content.clear();
            } else if response.content.len() > content_limit {
                let truncated: String = response.content.chars().take(content_limit).collect();
                response.content = truncated;
            }

            results.push(response);
            if results.len() >= limit as usize {
                break;
            }
        }

        Ok(results)
    }
}
