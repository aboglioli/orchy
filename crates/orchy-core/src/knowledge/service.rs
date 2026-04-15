use std::collections::HashMap;
use std::sync::Arc;

use super::{
    Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeKind, KnowledgeStore, Version, WriteKnowledge,
};
use crate::agent::AgentId;
use crate::embeddings::EmbeddingsProvider;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

pub struct KnowledgeService<S: KnowledgeStore, E: EmbeddingsProvider> {
    store: Arc<S>,
    embeddings: Option<Arc<E>>,
}

impl<S: KnowledgeStore, E: EmbeddingsProvider> KnowledgeService<S, E> {
    pub fn new(store: Arc<S>, embeddings: Option<Arc<E>>) -> Self {
        Self { store, embeddings }
    }

    fn apply_metadata_updates(
        entry: &mut Knowledge,
        set: Option<HashMap<String, String>>,
        remove_keys: Option<Vec<String>>,
    ) {
        if let Some(keys) = remove_keys {
            for k in keys {
                let _ = entry.remove_metadata(&k);
            }
        }
        if let Some(map) = set {
            for (k, v) in map {
                entry.set_metadata(k, v);
            }
        }
    }

    pub async fn write(&self, cmd: WriteKnowledge) -> Result<Knowledge> {
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
            // `kind` applies only when creating; updates keep the existing kind (use change_kind).
            existing.update(cmd.title, cmd.content, cmd.agent_id);
            for tag in &cmd.tags {
                existing.add_tag(tag.clone());
            }
            for k in &cmd.metadata_remove {
                let _ = existing.remove_metadata(k);
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
            let mut created = Knowledge::new(
                cmd.project,
                cmd.namespace,
                cmd.path,
                cmd.kind,
                cmd.title,
                cmd.content,
                cmd.tags,
                cmd.agent_id,
                cmd.metadata,
            )?;
            for k in &cmd.metadata_remove {
                let _ = created.remove_metadata(k);
            }
            created
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
    ) -> Result<Option<Knowledge>> {
        self.store.find_by_path(project, namespace, path).await
    }

    pub async fn get(&self, id: &KnowledgeId) -> Result<Knowledge> {
        self.store
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("entry {id}")))
    }

    pub async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<Knowledge>> {
        self.store.list(filter).await
    }

    pub async fn patch_metadata(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
        set: HashMap<String, String>,
        remove: Vec<String>,
        expected_version: Option<Version>,
    ) -> Result<Knowledge> {
        let mut entry = self
            .store
            .find_by_path(project, namespace, path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge entry not found: {path}")))?;

        if let Some(expected) = expected_version
            && entry.version() != expected
        {
            return Err(Error::VersionMismatch {
                expected: expected.as_u64(),
                actual: entry.version().as_u64(),
            });
        }

        if set.is_empty() && remove.is_empty() {
            return Ok(entry);
        }

        Self::apply_metadata_updates(
            &mut entry,
            Some(set).filter(|m| !m.is_empty()),
            Some(remove).filter(|v| !v.is_empty()),
        );

        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Knowledge>> {
        let embedding = if let Some(emb) = &self.embeddings {
            Some(emb.embed(query).await?)
        } else {
            None
        };
        self.store
            .search(query, embedding.as_deref(), namespace, limit)
            .await
    }

    pub async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        let mut entry = self.get(id).await?;
        entry.mark_deleted();
        self.store.save(&mut entry).await?;
        self.store.delete(id).await
    }

    pub async fn move_entry(
        &self,
        id: &KnowledgeId,
        new_namespace: Namespace,
        metadata: Option<HashMap<String, String>>,
        metadata_remove: Option<Vec<String>>,
    ) -> Result<Knowledge> {
        let mut entry = self.get(id).await?;
        Self::apply_metadata_updates(&mut entry, metadata, metadata_remove);
        entry.move_to(new_namespace);
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn rename(
        &self,
        id: &KnowledgeId,
        new_path: String,
        metadata: Option<HashMap<String, String>>,
        metadata_remove: Option<Vec<String>>,
    ) -> Result<Knowledge> {
        let mut entry = self.get(id).await?;
        Self::apply_metadata_updates(&mut entry, metadata, metadata_remove);
        entry.rename(new_path)?;
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn change_kind(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
        new_kind: KnowledgeKind,
        expected_version: Option<Version>,
        metadata: Option<HashMap<String, String>>,
        metadata_remove: Option<Vec<String>>,
    ) -> Result<Knowledge> {
        let mut entry = self
            .store
            .find_by_path(project, namespace, path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge entry not found: {path}")))?;

        if let Some(expected) = expected_version
            && entry.version() != expected
        {
            return Err(Error::VersionMismatch {
                expected: expected.as_u64(),
                actual: entry.version().as_u64(),
            });
        }

        Self::apply_metadata_updates(&mut entry, metadata, metadata_remove);

        if entry.kind() == new_kind {
            self.store.save(&mut entry).await?;
            return Ok(entry);
        }

        entry.change_kind(new_kind);

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn tag(
        &self,
        id: &KnowledgeId,
        tag: String,
        metadata: Option<HashMap<String, String>>,
        metadata_remove: Option<Vec<String>>,
    ) -> Result<Knowledge> {
        let mut entry = self.get(id).await?;
        Self::apply_metadata_updates(&mut entry, metadata, metadata_remove);
        entry.add_tag(tag);
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn untag(
        &self,
        id: &KnowledgeId,
        tag: &str,
        metadata: Option<HashMap<String, String>>,
        metadata_remove: Option<Vec<String>>,
    ) -> Result<Knowledge> {
        let mut entry = self.get(id).await?;
        Self::apply_metadata_updates(&mut entry, metadata, metadata_remove);
        entry.remove_tag(tag);
        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn append(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
        kind: KnowledgeKind,
        value: String,
        separator: &str,
        agent_id: Option<AgentId>,
        metadata: Option<HashMap<String, String>>,
        metadata_remove: Option<Vec<String>>,
    ) -> Result<Knowledge> {
        let existing = self.store.find_by_path(project, namespace, path).await?;

        let mut entry = if let Some(mut existing) = existing {
            let new_content = format!("{}{}{}", existing.content(), separator, value);
            existing.update(existing.title().to_string(), new_content, agent_id);
            existing
        } else {
            Knowledge::new(
                project.clone(),
                namespace.clone(),
                path.to_string(),
                kind,
                path.to_string(),
                value,
                vec![],
                agent_id,
                HashMap::new(),
            )?
        };

        Self::apply_metadata_updates(&mut entry, metadata, metadata_remove);

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&mut entry).await?;
        Ok(entry)
    }

    pub async fn list_skills(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Knowledge>> {
        let filter = KnowledgeFilter {
            project: Some(project.clone()),
            kind: Some(KnowledgeKind::Skill),
            ..Default::default()
        };
        let all = self.store.list(filter).await?;
        Ok(Self::filter_with_inheritance(all, namespace))
    }

    pub async fn list_overviews(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Knowledge>> {
        let filter = KnowledgeFilter {
            project: Some(project.clone()),
            kind: Some(KnowledgeKind::Overview),
            ..Default::default()
        };
        let all = self.store.list(filter).await?;
        Ok(Self::filter_with_inheritance(all, namespace))
    }

    fn filter_with_inheritance(entries: Vec<Knowledge>, namespace: &Namespace) -> Vec<Knowledge> {
        let mut result: Vec<Knowledge> = Vec::new();

        for entry in entries {
            if entry.namespace().starts_with(namespace) || namespace.starts_with(entry.namespace())
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
