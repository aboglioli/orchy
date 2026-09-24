use std::sync::Arc;

use orchy_core::{
    Body, Clock, DomainError, IdGenerator, Namespace, Skill, SkillName, SkillStore, Summary,
};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WriteSkillCommand {
    pub name: String,
    pub summary: Option<String>,
    pub namespace: Option<String>,
    pub body: Option<String>,
}

/// Creating and revising a skill are one command, because an agent writing down what it just
/// learned should not have to know whether the vault already says it.
pub struct WriteSkill {
    skills: Arc<dyn SkillStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl WriteSkill {
    pub fn new(
        skills: Arc<dyn SkillStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self { skills, ids, clock }
    }

    pub async fn execute(&self, cmd: WriteSkillCommand) -> ApplicationResult<SkillDto> {
        let name = SkillName::new(&cmd.name)?;
        let namespace = cmd
            .namespace
            .as_deref()
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default();

        let existing = self
            .skills
            .all()
            .await?
            .into_iter()
            .find(|s| s.name() == &name && s.namespace() == &namespace);

        let mut skill = match existing {
            Some(mut found) => {
                if let Some(summary) = &cmd.summary {
                    found.describe(Summary::new(summary)?, &*self.clock);
                }
                if let Some(body) = &cmd.body {
                    found.edit(Body::new(body), &*self.clock);
                }
                found
            }
            None => {
                let summary = cmd.summary.as_deref().ok_or_else(|| {
                    DomainError::validation(
                        "a new skill needs --summary: it is the line every agent reads first",
                    )
                })?;
                Skill::create(
                    name,
                    Summary::new(summary)?,
                    namespace,
                    Body::new(cmd.body.as_deref().unwrap_or_default()),
                    &*self.ids,
                    &*self.clock,
                )
            }
        };

        self.skills.save(&mut skill).await?;
        Ok(SkillDto::from(&skill))
    }
}
