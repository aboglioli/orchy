use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    #[serde(rename = "application/json")]
    Json,
    #[serde(rename = "text/plain")]
    PlainText,
    #[serde(rename = "application/octet-stream")]
    Binary,
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentType::Json => write!(f, "application/json"),
            ContentType::PlainText => write!(f, "text/plain"),
            ContentType::Binary => write!(f, "application/octet-stream"),
        }
    }
}

impl std::str::FromStr for ContentType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "application/json" => Ok(ContentType::Json),
            "text/plain" => Ok(ContentType::PlainText),
            "application/octet-stream" => Ok(ContentType::Binary),
            other => Err(Error::InvalidPayload(format!("unknown content type: {other}"))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Payload {
    data: Vec<u8>,
    content_type: ContentType,
}

impl Payload {
    pub fn from_json<T: Serialize>(val: &T) -> Result<Self> {
        let data = serde_json::to_vec(val).map_err(|e| Error::Serialization(e.to_string()))?;
        Ok(Self {
            data,
            content_type: ContentType::Json,
        })
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self {
            data: s.into().into_bytes(),
            content_type: ContentType::PlainText,
        }
    }

    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self {
            data,
            content_type: ContentType::Binary,
        }
    }

    pub fn from_raw(data: Vec<u8>, content_type: ContentType) -> Self {
        Self { data, content_type }
    }

    pub fn to_json<T: DeserializeOwned>(&self) -> Result<T> {
        if self.content_type != ContentType::Json {
            return Err(Error::InvalidPayload("not JSON content".into()));
        }
        serde_json::from_slice(&self.data).map_err(|e| Error::Serialization(e.to_string()))
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn content_type(&self) -> ContentType {
        self.content_type
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }
}
