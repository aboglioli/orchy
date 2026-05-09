use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use sqlx::postgres::PgListener;
use sqlx::{PgPool, Row};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use orchy_events::io::acker::{Either, NoopAcker, OnceAcker};
use orchy_events::io::{Acker, Message, Reader};
use orchy_events::{
    ConsumerGroupId, Error, Namespace, OrganizationId, Result, SerializedEvent, StartFrom, Topic,
};

#[derive(Clone)]
pub struct PgReaderConfig {
    pub organization: OrganizationId,
    pub consumer_group_id: Option<ConsumerGroupId>,
    pub start_from: StartFrom,
    pub topics: Option<Vec<Topic>>,
    pub namespace_prefix: Option<Namespace>,
    pub end_at: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub batch_size: usize,
    pub poll_interval: Duration,
}

impl PgReaderConfig {
    pub fn new(organization: OrganizationId) -> Self {
        Self {
            organization,
            consumer_group_id: None,
            start_from: StartFrom::Latest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: None,
            batch_size: 100,
            poll_interval: Duration::from_secs(5),
        }
    }
}

#[derive(Clone)]
pub struct PgAcker {
    pool: PgPool,
    group_id: ConsumerGroupId,
    seq: i64,
}

impl Acker for PgAcker {
    async fn ack(&self) -> Result<()> {
        sqlx::query(
            "INSERT INTO consumer_offsets (group_id, last_seq, updated_at) \
             VALUES ($1, $2, NOW()) \
             ON CONFLICT (group_id) DO UPDATE \
             SET last_seq = EXCLUDED.last_seq, updated_at = NOW()",
        )
        .bind(self.group_id.as_str())
        .bind(self.seq)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn nack(&self) -> Result<()> {
        Ok(())
    }
}

pub type PgAckerVariant = Either<OnceAcker<PgAcker>, NoopAcker>;

pub struct PgStream {
    rx: mpsc::Receiver<Result<Message<PgAckerVariant>>>,
    cancel: CancellationToken,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for PgStream {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

impl Stream for PgStream {
    type Item = Result<Message<PgAckerVariant>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub struct PgReader {
    pool: PgPool,
    config: PgReaderConfig,
}

impl PgReader {
    pub fn new(pool: PgPool, config: PgReaderConfig) -> Self {
        Self { pool, config }
    }
}

impl Reader for PgReader {
    type Acker = PgAckerVariant;
    type Stream = PgStream;

    async fn read(&self) -> Result<Self::Stream> {
        let pool = self.pool.clone();
        let config = self.config.clone();
        // Sized to the configured batch_size (clamped to [1, 64]) so the
        // producer can stage one batch ahead of the consumer; blocks
        // producer when consumer is slow.
        let (tx, rx) = mpsc::channel(config.batch_size.clamp(1, 64));
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let handle = tokio::spawn(async move {
            let mut delivered: usize = 0;
            let bounded = config.end_at.is_some() || config.limit.is_some();

            let (mut seq, lower_bound_ts) = match resolve_initial_position(&pool, &config).await {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

            let mut listener = if !bounded {
                match PgListener::connect_with(&pool).await {
                    Ok(mut l) => {
                        if let Err(e) = l.listen("orchy_events").await {
                            let _ = tx.send(Err(Error::Store(e.to_string()))).await;
                            return;
                        }
                        Some(l)
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Error::Store(e.to_string()))).await;
                        return;
                    }
                }
            } else {
                None
            };

            loop {
                if cancel_for_task.is_cancelled() {
                    break;
                }

                let limit_remaining = config
                    .limit
                    .map(|l| l.saturating_sub(delivered))
                    .unwrap_or(config.batch_size);
                let take = limit_remaining.min(config.batch_size).max(1);

                let batch = match fetch_batch(&pool, &config, seq, take, lower_bound_ts).await {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        if bounded {
                            break;
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(1)) => continue,
                            _ = cancel_for_task.cancelled() => break,
                        }
                    }
                };

                if batch.is_empty() {
                    if bounded {
                        break;
                    }
                    if let Some(l) = listener.as_mut() {
                        tokio::select! {
                            _ = l.recv() => {}
                            _ = tokio::time::sleep(config.poll_interval) => {}
                            _ = cancel_for_task.cancelled() => break,
                        }
                    } else {
                        tokio::select! {
                            _ = tokio::time::sleep(config.poll_interval) => {}
                            _ = cancel_for_task.cancelled() => break,
                        }
                    }
                    continue;
                }

                for (serialized, next_seq) in batch {
                    let event = match serialized.to_event() {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::warn!("skipping malformed event seq {next_seq}: {e}");
                            seq = next_seq;
                            continue;
                        }
                    };
                    let acker = if let Some(group) = config.consumer_group_id.as_ref() {
                        Either::Left(OnceAcker::new(PgAcker {
                            pool: pool.clone(),
                            group_id: group.clone(),
                            seq: next_seq,
                        }))
                    } else {
                        Either::Right(NoopAcker)
                    };
                    if tx.send(Ok(Message::new(event, acker))).await.is_err() {
                        return;
                    }
                    seq = next_seq;
                    delivered += 1;
                    if let Some(l) = config.limit
                        && delivered >= l
                    {
                        return;
                    }
                }
            }
        });

