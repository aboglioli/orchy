use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "orchy-runner", about = "Process manager for CLI agents")]
pub struct Cli {
    #[arg(long, env = "ORCHY_ALIAS")]
    pub alias: String,

    #[arg(long, env = "ORCHY_URL", default_value = "http://127.0.0.1:3100/mcp")]
    pub url: String,

    #[arg(long, env = "ORCHY_PROJECT")]
    pub project: Option<String>,

    #[arg(long, env = "ORCHY_DESCRIPTION")]
    pub description: Option<String>,

    #[arg(long, env = "ORCHY_AGENT_TYPE", default_value = "unknown")]
    pub agent_type: String,

    #[arg(long, env = "ORCHY_NAMESPACE")]
    pub namespace: Option<String>,

    #[arg(long, env = "ORCHY_ROLES")]
    pub roles: Option<String>,

    #[arg(long, env = "ORCHY_IDLE_PATTERNS")]
    pub idle_patterns: Option<String>,

    #[arg(long, env = "ORCHY_IDLE_WAKE_SECS", default_value_t = 120)]
    pub idle_wake_secs: u64,

    #[arg(long, env = "ORCHY_HEARTBEAT_SECS", default_value_t = 30)]
    pub heartbeat_secs: u64,

    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

pub struct RunnerConfig {
    pub alias: String,
    pub agent_type: String,
    pub description: String,
    pub url: String,
    pub project: String,
    pub namespace: Option<String>,
    pub roles: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
    pub pty_rows: u16,
    pub pty_cols: u16,
    pub idle_patterns: Vec<String>,
    pub idle_wake: Duration,
    pub heartbeat_interval: Duration,
}

impl RunnerConfig {
    pub fn from_cli(cli: Cli) -> Self {
        let project = cli
            .project
            .unwrap_or_else(current_dir_name);
        let description = cli
            .description
            .unwrap_or_else(|| format!("{} agent", cli.alias));
        let idle_patterns = cli
            .idle_patterns
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_else(|| default_idle_patterns(&cli.agent_type));
        let roles = cli
            .roles
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let (pty_rows, pty_cols) = crossterm::terminal::size().unwrap_or((24, 120));

        let mut env = HashMap::new();
        env.insert(
            "TERM".to_string(),
            std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()),
        );

        let mut command_parts = cli.command.into_iter();
        let command = command_parts.next().unwrap_or_default();
        let args: Vec<String> = command_parts.collect();

        Self {
            alias: cli.alias,
            agent_type: cli.agent_type,
            description,
            url: cli.url,
            project,
            namespace: cli.namespace,
            roles,
            command,
            args,
            env,
            working_dir: None,
            pty_rows,
            pty_cols,
            idle_patterns,
            idle_wake: Duration::from_secs(cli.idle_wake_secs),
            heartbeat_interval: Duration::from_secs(cli.heartbeat_secs),
        }
    }
}

/// Known idle prompt patterns per agent type.
/// Override with `--idle-patterns` / `ORCHY_IDLE_PATTERNS` if these don't match.
fn default_idle_patterns(agent_type: &str) -> Vec<String> {
    match agent_type {
        // Claude Code (Ink TUI) — waits at ❯
        "claude" => vec!["❯ ".to_string()],
        // Cursor CLI agent (`agent` binary, Ink TUI) — same pattern as Claude Code
        "cursor" => vec!["❯ ".to_string()],
        // OpenCode (Bubble Tea TUI) — waits at >
        "opencode" => vec!["> ".to_string()],
        // Gemini CLI — waits at >
        "gemini" => vec!["> ".to_string()],
        // Aider (line-mode chat) — waits at "> "
        "aider" => vec!["> ".to_string()],
        // Generic fallback covers most shells and unknown TUIs
        _ => vec!["❯ ".to_string(), "$ ".to_string(), "> ".to_string()],
    }
}

fn current_dir_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "default".to_string())
}
