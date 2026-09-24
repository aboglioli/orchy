use std::sync::Arc;

use orchy_core::{Clock, SkillStore, Tag};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;
use crate::read_skill::{ReadSkill, ReadSkillCommand};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetSkillFieldCommand {
    pub target: String,
    pub namespace: Option<String>,
    pub fields: Vec<(String, serde_json::Value)>,
    pub remove: Vec<String>,
    pub tag: Vec<String>,
    pub untag: Vec<String>,
}

pub struct SetSkillField {
    skills: Arc<dyn SkillStore>,
    read: ReadSkill,
    clock: Arc<dyn Clock>,
}

impl SetSkillField {
    pub fn new(skills: Arc<dyn SkillStore>, clock: Arc<dyn Clock>) -> Self {
        Self {
            read: ReadSkill::new(Arc::clone(&skills)),
            skills,
            clock,
        }
    }

    pub async fn execute(&self, cmd: SetSkillFieldCommand) -> ApplicationResult<SkillDto> {
        let found = self
            .read
            .execute(ReadSkillCommand {
                target: cmd.target,
                namespace: cmd.namespace,
            })
            .await?;
        let mut skill = self.skills.require(&found.id.parse()?).await?;

        for (field, value) in &cmd.fields {
            skill.set_field(field, value.clone(), &*self.clock)?;
        }
        for field in &cmd.remove {
            skill.remove_field(field, &*self.clock)?;
        }
        if !cmd.tag.is_empty() || !cmd.untag.is_empty() {
            let add: Vec<Tag> = cmd.tag.iter().map(Tag::new).collect::<Result<_, _>>()?;
            let remove: Vec<Tag> = cmd.untag.iter().map(Tag::new).collect::<Result<_, _>>()?;
            skill.tag(add, &remove, &*self.clock);
        }

        self.skills.save(&mut skill).await?;
        Ok(SkillDto::from(&skill))
    }
}
