use async_trait::async_trait;

use crate::error::Result;
use crate::io::Acker;

#[derive(Debug, Clone)]
pub struct NoopAcker;

#[async_trait]
impl Acker for NoopAcker {
    async fn ack(&self) -> Result<()> {
        Ok(())
    }

    async fn nack(&self) -> Result<()> {
        Ok(())
    }
}
