use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageStore, MessageTarget};
use orchy_core::task::{ReviewId, ReviewRequest, ReviewStore};

pub struct ResolveReviewCommand {
    pub review_id: String,
    pub resolver_agent_id: String,
    pub approved: bool,
    pub comments: Option<String>,
}

pub struct ResolveReview {
    reviews: Arc<dyn ReviewStore>,
    messages: Arc<dyn MessageStore>,
}

impl ResolveReview {
    pub fn new(reviews: Arc<dyn ReviewStore>, messages: Arc<dyn MessageStore>) -> Self {
        Self { reviews, messages }
    }

    pub async fn execute(&self, cmd: ResolveReviewCommand) -> Result<ReviewRequest> {
        let review_id = cmd
            .review_id
            .parse::<ReviewId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let resolver = AgentId::from_str(&cmd.resolver_agent_id).map_err(Error::InvalidInput)?;

        let mut review = self
            .reviews
            .find_by_id(&review_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("review {review_id}")))?;

        if cmd.approved {
            review.approve(cmd.comments)?;
        } else {
            review.reject(cmd.comments)?;
        }
        self.reviews.save(&mut review).await?;

        let body = format!(
            "Review {} for task {}: {}",
            review.id(),
            review.task_id(),
            review.status()
        );
        let mut msg = Message::new(
            review.org_id().clone(),
            review.project().clone(),
            review.namespace().clone(),
            resolver,
            MessageTarget::Agent(review.requester().clone()),
            body,
            None,
        )?;
        let _ = self.messages.save(&mut msg).await;

        Ok(review)
    }
}
