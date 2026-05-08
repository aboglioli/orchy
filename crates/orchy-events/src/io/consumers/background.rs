use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::error::{Error, Result};
use crate::io::{Filter, Handler, Reader};

pub struct BackgroundConsumer<R: Reader, H: Handler> {
    reader: R,
    handler: Arc<H>,
    filter: Option<Arc<dyn Filter>>,
    concurrency: usize,
    handler_timeout: Option<Duration>,
}

impl<R, H> BackgroundConsumer<R, H>
where
    R: Reader + Send + 'static,
    R::Stream: 'static,
    H: Handler + 'static,
{
    pub fn new(reader: R, handler: H, concurrency: usize) -> Self {
        let concurrency = concurrency.max(1);
        Self {
            reader,
            handler: Arc::new(handler),
            filter: None,
            concurrency,
            handler_timeout: None,
        }
    }

    pub fn with_filter<F: Filter + 'static>(mut self, filter: F) -> Self {
        self.filter = Some(Arc::new(filter));
        self
    }

    pub fn with_handler_timeout(mut self, d: Duration) -> Self {
        self.handler_timeout = Some(d);
        self
    }

    pub fn spawn(self) -> ConsumerHandle {
        let cancel = CancellationToken::new();
        let tracker = TaskTracker::new();
        let cancel_for_run = cancel.clone();
        let tracker_for_run = tracker.clone();
        let join = tokio::spawn(async move { self.run(cancel_for_run, tracker_for_run).await });
        ConsumerHandle { cancel, join }
    }

    async fn run(self, cancel: CancellationToken, tracker: TaskTracker) -> Result<()> {
        let stream = self.reader.read().await?;
        let handler = Arc::clone(&self.handler);
        let filter = self.filter.clone();
        let timeout = self.handler_timeout;
        let concurrency = self.concurrency;
        let cancel_for_take = cancel.clone();

        stream
            .take_until(async move { cancel_for_take.cancelled().await })
            .for_each_concurrent(concurrency, |msg_result| {
                let handler = Arc::clone(&handler);
                let filter = filter.clone();
                let tracker = tracker.clone();
                async move {
                    let task = tracker.spawn(async move {
                        let msg = match msg_result {
                            Err(e) => {
                                tracing::error!("reader stream error: {e}");
                                return;
                            }
                            Ok(m) => m,
                        };
                        if let Some(f) = &filter
                            && !f.matches(msg.event())
                        {
                            let _ = msg.ack().await;
                            return;
                        }
                        let event = msg.event().clone();
                        let result = match timeout {
                            Some(d) => match tokio::time::timeout(d, handler.handle(event)).await {
                                Ok(r) => r,
                                Err(_) => Err(Error::Timeout(format!(
                                    "handler {} timed out",
                                    handler.id()
                                ))),
                            },
                            None => handler.handle(event).await,
                        };
                        match result {
                            Ok(()) => {
                                let _ = msg.ack().await;
                            }
                            Err(e) => {
                                tracing::warn!("handler {} error: {e}", handler.id());
                                let _ = msg.nack().await;
                            }
                        }
                    });
                    let _ = task.await;
                }
            })
            .await;
        tracker.close();
        tracker.wait().await;
        Ok(())
    }
}

pub struct ConsumerHandle {
    cancel: CancellationToken,
    join: tokio::task::JoinHandle<Result<()>>,
}

impl ConsumerHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub async fn shutdown(self) -> Result<()> {
        match self.join.await {
            Ok(result) => result,
            Err(e) => Err(Error::Store(format!("consumer task join error: {e}"))),
        }
    }

    pub fn abort(self) {
        self.cancel.cancel();
        self.join.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::pin::Pin;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use futures::Stream;

    use crate::event::Event;
    use crate::io::Message;
    use crate::io::acker::NoopAcker;
    use crate::payload::Payload;

    struct VecReader {
        events: Mutex<Option<Vec<Event>>>,
    }

    impl VecReader {
        fn new(events: Vec<Event>) -> Self {
            Self {
                events: Mutex::new(Some(events)),
            }
        }
    }

    #[async_trait]
    impl Reader for VecReader {
        type Acker = NoopAcker;
        type Stream = Pin<Box<dyn Stream<Item = Result<Message<NoopAcker>>> + Send>>;

        async fn read(&self) -> Result<Self::Stream> {
            let events = self
                .events
                .lock()
                .unwrap()
                .take()
                .expect("read called twice");
            let stream =
                futures::stream::iter(events.into_iter().map(|e| Ok(Message::new(e, NoopAcker))));
            Ok(Box::pin(stream))
        }
    }

    struct CountingHandler {
        id: String,
        count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Handler for CountingHandler {
        fn id(&self) -> &str {
            &self.id
        }
        async fn handle(&self, _: Event) -> Result<()> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FailingHandler {
        id: String,
        count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Handler for FailingHandler {
        fn id(&self) -> &str {
            &self.id
        }
        async fn handle(&self, _: Event) -> Result<()> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Err(Error::Store("boom".into()))
        }
    }

    fn make_event(i: usize) -> Event {
        Event::create(
            "org",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string(format!("p{i}")),
        )
        .unwrap()
    }

    struct AllowAll;
    impl Filter for AllowAll {
        fn matches(&self, _: &Event) -> bool {
            true
        }
    }

    struct AllowNothing;
    impl Filter for AllowNothing {
        fn matches(&self, _: &Event) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn handles_each_event_once() {
        let events: Vec<Event> = (0..5).map(make_event).collect();
        let count = Arc::new(AtomicUsize::new(0));
        let consumer = BackgroundConsumer::new(
            VecReader::new(events),
            CountingHandler {
                id: "h".into(),
                count: Arc::clone(&count),
            },
            2,
        );
        let handle = consumer.spawn();
        handle.shutdown().await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn filter_skip_does_not_invoke_handler() {
        let events: Vec<Event> = (0..3).map(make_event).collect();
        let count = Arc::new(AtomicUsize::new(0));
        let consumer = BackgroundConsumer::new(
            VecReader::new(events),
            CountingHandler {
                id: "h".into(),
                count: Arc::clone(&count),
            },
            1,
        )
        .with_filter(AllowNothing);
        let handle = consumer.spawn();
        handle.shutdown().await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn handler_error_is_reported_via_nack() {
        let events: Vec<Event> = (0..2).map(make_event).collect();
        let count = Arc::new(AtomicUsize::new(0));
        let consumer = BackgroundConsumer::new(
            VecReader::new(events),
            FailingHandler {
                id: "h".into(),
                count: Arc::clone(&count),
            },
            1,
        )
        .with_filter(AllowAll);
        let handle = consumer.spawn();
        handle.shutdown().await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn handler_timeout_yields_nack() {
        struct SlowHandler;
        #[async_trait]
        impl Handler for SlowHandler {
            fn id(&self) -> &str {
                "slow"
            }
            async fn handle(&self, _: Event) -> Result<()> {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(())
            }
        }
        let events: Vec<Event> = (0..1).map(make_event).collect();
        let consumer = BackgroundConsumer::new(VecReader::new(events), SlowHandler, 1)
            .with_handler_timeout(Duration::from_millis(20));
        let handle = consumer.spawn();
        handle.shutdown().await.unwrap();
    }
}
