use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchy_core::{DomainError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::lock::{DEFAULT_WAIT, FileLock};

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

    /// One call for many keys, in the order given.
    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
        let mut found = Vec::with_capacity(keys.len());
        for key in keys {
            found.push(self.get(key).await?);
        }
        Ok(found)
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;

    /// Replace `key` only if what is there now digests to `expected`, atomically with respect
    /// to every other writer; `None` means the key must be absent. Returns `false` when the
    /// precondition did not hold and nothing was written. Comparing and writing as two calls
    /// is not enough: another process fits entirely between them.
    async fn compare_and_put(&self, key: &str, expected: Option<u64>, bytes: &[u8])
    -> Result<bool>;

    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;

    /// Each key with a value that changes whenever its bytes may have, when the backend can
    /// tell without reading them; `None` means the caller has to read.
    async fn list_fingerprinted(&self, prefix: &str) -> Result<Vec<(String, Option<u64>)>> {
        Ok(self
            .list(prefix)
            .await?
            .into_iter()
            .map(|key| (key, None))
            .collect())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.get(key).await?.is_some())
    }

    /// Applies every change or none: each precondition is checked with every key held, so no
    /// other writer fits between the checks and the writes, and a refused precondition is a
    /// conflict that leaves everything as it was.
    async fn commit(&self, changes: &[Change]) -> Result<()>;

    /// Finishes a commit that a crashed process left halfway.
    async fn recover(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    Anything,
    /// `None` means the key must be absent.
    Exactly(Option<u64>),
}

/// Rewrites what a key holds; `None` in or out means the key is absent.
pub type Patch = Arc<dyn Fn(Option<&[u8]>) -> Result<Option<Vec<u8>>> + Send + Sync>;

#[derive(Clone)]
pub enum Content {
    Put(Vec<u8>),
    Delete,
    /// Applied to whatever the key holds when the commit lands, with the key held: the edit
    /// commutes with other writers instead of conflicting with them.
    Patch(Patch),
    /// Writes nothing: the key was only read, and the commit holds only while it is unchanged.
    Keep,
}

impl Content {
    pub fn apply(&self, current: Option<&[u8]>) -> Result<Option<Vec<u8>>> {
        match self {
            Self::Put(bytes) => Ok(Some(bytes.clone())),
            Self::Delete => Ok(None),
            Self::Patch(patch) => patch(current),
            Self::Keep => Ok(current.map(<[u8]>::to_vec)),
        }
    }
}

impl fmt::Debug for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Put(bytes) => write!(f, "Put({} bytes)", bytes.len()),
            Self::Delete => f.write_str("Delete"),
            Self::Patch(_) => f.write_str("Patch"),
            Self::Keep => f.write_str("Keep"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Change {
    pub key: String,
    pub expected: Expect,
    pub content: Content,
}

fn changed_meanwhile(change: &Change) -> DomainError {
    let key = &change.key;
    // a keep or a patch carries a precondition only because the key was read to decide
    if matches!(change.content, Content::Keep | Content::Patch(_)) {
        return DomainError::contended(format!(
            "`{key}`, which this command read, changed before it finished; nothing was written"
        ));
    }
    DomainError::conflict(format!(
        "`{key}` changed while this command ran; nothing was written, so run it again"
    ))
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn io(context: &str, e: io::Error) -> DomainError {
    DomainError::unavailable(format!("{context}: {e}"))
}

async fn ensure_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| io(&format!("creating {}", parent.display()), e))
}

/// Writes replace files by rename, so a rewrite always changes the inode; an editor writing
/// in place changes the modification time.
fn fingerprint(meta: &std::fs::Metadata) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    meta.len().hash(&mut hasher);
    meta.modified().ok().hash(&mut hasher);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()).hash(&mut hasher);
    }
    hasher.finish()
}

