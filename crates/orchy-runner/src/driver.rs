use std::sync::Arc;

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, RawContent};
use rmcp::service::{Peer, RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use tokio::time;

use crate::config::RunnerConfig;
use crate::error::{Error, Result};
use crate::output::{OutputParser, ParsedOutput};
use crate::process::AgentProcess;

#[derive(Debug, Clone, PartialEq)]
enum DriverState {
    Idle,
    Working { task_id: String, title: String },
    ShuttingDown,
}

pub struct AgentDriver {
    process: AgentProcess,
    config: RunnerConfig,
    peer: Arc<Peer<RoleClient>>,
    #[allow(dead_code)]
    service: RunningService<RoleClient, ()>,
    parser: OutputParser,
    state: DriverState,
    agent_id: Option<String>,
    session_id: Option<String>,
}

impl AgentDriver {
    pub async fn connect(config: RunnerConfig) -> Result<Self> {
        let transport = StreamableHttpClientTransport::from_uri(config.orchy.url.as_str());

        let service: RunningService<RoleClient, ()> = ().serve(transport).await.map_err(|e| {
            Error::Mcp(format!(
                "failed to connect to orchy at {}: {e}",
                config.orchy.url
            ))
        })?;

        let peer = service.peer().clone();

        let is_json_mode = config
            .agent
            .args
            .iter()
            .any(|a| a.contains("stream-json") || a.contains("json"));

        Ok(Self {
            process: AgentProcess::spawn(&config.agent).await?,
            config,
            peer: Arc::new(peer),
            service,
            parser: OutputParser::new(is_json_mode),
            state: DriverState::Idle,
            agent_id: None,
            session_id: None,
        })
    }

    pub async fn run(config: RunnerConfig) -> Result<()> {
        let mut driver = Self::connect(config).await?;
        driver.register().await?;
        driver.main_loop().await
    }

    async fn register(&mut self) -> Result<()> {
        let mut args = serde_json::Map::new();
        args.insert(
            "project".into(),
            serde_json::Value::String(self.config.orchy.project.clone()),
        );
        args.insert(
            "description".into(),
            serde_json::Value::String(self.config.orchy.description.clone()),
        );

        if let Some(ns) = &self.config.orchy.namespace {
            args.insert("namespace".into(), serde_json::Value::String(ns.clone()));
        }

        if !self.config.orchy.roles.is_empty() {
            args.insert(
                "roles".into(),
                serde_json::Value::Array(
                    self.config
                        .orchy
                        .roles
                        .iter()
                        .map(|r| serde_json::Value::String(r.clone()))
                        .collect(),
                ),
            );
        }

        let result = self.call_tool("register_agent", args).await?;
        let text = extract_text(&result);

        if let Some(id) = extract_field(&text, "agent_id") {
            tracing::info!(agent_id = %id, "registered with orchy");
            self.agent_id = Some(id);
        } else {
            tracing::info!("registered with orchy (no agent_id in response)");
        }

        Ok(())
    }

    async fn main_loop(&mut self) -> Result<()> {
        let poll_interval = self.config.poll_interval();
        let heartbeat_interval = self.config.heartbeat_interval();

        let mut poll_timer = time::interval(poll_interval);
        let mut heartbeat_timer = time::interval(heartbeat_interval);

        // skip first immediate tick
        poll_timer.tick().await;
        heartbeat_timer.tick().await;

        tracing::info!(
            agent = %self.config.agent.name,
            poll_secs = poll_interval.as_secs(),
            heartbeat_secs = heartbeat_interval.as_secs(),
            "entering main loop"
        );

        loop {
            if self.state == DriverState::ShuttingDown {
                break;
            }

            if !self.process.is_running() {
                tracing::warn!(agent = %self.config.agent.name, "process exited");
                return Err(Error::ProcessExited);
            }

            tokio::select! {
                line = self.process.read_line() => {
                    match line {
                        Ok(Some(raw)) => self.handle_output(&raw).await?,
                        Ok(None) => {
                            tracing::warn!("process output stream closed");
                            return Err(Error::ProcessExited);
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "read error");
                        }
                    }
                }
                _ = poll_timer.tick() => {
                    if self.state == DriverState::Idle
                        && let Err(e) = self.poll_for_work().await {
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

    async fn handle_output(&mut self, raw: &str) -> Result<()> {
        let parsed = self.parser.parse(raw);

        match &parsed {
            ParsedOutput::Empty => {}
            ParsedOutput::Text(text) => {
                tracing::debug!(agent = %self.config.agent.name, text = %text, "output");
            }
            ParsedOutput::JsonEvent(event) => {
                tracing::debug!(agent = %self.config.agent.name, event = ?event, "json output");
            }
        }

        if let Some(sid) = self.parser.extract_session_id(&parsed) {
            self.session_id = Some(sid);
        }

        if self.parser.is_completion_signal(&parsed)
            && let DriverState::Working { task_id, .. } = &self.state
        {
            let summary = self
                .parser
                .extract_text(&parsed)
                .unwrap_or_else(|| "completed".to_string());

            tracing::info!(task_id = %task_id, "agent completed task");

            let tid = task_id.clone();
            self.complete_task(&tid, &summary).await?;
            self.state = DriverState::Idle;
        }

        Ok(())
    }

    async fn poll_for_work(&mut self) -> Result<()> {
        let mut args = serde_json::Map::new();
        args.insert("claim".into(), serde_json::Value::Bool(true));

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

        tracing::info!(task_id = %task_id, title = %title, "claimed task");

        self.state = DriverState::Working {
            task_id: task_id.clone(),
            title: title.clone(),
        };

        // start task
        let mut start_args = serde_json::Map::new();
        start_args.insert("task_id".into(), serde_json::Value::String(task_id.clone()));
        let _ = self.call_tool("start_task", start_args).await;

        // inject task into agent process
        let prompt = format!(
            "You have been assigned task {task_id}: {title}\n\
             Please complete this task. When done, provide a summary of what you did."
        );
        self.process.write_line(&prompt).await?;

        Ok(())
    }

    async fn check_mailbox(&mut self) -> Result<()> {
        let result = self
            .call_tool("check_mailbox", serde_json::Map::new())
            .await?;
        let text = extract_text(&result);

        if text.contains("no message") || text.contains("No message") || text.is_empty() {
            return Ok(());
        }

        tracing::info!(agent = %self.config.agent.name, "received messages");

        let prompt = format!("[SYSTEM MESSAGE]: {text}");
        self.process.write_line(&prompt).await?;

        Ok(())
    }

    async fn heartbeat(&mut self) -> Result<()> {
        self.call_tool("heartbeat", serde_json::Map::new()).await?;
        Ok(())
    }

    async fn complete_task(&mut self, task_id: &str, summary: &str) -> Result<()> {
        let mut args = serde_json::Map::new();
        args.insert(
            "task_id".into(),
            serde_json::Value::String(task_id.to_string()),
        );
        args.insert(
            "summary".into(),
            serde_json::Value::String(summary.to_string()),
        );

        self.call_tool("complete_task", args).await?;
        tracing::info!(task_id = %task_id, "task completed in orchy");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        tracing::info!(agent = %self.config.agent.name, "shutting down");

        // disconnect from orchy
        let _ = self.call_tool("disconnect", serde_json::Map::new()).await;

        // kill the process
        let _ = self.process.kill().await;

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
        self.state = DriverState::ShuttingDown;
    }
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
    // try JSON parse first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(val) = v.get(field)
    {
        return Some(val.as_str().unwrap_or(&val.to_string()).to_string());
    }

    // fallback: look for "field: value" or "field": "value" patterns
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
