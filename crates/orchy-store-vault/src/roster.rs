use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use orchy_core::{
    Actor, ActorId, ActorStore, Clock, DomainError, Lease, LeaseStore, ResourceKey, Result,
};

use sha2::{Digest, Sha256};

use crate::codec;
use crate::lock::{DEFAULT_WAIT, FileLock};
use crate::vault::Vault;

const PRESENCE_TTL_SECS: i64 = 300;

pub struct VaultActorStore {
    vault: Arc<Vault>,
}

impl VaultActorStore {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl ActorStore for VaultActorStore {
    async fn get(&self, id: &ActorId) -> Result<Option<Actor>> {
        let key = self.vault.layout().actor_key(id);
        let Some(file) = self.vault.read(&key).await? else {
            return Ok(None);
        };
        codec::actor_from_markdown(&file, &key).map(Some)
    }

    async fn roster(&self) -> Result<Vec<Actor>> {
        let mut actors = Vec::new();
        for key in self.vault.blobs().list(crate::layout::AGENTS).await? {
            if !self.vault.layout().is_markdown(&key) {
                continue;
            }
            if let Some(file) = self.vault.read(&key).await? {
                actors.push(codec::actor_from_markdown(&file, &key)?);
            }
        }
        actors.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(actors)
    }

    async fn save(&self, actor: &mut Actor) -> Result<()> {
        let key = self.vault.layout().actor_key(actor.id());
        let carried = match self.vault.read(&key).await? {
            Some(file) => codec::carried_frontmatter(&file),
            None => Default::default(),
        };
        let file = codec::actor_to_markdown(actor, carried);
        let bytes = file.render()?;
        self.vault.blobs().put(&key, bytes.as_bytes()).await?;

        let presence = self.vault.layout().presence_key(actor.id());
        let stamp = serde_json::json!({
            "actor": actor.id().to_string(),
            "last_seen": actor.last_seen().to_rfc3339(),
        });
        self.vault
            .blobs()
            .put(&presence, stamp.to_string().as_bytes())
            .await
    }

    async fn present(&self, now: DateTime<Utc>) -> Result<Vec<ActorId>> {
        let cutoff = now - Duration::seconds(PRESENCE_TTL_SECS);
        let mut present = Vec::new();
        for key in self
            .vault
            .blobs()
            .list(&format!("{}/presence", crate::layout::RUNTIME))
            .await?
        {
            let Some(bytes) = self.vault.blobs().get(&key).await? else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            let (Some(actor), Some(seen)) = (
                value.get("actor").and_then(|v| v.as_str()),
                value.get("last_seen").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            let Ok(seen) = DateTime::parse_from_rfc3339(seen) else {
                continue;
            };
            if seen.with_timezone(&Utc) >= cutoff
                && let Ok(id) = actor.parse::<ActorId>()
            {
                present.push(id);
            }
        }
        present.sort();
        Ok(present)
    }

    async fn touch(&self, id: &ActorId, now: DateTime<Utc>) -> Result<()> {
        let mut actor = self
            .get(id)
            .await?
            .ok_or_else(|| DomainError::not_found("actor", id))?;
        actor.seen_at(now);
        self.save(&mut actor).await
    }
}

/// The `flock` orders concurrent acquires; the TTL record outlives the process that took it,
/// so a holder that dies does not block the resource forever.
pub struct FileLeaseStore {
    root: PathBuf,
    clock: Arc<dyn Clock>,
}

impl FileLeaseStore {
    pub fn new(root: impl Into<PathBuf>, clock: Arc<dyn Clock>) -> Self {
        Self {
            root: root.into(),
            clock,
        }
    }

    /// Readable, and distinct for distinct keys. Folding every other character to `-` alone
    /// put `deploy/prod` and `deploy-prod` on one file, so one agent's lock refused a resource
    /// nobody held; the digest of the whole key is what keeps them apart.
    fn lock_path(&self, key: &ResourceKey) -> PathBuf {
        let readable: String = key
            .as_str()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let digest = Sha256::digest(key.as_str().as_bytes());
        self.root
            .join(format!("{readable}-{}.lock", hex::encode(&digest[..4])))
    }

