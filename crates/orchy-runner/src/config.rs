use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub enum SpawnMode {
    #[default]
    Pty,
    Pipe,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub spawn_mode: SpawnMode,
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
    pub pty_rows: u16,
    pub pty_cols: u16,
    /// Stripped byte sequences that indicate the agent is idle and ready for input.
    /// Matched against the tail of ANSI-stripped PTY output.
    pub idle_patterns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OrchyConfig {
    pub url: String,
    pub project: String,
    pub description: String,
    pub namespace: Option<String>,
    pub roles: Vec<String>,
    pub poll_interval_secs: u64,
    pub heartbeat_interval_secs: u64,
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub agent: AgentConfig,
    pub orchy: OrchyConfig,
}

impl RunnerConfig {
    /// Build config from CLI args + environment variables.
    ///
    /// Usage: `orchy-runner <command> [args...]`
    ///
    /// Env vars (all optional with defaults):
    ///   ORCHY_URL             — orchy MCP endpoint (default: http://127.0.0.1:3100/mcp)
    ///   ORCHY_PROJECT         — project name (default: current directory name)
    ///   ORCHY_DESCRIPTION     — agent description (default: "{name} agent")
    ///   ORCHY_AGENT_NAME      — agent name (default: command basename)
    ///   ORCHY_NAMESPACE       — namespace (optional)
    ///   ORCHY_ROLES           — comma-separated roles (optional)
    ///   ORCHY_IDLE_PATTERNS   — comma-separated idle prompt patterns (default: "❯ ,$ ")
    pub fn from_env() -> Result<Self, String> {
        let mut cli_args = std::env::args().skip(1);
        let command = cli_args.next().ok_or_else(|| {
            "usage: orchy-runner <command> [args...]\n\
             example: orchy-runner claude\n\
             example: orchy-runner opencode run --format json"
                .to_string()
        })?;
        let args: Vec<String> = cli_args.collect();

        let name = std::env::var("ORCHY_AGENT_NAME")
            .unwrap_or_else(|_| basename(&command).to_string());

        let (pty_rows, pty_cols) = crossterm::terminal::size().unwrap_or((24, 120));

        let mut env = HashMap::new();
        env.insert(
            "TERM".to_string(),
            std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()),
        );

        let idle_patterns = std::env::var("ORCHY_IDLE_PATTERNS")
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_else(|_| vec!["❯ ".to_string(), "$ ".to_string()]);

        let url = std::env::var("ORCHY_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3100/mcp".to_string());

        let project = std::env::var("ORCHY_PROJECT").unwrap_or_else(|_| current_dir_name());

        let description = std::env::var("ORCHY_DESCRIPTION")
            .unwrap_or_else(|_| format!("{name} agent"));

        let namespace = std::env::var("ORCHY_NAMESPACE").ok();

        let roles = std::env::var("ORCHY_ROLES")
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            agent: AgentConfig {
                name,
                command,
                args,
                spawn_mode: SpawnMode::Pty,
                env,
                working_dir: None,
                pty_rows,
                pty_cols,
                idle_patterns,
            },
            orchy: OrchyConfig {
                url,
                project,
                description,
                namespace,
                roles,
                poll_interval_secs: 10,
                heartbeat_interval_secs: 30,
            },
        })
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.orchy.poll_interval_secs)
    }

    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.orchy.heartbeat_interval_secs)
    }
}

fn basename(command: &str) -> &str {
    std::path::Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command)
}

fn current_dir_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "default".to_string())
}
