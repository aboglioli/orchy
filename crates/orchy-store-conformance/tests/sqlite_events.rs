use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use orchy_events::io::{ArcWriter, BoxReader, ReaderExt, WriterExt};

use orchy_store_conformance::event_conformance_suite;
use orchy_store_conformance::events::{
    EventBackend, EventBackendCapabilities, EventHarness, ReaderFactory, ReaderSpec,
};
use orchy_store_sqlite::{SqliteDatabase, SqliteEventWriter, SqliteReader, SqliteReaderConfig};

struct SqliteEventBackend;

impl EventBackend for SqliteEventBackend {
    fn capabilities() -> EventBackendCapabilities {
        EventBackendCapabilities {
            supports_replay: true,
            supports_timestamp_start: true,
            supports_nack_redelivery: false,
            preserves_total_order: true,
        }
    }

    fn build() -> Pin<Box<dyn Future<Output = EventHarness> + Send>> {
        Box::pin(async {
            let db = SqliteDatabase::new(":memory:", None).unwrap();
            db.run_migrations(&SqliteDatabase::migrations_dir())
                .unwrap();

            let conn_w = db.conn();
            let conn_r = db.conn();

            let writer: ArcWriter = SqliteEventWriter::new(conn_w).into_arced();

            let reader_factory: ReaderFactory = Arc::new(move |spec: ReaderSpec| {
                let conn = Arc::clone(&conn_r);
                Box::pin(async move {
                    let reader = SqliteReader::new(
                        conn,
                        SqliteReaderConfig {
                            organization: spec.organization,
                            consumer_group_id: spec.consumer_group_id,
                            stream: spec.stream,
                            start_from: spec.start_from,
                            topics: spec.topics,
                            namespace_prefix: spec.namespace_prefix,
                            end_at: spec.end_at,
                            limit: spec.limit,
                            poll_interval: Duration::from_millis(50),
                        },
                    );
                    let boxed: BoxReader = reader.into_boxed();
                    boxed
                })
            });

            EventHarness {
                writer,
                reader_factory,
            }
        })
    }
}

event_conformance_suite!(SqliteEventBackend);