    fn read_record(&self, key: &ResourceKey) -> Option<LeaseRecord> {
        let bytes = std::fs::read(self.lock_path(key)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Never unlinked, only rewritten: `flock` orders the holders of one inode, so removing
    /// the file would let the next two acquirers lock two inodes and both believe they won.
    fn open_lock(&self, key: &ResourceKey) -> Result<FileLock> {
        FileLock::exclusive(&self.lock_path(key), key.as_str(), DEFAULT_WAIT)
    }

    fn write_record(&self, key: &ResourceKey, record: &LeaseRecord) -> Result<()> {
        std::fs::write(
            self.lock_path(key),
            serde_json::to_vec(record).unwrap_or_default(),
        )
        .map_err(|e| DomainError::validation(format!("writing lock: {e}")))
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct LeaseRecord {
    // the filename is a digest, so the key it stands for has to be written down
    resource: String,
    holder: String,
    acquired_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    generation: u64,
}

#[async_trait]
impl LeaseStore for FileLeaseStore {
    async fn acquire(&self, key: &ResourceKey, by: &ActorId, ttl: Duration) -> Result<Lease> {
        let now = self.clock.now();
        let _guard = self.open_lock(key)?;

        let held = self.read_record(key);
        if let Some(record) = &held
            && record.expires_at > now
            && record.holder != by.to_string()
        {
            return Err(DomainError::conflict(format!(
                "`{key}` is held by {} until {}",
                record.holder, record.expires_at
            )));
        }

        let generation = held.as_ref().map_or(0, |r| r.generation) + 1;
        let record = LeaseRecord {
            resource: key.to_string(),
            holder: by.to_string(),
            acquired_at: now,
            expires_at: now + ttl,
            generation,
        };
        self.write_record(key, &record)?;

        Ok(Lease::new(
            key.clone(),
            by.clone(),
            record.acquired_at,
            record.expires_at,
            generation,
        ))
    }

    async fn renew(&self, key: &ResourceKey, by: &ActorId, ttl: Duration) -> Result<Lease> {
        let now = self.clock.now();
        let _guard = self.open_lock(key)?;

        let Some(record) = self.read_record(key).filter(|r| r.expires_at > now) else {
            return Err(DomainError::conflict(format!(
                "`{key}` is not held; acquire it rather than renewing it"
            )));
        };
        if record.holder != by.to_string() {
            return Err(DomainError::forbidden(format!(
                "`{key}` is held by {}, not {by}",
                record.holder
            )));
        }

        let renewed = LeaseRecord {
            expires_at: now + ttl,
            ..record
        };
        self.write_record(key, &renewed)?;
        Ok(Lease::new(
            key.clone(),
            by.clone(),
            renewed.acquired_at,
            renewed.expires_at,
            renewed.generation,
        ))
    }

    async fn release(&self, key: &ResourceKey, by: &ActorId) -> Result<()> {
        if !self.lock_path(key).exists() {
            return Ok(());
        }
        let _guard = self.open_lock(key)?;
        let now = self.clock.now();

        match self.read_record(key) {
            None => Ok(()),
            Some(record) if record.holder != by.to_string() && record.expires_at > now => Err(
                DomainError::forbidden(format!("`{key}` is held by {}, not {by}", record.holder)),
            ),
            Some(record) => self.write_record(
                key,
                &LeaseRecord {
                    expires_at: now,
                    ..record
                },
            ),
        }
    }

    async fn check(&self, key: &ResourceKey) -> Result<Option<Lease>> {
        let Some(record) = self.read_record(key) else {
            return Ok(None);
        };
        if record.expires_at <= self.clock.now() {
            return Ok(None);
        }
        let Ok(holder) = record.holder.parse::<ActorId>() else {
            return Ok(None);
        };
        Ok(Some(Lease::new(
            key.clone(),
            holder,
            record.acquired_at,
            record.expires_at,
            record.generation,
        )))
    }

    async fn held(&self) -> Result<Vec<Lease>> {
        let now = self.clock.now();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Ok(Vec::new());
        };

        let mut held: Vec<Lease> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "lock"))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<LeaseRecord>(&bytes).ok())
            .filter(|record| record.expires_at > now)
            .filter_map(|record| {
                Some(Lease::new(
                    ResourceKey::new(&record.resource).ok()?,
                    record.holder.parse().ok()?,
                    record.acquired_at,
                    record.expires_at,
                    record.generation,
                ))
            })
            .collect();
        held.sort_by(|a, b| a.resource().as_str().cmp(b.resource().as_str()));
        Ok(held)
    }
}

#[cfg(test)]
mod lease_tests {
    use super::*;
    use crate::time::SystemClock;
    use std::path::Path;

