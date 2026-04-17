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

pub use assign_task::{AssignTask, AssignTaskCommand};
pub use cancel_task::{CancelTask, CancelTaskCommand};
pub use claim_task::{ClaimTask, ClaimTaskCommand};
pub use complete_task::{CompleteTask, CompleteTaskCommand};
pub use fail_task::{FailTask, FailTaskCommand};
pub use get_next_task::{GetNextTask, GetNextTaskCommand};
pub use get_task::GetTask;
pub use list_tasks::{ListTasks, ListTasksCommand};
pub use post_task::{PostTask, PostTaskCommand};
pub use release_task::{ReleaseTask, ReleaseTaskCommand};
pub use start_task::{StartTask, StartTaskCommand};
pub use unblock_task::{UnblockTask, UnblockTaskCommand};
pub use update_task::{UpdateTask, UpdateTaskCommand};

pub use add_dependency::{AddDependency, AddDependencyCommand};
pub use delegate_task::{DelegateTask, DelegateTaskCommand};
pub use merge_tasks::{MergeTasks, MergeTasksCommand};
pub use remove_dependency::{RemoveDependency, RemoveDependencyCommand};
pub use replace_task::{ReplaceTask, ReplaceTaskCommand};
pub use split_task::{SplitTask, SplitTaskCommand, SubtaskInput};

pub use add_task_note::{AddTaskNote, AddTaskNoteCommand};
pub use list_tags::{ListTags, ListTagsCommand};
pub use move_task::{MoveTask, MoveTaskCommand};
pub use tag_task::{TagTask, TagTaskCommand};
pub use untag_task::{UntagTask, UntagTaskCommand};

pub use unwatch_task::{UnwatchTask, UnwatchTaskCommand};
pub use watch_task::{WatchTask, WatchTaskCommand};

pub use get_review::GetReview;
pub use list_reviews::{ListReviews, ListReviewsCommand};
pub use request_review::{RequestReview, RequestReviewCommand};
pub use resolve_review::{ResolveReview, ResolveReviewCommand};

pub use check_mailbox::{CheckMailbox, CheckMailboxCommand};
pub use check_sent_messages::{CheckSentMessages, CheckSentMessagesCommand};
pub use list_conversation::{ListConversation, ListConversationCommand};
pub use mark_read::{MarkRead, MarkReadCommand};
pub use send_message::{SendMessage, SendMessageCommand};

pub use append_knowledge::{AppendKnowledge, AppendKnowledgeCommand};
pub use change_knowledge_kind::{ChangeKnowledgeKind, ChangeKnowledgeKindCommand};
pub use delete_knowledge::{DeleteKnowledge, DeleteKnowledgeCommand};
pub use import_knowledge::{ImportKnowledge, ImportKnowledgeCommand};
pub use list_knowledge::{ListKnowledge, ListKnowledgeCommand};
pub use move_knowledge::{MoveKnowledge, MoveKnowledgeCommand};
pub use patch_knowledge_metadata::{PatchKnowledgeMetadata, PatchKnowledgeMetadataCommand};
pub use read_knowledge::{ReadKnowledge, ReadKnowledgeCommand};
pub use rename_knowledge::{RenameKnowledge, RenameKnowledgeCommand};
pub use search_knowledge::{SearchKnowledge, SearchKnowledgeCommand};
pub use tag_knowledge::{TagKnowledge, TagKnowledgeCommand};
pub use untag_knowledge::{UntagKnowledge, UntagKnowledgeCommand};
pub use write_knowledge::{WriteKnowledge, WriteKnowledgeCommand};

pub use get_project::{GetProject, GetProjectCommand};
pub use list_namespaces::{ListNamespaces, ListNamespacesCommand};
pub use set_project_metadata::{SetProjectMetadata, SetProjectMetadataCommand};
pub use update_project::{UpdateProject, UpdateProjectCommand};

pub use check_lock::{CheckLock, CheckLockCommand};
pub use lock_resource::{LockResource, LockResourceCommand};
pub use unlock_resource::{UnlockResource, UnlockResourceCommand};

pub use dto::ProjectOverview;
pub use get_project_overview::{GetProjectOverview, GetProjectOverviewCommand};
pub use poll_updates::{EventQuery, PollUpdates, PollUpdatesCommand};
