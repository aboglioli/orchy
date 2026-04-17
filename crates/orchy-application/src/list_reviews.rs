use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::pagination::{Page, PageParams};
use orchy_core::task::{ReviewRequest, ReviewStore, TaskId};

pub struct ListReviewsCommand {
    pub task_id: String,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

pub struct ListReviews {
    reviews: Arc<dyn ReviewStore>,
}

impl ListReviews {
    pub fn new(reviews: Arc<dyn ReviewStore>) -> Self {
        Self { reviews }
    }

    pub async fn execute(&self, cmd: ListReviewsCommand) -> Result<Page<ReviewRequest>> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let page = PageParams::new(cmd.after, cmd.limit);
        self.reviews.find_by_task(&task_id, page).await
    }
}
