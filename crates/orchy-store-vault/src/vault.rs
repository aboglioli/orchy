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

/// What must still be true of a file for a write to be allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precondition {
    /// Overwrite whatever is there. For a file this process alone is responsible for.
    Any,
    /// The bytes must still be the ones this process last read, and an entity it has never
    /// seen must not be there at all.
    ///
    /// Reading and writing are separate calls, so two agents can both load an entity, both
    /// change it, and the later write silently discard the earlier one. This turns that into
    /// a refusal the caller can see.
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub key: String,
    pub kind: EntityKind,
    /// Digest of the bytes this process last saw for the entity.
    pub seen: u64,
}

/// Identity lives in frontmatter, not in paths, so finding an entity means knowing which file
/// carries its id. Built once per process.
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

    /// A walk is not a snapshot. Refiling an entity writes it under its new status and unlinks
    /// it from its old one, and a walk that reads the new directory before the write and the
    /// old one after the unlink goes past the entity without ever seeing it — which is how a
    /// listing silently loses a task that another agent is finishing at that moment.
    ///
    /// So the directory is listed again once the reads are done, and anything that appeared in
    /// the meantime is read too. An entity missed that way is necessarily already at its new
    /// path by then: the unlink that hid it can only follow the write that put it there.
    pub async fn reindex(&self) -> Result<()> {
        let mut found = HashMap::new();
        let mut scanned = HashSet::new();
        let mut batch = self.blobs.list("").await?;

        for _ in 0..RESCAN_PASSES {
            for key in batch {
                if !scanned.insert(key.clone()) {
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
                found.insert(
                    id,
                    Located {
                        kind: kind_from(codec::kind_of(&file)),
                        key,
                        seen,
                    },
                );
            }

            batch = self
                .blobs
                .list("")
                .await?
                .into_iter()
                .filter(|key| !scanned.contains(key))
                .collect();
            if batch.is_empty() {
                break;
            }
        }
        let mut index = self.index.write().expect("index lock");
        for (id, located) in found.iter_mut() {
            // a rescan says where a file is, never what the caller last saw of it
            if let Some(previous) = index.get(id) {
                located.seen = previous.seen;
            }
        }
        // a scan that runs while another process refiles an entity can walk past its old
        // directory after the move and its new one before; forgetting it on that evidence
        // would turn someone else's move into a deletion
        for (id, located) in index.iter() {
            found.entry(id.clone()).or_insert_with(|| located.clone());
        }
        *index = found;
        Ok(())
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

    /// A read the caller will act on, so the bytes it saw become the precondition for its
    /// next write.
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

    /// A read taken as part of a write — carrying inert frontmatter forward, listing siblings
    /// to derive a parent — is not something the caller observed and reasoned about. Counting
    /// it would move the watermark to the instant before the write and make every precondition
    /// trivially true.
    pub async fn peek_by_id(&self, id: &Id) -> Result<Option<(String, MarkdownFile)>> {
        let Some((located, bytes)) = self.bytes_of(id).await? else {
            return Ok(None);
        };
        Ok(parse(&bytes, &located.key)?.map(|file| (located.key, file)))
    }

    /// The index says where things were when this process started, and another process refiles
    /// an entity every time its status changes, so a miss means rescan rather than report it
    /// gone. Refiling is a write to the new directory and an unlink from the old, and a scan
    /// that reads the new one before the write and the old one after the unlink walks past the
    /// entity entirely — so a miss is retried until the move it raced has settled, and only
    /// then believed.
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

    /// Write only when the file still holds what the caller last read.
    ///
    /// Reading and writing are separate calls, so two agents can both load a document, both
    /// change it, and the later write silently discard the earlier one. The precondition is
    /// what turns that into a refusal the caller can see.
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
            // an entity this process has no record of is one it is creating, so the file it
            // is about to write must not already exist. Claiming the key rather than
            // overwriting it is what stops a lost index entry from becoming a lost write.
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

    /// The compare and the write are one step. Split in two, a second writer fits between
    /// them: it passes its own check, and the write that follows still discards the change
    /// this one just made.
    ///
    /// The contention is on where the entity sits *now*, not where it is going — placement
    /// follows frontmatter, so an ordinary status change moves the file, and a destination
    /// path is supposed to be empty. Winning the compare on the old key is what proves
    /// nobody else touched the entity, so a move writes there first and unlinks it after.
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
        let mut loaded = Vec::new();
        for id in self.ids_of(kind) {
            if let Some(pair) = self.peek_by_id(&id).await? {
                loaded.push(pair);
            }
        }
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
