use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::api_key::ApiKeyStore;
use orchy_core::graph::EdgeStore;
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;

pub struct Bundle {
    pub agents: Arc<dyn AgentStore>,
    pub tasks: Arc<dyn TaskStore>,
    pub messages: Arc<dyn MessageStore>,
    pub knowledge: Arc<dyn KnowledgeStore>,
    pub edges: Arc<dyn EdgeStore>,
    pub locks: Arc<dyn LockStore>,
    pub api_keys: Arc<dyn ApiKeyStore>,
}

pub trait Backend: Send + Sync + 'static {
    fn build() -> Pin<Box<dyn Future<Output = Bundle> + Send>>;
}

#[macro_export]
macro_rules! conformance_suite {
    ($backend:ty) => {
        $crate::declare_test! { $backend, task_save_then_find_returns_same, $crate::scenarios::task_save_then_find_returns_same }
        $crate::declare_test! { $backend, task_filter_tag_does_not_match_substring, $crate::scenarios::task_filter_tag_does_not_match_substring }
        $crate::declare_test! { $backend, knowledge_optimistic_concurrency, $crate::scenarios::knowledge_optimistic_concurrency }
        $crate::declare_test! { $backend, message_claim_visibility, $crate::scenarios::message_claim_visibility }
        $crate::declare_test! { $backend, edge_alias_blocks_normalizes_to_depends_on, $crate::scenarios::edge_alias_blocks_normalizes_to_depends_on }
        $crate::declare_test! { $backend, agent_save_then_find_returns_same, $crate::scenarios::agent_save_then_find_returns_same }
        $crate::declare_test! { $backend, pagination_respects_limit, $crate::scenarios::pagination_respects_limit }
        $crate::declare_test! { $backend, lock_acquire_and_release, $crate::scenarios::lock_acquire_and_release }
    };
}

#[macro_export]
macro_rules! declare_test {
    ($backend:ty, $name:ident, $scenario:path) => {
        #[tokio::test]
        async fn $name() {
            let bundle = <$backend as $crate::Backend>::build().await;
            $scenario(&bundle).await;
        }
    };
}

pub mod events;
pub mod scenarios;
