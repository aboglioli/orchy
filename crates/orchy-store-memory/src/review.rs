use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{ReviewId, ReviewRequest, ReviewStatus, ReviewStore, TaskId};

use crate::MemoryBackend;

impl ReviewStore for MemoryBackend {
    async fn save(&self, review: &mut ReviewRequest) -> Result<()> {
        {
            let mut reviews = self
                .reviews
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            reviews.insert(review.id(), review.clone());
        }

        let events = review.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &ReviewId) -> Result<Option<ReviewRequest>> {
        let reviews = self
            .reviews
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(reviews.get(id).cloned())
    }

    async fn find_pending_for_agent(&self, agent_id: &AgentId) -> Result<Vec<ReviewRequest>> {
        let reviews = self
            .reviews
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(reviews
            .values()
            .filter(|r| {
                r.status() == ReviewStatus::Pending
                    && (r.reviewer() == Some(*agent_id) || r.reviewer().is_none())
            })
            .cloned()
            .collect())
    }

    async fn find_by_task(&self, task_id: &TaskId) -> Result<Vec<ReviewRequest>> {
        let reviews = self
            .reviews
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(reviews
            .values()
            .filter(|r| r.task_id() == *task_id)
            .cloned()
            .collect())
    }
}
