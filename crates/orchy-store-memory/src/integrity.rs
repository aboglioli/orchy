use async_trait::async_trait;
use orchy_core::{Integrity, Problem, Result};

#[derive(Debug, Default)]
pub struct MemoryIntegrity;

impl MemoryIntegrity {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Integrity for MemoryIntegrity {
    async fn unreadable(&self) -> Result<Vec<Problem>> {
        Ok(Vec::new())
    }

    async fn problems(&self) -> Result<Vec<Problem>> {
        Ok(Vec::new())
    }

    async fn repair(&self, _problem: &Problem) -> Result<bool> {
        Ok(false)
    }
}
