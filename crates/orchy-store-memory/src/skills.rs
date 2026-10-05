use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchy_core::{EventLog, Id, Result, Skill, SkillStore};

use crate::eventlog::MemoryEventLog;

pub struct MemorySkillStore {
    skills: Mutex<BTreeMap<Id, Skill>>,
    log: Arc<MemoryEventLog>,
}

impl MemorySkillStore {
    pub(crate) fn state(&self) -> BTreeMap<Id, Skill> {
        self.skills.lock().expect("skills lock").clone()
    }

    pub(crate) fn restore_state(&self, state: BTreeMap<Id, Skill>) {
        *self.skills.lock().expect("skills lock") = state;
    }

    pub fn new(log: Arc<MemoryEventLog>) -> Self {
        Self {
            skills: Mutex::new(BTreeMap::new()),
            log,
        }
    }
}

#[async_trait]
impl SkillStore for MemorySkillStore {
    async fn get(&self, id: &Id) -> Result<Option<Skill>> {
        Ok(self.skills.lock().expect("skill mutex").get(id).cloned())
    }

    async fn all(&self) -> Result<Vec<Skill>> {
        let mut skills: Vec<Skill> = self
            .skills
            .lock()
            .expect("skill mutex")
            .values()
            .cloned()
            .collect();
        skills.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));
        Ok(skills)
    }

    async fn save(&self, skill: &mut Skill) -> Result<()> {
        let events = skill.drain_events();
        self.skills
            .lock()
            .expect("skill mutex")
            .insert(skill.id().clone(), skill.clone());
        self.log.append(&events).await
    }
}
