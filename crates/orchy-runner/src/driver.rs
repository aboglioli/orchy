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
use tokio::time::{self, Duration, sleep};

use crate::config::RunnerConfig;
use crate::error::{Error, Result};
use crate::process::spawn_pty_raw;

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
}

impl AgentDriver {
    async fn connect(config: RunnerConfig) -> Result<Self> {
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
        })
    }

    pub async fn run(config: RunnerConfig) -> Result<()> {
        let mut driver = Self::connect(config).await?;
        driver.register().await?;
        driver.inject_bootstrap().await?;
        driver.main_loop().await
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

    async fn main_loop(&mut self) -> Result<()> {
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
        // Collapse newlines to spaces — in raw PTY mode (Ink, Bubble Tea), `\n` is
        // interpreted as Enter and would submit the text prematurely line by line.
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
) {
    let patterns: Vec<Vec<u8>> = idle_patterns.into_iter().map(String::into_bytes).collect();
    tokio::spawn(async move {
        let mut tail: Vec<u8> = Vec::with_capacity(512);
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
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