fn read_all(paths: &[PathBuf]) -> Result<Vec<Option<Vec<u8>>>> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = paths.len().div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || part.iter().map(|p| read_one(p)).collect::<Result<Vec<_>>>())
            })
            .collect();
        let mut all = Vec::with_capacity(paths.len());
        for handle in handles {
            all.extend(
                handle
                    .join()
                    .map_err(|_| DomainError::unavailable("a read worker panicked"))??,
            );
        }
        Ok(all)
    })
}

fn read_one(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(&format!("reading {}", path.display()), e)),
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = write_temp(path, bytes)?;
    std::fs::rename(&temp, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        io(&format!("renaming into {}", path.display()), e)
    })
}

/// The bytes, durable, beside `path` under a name no other writer shares: derived from the
/// target alone, two processes writing one key would share a scratch file and the loser would
/// rename half-written bytes into place.
fn write_temp(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    use std::io::Write;

    let temp = path.with_extension(format!(
        "{}.{}.{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| io(&format!("creating {}", parent.display()), e))?;
    }
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        io(&format!("writing {}", temp.display()), e)
    })?;
    Ok(temp)
}

fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(&format!("deleting {}", path.display()), e)),
    }
}

/// A stable digest, unlike [`digest`]: a journal may be recovered by another build.
fn fingerprint_of(path: &Path) -> Result<Option<String>> {
    Ok(read_one(path)?.map(|bytes| hex::encode(Sha256::digest(bytes))))
}

/// One change of a commit, as the journal records it before any of them is applied.
#[derive(Debug, Serialize, Deserialize)]
struct Step {
    key: String,
    /// Where the new bytes wait; `None` deletes the key.
    temp: Option<String>,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    steps: Vec<Step>,
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

    async fn walk(&self, prefix: &str, fingerprints: bool) -> Result<Vec<(String, Option<u64>)>> {
        let root = self.root.clone();
        let prefix = prefix.to_owned();
        tokio::task::spawn_blocking(move || {
            let base = root.join(&prefix);
            if !base.exists() {
                return Vec::new();
            }
            let found = std::sync::Mutex::new(Vec::new());
            // hidden entries are skipped (`.orchy`, an editor's swap files); ignore files are not
            // read, so what orchy sees never depends on git
            ignore::WalkBuilder::new(&base)
                .standard_filters(false)
                .hidden(true)
                .build_parallel()
                .run(|| {
                    Box::new(|entry| {
                        let Ok(entry) = entry else {
                            return ignore::WalkState::Continue;
                        };
                        if !entry.file_type().is_some_and(|t| t.is_file()) {
                            return ignore::WalkState::Continue;
                        }
                        let Some(key) = entry
                            .path()
                            .strip_prefix(&root)
                            .ok()
                            .and_then(|relative| relative.to_str())
                        else {
                            return ignore::WalkState::Continue;
                        };
                        let print = if fingerprints {
                            entry.metadata().ok().map(|m| fingerprint(&m))
                        } else {
                            None
                        };
                        found
                            .lock()
                            .expect("walk results lock")
                            .push((key.to_owned(), print));
                        ignore::WalkState::Continue
                    })
                });
            let mut keys = found.into_inner().expect("walk results lock");
            keys.sort();
            keys
        })
        .await
        .map_err(|e| DomainError::unavailable(format!("list task failed: {e}")))
    }

    fn journal_dir(&self) -> PathBuf {
        self.root.join(".orchy/journal")
    }

