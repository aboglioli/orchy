use std::sync::Arc;

use orchy_core::{DomainError, Id, Namespace, Skill, SkillName, SkillStore, skill};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadSkillCommand {
    pub target: String,
    pub namespace: Option<String>,
}

pub struct ReadSkill {
    skills: Arc<dyn SkillStore>,
}

impl ReadSkill {
    pub fn new(skills: Arc<dyn SkillStore>) -> Self {
        Self { skills }
    }

    pub async fn execute(&self, cmd: ReadSkillCommand) -> ApplicationResult<SkillDto> {
        let namespace = cmd
            .namespace
            .as_deref()
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default();

        if let Ok(id) = Id::new(&cmd.target) {
            return Ok(SkillDto::from(&self.skills.require(&id).await?));
        }

        let name = SkillName::new(&cmd.target)?;
        let all = self.skills.all().await?;

        if let Some(found) = skill::in_scope(&all, &namespace)
            .into_iter()
            .find(|s: &Skill| s.name() == &name)
        {
            return Ok(SkillDto::from(&found));
        }

        let elsewhere: Vec<&Skill> = all.iter().filter(|s| s.name() == &name).collect();
        match elsewhere.as_slice() {
            [only] => Ok(SkillDto::from(*only)),
            [] => Err(DomainError::not_found("skill", &cmd.target).into()),
            many => Err(DomainError::Ambiguous {
                input: cmd.target.clone(),
                count: many.len(),
            }
            .into()),
        }
    }
}
