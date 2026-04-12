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