    /// Holds every key of the commit, in key order so two commits never wait on each other,
    /// checks every precondition, then journals the whole change before applying any of it:
    /// a crash after that point is finished by the next [`BlobStore::recover`].
    fn commit_blocking(&self, changes: &[Change]) -> Result<()> {
        let mut ordered: Vec<&Change> = changes.iter().collect();
        ordered.sort_by(|a, b| a.key.cmp(&b.key));
        ordered.dedup_by(|a, b| a.key == b.key);

        let mut held = Vec::with_capacity(ordered.len());
        for change in &ordered {
            held.push(FileLock::exclusive(
                &self.guard_path(&change.key)?,
                "write guard",
                DEFAULT_WAIT,
            )?);
        }
        let mut finals = Vec::with_capacity(ordered.len());
        for change in &ordered {
            let current = read_one(&self.path_of(&change.key)?)?;
            if let Expect::Exactly(expected) = change.expected
                && current.as_deref().map(digest) != expected
            {
                return Err(changed_meanwhile(change));
            }
            let after = change.content.apply(current.as_deref())?;
            if after != current {
                finals.push((change.key.as_str(), after));
            }
        }

        if let [(key, only)] = finals.as_slice() {
            let path = self.path_of(key)?;
            return match only {
                Some(bytes) => write_atomically(&path, bytes),
                None => remove_if_present(&path),
            };
        }
        if finals.is_empty() {
            return Ok(());
        }

        let mut steps = Vec::with_capacity(finals.len());
        let staged = (|| -> Result<()> {
            for (key, content) in &finals {
                let path = self.path_of(key)?;
                let before = fingerprint_of(&path)?;
                let (temp, after) = match content {
                    Some(bytes) => {
                        let temp = write_temp(&path, bytes)?;
                        (
                            Some(self.key_of(&temp)?),
                            Some(hex::encode(Sha256::digest(bytes))),
                        )
                    }
                    None => (None, None),
                };
                steps.push(Step {
                    key: (*key).to_owned(),
                    temp,
                    before,
                    after,
                });
            }
            Ok(())
        })();
        if let Err(e) = staged {
            self.discard_temps(&steps);
            return Err(e);
        }

        let journal = self.journal_dir().join(format!(
            "{}.{}.json",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let record = Journal { steps };
        let encoded = serde_json::to_vec(&record)
            .map_err(|e| DomainError::unavailable(format!("encoding the commit journal: {e}")))?;
        if let Err(e) = write_atomically(&journal, &encoded) {
            self.discard_temps(&record.steps);
            return Err(e);
        }

        self.apply(&record.steps)?;
        remove_if_present(&journal)?;
        drop(held);
        Ok(())
    }

    fn apply(&self, steps: &[Step]) -> Result<()> {
        for step in steps {
            let path = self.path_of(&step.key)?;
            match &step.temp {
                Some(temp) => std::fs::rename(self.path_of(temp)?, &path)
                    .map_err(|e| io(&format!("renaming into {}", path.display()), e))?,
                None => remove_if_present(&path)?,
            }
        }
        Ok(())
    }

    fn discard_temps(&self, steps: &[Step]) {
        for temp in steps.iter().filter_map(|s| s.temp.as_deref()) {
            if let Ok(path) = self.path_of(temp) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// A step is finished only while its key still holds what it held when the commit was
    /// journaled; a key someone wrote since keeps that newer write.
    fn recover_blocking(&self) -> Result<()> {
        let entries = match std::fs::read_dir(self.journal_dir()) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io("reading the commit journal", e)),
        };
        let mut journals: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        journals.sort();

        for journal in journals {
            let Some(bytes) = read_one(&journal)? else {
                continue;
            };
            let Ok(record) = serde_json::from_slice::<Journal>(&bytes) else {
                remove_if_present(&journal)?;
                continue;
            };
            let mut keys: Vec<&str> = record.steps.iter().map(|s| s.key.as_str()).collect();
            keys.sort_unstable();
            let mut held = Vec::with_capacity(keys.len());
            for key in keys {
                held.push(FileLock::exclusive(
                    &self.guard_path(key)?,
                    "write guard",
                    DEFAULT_WAIT,
                )?);
            }
            // its owner may have finished while we waited for the keys
            if !journal.exists() {
                continue;
            }
            for step in &record.steps {
                let path = self.path_of(&step.key)?;
                let current = fingerprint_of(&path)?;
                let temp = step.temp.as_deref().map(|t| self.path_of(t)).transpose()?;
                if current != step.after && current == step.before {
                    match &temp {
                        Some(temp) if temp.exists() => std::fs::rename(temp, &path)
                            .map_err(|e| io(&format!("renaming into {}", path.display()), e))?,
                        Some(_) => {}
                        None => remove_if_present(&path)?,
                    }
                }
                if let Some(temp) = &temp {
                    remove_if_present(temp)?;
                }
            }
            remove_if_present(&journal)?;
            drop(held);
        }
        Ok(())
    }

    fn key_of(&self, path: &Path) -> Result<String> {
        path.strip_prefix(&self.root)
            .ok()
            .and_then(|relative| relative.to_str())
            .map(str::to_owned)
            .ok_or_else(|| {
                DomainError::unavailable(format!("{} is outside the vault", path.display()))
            })
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

    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
        let paths = keys
            .iter()
            .map(|key| self.path_of(key))
            .collect::<Result<Vec<_>>>()?;
        tokio::task::spawn_blocking(move || read_all(&paths))
            .await
            .map_err(|e| DomainError::unavailable(format!("read task failed: {e}")))?
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_of(key)?;
        ensure_parent(&path).await?;
        let contents = bytes.to_vec();
        tokio::task::spawn_blocking(move || write_atomically(&path, &contents))
            .await
            .map_err(|e| DomainError::unavailable(format!("write task failed: {e}")))?
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
            let _guard = FileLock::exclusive(&guard, "write guard", DEFAULT_WAIT)?;

            let current = std::fs::read(&path).ok().map(|b| digest(&b));
            if current != expected {
                return Ok(false);
            }
            write_atomically(&path, &contents).map(|()| true)
        })
        .await
        .map_err(|e| DomainError::unavailable(format!("write task failed: {e}")))?
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
        Ok(self
            .walk(prefix, false)
            .await?
            .into_iter()
            .map(|(key, _)| key)
            .collect())
    }

