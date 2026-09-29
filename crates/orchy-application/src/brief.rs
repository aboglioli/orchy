use std::sync::Arc;

use chrono::{DateTime, Utc};
use orchy_core::task::dependencies::Outcome;
use orchy_core::{
    ActorId, ActorStore, DocumentQuery, DocumentStore, EventLog, EventQuery, Integrity, Kind,
    MessageStore, Namespace, ReadWatermarks, SkillStore, Task, TaskQuery, TaskStatus, TaskStore,
    skill,
};
use serde::{Deserialize, Serialize};

use crate::assess_dependencies::AssessDependencies;
use crate::dto::{ActorDto, BriefingDto, DocumentDto, SinceLastDto, SkillDto, TaskDto};
use crate::error::{ApplicationError, ApplicationResult};
use crate::rank_claimable::RankClaimable;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BriefCommand {
    pub actor: String,
    pub since: Option<DateTime<Utc>>,
}

pub struct BriefSources {
    pub actors: Arc<dyn ActorStore>,
    pub skills: Arc<dyn SkillStore>,
    pub tasks: Arc<dyn TaskStore>,
    pub messages: Arc<dyn MessageStore>,
    pub watermarks: Arc<dyn ReadWatermarks>,
    pub documents: Arc<dyn DocumentStore>,
    pub integrity: Arc<dyn Integrity>,
    pub ranking: Arc<RankClaimable>,
    pub dependencies: Arc<AssessDependencies>,
    pub log: Arc<dyn EventLog>,
}

pub struct Brief {
    sources: BriefSources,
}

impl Brief {
    pub fn new(sources: BriefSources) -> Self {
        Self { sources }
    }

    pub async fn execute(&self, cmd: BriefCommand) -> ApplicationResult<BriefingDto> {
        let id: ActorId = cmd.actor.parse()?;
        let actor = self
            .sources
            .actors
            .get(&id)
            .await?
            .ok_or_else(|| ApplicationError::not_found("actor", &id))?;
        let namespace = actor.namespace().clone();

        let claimed = self.claimed_by(&id).await?;
        let mut doomed = Vec::new();
        for task in &claimed {
            if self.sources.dependencies.outcome(task).await? == Outcome::Doomed {
                doomed.push(TaskDto::from(task));
            }
        }

        Ok(BriefingDto {
            actor: ActorDto::from(&actor),
            skills: self.skills_in_force(&namespace).await?,
            unread: self.unread(&id).await?,
            claimed: claimed.iter().map(TaskDto::from).collect(),
            next: self.next_up(&namespace).await?,
            handoff: self.handoff(&namespace).await?,
            unreadable: self.sources.integrity.unreadable().await?.len(),
            doomed,
            since_last: match cmd.since {
                Some(since) => Some(self.since_last(&id, &namespace, since).await?),
                None => None,
            },
        })
    }

    /// What others changed where the actor works while it was away.
    async fn since_last(
        &self,
        actor: &ActorId,
        namespace: &Namespace,
        since: DateTime<Utc>,
    ) -> ApplicationResult<SinceLastDto> {
        let events = self
            .sources
            .log
            .replay(&EventQuery {
                since: Some(since),
                ..Default::default()
            })
            .await?;
        let mut changes = SinceLastDto {
            since,
            ..Default::default()
        };
        let me = actor.to_string();
        for event in events {
            if event.actor.as_deref() == Some(me.as_str()) {
                continue;
            }
            let within = Namespace::new(&event.namespace).is_ok_and(|ns| namespace.contains(&ns));
            if !within {
                continue;
            }
            match event.topic.as_str() {
                "task.finished" | "task.rolled_up" => match event.payload["status"].as_str() {
                    Some("completed") => changes.tasks_completed += 1,
                    Some("failed") => changes.tasks_failed += 1,
                    _ => {}
                },
                "document.created" => changes.documents_created += 1,
                "document.superseded" => changes.documents_superseded += 1,
                topic if topic.starts_with("skill.") => changes.skills_changed += 1,
                _ => {}
            }
        }
        Ok(changes)
    }

    async fn skills_in_force(&self, namespace: &Namespace) -> ApplicationResult<Vec<SkillDto>> {
        let all = self.sources.skills.all().await?;
        Ok(skill::in_scope(&all, namespace)
            .iter()
            .map(SkillDto::from)
            .collect())
    }

    async fn unread(&self, actor: &ActorId) -> ApplicationResult<usize> {
        let after = self.sources.watermarks.watermark(actor)?;
        Ok(self
            .sources
            .messages
            .inbox(actor, after.as_ref())
            .await?
            .len())
    }

    async fn claimed_by(&self, actor: &ActorId) -> ApplicationResult<Vec<Task>> {
        Ok(self
            .sources
            .tasks
            .matching(&TaskQuery {
                claimed_by: Some(actor.clone()),
                status: Some(vec![TaskStatus::Claimed, TaskStatus::InProgress]),
                ..Default::default()
            })
            .await?)
    }

    /// Exactly what `orchy task next --namespace <namespace>` would hand out.
    async fn next_up(&self, namespace: &Namespace) -> ApplicationResult<Option<TaskDto>> {
        let ranked = self
            .sources
            .ranking
            .execute(Some(namespace.clone()), None)
            .await?;
        Ok(ranked.first().map(TaskDto::from))
    }

    async fn handoff(&self, namespace: &Namespace) -> ApplicationResult<Option<DocumentDto>> {
        let handoffs = self
            .sources
            .documents
            .matching(&DocumentQuery {
                kind: Some(vec![Kind::Context]),
                namespace: Some(namespace.clone()),
                ..Default::default()
            })
            .await?;
        Ok(handoffs
            .iter()
            .max_by(|a, b| {
                a.updated_at()
                    .cmp(&b.updated_at())
                    .then_with(|| a.id().cmp(b.id()))
            })
            .map(DocumentDto::from))
    }
}
