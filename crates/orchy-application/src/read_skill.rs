use std::sync::Arc;

use orchy_core::{
    ActorId, ActorStore, DomainError, Id, Namespace, Skill, SkillName, SkillStore, skill,
};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadSkillCommand {
    pub target: String,
    pub namespace: Option<String>,
    pub actor: Option<String>,
}

/// A name resolves where the actor works, so it finds the same skill the briefing shows.
pub struct ReadSkill {
    skills: Arc<dyn SkillStore>,
    actors: Arc<dyn ActorStore>,
}

impl ReadSkill {
    pub fn new(skills: Arc<dyn SkillStore>, actors: Arc<dyn ActorStore>) -> Self {
        Self { skills, actors }
    }

    pub async fn execute(&self, cmd: ReadSkillCommand) -> ApplicationResult<SkillDto> {
        let namespace = match (&cmd.namespace, &cmd.actor) {
            (Some(namespace), _) => Namespace::new(namespace)?,
            (None, Some(actor)) => self.actors.home_of(&actor.parse::<ActorId>()?).await?,
            (None, None) => Namespace::root(),
        };

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
