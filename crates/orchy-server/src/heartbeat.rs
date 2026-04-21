use std::sync::Arc;

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
                    info!(agent_id = %agent.id, "agent heartbeat timeout");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "heartbeat check failed");
            }
        }
    }
}
