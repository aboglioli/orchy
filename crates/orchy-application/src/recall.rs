use std::sync::Arc;

use chrono::{DateTime, Utc};
use orchy_core::{
    Clock, DocumentStatus, DocumentStore, EdgeStore, EntityKind, EntityRef, Hit, Kind, Namespace,
    Passage, Search, SearchQuery, SkillStore, Tag, document_passages, rank, skill_passage,
    within_budget,
};
use serde::{Deserialize, Serialize};

use crate::dto::HitDto;
use crate::error::ApplicationResult;

const DEFAULT_LIMIT: usize = 20;
const DECAY_PER_HOP: f64 = 0.5;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecallCommand {
    pub text: String,
    pub entities: Vec<String>,
    pub kind: Vec<String>,
    pub retired: bool,
    pub status: Vec<String>,
    pub namespace: Option<String>,
    pub anchor: Option<String>,
    pub tags: Vec<String>,
    pub limit: Option<usize>,
    pub budget: Option<usize>,
    pub since: Option<DateTime<Utc>>,
    pub graph: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallDto {
    pub hits: Vec<HitDto>,
    pub total: usize,
}

pub struct Recall {
    search: Arc<dyn Search>,
    documents: Arc<dyn DocumentStore>,
    skills: Arc<dyn SkillStore>,
    edges: Arc<dyn EdgeStore>,
    clock: Arc<dyn Clock>,
}

impl Recall {
    pub fn new(
        search: Arc<dyn Search>,
        documents: Arc<dyn DocumentStore>,
        skills: Arc<dyn SkillStore>,
        edges: Arc<dyn EdgeStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            search,
            documents,
            skills,
            edges,
            clock,
        }
    }

    pub async fn execute(&self, cmd: RecallCommand) -> ApplicationResult<RecallDto> {
        let limit = cmd.limit.unwrap_or(DEFAULT_LIMIT);
        let query = SearchQuery {
            text: cmd.text,
            entities: if cmd.entities.is_empty() {
                None
            } else {
                Some(
                    cmd.entities
                        .iter()
                        .map(|e| e.parse::<EntityKind>())
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            retired: cmd.retired,
            kind: if cmd.kind.is_empty() {
                None
            } else {
                Some(
                    cmd.kind
                        .iter()
                        .map(|k| k.parse::<Kind>())
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            exclude_status: if cmd.status.is_empty() {
                DocumentStatus::RETIRED.to_vec()
            } else {
                Vec::new()
            },
            status: if cmd.status.is_empty() {
                None
            } else {
                Some(
                    cmd.status
                        .iter()
                        .map(|s| s.parse::<DocumentStatus>())
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            namespace: cmd.namespace.as_deref().map(Namespace::new).transpose()?,
            tags: cmd
                .tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?,
            since: cmd.since,
            limit,
        };

        let anchor = cmd.anchor.as_deref().map(Namespace::new).transpose()?;
        let mut hits = self.search.sections(&query).await?;
        rank(&mut hits, anchor.as_ref(), self.clock.now());
        hits.truncate(limit);
        if cmd.graph > 0 {
            self.expand(&mut hits, &query, cmd.graph).await?;
        }
        let total = hits.len();
        hits.truncate(limit);

        let hits = match cmd.budget {
            None => hits.iter().map(HitDto::from).collect(),
            Some(tokens) => within_budget(hits, tokens)
                .iter()
                .map(|hit| HitDto {
                    text: Some(hit.body.trim().to_owned()),
                    ..HitDto::from(hit)
                })
                .collect(),
        };
        Ok(RecallDto { hits, total })
    }

    /// Adds what the hits link to, each scored as the hit that led there, halved per hop.
    async fn expand(
        &self,
        hits: &mut Vec<Hit>,
        query: &SearchQuery,
        depth: u8,
    ) -> ApplicationResult<()> {
        let mut neighbours: Vec<(EntityRef, f64)> = Vec::new();
        for hit in hits.iter() {
            for hop in self.edges.neighbourhood(&hit.entity, depth).await? {
                let relevance = hit.relevance * DECAY_PER_HOP.powi(i32::from(hop.depth));
                for end in [hop.edge.from(), hop.edge.to()] {
                    if hits.iter().any(|h| &h.entity == end) {
                        continue;
                    }
                    match neighbours.iter_mut().find(|(e, _)| e == end) {
                        Some((_, best)) => *best = best.max(relevance),
                        None => neighbours.push((end.clone(), relevance)),
                    }
                }
            }
        }
        for (entity, relevance) in neighbours {
            if let Some(passage) = self.passage(&entity, query).await? {
                hits.push(Hit {
                    entity: passage.entity,
                    heading: passage.heading,
                    excerpt: passage.excerpt,
                    body: passage.body,
                    namespace: passage.namespace,
                    updated_at: passage.updated_at,
                    relevance,
                });
            }
        }
        hits.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
        Ok(())
    }

    async fn passage(
        &self,
        entity: &EntityRef,
        query: &SearchQuery,
    ) -> ApplicationResult<Option<Passage>> {
        match entity.kind() {
            EntityKind::Document if query.covers(EntityKind::Document) => Ok(self
                .documents
                .get(entity.id())
                .await?
                .filter(|d| query.selects_document(d))
                .and_then(|d| document_passages(&d).into_iter().next())),
            EntityKind::Skill if query.covers(EntityKind::Skill) => Ok(self
                .skills
                .get(entity.id())
                .await?
                .filter(|s| query.selects_skill(s))
                .map(|s| skill_passage(&s))),
            _ => Ok(None),
        }
    }
}
