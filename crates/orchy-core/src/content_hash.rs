use sha2::{Digest, Sha256};

use crate::document::Frontmatter;
use crate::error::{DomainError, Result};

pub(crate) fn content_hash(parts: &[(&str, &str)], frontmatter: &Frontmatter) -> String {
    let mut hasher = Sha256::new();
    for (at, (label, text)) in parts.iter().enumerate() {
        if at > 0 {
            hasher.update(b"\x00");
        }
        hasher.update(label.as_bytes());
        hasher.update(b"\x00");
        hasher.update(text.as_bytes());
    }
    for (key, value) in frontmatter.iter() {
        hasher.update(b"\x00field\x00");
        hasher.update(key.as_bytes());
        hasher.update(b"\x00");
        hasher.update(value.to_string().as_bytes());
    }
    hex::encode(hasher.finalize())
}

pub(crate) fn ensure_matches(actual: &str, expected: Option<&str>) -> Result<()> {
    match expected {
        Some(expected) if expected != actual => Err(DomainError::conflict(format!(
            "changed since it was read (expected {expected}, found {actual})"
        ))),
        _ => Ok(()),
    }
}
