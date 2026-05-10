use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::Stream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use orchy_events::io::acker::{Either, NoopAcker, OnceAcker};
use orchy_events::io::{Acker, Message, Reader};
use orchy_events::{
    ConsumerGroupId, ConsumerStream, Error, Namespace, OrganizationId, Result, SerializedEvent,
    StartFrom, Topic,
};

use crate::SqliteConn;

#[derive(Clone)]
pub struct SqliteReaderConfig {
    pub organization: OrganizationId,
    pub consumer_group_id: Option<ConsumerGroupId>,
    pub stream: ConsumerStream,
    pub start_from: StartFrom,
    pub topics: Option<Vec<Topic>>,
    pub namespace_prefix: Option<Namespace>,
    pub end_at: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub poll_interval: Duration,
}

impl SqliteReaderConfig {
    pub fn new(organization: OrganizationId) -> Self {
        Self {
            organization,
            consumer_group_id: None,
            stream: ConsumerStream::default(),
            start_from: StartFrom::Latest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: None,
            poll_interval: Duration::from_secs(1),
        }
    }
}

/// SQLite-backed [`Acker`] persisting consumer offsets in `consumer_offsets`.
///
/// `ack` advances `consumer_offsets.last_seq` when the new sequence is higher;
/// older acks are silently dropped (monotonic checkpoint). `nack` leaves the
/// checkpoint unchanged.
#[derive(Clone)]
pub struct SqliteAcker {
    conn: SqliteConn,
    organization: OrganizationId,
    group_id: ConsumerGroupId,
    stream: ConsumerStream,
    seq: i64,
}

impl Acker for SqliteAcker {
    async fn ack(&self) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let organization = self.organization.clone();
        let group_id = self.group_id.clone();
        let stream = self.stream.clone();
        let seq = self.seq;
        let now = Utc::now().to_rfc3339();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock().map_err(|e| Error::Store(e.to_string()))?;
            conn.execute(
                "INSERT INTO consumer_offsets (organization, group_id, stream, last_seq, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(organization, group_id, stream) DO UPDATE \
                 SET last_seq = excluded.last_seq, updated_at = excluded.updated_at \
                 WHERE excluded.last_seq > consumer_offsets.last_seq",
                rusqlite::params![
                    organization.as_str(),
                    group_id.as_str(),
                    stream.as_str(),
                    seq,
                    now
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| Error::Store(format!("ack task panicked: {e}")))?
    }

    async fn nack(&self) -> Result<()> {
        Ok(())
    }
}

pub type SqliteAckerVariant = Either<OnceAcker<SqliteAcker>, NoopAcker>;

