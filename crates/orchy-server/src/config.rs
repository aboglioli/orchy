use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub store: StoreConfig,
    pub embeddings: Option<EmbeddingsConfig>,
    pub skills: Option<SkillsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SkillsConfig {
    pub dir: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout_secs: u64,
    /// MCP Streamable HTTP idle timeout (seconds). Omitted or 0 disables (orchy default; avoids
    /// session loss between sparse client calls). Set e.g. 300 for rmcp-style zombie cleanup.
    #[serde(default)]
    pub mcp_session_keep_alive_secs: Option<u64>,
}

impl ServerConfig {
    pub fn mcp_session_keep_alive(&self) -> Option<Duration> {
        match self.mcp_session_keep_alive_secs {
            None | Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
        }
    }
}

fn default_heartbeat_timeout() -> u64 {
    300
}

#[derive(Debug, Deserialize)]
pub struct StoreConfig {
    pub backend: String,
    pub sqlite: Option<SqliteConfig>,
    pub postgres: Option<PostgresConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SqliteConfig {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingsConfig {
    pub provider: String,
    pub openai: Option<OpenAiEmbeddingsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiEmbeddingsConfig {
    pub url: String,
    pub model: String,
    pub dimensions: u32,
}
