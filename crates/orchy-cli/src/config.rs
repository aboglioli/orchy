use std::path::PathBuf;

use orchy_core::{ActorId, MachineId};
use serde::{Deserialize, Serialize};

use crate::error::{CliError, CliResult};

const APP: &str = "orchy";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct Settings {
    pub machine: Option<String>,
    pub vault: Option<PathBuf>,
    pub actor: Option<String>,
}

/// Vault-level configuration, read from `<vault>/orchy.toml`. Distinct from `Settings`,
/// which is per machine and lives in the user's config directory.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct VaultConfig {
    pub events: EventsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct EventsConfig {
    /// Fixed when a machine's log is first created; eventuary refuses to reopen a log with a
    /// different count, so changing it later is an error rather than a silent migration.
    pub partitions: u32,
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            partitions: orchy_store_vault::eventlog::DEFAULT_PARTITIONS,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub vault: PathBuf,
    pub actor: ActorId,
    pub machine: MachineId,
    pub organization: String,
    pub vault_config: VaultConfig,
}

impl Config {
    pub(crate) fn resolve(
        vault_flag: Option<PathBuf>,
        actor_flag: Option<String>,
    ) -> CliResult<Self> {
        let settings = read_settings()?;
        let machine = machine_id(&settings)?;

        let vault = vault_flag
            .or_else(|| std::env::var_os("ORCHY_VAULT").map(PathBuf::from))
            .or_else(|| settings.vault.clone())
            .unwrap_or_else(default_vault);

        let alias = actor_flag
            .or_else(|| std::env::var("ORCHY_ACTOR").ok())
            .or_else(|| settings.actor.clone())
            .unwrap_or_else(|| "human".to_owned());

        let actor = if alias.contains('@') {
            alias.parse::<ActorId>()?
        } else {
            ActorId::new(&alias, machine.to_string())?
        };

        let vault_config = read_vault_config(&vault)?;

        Ok(Self {
            vault,
            actor,
            machine,
            organization: APP.to_owned(),
            vault_config,
        })
    }

    pub(crate) fn events_root(&self) -> PathBuf {
        self.vault.join("events")
    }

    pub(crate) fn runtime_root(&self) -> PathBuf {
        self.vault.join(".orchy")
    }

    pub(crate) fn is_initialised(&self) -> bool {
        self.vault.join("orchy.toml").exists()
    }
}

pub(crate) fn settings_path() -> PathBuf {
    config_home().join(APP).join("settings.toml")
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

fn default_vault() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
        .join(APP)
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn read_vault_config(vault: &std::path::Path) -> CliResult<VaultConfig> {
    let path = vault.join("orchy.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(VaultConfig::default());
    };
    toml::from_str(&text).map_err(|e| CliError::config(format!("{}: {e}", path.display())))
}

fn read_settings() -> CliResult<Settings> {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Settings::default());
    };
    toml::from_str(&text).map_err(|e| CliError::config(format!("{}: {e}", path.display())))
}

fn write_settings(settings: &Settings) -> CliResult<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(CliError::io)?;
    }
    let text = toml::to_string_pretty(settings)
        .map_err(|e| CliError::config(format!("serialising settings: {e}")))?;
    std::fs::write(&path, text).map_err(CliError::io)
}

/// Separates this machine's event-log root from every other's, so it must not change.
fn machine_id(settings: &Settings) -> CliResult<MachineId> {
    if let Some(existing) = &settings.machine {
        return Ok(MachineId::new(existing)?);
    }
    let generated = MachineId::new(ulid_string())?;
    let mut updated = settings.clone();
    updated.machine = Some(generated.to_string());
    write_settings(&updated)?;
    Ok(generated)
}

fn ulid_string() -> String {
    use orchy_core::IdGenerator;
    orchy_store_vault::time::UlidGenerator::new()
        .generate()
        .to_string()
}
