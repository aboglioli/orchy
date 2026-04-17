use std::sync::Arc;

use orchy_application::{DisconnectAgentCommand, UpdateAgentStatusCommand};
use orchy_core::agent::AgentStatus;
use tokio::time::{Duration, interval};
use tracing::info;

use crate::container::Container;

pub async fn run_heartbeat_monitor(container: Arc<Container>) {
    let timeout = container.config.server.heartbeat_timeout_secs;
    let check_interval = Duration::from_secs(timeout.max(2) / 2);

    let mut ticker = interval(check_interval);

    loop {
        ticker.tick().await;

        match container.app.check_timed_out_agents.execute(timeout).await {
            Ok(agents) => {
                for agent in &agents {
                    match agent.status() {
                        AgentStatus::Online | AgentStatus::Busy => {
                            info!(agent_id = %agent.id(), "agent idle, marking as idle");
                            let cmd = UpdateAgentStatusCommand {
                                agent_id: agent.id().to_string(),
                                status: AgentStatus::Idle,
                            };
                            let _ = container.app.update_agent_status.execute(cmd).await;
                        }
                        AgentStatus::Idle => {
                            info!(agent_id = %agent.id(), "idle agent timed out, disconnecting");
                            let cmd = DisconnectAgentCommand {
                                agent_id: agent.id().to_string(),
                            };
                            let _ = container.app.disconnect_agent.execute(cmd).await;
                        }
                        AgentStatus::Disconnected => {}
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "heartbeat check failed");
            }
        }
    }
}
