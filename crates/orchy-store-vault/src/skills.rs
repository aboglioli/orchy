use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{EntityKind, EventLog, Id, Result, Skill, SkillStore};

use crate::codec;
use crate::vault::{Precondition, Vault};

pub struct VaultSkillStore {
    vault: Arc<Vault>,
    log: Arc<dyn EventLog>,
}

impl VaultSkillStore {
    pub fn new(vault: Arc<Vault>, log: Arc<dyn EventLog>) -> Self {
        Self { vault, log }
    }
}

#[async_trait]
impl SkillStore for VaultSkillStore {
    async fn get(&self, id: &Id) -> Result<Option<Skill>> {
        let Some((key, file)) = self.vault.read_by_id(id).await? else {
            return Ok(None);
        };
        if codec::kind_of(&file) != Some("skill") {
            return Ok(None);
        }
        codec::skill_from_markdown(&file, &key).map(Some)
    }

    async fn all(&self) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();
        for (key, file) in self.vault.load_all(EntityKind::Skill).await? {
            skills.push(codec::skill_from_markdown(&file, &key)?);
        }
        skills.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));
        Ok(skills)
    }

    async fn save(&self, skill: &mut Skill) -> Result<()> {
        let events = skill.drain_events();
        let key = self
            .vault
            .layout()
            .skill_key(skill.namespace(), skill.name());
        let file = codec::skill_to_markdown(skill);
        self.vault
            .write_if(
                &key,
                &file,
                skill.id(),
                EntityKind::Skill,
                Precondition::Unchanged,
            )
            .await?;
        self.log.append(&events).await
    }

    async fn delete(&self, id: &Id) -> Result<()> {
        self.vault.remove(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::{BlobStore, MemoryBlobStore};
    use orchy_core::{Body, Namespace, SkillName, Summary};
    use orchy_store_memory::{FixedClock, MemoryEventLog, SeqIdGenerator};

    async fn store() -> (VaultSkillStore, Arc<Vault>) {
        let blobs = Arc::new(MemoryBlobStore::new()) as Arc<dyn BlobStore>;
        let vault = Arc::new(Vault::open(blobs).await.unwrap());
        let log = Arc::new(MemoryEventLog::new()) as Arc<dyn EventLog>;
        (VaultSkillStore::new(Arc::clone(&vault), log), vault)
    }

    fn skill(name: &str, namespace: &str) -> Skill {
        Skill::create(
            SkillName::new(name).unwrap(),
            Summary::new("how we do it here").unwrap(),
            Namespace::new(namespace).unwrap(),
            Body::new("## When to use\n\nalways"),
            &SeqIdGenerator::new(),
            &FixedClock::at(1_700_000_000),
        )
    }

    #[tokio::test]
    async fn a_skill_is_filed_under_its_namespace_by_name() {
        let (skills, vault) = store().await;
        let mut written = skill("code-review", "/backend/auth");
        skills.save(&mut written).await.unwrap();

        assert_eq!(
            vault.locate(written.id()).unwrap().key,
            "skills/backend/auth/code-review.md",
            "the path a human would look in"
        );
    }

    #[tokio::test]
    async fn renaming_refiles_it_and_leaves_no_copy_behind() {
        let (skills, vault) = store().await;
        let mut written = skill("old-name", "/");
        skills.save(&mut written).await.unwrap();

        written.rename(
            SkillName::new("new-name").unwrap(),
            &FixedClock::at(1_700_000_000),
        );
        skills.save(&mut written).await.unwrap();

        assert_eq!(
            vault.locate(written.id()).unwrap().key,
            "skills/new-name.md"
        );
        assert_eq!(
            skills.all().await.unwrap().len(),
            1,
            "a rename moves the skill, it does not clone it"
        );
    }

    #[tokio::test]
    async fn a_skill_round_trips_through_markdown() {
        let (skills, _) = store().await;
        let mut written = skill("migrations", "/backend");
        skills.save(&mut written).await.unwrap();

        let read = skills.require(written.id()).await.unwrap();
        assert_eq!(read.name(), written.name());
        assert_eq!(read.summary(), written.summary());
        assert_eq!(read.namespace(), written.namespace());
        assert_eq!(read.body().as_str(), written.body().as_str());
    }

    #[tokio::test]
    async fn a_document_is_not_mistaken_for_a_skill() {
        let (skills, vault) = store().await;
        let id = Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let mut frontmatter = orchy_core::Frontmatter::new();
        frontmatter.set("id", serde_json::json!(id.to_string()));
        frontmatter.set("type", serde_json::json!("note"));
        vault
            .write(
                "docs/a.md",
                &crate::markdown::MarkdownFile {
                    frontmatter,
                    body: Body::new("x"),
                },
                &id,
                EntityKind::Document,
            )
            .await
            .unwrap();

        assert!(skills.get(&id).await.unwrap().is_none());
    }
}
