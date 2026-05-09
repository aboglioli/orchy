use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::error::Result;
use crate::io::message::Message;
use crate::io::{Acker, BoxAcker};

pub type BoxStream<A = BoxAcker> = Pin<Box<dyn Stream<Item = Result<Message<A>>> + Send>>;

pub trait Reader: Send + Sync {
    type Acker: Acker;
    type Stream: Stream<Item = Result<Message<Self::Acker>>> + Send;

    fn read(&self) -> impl Future<Output = Result<Self::Stream>> + Send;
}

impl<T: Reader + ?Sized> Reader for Arc<T> {
    type Acker = T::Acker;
    type Stream = T::Stream;

    fn read(&self) -> impl Future<Output = Result<Self::Stream>> + Send {
        (**self).read()
    }
}

impl<T: Reader + ?Sized> Reader for Box<T> {
    type Acker = T::Acker;
    type Stream = T::Stream;

    fn read(&self) -> impl Future<Output = Result<Self::Stream>> + Send {
        (**self).read()
    }
}

pub trait DynReader<A: Acker = BoxAcker>: Send + Sync {
    fn read_dyn<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<BoxStream<A>>> + Send + 'a>>;
}

struct DynReaderAdapter<R>(R);

impl<R> DynReader<BoxAcker> for DynReaderAdapter<R>
where
    R: Reader + Send + Sync + 'static,
    R::Acker: 'static,
    R::Stream: 'static,
{
    fn read_dyn<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<BoxStream<BoxAcker>>> + Send + 'a>> {
        Box::pin(async move {
            let stream = Reader::read(&self.0).await?;
            let erased: BoxStream<BoxAcker> = Box::pin(
                stream.map(|res| res.map(|msg| msg.map_acker(|a| Box::new(a) as BoxAcker))),
            );
            Ok(erased)
        })
    }
}

pub type BoxReader<A = BoxAcker> = Box<dyn DynReader<A>>;
pub type ArcReader<A = BoxAcker> = Arc<dyn DynReader<A>>;

impl<A: Acker + 'static> Reader for dyn DynReader<A> + '_ {
    type Acker = A;
    type Stream = BoxStream<A>;

    fn read(&self) -> impl Future<Output = Result<Self::Stream>> + Send {
        DynReader::read_dyn(self)
    }
}

pub trait ReaderExt: Reader + Send + Sync + Sized + 'static
where
    Self::Acker: 'static,
    Self::Stream: 'static,
{
    fn into_boxed(self) -> BoxReader {
        Box::new(DynReaderAdapter(self))
    }

    fn into_arced(self) -> ArcReader {
        Arc::new(DynReaderAdapter(self))
    }
}

impl<R> ReaderExt for R
where
    R: Reader + Send + Sync + 'static,
    R::Acker: 'static,
    R::Stream: 'static,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::stream;

    use crate::event::Event;
    use crate::io::Message;
    use crate::io::acker::NoopAcker;
    use crate::payload::Payload;

    struct UnitReader;

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
    async fn into_arced_yields_shared_reader() {
        let reader: ArcReader = UnitReader.into_arced();
        let clone = Arc::clone(&reader);
        let mut stream = clone.read().await.unwrap();
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

    fn _assert_box_passes_as_generic_reader() {
        fn _take<R: Reader>(_: R) {}
        let r: BoxReader = UnitReader.into_boxed();
        _take(r);
    }

    fn _assert_reader_dyn_safe() {
        fn _take(_: BoxReader) {}
    }
}
