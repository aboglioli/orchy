use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use orchy_core::{DomainError, EntityKind, Id, Result};

use crate::blob::BlobStore;
use crate::codec;
use crate::layout::Layout;
use crate::markdown::MarkdownFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub key: String,
    pub kind: EntityKind,
}

/// Identity comes from frontmatter, never from a path, so finding an entity means knowing
/// which file currently carries its id. The index is that map, built once per process.
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
        let mut found = HashMap::new();
        for key in self.blobs.list("").await? {
            if !self.layout.is_markdown(&key) || self.layout.is_runtime(&key) {
                continue;
            }
            let Some(file) = self.read(&key).await? else {
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
                },
            );
        }
        *self.index.write().expect("index lock") = found;
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

    pub async fn read_by_id(&self, id: &Id) -> Result<Option<(String, MarkdownFile)>> {
        let Some(located) = self.locate(id) else {
            return Ok(None);
        };
        Ok(self
            .read(&located.key)
            .await?
            .map(|file| (located.key, file)))
    }

    pub async fn write(
        &self,
        key: &str,
        file: &MarkdownFile,
        id: &Id,
        kind: EntityKind,
    ) -> Result<()> {
        if let Some(previous) = self.locate(id)
            && previous.key != key
        {
            self.blobs.delete(&previous.key).await?;
        }
        self.blobs.put(key, file.render()?.as_bytes()).await?;
        self.index.write().expect("index lock").insert(
            id.clone(),
            Located {
                key: key.to_owned(),
                kind,
            },
        );
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
            if let Some(pair) = self.read_by_id(&id).await? {
                loaded.push(pair);
            }
        }
        loaded.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(loaded)
    }
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
