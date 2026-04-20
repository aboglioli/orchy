use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::edge::{EdgeStore, RelationDirection, TraversalHop};
use orchy_core::error::{Error, Result};
use orchy_core::graph::neighborhood::{
    AgentSummary, EntityNeighborhood, KnowledgeSummary, MessageSummary, PeerEntity, Relation,
    TaskSummary,
};
use orchy_core::graph::relation_options::RelationOptions;
use orchy_core::knowledge::{Knowledge, KnowledgeId, KnowledgeStore};
use orchy_core::message::{Message, MessageId, MessageStatus, MessageStore};
use orchy_core::namespace::Namespace;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::{ResourceKind, ResourceRef};
use orchy_core::task::{Task, TaskId, TaskStore};

pub struct MaterializeNeighborhoodCommand {
    pub org_id: String,
    pub anchor_kind: String,
    pub anchor_id: String,
    pub options: RelationOptions,
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
    pub project: Option<String>,
    pub namespace: Option<String>,
    pub semantic_query: Option<String>,
}

pub struct MaterializeNeighborhood {
    edges: Arc<dyn EdgeStore>,
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
    agents: Arc<dyn AgentStore>,
    messages: Arc<dyn MessageStore>,
}

impl MaterializeNeighborhood {
    pub fn new(
        edges: Arc<dyn EdgeStore>,
        tasks: Arc<dyn TaskStore>,
        knowledge: Arc<dyn KnowledgeStore>,
        agents: Arc<dyn AgentStore>,
        messages: Arc<dyn MessageStore>,
    ) -> Self {
        Self {
            edges,
            tasks,
            knowledge,
            agents,
            messages,
        }
    }

    pub async fn execute(&self, cmd: MaterializeNeighborhoodCommand) -> Result<EntityNeighborhood> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let anchor_kind = cmd
            .anchor_kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let anchor = ResourceRef::new(anchor_kind.clone(), cmd.anchor_id.clone());

        let rel_types = cmd.options.resolve_rel_types(&anchor_kind);
        let max_depth = cmd.options.effective_max_depth();
        let limit = cmd.options.effective_limit();

        let hops: Vec<TraversalHop> = self
            .edges
            .find_neighbors(
                &org_id,
                &anchor_kind,
                &cmd.anchor_id,
                rel_types,
                &cmd.options.target_kinds,
                cmd.options.direction,
                max_depth,
                cmd.as_of,
                limit,
            )
            .await?;

        let mut task_ids: Vec<TaskId> = vec![];
        let mut knowledge_ids: Vec<KnowledgeId> = vec![];
        let mut knowledge_paths: Vec<String> = vec![];
        let mut agent_ids: Vec<AgentId> = vec![];
        let mut message_ids: Vec<MessageId> = vec![];

        for hop in &hops {
            let (peer_kind, peer_id) = match hop.direction {
                RelationDirection::Outgoing => (hop.edge.to_kind(), hop.edge.to_id()),
                RelationDirection::Incoming => (hop.edge.from_kind(), hop.edge.from_id()),
            };
            match peer_kind {
                ResourceKind::Task => {
                    if let Ok(id) = peer_id.parse::<TaskId>() {
                        task_ids.push(id);
                    }
                }
                ResourceKind::Knowledge => {
                    if let Ok(id) = peer_id.parse::<KnowledgeId>() {
                        knowledge_ids.push(id);
                    } else {
                        knowledge_paths.push(peer_id.to_string());
                    }
                }
                ResourceKind::Agent => {
                    if let Ok(id) = peer_id.parse::<AgentId>() {
                        agent_ids.push(id);
                    }
                }
                ResourceKind::Message => {
                    if let Ok(id) = peer_id.parse::<MessageId>() {
                        message_ids.push(id);
                    }
                }
            }
        }

        task_ids.sort_unstable_by_key(|id| id.to_string());
        task_ids.dedup_by_key(|id| id.to_string());
        knowledge_ids.sort_unstable_by_key(|id| id.to_string());
        knowledge_ids.dedup_by_key(|id| id.to_string());
        agent_ids.sort_unstable_by_key(|id| id.to_string());
        agent_ids.dedup_by_key(|id| id.to_string());
        message_ids.sort_unstable_by_key(|id| id.to_string());
        message_ids.dedup_by_key(|id| id.to_string());

