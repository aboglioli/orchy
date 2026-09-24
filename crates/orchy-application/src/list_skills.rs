use std::sync::Arc;

use orchy_core::{Namespace, SkillStore, skill};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListSkillsCommand {
    pub namespace: Option<String>,
    /// Every skill in the vault, including ones no namespace the agent works in declares
    pub everywhere: bool,
    /// Retired skills too, which no briefing shows
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
        let all = self.skills.all().await?;

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
