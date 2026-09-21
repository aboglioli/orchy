use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use orchy_core::{DomainError, Result};

/// The seam that makes the byte source replaceable; everything above it is backend-agnostic.
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.get(key).await?.is_some())
    }
}

fn io(context: &str, e: std::io::Error) -> DomainError {
    DomainError::validation(format!("{context}: {e}"))
}

/// Writes are atomic: temp file, fsync, rename, so a crash never leaves a half-parsed
/// document behind.
pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_of(&self, key: &str) -> Result<PathBuf> {
        if key.is_empty() {
            return Err(DomainError::validation("blob key must not be empty"));
        }
        if key.starts_with('/') || key.contains("..") {
            return Err(DomainError::validation(format!(
                "blob key `{key}` must be a relative path without `..`"
            )));
        }
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl BlobStore for FsBlobStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path_of(key)?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(&format!("reading {}", path.display()), e)),
        }
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_of(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| io(&format!("creating {}", parent.display()), e))?;
        }

        let temp = path.with_extension(format!(
            "{}.tmp",
            path.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));
        let contents = bytes.to_vec();
        let temp_for_write = temp.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::fs::File::create(&temp_for_write)?;
            file.write_all(&contents)?;
            file.sync_all()
        })
        .await
        .map_err(|e| DomainError::validation(format!("write task failed: {e}")))?
        .map_err(|e| io(&format!("writing {}", temp.display()), e))?;

        tokio::fs::rename(&temp, &path)
            .await
            .map_err(|e| io(&format!("renaming into {}", path.display()), e))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.path_of(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(&format!("deleting {}", path.display()), e)),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let root = self.root.clone();
        let prefix = prefix.to_owned();
        tokio::task::spawn_blocking(move || {
            let base = root.join(&prefix);
            if !base.exists() {
                return Vec::new();
            }
            let mut keys = Vec::new();
            for entry in ignore::WalkBuilder::new(&base)
                .hidden(true)
                .git_ignore(true)
                .build()
                .flatten()
            {
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    continue;
                }
                if let Ok(relative) = entry.path().strip_prefix(&root)
                    && let Some(key) = relative.to_str()
                {
                    keys.push(key.to_owned());
                }
            }
            keys.sort();
            keys
        })
        .await
        .map_err(|e| DomainError::validation(format!("list task failed: {e}")))
    }
}

#[derive(Default)]
pub struct MemoryBlobStore(Mutex<BTreeMap<String, Vec<u8>>>);

impl MemoryBlobStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl BlobStore for MemoryBlobStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().expect("blob mutex").get(key).cloned())
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        self.0
            .lock()
            .expect("blob mutex")
            .insert(key.to_owned(), bytes.to_vec());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().expect("blob mutex").remove(key);
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .0
            .lock()
            .expect("blob mutex")
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn round_trip(store: &dyn BlobStore) {
        assert_eq!(store.get("a/b.md").await.unwrap(), None);
        assert!(!store.exists("a/b.md").await.unwrap());

        store.put("a/b.md", b"hello").await.unwrap();
        assert_eq!(store.get("a/b.md").await.unwrap(), Some(b"hello".to_vec()));
        assert!(store.exists("a/b.md").await.unwrap());

        store.put("a/b.md", b"goodbye").await.unwrap();
        assert_eq!(
            store.get("a/b.md").await.unwrap(),
            Some(b"goodbye".to_vec()),
            "put overwrites"
        );

        store.put("a/c.md", b"x").await.unwrap();
        store.put("z/d.md", b"x").await.unwrap();
        let listed = store.list("a").await.unwrap();
        assert_eq!(listed.len(), 2, "list is prefix-scoped: {listed:?}");

        store.delete("a/b.md").await.unwrap();
        assert_eq!(store.get("a/b.md").await.unwrap(), None);
        store.delete("a/b.md").await.unwrap();
    }

    #[tokio::test]
    async fn memory_and_fs_backends_behave_identically() {
        let temp = tempfile::tempdir().unwrap();
        round_trip(&MemoryBlobStore::new()).await;
        round_trip(&FsBlobStore::new(temp.path())).await;
    }

    #[tokio::test]
    async fn fs_rejects_keys_that_would_escape_the_vault() {
        let temp = tempfile::tempdir().unwrap();
        let store = FsBlobStore::new(temp.path());
        assert!(store.get("../outside.md").await.is_err());
        assert!(store.get("/etc/passwd").await.is_err());
        assert!(store.put("a/../../x.md", b"x").await.is_err());
        assert!(store.get("").await.is_err());
    }

    #[tokio::test]
    async fn fs_writes_leave_no_temp_files_behind() {
        let temp = tempfile::tempdir().unwrap();
        let store = FsBlobStore::new(temp.path());
        store.put("docs/a.md", b"hello").await.unwrap();

        let stray: Vec<_> = std::fs::read_dir(temp.path().join("docs"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(stray.is_empty(), "atomic rename must clean up: {stray:?}");
    }
}
