use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;

use crate::error::Result;

pub type Work<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

#[async_trait]
pub trait UnitOfWork: Send + Sync {
    async fn run<'a>(&self, work: Work<'a>) -> Result<()>;
}
