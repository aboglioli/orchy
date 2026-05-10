use chrono::{DateTime, Utc};

use crate::{Event, Namespace, OrganizationId, StartFrom, Topic};

#[derive(Debug, Clone)]
pub struct EventSelector {
    pub organization: OrganizationId,
    pub topics: Option<Vec<Topic>>,
    pub namespace_prefix: Option<Namespace>,
    pub start_from: StartFrom,
    pub end_at: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

impl EventSelector {
    pub fn new(organization: OrganizationId) -> Self {
        Self {
            organization,
            topics: None,
            namespace_prefix: None,
            start_from: StartFrom::Latest,
            end_at: None,
            limit: None,
        }
    }

    pub fn matches(&self, event: &Event) -> bool {
        if event.organization() != &self.organization {
            return false;
        }
        if let Some(topics) = self.topics.as_ref()
            && !topics.iter().any(|t| t == event.topic())
        {
            return false;
        }
        if let Some(prefix) = self.namespace_prefix.as_ref()
            && !event.namespace().starts_with(prefix)
        {
            return false;
        }
        if let Some(end_at) = self.end_at
            && event.timestamp() > end_at
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Payload;

    fn org() -> OrganizationId {
        OrganizationId::new("test-org").unwrap()
    }

    fn event_in(org: &str, namespace: &str, topic: &str) -> Event {
        let payload = Payload::from_string("test");
        Event::create(org, namespace, topic, "key-1", payload).unwrap()
    }

    #[test]
    fn new_selector_matches_org() {
        let selector = EventSelector::new(org());
        let event = event_in("test-org", "/backend", "task.created");
        assert!(selector.matches(&event));
    }

    #[test]
    fn topic_filter_excludes_non_matching() {
        let mut selector = EventSelector::new(org());
        selector.topics = Some(vec![Topic::new("task.created").unwrap()]);
        let matching = event_in("test-org", "/backend", "task.created");
        let other = event_in("test-org", "/backend", "task.completed");
        assert!(selector.matches(&matching));
        assert!(!selector.matches(&other));
    }

    #[test]
    fn namespace_prefix_excludes_non_matching() {
        let mut selector = EventSelector::new(org());
        selector.namespace_prefix = Some(Namespace::new("/backend").unwrap());
        let inside = event_in("test-org", "/backend/auth", "task.created");
        let outside = event_in("test-org", "/frontend", "task.created");
        assert!(selector.matches(&inside));
        assert!(!selector.matches(&outside));
    }

    #[test]
    fn end_at_excludes_future_events() {
        let mut selector = EventSelector::new(org());
        selector.end_at = Some(Utc::now() - chrono::Duration::seconds(60));
        let event = event_in("test-org", "/backend", "task.created");
        assert!(!selector.matches(&event));
    }

    #[test]
    fn different_organization_does_not_match() {
        let selector = EventSelector::new(org());
        let event = event_in("other-org", "/backend", "task.created");
        assert!(!selector.matches(&event));
    }
}
