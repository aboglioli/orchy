use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;

use crate::error::Result;

pub type Work<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Every change a use case makes lands together or not at all: a command that fails halfway,
/// or loses a race to another writer, leaves the stores exactly as they were.
///
/// Runs nest: an inner run that fails undoes only its own changes, and nothing is written
/// until the outermost run succeeds. Events are appended only once the writes they describe
/// have landed.
#[async_trait]
pub trait UnitOfWork: Send + Sync {
    async fn run<'a>(&self, work: Work<'a>) -> Result<()>;
}
