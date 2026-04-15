use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnMode {
    #[default]
    Pty,
    Pipe,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub spawn_mode: SpawnMode,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    #[serde(default = "default_rows")]
    pub pty_rows: u16,
    #[serde(default = "default_cols")]
    pub pty_cols: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrchyConfig {
    #[serde(default = "default_orchy_url")]
    pub url: String,
    pub project: String,
    pub description: String,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunnerConfig {
    pub agent: AgentConfig,
    pub orchy: OrchyConfig,
}

impl RunnerConfig {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: RunnerConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.orchy.poll_interval_secs)
    }

    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.orchy.heartbeat_interval_secs)
    }
}

fn default_orchy_url() -> String {
    "http://127.0.0.1:3100/mcp".to_string()
}

fn default_poll_interval_secs() -> u64 {
    10
}

fn default_heartbeat_interval_secs() -> u64 {
    30
}

fn default_rows() -> u16 {
    24
}

fn default_cols() -> u16 {
    120
}
