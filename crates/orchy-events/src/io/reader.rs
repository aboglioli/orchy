use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;

use crate::error::Result;
use crate::io::Acker;
use crate::io::message::Message;

pub type BoxAcker = Box<dyn Acker + Send + Sync>;
pub type BoxStream = Pin<Box<dyn Stream<Item = Result<Message<BoxAcker>>> + Send>>;

#[async_trait]
pub trait Reader: Send + Sync {
    type Acker: Acker;
    type Stream: Stream<Item = Result<Message<Self::Acker>>> + Send;

    async fn read(&self) -> Result<Self::Stream>;
}

#[async_trait]
impl<T: Reader + ?Sized> Reader for Arc<T> {
    type Acker = T::Acker;
    type Stream = T::Stream;

    async fn read(&self) -> Result<Self::Stream> {
        (**self).read().await
    }
}

pub struct BoxedReader {
    inner: Box<dyn ReaderObject + Send + Sync>,
}

#[async_trait]
trait ReaderObject {
    async fn read_box(&self) -> Result<BoxStream>;
}

#[async_trait]
impl<R> ReaderObject for R
where
    R: Reader + Send + Sync,
    R::Acker: 'static,
    R::Stream: 'static,
{
    async fn read_box(&self) -> Result<BoxStream> {
        use futures::StreamExt;
        let stream = self.read().await?;
        Ok(Box::pin(stream.map(|res| {
            res.map(|msg| {
                let (event, acker) = msg.into_parts();
                let boxed: BoxAcker = Box::new(acker);
                Message::new(event, boxed)
            })
        })))
    }
}

#[async_trait]
impl Reader for BoxedReader {
    type Acker = BoxAcker;
    type Stream = BoxStream;

    async fn read(&self) -> Result<Self::Stream> {
        self.inner.read_box().await
    }
}

pub trait ReaderExt: Reader + Send + Sync + 'static + Sized {
    fn into_boxed(self) -> BoxedReader {
        BoxedReader {
            inner: Box::new(self),
        }
    }
}

impl<R: Reader + Send + Sync + 'static> ReaderExt for R {}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use async_trait::async_trait;
    use futures::stream;

    use crate::event::Event;
    use crate::io::Message;
    use crate::io::ackers::NoopAcker;
    use crate::payload::Payload;

    struct UnitReader;

    #[async_trait]
    impl Reader for UnitReader {
        type Acker = NoopAcker;
        type Stream = Pin<Box<dyn Stream<Item = Result<Message<NoopAcker>>> + Send>>;

        async fn read(&self) -> Result<Self::Stream> {
            let event = Event::create(
                "org",
                "/x",
                "thing.happened",
                "k",
                Payload::from_string("p"),
            )
            .unwrap();
            let msg = Message::new(event, NoopAcker);
            Ok(Box::pin(stream::once(async move { Ok(msg) })))
        }
    }

    #[tokio::test]
    async fn into_boxed_yields_dyn_safe_reader() {
        let reader: BoxedReader = UnitReader.into_boxed();
        let arc: Arc<dyn Reader<Acker = BoxAcker, Stream = BoxStream> + Send + Sync> =
            Arc::new(reader);
        let mut stream = arc.read().await.unwrap();
        use futures::StreamExt;
        let msg = stream.next().await.unwrap().unwrap();
        msg.ack().await.unwrap();
    }

    fn _assert_reader_dyn_safe() {
        fn _take(_: Arc<dyn Reader<Acker = BoxAcker, Stream = BoxStream> + Send + Sync>) {}
    }
}
