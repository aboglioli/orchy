use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::Stream;
use sqlx::postgres::PgListener;
use sqlx::{PgPool, Row};
use tokio::sync::watch;
use uuid::Uuid;

use orchy_events::io::Acker;
use orchy_events::io::Message;
use orchy_events::io::Reader;
use orchy_events::io::ackers::OnceAcker;
use orchy_events::{Error, Result, SerializedEvent};

pub struct ConsumerConfig {
    pub organization: String,
}

#[derive(Clone)]
pub(crate) struct PgAcker {
    pool: PgPool,
    group_id: String,
    seq: i64,
}

impl Acker for PgAcker {
    fn ack(&self) -> impl std::future::Future<Output = Result<()>> + Send {
        let pool = self.pool.clone();
        let group_id = self.group_id.clone();
        let seq = self.seq;
        async move {
            sqlx::query(
                "INSERT INTO consumer_offsets (group_id, last_seq, updated_at) \
                 VALUES ($1, $2, NOW()) \
                 ON CONFLICT (group_id) DO UPDATE \
                 SET last_seq = EXCLUDED.last_seq, updated_at = NOW()",
            )
            .bind(&group_id)
            .bind(seq)
            .execute(&pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
            Ok(())
        }
    }

    async fn nack(&self) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct PgStream {
    rx: tokio::sync::mpsc::Receiver<Result<Message<OnceAcker<PgAcker>>>>,
    shutdown_tx: watch::Sender<bool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for PgStream {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Stream for PgStream {
    type Item = Result<Message<OnceAcker<PgAcker>>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub(crate) struct PgReader {
    pool: PgPool,
    config: ConsumerConfig,
}

impl PgReader {
    pub(crate) fn new(pool: PgPool, config: ConsumerConfig) -> Self {
        Self { pool, config }
    }
}

impl Reader for PgReader {
    type Acker = OnceAcker<PgAcker>;
    type Stream = PgStream;

    fn read(&self, consumer_group_id: &str) -> Result<Self::Stream> {
        let pool = self.pool.clone();
        let group_id = consumer_group_id.to_string();
        let org = self.config.organization.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        let handle = tokio::spawn(async move {
            let mut seq = match load_seq(&pool, &group_id).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

            let mut listener = match PgListener::connect_with(&pool).await {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(Err(Error::Store(e.to_string()))).await;
                    return;
                }
            };

            if let Err(e) = listener.listen("orchy_events").await {
                let _ = tx.send(Err(Error::Store(e.to_string()))).await;
                return;
            }

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                match fetch_batch(&pool, &org, seq).await {
                    Err(e) => {
                        tracing::error!("event poll error: {e}");
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                            _ = shutdown_rx.changed() => break,
                        }
                    }
                    Ok(batch) if batch.is_empty() => {
                        tokio::select! {
                            result = listener.recv() => {
                                match result {
                                    Ok(notification) => {
                                        if notification.payload() != org {
                                            continue;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("listener recv error: {e}");
                                    }
                                }
                            }
                            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                            _ = shutdown_rx.changed() => break,
                        }
                    }
                    Ok(batch) => {
                        for (serialized, next_seq) in batch {
                            match serialized.to_event() {
                                Ok(event) => {
                                    let acker = OnceAcker::new(PgAcker {
                                        pool: pool.clone(),
                                        group_id: group_id.clone(),
                                        seq: next_seq,
                                    });
                                    if tx.send(Ok(Message::new(event, acker))).await.is_err() {
                                        return;
                                    }
                                    seq = next_seq;
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "skipping malformed event at seq {next_seq}: {e}"
                                    );
                                    seq = next_seq;
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(PgStream {
            rx,
            shutdown_tx,
            handle: Some(handle),
        })
    }
}

async fn load_seq(pool: &PgPool, group_id: &str) -> Result<i64> {
    let row = sqlx::query("SELECT last_seq FROM consumer_offsets WHERE group_id = $1")
        .bind(group_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
    Ok(row.map(|r| r.get::<i64, _>("last_seq")).unwrap_or(0))
}

async fn fetch_batch(
    pool: &PgPool,
    organization: &str,
    after_seq: i64,
) -> Result<Vec<(SerializedEvent, i64)>> {
    let rows = sqlx::query(
        "SELECT seq, id, organization, namespace, topic, payload, content_type, metadata, timestamp, version \
         FROM events \
         WHERE seq > $1 AND organization = $2 \
         ORDER BY seq ASC \
         LIMIT 100",
    )
    .bind(after_seq)
    .bind(organization)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Store(e.to_string()))?;

    rows.into_iter()
        .map(|row| {
            let seq: i64 = row.get("seq");
            let id: Uuid = row.get("id");
            let metadata_json: serde_json::Value = row.get("metadata");

            let serialized = SerializedEvent {
                id: id.to_string(),
                organization: row.get("organization"),
                namespace: row.get("namespace"),
                topic: row.get("topic"),
                payload: row.get("payload"),
                content_type: row.get("content_type"),
                metadata: serde_json::from_value(metadata_json).unwrap_or_default(),
                timestamp: row.get("timestamp"),
                version: row.get::<i64, _>("version") as u64,
            };

            Ok((serialized, seq))
        })
        .collect()
}
