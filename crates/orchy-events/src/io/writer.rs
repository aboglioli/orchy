use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;
use crate::event::Event;

pub type BoxWriter = Box<dyn Writer + Send + Sync>;
pub type ArcWriter = Arc<dyn Writer + Send + Sync>;

#[async_trait]
pub trait Writer: Send + Sync {
    async fn write(&self, event: &Event) -> Result<()>;

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        for event in events {
            self.write(event).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<T: Writer + ?Sized> Writer for Arc<T> {
    async fn write(&self, event: &Event) -> Result<()> {
        (**self).write(event).await
    }

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        (**self).write_all(events).await
    }
}

#[async_trait]
impl<T: Writer + ?Sized> Writer for Box<T> {
    async fn write(&self, event: &Event) -> Result<()> {
        (**self).write(event).await
    }

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        (**self).write_all(events).await
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

    #[async_trait]
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
            writes: writes.clone(),
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
            writes: writes.clone(),
        }
        .into_arced();
        let clone = writer.clone();
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
            writes: writes.clone(),
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
            writes: writes.clone(),
        }
        .into_arced();
        take(arced, &ev()).await;
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }
}
