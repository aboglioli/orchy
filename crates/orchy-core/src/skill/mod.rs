pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::{Namespace, Project};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub namespace: Namespace,
    pub name: String,
    pub description: String,
    pub content: String,
    pub written_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Skill {
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
    pub project: Option<Project>,
}

pub trait SkillStore: Send + Sync {
    async fn write(&self, skill: WriteSkill) -> Result<Skill>;
    async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>>;
    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>>;
    async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(namespace: &str, name: &str) -> Skill {
        Skill {
            namespace: Namespace::try_from(namespace.to_string()).unwrap(),
            name: name.to_string(),
            description: "test".to_string(),
            content: "content".to_string(),
            written_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
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
        assert_eq!(result[0].name, "test");
    }

    #[test]
    fn parent_namespace_sees_child_skills() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![make_skill("orchy/tasks", "test")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn child_namespace_inherits_parent_skills() {
        let ns = Namespace::try_from("orchy/tasks".to_string()).unwrap();
        let skills = vec![make_skill("orchy", "test")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filters_unrelated_namespace() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![make_skill("other", "test")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert!(result.is_empty());
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
        assert_eq!(result[0].name, "test");
        assert_eq!(result[0].namespace.as_ref(), "orchy/tasks");
    }

    #[test]
    fn sorts_by_name() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![
            make_skill("orchy", "zebra"),
            make_skill("orchy", "alpha"),
            make_skill("orchy", "beta"),
        ];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result[0].name, "alpha");
        assert_eq!(result[1].name, "beta");
        assert_eq!(result[2].name, "zebra");
    }

    #[test]
    fn same_name_different_namespaces_keeps_most_specific() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![
            make_skill("orchy/tasks", "test"),
            make_skill("orchy/tasks/backend", "test"),
        ];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].namespace.as_ref(), "orchy/tasks/backend");
    }

    #[test]
    fn nested_namespace_partial_match() {
        let ns = Namespace::try_from("orchy/tasks".to_string()).unwrap();
        let skills = vec![make_skill("orchy/tasks/processing", "test")];
        let result = Skill::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }
}