        Ok(PgStream {
            rx,
            cancel,
            handle: Some(handle),
        })
    }
}

async fn resolve_initial_position(
    pool: &PgPool,
    config: &PgReaderConfig,
) -> Result<(i64, Option<DateTime<Utc>>)> {
    if let Some(group) = config.consumer_group_id.as_ref() {
        let row = sqlx::query("SELECT last_seq FROM consumer_offsets WHERE group_id = $1")
            .bind(group.as_str())
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        if let Some(r) = row {
            return Ok((r.get::<i64, _>("last_seq"), None));
        }
    }
    match config.start_from {
        StartFrom::Earliest => Ok((0, None)),
        StartFrom::Latest => {
            let row = sqlx::query("SELECT COALESCE(MAX(seq), 0) AS s FROM events")
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?;
            Ok((row.get::<i64, _>("s"), None))
        }
        StartFrom::Timestamp(ts) => {
            let row = sqlx::query(
                "SELECT COALESCE(MIN(seq), 0) - 1 AS s FROM events WHERE timestamp >= $1",
            )
            .bind(ts)
            .fetch_one(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
            Ok((row.get::<i64, _>("s"), Some(ts)))
        }
    }
}

async fn fetch_batch(
    pool: &PgPool,
    config: &PgReaderConfig,
    after_seq: i64,
    take: usize,
    lower_bound_ts: Option<DateTime<Utc>>,
) -> Result<Vec<(SerializedEvent, i64)>> {
    let mut sql = String::from(
        "SELECT seq, id, organization, namespace, topic, key, payload, content_type, metadata, timestamp, version \
         FROM events WHERE seq > $1 AND organization = $2",
    );
    let mut bind_index = 3usize;

    if config.topics.is_some() {
        sql.push_str(&format!(" AND topic = ANY(${bind_index})"));
        bind_index += 1;
    }
    if let Some(prefix) = config.namespace_prefix.as_ref()
        && !prefix.is_root()
    {
        sql.push_str(&format!(
            " AND (namespace = ${bind_index} OR namespace LIKE ${bind_index} || '/%')"
        ));
        bind_index += 1;
    }
    if lower_bound_ts.is_some() {
        sql.push_str(&format!(" AND timestamp >= ${bind_index}"));
        bind_index += 1;
    }
    if config.end_at.is_some() {
        sql.push_str(&format!(" AND timestamp <= ${bind_index}"));
        bind_index += 1;
    }
    sql.push_str(&format!(" ORDER BY seq ASC LIMIT ${bind_index}"));

    let mut q = sqlx::query(&sql)
        .bind(after_seq)
        .bind(config.organization.as_str());
    if let Some(topics) = config.topics.as_ref() {
        let topics: Vec<String> = topics.iter().map(|t| t.as_str().to_string()).collect();
        q = q.bind(topics);
    }
    if let Some(prefix) = config.namespace_prefix.as_ref()
        && !prefix.is_root()
    {
        q = q.bind(prefix.as_str().to_string());
    }
    if let Some(ts) = lower_bound_ts {
        q = q.bind(ts);
    }
    if let Some(end_at) = config.end_at {
        q = q.bind(end_at);
    }
    q = q.bind(take as i64);

    let rows = q
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
                key: row.get("key"),
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
