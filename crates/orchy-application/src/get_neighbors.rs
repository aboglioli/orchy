use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use orchy_core::agent::AgentStore;
use orchy_core::edge::{EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::TaskStore;

use crate::dto::{EdgeResponse, GetNeighborsResponse, NodeSummary};

pub struct GetNeighborsCommand {
    pub org_id: String,
    pub kind: String,
    pub id: String,
    pub direction: Option<String>,
    pub rel_type: Option<String>,
    pub include_nodes: bool,
    pub node_content_limit: Option<usize>,
    pub only_active: bool,
    pub as_of: Option<DateTime<Utc>>,
}

pub struct GetNeighbors {
    store: Arc<dyn EdgeStore>,
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
    agents: Arc<dyn AgentStore>,
}

impl GetNeighbors {
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

    pub async fn execute(&self, cmd: GetNeighborsCommand) -> Result<GetNeighborsResponse> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let kind = cmd
            .kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let rel_type: Option<RelationType> = cmd
            .rel_type
            .map(|s| s.parse().map_err(Error::InvalidInput))
            .transpose()?;

        let rel_types: &[RelationType] = if let Some(ref rt) = rel_type {
            std::slice::from_ref(rt)
        } else {
            &[]
        };
        let edges = match cmd.direction.as_deref() {
            Some("incoming") => {
                self.store
                    .find_to(&org, &kind, &cmd.id, rel_types, cmd.as_of)
                    .await?
            }
            Some("outgoing") => {
                self.store
                    .find_from(&org, &kind, &cmd.id, rel_types, cmd.as_of)
                    .await?
            }
            _ => {
                let mut out = self
                    .store
                    .find_from(&org, &kind, &cmd.id, rel_types, cmd.as_of)
                    .await?;
                let inc = self
                    .store
                    .find_to(&org, &kind, &cmd.id, rel_types, cmd.as_of)
                    .await?;
                out.extend(inc);
                out
            }
        };

        let edge_responses: Vec<EdgeResponse> = edges.iter().map(EdgeResponse::from).collect();

        let nodes = if cmd.include_nodes {
            let mut node_set: HashSet<String> = HashSet::new();
            node_set.insert(format!("{}:{}", kind, cmd.id));
            for e in &edges {
                node_set.insert(format!("{}:{}", e.from_kind(), e.from_id()));
                node_set.insert(format!("{}:{}", e.to_kind(), e.to_id()));
            }
            let node_ids: Vec<String> = node_set.into_iter().collect();
            let limit = cmd.node_content_limit.unwrap_or(500);
            Some(self.fetch_nodes(&node_ids, limit).await)
        } else {
            None
        };

        Ok(GetNeighborsResponse {
            edges: edge_responses,
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
