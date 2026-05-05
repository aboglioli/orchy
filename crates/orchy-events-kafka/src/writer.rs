use async_trait::async_trait;

use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};

use orchy_events::io::Writer;
use orchy_events::{Error, Event, Result, SerializedEvent};

pub struct KafkaWriter {
    producer: FutureProducer,
    topic: String,
}

impl KafkaWriter {
    pub fn new(brokers: &[String], topic: impl Into<String>) -> Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers.join(","))
            .set("message.timeout.ms", "5000")
            .create()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(Self {
            producer,
            topic: topic.into(),
        })
    }

    fn body(event: &Event) -> Result<String> {
        let s = SerializedEvent::from_event(event)?;
        serde_json::to_string(&serde_json::json!({
            "id": s.id,
            "organization": s.organization,
            "namespace": s.namespace,
            "topic": s.topic,
            "key": s.key,
            "payload": s.payload,
            "content_type": s.content_type,
            "metadata": s.metadata,
            "timestamp": s.timestamp,
            "version": s.version,
        }))
        .map_err(|e| Error::Serialization(e.to_string()))
    }
}

#[async_trait]
impl Writer for KafkaWriter {
    async fn write(&self, event: &Event) -> Result<()> {
        let body = Self::body(event)?;
        let key = event.key().as_str().to_string();
        let record: FutureRecord<String, String> =
            FutureRecord::to(&self.topic).payload(&body).key(&key);
        self.producer
            .send(record, std::time::Duration::from_secs(5))
            .await
            .map_err(|(e, _)| Error::Store(e.to_string()))?;
        Ok(())
    }
}
