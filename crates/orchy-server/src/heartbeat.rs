use std::sync::Arc;

use tokio::time::{Duration, interval};
use tracing::info;

use crate::container::Container;

pub async fn run_heartbeat_monitor(container: Arc<Container>) {
    let timeout = container.config.server.heartbeat_timeout_secs;
    let check_interval = Duration::from_secs(timeout.max(2) / 2);

    let mut ticker = interval(check_interval);

    loop {
        ticker.tick().await;

        match container.agent_service.disconnect_timed_out(timeout).await {
            Ok(disconnected) => {
                for agent_id in &disconnected {
                    info!(%agent_id, "agent timed out, disconnecting");
                    if let Err(e) = container.task_service.release_agent_tasks(agent_id).await {
                        tracing::error!(%agent_id, error = %e, "failed to release agent tasks");
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "heartbeat check failed");
            }
        }
    }
}
