use std::sync::Arc;

use orchy_core::{Clock, Id, SkillStore, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetireSkillCommand {
    pub skill_id: String,
    pub restore: bool,
}

pub struct RetireSkill {
    skills: Arc<dyn SkillStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl RetireSkill {
    pub fn new(
        skills: Arc<dyn SkillStore>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            skills,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: RetireSkillCommand) -> ApplicationResult<SkillDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: RetireSkillCommand) -> ApplicationResult<SkillDto> {
        let mut skill = self.skills.require(&Id::new(&cmd.skill_id)?).await?;
        if cmd.restore {
            skill.restore(&*self.clock)?;
        } else {
            skill.retire(&*self.clock)?;
        }
        self.skills.save(&mut skill).await?;
        Ok(SkillDto::from(&skill))
    }
}
