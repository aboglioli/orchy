use crate::entities::Skill;
use crate::value_objects::Namespace;

pub struct SkillAggregate;

impl SkillAggregate {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

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
        let result = SkillAggregate::filter_with_inheritance(vec![], &ns);
        assert!(result.is_empty());
    }

    #[test]
    fn exact_namespace_match() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![make_skill("orchy", "test")];
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "test");
    }

    #[test]
    fn parent_namespace_sees_child_skills() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![make_skill("orchy/tasks", "test")];
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn child_namespace_inherits_parent_skills() {
        let ns = Namespace::try_from("orchy/tasks".to_string()).unwrap();
        let skills = vec![make_skill("orchy", "test")];
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filters_unrelated_namespace() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![make_skill("other", "test")];
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
        assert!(result.is_empty());
    }

    #[test]
    fn deduplicates_by_name_keeps_most_specific() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let skills = vec![
            make_skill("orchy", "test"),
            make_skill("orchy/tasks", "test"),
        ];
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
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
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
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
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].namespace.as_ref(), "orchy/tasks/backend");
    }

    #[test]
    fn nested_namespace_partial_match() {
        let ns = Namespace::try_from("orchy/tasks".to_string()).unwrap();
        let skills = vec![make_skill("orchy/tasks/processing", "test")];
        let result = SkillAggregate::filter_with_inheritance(skills, &ns);
        assert_eq!(result.len(), 1);
    }
}
