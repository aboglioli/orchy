use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::Serialize;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

#[derive(Debug, Clone)]
pub struct PageParams {
    pub after: Option<String>,
    pub limit: u32,
}

impl PageParams {
    pub fn new(after: Option<String>, limit: Option<u32>) -> Self {
        Self {
            after,
            limit: limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT),
        }
    }

    pub fn unbounded() -> Self {
        Self {
            after: None,
            limit: u32::MAX,
        }
    }
}

impl Default for PageParams {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T: Serialize> Page<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }

    pub fn empty() -> Self {
        Self {
            items: vec![],
            next_cursor: None,
        }
    }
}

pub fn encode_cursor(id: &str) -> String {
    BASE64.encode(id.as_bytes())
}

pub fn decode_cursor(cursor: &str) -> Option<String> {
    BASE64
        .decode(cursor)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip() {
        let id = "0192f3e4-4a3b-7c8d-9e0f-1a2b3c4d5e6f";
        let encoded = encode_cursor(id);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(id, decoded);
    }

    #[test]
    fn invalid_cursor_returns_none() {
        assert!(decode_cursor("!!!invalid!!!").is_none());
    }

    #[test]
    fn page_params_clamps_limit() {
        let params = PageParams::new(None, Some(999));
        assert_eq!(params.limit, MAX_LIMIT);
    }

    #[test]
    fn page_params_default_limit() {
        let params = PageParams::new(None, None);
        assert_eq!(params.limit, DEFAULT_LIMIT);
    }

    #[test]
    fn page_params_unbounded() {
        let params = PageParams::unbounded();
        assert_eq!(params.limit, u32::MAX);
        assert!(params.after.is_none());
    }
}
