use std::time::Duration;

use orchy_events::io::acker::AckBufferConfig;
use orchy_events::{Error, OrganizationId, Result, StartFrom};

#[derive(Clone)]
pub struct SqsReaderConfig {
    pub queue_url: String,
    pub organization: OrganizationId,
    pub max_in_flight: usize,
    pub visibility_timeout: Duration,
    pub wait_time: Duration,
    pub ack_buffer: AckBufferConfig,
}

pub struct SqsReaderParams {
    pub queue_url: String,
    pub organization: OrganizationId,
    pub max_in_flight: usize,
    pub visibility_timeout: Duration,
    pub wait_time: Duration,
    pub ack_buffer: AckBufferConfig,
    pub start_from: StartFrom,
    pub end_at: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<usize>,
    pub consumer_group_id: Option<String>,
}

impl SqsReaderConfig {
    pub fn new(params: SqsReaderParams) -> Result<Self> {
        if !(1..=10).contains(&params.max_in_flight) {
            return Err(Error::InvalidStartFrom(
                "max_in_flight must be 1..=10".into(),
            ));
        }
        if params.wait_time > Duration::from_secs(20) {
            return Err(Error::InvalidStartFrom("wait_time max 20s".into()));
        }
        if params.visibility_timeout > Duration::from_secs(43200) {
            return Err(Error::InvalidStartFrom("visibility_timeout max 12h".into()));
        }
        if params.ack_buffer.max_pending == 0 || params.ack_buffer.max_pending > 10 {
            return Err(Error::InvalidStartFrom(
                "ack_buffer.max_pending must be 1..=10 for SQS".into(),
            ));
        }
        if !matches!(params.start_from, StartFrom::Latest) {
            return Err(Error::InvalidStartFrom(
                "SQS only supports StartFrom::Latest".into(),
            ));
        }
        if params.end_at.is_some() {
            return Err(Error::InvalidStartFrom(
                "SQS does not support end_at".into(),
            ));
        }
        if params.limit.is_some() {
            return Err(Error::InvalidStartFrom("SQS does not support limit".into()));
        }
        if params.consumer_group_id.is_some() {
            return Err(Error::InvalidStartFrom(
                "SQS uses queue URL as consumer identity; consumer_group_id is not supported"
                    .into(),
            ));
        }
        Ok(Self {
            queue_url: params.queue_url,
            organization: params.organization,
            max_in_flight: params.max_in_flight,
            visibility_timeout: params.visibility_timeout,
            wait_time: params.wait_time,
            ack_buffer: params.ack_buffer,
        })
    }

    pub fn defaults_for(
        queue_url: impl Into<String>,
        organization: OrganizationId,
    ) -> Result<Self> {
        Self::new(SqsReaderParams {
            queue_url: queue_url.into(),
            organization,
            max_in_flight: 10,
            visibility_timeout: Duration::from_secs(30),
            wait_time: Duration::from_secs(20),
            ack_buffer: AckBufferConfig {
                max_pending: 10,
                flush_interval: Duration::from_secs(1),
            },
            start_from: StartFrom::Latest,
            end_at: None,
            limit: None,
            consumer_group_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ack(n: usize) -> AckBufferConfig {
        AckBufferConfig {
            max_pending: n,
            flush_interval: Duration::from_secs(1),
        }
    }

    fn org() -> OrganizationId {
        OrganizationId::new("o").unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn params(
        max_in_flight: usize,
        visibility_timeout: Duration,
        wait_time: Duration,
        ack_pending: usize,
        start_from: StartFrom,
        end_at: Option<chrono::DateTime<chrono::Utc>>,
        limit: Option<usize>,
        consumer_group: Option<String>,
    ) -> SqsReaderParams {
        SqsReaderParams {
            queue_url: "q".to_string(),
            organization: org(),
            max_in_flight,
            visibility_timeout,
            wait_time,
            ack_buffer: ack(ack_pending),
            start_from,
            end_at,
            limit,
            consumer_group_id: consumer_group,
        }
    }

    #[test]
    fn defaults_ok() {
        SqsReaderConfig::defaults_for("https://q", org()).unwrap();
    }

    #[test]
    fn rejects_max_in_flight_above_10() {
        let r = SqsReaderConfig::new(params(
            11,
            Duration::from_secs(30),
            Duration::from_secs(20),
            10,
            StartFrom::Latest,
            None,
            None,
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_wait_time_above_20s() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(30),
            Duration::from_secs(21),
            10,
            StartFrom::Latest,
            None,
            None,
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_visibility_above_12h() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(43201),
            Duration::from_secs(20),
            10,
            StartFrom::Latest,
            None,
            None,
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_ack_buffer_above_10() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(30),
            Duration::from_secs(20),
            11,
            StartFrom::Latest,
            None,
            None,
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_earliest() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(30),
            Duration::from_secs(20),
            10,
            StartFrom::Earliest,
            None,
            None,
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_timestamp_start() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(30),
            Duration::from_secs(20),
            10,
            StartFrom::Timestamp(chrono::Utc::now()),
            None,
            None,
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_end_at() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(30),
            Duration::from_secs(20),
            10,
            StartFrom::Latest,
            Some(chrono::Utc::now()),
            None,
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_limit() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(30),
            Duration::from_secs(20),
            10,
            StartFrom::Latest,
            None,
            Some(5),
            None,
        ));
        assert!(r.is_err());
    }

    #[test]
    fn rejects_consumer_group_id() {
        let r = SqsReaderConfig::new(params(
            10,
            Duration::from_secs(30),
            Duration::from_secs(20),
            10,
            StartFrom::Latest,
            None,
            None,
            Some("g".to_string()),
        ));
        assert!(r.is_err());
    }
}