    async fn list_fingerprinted(&self, prefix: &str) -> Result<Vec<(String, Option<u64>)>> {
        self.walk(prefix, true).await
    }

    async fn commit(&self, changes: &[Change]) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let store = Self::new(self.root.clone());
        let changes = changes.to_vec();
        tokio::task::spawn_blocking(move || store.commit_blocking(&changes))
            .await
            .map_err(|e| DomainError::unavailable(format!("commit task failed: {e}")))?
    }

    async fn recover(&self) -> Result<()> {
        let store = Self::new(self.root.clone());
        tokio::task::spawn_blocking(move || store.recover_blocking())
            .await
            .map_err(|e| DomainError::unavailable(format!("recovery task failed: {e}")))?
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

    async fn commit(&self, changes: &[Change]) -> Result<()> {
        let mut blobs = self.0.lock().expect("blob mutex");
        let mut finals = Vec::with_capacity(changes.len());
        for change in changes {
            let current = blobs.get(&change.key);
            if let Expect::Exactly(expected) = change.expected
                && current.map(|b| digest(b)) != expected
            {
                return Err(changed_meanwhile(change));
            }
            finals.push((
                &change.key,
                change.content.apply(current.map(Vec::as_slice))?,
            ));
        }
        for (key, content) in finals {
            match content {
                Some(bytes) => blobs.insert(key.clone(), bytes),
                None => blobs.remove(key),
            };
        }
        Ok(())
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

    async fn compare_and_put_contract(store: &dyn BlobStore) {
        assert!(
            !store.compare_and_put("a.md", Some(1), b"x").await.unwrap(),
            "a key that is not there cannot match a digest"
        );
        assert!(
            store.compare_and_put("a.md", None, b"first").await.unwrap(),
            "None claims an absent key"
        );
        assert!(
            !store
                .compare_and_put("a.md", None, b"second")
                .await
                .unwrap(),
            "and refuses once somebody holds it"
        );
        assert_eq!(store.get("a.md").await.unwrap(), Some(b"first".to_vec()));

        let held = digest(b"first");
        assert!(
            !store
                .compare_and_put("a.md", Some(held + 1), b"no")
                .await
                .unwrap()
        );
        assert_eq!(store.get("a.md").await.unwrap(), Some(b"first".to_vec()));

        assert!(
            store
                .compare_and_put("a.md", Some(held), b"next")
                .await
                .unwrap()
        );
        assert_eq!(store.get("a.md").await.unwrap(), Some(b"next".to_vec()));
    }

    #[tokio::test]
    async fn both_backends_compare_and_put_the_same_way() {
        let temp = tempfile::tempdir().unwrap();
        compare_and_put_contract(&MemoryBlobStore::new()).await;
        compare_and_put_contract(&FsBlobStore::new(temp.path())).await;
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
    async fn exactly_one_of_many_compare_and_puts_on_one_key_succeeds() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(FsBlobStore::new(temp.path()));
        store.put("docs/a.md", b"start").await.unwrap();
        let expected = digest(b"start");

        let attempts = (0..16).map(|n| {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                store
                    .compare_and_put("docs/a.md", Some(expected), format!("by {n}").as_bytes())
                    .await
                    .unwrap()
            })
        });

        let mut winners = 0;
        for attempt in attempts {
            if attempt.await.unwrap() {
                winners += 1;
            }
        }
        assert_eq!(
            winners, 1,
            "the compare and the write are one step, so only one writer can see `start`"
        );
    }

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

