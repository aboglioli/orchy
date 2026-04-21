use std::path::PathBuf;

use serde::Deserialize;

/// Resolved CLI configuration after layering: global file → repo-local file → env → flags.
#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub api_key: String,
    pub org: String,
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
    ) -> Result<Self, String> {
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
        )?;

        let api_key = pick(
            &[
                global.as_ref().and_then(|c| c.api_key.as_deref()),
                local.as_ref().and_then(|c| c.api_key.as_deref()),
                env("ORCHY_API_KEY"),
                flag_api_key,
            ],
            "api_key",
        )?;

        let org = pick(
            &[
                global.as_ref().and_then(|c| c.org.as_deref()),
                local.as_ref().and_then(|c| c.org.as_deref()),
                env("ORCHY_ORG"),
                flag_org,
            ],
            "org",
        )?;

        let project = pick(
            &[
                global.as_ref().and_then(|c| c.project.as_deref()),
                local.as_ref().and_then(|c| c.project.as_deref()),
                env("ORCHY_PROJECT"),
                flag_project,
            ],
            "project",
        )?;

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

        Ok(Config {
            url,
            api_key,
            org,
            project,
            namespace,
            alias,
            description,
            roles,
            json,
        })
    }
}

fn env(key: &str) -> Option<&'static str> {
    std::env::var(key).ok().map(|s| s.leak() as &_)
}

fn pick(opts: &[Option<&str>], name: &str) -> Result<String, String> {
    opts.iter()
        .rev()
        .find_map(|o| *o)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            let name_upper = match name {
                "api_key" => "API_KEY",
                "url" => "URL",
                n => n,
            };
            let flag = match name {
                "api_key" => "api-key",
                "url" => "url",
                n => n,
            };
            format!(
                "{name} is required — set it in config, env (ORCHY_{name_upper}), or pass --{flag}"
            )
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
        eprintln!(
            "Warning: could not save alias to {}: {e}",
            path.display()
        );
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
