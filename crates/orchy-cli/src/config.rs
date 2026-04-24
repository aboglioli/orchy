use std::path::PathBuf;

use serde::Deserialize;

/// Resolved CLI configuration after layering: global file → repo-local file → env → flags.
#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub api_key: String,
    pub org: Option<String>,
    pub project: String,
    pub namespace: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub roles: Vec<String>,
    pub json: bool,
}

/// File-level config schema (shared by ~/.orchy/config.toml and .orchy.toml).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FileConfig {
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub org: Option<String>,
    pub project: Option<String>,
    pub namespace: Option<String>,
    pub alias: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

/// CLI config validation errors.
#[derive(Debug, Clone)]
pub enum ConfigError {
    MissingField { field: String, source: String },
    InvalidField { field: String, message: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingField { field, source } => {
                write!(f, "{field} is required — set it in {source}")
            }
            ConfigError::InvalidField { field, message } => {
                write!(f, "invalid {field}: {message}")
            }
        }
    }
}

impl Config {
    /// Resolve config from all layers:
    /// 1. Global file (~/.orchy/config.toml)
    /// 2. Repo-local file (.orchy.toml, walked up from cwd)
    /// 3. Environment variables
    /// 4. CLI flags
    pub fn resolve(
        flag_url: Option<&str>,
        flag_api_key: Option<&str>,
        flag_org: Option<&str>,
        flag_project: Option<&str>,
        flag_namespace: Option<&str>,
        flag_agent: Option<&str>,
        json: bool,
        requires_api_key: bool,
        requires_project: bool,
    ) -> Result<Self, ConfigError> {
        let global = read_global_config();
        let local = read_repo_config();

        // Layer: global → local → env → flags (last wins)
        let url = pick(
            &[
                global.as_ref().and_then(|c| c.url.as_deref()),
                local.as_ref().and_then(|c| c.url.as_deref()),
                env("ORCHY_URL"),
                flag_url,
            ],
            "url",
            "ORCHY_URL",
            "config file, env (ORCHY_URL), or --url",
        )?;

        let api_key = if requires_api_key {
            pick(
                &[
                    global.as_ref().and_then(|c| c.api_key.as_deref()),
                    local.as_ref().and_then(|c| c.api_key.as_deref()),
                    env("ORCHY_API_KEY"),
                    flag_api_key,
                ],
                "api_key",
                "ORCHY_API_KEY",
                "config file, env (ORCHY_API_KEY), or --api-key",
            )?
        } else {
            pick_opt(&[
                global.as_ref().and_then(|c| c.api_key.as_deref()),
                local.as_ref().and_then(|c| c.api_key.as_deref()),
                env("ORCHY_API_KEY"),
                flag_api_key,
            ])
            .unwrap_or_default()
        };

        let org = pick_opt(&[
            global.as_ref().and_then(|c| c.org.as_deref()),
            local.as_ref().and_then(|c| c.org.as_deref()),
            env("ORCHY_ORG"),
            flag_org,
        ]);

        let project = if requires_project {
            pick(
                &[
                    global.as_ref().and_then(|c| c.project.as_deref()),
                    local.as_ref().and_then(|c| c.project.as_deref()),
                    env("ORCHY_PROJECT"),
                    flag_project,
                ],
                "project",
                "ORCHY_PROJECT",
                "config file, env (ORCHY_PROJECT), or --project",
            )?
        } else {
            pick_opt(&[
                global.as_ref().and_then(|c| c.project.as_deref()),
                local.as_ref().and_then(|c| c.project.as_deref()),
                env("ORCHY_PROJECT"),
                flag_project,
            ])
            .unwrap_or_default()
        };

        let namespace = pick_opt(&[
            global.as_ref().and_then(|c| c.namespace.as_deref()),
            local.as_ref().and_then(|c| c.namespace.as_deref()),
            env("ORCHY_NAMESPACE"),
            flag_namespace,
        ])
        .unwrap_or_else(|| "/".to_string());

        let alias = pick_opt(&[
            global.as_ref().and_then(|c| c.alias.as_deref()),
            local.as_ref().and_then(|c| c.alias.as_deref()),
            env("ORCHY_ALIAS"),
            flag_agent,
        ]);

        let description = pick_opt(&[
            global.as_ref().and_then(|c| c.description.as_deref()),
            local.as_ref().and_then(|c| c.description.as_deref()),
        ]);

        let roles = local
            .as_ref()
            .filter(|c| !c.roles.is_empty())
            .or(global.as_ref().filter(|c| !c.roles.is_empty()))
            .map(|c| c.roles.clone())
            .unwrap_or_default();

        let config = Config {
            url: url.clone(),
            api_key: api_key.clone(),
            org,
            project: project.clone(),
            namespace: namespace.clone(),
            alias,
            description,
            roles,
            json,
        };

        // Validate resolved values
        config.validate(requires_api_key, requires_project)?;

        Ok(config)
    }

