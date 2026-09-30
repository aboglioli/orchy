use std::sync::Arc;

use orchy_core::{ActorId, ActorStore, Namespace, SkillStore, Tag, skill};
use serde::{Deserialize, Serialize};

use crate::dto::SkillDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListSkillsCommand {
    pub namespace: Option<String>,
    pub tags: Vec<String>,
    pub everywhere: bool,
    pub retired: bool,
    pub actor: Option<String>,
}

pub struct ListSkills {
    skills: Arc<dyn SkillStore>,
    actors: Arc<dyn ActorStore>,
}

impl ListSkills {
    pub fn new(skills: Arc<dyn SkillStore>, actors: Arc<dyn ActorStore>) -> Self {
        Self { skills, actors }
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

        let namespace = match (&cmd.namespace, &cmd.actor) {
            (Some(namespace), _) => Namespace::new(namespace)?,
            (None, Some(actor)) => self.actors.home_of(&actor.parse::<ActorId>()?).await?,
            (None, None) => Namespace::root(),
        };
        Ok(skill::in_scope(&all, &namespace)
            .iter()
            .map(SkillDto::from)
            .collect())
    }
}
