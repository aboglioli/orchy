use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;

use orchy_events::io::{ArcWriter, BoxReader, ReaderExt, WriterExt};

use orchy_store_conformance::event_conformance_suite;
use orchy_store_conformance::events::{
    EventBackend, EventBackendCapabilities, EventHarness, ReaderFactory, ReaderSpec,
};
use orchy_store_memory::{
    MemoryEventWriter, MemoryReader, MemoryReaderConfig, MemoryState, OffsetMap,
};

struct MemoryEventBackend;

impl EventBackend for MemoryEventBackend {
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
            let state = Arc::new(MemoryState::new());
            let offsets = Arc::new(RwLock::new(OffsetMap::new()));

            let writer: ArcWriter = MemoryEventWriter::new(Arc::clone(&state)).into_arced();

            let factory_state = Arc::clone(&state);
            let factory_offsets = Arc::clone(&offsets);
            let reader_factory: ReaderFactory = Arc::new(move |spec: ReaderSpec| {
                let s = Arc::clone(&factory_state);
                let o = Arc::clone(&factory_offsets);
                Box::pin(async move {
                    let reader = MemoryReader::new(
                        s,
                        o,
                        MemoryReaderConfig {
                            organization: spec.organization,
                            consumer_group_id: spec.consumer_group_id,
                            stream: spec.stream,
                            start_from: spec.start_from,
                            topics: spec.topics,
                            namespace_prefix: spec.namespace_prefix,
                            end_at: spec.end_at,
                            limit: spec.limit,
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

event_conformance_suite!(MemoryEventBackend);
