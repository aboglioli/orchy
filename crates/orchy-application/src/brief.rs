use std::sync::Arc;

use orchy_core::{
    ActorId, ActorStore, DocumentQuery, DocumentStore, Kind, MessageStore, Namespace, PageRequest,
    ReadWatermarks, SkillStore, TaskQuery, TaskStatus, TaskStore, skill,
};
use serde::{Deserialize, Serialize};

use crate::dto::{ActorDto, BriefingDto, DocumentDto, SkillDto, TaskDto};
use crate::error::{ApplicationError, ApplicationResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BriefCommand {
    pub actor: String,
}

/// One read of everything a joining agent needs. Split across five commands it would be five
/// chances to skip one, and the one skipped is always the skills.
pub struct Brief {
    actors: Arc<dyn ActorStore>,
    skills: Arc<dyn SkillStore>,
    tasks: Arc<dyn TaskStore>,
    messages: Arc<dyn MessageStore>,
    watermarks: Arc<dyn ReadWatermarks>,
    documents: Arc<dyn DocumentStore>,
}

impl Brief {
    pub fn new(
        actors: Arc<dyn ActorStore>,
        skills: Arc<dyn SkillStore>,
        tasks: Arc<dyn TaskStore>,
        messages: Arc<dyn MessageStore>,
        watermarks: Arc<dyn ReadWatermarks>,
        documents: Arc<dyn DocumentStore>,
    ) -> Self {
        Self {
            actors,
            skills,
            tasks,
            messages,
            watermarks,
            documents,
        }
    }

    pub async fn execute(&self, cmd: BriefCommand) -> ApplicationResult<BriefingDto> {
        let id: ActorId = cmd.actor.parse()?;
        let actor = self
            .actors
            .get(&id)
            .await?
            .ok_or_else(|| ApplicationError::not_found("actor", &id))?;
        let namespace = actor.namespace().clone();

        Ok(BriefingDto {
            actor: ActorDto::from(&actor),
            skills: self.skills_in_force(&namespace).await?,
            unread: self.unread(&id).await?,
            claimed: self.claimed_by(&id).await?,
            next: self.next_up(&namespace).await?,
            handoff: self.handoff(&namespace).await?,
        })
    }

    async fn skills_in_force(&self, namespace: &Namespace) -> ApplicationResult<Vec<SkillDto>> {
        let all = self.skills.all().await?;
        Ok(skill::in_scope(&all, namespace)
            .iter()
            .map(SkillDto::from)
            .collect())
    }

    async fn unread(&self, actor: &ActorId) -> ApplicationResult<usize> {
        let after = self.watermarks.watermark(actor)?;
        Ok(self.messages.inbox(actor, after.as_ref()).await?.len())
    }

    async fn claimed_by(&self, actor: &ActorId) -> ApplicationResult<Vec<TaskDto>> {
        let query = TaskQuery {
            claimed_by: Some(actor.clone()),
            status: Some(vec![TaskStatus::Claimed, TaskStatus::InProgress]),
            ..Default::default()
        };
        Ok(self
            .tasks
            .find(&query, PageRequest::default())
            .await?
            .items
            .iter()
            .map(TaskDto::from)
            .collect())
    }

    /// Peeked, never claimed: a briefing tells an agent what is there, it does not decide for it.
    async fn next_up(&self, namespace: &Namespace) -> ApplicationResult<Option<TaskDto>> {
        let query = TaskQuery {
            status: Some(vec![TaskStatus::Pending]),
            namespace: Some(namespace.clone()),
            ..Default::default()
        };
        let mut open = self.tasks.find(&query, PageRequest::default()).await?.items;
        open.sort_by(|a, b| b.priority().cmp(&a.priority()).then(a.id().cmp(b.id())));
        Ok(open.first().map(TaskDto::from))
    }

    /// The last thing an agent wrote down before it stopped, which is what makes a handover a
    /// handover rather than a restart.
    async fn handoff(&self, namespace: &Namespace) -> ApplicationResult<Option<DocumentDto>> {
        let query = DocumentQuery {
            kind: Some(vec![Kind::Context]),
            namespace: Some(namespace.clone()),
            ..Default::default()
        };
        let mut found = self
            .documents
            .find(&query, PageRequest::default())
            .await?
            .items;
        found.sort_by_key(|d| d.updated_at());
        Ok(found.last().map(DocumentDto::from))
    }
}
