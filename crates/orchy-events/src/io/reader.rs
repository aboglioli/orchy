use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};

use crate::error::Result;
use crate::io::Acker;
use crate::io::message::Message;

pub type BoxAcker = Box<dyn Acker + Send + Sync>;
pub type BoxStream<A = BoxAcker> = Pin<Box<dyn Stream<Item = Result<Message<A>>> + Send>>;
pub type BoxReader<A = BoxAcker> = Box<dyn Reader<Acker = A, Stream = BoxStream<A>> + Send + Sync>;

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

pub trait ReaderExt: Reader + Send + Sync + Sized + 'static
where
    Self::Acker: 'static,
    Self::Stream: 'static,
{
    fn into_boxed(self) -> BoxReader<BoxAcker> {
        Box::new(DynReader(self))
    }
}

impl<R> ReaderExt for R
where
    R: Reader + Send + Sync + 'static,
    R::Acker: 'static,
    R::Stream: 'static,
{
}

struct DynReader<R>(R);

#[async_trait]
impl<R> Reader for DynReader<R>
where
    R: Reader + Send + Sync + 'static,
    R::Acker: 'static,
    R::Stream: 'static,
{
    type Acker = BoxAcker;
    type Stream = BoxStream<BoxAcker>;

    async fn read(&self) -> Result<BoxStream<BoxAcker>> {
        let stream = self.0.read().await?;
        Ok(Box::pin(stream.map(|res| {
            res.map(|msg| msg.map_acker(|a| Box::new(a) as BoxAcker))
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let reader: BoxReader = UnitReader.into_boxed();
        let mut stream = reader.read().await.unwrap();
        let msg = stream.next().await.unwrap().unwrap();
        msg.ack().await.unwrap();
    }

    #[tokio::test]
    async fn vec_of_boxed_readers_dispatches_each() {
        let readers: Vec<BoxReader> = vec![UnitReader.into_boxed(), UnitReader.into_boxed()];
        for r in &readers {
            let mut stream = r.read().await.unwrap();
            let msg = stream.next().await.unwrap().unwrap();
            msg.ack().await.unwrap();
        }
    }

    fn _assert_reader_dyn_safe() {
        fn _take(_: BoxReader) {}
        fn _take_with_concrete_acker(_: BoxReader<NoopAcker>) {}
    }
}
