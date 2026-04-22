use std::collections::HashSet;
use std::sync::Arc;

use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::graph::{EdgeStore, TraversalDirection};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use orchy_core::namespace::ProjectId;

use crate::dto::KnowledgeResponse;
use crate::parse_namespace;

pub struct SearchKnowledgeCommand {
    pub org_id: String,
    pub query: String,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<u32>,
    pub project: Option<String>,
    pub min_score: Option<f32>,
    pub anchor_kind: Option<String>,
    pub anchor_id: Option<String>,
    pub task_id: Option<String>,
}

pub struct SearchKnowledge {
    store: Arc<dyn KnowledgeStore>,
    embeddings: Option<Arc<dyn EmbeddingsProvider>>,
    edges: Arc<dyn EdgeStore>,
}

impl SearchKnowledge {
    pub fn new(
        store: Arc<dyn KnowledgeStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
        edges: Arc<dyn EdgeStore>,
    ) -> Self {
        Self {
            store,
            embeddings,
            edges,
        }
    }

    pub async fn execute(&self, cmd: SearchKnowledgeCommand) -> Result<Vec<KnowledgeResponse>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let namespace = cmd
            .namespace
            .as_deref()
            .map(|s| parse_namespace(Some(s)))
            .transpose()?;

        let limit = cmd.limit.unwrap_or(20) as usize;

        let embedding = if let Some(emb) = &self.embeddings {
            Some(emb.embed(&cmd.query).await?)
        } else {
            None
        };

        let project = cmd
            .project
            .map(|s| ProjectId::try_from(s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        let mut scored = self
            .store
            .search(
                &org_id,
                &cmd.query,
                embedding.as_deref(),
                namespace.as_ref(),
                limit * 3,
            )
            .await?;

        for (entry, score) in &mut scored {
            let kw = keyword_overlap_score(&cmd.query, entry.title(), entry.content());
            *score = Some(match *score {
                Some(emb_score) => 0.7 * emb_score + 0.3 * kw,
                None => kw,
            });
        }

        if let (Some(ak), Some(ai)) = (cmd.anchor_kind.as_deref(), cmd.anchor_id.as_deref())
            && let Ok(anchor_kind) = ak.parse::<ResourceKind>()
        {
            let linked = self.linked_knowledge_ids(&org_id, anchor_kind, ai).await?;
            for (entry, score) in &mut scored {
                if linked.contains(&entry.id().to_string()) {
                    *score = Some(score.unwrap_or(0.0) + 0.2);
                }
            }
        }

        if let Some(task_id) = cmd.task_id.as_deref() {
            let linked = self.task_subgraph_knowledge_ids(&org_id, task_id).await?;
            for (entry, score) in &mut scored {
                if linked.contains(&entry.id().to_string()) {
                    *score = Some(score.unwrap_or(0.0) + 0.2);
                }
            }
        }

        scored.sort_by(|(_, a), (_, b)| {
            b.unwrap_or(0.0)
                .partial_cmp(&a.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let min_score = cmd.min_score;
        let filtered: Vec<_> = scored
            .iter()
            .take(limit)
            .filter(|(_, score)| {
                min_score
                    .and_then(|m| score.map(|s| s >= m))
                    .unwrap_or(true)
            })
            .filter(|(e, _)| {
                if let Some(ref pid) = project {
                    e.project().map(|p| p == pid).unwrap_or(false)
                } else {
                    true
                }
            })
            .filter(|(e, _)| {
                if let Some(ref k) = cmd.kind {
                    e.kind().to_string() == *k
                } else {
                    true
                }
            })
            .map(|(k, score)| KnowledgeResponse::with_score(k, *score))
            .collect();

        Ok(filtered)
    }

    async fn linked_knowledge_ids(
        &self,
        org: &OrganizationId,
        kind: ResourceKind,
        id: &str,
    ) -> Result<HashSet<String>> {
        let mut ids = HashSet::new();
        let from = self.edges.find_from(org, &kind, id, &[], None).await?;
        for e in from {
            if e.to_kind() == &ResourceKind::Knowledge {
                ids.insert(e.to_id().to_string());
            }
        }
        let to = self.edges.find_to(org, &kind, id, &[], None).await?;
        for e in to {
            if e.from_kind() == &ResourceKind::Knowledge {
                ids.insert(e.from_id().to_string());
            }
        }
        Ok(ids)
    }

    async fn task_subgraph_knowledge_ids(
        &self,
        org: &OrganizationId,
        task_id: &str,
    ) -> Result<HashSet<String>> {
        let traversal = self
            .edges
            .find_neighbors(
                org,
                &ResourceKind::Task,
                task_id,
                &[],
                &[],
                TraversalDirection::Both,
                3,
                None,
                500,
            )
            .await?;

        let mut task_ids: HashSet<String> = traversal
            .iter()
            .flat_map(|hop| {
                let mut v = Vec::new();
                if hop.edge.from_kind() == &ResourceKind::Task {
                    v.push(hop.edge.from_id().to_string());
                }
                if hop.edge.to_kind() == &ResourceKind::Task {
                    v.push(hop.edge.to_id().to_string());
                }
                v
            })
            .collect();
        task_ids.insert(task_id.to_string());

        let mut knowledge_ids = HashSet::new();
        for tid in &task_ids {
            let found = self
                .linked_knowledge_ids(org, ResourceKind::Task, tid)
                .await?;
            knowledge_ids.extend(found);
        }
        Ok(knowledge_ids)
    }
}

fn keyword_overlap_score(query: &str, title: &str, content: &str) -> f32 {
    let query_lower = query.to_lowercase();
    let words: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();
    if words.is_empty() {
        return 0.0;
    }
    let text = format!("{} {}", title.to_lowercase(), content.to_lowercase());
    let matched = words.iter().filter(|w| text.contains(**w)).count();
    matched as f32 / words.len() as f32
}
