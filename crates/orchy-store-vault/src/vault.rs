use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use orchy_core::{DomainError, EntityKind, Id, Result};
use tokio::time::sleep;

use crate::blob::{BlobStore, digest};
use crate::codec;
use crate::layout::Layout;
use crate::markdown::MarkdownFile;

const LOOKUP_ATTEMPTS: u32 = 4;
const RESCAN_PASSES: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precondition {
    Any,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub key: String,
    pub kind: EntityKind,
    pub seen: u64,
}

pub struct Vault {
    blobs: Arc<dyn BlobStore>,
    layout: Layout,
    index: RwLock<HashMap<Id, Located>>,
}

impl Vault {
    pub async fn open(blobs: Arc<dyn BlobStore>) -> Result<Self> {
        let vault = Self {
            blobs,
            layout: Layout,
            index: RwLock::new(HashMap::new()),
        };
        vault.reindex().await?;
        Ok(vault)
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn blobs(&self) -> &Arc<dyn BlobStore> {
        &self.blobs
    }

    pub async fn reindex(&self) -> Result<()> {
        let scanned = self.scan().await?;
        self.absorb(&scanned);
        Ok(())
    }

    async fn scan(&self) -> Result<Vec<(Id, Located, MarkdownFile)>> {
        let mut found = Vec::new();
        let mut seen_keys = HashSet::new();
        let mut batch = self.blobs.list("").await?;

        for _ in 0..RESCAN_PASSES {
            for key in batch {
                if !seen_keys.insert(key.clone()) {
                    continue;
                }
                if !self.layout.is_markdown(&key) || self.layout.is_runtime(&key) {
                    continue;
                }
                let Some(bytes) = self.blobs.get(&key).await? else {
                    continue;
                };
                let seen = digest(&bytes);
                let Some(file) = parse(&bytes, &key)? else {
                    continue;
                };
                let Some(raw) = codec::id_of(&file) else {
                    continue;
                };
                let Ok(id) = Id::new(raw) else {
                    continue;
                };
                let kind = kind_from(codec::kind_of(&file));
                found.push((id, Located { kind, key, seen }, file));
            }

            batch = self
                .blobs
                .list("")
                .await?
                .into_iter()
                .filter(|key| !seen_keys.contains(key))
                .collect();
            if batch.is_empty() {
                break;
            }
        }
        Ok(found)
    }

    fn absorb(&self, scanned: &[(Id, Located, MarkdownFile)]) {
        let mut index = self.index.write().expect("index lock");
        let mut refreshed: HashMap<Id, Located> = HashMap::new();
        for (id, located, _) in scanned {
            let mut located = located.clone();
            if let Some(previous) = index.get(id) {
                located.seen = previous.seen;
            }
            refreshed.insert(id.clone(), located);
        }
        for (id, located) in index.iter() {
            refreshed
                .entry(id.clone())
                .or_insert_with(|| located.clone());
        }
        *index = refreshed;
    }

    pub fn locate(&self, id: &Id) -> Option<Located> {
        self.index.read().expect("index lock").get(id).cloned()
    }

    pub fn ids_of(&self, kind: EntityKind) -> Vec<Id> {
        self.index
            .read()
            .expect("index lock")
            .iter()
            .filter(|(_, located)| located.kind == kind)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub async fn read(&self, key: &str) -> Result<Option<MarkdownFile>> {
        let Some(bytes) = self.blobs.get(key).await? else {
            return Ok(None);
        };
        let text = String::from_utf8(bytes)
            .map_err(|_| DomainError::validation(format!("`{key}` is not valid UTF-8")))?;
        MarkdownFile::parse(&text).map(Some)
    }

    pub async fn read_by_id(&self, id: &Id) -> Result<Option<(String, MarkdownFile)>> {
        let Some((located, bytes)) = self.bytes_of(id).await? else {
            return Ok(None);
        };
        self.index.write().expect("index lock").insert(
            id.clone(),
            Located {
                seen: digest(&bytes),
                ..located.clone()
            },
        );
        Ok(parse(&bytes, &located.key)?.map(|file| (located.key, file)))
    }

    pub async fn peek_by_id(&self, id: &Id) -> Result<Option<(String, MarkdownFile)>> {
        let Some((located, bytes)) = self.bytes_of(id).await? else {
            return Ok(None);
        };
        Ok(parse(&bytes, &located.key)?.map(|file| (located.key, file)))
    }

    async fn bytes_of(&self, id: &Id) -> Result<Option<(Located, Vec<u8>)>> {
        for attempt in 0..LOOKUP_ATTEMPTS {
            let Some(located) = self.locate(id) else {
                return Ok(None);
            };
            if let Some(bytes) = self.blobs.get(&located.key).await? {
                return Ok(Some((located, bytes)));
            }
            if attempt > 0 {
                sleep(Duration::from_millis(2 * u64::from(attempt))).await;
            }
            self.reindex().await?;
        }
        self.index.write().expect("index lock").remove(id);
        Ok(None)
    }

    pub async fn write(
        &self,
        key: &str,
        file: &MarkdownFile,
        id: &Id,
        kind: EntityKind,
    ) -> Result<()> {
        self.write_if(key, file, id, kind, Precondition::Any).await
    }

    pub async fn write_if(
        &self,
        key: &str,
        file: &MarkdownFile,
        id: &Id,
        kind: EntityKind,
        precondition: Precondition,
    ) -> Result<()> {
        let rendered = file.render()?;
        let previous = self.locate(id);

        match (precondition, &previous) {
            (Precondition::Unchanged, Some(located)) => {
                self.take(located, key, &rendered, id).await?;
            }
            (Precondition::Unchanged, None) => {
                if !self
                    .blobs
                    .compare_and_put(key, None, rendered.as_bytes())
                    .await?
                {
                    return Err(DomainError::conflict(format!(
                        "`{id}` already exists at `{key}`; reload it and reapply the change"
                    )));
                }
            }
            _ => {
                if let Some(located) = &previous
                    && located.key != key
                {
                    self.blobs.delete(&located.key).await?;
                }
                self.blobs.put(key, rendered.as_bytes()).await?;
            }
        }

        self.index.write().expect("index lock").insert(
            id.clone(),
            Located {
                key: key.to_owned(),
                kind,
                seen: digest(rendered.as_bytes()),
            },
        );
        Ok(())
    }

    async fn take(&self, located: &Located, key: &str, rendered: &str, id: &Id) -> Result<()> {
        let taken = self
            .blobs
            .compare_and_put(&located.key, Some(located.seen), rendered.as_bytes())
            .await?;
        if !taken {
            return Err(if self.blobs.exists(&located.key).await? {
                DomainError::conflict(format!(
                    "`{id}` changed since it was read; reload it and reapply the change"
                ))
            } else {
                DomainError::conflict(format!("`{id}` was deleted since it was read"))
            });
        }
        if located.key != key {
            self.blobs.put(key, rendered.as_bytes()).await?;
            self.blobs.delete(&located.key).await?;
        }
        Ok(())
    }

    pub async fn remove(&self, id: &Id) -> Result<()> {
        let Some(located) = self.locate(id) else {
            return Ok(());
        };
        self.blobs.delete(&located.key).await?;
        self.index.write().expect("index lock").remove(id);
        Ok(())
    }

    pub async fn load_all(&self, kind: EntityKind) -> Result<Vec<(String, MarkdownFile)>> {
        let scanned = self.scan().await?;
        self.absorb(&scanned);

        let mut loaded: Vec<(String, MarkdownFile)> = scanned
            .into_iter()
            .filter(|(_, located, _)| located.kind == kind)
            .map(|(_, located, file)| (located.key, file))
            .collect();
        loaded.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(loaded)
    }
}

fn parse(bytes: &[u8], key: &str) -> Result<Option<MarkdownFile>> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(DomainError::validation(format!(
            "`{key}` is not valid UTF-8"
        )));
    };
    MarkdownFile::parse(text).map(Some)
}

