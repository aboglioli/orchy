use std::sync::Arc;

use orchy_core::{
    DocumentQuery, DocumentStore, MessageStore, Namespace, SkillStore, TaskQuery, TaskStore,
};
use serde::{Deserialize, Serialize};

use crate::dto::{DocumentDto, MessageDto, SkillDto, TaskDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportVaultCommand {
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "entity", rename_all = "lowercase")]
pub enum ExportedEntity {
    Document(DocumentDto),
    Skill(SkillDto),
    Task(TaskDto),
    Message(MessageDto),
}

pub struct ExportVault {
    documents: Arc<dyn DocumentStore>,
    skills: Arc<dyn SkillStore>,
    tasks: Arc<dyn TaskStore>,
    messages: Arc<dyn MessageStore>,
}

impl ExportVault {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        skills: Arc<dyn SkillStore>,
        tasks: Arc<dyn TaskStore>,
        messages: Arc<dyn MessageStore>,
    ) -> Self {
        Self {
            documents,
            skills,
            tasks,
            messages,
        }
    }

    pub async fn execute(&self, cmd: ExportVaultCommand) -> ApplicationResult<Vec<ExportedEntity>> {
        let namespace = cmd.namespace.as_deref().map(Namespace::new).transpose()?;
        let within = |ns: &Namespace| namespace.as_ref().is_none_or(|n| n.contains(ns));

        let mut exported = Vec::new();
        for document in self.documents.matching(&DocumentQuery::default()).await? {
            if within(document.namespace()) {
                exported.push(ExportedEntity::Document(DocumentDto::from(&document)));
            }
        }
        for skill in self.skills.all().await? {
            if within(skill.namespace()) {
                exported.push(ExportedEntity::Skill(SkillDto::from(&skill)));
            }
        }
        for task in self.tasks.matching(&TaskQuery::default()).await? {
            if within(task.namespace()) {
                exported.push(ExportedEntity::Task(TaskDto::from(&task)));
            }
        }
        for message in self.messages.all().await? {
            if within(message.namespace()) {
                exported.push(ExportedEntity::Message(MessageDto::from(&message)));
            }
        }
        Ok(exported)
    }
}
