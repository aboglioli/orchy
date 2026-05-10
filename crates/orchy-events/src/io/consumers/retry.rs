use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio::time;

use crate::error::{Error, Result};
use crate::event::Event;
use crate::io::{Handler, Writer};
use crate::payload::Payload;
use crate::serialization::SerializedEvent;

#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

pub enum RetryAction {
    Retry,
    DeadLetter(String),
    Skip,
}

pub trait RetryPolicy: Send + Sync {
    fn classify(&self, error: &Error, attempt: u32, max_attempts: u32) -> RetryAction;
}

pub struct DefaultRetryPolicy;

impl RetryPolicy for DefaultRetryPolicy {
    fn classify(&self, error: &Error, attempt: u32, max_attempts: u32) -> RetryAction {
        if attempt >= max_attempts {
            return RetryAction::DeadLetter(format!(
                "max attempts ({max_attempts}) exceeded: {error}"
            ));
        }
        RetryAction::Retry
    }
}

pub fn backoff_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1) as i32;
    let delay = config.base_delay.mul_f64(config.multiplier.powi(exponent));
    delay.min(config.max_delay)
}

pub struct DeadLetterWriter<W: Writer> {
    writer: W,
}

impl<W: Writer> DeadLetterWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub async fn send(
        &self,
        event: &Event,
        handler_id: &str,
        attempts: u32,
        reason: &str,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct DeadLetterPayload {
            original_event: Value,
            original_event_id: String,
            handler_id: String,
            attempts: u32,
            error: String,
            failed_at: DateTime<Utc>,
        }

        let serialized = SerializedEvent::from_event(event)?;
        let payload = DeadLetterPayload {
            original_event: serialized.to_json_value(),
            original_event_id: event.id().to_string(),
            handler_id: handler_id.to_owned(),
            attempts,
            error: reason.to_owned(),
            failed_at: Utc::now(),
        };
        let dead_letter = Event::create(
            event.organization().as_str(),
            event.namespace().as_str(),
            format!("{}.dead_letter", event.topic().as_str()),
            event.key().as_str(),
            Payload::from_json(&payload)?,
        )?;
        self.writer.write(&dead_letter).await
    }
}

pub struct RetryHandler<H, P, W>
where
    H: Handler,
    P: RetryPolicy,
    W: Writer,
{
    handler: H,
    policy: P,
    dead_letter: DeadLetterWriter<W>,
    config: RetryConfig,
}

impl<H, P, W> RetryHandler<H, P, W>
where
    H: Handler,
    P: RetryPolicy,
    W: Writer,
{
    pub fn new(
        handler: H,
        policy: P,
        dead_letter: DeadLetterWriter<W>,
        config: RetryConfig,
    ) -> Self {
        Self {
            handler,
            policy,
            dead_letter,
            config,
        }
    }
}

