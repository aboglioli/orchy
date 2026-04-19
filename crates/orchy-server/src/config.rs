use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub store: StoreConfig,
    pub auth: AuthConfig,
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

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.server.heartbeat_timeout_secs < 5 {
            return Err(format!(
                "heartbeat_timeout_secs must be at least 5 (got {})",
                self.server.heartbeat_timeout_secs
            ));
        }
        Ok(())
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

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_jwt_duration_hours")]
    pub jwt_duration_hours: i64,
    #[serde(default = "default_cookie_secure")]
    pub cookie_secure: bool,
    #[serde(default = "default_bcrypt_cost")]
    pub bcrypt_cost: u32,
    #[serde(default = "default_keys_dir")]
    pub keys_dir: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_duration_hours: default_jwt_duration_hours(),
            cookie_secure: default_cookie_secure(),
            bcrypt_cost: default_bcrypt_cost(),
            keys_dir: default_keys_dir(),
        }
    }
}

fn default_jwt_duration_hours() -> i64 {
    24
}

fn default_cookie_secure() -> bool {
    false
}

fn default_bcrypt_cost() -> u32 {
    10
}

fn default_keys_dir() -> String {
    "keys".to_string()
}
