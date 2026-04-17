use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{ReviewId, ReviewRequest, ReviewStore};

pub struct GetReview {
    reviews: Arc<dyn ReviewStore>,
}

impl GetReview {
    pub fn new(reviews: Arc<dyn ReviewStore>) -> Self {
        Self { reviews }
    }

    pub async fn execute(&self, review_id: &str) -> Result<ReviewRequest> {
        let id = review_id
            .parse::<ReviewId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        self.reviews
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("review {id}")))
    }
}
