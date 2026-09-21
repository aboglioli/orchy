use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use orchy_core::{DomainError, Result};

/// Digest used for write preconditions. Process-local: it is compared only against another
/// digest taken by the same build, never stored.
pub fn digest(bytes: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// The seam that makes the byte source replaceable; everything above it is backend-agnostic.
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;

    /// Replace `key` only if what is there now digests to `expected`, atomically with respect
    /// to every other writer. `None` means the key must be absent.
    ///
    /// Comparing and writing as two calls is not enough: another process fits entirely between
    /// them, and the write that follows a passing check still discards someone else's change.
    /// Returns `false` when the precondition did not hold and nothing was written.
    async fn compare_and_put(&self, key: &str, expected: Option<u64>, bytes: &[u8])
    -> Result<bool>;

    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.get(key).await?.is_some())
    }
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn io(context: &str, e: std::io::Error) -> DomainError {
    DomainError::validation(format!("{context}: {e}"))
}

async fn ensure_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| io(&format!("creating {}", parent.display()), e))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;

    // The scratch name is unique per writer. Derived from the target alone, two processes
    // writing the same key would share one file: each truncates the other's, and the loser
    // renames either nothing or the winner's half-written bytes into place.
    let temp = path.with_extension(format!(
        "{}.{}.{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));

    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()
    };
    write().map_err(|e| io(&format!("writing {}", temp.display()), e))?;

    std::fs::rename(&temp, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        io(&format!("renaming into {}", path.display()), e)
    })
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

    /// Kept beside the vault's runtime state rather than next to the file, so a guard never
    /// shows up as a document and never reaches git.
    fn guard_path(&self, key: &str) -> Result<PathBuf> {
        let safe: String = key
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        Ok(self
            .root
            .join(".orchy/write-guards")
            .join(format!("{safe}.lock")))
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
        ensure_parent(&path).await?;
        let contents = bytes.to_vec();
        tokio::task::spawn_blocking(move || write_atomically(&path, &contents))
            .await
            .map_err(|e| DomainError::validation(format!("write task failed: {e}")))?
    }

    async fn compare_and_put(
        &self,
        key: &str,
        expected: Option<u64>,
        bytes: &[u8],
    ) -> Result<bool> {
        let path = self.path_of(key)?;
        let guard = self.guard_path(key)?;
        ensure_parent(&path).await?;
        ensure_parent(&guard).await?;

        let contents = bytes.to_vec();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            use fs4::fs_std::FileExt;

            let lock = std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&guard)
                .map_err(|e| io("opening write guard", e))?;
            FileExt::lock_exclusive(&lock).map_err(|e| io("locking write guard", e))?;

            let current = std::fs::read(&path).ok().map(|b| digest(&b));
            if current != expected {
                let _ = FileExt::unlock(&lock);
                return Ok(false);
            }

            let result = write_atomically(&path, &contents);
            let _ = FileExt::unlock(&lock);
            result.map(|()| true)
        })
        .await
        .map_err(|e| DomainError::validation(format!("write task failed: {e}")))?
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

    async fn compare_and_put(
        &self,
        key: &str,
        expected: Option<u64>,
        bytes: &[u8],
    ) -> Result<bool> {
        let mut blobs = self.0.lock().expect("blob mutex");
        if blobs.get(key).map(|b| digest(b)) != expected {
            return Ok(false);
        }
        blobs.insert(key.to_owned(), bytes.to_vec());
        Ok(true)
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

#[cfg(test)]
mod concurrent_write_tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn writers_to_one_key_do_not_share_a_scratch_file() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(FsBlobStore::new(temp.path()));

        let writes = (0..16).map(|n| {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                store
                    .put("docs/contended.md", format!("written by {n}").as_bytes())
                    .await
            })
        });

        for write in writes {
            write
                .await
                .unwrap()
                .expect("a concurrent write must not fail on a scratch file it does not own");
        }

        let landed = store.get("docs/contended.md").await.unwrap().unwrap();
        let text = String::from_utf8(landed).unwrap();
        assert!(
            text.starts_with("written by "),
            "one writer wins whole; nobody sees a torn file: {text:?}"
        );

        let strays: Vec<_> = std::fs::read_dir(temp.path().join("docs"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(
            strays.is_empty(),
            "every scratch file is renamed away: {strays:?}"
        );
    }
}
