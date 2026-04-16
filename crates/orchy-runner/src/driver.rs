use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::Instant;

use pty_process::OwnedWritePty;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

use crate::config::RunnerConfig;
use crate::error::{Error, Result};
use crate::mcp_config;
use crate::process::spawn_pty_raw;
use crate::session::AgentSession;

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("failed to build HTTP client")
});

pub struct AgentDriver {
    writer: Arc<Mutex<OwnedWritePty>>,
    child: tokio::process::Child,
    is_idle: Arc<AtomicBool>,
    last_output_ms: Arc<AtomicU64>,
    config: RunnerConfig,
    shutting_down: bool,
    mcp_injected: bool,
}

impl AgentDriver {
    fn connect(config: RunnerConfig, output_tx: UnboundedSender<Vec<u8>>) -> Result<Self> {
        let parts = spawn_pty_raw(&config)?;
        let writer = Arc::new(Mutex::new(parts.writer));
        let is_idle = Arc::new(AtomicBool::new(false));
        let last_output_ms = Arc::new(AtomicU64::new(0));

        spawn_output_reader(
            parts.reader,
            config.idle_patterns.clone(),
            Arc::clone(&is_idle),
            Arc::clone(&last_output_ms),
            output_tx,
        );

        Ok(Self {
            writer,
            child: parts.child,
            is_idle,
            last_output_ms,
            config,
            shutting_down: false,
            mcp_injected: false,
        })
    }

    /// Standalone entry point: transparent PTY passthrough.
    pub async fn run(config: RunnerConfig) -> Result<()> {
        let (mut session, handle) = Self::start(config).await?;

        crossterm::terminal::enable_raw_mode()
            .map_err(|e| Error::Io(format!("enable raw mode: {e}")))?;

        let output_fwd = tokio::spawn(async move {
            let mut stdout = tokio::io::stdout();
            while let Some(bytes) = session.output_rx.recv().await {
                if stdout.write_all(&bytes).await.is_err() {
                    break;
                }
                let _ = stdout.flush().await;
            }
        });

        let input_tx = session.input_tx;
        let stdin_fwd = tokio::spawn(async move {
            let mut stdin = tokio::io::stdin();
            let mut buf = [0u8; 256];
            loop {
                match stdin.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if input_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let result = handle.await.map_err(|e| Error::Io(format!("join: {e}")))?;

        crossterm::terminal::disable_raw_mode()
            .map_err(|e| Error::Io(format!("disable raw mode: {e}")))?;

        output_fwd.abort();
        stdin_fwd.abort();

        result
    }

    pub async fn start(config: RunnerConfig) -> Result<(AgentSession, JoinHandle<Result<()>>)> {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let mcp_injected = if mcp_config::supports_mcp_json(&config.agent_type) {
            let dir = config
                .working_dir
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_default();
            match mcp_config::inject(&dir, &config.url) {
                Ok(injected) => {
                    if injected {
                        tracing::info!(dir = %dir.display(), "injected orchy into .mcp.json");
                    }
                    injected
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to inject .mcp.json, continuing");
                    false
                }
            }
        } else {
            false
        };

        let mut driver = Self::connect(config, output_tx)?;
        driver.mcp_injected = mcp_injected;

        let alias = driver.config.alias.clone();
        let agent_type = driver.config.agent_type.clone();
        let is_idle = Arc::clone(&driver.is_idle);

        let handle = tokio::spawn(async move { driver.main_loop_inner(input_rx).await });

        let session = AgentSession {
            alias,
            agent_type,
            is_idle,
            output_rx,
            input_tx,
        };

        Ok((session, handle))
    }

    async fn main_loop_inner(&mut self, mut input_rx: UnboundedReceiver<Vec<u8>>) -> Result<()> {
        self.inject_bootstrap().await?;

        let mut last_was_idle = false;
        let mut idle_since: Option<Instant> = None;

        tracing::info!(alias = %self.config.alias, "entering main loop");

        loop {
            if self.shutting_down {
                break;
            }

            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|e| Error::Io(format!("wait: {e}")))?
            {
                if status.success() {
                    tracing::info!(alias = %self.config.alias, "agent process exited cleanly");
                    return Ok(());
                }
                tracing::warn!(alias = %self.config.alias, ?status, "process exited with error");
                return Err(Error::ProcessExited);
            }

            let last_ms = self.last_output_ms.load(Ordering::Relaxed);
            if last_ms > 0 && now_ms().saturating_sub(last_ms) > 800 {
                self.is_idle.store(true, Ordering::Relaxed);
            }

            let currently_idle = self.is_idle.load(Ordering::Relaxed);

            if currently_idle && !last_was_idle {
                idle_since = Some(Instant::now());
            } else if !currently_idle && last_was_idle {
                idle_since = None;
            }
            last_was_idle = currently_idle;

            if let Some(since) = idle_since {
                let elapsed = since.elapsed();
                if elapsed > Duration::from_secs(5) && elapsed > self.config.idle_wake {
                    tracing::info!(alias = %self.config.alias, "idle too long, injecting wake-up");
                    self.inject("Check your mailbox and get your next task.").await?;
                    idle_since = None;
                }
            }

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!(alias = %self.config.alias, "ctrl-c received, shutting down");
                    self.shutting_down = true;
                }
                Some(bytes) = input_rx.recv() => {
                    let mut w = self.writer.lock().await;
                    let _ = w.write_all(&bytes).await;
                    let _ = w.flush().await;
                }
                _ = sleep(Duration::from_secs(1)) => {}
            }
        }

        self.shutdown().await
    }

