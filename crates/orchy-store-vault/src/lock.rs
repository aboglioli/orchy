use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use orchy_core::{DomainError, Result};

pub const DEFAULT_WAIT: Duration = Duration::from_secs(10);

const POLL: Duration = Duration::from_millis(5);

/// An advisory lock on a file, released when it is dropped.
///
/// `flock` has no timed variant, so every acquisition polls to a deadline: an agent that dies
/// holding a lock delays the next one rather than stranding it, which is what rules out a
/// deadlock between the vault's locks and the event log's.
#[must_use = "dropping the lock releases the file to other processes"]
pub struct FileLock {
    file: File,
}

impl FileLock {
    pub fn exclusive(path: &Path, resource: &str, wait: Duration) -> Result<Self> {
        Self::acquire(path, resource, wait, false)
    }

    pub fn shared(path: &Path, resource: &str, wait: Duration) -> Result<Self> {
        Self::acquire(path, resource, wait, true)
    }

    /// The same handle the lock is held on, so a caller can read and write the file it locked
    /// without opening a second one the lock would not cover.
    pub fn file(&self) -> &File {
        &self.file
    }

    fn acquire(path: &Path, resource: &str, wait: Duration, shared: bool) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DomainError::validation(format!("creating {}: {e}", parent.display()))
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|e| DomainError::validation(format!("opening {resource}: {e}")))?;

        let deadline = Instant::now() + wait;
        loop {
            let taken = if shared {
                file.try_lock_shared()
            } else {
                file.try_lock()
            };
            match taken {
                Ok(()) => return Ok(Self { file }),
                Err(TryLockError::WouldBlock) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(DomainError::conflict(format!(
                            "{resource} is held by another process"
                        )));
                    }
                    sleep(POLL.min(left));
                }
                Err(TryLockError::Error(e)) => {
                    return Err(DomainError::validation(format!("locking {resource}: {e}")));
                }
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_holder_waits_only_as_long_as_it_was_told_to() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("guard.lock");
        let _held = FileLock::exclusive(&path, "guard", DEFAULT_WAIT).unwrap();

        let started = Instant::now();
        let refused = FileLock::exclusive(&path, "guard", Duration::from_millis(50))
            .err()
            .expect("a held guard must refuse a second holder");

        assert!(
            matches!(refused, DomainError::Conflict(_)),
            "a busy resource is a refusal, not a broken one: {refused:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the wait is bounded by what the caller asked for"
        );
    }

    #[test]
    fn dropping_hands_the_file_to_the_next_caller() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("guard.lock");

        drop(FileLock::exclusive(&path, "guard", DEFAULT_WAIT).unwrap());
        assert!(FileLock::exclusive(&path, "guard", Duration::from_millis(50)).is_ok());
    }

    #[test]
    fn readers_share_what_a_writer_would_not() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("marks.json");

        let _first = FileLock::shared(&path, "watermark", DEFAULT_WAIT).unwrap();
        assert!(
            FileLock::shared(&path, "watermark", Duration::from_millis(50)).is_ok(),
            "two readers do not exclude each other"
        );
        assert!(
            FileLock::exclusive(&path, "watermark", Duration::from_millis(50)).is_err(),
            "but a writer waits for them"
        );
    }
}
