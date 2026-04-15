use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use pty_process::OwnedWritePty;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, RawContent};
use rmcp::service::{Peer, RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time::{self, Duration, sleep};

use crate::config::RunnerConfig;
use crate::error::{Error, Result};
use crate::mcp_config;
use crate::process::spawn_pty_raw;
use crate::session::AgentSession;

pub struct AgentDriver {
    writer: Arc<Mutex<OwnedWritePty>>,
    child: tokio::process::Child,
    is_idle: Arc<AtomicBool>,
    config: RunnerConfig,
    peer: Arc<Peer<RoleClient>>,
    #[allow(dead_code)]
    service: RunningService<RoleClient, ()>,
    agent_id: String,
    shutting_down: bool,
    /// True if we wrote the orchy entry into .mcp.json and must clean it up.
    mcp_injected: bool,
}

impl AgentDriver {
    async fn connect(
        config: RunnerConfig,
        output_tx: UnboundedSender<Vec<u8>>,
    ) -> Result<Self> {
        let transport = StreamableHttpClientTransport::from_uri(config.url.as_str());

        let service: RunningService<RoleClient, ()> =
            ().serve(transport).await.map_err(|e| {
                Error::Mcp(format!("connect to {}: {e}", config.url))
            })?;

        let peer = Arc::new(service.peer().clone());
        let parts = spawn_pty_raw(&config)?;
        let writer = Arc::new(Mutex::new(parts.writer));
        let is_idle = Arc::new(AtomicBool::new(false));

        spawn_output_reader(
            parts.reader,
            config.idle_patterns.clone(),
            Arc::clone(&is_idle),
            output_tx,
        );

        Ok(Self {
            writer,
            child: parts.child,
            is_idle,
            config,
            peer,
            service,
            agent_id: String::new(),
            shutting_down: false,
            mcp_injected: false,
        })
    }

    /// Standalone entry point: transparent PTY passthrough.
    /// Puts the terminal in raw mode, pipes PTY output to stdout and stdin to
    /// the PTY, so the agent's UI is fully visible and interactive.
    pub async fn run(config: RunnerConfig) -> Result<()> {
        let (mut session, handle) = Self::start(config).await?;

        crossterm::terminal::enable_raw_mode()
            .map_err(|e| Error::Io(format!("enable raw mode: {e}")))?;

        // PTY output → stdout
        let output_fwd = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut stdout = tokio::io::stdout();
            while let Some(bytes) = session.output_rx.recv().await {
                if stdout.write_all(&bytes).await.is_err() {
                    break;
                }
                let _ = stdout.flush().await;
            }
        });

        // stdin → PTY (raw bytes, no processing)
        let input_tx = session.input_tx;
        let stdin_fwd = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
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

    pub async fn start(
        config: RunnerConfig,
    ) -> Result<(AgentSession, JoinHandle<Result<()>>)> {
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

        let mut driver = Self::connect(config, output_tx).await?;
        driver.mcp_injected = mcp_injected;
        driver.register().await?;
        driver.inject_bootstrap().await?;

        let alias = driver.config.alias.clone();
        let agent_id = driver.agent_id.clone();
        let agent_type = driver.config.agent_type.clone();
        let is_idle = Arc::clone(&driver.is_idle);

        let handle = tokio::spawn(async move { driver.main_loop_inner(input_rx).await });

        let session = AgentSession {
            alias,
            agent_id,
            agent_type,
            is_idle,
            output_rx,
            input_tx,
        };

        Ok((session, handle))
    }

    async fn register(&mut self) -> Result<()> {
        let mut args = serde_json::Map::new();
        args.insert("project".into(), self.config.project.clone().into());
        args.insert("alias".into(), self.config.alias.clone().into());
        args.insert("description".into(), self.config.description.clone().into());

        if let Some(ns) = &self.config.namespace {
            args.insert("namespace".into(), ns.clone().into());
        }

        if !self.config.roles.is_empty() {
            args.insert(
                "roles".into(),
                serde_json::Value::Array(
                    self.config.roles.iter().map(|r| r.clone().into()).collect(),
                ),
            );
        }

        let mut metadata = serde_json::Map::new();
        metadata.insert("agent_type".into(), self.config.agent_type.clone().into());
        metadata.insert("runner_managed".into(), "true".into());
        args.insert("metadata".into(), serde_json::Value::Object(metadata));

        let result = self.call_tool("register_agent", args).await?;
        let text = extract_text(&result);

        let agent_id = extract_field(&text, "id").unwrap_or_default();
        tracing::info!(agent_id = %agent_id, alias = %self.config.alias, "registered with orchy");
        self.agent_id = agent_id;

        Ok(())
    }

    async fn inject_bootstrap(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if self.is_idle.load(Ordering::Relaxed) {
                break;
            }
            if Instant::now() >= deadline {
                tracing::warn!("timed out waiting for agent idle before bootstrap");
                break;
            }
            sleep(Duration::from_millis(500)).await;
        }

        let prompt = format!(
            "You are agent '{}' (id: {}).\n\nConnect to orchy MCP at {}. On startup:\n1. register_agent(project: \"{}\", agent_id: \"{}\") — resumes your existing profile\n2. list_knowledge(kind: \"skill\") — load project conventions\n3. check_mailbox — read incoming messages\n4. get_next_task — claim your first task\n\nYour heartbeat is managed by orchy-runner. Focus on completing tasks.",
            self.config.alias,
            self.agent_id,
            self.config.url,
            self.config.project,
            self.agent_id,
        );

        self.inject(&prompt).await
    }

    pub async fn main_loop(&mut self) -> Result<()> {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        self.main_loop_inner(rx).await
    }

    async fn main_loop_inner(&mut self, mut input_rx: UnboundedReceiver<Vec<u8>>) -> Result<()> {
        let heartbeat_interval = self.config.heartbeat_interval;
        let mut heartbeat_timer = time::interval(heartbeat_interval);
        let mut last_was_idle = false;
        let mut idle_since: Option<Instant> = None;

        heartbeat_timer.tick().await;

        tracing::info!(
            alias = %self.config.alias,
            heartbeat_secs = heartbeat_interval.as_secs(),
            "entering main loop"
        );

        loop {
            if self.shutting_down {
                break;
            }

            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|e| Error::Io(format!("wait: {e}")))?
            {
                tracing::warn!(alias = %self.config.alias, ?status, "process exited");
                return Err(Error::ProcessExited);
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
                _ = heartbeat_timer.tick() => {
                    if let Err(e) = self.heartbeat().await {
                        tracing::warn!(error = %e, "heartbeat failed");
                    }
                }
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

    async fn heartbeat(&self) -> Result<()> {
        self.call_tool("heartbeat", serde_json::Map::new()).await?;
        Ok(())
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
        let _ = self.call_tool("disconnect", serde_json::Map::new()).await;
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

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> Result<CallToolResult> {
        let params = CallToolRequestParams::new(name.to_string()).with_arguments(args);
        self.peer
            .call_tool(params)
            .await
            .map_err(|e| Error::Mcp(format!("call_tool({name}): {e}")))
    }
}

fn spawn_output_reader(
    mut reader: pty_process::OwnedReadPty,
    idle_patterns: Vec<String>,
    is_idle: Arc<AtomicBool>,
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
                    let raw = buf[..n].to_vec();
                    let _ = output_tx.send(raw);

                    let stripped = strip_ansi_escapes::strip(&buf[..n]);
                    tail.extend_from_slice(&stripped);
                    if tail.len() > 512 {
                        let excess = tail.len() - 512;
                        tail.drain(..excess);
                    }
                    let idle = patterns.iter().any(|p| tail.ends_with(p.as_slice()));
                    is_idle.store(idle, Ordering::Relaxed);
                }
            }
        }
    });
}

fn extract_text(result: &CallToolResult) -> String {
    use std::ops::Deref;
    result
        .content
        .iter()
        .filter_map(|c| match c.deref() {
            RawContent::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_field(text: &str, field: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(val) = v.get(field)
    {
        return Some(val.as_str().unwrap_or(&val.to_string()).to_string());
    }

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(field)
            && let Some(rest) = trimmed.strip_prefix(field)
        {
            let rest = rest.trim_start_matches([':', '"', ' ', '\t']);
            let rest = rest.trim_end_matches(['"', ',']);
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }

    None
}
