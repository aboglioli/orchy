use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    /// Set to true when an idle prompt pattern is detected in ANSI-stripped PTY output.
    /// Guards task/message injection — never inject while agent is processing.
    is_idle: Arc<AtomicBool>,
    config: RunnerConfig,
    peer: Arc<Peer<RoleClient>>,
    #[allow(dead_code)]
    service: RunningService<RoleClient, ()>,
    shutting_down: bool,
    agent_id: Option<String>,
}

impl AgentDriver {
    pub async fn connect(config: RunnerConfig) -> Result<Self> {
        let transport = StreamableHttpClientTransport::from_uri(config.orchy.url.as_str());

        let service: RunningService<RoleClient, ()> =
            ().serve(transport).await.map_err(|e| {
                Error::Mcp(format!("connect to {}: {e}", config.orchy.url))
            })?;

        let peer = Arc::new(service.peer().clone());
        let parts = spawn_pty_raw(&config.agent)?;
        let writer = Arc::new(Mutex::new(parts.writer));
        let is_idle = Arc::new(AtomicBool::new(false));

        spawn_output_reader(
            parts.reader,
            config.agent.idle_patterns.clone(),
            Arc::clone(&is_idle),
        );

        Ok(Self {
            writer,
            child: parts.child,
            is_idle,
            config,
            peer,
            service,
            shutting_down: false,
            agent_id: None,
        })
    }

    pub async fn run(config: RunnerConfig) -> Result<()> {
        let mut driver = Self::connect(config).await?;
        driver.register().await?;
        driver.main_loop().await
    }

    async fn register(&mut self) -> Result<()> {
        let mut args = serde_json::Map::new();
        args.insert("project".into(), self.config.orchy.project.clone().into());
        args.insert("description".into(), self.config.orchy.description.clone().into());

        if let Some(ns) = &self.config.orchy.namespace {
            args.insert("namespace".into(), ns.clone().into());
        }

        if !self.config.orchy.roles.is_empty() {
            args.insert(
                "roles".into(),
                serde_json::Value::Array(
                    self.config.orchy.roles.iter().map(|r| r.clone().into()).collect(),
                ),
            );
        }

        let result = self.call_tool("register_agent", args).await?;
        let text = extract_text(&result);

        if let Some(id) = extract_field(&text, "agent_id") {
            tracing::info!(agent_id = %id, "registered with orchy");
            self.agent_id = Some(id);
        } else {
            tracing::info!("registered with orchy");
        }

        Ok(())
    }

    async fn main_loop(&mut self) -> Result<()> {
        let poll_interval = self.config.poll_interval();
        let heartbeat_interval = self.config.heartbeat_interval();

        let mut poll_timer = time::interval(poll_interval);
        let mut heartbeat_timer = time::interval(heartbeat_interval);

        poll_timer.tick().await;
        heartbeat_timer.tick().await;

        tracing::info!(
            agent = %self.config.agent.name,
            poll_secs = poll_interval.as_secs(),
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
                tracing::warn!(agent = %self.config.agent.name, ?status, "process exited");
                return Err(Error::ProcessExited);
            }

            tokio::select! {
                _ = poll_timer.tick() => {
                    if let Err(e) = self.poll_for_work().await {
                        tracing::warn!(error = %e, "poll_for_work failed");
                    }
                    if let Err(e) = self.check_mailbox().await {
                        tracing::warn!(error = %e, "check_mailbox failed");
                    }
                }
                _ = heartbeat_timer.tick() => {
                    if let Err(e) = self.heartbeat().await {
                        tracing::warn!(error = %e, "heartbeat failed");
                    }
                }
            }
        }

        self.shutdown().await
    }

    async fn poll_for_work(&mut self) -> Result<()> {
        if !self.is_idle.load(Ordering::Relaxed) {
            tracing::debug!(agent = %self.config.agent.name, "agent busy, skipping poll");
            return Ok(());
        }

        let mut args = serde_json::Map::new();
        args.insert("claim".into(), true.into());

        let result = self.call_tool("get_next_task", args).await?;
        let text = extract_text(&result);

        if text.contains("no task") || text.contains("No task") || text.is_empty() {
            return Ok(());
        }

        let task_id = extract_field(&text, "task_id")
            .or_else(|| extract_field(&text, "id"))
            .unwrap_or_default();
        let title = extract_field(&text, "title").unwrap_or_default();

        if task_id.is_empty() {
            return Ok(());
        }

        tracing::info!(task_id = %task_id, title = %title, "claimed task, injecting prompt");

        let mut start_args = serde_json::Map::new();
        start_args.insert("task_id".into(), task_id.clone().into());
        let _ = self.call_tool("start_task", start_args).await;

        let prompt = format!(
            "You have been assigned task {task_id}: {title}\n\
             Please complete this task. When done, call complete_task with a summary."
        );
        self.inject(&prompt).await
    }

    async fn check_mailbox(&mut self) -> Result<()> {
        if !self.is_idle.load(Ordering::Relaxed) {
            return Ok(());
        }

        let result = self.call_tool("check_mailbox", serde_json::Map::new()).await?;
        let text = extract_text(&result);

        if text.contains("no message") || text.contains("No message") || text.is_empty() {
            return Ok(());
        }

        tracing::info!(agent = %self.config.agent.name, "received messages, injecting");
        self.inject(&format!("[SYSTEM MESSAGE]: {text}")).await
    }

    async fn heartbeat(&mut self) -> Result<()> {
        self.call_tool("heartbeat", serde_json::Map::new()).await?;
        Ok(())
    }

    /// Write text into the PTY and send Enter. Marks the agent as busy immediately.
    async fn inject(&self, text: &str) -> Result<()> {
        {
            let mut w = self.writer.lock().await;
            w.write_all(text.as_bytes())
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
        tracing::debug!(agent = %self.config.agent.name, bytes = text.len(), "injected prompt");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        tracing::info!(agent = %self.config.agent.name, "shutting down");
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

    pub fn request_shutdown(&mut self) {
        self.shutting_down = true;
    }
}

/// Reads raw PTY bytes, strips ANSI, keeps a rolling tail, and sets `is_idle` when
/// the tail ends with one of the configured idle prompt patterns.
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
