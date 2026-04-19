use std::sync::Arc;

use orchy_core::edge::EdgeStore;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgeId, KnowledgeKind, KnowledgeStore};
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskFilter, TaskId, TaskStore, TaskWithContext};

use crate::dto::{KnowledgeResponse, TaskResponse, TaskWithContextResponse};

pub struct GetTaskWithContextCommand {
    pub task_id: String,
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
        let id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        self.get_with_context(&id, cmd).await
    }

    async fn get_with_context(
        &self,
        id: &TaskId,
        cmd: GetTaskWithContextCommand,
    ) -> Result<TaskWithContextResponse> {
        let task = self
            .tasks
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let mut ancestors = Vec::new();
        let mut current_parent_id = task.parent_id();
        while let Some(pid) = current_parent_id {
            match self.tasks.find_by_id(&pid).await? {
                Some(parent) => {
                    current_parent_id = parent.parent_id();
                    ancestors.push(parent);
                }
                None => break,
            }
        }

        let children = self
            .tasks
            .list(
                TaskFilter {
                    parent_id: Some(*id),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items;

        let mut response = TaskWithContextResponse::from(TaskWithContext {
            task,
            ancestors,
            children,
        });

        if cmd.include_dependencies {
            response.dependencies = self.load_dependencies(&response.task.depends_on).await?;
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

    async fn load_dependencies(&self, dependency_ids: &[String]) -> Result<Vec<TaskResponse>> {
        let mut dependencies = Vec::new();
        for dep in dependency_ids {
            let dep_id = dep
                .parse::<TaskId>()
                .map_err(|e| Error::InvalidInput(e.to_string()))?;
            let Some(task) = self.tasks.find_by_id(&dep_id).await? else {
                continue;
            };
            dependencies.push(TaskResponse::from(task));
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
    ) -> Result<Vec<KnowledgeResponse>> {
        let org = orchy_core::organization::OrganizationId::new(org_id)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut edges = self
            .edges
            .find_from(&org, &ResourceKind::Task, task_id, None, true)
            .await?;
        edges.extend(
            self.edges
                .find_to(&org, &ResourceKind::Task, task_id, None, true)
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

            let mut response = KnowledgeResponse::from(&entry);
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
