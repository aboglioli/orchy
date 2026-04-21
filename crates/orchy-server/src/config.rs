use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub store: StoreConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    pub embeddings: Option<EmbeddingsConfig>,
    pub skills: Option<SkillsConfig>,
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.server.validate()?;
        self.store.validate()?;
        self.auth.validate()?;
        if let Some(ref embeddings) = self.embeddings {
            embeddings.validate()?;
        }
        if let Some(ref skills) = self.skills {
            skills.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct SkillsConfig {
    pub dir: String,
}

impl SkillsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.dir.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "skills.dir".into(),
                message: "skills directory path must not be empty".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout_secs: u64,
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

    fn validate(&self) -> Result<(), ConfigError> {
        if self.host.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "server.host".into(),
                message: "host must not be empty".into(),
            });
        }

        if self.port == 0 {
            return Err(ConfigError::InvalidField {
                field: "server.port".into(),
                message: "port must not be 0 (use 1-65535)".into(),
            });
        }

        if self.heartbeat_timeout_secs < 5 {
            return Err(ConfigError::InvalidField {
                field: "server.heartbeat_timeout_secs".into(),
                message: format!(
                    "heartbeat_timeout_secs must be at least 5 (got {})",
                    self.heartbeat_timeout_secs
                ),
            });
        }

        if let Some(keep_alive) = self.mcp_session_keep_alive_secs && keep_alive > 86400 {
            return Err(ConfigError::InvalidField {
                field: "server.mcp_session_keep_alive_secs".into(),
                message: "mcp_session_keep_alive_secs should not exceed 86400 (24 hours)".into(),
            });
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

impl StoreConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        let backend = self.backend.to_lowercase();
        match backend.as_str() {
            "sqlite" => {
                let Some(ref sqlite) = self.sqlite else {
                    return Err(ConfigError::MissingField {
                        section: "store.sqlite".into(),
                        message: "backend is 'sqlite' but [store.sqlite] section is missing".into(),
                    });
                };
                sqlite.validate()?;
            }
            "postgres" => {
                let Some(ref postgres) = self.postgres else {
                    return Err(ConfigError::MissingField {
                        section: "store.postgres".into(),
                        message: "backend is 'postgres' but [store.postgres] section is missing".into(),
                    });
                };
                postgres.validate()?;
            }
            "memory" => {
                // Memory backend requires no additional config
            }
            _ => {
                return Err(ConfigError::InvalidField {
                    field: "store.backend".into(),
                    message: format!(
                        "invalid backend '{}': must be 'sqlite', 'postgres', or 'memory'",
                        self.backend
                    ),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct SqliteConfig {
    pub path: String,
}

impl SqliteConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.path.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "store.sqlite.path".into(),
                message: "sqlite database path must not be empty".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
}

impl PostgresConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "store.postgres.url".into(),
                message: "postgres connection url must not be empty".into(),
            });
        }

        // Basic URL format validation
        if !self.url.starts_with("postgres://") && !self.url.starts_with("postgresql://") {
            return Err(ConfigError::InvalidField {
                field: "store.postgres.url".into(),
                message: "postgres url must start with 'postgres://' or 'postgresql://'".into(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingsConfig {
    pub provider: String,
    pub openai: Option<OpenAiEmbeddingsConfig>,
}

impl EmbeddingsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        let provider = self.provider.to_lowercase();
        match provider.as_str() {
            "openai" => {
                let Some(ref openai) = self.openai else {
                    return Err(ConfigError::MissingField {
                        section: "embeddings.openai".into(),
                        message: "embeddings.provider is 'openai' but [embeddings.openai] section is missing".into(),
                    });
                };
                openai.validate()?;
            }
            _ => {
                return Err(ConfigError::InvalidField {
                    field: "embeddings.provider".into(),
                    message: format!(
                        "invalid embeddings provider '{}': currently only 'openai' is supported",
                        self.provider
                    ),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct OpenAiEmbeddingsConfig {
    pub url: String,
    pub model: String,
    pub dimensions: u32,
}

impl OpenAiEmbeddingsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "embeddings.openai.url".into(),
                message: "embeddings.openai.url must not be empty".into(),
            });
        }

        if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            return Err(ConfigError::InvalidField {
                field: "embeddings.openai.url".into(),
                message: "embeddings.openai.url must start with 'http://' or 'https://'".into(),
            });
        }

        if self.model.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "embeddings.openai.model".into(),
                message: "embeddings.openai.model must not be empty".into(),
            });
        }

        if self.dimensions == 0 {
            return Err(ConfigError::InvalidField {
                field: "embeddings.openai.dimensions".into(),
                message: "embeddings.openai.dimensions must be greater than 0".into(),
            });
        }

        // Common embedding model dimension limits
        if self.dimensions > 8192 {
            return Err(ConfigError::InvalidField {
                field: "embeddings.openai.dimensions".into(),
                message: "embeddings.openai.dimensions exceeds maximum supported (8192)".into(),
            });
        }

        Ok(())
    }
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

impl AuthConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.jwt_duration_hours <= 0 {
            return Err(ConfigError::InvalidField {
                field: "auth.jwt_duration_hours".into(),
                message: "jwt_duration_hours must be greater than 0".into(),
            });
        }

        if self.jwt_duration_hours > 8760 {
            // More than 1 year
            return Err(ConfigError::InvalidField {
                field: "auth.jwt_duration_hours".into(),
                message: "jwt_duration_hours should not exceed 8760 (1 year)".into(),
            });
        }

        if self.bcrypt_cost < 4 || self.bcrypt_cost > 31 {
            return Err(ConfigError::InvalidField {
                field: "auth.bcrypt_cost".into(),
                message: format!(
                    "bcrypt_cost must be between 4 and 31 (got {})",
                    self.bcrypt_cost
                ),
            });
        }

        if self.keys_dir.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "auth.keys_dir".into(),
                message: "auth.keys_dir must not be empty".into(),
            });
        }

        Ok(())
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

/// Configuration validation errors with detailed context.
#[derive(Debug, Clone)]
pub enum ConfigError {
    InvalidField {
        field: String,
        message: String,
    },
    MissingField {
        section: String,
        message: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidField { field, message } => {
                write!(f, "invalid configuration for '{}': {}", field, message)
            }
            ConfigError::MissingField { section, message } => {
                write!(f, "missing configuration in [{}]: {}", section, message)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Result type alias for config operations.
pub type ConfigResult<T> = Result<T, ConfigError>;
