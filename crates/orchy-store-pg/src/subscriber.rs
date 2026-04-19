use std::collections::HashMap;
use std::sync::Mutex;

use futures::StreamExt;
use futures::future::BoxFuture;
use sqlx::PgPool;
use tokio::task::JoinHandle;

use orchy_events::io::{Acker, Filter, Handler, Reader};
use orchy_events::{Event, Result};

pub use super::consumer::ConsumerConfig;
use super::consumer::PgReader;

type BoxedHandlerFn = dyn Fn(Event) -> BoxFuture<'static, Result<()>> + Send + Sync + 'static;

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

    pub fn subscribe<H, F>(
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
        let handler_fn = make_handler_fn(handler);
        let filter = std::sync::Arc::new(filter);
        let reader = PgReader::new(
            self.pool.clone(),
            ConsumerConfig {
                organization: organization.into(),
            },
        );
        let mut stream = reader.read(&group_id)?;

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
                        let h = handler_fn.clone();
                        match h(event).await {
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

        self.handles
            .lock()
            .unwrap()
            .insert(group_id, SubscriptionHandle { join: task });
        Ok(())
    }

    pub fn unsubscribe(&self, group_id: &str) {
        if let Some(handle) = self.handles.lock().unwrap().remove(group_id) {
            handle.join.abort();
        }
    }

    pub async fn close(&self) {
        let handles: HashMap<_, _> = std::mem::take(&mut *self.handles.lock().unwrap());
        for (_, handle) in handles {
            handle.join.abort();
            let _ = handle.join.await;
        }
    }
}

fn make_handler_fn<H: Handler + Send + Sync + 'static>(
    handler: H,
) -> std::sync::Arc<BoxedHandlerFn> {
    let handler = std::sync::Arc::new(handler);
    std::sync::Arc::new(move |event: Event| {
        let h = std::sync::Arc::clone(&handler);
        Box::pin(async move { h.handle(event).await }) as BoxFuture<'static, Result<()>>
    })
}
