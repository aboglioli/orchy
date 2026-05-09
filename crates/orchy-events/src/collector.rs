use crate::event::Event;
use std::mem;

#[derive(Debug, Clone)]
pub struct EventCollector {
    events: Vec<Event>,
}

impl EventCollector {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn with(event: Event) -> Self {
        Self {
            events: vec![event],
        }
    }

    pub fn collect(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn drain(&mut self) -> Vec<Event> {
        mem::take(&mut self.events)
    }

    pub fn list(&self) -> &[Event] {
        &self.events
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Event> {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for EventCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Payload;

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

    #[test]
    fn new_is_empty() {
        let c = EventCollector::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.list().is_empty());
    }

    #[test]
    fn with_single_event() {
        let c = EventCollector::with(ev());
        assert_eq!(c.len(), 1);
        assert!(!c.is_empty());
    }

    #[test]
    fn collect_adds_events() {
        let mut c = EventCollector::new();
        c.collect(ev());
        c.collect(ev());
        assert_eq!(c.len(), 2);
        assert!(!c.is_empty());
    }

    #[test]
    fn drain_removes_and_returns() {
        let mut c = EventCollector::new();
        c.collect(ev());

        let drained = c.drain();
        assert_eq!(drained.len(), 1);
        assert!(c.is_empty());
    }

    #[test]
    fn drain_twice_returns_empty_second() {
        let mut c = EventCollector::new();
        c.collect(ev());

        let first = c.drain();
        assert_eq!(first.len(), 1);

        let second = c.drain();
        assert!(second.is_empty());
    }

    #[test]
    fn iter_yields_collected() {
        let mut c = EventCollector::new();
        c.collect(ev());
        c.collect(ev());

        let items: Vec<_> = c.iter().collect();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn default_is_empty() {
        let c = EventCollector::default();
        assert!(c.is_empty());
    }
}
