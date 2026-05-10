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
        SerializedEvent::from_event(event)?.to_json_string()
    }
}

impl Writer for KafkaWriter {
    async fn write(&self, event: &Event) -> Result<()> {
        let body = Self::body(event)?;
        let key = event.key().as_str().to_owned();
        let record: FutureRecord<String, String> =
            FutureRecord::to(&self.topic).payload(&body).key(&key);
        self.producer
            .send(record, std::time::Duration::from_secs(5))
            .await
            .map_err(|(e, _)| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let payloads: Vec<(String, String)> = events
            .iter()
            .map(|e| Ok::<_, Error>((Self::body(e)?, e.key().as_str().to_owned())))
            .collect::<Result<_>>()?;

        let futs = payloads.iter().map(|(body, key)| {
            let record: FutureRecord<String, String> =
                FutureRecord::to(&self.topic).payload(body).key(key);
            self.producer
                .send(record, std::time::Duration::from_secs(5))
        });

        for fut in futs {
            fut.await.map_err(|(e, _)| Error::Store(e.to_string()))?;
        }
        Ok(())
    }
}