    fn store(root: &Path) -> FileLeaseStore {
        FileLeaseStore::new(root.join("locks"), Arc::new(SystemClock))
    }

    fn actor(alias: &str) -> ActorId {
        ActorId::new(alias, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    fn key(name: &str) -> ResourceKey {
        ResourceKey::new(name).unwrap()
    }

    #[tokio::test]
    async fn keys_that_look_alike_once_sanitised_are_still_separate_resources() {
        let temp = tempfile::tempdir().unwrap();
        let leases = store(temp.path());

        leases
            .acquire(&key("deploy/prod"), &actor("claude"), Duration::seconds(60))
            .await
            .unwrap();
        leases
            .acquire(&key("deploy-prod"), &actor("codex"), Duration::seconds(60))
            .await
            .expect("a different resource is not the same lock");

        assert_eq!(
            leases
                .check(&key("deploy/prod"))
                .await
                .unwrap()
                .unwrap()
                .holder(),
            &actor("claude")
        );
        assert_eq!(
            leases
                .check(&key("deploy-prod"))
                .await
                .unwrap()
                .unwrap()
                .holder(),
            &actor("codex")
        );
    }

    #[tokio::test]
    async fn renewing_extends_the_lease_without_spending_its_generation() {
        let temp = tempfile::tempdir().unwrap();
        let leases = store(temp.path());
        let taken = leases
            .acquire(&key("build"), &actor("claude"), Duration::seconds(1))
            .await
            .unwrap();

        let renewed = leases
            .renew(&key("build"), &actor("claude"), Duration::seconds(600))
            .await
            .unwrap();

        assert_eq!(
            renewed.generation(),
            taken.generation(),
            "the holder's fencing token survives its own renewal"
        );
        assert!(renewed.expires_at() > taken.expires_at());
    }

    #[tokio::test]
    async fn only_the_holder_renews_and_only_what_is_still_held() {
        let temp = tempfile::tempdir().unwrap();
        let leases = store(temp.path());
        leases
            .acquire(&key("build"), &actor("claude"), Duration::seconds(60))
            .await
            .unwrap();

        let stolen = leases
            .renew(&key("build"), &actor("codex"), Duration::seconds(60))
            .await;
        assert!(
            matches!(stolen, Err(DomainError::Forbidden(_))),
            "{stolen:?}"
        );

        let absent = leases
            .renew(&key("untouched"), &actor("claude"), Duration::seconds(60))
            .await;
        assert!(
            matches!(absent, Err(DomainError::Conflict(_))),
            "{absent:?}"
        );
    }

    #[tokio::test]
    async fn held_reports_what_is_live_and_forgets_what_lapsed() {
        let temp = tempfile::tempdir().unwrap();
        let leases = store(temp.path());
        leases
            .acquire(&key("alpha"), &actor("claude"), Duration::seconds(600))
            .await
            .unwrap();
        leases
            .acquire(&key("beta"), &actor("codex"), Duration::seconds(-1))
            .await
            .unwrap();

        let held = leases.held().await.unwrap();
        let names: Vec<&str> = held.iter().map(|l| l.resource().as_str()).collect();
        assert_eq!(names, vec!["alpha"], "an expired lease is held by nobody");
        assert_eq!(held[0].holder(), &actor("claude"));
    }

    #[tokio::test]
    async fn a_released_resource_is_reported_free_and_listed_by_nobody() {
        let temp = tempfile::tempdir().unwrap();
        let leases = store(temp.path());
        leases
            .acquire(&key("build"), &actor("claude"), Duration::seconds(600))
            .await
            .unwrap();
        leases
            .release(&key("build"), &actor("claude"))
            .await
            .unwrap();

        assert!(leases.check(&key("build")).await.unwrap().is_none());
        assert!(leases.held().await.unwrap().is_empty());
    }
}
