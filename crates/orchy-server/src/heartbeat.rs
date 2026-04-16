use std::sync::Arc;

use orchy_core::agent::AgentStatus;
use orchy_core::task::{ReviewStore, WatcherStore};
use tokio::time::{Duration, interval};
use tracing::info;

use crate::container::Container;

pub async fn run_heartbeat_monitor(container: Arc<Container>) {
    let timeout = container.config.server.heartbeat_timeout_secs;
    let check_interval = Duration::from_secs(timeout.max(2) / 2);

    let mut ticker = interval(check_interval);

    loop {
        ticker.tick().await;

        match container.agent_service.find_timed_out(timeout).await {
            Ok(agents) => {
                for agent in &agents {
                    match agent.status() {
                        AgentStatus::Online | AgentStatus::Busy => {
                            info!(agent_id = %agent.id(), "agent idle, marking as idle");
                            let _ = container
                                .agent_service
                                .update_status(agent.id(), AgentStatus::Idle)
                                .await;
                        }
                        AgentStatus::Idle => {
                            info!(agent_id = %agent.id(), "idle agent timed out, disconnecting");
                            let _ = container.agent_service.disconnect(agent.id()).await;
                            if let Err(e) =
                                container.task_service.release_agent_tasks(agent.id()).await
                            {
                                tracing::error!(agent_id = %agent.id(), error = %e, "failed to release agent tasks");
                            }
                            let _ = container.lock_service.release_agent_locks(agent.id()).await;
                            let watchers =
                                WatcherStore::find_by_agent(&*container.store, agent.id())
                                    .await
                                    .unwrap_or_default();
                            for w in &watchers {
                                let _ = WatcherStore::delete(
                                    &*container.store,
                                    &w.task_id(),
                                    agent.id(),
                                )
                                .await;
                            }
                            let reviews =
                                ReviewStore::find_pending_for_agent(&*container.store, agent.id())
                                    .await
                                    .unwrap_or_default();
                            for mut r in reviews {
                                r.unassign_reviewer();
                                let _ = ReviewStore::save(&*container.store, &mut r).await;
                            }
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
