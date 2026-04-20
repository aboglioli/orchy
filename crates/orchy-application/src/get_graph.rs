use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use orchy_core::agent::AgentStore;
use orchy_core::edge::{EdgeStore, RelationType, TraversalDirection};
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::TaskStore;

use crate::dto::{GraphResponse, NodeSummary, TraversalEdgeResponse};

pub struct GetGraphCommand {
    pub org_id: String,
    pub kind: String,
    pub id: String,
    pub max_depth: Option<u32>,
    pub rel_types: Option<Vec<String>>,
    pub direction: Option<String>,
    pub include_nodes: bool,
    pub node_content_limit: Option<usize>,
    pub only_active: bool,
    pub max_results: Option<usize>,
    pub as_of: Option<DateTime<Utc>>,
}

pub struct GetGraph {
    store: Arc<dyn EdgeStore>,
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
    agents: Arc<dyn AgentStore>,
}

impl GetGraph {
    pub fn new(
        store: Arc<dyn EdgeStore>,
        tasks: Arc<dyn TaskStore>,
        knowledge: Arc<dyn KnowledgeStore>,
        agents: Arc<dyn AgentStore>,
    ) -> Self {
        Self {
            store,
            tasks,
            knowledge,
            agents,
        }
    }

    pub async fn execute(&self, cmd: GetGraphCommand) -> Result<GraphResponse> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let kind = cmd
            .kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let max_depth = cmd.max_depth.unwrap_or(3).min(10);

        let rel_types: Option<Vec<RelationType>> = cmd
            .rel_types
            .map(|v| {
                v.into_iter()
                    .map(|s| s.parse().map_err(Error::InvalidInput))
                    .collect()
            })
            .transpose()?;

        let direction = match cmd.direction.as_deref() {
            Some("incoming") => TraversalDirection::Incoming,
            Some("both") => TraversalDirection::Both,
            _ => TraversalDirection::Outgoing,
        };

        let limit = cmd.max_results.unwrap_or(500) as u32;
        let rel_type_slice: Vec<RelationType> = rel_types.unwrap_or_default();
        let mut traversal = self
            .store
            .find_neighbors(
                &org,
                &kind,
                &cmd.id,
                &rel_type_slice,
                &[],
                direction,
                max_depth,
                cmd.as_of,
                limit,
            )
            .await?;

        if let Some(max) = cmd.max_results {
            traversal.truncate(max);
        }

        let mut node_set: HashSet<String> = HashSet::new();
        node_set.insert(format!("{}:{}", kind, cmd.id));
        for h in &traversal {
            node_set.insert(format!("{}:{}", h.edge.from_kind(), h.edge.from_id()));
            node_set.insert(format!("{}:{}", h.edge.to_kind(), h.edge.to_id()));
        }
        let mut node_ids: Vec<String> = node_set.into_iter().collect();
        node_ids.sort();

        let edges: Vec<TraversalEdgeResponse> =
            traversal.iter().map(TraversalEdgeResponse::from).collect();

        let nodes = if cmd.include_nodes {
            let limit = cmd.node_content_limit.unwrap_or(500);
            Some(self.fetch_nodes(&node_ids, limit).await)
        } else {
            None
        };

        Ok(GraphResponse {
            root_kind: kind.to_string(),
            root_id: cmd.id,
            edges,
            node_ids,
            nodes,
        })
    }

    async fn fetch_nodes(
        &self,
        node_ids: &[String],
        content_limit: usize,
    ) -> HashMap<String, NodeSummary> {
        let mut result = HashMap::new();

        for node_key in node_ids {
            let Some((kind_str, id)) = node_key.split_once(':') else {
                continue;
            };
            let Ok(kind) = kind_str.parse::<ResourceKind>() else {
                continue;
            };

            let summary = match kind {
                ResourceKind::Task => {
                    if let Ok(task_id) = id.parse() {
                        if let Ok(Some(task)) = self.tasks.find_by_id(&task_id).await {
                            let content = if content_limit > 0 {
                                let desc = task.description();
                                if desc.len() > content_limit {
                                    let truncated: String =
                                        desc.chars().take(content_limit).collect();
                                    Some(format!("{truncated}…"))
                                } else {
                                    Some(desc.to_string())
                                }
                            } else {
                                None
                            };
                            Some(NodeSummary {
                                kind: "task".to_string(),
                                id: id.to_string(),
                                label: task.title().to_string(),
                                content,
                                tags: task.tags().to_vec(),
                                status: Some(task.status().to_string()),
                                priority: Some(task.priority().to_string()),
                                updated_at: Some(task.updated_at().to_rfc3339()),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                ResourceKind::Knowledge => {
                    if let Ok(know_id) = id.parse() {
                        if let Ok(Some(entry)) = self.knowledge.find_by_id(&know_id).await {
                            let content = if content_limit > 0 {
                                let c = entry.content();
                                if c.len() > content_limit {
                                    let truncated: String = c.chars().take(content_limit).collect();
                                    Some(format!("{truncated}…"))
                                } else {
                                    Some(c.to_string())
                                }
                            } else {
                                None
                            };
                            Some(NodeSummary {
                                kind: "knowledge".to_string(),
                                id: id.to_string(),
                                label: entry.title().to_string(),
                                content,
                                tags: entry.tags().to_vec(),
                                status: None,
                                priority: None,
                                updated_at: Some(entry.updated_at().to_rfc3339()),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                ResourceKind::Agent => {
                    if let Ok(agent_id) = id.parse() {
                        if let Ok(Some(agent)) = self.agents.find_by_id(&agent_id).await {
                            Some(NodeSummary {
                                kind: "agent".to_string(),
                                id: id.to_string(),
                                label: agent.description().to_string(),
                                content: None,
                                tags: vec![],
                                status: Some(agent.status().to_string()),
                                priority: None,
                                updated_at: Some(agent.last_heartbeat().to_rfc3339()),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                ResourceKind::Message => None,
            };

            if let Some(s) = summary {
                result.insert(node_key.clone(), s);
            }
        }

        result
    }
}
