use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use eventuary::{Payload, Topic};
use orchy_core::{
    DomainError, DomainEvent, EventLog, EventQuery, Id, Namespace, RecordedEvent, Result,
    UnitOfWork, Work,
};

use crate::blob::{BlobStore, Change, Content, Expect, Patch, digest};
use crate::vault::Vault;

tokio::task_local! {
    static STAGING: Mutex<Staging>;
}

/// What a unit of work wrote so far. Nothing in it has reached the inner store.
#[derive(Debug, Clone, Default)]
struct Staging {
    changes: BTreeMap<String, Staged>,
    events: Vec<FrozenEvent>,
}

#[derive(Debug, Clone)]
struct Staged {
    /// What the key held before this unit of work first touched it.
    expected: Expect,
    content: Content,
}

impl Staging {
    fn into_changes(self) -> (Vec<Change>, Vec<FrozenEvent>) {
        let changes = self
            .changes
            .into_iter()
            .map(|(key, staged)| Change {
                key,
                expected: staged.expected,
                content: staged.content,
            })
            .collect();
        (changes, self.events)
    }
}

fn staged<R>(f: impl FnOnce(&mut Staging) -> R) -> Option<R> {
    STAGING
        .try_with(|staging| f(&mut staging.lock().expect("staging lock")))
        .ok()
}

fn is_active() -> bool {
    STAGING.try_with(|_| ()).is_ok()
}

/// Runs `work` so that every write it makes through the vault lands together, or none does.
/// Nested runs keep a savepoint: a failed inner run undoes only its own writes.
pub(crate) async fn atomically(
    vault: &Vault,
    log: Option<&dyn EventLog>,
    work: Work<'_>,
) -> Result<()> {
    let index = vault.remember();
    if let Some(saved) = staged(|staging| staging.clone()) {
        let result = work.await;
        if result.is_err() {
            staged(|staging| *staging = saved);
            vault.restore(index);
        }
        return result;
    }

    let (result, staging) = STAGING
        .scope(Mutex::new(Staging::default()), async {
            let result = work.await;
            let staging = staged(std::mem::take).unwrap_or_default();
            (result, staging)
        })
        .await;
    if let Err(e) = result {
        vault.restore(index);
        return Err(e);
    }

    let (changes, events) = staging.into_changes();
    if let Err(e) = vault.staged().inner().commit(&changes).await {
        vault.restore(index);
        return Err(e);
    }
    if events.is_empty() {
        return Ok(());
    }
    let Some(log) = log else {
        return Err(DomainError::unavailable(
            "events were recorded inside a unit of work that has no log to append them to",
        ));
    };
    let events: Vec<Box<dyn DomainEvent>> = events
        .into_iter()
        .map(|e| Box::new(e) as Box<dyn DomainEvent>)
        .collect();
    log.append(&events).await
}

/// Inside a unit of work, writes are kept here and reads see them; outside one, every call
/// goes straight to the inner store.
pub struct StagedBlobStore {
    inner: Arc<dyn BlobStore>,
}

