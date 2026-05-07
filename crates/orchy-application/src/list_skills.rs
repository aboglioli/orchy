use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::Result;
use orchy_core::knowledge::{Knowledge, KnowledgeFilter, KnowledgeKind, KnowledgeStore};

use crate::dto::KnowledgeDto;
use crate::parse_namespace;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

pub struct ListSkillsCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
}

pub struct ListSkills {
    store: Arc<dyn KnowledgeStore>,
}

impl ListSkills {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: ListSkillsCommand) -> ApplicationResult<Vec<KnowledgeDto>> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let entries = self
            .list_with_inheritance(&org_id, &project, &namespace, KnowledgeKind::Skill)
            .await?;
        Ok(entries.iter().map(KnowledgeDto::from).collect())
    }

    pub(crate) async fn list_with_inheritance(
        &self,
        org_id: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        kind: KnowledgeKind,
    ) -> Result<Vec<Knowledge>> {
        let filter = KnowledgeFilter {
            org_id: Some(org_id.clone()),
            project: Some(project.clone()),
            include_org_level: true,
            kind: Some(kind),
            include_archived: None,
            ..Default::default()
        };
        let all = self
            .store
            .list(filter, PageParams::unbounded())
            .await?
            .items;
        Ok(filter_with_inheritance(all, namespace))
    }
}

fn filter_with_inheritance(entries: Vec<Knowledge>, namespace: &Namespace) -> Vec<Knowledge> {
    let mut result: Vec<Knowledge> = Vec::new();

    for entry in entries {
        if entry.namespace().starts_with(namespace) || namespace.starts_with(entry.namespace()) {
            if let Some(pos) = result.iter().position(|e| e.path() == entry.path()) {
                if entry.namespace().as_ref().len() > result[pos].namespace().as_ref().len() {
                    result[pos] = entry;
                }
            } else {
                result.push(entry);
            }
        }
    }

    result.sort_by(|a, b| a.path().cmp(b.path()));
    result
}
