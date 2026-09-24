use std::sync::Arc;

use orchy_core::{Namespace, SkillStore, Tag, skill};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListSkillsCommand {
    pub namespace: Option<String>,
    pub tags: Vec<String>,
    pub everywhere: bool,
    pub retired: bool,
}

pub struct ListSkills {
    skills: Arc<dyn SkillStore>,
}

impl ListSkills {
    pub fn new(skills: Arc<dyn SkillStore>) -> Self {
        Self { skills }
    }

    pub async fn execute(&self, cmd: ListSkillsCommand) -> ApplicationResult<Vec<SkillDto>> {
        let wanted: Vec<Tag> = cmd.tags.iter().map(Tag::new).collect::<Result<_, _>>()?;
        let all: Vec<_> = self
            .skills
            .all()
            .await?
            .into_iter()
            .filter(|s| wanted.iter().all(|t| s.tags().contains(t)))
            .collect();

        if cmd.everywhere || cmd.retired {
            return Ok(all
                .iter()
                .filter(|s| cmd.retired || s.is_active())
                .map(SkillDto::from)
                .collect());
        }

        let namespace = cmd
            .namespace
            .as_deref()
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default();
        Ok(skill::in_scope(&all, &namespace)
            .iter()
            .map(SkillDto::from)
            .collect())
    }
}
