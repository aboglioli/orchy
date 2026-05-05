use sqlx::PgPool;
use std::collections::HashMap;
use std::mem;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use tokio::task::JoinHandle;

use orchy_events::Result;
use orchy_events::io::{Acker, Filter, Handler, Reader};

pub use super::consumer::ConsumerConfig;
use super::consumer::PgReader;

struct SubscriptionHandle {
    join: JoinHandle<()>,
}

pub struct PgSubscriber {
    pool: PgPool,
    handles: Mutex<HashMap<String, SubscriptionHandle>>,
}

impl PgSubscriber {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            handles: Mutex::new(HashMap::new()),
        }
    }

    fn lock_handles(&self) -> MutexGuard<'_, HashMap<String, SubscriptionHandle>> {
        self.handles.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub async fn subscribe<H, F>(
        &self,
        group_id: impl Into<String>,
        organization: impl Into<String>,
        handler: H,
        filter: F,
    ) -> Result<()>
    where
        H: Handler + Send + Sync + 'static,
        F: Filter + Send + Sync + 'static,
    {
        let group_id = group_id.into();
        let handler = Arc::new(handler);
        let filter = Arc::new(filter);
        let reader = PgReader::new(
            self.pool.clone(),
            ConsumerConfig {
                organization: organization.into(),
            },
        );
        let mut stream = reader.read(&group_id).await?;

        let task = tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Err(e) => tracing::error!("stream error: {e}"),
                    Ok(msg) => {
                        let (event, acker) = msg.into_parts();
                        if !filter.matches(&event) {
                            let _ = acker.ack().await;
                            continue;
                        }
                        match handler.handle(event).await {
                            Ok(()) => {
                                let _ = acker.ack().await;
                            }
                            Err(e) => {
                                tracing::error!("handler error: {e}");
                                let _ = acker.nack().await;
                            }
                        }
                    }
                }
            }
        });

        self.lock_handles()
            .insert(group_id, SubscriptionHandle { join: task });
        Ok(())
    }

    pub fn unsubscribe(&self, group_id: &str) {
        if let Some(handle) = self.lock_handles().remove(group_id) {
            handle.join.abort();
        }
    }

    pub async fn close(&self) {
        let handles: HashMap<_, _> = mem::take(&mut *self.lock_handles());
        for (_, handle) in handles {
            handle.join.abort();
            let _ = handle.join.await;
        }
    }
}
