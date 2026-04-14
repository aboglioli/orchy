use std::collections::HashMap;
use std::sync::Arc;

use super::{Entry, EntryFilter, EntryId, EntryStore, EntryType, Version, WriteEntry};
use crate::agent::AgentId;
use crate::embeddings::EmbeddingsProvider;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

pub struct KnowledgeService<S: EntryStore, E: EmbeddingsProvider> {
    store: Arc<S>,
    embeddings: Option<Arc<E>>,
}

impl<S: EntryStore, E: EmbeddingsProvider> KnowledgeService<S, E> {
    pub fn new(store: Arc<S>, embeddings: Option<Arc<E>>) -> Self {
        Self { store, embeddings }
    }

    pub async fn write(&self, cmd: WriteEntry) -> Result<Entry> {
        let existing = self
            .store
            .find_by_path(&cmd.project, &cmd.namespace, &cmd.path)
            .await?;

        let mut entry = if let Some(mut existing) = existing {
            if let Some(expected) = cmd.expected_version.filter(|v| existing.version() != *v) {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: existing.version().as_u64(),
                });
            }
            existing.update(cmd.title, cmd.content, cmd.agent_id);
            for tag in &cmd.tags {
                existing.add_tag(tag.clone());
            }
            for (k, v) in &cmd.metadata {
                existing.set_metadata(k.clone(), v.clone());
            }
            existing
        } else {
            if let Some(expected) = cmd.expected_version {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: 0,
                });
            }
            Entry::new(
                cmd.project,
                cmd.namespace,
                cmd.path,
                cmd.entry_type,
                cmd.title,
                cmd.content,
                cmd.tags,
                cmd.agent_id,
                cmd.metadata,
            )?
        };

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn read(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Entry>> {
        self.store.find_by_path(project, namespace, path).await
    }

    pub async fn get(&self, id: &EntryId) -> Result<Entry> {
        self.store
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("entry {id}")))
    }

    pub async fn list(&self, filter: EntryFilter) -> Result<Vec<Entry>> {
        self.store.list(filter).await
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Entry>> {
        let embedding = if let Some(emb) = &self.embeddings {
            Some(emb.embed(query).await?)
        } else {
            None
        };
        self.store
            .search(query, embedding.as_deref(), namespace, limit)
            .await
    }

    pub async fn delete(&self, id: &EntryId) -> Result<()> {
        let mut entry = self.get(id).await?;
        entry.mark_deleted();
        self.store.save(&mut entry).await?;
        self.store.delete(id).await
    }

    pub async fn move_entry(&self, id: &EntryId, new_namespace: Namespace) -> Result<Entry> {
        let mut entry = self.get(id).await?;
        entry.move_to(new_namespace);
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn rename(&self, id: &EntryId, new_path: String) -> Result<Entry> {
        let mut entry = self.get(id).await?;
        entry.rename(new_path)?;
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn tag(&self, id: &EntryId, tag: String) -> Result<Entry> {
        let mut entry = self.get(id).await?;
        entry.add_tag(tag);
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn untag(&self, id: &EntryId, tag: &str) -> Result<Entry> {
        let mut entry = self.get(id).await?;
        entry.remove_tag(tag);
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn append(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
        entry_type: EntryType,
        value: String,
        separator: &str,
        agent_id: Option<AgentId>,
    ) -> Result<Entry> {
        let existing = self.store.find_by_path(project, namespace, path).await?;

        let mut entry = if let Some(mut existing) = existing {
            let new_content = format!("{}{}{}", existing.content(), separator, value);
            existing.update(existing.title().to_string(), new_content, agent_id);
            existing
        } else {
            Entry::new(
                project.clone(),
                namespace.clone(),
                path.to_string(),
                entry_type,
                path.to_string(),
                value,
                vec![],
                agent_id,
                HashMap::new(),
            )?
        };

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn save_context(
        &self,
        project: ProjectId,
        agent_id: AgentId,
        namespace: Namespace,
        summary: String,
        metadata: HashMap<String, String>,
    ) -> Result<Entry> {
        let path = format!("context/{agent_id}");
        let cmd = WriteEntry {
            project,
            namespace,
            path,
            entry_type: EntryType::Context,
            title: "session context".into(),
            content: summary,
            tags: vec![],
            expected_version: None,
            agent_id: Some(agent_id),
            metadata,
        };
        self.write(cmd).await
    }

    pub async fn load_context(&self, agent_id: &AgentId) -> Result<Option<Entry>> {
        let filter = EntryFilter {
            entry_type: Some(EntryType::Context),
            agent_id: Some(*agent_id),
            ..Default::default()
        };
        let mut entries = self.store.list(filter).await?;
        entries.sort_by(|a, b| b.updated_at().cmp(&a.updated_at()));
        Ok(entries.into_iter().next())
    }

    pub async fn list_skills(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Entry>> {
        let filter = EntryFilter {
            project: Some(project.clone()),
            entry_type: Some(EntryType::Skill),
            ..Default::default()
        };
        let all = self.store.list(filter).await?;
        Ok(Self::filter_with_inheritance(all, namespace))
    }

    fn filter_with_inheritance(entries: Vec<Entry>, namespace: &Namespace) -> Vec<Entry> {
        let mut result: Vec<Entry> = Vec::new();

        for entry in entries {
            if entry.namespace().starts_with(namespace)
                || namespace.starts_with(entry.namespace())
            {
                if let Some(pos) = result.iter().position(|e| e.path() == entry.path()) {
                    if entry.namespace().as_ref().len() > result[pos].namespace().as_ref().len() {
                        result[pos] = entry;
                    }
                } else {
                    result.push(entry);
                }
            }
        }

        result.sort_by(|a, b| a.path().cmp(b.path()));
        result
    }
}
