use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use orchy_core::edge::{EdgeStore, RelationType, TraversalDirection};
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgeId, KnowledgeKind, KnowledgeStore};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskId, TaskStatus, TaskStore};

use crate::dto::{AssembleContextResponse, KnowledgeResponse, TaskResponse};

pub struct AssembleContextCommand {
    pub org_id: String,
    pub kind: String,
    pub id: String,
    pub max_tokens: Option<usize>,
}

pub struct AssembleContext {
    edges: Arc<dyn EdgeStore>,
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
}

impl AssembleContext {
    pub fn new(
        edges: Arc<dyn EdgeStore>,
        tasks: Arc<dyn TaskStore>,
        knowledge: Arc<dyn KnowledgeStore>,
    ) -> Self {
        Self {
            edges,
            tasks,
            knowledge,
        }
    }

    pub async fn execute(&self, cmd: AssembleContextCommand) -> Result<AssembleContextResponse> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let kind = cmd
            .kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let content_limit = cmd.max_tokens.unwrap_or(4000) / 5;

        let from_edges = self
            .edges
            .find_from(&org, &kind, &cmd.id, None, true, None)
            .await?;
        let to_edges = self
            .edges
            .find_to(&org, &kind, &cmd.id, None, true, None)
            .await?;

        let mut knowledge_with_rel: HashMap<KnowledgeId, RelationType> = HashMap::new();
        for e in from_edges.iter().chain(to_edges.iter()) {
            let knowledge_id_str = if e.to_kind() == &ResourceKind::Knowledge {
                e.to_id()
            } else if e.from_kind() == &ResourceKind::Knowledge {
                e.from_id()
            } else {
                continue;
            };
            if let Ok(kid) = knowledge_id_str.parse::<KnowledgeId>() {
                knowledge_with_rel.entry(kid).or_insert(*e.rel_type());
            }
        }

        let mut all_entries: Vec<(orchy_core::knowledge::Knowledge, RelationType)> = Vec::new();
        for (kid, rel) in knowledge_with_rel {
            if let Some(entry) = self.knowledge.find_by_id(&kid).await? {
                all_entries.push((entry, rel));
            }
        }

        all_entries.sort_by_key(|(a, _)| std::cmp::Reverse(a.updated_at()));

        let mut used_ids: HashSet<String> = HashSet::new();

        let core_fact_entries: Vec<&(orchy_core::knowledge::Knowledge, RelationType)> = all_entries
            .iter()
            .filter(|(_, rel)| matches!(rel, RelationType::Produces | RelationType::DerivedFrom))
            .collect();
        for (k, _) in &core_fact_entries {
            used_ids.insert(k.id().to_string());
        }
        let core_facts: Vec<KnowledgeResponse> = core_fact_entries
            .iter()
            .map(|(k, _)| truncate_knowledge(k, content_limit))
            .collect();

        let decision_entries: Vec<&(orchy_core::knowledge::Knowledge, RelationType)> = all_entries
            .iter()
            .filter(|(k, _)| {
                !used_ids.contains(&k.id().to_string())
                    && matches!(
                        k.kind(),
                        KnowledgeKind::Decision | KnowledgeKind::Plan | KnowledgeKind::Skill
                    )
            })
            .collect();
        for (k, _) in &decision_entries {
            used_ids.insert(k.id().to_string());
        }
        let relevant_decisions: Vec<KnowledgeResponse> = decision_entries
            .iter()
            .map(|(k, _)| truncate_knowledge(k, content_limit))
            .collect();

        let recent_change_entries: Vec<&(orchy_core::knowledge::Knowledge, RelationType)> =
            all_entries
                .iter()
                .filter(|(k, _)| !used_ids.contains(&k.id().to_string()))
                .take(5)
                .collect();
        let recent_changes: Vec<KnowledgeResponse> = recent_change_entries
            .iter()
            .map(|(k, _)| truncate_knowledge(k, content_limit))
            .collect();

        let open_dependencies = if kind == ResourceKind::Task {
            let mut deps = Vec::new();
            for e in from_edges.iter().filter(|e| {
                *e.rel_type() == RelationType::DependsOn && e.to_kind() == &ResourceKind::Task
            }) {
                if let Ok(dep_id) = e.to_id().parse::<TaskId>()
                    && let Some(dep_task) = self.tasks.find_by_id(&dep_id).await?
                    && dep_task.status() != TaskStatus::Completed
                {
                    deps.push(TaskResponse::from(&dep_task));
                }
            }
            deps
        } else {
            Vec::new()
        };

        let risk_flags = if kind == ResourceKind::Task {
            let traversal = self
                .edges
                .traverse(
                    &org,
                    &kind,
                    &cmd.id,
                    2,
                    Some(&[RelationType::DependsOn]),
                    TraversalDirection::Outgoing,
                    true,
                    None,
                )
                .await?;
            let mut flags = Vec::new();
            for te in traversal {
                if te.to_kind == ResourceKind::Task
                    && let Ok(tid) = te.to_id.parse::<TaskId>()
                    && let Some(task) = self.tasks.find_by_id(&tid).await?
                    && matches!(task.status(), TaskStatus::Failed | TaskStatus::Cancelled)
                {
                    flags.push(format!(
                        "Dependency '{}' ({}) is {}",
                        task.title(),
                        task.id(),
                        task.status()
                    ));
                }
            }
            flags
        } else {
            Vec::new()
        };

        Ok(AssembleContextResponse {
            root_kind: cmd.kind,
            root_id: cmd.id,
            core_facts,
            open_dependencies,
            relevant_decisions,
            recent_changes,
            risk_flags,
        })
    }
}

fn truncate_knowledge(k: &orchy_core::knowledge::Knowledge, limit: usize) -> KnowledgeResponse {
    let mut resp = KnowledgeResponse::from(k);
    if resp.content.len() > limit {
        resp.content = resp.content.chars().take(limit).collect();
    }
    resp
}