fn kind_from(declared: Option<&str>) -> EntityKind {
    match declared {
        Some("task") => EntityKind::Task,
        Some("message") => EntityKind::Message,
        Some("skill") => EntityKind::Skill,
        Some("agent") => EntityKind::Actor,
        _ => EntityKind::Document,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::MemoryBlobStore;
    use orchy_core::Frontmatter;
    use serde_json::json;

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    fn file(id: &str, kind: &str) -> MarkdownFile {
        let mut frontmatter = Frontmatter::new();
        frontmatter.set("id", json!(id));
        frontmatter.set("type", json!(kind));
        MarkdownFile {
            frontmatter,
            body: orchy_core::Body::new("body"),
        }
    }

    async fn vault() -> (Vault, Arc<MemoryBlobStore>) {
        let blobs = Arc::new(MemoryBlobStore::new());
        let vault = Vault::open(Arc::clone(&blobs) as Arc<dyn BlobStore>)
            .await
            .unwrap();
        (vault, blobs)
    }

    #[tokio::test]
    async fn an_entity_is_found_by_id_regardless_of_where_it_sits() {
        let (vault, _) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write(
                "anywhere/at/all.md",
                &file(A, "decision"),
                &id,
                EntityKind::Document,
            )
            .await
            .unwrap();

        let (key, found) = vault.read_by_id(&id).await.unwrap().unwrap();
        assert_eq!(key, "anywhere/at/all.md");
        assert_eq!(codec::id_of(&found), Some(A));
    }

    #[tokio::test]
    async fn moving_a_file_does_not_leave_the_old_copy_behind() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("tasks/open/x.md", &file(A, "task"), &id, EntityKind::Task)
            .await
            .unwrap();
        vault
            .write("tasks/done/x.md", &file(A, "task"), &id, EntityKind::Task)
            .await
            .unwrap();

        assert!(blobs.get("tasks/open/x.md").await.unwrap().is_none());
        assert!(blobs.get("tasks/done/x.md").await.unwrap().is_some());
        assert_eq!(vault.locate(&id).unwrap().key, "tasks/done/x.md");
    }

    #[tokio::test]
    async fn the_index_survives_a_reopen_by_rescanning() {
        let blobs = Arc::new(MemoryBlobStore::new());
        {
            let vault = Vault::open(Arc::clone(&blobs) as Arc<dyn BlobStore>)
                .await
                .unwrap();
            vault
                .write(
                    "notes/a.md",
                    &file(A, "note"),
                    &Id::new(A).unwrap(),
                    EntityKind::Document,
                )
                .await
                .unwrap();
        }
        let reopened = Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap();
        assert!(reopened.locate(&Id::new(A).unwrap()).is_some());
    }

    #[tokio::test]
    async fn entities_are_grouped_by_the_kind_declared_in_frontmatter() {
        let (vault, _) = vault().await;
        vault
            .write(
                "tasks/open/a.md",
                &file(A, "task"),
                &Id::new(A).unwrap(),
                EntityKind::Task,
            )
            .await
            .unwrap();
        vault
            .write(
                "notes/b.md",
                &file(B, "note"),
                &Id::new(B).unwrap(),
                EntityKind::Document,
            )
            .await
            .unwrap();

        assert_eq!(vault.ids_of(EntityKind::Task), vec![Id::new(A).unwrap()]);
        assert_eq!(
            vault.ids_of(EntityKind::Document),
            vec![Id::new(B).unwrap()]
        );
    }

    #[tokio::test]
    async fn a_file_without_an_id_is_skipped_rather_than_failing_the_scan() {
        let blobs = Arc::new(MemoryBlobStore::new());
        blobs.put("README.md", b"# Just prose\n").await.unwrap();
        blobs
            .put("half.md", b"---\ntype: note\n---\n\nno id\n")
            .await
            .unwrap();

        let vault = Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap();
        assert!(vault.ids_of(EntityKind::Document).is_empty());
    }

    #[tokio::test]
    async fn runtime_files_are_never_indexed() {
        let blobs = Arc::new(MemoryBlobStore::new());
        blobs
            .put(
                ".orchy/scratch.md",
                format!("---\nid: {A}\ntype: note\n---\n\nx\n").as_bytes(),
            )
            .await
            .unwrap();
        let vault = Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap();
        assert!(vault.locate(&Id::new(A).unwrap()).is_none());
    }

    #[tokio::test]
    async fn a_write_is_refused_when_the_file_moved_on_since_it_was_read() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();
        vault.read_by_id(&id).await.unwrap();

        blobs
            .put("notes/a.md", b"---\nid: x\n---\n\nsomeone else\n")
            .await
            .unwrap();

        let refused = vault
            .write_if(
                "notes/a.md",
                &file(A, "note"),
                &id,
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await;
        assert!(
            matches!(refused, Err(DomainError::Conflict(_))),
            "the later writer is told, not quietly preferred: {refused:?}"
        );
    }

    #[tokio::test]
    async fn a_peek_leaves_the_precondition_where_the_last_real_read_put_it() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();
        vault.read_by_id(&id).await.unwrap();

        blobs
            .put("notes/a.md", b"---\nid: x\n---\n\nsomeone else\n")
            .await
            .unwrap();
        vault.peek_by_id(&id).await.unwrap();

        let refused = vault
            .write_if(
                "notes/a.md",
                &file(A, "note"),
                &id,
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await;
        assert!(
            matches!(refused, Err(DomainError::Conflict(_))),
            "a peek taken on the way to a write must not vouch for the bytes: {refused:?}"
        );
    }

    #[tokio::test]
    async fn an_entity_this_process_never_saw_must_not_already_be_on_disk() {
        let (vault, blobs) = vault().await;
        blobs.put("notes/a.md", b"already here").await.unwrap();

        let refused = vault
            .write_if(
                "notes/a.md",
                &file(A, "note"),
                &Id::new(A).unwrap(),
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await;
        assert!(
            matches!(refused, Err(DomainError::Conflict(_))),
            "creating claims the key rather than overwriting it: {refused:?}"
        );
    }

    #[tokio::test]
    async fn a_rescan_relocates_an_entity_without_forgetting_what_was_read_of_it() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("tasks/open/x.md", &file(A, "task"), &id, EntityKind::Task)
            .await
            .unwrap();
        vault.read_by_id(&id).await.unwrap();
        let seen = vault.locate(&id).unwrap().seen;

        let bytes = blobs.get("tasks/open/x.md").await.unwrap().unwrap();
        blobs.put("tasks/done/x.md", &bytes).await.unwrap();
        blobs.delete("tasks/open/x.md").await.unwrap();

        let (key, _) = vault.read_by_id(&id).await.unwrap().unwrap();
        assert_eq!(
            key, "tasks/done/x.md",
            "refiled by somebody else, still found"
        );
        assert_ne!(seen, 0);
    }

    #[tokio::test]
    async fn an_entity_deleted_by_somebody_else_is_reported_gone() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();

        blobs.delete("notes/a.md").await.unwrap();
        assert!(vault.read_by_id(&id).await.unwrap().is_none());
        assert!(vault.locate(&id).is_none(), "and dropped from the index");
    }

    #[tokio::test]
    async fn removing_clears_both_the_file_and_the_index() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();
        vault.remove(&id).await.unwrap();

        assert!(vault.locate(&id).is_none());
        assert!(blobs.get("notes/a.md").await.unwrap().is_none());
    }
}
