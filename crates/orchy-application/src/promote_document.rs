use std::sync::Arc;

use orchy_core::{
    ActorId, ActorStore, Clock, Document, DocumentStore, DomainError, Edge, EdgeStore, EntityKind,
    EntityRef, Id, IdGenerator, Kind, Namespace, Relation, Skill, SkillName, SkillStore, Summary,
    UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::dto::{DocumentDto, SkillDto};
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

const INTO_SKILL: &str = "skill";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromoteDocumentCommand {
    pub actor: Option<String>,
    pub document_id: String,
    pub into: String,
    pub namespace: Option<String>,
    pub skill_name: Option<String>,
    pub summary: Option<String>,
    pub if_match: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteDocumentResponse {
    pub document: DocumentDto,
    pub skill: Option<SkillDto>,
}

pub struct PromoteDocument {
    documents: Arc<dyn DocumentStore>,
    actors: Arc<dyn ActorStore>,
    skills: Arc<dyn SkillStore>,
    edges: Arc<dyn EdgeStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl PromoteDocument {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        actors: Arc<dyn ActorStore>,
        skills: Arc<dyn SkillStore>,
        edges: Arc<dyn EdgeStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            documents,
            actors,
            skills,
            edges,
            ids,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(
        &self,
        cmd: PromoteDocumentCommand,
    ) -> ApplicationResult<PromoteDocumentResponse> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(
        &self,
        cmd: PromoteDocumentCommand,
    ) -> ApplicationResult<PromoteDocumentResponse> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;
        document.ensure_unchanged(cmd.if_match.as_deref())?;
        let into = match (&cmd.namespace, &cmd.actor) {
            (Some(ns), _) => Namespace::new(ns)?,
            (None, Some(actor)) => self.actors.home_of(&actor.parse::<ActorId>()?).await?,
            (None, None) => Namespace::root(),
        };

        if cmd.into.trim().eq_ignore_ascii_case(INTO_SKILL) {
            return self.graduate_as_skill(document, into, &cmd).await;
        }

        document.promote(cmd.into.parse::<Kind>()?, into, &*self.clock)?;
        self.documents.save(&mut document).await?;
        Ok(PromoteDocumentResponse {
            document: DocumentDto::from(&document),
            skill: None,
        })
    }

    async fn graduate_as_skill(
        &self,
        mut candidate: Document,
        namespace: Namespace,
        cmd: &PromoteDocumentCommand,
    ) -> ApplicationResult<PromoteDocumentResponse> {
        let raw_name = cmd.skill_name.as_deref().ok_or_else(|| {
            DomainError::validation("promoting into a skill needs --name: skills are found by name")
        })?;
        let name = SkillName::new(raw_name)?;
        let summary = Summary::new(
            cmd.summary
                .clone()
                .unwrap_or_else(|| candidate.title().to_string()),
        )?;

        let taken = self
            .skills
            .all()
            .await?
            .into_iter()
            .any(|s| s.name() == &name && s.namespace() == &namespace);
        if taken {
            return Err(DomainError::conflict(format!(
                "a skill named `{name}` already exists in {namespace}; revise it with `orchy skill write`"
            ))
            .into());
        }

        candidate.mark_promoted(&*self.clock)?;
        let mut skill = Skill::create(
            name,
            summary,
            namespace,
            candidate.body().clone(),
            &*self.ids,
            &*self.clock,
        );
        self.skills.save(&mut skill).await?;
        self.documents.save(&mut candidate).await?;
        self.edges
            .add(&Edge::new(
                EntityRef::new(EntityKind::Skill, skill.id().clone()),
                EntityRef::document(candidate.id().clone()),
                Relation::DerivedFrom,
            )?)
            .await?;

        Ok(PromoteDocumentResponse {
            document: DocumentDto::from(&candidate),
            skill: Some(SkillDto::from(&skill)),
        })
    }
}