    async fn inject_bootstrap(&mut self) -> Result<()> {
        self.wait_for_silence(30).await;

        {
            let mut w = self.writer.lock().await;
            let _ = w.write_all(b"\r").await;
            let _ = w.flush().await;
        }
        self.is_idle.store(false, Ordering::Relaxed);

        sleep(Duration::from_millis(500)).await;
        self.wait_for_silence(20).await;

        let agents_ctx =
            fetch_agents(&self.config.url, &self.config.project, &self.config.alias).await;
        let prompt = format!(
            "You are agent '{alias}'.{agents_ctx}\n\nConnect to orchy MCP server at {url}. On startup:\n1. register_agent(project: \"{project}\", alias: \"{alias}\", description: \"{alias} agent\") — establish your session\n2. list_knowledge(kind: \"skill\") — load project conventions\n3. check_mailbox — read incoming messages\n4. get_next_task — claim your first task\n\nCall heartbeat every 30 seconds. Focus on completing tasks.",
            alias = self.config.alias,
            agents_ctx = agents_ctx,
            url = self.config.url,
            project = self.config.project,
        );

        tracing::info!(alias = %self.config.alias, "injecting bootstrap prompt");
        self.inject(&prompt).await
    }

    async fn wait_for_silence(&self, timeout_secs: u64) {
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            if self.is_idle.load(Ordering::Relaxed) {
                break;
            }
            let last_ms = self.last_output_ms.load(Ordering::Relaxed);
            if last_ms > 0 && now_ms().saturating_sub(last_ms) > 800 {
                break;
            }
            if Instant::now() >= deadline {
                tracing::warn!(alias = %self.config.alias, "timed out waiting for agent idle");
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }
    }

    async fn inject(&self, text: &str) -> Result<()> {
        let normalized: String = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        {
            let mut w = self.writer.lock().await;
            w.write_all(normalized.as_bytes())
                .await
                .map_err(|e| Error::Io(format!("inject: {e}")))?;
            w.flush()
                .await
                .map_err(|e| Error::Io(format!("inject flush: {e}")))?;
        }
        sleep(Duration::from_millis(100)).await;
        {
            let mut w = self.writer.lock().await;
            w.write_all(b"\r")
                .await
                .map_err(|e| Error::Io(format!("inject enter: {e}")))?;
            w.flush()
                .await
                .map_err(|e| Error::Io(format!("inject enter flush: {e}")))?;
        }
        self.is_idle.store(false, Ordering::Relaxed);
        tracing::debug!(alias = %self.config.alias, bytes = normalized.len(), "injected prompt");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        tracing::info!(alias = %self.config.alias, "shutting down");
        {
            let mut w = self.writer.lock().await;
            let _ = w.write_all(b"/exit\r").await;
            let _ = w.flush().await;
        }
        sleep(Duration::from_millis(500)).await;
        let _ = self.child.kill().await;
        if self.mcp_injected {
            let dir = self
                .config
                .working_dir
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_default();
            mcp_config::remove(&dir);
            tracing::info!(dir = %dir.display(), "removed orchy from .mcp.json");
        }
        Ok(())
    }

    pub fn request_shutdown(&mut self) {
        self.shutting_down = true;
    }
}

fn spawn_output_reader(
    mut reader: pty_process::OwnedReadPty,
    idle_patterns: Vec<String>,
    is_idle: Arc<AtomicBool>,
    last_output_ms: Arc<AtomicU64>,
    output_tx: UnboundedSender<Vec<u8>>,
) {
    let patterns: Vec<Vec<u8>> = idle_patterns.into_iter().map(String::into_bytes).collect();
    tokio::spawn(async move {
        let mut tail: Vec<u8> = Vec::with_capacity(512);
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = output_tx.send(buf[..n].to_vec());

                    last_output_ms.store(now_ms(), Ordering::Relaxed);
                    is_idle.store(false, Ordering::Relaxed);

                    let stripped = strip_ansi_escapes::strip(&buf[..n]);
                    tail.extend_from_slice(&stripped);
                    if tail.len() > 512 {
                        let excess = tail.len() - 512;
                        tail.drain(..excess);
                    }
                    if patterns.iter().any(|p| tail.ends_with(p.as_slice())) {
                        is_idle.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    });
}

async fn fetch_agents(mcp_url: &str, project: &str, self_alias: &str) -> String {
    let base = mcp_url
        .trim_end_matches('/')
        .strip_suffix("/mcp")
        .unwrap_or(mcp_url.trim_end_matches('/'));
    let url = format!("{base}/api/agents?project={project}");

    #[derive(serde::Deserialize)]
    struct AgentDto {
        alias: Option<String>,
        agent_type: Option<String>,
    }

    let Ok(resp) = HTTP_CLIENT.get(&url).send().await else {
        return String::new();
    };
    let Ok(agents) = resp.json::<Vec<AgentDto>>().await else {
        return String::new();
    };

    let others: Vec<String> = agents
        .into_iter()
        .filter_map(|a| {
            let alias = a.alias?;
            if alias == self_alias {
                return None;
            }
            Some(match a.agent_type {
                Some(t) => format!("{alias} ({t})"),
                None => alias,
            })
        })
        .collect();

    if others.is_empty() {
        return String::new();
    }

    format!(
        " Other active agents in this project: {}. You can reach them with send_message(alias: \"...\", ...).",
        others.join(", ")
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