impl<H, P, W> Handler for RetryHandler<H, P, W>
where
    H: Handler,
    P: RetryPolicy,
    W: Writer,
{
    fn id(&self) -> &str {
        self.handler.id()
    }

    async fn handle(&self, event: Event) -> Result<()> {
        let mut last_error: Option<Error> = None;
        for attempt in 1..=self.config.max_attempts {
            match self.handler.handle(event.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => match self.policy.classify(&e, attempt, self.config.max_attempts) {
                    RetryAction::Retry => {
                        last_error = Some(e);
                        time::sleep(backoff_delay(&self.config, attempt)).await;
                    }
                    RetryAction::DeadLetter(reason) => {
                        self.dead_letter
                            .send(&event, self.handler.id(), attempt, &reason)
                            .await?;
                        return Ok(());
                    }
                    RetryAction::Skip => return Ok(()),
                },
            }
        }
        let reason = last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "handler failed without captured error".to_owned());
        self.dead_letter
            .send(&event, self.handler.id(), self.config.max_attempts, &reason)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ev() -> Event {
        Event::create(
            "org",
            "/x",
            "thing.happened",
            "k",
            Payload::from_string("p"),
        )
        .unwrap()
    }

    struct CountingFailHandler {
        id: String,
        fails_remaining: AtomicUsize,
        attempts: Arc<AtomicUsize>,
    }

    impl Handler for CountingFailHandler {
        fn id(&self) -> &str {
            &self.id
        }
        async fn handle(&self, _: Event) -> Result<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let prev = self.fails_remaining.load(Ordering::SeqCst);
            if prev == 0 {
                return Ok(());
            }
            self.fails_remaining.fetch_sub(1, Ordering::SeqCst);
            Err(Error::Store("transient failure".into()))
        }
    }

    struct AlwaysFailHandler {
        id: String,
        attempts: Arc<AtomicUsize>,
    }

    impl Handler for AlwaysFailHandler {
        fn id(&self) -> &str {
            &self.id
        }
        async fn handle(&self, _: Event) -> Result<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err(Error::Store("permanent failure".into()))
        }
    }

    #[derive(Clone)]
    struct CapturingWriter {
        events: Arc<Mutex<Vec<Event>>>,
    }

    impl CapturingWriter {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }
        fn captured(&self) -> Vec<Event> {
            self.events.lock().unwrap().clone()
        }
    }

    impl Writer for CapturingWriter {
        async fn write(&self, event: &Event) -> Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    fn fast_config() -> RetryConfig {
        RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(2),
            multiplier: 2.0,
        }
    }

    #[test]
    fn backoff_delay_grows_exponentially() {
        let config = RetryConfig {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
        };
        let d1 = backoff_delay(&config, 1);
        let d2 = backoff_delay(&config, 2);
        let d3 = backoff_delay(&config, 3);
        assert_eq!(d1, Duration::from_millis(100));
        assert!(d2 > d1);
        assert!(d3 > d2);
        assert_eq!(d2, Duration::from_millis(200));
        assert_eq!(d3, Duration::from_millis(400));
    }

    #[test]
    fn backoff_delay_capped_at_max() {
        let config = RetryConfig {
            max_attempts: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(500),
            multiplier: 2.0,
        };
        let d = backoff_delay(&config, 10);
        assert_eq!(d, Duration::from_millis(500));
    }

    #[tokio::test]
    async fn retry_handler_retries_until_success() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let handler = CountingFailHandler {
            id: "h".into(),
            fails_remaining: AtomicUsize::new(2),
            attempts: Arc::clone(&attempts),
        };
        let dl_writer = CapturingWriter::new();
        let dl_writer_clone = dl_writer.clone();
        let retry = RetryHandler::new(
            handler,
            DefaultRetryPolicy,
            DeadLetterWriter::new(dl_writer),
            fast_config(),
        );
        retry.handle(ev()).await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert!(dl_writer_clone.captured().is_empty());
    }

    #[tokio::test]
    async fn retry_handler_writes_dead_letter_after_max_attempts() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let handler = AlwaysFailHandler {
            id: "h".into(),
            attempts: Arc::clone(&attempts),
        };
        let dl_writer = CapturingWriter::new();
        let dl_writer_clone = dl_writer.clone();
        let retry = RetryHandler::new(
            handler,
            DefaultRetryPolicy,
            DeadLetterWriter::new(dl_writer),
            fast_config(),
        );
        retry.handle(ev()).await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        let captured = dl_writer_clone.captured();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].topic().as_str(), "thing.happened.dead_letter");
        assert_eq!(captured[0].organization().as_str(), "org");
        assert_eq!(captured[0].namespace().as_str(), "/x");
        assert_eq!(captured[0].key().as_str(), "k");
    }

    #[tokio::test]
    async fn dead_letter_payload_contains_original_event_and_handler_context() {
        let original = ev();
        let dl_writer = CapturingWriter::new();
        let dl_writer_clone = dl_writer.clone();
        let writer = DeadLetterWriter::new(dl_writer);
        writer
            .send(&original, "handler-xyz", 4, "boom")
            .await
            .unwrap();
        let captured = dl_writer_clone.captured();
        assert_eq!(captured.len(), 1);
        let dl = &captured[0];
        let payload: Value = serde_json::from_slice(dl.payload().data()).unwrap();
        let obj = payload.as_object().expect("payload is an object");
        assert!(obj.contains_key("original_event"));
        assert_eq!(
            obj.get("original_event_id").and_then(|v| v.as_str()),
            Some(original.id().to_string().as_str())
        );
        assert_eq!(
            obj.get("handler_id").and_then(|v| v.as_str()),
            Some("handler-xyz")
        );
        assert_eq!(obj.get("attempts").and_then(|v| v.as_u64()), Some(4));
        assert_eq!(obj.get("error").and_then(|v| v.as_str()), Some("boom"));
        assert!(obj.get("failed_at").and_then(|v| v.as_str()).is_some());

        let original_event = obj.get("original_event").unwrap();
        assert_eq!(
            original_event.get("id").and_then(|v| v.as_str()),
            Some(original.id().to_string().as_str())
        );
        assert_eq!(
            original_event.get("topic").and_then(|v| v.as_str()),
            Some("thing.happened")
        );
    }
}
