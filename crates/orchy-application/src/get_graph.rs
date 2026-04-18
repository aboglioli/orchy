use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
        Self { store, tasks, knowledge, agents }
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

        let traversal = self
            .store
            .traverse(
                &org,
                &kind,
                &cmd.id,
                max_depth,
                rel_types.as_deref(),
                direction,
            )
            .await?;

        let mut node_set: HashSet<String> = HashSet::new();
        node_set.insert(format!("{}:{}", kind, cmd.id));
        for e in &traversal {
            node_set.insert(format!("{}:{}", e.from_kind, e.from_id));
            node_set.insert(format!("{}:{}", e.to_kind, e.to_id));
        }
        let mut node_ids: Vec<String> = node_set.into_iter().collect();
        node_ids.sort();

        let edges: Vec<TraversalEdgeResponse> =
            traversal.iter().map(TraversalEdgeResponse::from).collect();

        let nodes = if cmd.include_nodes {
            Some(self.fetch_nodes(&node_ids).await)
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

    async fn fetch_nodes(&self, node_ids: &[String]) -> HashMap<String, NodeSummary> {
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
                            Some(NodeSummary {
                                kind: "task".to_string(),
                                id: id.to_string(),
                                label: task.title().to_string(),
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
                            Some(NodeSummary {
                                kind: "knowledge".to_string(),
                                id: id.to_string(),
                                label: entry.title().to_string(),
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
