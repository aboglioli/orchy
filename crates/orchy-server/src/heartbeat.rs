use std::sync::Arc;

use tokio::time::{Duration, sleep};
use tracing::{debug, error, warn};

use crate::container::Container;

const MAX_BACKOFF_SECS: u64 = 60;
const HARD_FAIL_THRESHOLD: u32 = 30;

pub async fn run_heartbeat_monitor(container: Arc<Container>) {
    let timeout = container.config.server.heartbeat_timeout_secs;
    let base_interval = Duration::from_secs(timeout.max(10) / 2);

    let mut consecutive_failures: u32 = 0;

    loop {
        let interval = compute_interval(base_interval, consecutive_failures);
        sleep(interval).await;

        match container.app.check_timed_out_agents.execute(timeout).await {
            Ok(result) => {
                if consecutive_failures > 0 {
                    warn!(
                        failures = consecutive_failures,
                        "heartbeat monitor recovered"
                    );
                }
                consecutive_failures = 0;
                for agent in &result.agents {
                    debug!(agent_id = %agent.id, "agent heartbeat timeout");
                }
                if !result.agents.is_empty() {
                    tracing::info!(
                        agents = result.agents.len(),
                        locks_released = result.locks_released,
                        tasks_released = result.tasks_released,
                        "agent timeout cleanup"
                    );
                }
            }
            Err(e) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                error!(error = %e, failures = consecutive_failures, "heartbeat check failed");
                if consecutive_failures >= HARD_FAIL_THRESHOLD {
                    error!(
                        failures = consecutive_failures,
                        "heartbeat monitor exceeded hard failure threshold; aborting process for supervisor restart"
                    );
                    std::process::exit(2);
                }
            }
        }
    }
}

fn compute_interval(base: Duration, failures: u32) -> Duration {
    if failures == 0 {
        return base;
    }
    let backoff = base
        .as_secs()
        .saturating_mul(2u64.saturating_pow(failures.min(6)));
    Duration::from_secs(backoff.min(MAX_BACKOFF_SECS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_caps_at_max() {
        let base = Duration::from_secs(15);
        for failures in 0..20u32 {
            let interval = compute_interval(base, failures);
            assert!(interval.as_secs() <= MAX_BACKOFF_SECS);
        }
    }

    #[test]
    fn zero_failures_uses_base_interval() {
        let base = Duration::from_secs(15);
        assert_eq!(compute_interval(base, 0), base);
    }

    #[test]
    fn first_failure_doubles_interval() {
        let base = Duration::from_secs(5);
        assert_eq!(compute_interval(base, 1), Duration::from_secs(10));
    }
}
