use std::collections::HashSet;
use std::sync::Arc;

use super::{Skill, SkillFilter, SkillStore, WriteSkill};
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

pub struct SkillService<S: SkillStore> {
    store: Arc<S>,
}

impl<S: SkillStore> SkillService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn write(&self, cmd: WriteSkill) -> Result<Skill> {
        if cmd.name.is_empty() {
            return Err(Error::InvalidInput(
                "skill name must not be empty".to_string(),
            ));
        }

        let mut skill = if let Some(mut existing) = self
            .store
            .find_by_name(&cmd.project, &cmd.namespace, &cmd.name)
            .await?
        {
            existing.update(cmd.description, cmd.content, cmd.written_by);
            existing
        } else {
            Skill::new(
                cmd.project,
                cmd.namespace,
                cmd.name,
                cmd.description,
                cmd.content,
                cmd.written_by,
            )?
        };

        self.store.save(&mut skill).await?;
        Ok(skill)
    }

    pub async fn read(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<Skill>> {
        self.store.find_by_name(project, namespace, name).await
    }

    pub async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        self.store.list(filter).await
    }

    pub async fn list_with_inherited(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Skill>> {
        let all = self
            .store
            .list(SkillFilter {
                project: Some(project.clone()),
                ..Default::default()
            })
            .await?;

        Ok(Skill::filter_with_inheritance(all, namespace))
    }

    pub async fn list_with_inherited_and_linked(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        linked_projects: &[ProjectId],
    ) -> Result<Vec<Skill>> {
        let mut result = self.list_with_inherited(project, namespace).await?;
        let local_names: HashSet<String> = result.iter().map(|s| s.name().to_string()).collect();

        for linked in linked_projects {
            let linked_skills = self
                .store
                .list(SkillFilter {
                    project: Some(linked.clone()),
                    ..Default::default()
                })
                .await?;

            for skill in linked_skills {
                if !local_names.contains(skill.name()) {
                    result.push(skill);
                }
            }
        }

        result.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(result)
    }

    pub async fn move_skill(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
        new_namespace: Namespace,
    ) -> Result<Skill> {
        let mut skill = self
            .store
            .find_by_name(project, namespace, name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("skill {namespace}/{name}")))?;

        let old_namespace = skill.namespace().clone();
        let old_name = skill.name().to_string();
        skill.move_to(new_namespace);
        self.store.save(&mut skill).await?;
        self.store
            .delete(project, &old_namespace, &old_name)
            .await?;
        Ok(skill)
    }

    pub async fn delete(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        if let Some(mut skill) = self.store.find_by_name(project, namespace, name).await? {
            skill.mark_deleted();
            self.store.save(&mut skill).await?;
        }
        self.store.delete(project, namespace, name).await
    }
}