pub struct SqliteStream {
    rx: mpsc::Receiver<Result<Message<SqliteAckerVariant>>>,
    cancel: CancellationToken,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for SqliteStream {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

impl Stream for SqliteStream {
    type Item = Result<Message<SqliteAckerVariant>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub struct SqliteReader {
    conn: SqliteConn,
    config: SqliteReaderConfig,
}

impl SqliteReader {
    pub fn new(conn: SqliteConn, config: SqliteReaderConfig) -> Self {
        Self { conn, config }
    }
}

impl Reader for SqliteReader {
    type Acker = SqliteAckerVariant;
    type Stream = SqliteStream;

    async fn read(&self) -> Result<Self::Stream> {
        let conn = Arc::clone(&self.conn);
        let config = self.config.clone();
        // 64 in-flight messages: blocks the producer task when the consumer
        // stalls so we don't keep round-tripping to SQLite ahead of demand.
        let (tx, rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let handle = tokio::spawn(async move {
            let bounded = config.end_at.is_some() || config.limit.is_some();
            let (mut seq, lower_bound_ts) = match resolve_initial_position(&conn, &config) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };
            let mut delivered = 0usize;

            loop {
                if cancel_for_task.is_cancelled() {
                    break;
                }
                let take = config
                    .limit
                    .map(|l| l.saturating_sub(delivered))
                    .unwrap_or(100)
                    .clamp(1, 100);
                let batch = match fetch_batch(&conn, &config, seq, take, lower_bound_ts) {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                };
                if batch.is_empty() {
                    if bounded {
                        return;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(config.poll_interval) => continue,
                        _ = cancel_for_task.cancelled() => return,
                    }
                }
                for (serialized, next_seq) in batch {
                    let event = match serialized.to_event() {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::warn!("malformed event seq {next_seq}: {e}");
                            seq = next_seq;
                            continue;
                        }
                    };
                    let acker = if let Some(group) = config.consumer_group_id.as_ref() {
                        Either::Left(OnceAcker::new(SqliteAcker {
                            conn: Arc::clone(&conn),
                            organization: config.organization.clone(),
                            group_id: group.clone(),
                            stream: config.stream.clone(),
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

        Ok(SqliteStream {
            rx,
            cancel,
            handle: Some(handle),
        })
    }
}

fn resolve_initial_position(
    conn: &SqliteConn,
    config: &SqliteReaderConfig,
) -> Result<(i64, Option<DateTime<Utc>>)> {
    let conn = conn.lock().map_err(|e| Error::Store(e.to_string()))?;
    if let Some(group) = config.consumer_group_id.as_ref() {
        let row: Option<i64> = conn
            .query_row(
                "SELECT last_seq FROM consumer_offsets \
                 WHERE organization = ?1 AND group_id = ?2 AND stream = ?3",
                rusqlite::params![
                    config.organization.as_str(),
                    group.as_str(),
                    config.stream.as_str()
                ],
                |r| r.get(0),
            )
            .ok();
        if let Some(s) = row {
            return Ok((s, None));
        }
    }
    match config.start_from {
        StartFrom::Earliest => Ok((0, None)),
        StartFrom::Latest => {
            let max: i64 = conn
                .query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))
                .map_err(|e| Error::Store(e.to_string()))?;
            Ok((max, None))
        }
        StartFrom::Timestamp(ts) => {
            let s: Option<i64> = conn
                .query_row(
                    "SELECT MIN(seq) - 1 FROM events WHERE timestamp >= ?1",
                    rusqlite::params![ts.to_rfc3339()],
                    |r| r.get(0),
                )
                .ok();
            Ok((s.unwrap_or(0), Some(ts)))
        }
    }
}

fn fetch_batch(
    conn: &SqliteConn,
    config: &SqliteReaderConfig,
    after_seq: i64,
    take: usize,
    lower_bound_ts: Option<DateTime<Utc>>,
) -> Result<Vec<(SerializedEvent, i64)>> {
    let conn = conn.lock().map_err(|e| Error::Store(e.to_string()))?;
    let mut sql = String::from(
        "SELECT seq, id, organization, namespace, topic, key, payload, content_type, metadata, timestamp, version \
         FROM events WHERE seq > ?1 AND organization = ?2",
    );
    let mut bindings: Vec<rusqlite::types::Value> = vec![
        after_seq.into(),
        config.organization.as_str().to_owned().into(),
    ];

    if let Some(topics) = config.topics.as_ref() {
        let placeholders: Vec<String> = (0..topics.len())
            .map(|i| format!("?{}", bindings.len() + 1 + i))
            .collect();
        sql.push_str(&format!(" AND topic IN ({})", placeholders.join(", ")));
        for t in topics {
            bindings.push(t.as_str().to_owned().into());
        }
    }
    if let Some(prefix) = config.namespace_prefix.as_ref()
        && !prefix.is_root()
    {
        let i1 = bindings.len() + 1;
        let i2 = bindings.len() + 2;
        sql.push_str(&format!(" AND (namespace = ?{i1} OR namespace LIKE ?{i2})"));
        bindings.push(prefix.as_str().to_owned().into());
        bindings.push(format!("{}/%", prefix.as_str()).into());
    }
    if let Some(ts) = lower_bound_ts {
        let i = bindings.len() + 1;
        sql.push_str(&format!(" AND timestamp >= ?{i}"));
        bindings.push(ts.to_rfc3339().into());
    }
    if let Some(ts) = config.end_at {
        let i = bindings.len() + 1;
        sql.push_str(&format!(" AND timestamp <= ?{i}"));
        bindings.push(ts.to_rfc3339().into());
    }
    let i = bindings.len() + 1;
    sql.push_str(&format!(" ORDER BY seq ASC LIMIT ?{i}"));
    bindings.push((take as i64).into());

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| Error::Store(e.to_string()))?;
    let params: Vec<&dyn rusqlite::ToSql> =
        bindings.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let rows = stmt
        .query_map(&*params, |row| {
            let seq: i64 = row.get(0)?;
            let payload_str: String = row.get(6)?;
            let metadata_str: String = row.get(8)?;
            let timestamp_str: String = row.get(9)?;
            let serialized = SerializedEvent {
                id: row.get(1)?,
                organization: row.get(2)?,
                namespace: row.get(3)?,
                topic: row.get(4)?,
                key: row.get(5)?,
                payload: crate::decode_json(&payload_str, "payload")?,
                content_type: row.get(7)?,
                metadata: crate::decode_json(&metadata_str, "metadata")?,
                timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            9,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                version: row.get::<_, i64>(10)? as u64,
            };
            Ok((serialized, seq))
        })
        .map_err(|e| Error::Store(e.to_string()))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| Error::Store(e.to_string()))?);
    }
    Ok(out)
}