impl StagedBlobStore {
    pub fn new(inner: Arc<dyn BlobStore>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<dyn BlobStore> {
        &self.inner
    }

    fn entry(&self, key: &str) -> Option<Staged> {
        staged(|staging| staging.changes.get(key).cloned()).flatten()
    }

    fn set(&self, key: &str, staged_as: Staged) {
        staged(|staging| staging.changes.insert(key.to_owned(), staged_as));
    }

    /// What the key holds as this unit of work sees it.
    async fn view(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.entry(key) {
            Some(Staged {
                content: Content::Patch(patch),
                ..
            }) => patch(self.inner.get(key).await?.as_deref()),
            Some(Staged {
                content: Content::Keep,
                ..
            })
            | None => self.inner.get(key).await,
            Some(staged) => staged.content.apply(None),
        }
    }

    /// The unit of work holds only while the key still digests to `seen`, what it was read as.
    pub(crate) fn guard(&self, key: &str, seen: u64) {
        staged(|staging| {
            staging.changes.entry(key.to_owned()).or_insert(Staged {
                expected: Expect::Exactly(Some(seen)),
                content: Content::Keep,
            });
        });
    }

    /// Edits the key where it stands when the unit of work lands, so the edit never conflicts
    /// with another writer's.
    pub async fn amend(&self, key: &str, patch: Patch) -> Result<()> {
        if !is_active() {
            return self
                .inner
                .commit(&[Change {
                    key: key.to_owned(),
                    expected: Expect::Anything,
                    content: Content::Patch(patch),
                }])
                .await;
        }
        let staged_as = match self.entry(key) {
            None => Staged {
                expected: Expect::Anything,
                content: Content::Patch(patch),
            },
            Some(Staged {
                expected,
                content: Content::Keep,
            }) => Staged {
                expected,
                content: Content::Patch(patch),
            },
            Some(Staged {
                expected,
                content: Content::Patch(earlier),
            }) => Staged {
                expected,
                content: Content::Patch(Arc::new(move |current| {
                    let between = earlier(current)?;
                    patch(between.as_deref())
                })),
            },
            Some(Staged { expected, content }) => Staged {
                expected,
                content: match patch(content.apply(None)?.as_deref())? {
                    Some(bytes) => Content::Put(bytes),
                    None => Content::Delete,
                },
            },
        };
        self.set(key, staged_as);
        Ok(())
    }

    fn overlay(
        &self,
        prefix: &str,
        mut listed: Vec<(String, Option<u64>)>,
    ) -> Vec<(String, Option<u64>)> {
        let Some(changes) = staged(|staging| staging.changes.clone()) else {
            return listed;
        };
        listed.retain(|(key, _)| {
            changes
                .get(key)
                .is_none_or(|s| matches!(s.content, Content::Keep))
        });
        for (key, staged) in changes {
            if !key.starts_with(prefix) {
                continue;
            }
            match staged.content {
                Content::Delete | Content::Keep => {}
                Content::Put(_) | Content::Patch(_) => listed.push((key, None)),
            }
        }
        listed.sort();
        listed
    }
}

#[async_trait]
impl BlobStore for StagedBlobStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.view(key).await
    }

    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>> {
        if !is_active() {
            return self.inner.get_many(keys).await;
        }
        let mut found = Vec::with_capacity(keys.len());
        for key in keys {
            found.push(self.view(key).await?);
        }
        Ok(found)
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        if !is_active() {
            return self.inner.put(key, bytes).await;
        }
        let expected = self.entry(key).map_or(Expect::Anything, |s| s.expected);
        self.set(
            key,
            Staged {
                expected,
                content: Content::Put(bytes.to_vec()),
            },
        );
        Ok(())
    }

    async fn compare_and_put(
        &self,
        key: &str,
        expected: Option<u64>,
        bytes: &[u8],
    ) -> Result<bool> {
        if !is_active() {
            return self.inner.compare_and_put(key, expected, bytes).await;
        }
        if let Some(Staged {
            content: Content::Patch(patch),
            ..
        }) = self.entry(key)
        {
            // a pending patch commutes with this write: the writer may have read before it or
            // after it, and the patch is applied again on top of what it wrote
            let base = self.inner.get(key).await?;
            let viewed = patch(base.as_deref())?;
            let base = base.as_deref().map(digest);
            if base != expected && viewed.as_deref().map(digest) != expected {
                return Ok(false);
            }
            let content = match patch(Some(bytes))? {
                Some(bytes) => Content::Put(bytes),
                None => Content::Delete,
            };
            self.set(
                key,
                Staged {
                    expected: Expect::Exactly(base),
                    content,
                },
            );
            return Ok(true);
        }
        if self.view(key).await?.as_deref().map(digest) != expected {
            return Ok(false);
        }
        let precondition = match self.entry(key) {
            None
            | Some(Staged {
                content: Content::Keep,
                ..
            }) => Expect::Exactly(expected),
            Some(staged) => staged.expected,
        };
        self.set(
            key,
            Staged {
                expected: precondition,
                content: Content::Put(bytes.to_vec()),
            },
        );
        Ok(true)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        if !is_active() {
            return self.inner.delete(key).await;
        }
        let expected = self.entry(key).map_or(Expect::Anything, |s| s.expected);
        self.set(
            key,
            Staged {
                expected,
                content: Content::Delete,
            },
        );
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .list_fingerprinted(prefix)
            .await?
            .into_iter()
            .map(|(key, _)| key)
            .collect())
    }

    /// A staged key has no fingerprint, so the vault rereads it instead of trusting a cache
    /// entry made from what is on disk.
    async fn list_fingerprinted(&self, prefix: &str) -> Result<Vec<(String, Option<u64>)>> {
        let listed = self.inner.list_fingerprinted(prefix).await?;
        Ok(self.overlay(prefix, listed))
    }

    async fn commit(&self, changes: &[Change]) -> Result<()> {
        self.inner.commit(changes).await
    }

    async fn recover(&self) -> Result<()> {
        self.inner.recover().await
    }
}

