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
}

fn default_heartbeat_timeout() -> u64 {
    60
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
