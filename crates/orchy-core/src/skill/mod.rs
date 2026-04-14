pub mod events;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::future::Future;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

use self::events as skill_events;

pub trait SkillStore: Send + Sync {
    fn save(&self, skill: &Skill) -> impl Future<Output = Result<()>> + Send;
    fn find_by_name(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> impl Future<Output = Result<Option<Skill>>> + Send;
    fn list(&self, filter: SkillFilter) -> impl Future<Output = Result<Vec<Skill>>> + Send;
    fn delete(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    project: ProjectId,
    namespace: Namespace,
    name: String,
    description: String,
    content: String,
    written_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Skill {
    pub fn new(
        project: ProjectId,
        namespace: Namespace,
        name: String,
        description: String,
        content: String,
        written_by: Option<AgentId>,
    ) -> Result<Self> {
        if name.trim().is_empty() {
            return Err(Error::InvalidInput("skill name must not be empty".into()));
        }

        let now = Utc::now();
        let mut skill = Self {
            project,
            namespace,
            name,
            description,
            content,
            written_by,
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        skill.collector.collect(
            Event::create(
                skill.project.as_ref(),
                skill_events::NAMESPACE,
                skill_events::TOPIC_CREATED,
                Payload::from_json(&skill_events::SkillCreatedPayload {
                    project: skill.project.to_string(),
                    namespace: skill.namespace.to_string(),
                    name: skill.name.clone(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(skill)
    }

    pub fn restore(r: RestoreSkill) -> Self {
        Self {
            project: r.project,
            namespace: r.namespace,
            name: r.name,
            description: r.description,
            content: r.content,
            written_by: r.written_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn update(&mut self, description: String, content: String, written_by: Option<AgentId>) {
        self.description = description;
        self.content = content;
        if let Some(author) = written_by {
            self.written_by = Some(author);
        }
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            skill_events::NAMESPACE,
            skill_events::TOPIC_UPDATED,
            Payload::from_json(&skill_events::SkillUpdatedPayload {
                project: self.project.to_string(),
                namespace: self.namespace.to_string(),
                name: self.name.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn move_to(&mut self, namespace: Namespace) {
        let from_namespace = self.namespace.to_string();
        self.namespace = namespace;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            skill_events::NAMESPACE,
            skill_events::TOPIC_MOVED,
            Payload::from_json(&skill_events::SkillMovedPayload {
                project: self.project.to_string(),
                from_namespace,
                to_namespace: self.namespace.to_string(),
                name: self.name.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn filter_with_inheritance(skills: Vec<Skill>, namespace: &Namespace) -> Vec<Skill> {
        let mut result: Vec<Skill> = Vec::new();

        for skill in skills {
            if skill.namespace.starts_with(namespace) || namespace.starts_with(&skill.namespace) {
                if let Some(pos) = result.iter().position(|s| s.name == skill.name) {
                    if skill.namespace.as_ref().len() > result[pos].namespace.as_ref().len() {
                        result[pos] = skill;
                    }
                } else {
                    result.push(skill);
                }
            }
        }

        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn content(&self) -> &str {
        &self.content
    }
    pub fn written_by(&self) -> Option<AgentId> {
        self.written_by
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone)]
pub struct RestoreSkill {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub name: String,
    pub description: String,
    pub content: String,
    pub written_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WriteSkill {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub name: String,
    pub description: String,
    pub content: String,
    pub written_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillFilter {
    pub namespace: Option<Namespace>,
    pub project: Option<ProjectId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    fn make_skill(namespace: &str, name: &str) -> Skill {
        Skill::new(
            test_project(),
            Namespace::try_from(namespace).unwrap(),
            name.to_string(),
            "test".to_string(),
            "content".to_string(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn empty_skills_returns_empty() {
        let ns = Namespace::try_from("/orchy").unwrap();
        let result = Skill::filter_with_inheritance(vec![], &ns);
        assert!(result.is_empty());
    }

    #[test]
    fn exact_namespace_match() {
        let ns = Namespace::try_from("/orchy").unwrap();
        let skills = vec![make_skill("/orchy", "test")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn deduplicates_by_name_keeps_most_specific() {
        let ns = Namespace::try_from("/orchy").unwrap();
        let skills = vec![
            make_skill("/orchy", "test"),
            make_skill("/orchy/tasks", "test"),
        ];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].namespace().as_ref(), "/orchy/tasks");
    }

    #[test]
    fn sorts_by_name() {
        let ns = Namespace::try_from("/orchy").unwrap();
        let skills = vec![make_skill("/orchy", "zebra"), make_skill("/orchy", "alpha")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result[0].name(), "alpha");
        assert_eq!(result[1].name(), "zebra");
    }
}
