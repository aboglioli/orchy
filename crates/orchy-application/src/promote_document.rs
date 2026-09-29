use std::sync::Arc;

use orchy_core::{
    Clock, Document, DocumentStore, DomainError, Edge, EdgeStore, EntityKind, EntityRef, Id,
    IdGenerator, Kind, Namespace, Relation, Skill, SkillName, SkillStore, Summary,
};
use serde::{Deserialize, Serialize};

use crate::dto::{DocumentDto, SkillDto};
use crate::error::ApplicationResult;

const INTO_SKILL: &str = "skill";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromoteDocumentCommand {
    pub document_id: String,
    pub into: String,
    pub namespace: Option<String>,
    pub skill_name: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteDocumentResponse {
    pub document: DocumentDto,
    pub skill: Option<SkillDto>,
}

pub struct PromoteDocument {
    documents: Arc<dyn DocumentStore>,
    skills: Arc<dyn SkillStore>,
    edges: Arc<dyn EdgeStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl PromoteDocument {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        skills: Arc<dyn SkillStore>,
        edges: Arc<dyn EdgeStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            documents,
            skills,
            edges,
            ids,
            clock,
        }
    }

    pub async fn execute(
        &self,
        cmd: PromoteDocumentCommand,
    ) -> ApplicationResult<PromoteDocumentResponse> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;
        let into = match &cmd.namespace {
            Some(ns) => Namespace::new(ns)?,
            None => Namespace::root(),
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
