use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::Result;
use crate::event::Event;

pub trait Handler: Send + Sync {
    fn id(&self) -> &str;
    fn handle(&self, event: Event) -> impl Future<Output = Result<()>> + Send;
}

impl<T: Handler + ?Sized> Handler for Arc<T> {
    fn id(&self) -> &str {
        (**self).id()
    }
    fn handle(&self, event: Event) -> impl Future<Output = Result<()>> + Send {
        (**self).handle(event)
    }
}

impl<T: Handler + ?Sized> Handler for Box<T> {
    fn id(&self) -> &str {
        (**self).id()
    }
    fn handle(&self, event: Event) -> impl Future<Output = Result<()>> + Send {
        (**self).handle(event)
    }
}

pub trait DynHandler: Send + Sync {
    fn id_dyn(&self) -> &str;
    fn handle_dyn<'a>(
        &'a self,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

impl<T: Handler + ?Sized> DynHandler for T {
    fn id_dyn(&self) -> &str {
        <Self as Handler>::id(self)
    }
    fn handle_dyn<'a>(
        &'a self,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(<Self as Handler>::handle(self, event))
    }
}

pub type BoxHandler = Box<dyn DynHandler>;
pub type ArcHandler = Arc<dyn DynHandler>;

impl Handler for dyn DynHandler + '_ {
    fn id(&self) -> &str {
        DynHandler::id_dyn(self)
    }
    fn handle(&self, event: Event) -> impl Future<Output = Result<()>> + Send {
        DynHandler::handle_dyn(self, event)
    }
}

pub trait HandlerExt: Handler + Sized + 'static {
    fn into_boxed(self) -> BoxHandler {
        Box::new(self)
    }

    fn into_arced(self) -> ArcHandler {
        Arc::new(self)
    }
}

impl<T: Handler + 'static> HandlerExt for T {}

pub trait Filter: Send + Sync {
    fn matches(&self, event: &Event) -> bool;
}

impl<T: Filter + ?Sized> Filter for Arc<T> {
    fn matches(&self, event: &Event) -> bool {
        (**self).matches(event)
    }
}

impl<T: Filter + ?Sized> Filter for Box<T> {
    fn matches(&self, event: &Event) -> bool {
        (**self).matches(event)
    }
}

pub type BoxFilter = Box<dyn Filter>;
pub type ArcFilter = Arc<dyn Filter>;

pub trait FilterExt: Filter + Sized + 'static {
    fn into_boxed(self) -> BoxFilter {
        Box::new(self)
    }

    fn into_arced(self) -> ArcFilter {
        Arc::new(self)
    }
}

impl<T: Filter + 'static> FilterExt for T {}

pub struct FilteredHandler<H, F> {
    handler: H,
    filter: F,
}

impl<H: Handler, F: Filter> FilteredHandler<H, F> {
    pub fn new(handler: H, filter: F) -> Self {
        Self { handler, filter }
    }
}

impl<H: Handler, F: Filter> Handler for FilteredHandler<H, F> {
    fn id(&self) -> &str {
        self.handler.id()
    }

    async fn handle(&self, event: Event) -> Result<()> {
        if !self.filter.matches(&event) {
            return Ok(());
        }
        self.handler.handle(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::payload::Payload;

    struct CountingHandler {
        id: String,
        count: Arc<AtomicUsize>,
    }

    impl Handler for CountingHandler {
        fn id(&self) -> &str {
            &self.id
        }
        async fn handle(&self, _: Event) -> Result<()> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct AllowNothing;
    impl Filter for AllowNothing {
        fn matches(&self, _: &Event) -> bool {
            false
        }
    }

    struct AllowAll;
    impl Filter for AllowAll {
        fn matches(&self, _: &Event) -> bool {
            true
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
    async fn filter_pass_invokes_inner() {
        let count = Arc::new(AtomicUsize::new(0));
        let h = FilteredHandler::new(
            CountingHandler {
                id: "h".into(),
                count: Arc::clone(&count),
            },
            AllowAll,
        );
        h.handle(ev()).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn filter_fail_skips_inner() {
        let count = Arc::new(AtomicUsize::new(0));
        let h = FilteredHandler::new(
            CountingHandler {
                id: "h".into(),
                count: Arc::clone(&count),
            },
            AllowNothing,
        );
        h.handle(ev()).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn id_passthrough() {
        let h = FilteredHandler::new(
            CountingHandler {
                id: "inner-id".into(),
                count: Arc::new(AtomicUsize::new(0)),
            },
            AllowAll,
        );
        assert_eq!(h.id(), "inner-id");
    }

    #[tokio::test]
    async fn handler_into_boxed_yields_dyn_handler() {
        let count = Arc::new(AtomicUsize::new(0));
        let handler: BoxHandler = CountingHandler {
            id: "h".into(),
            count: Arc::clone(&count),
        }
        .into_boxed();
        assert_eq!(handler.id(), "h");
        handler.handle(ev()).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn handler_into_arced_yields_shared_handler() {
        let count = Arc::new(AtomicUsize::new(0));
        let handler: ArcHandler = CountingHandler {
            id: "h".into(),
            count: Arc::clone(&count),
        }
        .into_arced();
        let clone = Arc::clone(&handler);
        handler.handle(ev()).await.unwrap();
        clone.handle(ev()).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn handler_box_blanket_passes_as_generic_handler() {
        async fn take<H: Handler>(h: H, e: Event) {
            h.handle(e).await.unwrap();
        }
        let count = Arc::new(AtomicUsize::new(0));
        let boxed: BoxHandler = CountingHandler {
            id: "h".into(),
            count: Arc::clone(&count),
        }
        .into_boxed();
        take(boxed, ev()).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn filter_into_boxed_yields_dyn_filter() {
        let f: BoxFilter = AllowAll.into_boxed();
        assert!(f.matches(&ev()));
        let f: BoxFilter = AllowNothing.into_boxed();
        assert!(!f.matches(&ev()));
    }

    #[test]
    fn filter_into_arced_yields_shared_filter() {
        let f: ArcFilter = AllowAll.into_arced();
        let clone = Arc::clone(&f);
        assert!(f.matches(&ev()));
        assert!(clone.matches(&ev()));
    }

    #[test]
    fn filter_box_blanket_passes_as_generic_filter() {
        fn take<F: Filter>(f: F, e: &Event) -> bool {
            f.matches(e)
        }
        let boxed: BoxFilter = AllowAll.into_boxed();
        assert!(take(boxed, &ev()));
    }

    fn _assert_handler_dyn_safe() {
        fn _take(_: BoxHandler) {}
        fn _take_arc(_: ArcHandler) {}
    }

    fn _assert_filter_dyn_safe() {
        fn _take(_: BoxFilter) {}
        fn _take_arc(_: ArcFilter) {}
    }
}