#[cfg(test)]
mod commit_tests {
    use super::*;

    fn put(key: &str, bytes: &str, expected: Expect) -> Change {
        Change {
            key: key.to_owned(),
            expected,
            content: Content::Put(bytes.as_bytes().to_vec()),
        }
    }

    async fn all_or_nothing(store: &dyn BlobStore) {
        store.put("a.md", b"a0").await.unwrap();
        store.put("b.md", b"b0").await.unwrap();

        let refused = store
            .commit(&[
                put("a.md", "a1", Expect::Exactly(Some(digest(b"a0")))),
                put("b.md", "b1", Expect::Exactly(Some(digest(b"stale")))),
                put("c.md", "c1", Expect::Exactly(None)),
            ])
            .await;
        assert!(
            matches!(refused, Err(DomainError::Conflict(_))),
            "{refused:?}"
        );
        assert_eq!(store.get("a.md").await.unwrap(), Some(b"a0".to_vec()));
        assert_eq!(
            store.get("c.md").await.unwrap(),
            None,
            "nothing of it landed"
        );

        store
            .commit(&[
                put("a.md", "a1", Expect::Exactly(Some(digest(b"a0")))),
                put("c.md", "c1", Expect::Exactly(None)),
                Change {
                    key: "b.md".to_owned(),
                    expected: Expect::Anything,
                    content: Content::Delete,
                },
            ])
            .await
            .unwrap();
        assert_eq!(store.get("a.md").await.unwrap(), Some(b"a1".to_vec()));
        assert_eq!(store.get("b.md").await.unwrap(), None);
        assert_eq!(store.get("c.md").await.unwrap(), Some(b"c1".to_vec()));
    }

    #[tokio::test]
    async fn a_commit_lands_whole_or_not_at_all_on_both_backends() {
        let temp = tempfile::tempdir().unwrap();
        all_or_nothing(&MemoryBlobStore::new()).await;
        all_or_nothing(&FsBlobStore::new(temp.path())).await;
    }

    #[tokio::test]
    async fn a_patch_applies_to_what_the_key_holds_when_the_commit_lands() {
        let temp = tempfile::tempdir().unwrap();
        let store = FsBlobStore::new(temp.path());
        store.put("hub.md", b"one").await.unwrap();
        let append: Patch = Arc::new(|current| {
            let mut bytes = current.unwrap_or_default().to_vec();
            bytes.extend_from_slice(b"+two");
            Ok(Some(bytes))
        });
        store.put("hub.md", b"one+other").await.unwrap();
        store
            .commit(&[Change {
                key: "hub.md".to_owned(),
                expected: Expect::Anything,
                content: Content::Patch(append),
            }])
            .await
            .unwrap();
        assert_eq!(
            store.get("hub.md").await.unwrap(),
            Some(b"one+other+two".to_vec()),
            "the other writer's change is kept"
        );
    }