    fn validate(&self, requires_api_key: bool, requires_project: bool) -> Result<(), ConfigError> {
        // URL validation
        if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            return Err(ConfigError::InvalidField {
                field: "url".into(),
                message: "must start with 'http://' or 'https://'".into(),
            });
        }

        // Basic URL structure check
        if !self.url.contains("://") || self.url.ends_with("://") {
            return Err(ConfigError::InvalidField {
                field: "url".into(),
                message: "must be a valid URL (e.g., http://localhost:3100)".into(),
            });
        }

        // API key validation
        if requires_api_key && self.api_key.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "api_key".into(),
                message: "must not be empty".into(),
            });
        }

        // Project validation
        if requires_project && self.project.is_empty() {
            return Err(ConfigError::InvalidField {
                field: "project".into(),
                message: "must not be empty".into(),
            });
        }

        if !self.project.is_empty() && self.project.len() > 64 {
            return Err(ConfigError::InvalidField {
                field: "project".into(),
                message: "must be 64 characters or less".into(),
            });
        }

        // Namespace validation
        if !self.namespace.is_empty() && self.namespace != "/" {
            if !self.namespace.starts_with('/') {
                return Err(ConfigError::InvalidField {
                    field: "namespace".into(),
                    message: "must start with '/' (e.g., '/backend' or '/')".into(),
                });
            }

            // Check for valid namespace characters
            let ns = &self.namespace[1..]; // Skip leading '/'
            if !ns
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/')
            {
                return Err(ConfigError::InvalidField {
                    field: "namespace".into(),
                    message: "contains invalid characters (use alphanumeric, '-', '_', '/')".into(),
                });
            }
        }

        Ok(())
    }
}

fn env(key: &str) -> Option<&'static str> {
    std::env::var(key).ok().map(|s| s.leak() as &_)
}

fn pick(
    opts: &[Option<&str>],
    name: &str,
    _env_var: &str,
    source_hint: &str,
) -> Result<String, ConfigError> {
    opts.iter()
        .rev()
        .find_map(|o| *o)
        .map(|s| s.to_string())
        .ok_or_else(|| ConfigError::MissingField {
            field: name.to_string(),
            source: source_hint.to_string(),
        })
}

fn pick_opt(opts: &[Option<&str>]) -> Option<String> {
    opts.iter().rev().find_map(|o| *o).map(|s| s.to_string())
}

fn read_global_config() -> Option<FileConfig> {
    let home = dirs::home_dir()?;
    let path = home.join(".orchy").join("config.toml");
    read_toml_file(&path)
}

fn read_repo_config() -> Option<FileConfig> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let path = dir.join(".orchy.toml");
        if path.is_file() {
            return read_toml_file(&path);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn read_toml_file(path: &PathBuf) -> Option<FileConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// Write or update `alias` in the nearest `.orchy.toml`.
/// If no `.orchy.toml` exists, creates one in the current directory.
pub fn save_alias(alias: &str) {
    let path = find_repo_config_path().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".orchy.toml")
    });

    let content = std::fs::read_to_string(&path).unwrap_or_default();

    let updated = if content.contains("alias") {
        // Replace existing alias line
        content
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("alias") {
                    format!("alias  = \"{alias}\"")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    } else {
        // Append alias
        format!("{content}alias  = \"{alias}\"\n")
    };

    if let Err(e) = std::fs::write(&path, updated) {
        eprintln!("Warning: could not save alias to {}: {e}", path.display());
    }
}

fn find_repo_config_path() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let path = dir.join(".orchy.toml");
        if path.is_file() {
            return Some(path);
        }
        if !dir.pop() {
            return None;
        }
    }
}
