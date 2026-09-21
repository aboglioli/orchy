use std::path::PathBuf;

use orchy_core::{ActorId, DomainError, Id, ReadWatermarks, Result};

/// Per actor, per machine, never tracked: a broadcast to six agents would otherwise put six
/// writers on one file and record that agents looked at things.
pub struct FileWatermarks {
    root: PathBuf,
}

impl FileWatermarks {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, actor: &ActorId) -> PathBuf {
        self.root.join(format!("{actor}.json"))
    }
}

impl ReadWatermarks for FileWatermarks {
    fn watermark(&self, actor: &ActorId) -> Result<Option<Id>> {
        let Ok(bytes) = std::fs::read(self.path(actor)) else {
            return Ok(None);
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(None);
        };
        Ok(value
            .get("watermark")
            .and_then(|v| v.as_str())
            .and_then(|s| Id::new(s).ok()))
    }

    fn advance(&self, actor: &ActorId, to: &Id) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .map_err(|e| DomainError::validation(format!("creating watermark directory: {e}")))?;
        let value = serde_json::json!({ "actor": actor.to_string(), "watermark": to.to_string() });
        std::fs::write(self.path(actor), value.to_string())
            .map_err(|e| DomainError::validation(format!("writing watermark: {e}")))
    }
}
