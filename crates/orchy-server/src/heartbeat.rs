use std::sync::Arc;

use orchy_application::UpdateAgentStatusCommand;
use orchy_core::agent::AgentStatus;
use tokio::time::{Duration, interval};
use tracing::info;

use crate::container::Container;

pub async fn run_heartbeat_monitor(container: Arc<Container>) {
    let timeout = container.config.server.heartbeat_timeout_secs;
    let check_interval = Duration::from_secs(timeout.max(10) / 2);

    let mut ticker = interval(check_interval);

    loop {
        ticker.tick().await;

        match container.app.check_timed_out_agents.execute(timeout).await {
            Ok(agents) => {
                for agent in &agents {
                    match agent.status.as_str() {
                        "online" | "busy" => {
                            info!(agent_id = %agent.id, "agent idle, marking as idle");
                            let cmd = UpdateAgentStatusCommand {
                                agent_id: agent.id.clone(),
                                status: AgentStatus::Idle,
                            };
                            let _ = container.app.update_agent_status.execute(cmd).await;
                        }
                        "idle" => {
                            info!(agent_id = %agent.id, "agent stale (no disconnect)");
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "heartbeat check failed");
            }
        }
    }
}
