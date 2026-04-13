pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::future::Future;

use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::{Namespace, ProjectId};

pub trait SkillStore: Send + Sync {
    fn save(&self, skill: &Skill) -> impl Future<Output = Result<()>> + Send;
    fn find_by_name(
        &self,
        namespace: &Namespace,
        name: &str,
    ) -> impl Future<Output = Result<Option<Skill>>> + Send;
    fn list(&self, filter: SkillFilter) -> impl Future<Output = Result<Vec<Skill>>> + Send;
    fn delete(&self, namespace: &Namespace, name: &str) -> impl Future<Output = Result<()>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    namespace: Namespace,
    name: String,
    description: String,
    content: String,
    written_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Skill {
    pub fn new(
        namespace: Namespace,
        name: String,
        description: String,
        content: String,
        written_by: Option<AgentId>,
    ) -> Self {
        let now = Utc::now();
        Self {
            namespace,
            name,
            description,
            content,
            written_by,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn restore(
        namespace: Namespace,
        name: String,
        description: String,
        content: String,
        written_by: Option<AgentId>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            namespace,
            name,
            description,
            content,
            written_by,
            created_at,
            updated_at,
        }
    }

    pub fn update(&mut self, description: String, content: String, written_by: Option<AgentId>) {
        self.description = description;
        self.content = content;
        if let Some(author) = written_by {
            self.written_by = Some(author);
        }
        self.updated_at = Utc::now();
    }

    pub fn move_to(&mut self, namespace: Namespace) {
        self.namespace = namespace;
        self.updated_at = Utc::now();
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
pub struct WriteSkill {
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

    fn make_skill(namespace: &str, name: &str) -> Skill {
        Skill::new(
            Namespace::try_from(namespace.to_string()).unwrap(),
            name.to_string(),
            "test".to_string(),
            "content".to_string(),
            None,
        )
    }

    #[test]
    fn empty_skills_returns_empty() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let result = Skill::filter_with_inheritance(vec![], &ns);
        assert!(result.is_empty());
    }

    #[test]
    fn exact_namespace_match() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![make_skill("orchy", "test")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn deduplicates_by_name_keeps_most_specific() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![
            make_skill("orchy", "test"),
            make_skill("orchy/tasks", "test"),
        ];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].namespace().as_ref(), "orchy/tasks");
    }

    #[test]
    fn sorts_by_name() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![make_skill("orchy", "zebra"), make_skill("orchy", "alpha")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result[0].name(), "alpha");
        assert_eq!(result[1].name(), "zebra");
    }
}
