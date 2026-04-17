use orchy_core::error::{Error, Result};
use orchy_core::namespace::Namespace;

mod dto;

pub(crate) fn parse_namespace(ns: Option<&str>) -> Result<Namespace> {
    match ns {
        Some(s) if !s.is_empty() => {
            let normalized = if s.starts_with('/') {
                s.to_string()
            } else {
                format!("/{s}")
            };
            Namespace::try_from(normalized).map_err(Error::InvalidInput)
        }
        _ => Ok(Namespace::root()),
    }
}

// Agent
mod change_roles;
mod disconnect_agent;
mod get_agent;
mod heartbeat;
mod list_agents;
mod register_agent;
mod switch_context;

// Task lifecycle
mod assign_task;
mod cancel_task;
mod claim_task;
mod complete_task;
mod fail_task;
mod get_next_task;
mod get_task;
mod list_tasks;
mod post_task;
mod release_task;
mod start_task;
mod unblock_task;
mod update_task;

// Task structure
mod add_dependency;
mod delegate_task;
mod merge_tasks;
mod remove_dependency;
mod replace_task;
mod split_task;

// Task metadata
mod add_task_note;
mod list_tags;
mod move_task;
mod tag_task;
mod untag_task;

// Task watchers
mod unwatch_task;
mod watch_task;

// Reviews
mod get_review;
mod list_reviews;
mod request_review;
mod resolve_review;

// Messages
mod check_mailbox;
mod check_sent_messages;
mod list_conversation;
mod mark_read;
mod send_message;

// Knowledge
mod append_knowledge;
mod change_knowledge_kind;
mod delete_knowledge;
mod import_knowledge;
mod list_knowledge;
mod move_knowledge;
mod patch_knowledge_metadata;
mod read_knowledge;
mod rename_knowledge;
mod search_knowledge;
mod tag_knowledge;
mod untag_knowledge;
mod write_knowledge;

// Project
mod get_project;
mod list_namespaces;
mod set_project_metadata;
mod update_project;

// Locks
mod check_lock;
mod lock_resource;
mod unlock_resource;

// Events/overview
mod get_project_overview;
mod poll_updates;

pub use change_roles::ChangeRoles;
pub use disconnect_agent::DisconnectAgent;
pub use get_agent::GetAgent;
pub use heartbeat::Heartbeat;
pub use list_agents::ListAgents;
pub use register_agent::RegisterAgent;
pub use switch_context::SwitchContext;

pub use assign_task::AssignTask;
pub use cancel_task::CancelTask;
pub use claim_task::ClaimTask;
pub use complete_task::CompleteTask;
pub use fail_task::FailTask;
pub use get_next_task::GetNextTask;
pub use get_task::GetTask;
pub use list_tasks::ListTasks;
pub use post_task::PostTask;
pub use release_task::ReleaseTask;
pub use start_task::StartTask;
pub use unblock_task::UnblockTask;
pub use update_task::UpdateTask;

pub use add_dependency::AddDependency;
pub use delegate_task::DelegateTask;
pub use merge_tasks::MergeTasks;
pub use remove_dependency::RemoveDependency;
pub use replace_task::ReplaceTask;
pub use split_task::SplitTask;

pub use add_task_note::AddTaskNote;
pub use list_tags::ListTags;
pub use move_task::MoveTask;
pub use tag_task::TagTask;
pub use untag_task::UntagTask;

pub use unwatch_task::UnwatchTask;
pub use watch_task::WatchTask;

pub use get_review::GetReview;
pub use list_reviews::ListReviews;
pub use request_review::RequestReview;
pub use resolve_review::ResolveReview;

pub use check_mailbox::CheckMailbox;
pub use check_sent_messages::CheckSentMessages;
pub use list_conversation::ListConversation;
pub use mark_read::MarkRead;
pub use send_message::SendMessage;

pub use append_knowledge::AppendKnowledge;
pub use change_knowledge_kind::ChangeKnowledgeKind;
pub use delete_knowledge::DeleteKnowledge;
pub use import_knowledge::ImportKnowledge;
pub use list_knowledge::ListKnowledge;
pub use move_knowledge::MoveKnowledge;
pub use patch_knowledge_metadata::PatchKnowledgeMetadata;
pub use read_knowledge::ReadKnowledge;
pub use rename_knowledge::RenameKnowledge;
pub use search_knowledge::SearchKnowledge;
pub use tag_knowledge::TagKnowledge;
pub use untag_knowledge::UntagKnowledge;
pub use write_knowledge::WriteKnowledge;

pub use get_project::GetProject;
pub use list_namespaces::ListNamespaces;
pub use set_project_metadata::SetProjectMetadata;
pub use update_project::UpdateProject;

pub use check_lock::CheckLock;
pub use lock_resource::LockResource;
pub use unlock_resource::UnlockResource;

pub use get_project_overview::GetProjectOverview;
pub use poll_updates::PollUpdates;