        let (tasks, agents, messages) = tokio::try_join!(
            self.tasks.find_by_ids(&task_ids),
            self.agents.find_by_ids(&agent_ids),
            self.messages.find_by_ids(&message_ids),
        )?;
        let mut knowledge_entries = self.knowledge.find_by_ids(&knowledge_ids).await?;

        // Resolve knowledge paths to entries (edges may store path instead of UUID)
        knowledge_paths.sort();
        knowledge_paths.dedup();
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        for path in &knowledge_paths {
            if let Ok(Some(entry)) = self
                .knowledge
                .find_by_path(
                    &org_id,
                    None,
                    &orchy_core::namespace::Namespace::root(),
                    path,
                )
                .await
            {
                knowledge_entries.push(entry);
            }
        }

        let task_map: HashMap<String, &Task> =
            tasks.iter().map(|t| (t.id().to_string(), t)).collect();
        let mut knowledge_map: HashMap<String, &Knowledge> = knowledge_entries
            .iter()
            .map(|k| (k.id().to_string(), k))
            .collect();
        // Also index by path for path-based edge lookups
        for k in &knowledge_entries {
            knowledge_map.insert(k.path().to_string(), k);
        }
        let agent_map: HashMap<String, &Agent> =
            agents.iter().map(|a| (a.id().to_string(), a)).collect();
        let message_map: HashMap<String, &Message> =
            messages.iter().map(|m| (m.id().to_string(), m)).collect();

        let mut relations: Vec<Relation> = hops
            .into_iter()
            .filter_map(|hop| {
                let (peer_kind, peer_id) = match hop.direction {
                    RelationDirection::Outgoing => {
                        (hop.edge.to_kind().clone(), hop.edge.to_id().to_string())
                    }
                    RelationDirection::Incoming => {
                        (hop.edge.from_kind().clone(), hop.edge.from_id().to_string())
                    }
                };

                let peer = match &peer_kind {
                    ResourceKind::Task => task_map.get(&peer_id).map(|t| {
                        PeerEntity::Task(TaskSummary {
                            id: t.id().to_string(),
                            title: t.title().to_string(),
                            status: t.status().to_string(),
                            priority: t.priority().to_string(),
                            assigned_to: t.assigned_to().map(|id| id.to_string()),
                        })
                    })?,
                    ResourceKind::Knowledge => knowledge_map.get(&peer_id).map(|k| {
                        PeerEntity::Knowledge(KnowledgeSummary {
                            id: k.id().to_string(),
                            title: k.title().to_string(),
                            entry_kind: k.kind().to_string(),
                            path: k.path().to_string(),
                            tags: k.tags().to_vec(),
                        })
                    })?,
                    ResourceKind::Agent => agent_map.get(&peer_id).map(|a| {
                        PeerEntity::Agent(AgentSummary {
                            id: a.id().to_string(),
                            description: a.description().to_string(),
                            status: a.status().to_string(),
                            roles: a.roles().to_vec(),
                        })
                    })?,
                    ResourceKind::Message => message_map.get(&peer_id).map(|m| {
                        PeerEntity::Message(MessageSummary {
                            id: m.id().to_string(),
                            body: m.body().to_string(),
                            status: match m.status() {
                                MessageStatus::Pending => "pending".to_string(),
                                MessageStatus::Delivered => "delivered".to_string(),
                                MessageStatus::Read => "read".to_string(),
                            },
                            from: m.from().to_string(),
                        })
                    })?,
                };

                Some(Relation {
                    edge_id: hop.edge.id().to_string(),
                    rel_type: hop.edge.rel_type().clone(),
                    direction: hop.direction,
                    depth: hop.depth,
                    via: hop.via,
                    created_at: hop.edge.created_at(),
                    created_by: hop.edge.created_by().map(|id| id.to_string()),
                    peer,
                    similarity_score: None,
                })
            })
            .collect();

        relations.sort_by(|a, b| a.depth.cmp(&b.depth).then(b.created_at.cmp(&a.created_at)));

        Ok(EntityNeighborhood { anchor, relations })
    }
}
