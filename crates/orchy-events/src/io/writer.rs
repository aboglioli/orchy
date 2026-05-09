use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::Result;
use crate::event::Event;

pub trait Writer: Send + Sync {
    fn write<'a>(&'a self, event: &'a Event) -> impl Future<Output = Result<()>> + Send + 'a;

    fn write_all<'a>(
        &'a self,
        events: &'a [Event],
    ) -> impl Future<Output = Result<()>> + Send + 'a {
        async move {
            for event in events {
                self.write(event).await?;
            }
            Ok(())
        }
    }
}

impl<T: Writer + ?Sized> Writer for Arc<T> {
    fn write<'a>(&'a self, event: &'a Event) -> impl Future<Output = Result<()>> + Send + 'a {
        (**self).write(event)
    }

    fn write_all<'a>(
        &'a self,
        events: &'a [Event],
    ) -> impl Future<Output = Result<()>> + Send + 'a {
        (**self).write_all(events)
    }
}

impl<T: Writer + ?Sized> Writer for Box<T> {
    fn write<'a>(&'a self, event: &'a Event) -> impl Future<Output = Result<()>> + Send + 'a {
        (**self).write(event)
    }

    fn write_all<'a>(
        &'a self,
        events: &'a [Event],
    ) -> impl Future<Output = Result<()>> + Send + 'a {
        (**self).write_all(events)
    }
}

pub trait DynWriter: Send + Sync {
    fn write_dyn<'a>(
        &'a self,
        event: &'a Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
    fn write_all_dyn<'a>(
        &'a self,
        events: &'a [Event],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

impl<T: Writer + ?Sized> DynWriter for T {
    fn write_dyn<'a>(
        &'a self,
        event: &'a Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(<Self as Writer>::write(self, event))
    }
    fn write_all_dyn<'a>(
        &'a self,
        events: &'a [Event],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(<Self as Writer>::write_all(self, events))
    }
}

pub type BoxWriter = Box<dyn DynWriter>;
pub type ArcWriter = Arc<dyn DynWriter>;

impl Writer for dyn DynWriter + '_ {
    fn write<'a>(&'a self, event: &'a Event) -> impl Future<Output = Result<()>> + Send + 'a {
        DynWriter::write_dyn(self, event)
    }
    fn write_all<'a>(
        &'a self,
        events: &'a [Event],
    ) -> impl Future<Output = Result<()>> + Send + 'a {
        DynWriter::write_all_dyn(self, events)
    }
}

pub trait WriterExt: Writer + Sized + 'static {
    fn into_boxed(self) -> BoxWriter {
        Box::new(self)
    }

    fn into_arced(self) -> ArcWriter {
        Arc::new(self)
    }
}

impl<T: Writer + 'static> WriterExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::payload::Payload;

    struct CountingWriter {
        writes: Arc<AtomicUsize>,
    }

    impl Writer for CountingWriter {
        async fn write(&self, _: &Event) -> Result<()> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

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

    #[tokio::test]
    async fn into_boxed_yields_dyn_writer() {
        let writes = Arc::new(AtomicUsize::new(0));
        let writer: BoxWriter = CountingWriter {
            writes: Arc::clone(&writes),
        }
        .into_boxed();
        writer.write(&ev()).await.unwrap();
        writer.write_all(&[ev(), ev()]).await.unwrap();
        assert_eq!(writes.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn into_arced_yields_shared_writer() {
        let writes = Arc::new(AtomicUsize::new(0));
        let writer: ArcWriter = CountingWriter {
            writes: Arc::clone(&writes),
        }
        .into_arced();
        let clone = Arc::clone(&writer);
        writer.write(&ev()).await.unwrap();
        clone.write(&ev()).await.unwrap();
        assert_eq!(writes.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn box_blanket_passes_as_generic_writer() {
        async fn take<W: Writer>(w: W, e: &Event) {
            w.write(e).await.unwrap();
        }
        let writes = Arc::new(AtomicUsize::new(0));
        let boxed: BoxWriter = CountingWriter {
            writes: Arc::clone(&writes),
        }
        .into_boxed();
        take(boxed, &ev()).await;
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn arc_blanket_passes_as_generic_writer() {
        async fn take<W: Writer>(w: W, e: &Event) {
            w.write(e).await.unwrap();
        }
        let writes = Arc::new(AtomicUsize::new(0));
        let arced: ArcWriter = CountingWriter {
            writes: Arc::clone(&writes),
        }
        .into_arced();
        take(arced, &ev()).await;
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }
}