/// An event whose topic, key and payload were taken when it was recorded, so it can wait for
/// the writes it describes to land.
#[derive(Debug, Clone)]
struct FrozenEvent {
    topic: Topic,
    key: Id,
    namespace: Namespace,
    payload: Payload,
}

impl FrozenEvent {
    fn of(event: &dyn DomainEvent) -> Result<Self> {
        Ok(Self {
            topic: event.topic(),
            key: event.key(),
            namespace: event.namespace(),
            payload: event.payload()?,
        })
    }
}

impl DomainEvent for FrozenEvent {
    fn topic(&self) -> Topic {
        self.topic.clone()
    }

    fn key(&self) -> Id {
        self.key.clone()
    }

    fn namespace(&self) -> Namespace {
        self.namespace.clone()
    }

    fn payload(&self) -> Result<Payload> {
        Ok(self.payload.clone())
    }
}

/// Inside a unit of work, events wait for its writes to land; a unit of work that fails
/// records nothing.
pub struct StagedEventLog {
    inner: Arc<dyn EventLog>,
}

impl StagedEventLog {
    pub fn new(inner: Arc<dyn EventLog>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl EventLog for StagedEventLog {
    async fn append(&self, events: &[Box<dyn DomainEvent>]) -> Result<()> {
        if !is_active() {
            return self.inner.append(events).await;
        }
        let frozen = events
            .iter()
            .map(|e| FrozenEvent::of(e.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        staged(|staging| staging.events.extend(frozen));
        Ok(())
    }

    async fn replay(&self, query: &EventQuery) -> Result<Vec<RecordedEvent>> {
        self.inner.replay(query).await
    }
}

pub struct VaultUnitOfWork {
    vault: Arc<Vault>,
    log: Arc<dyn EventLog>,
}

impl VaultUnitOfWork {
    /// `log` is the log events finally go to, not the staged one in front of it.
    pub fn new(vault: Arc<Vault>, log: Arc<dyn EventLog>) -> Self {
        Self { vault, log }
    }
}

#[async_trait]
impl UnitOfWork for VaultUnitOfWork {
    async fn run<'a>(&self, work: Work<'a>) -> Result<()> {
        atomically(&self.vault, Some(self.log.as_ref()), work).await
    }
}

#[cfg(test)]
mod tests {
    use orchy_core::{Body, EntityKind, Frontmatter};
    use orchy_store_memory::MemoryEventLog;
    use serde_json::json;

    use super::*;
    use crate::blob::MemoryBlobStore;
    use crate::markdown::MarkdownFile;
    use crate::vault::Precondition;

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    fn note(id: &str, body: &str) -> MarkdownFile {
        let mut frontmatter = Frontmatter::new();
        frontmatter.set("id", json!(id));
        frontmatter.set("type", json!("note"));
        MarkdownFile {
            frontmatter,
            body: Body::new(body),
        }
    }

    struct Fixture {
        blobs: Arc<MemoryBlobStore>,
        vault: Arc<Vault>,
        recorded: Arc<MemoryEventLog>,
        log: StagedEventLog,
        unit: VaultUnitOfWork,
    }

    async fn fixture() -> Fixture {
        let blobs = Arc::new(MemoryBlobStore::new());
        let vault = Arc::new(
            Vault::open(Arc::clone(&blobs) as Arc<dyn BlobStore>)
                .await
                .unwrap(),
        );
        let recorded = Arc::new(MemoryEventLog::new());
        Fixture {
            log: StagedEventLog::new(Arc::clone(&recorded) as Arc<dyn EventLog>),
            unit: VaultUnitOfWork::new(Arc::clone(&vault), Arc::clone(&recorded) as _),
            blobs,
            vault,
            recorded,
        }
    }

    async fn write(vault: &Vault, id: &str, body: &str) -> Result<()> {
        let key = format!("docs/{id}.md");
        vault
            .write_if(
                &key,
                &note(id, body),
                &Id::new(id).unwrap(),
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await
    }

    #[derive(Debug)]
    struct Noted;

    impl DomainEvent for Noted {
        fn topic(&self) -> Topic {
            orchy_core::event::topic("document.written")
        }
        fn key(&self) -> Id {
            Id::new(A).unwrap()
        }
        fn namespace(&self) -> Namespace {
            Namespace::root()
        }
        fn payload(&self) -> Result<Payload> {
            orchy_core::event::payload_of(&json!({}))
        }
    }

    async fn recorded(f: &Fixture) -> usize {
        f.recorded
            .replay(&EventQuery::default())
            .await
            .unwrap()
            .len()
    }

    #[tokio::test]
    async fn writes_wait_for_the_unit_and_its_reads_see_them() {
        let f = fixture().await;
        f.unit
            .run(Box::pin(async {
                write(&f.vault, A, "first").await?;
                assert_eq!(f.blobs.get(&format!("docs/{A}.md")).await?, None);
                let seen = f.vault.peek_by_id(&Id::new(A).unwrap()).await?;
                assert!(seen.is_some(), "a unit of work reads what it wrote");
                f.log.append(&[Box::new(Noted)]).await?;
                assert_eq!(recorded(&f).await, 0, "events wait for the writes");
                Ok(())
            }))
            .await
            .unwrap();
        assert!(
            f.blobs
                .get(&format!("docs/{A}.md"))
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(recorded(&f).await, 1);
    }

    #[tokio::test]
    async fn a_unit_that_fails_leaves_nothing_behind() {
        let f = fixture().await;
        let failed = f
            .unit
            .run(Box::pin(async {
                write(&f.vault, A, "first").await?;
                write(&f.vault, B, "second").await?;
                f.log.append(&[Box::new(Noted)]).await?;
                Err(DomainError::validation("changed my mind"))
            }))
            .await;
        assert!(failed.is_err());
        assert!(f.blobs.list("").await.unwrap().is_empty());
        assert_eq!(recorded(&f).await, 0);
        assert!(
            f.vault.locate(&Id::new(A).unwrap()).is_none(),
            "the index forgets what never landed"
        );
    }

    #[tokio::test]
    async fn a_failed_inner_unit_undoes_only_its_own_writes() {
        let f = fixture().await;
        f.unit
            .run(Box::pin(async {
                write(&f.vault, A, "kept").await?;
                let inner = f
                    .unit
                    .run(Box::pin(async {
                        write(&f.vault, B, "dropped").await?;
                        Err(DomainError::validation("inner fails"))
                    }))
                    .await;
                assert!(inner.is_err());
                Ok(())
            }))
            .await
            .unwrap();
        assert!(
            f.blobs
                .get(&format!("docs/{A}.md"))
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(f.blobs.get(&format!("docs/{B}.md")).await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_entity_loaded_and_changed_by_another_writer_makes_the_unit_contended() {
        let f = fixture().await;
        write(&f.vault, A, "first").await.unwrap();
        let landed = f
            .unit
            .run(Box::pin(async {
                f.vault.read_by_id(&Id::new(A).unwrap()).await?;
                f.blobs
                    .put(
                        &format!("docs/{A}.md"),
                        note(A, "theirs").render()?.as_bytes(),
                    )
                    .await?;
                write(&f.vault, B, "decided from the first").await
            }))
            .await;
        assert!(
            matches!(landed, Err(DomainError::Contended(_))),
            "{landed:?}"
        );
        assert_eq!(f.blobs.get(&format!("docs/{B}.md")).await.unwrap(), None);
    }
}