    #[tokio::test]
    async fn a_read_that_changed_fails_as_contention_not_as_a_conflict() {
        let store = MemoryBlobStore::new();
        store.put("read.md", b"seen").await.unwrap();
        store.put("read.md", b"changed").await.unwrap();
        let refused = store
            .commit(&[Change {
                key: "read.md".to_owned(),
                expected: Expect::Exactly(Some(digest(b"seen"))),
                content: Content::Keep,
            }])
            .await;
        assert!(
            matches!(refused, Err(DomainError::Contended(_))),
            "{refused:?}"
        );
    }

    fn journal(store: &FsBlobStore, steps: Vec<Step>) {
        let record = serde_json::to_vec(&Journal { steps }).unwrap();
        write_atomically(&store.journal_dir().join("1.1.json"), &record).unwrap();
    }

    fn stable(bytes: &[u8]) -> Option<String> {
        Some(hex::encode(Sha256::digest(bytes)))
    }

    #[tokio::test]
    async fn a_commit_a_crash_left_halfway_is_finished_on_the_next_open() {
        let temp = tempfile::tempdir().unwrap();
        let store = FsBlobStore::new(temp.path());
        store.put("docs/a.md", b"old a").await.unwrap();
        store.put("docs/b.md", b"new b").await.unwrap();
        store.put("docs/gone.md", b"doomed").await.unwrap();
        let temp_a = write_temp(&temp.path().join("docs/a.md"), b"new a").unwrap();

        journal(
            &store,
            vec![
                Step {
                    key: "docs/a.md".to_owned(),
                    temp: Some(store.key_of(&temp_a).unwrap()),
                    before: stable(b"old a"),
                    after: stable(b"new a"),
                },
                Step {
                    key: "docs/b.md".to_owned(),
                    temp: Some("docs/b.md.applied.tmp".to_owned()),
                    before: stable(b"old b"),
                    after: stable(b"new b"),
                },
                Step {
                    key: "docs/gone.md".to_owned(),
                    temp: None,
                    before: stable(b"doomed"),
                    after: None,
                },
            ],
        );

        store.recover().await.unwrap();
        assert_eq!(
            store.get("docs/a.md").await.unwrap(),
            Some(b"new a".to_vec())
        );
        assert_eq!(
            store.get("docs/b.md").await.unwrap(),
            Some(b"new b".to_vec())
        );
        assert_eq!(store.get("docs/gone.md").await.unwrap(), None);
        assert!(!temp_a.exists(), "the staged bytes were renamed into place");
        assert!(
            std::fs::read_dir(store.journal_dir())
                .unwrap()
                .next()
                .is_none(),
            "a finished journal is removed"
        );
    }

    #[tokio::test]
    async fn recovery_keeps_a_write_made_after_the_crash() {
        let temp = tempfile::tempdir().unwrap();
        let store = FsBlobStore::new(temp.path());
        store.put("docs/a.md", b"written since").await.unwrap();
        let temp_a = write_temp(&temp.path().join("docs/a.md"), b"from the crash").unwrap();
        journal(
            &store,
            vec![Step {
                key: "docs/a.md".to_owned(),
                temp: Some(store.key_of(&temp_a).unwrap()),
                before: stable(b"before the crash"),
                after: stable(b"from the crash"),
            }],
        );

        store.recover().await.unwrap();
        assert_eq!(
            store.get("docs/a.md").await.unwrap(),
            Some(b"written since".to_vec()),
            "a newer write is never rolled over by an old commit"
        );
        assert!(!temp_a.exists());
    }

    #[tokio::test]
    async fn listing_never_depends_on_git_ignore_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".git")).unwrap();
        std::fs::write(temp.path().join(".gitignore"), "docs/\n").unwrap();
        std::fs::write(temp.path().join(".ignore"), "tasks/\n").unwrap();
        let store = FsBlobStore::new(temp.path());
        store.put("docs/a.md", b"a").await.unwrap();
        store.put("tasks/open/b.md", b"b").await.unwrap();

        let listed = store.list("").await.unwrap();
        assert!(listed.contains(&"docs/a.md".to_owned()), "{listed:?}");
        assert!(listed.contains(&"tasks/open/b.md".to_owned()), "{listed:?}");
    }
}
