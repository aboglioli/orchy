use crate::error::Result;
use crate::event::Event;
use crate::io::Acker;

pub struct Message<A: Acker> {
    event: Event,
    acker: A,
}

impl<A: Acker> Message<A> {
    pub fn new(event: Event, acker: A) -> Self {
        Self { event, acker }
    }

    pub fn event(&self) -> &Event {
        &self.event
    }

    pub fn into_event(self) -> Event {
        self.event
    }

    pub async fn ack(&self) -> Result<()> {
        self.acker.ack().await
    }

    pub async fn nack(&self) -> Result<()> {
        self.acker.nack().await
    }

    pub fn acker(&self) -> &A {
        &self.acker
    }

    pub fn into_parts(self) -> (Event, A) {
        (self.event, self.acker)
    }

    pub fn map_acker<B, F>(self, f: F) -> Message<B>
    where
        B: Acker,
        F: FnOnce(A) -> B,
    {
        Message {
            event: self.event,
            acker: f(self.acker),
        }
    }

    pub fn map_event<F>(self, f: F) -> Self
    where
        F: FnOnce(Event) -> Event,
    {
        Message {
            event: f(self.event),
            acker: self.acker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::io::acker::NoopAcker;
    use crate::payload::Payload;

    struct OtherAcker;

    impl Acker for OtherAcker {
        async fn ack(&self) -> Result<()> {
            Ok(())
        }
        async fn nack(&self) -> Result<()> {
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

    #[test]
    fn map_acker_swaps_acker_keeps_event() {
        let msg = Message::new(ev(), NoopAcker);
        let topic_before = msg.event().topic().as_str().to_owned();
        let mapped: Message<OtherAcker> = msg.map_acker(|_| OtherAcker);
        assert_eq!(mapped.event().topic().as_str(), topic_before);
    }

    #[test]
    fn map_event_swaps_event_keeps_acker() {
        let msg = Message::new(ev(), NoopAcker);
        let mapped = msg.map_event(|e| {
            Event::create(
                "org",
                "/y",
                e.topic().as_str(),
                "k2",
                Payload::from_string("p2"),
            )
            .unwrap()
        });
        assert_eq!(mapped.event().namespace().as_str(), "/y");
    }
}
