use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, NamespaceStore};
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::{TaskStore, WatcherStore};

mod dto;

pub(crate) fn parse_namespace(ns: Option<&str>) -> Result<Namespace> {
    match ns {
        Some(s) if !s.is_empty() => {
            let normalized = if s.starts_with('/') {
                s.to_string()
            } else {
                format!("/{s}")
            };
            Namespace::try_from(normalized).map_err(|e| Error::InvalidInput(e.to_string()))
        }
        _ => Ok(Namespace::root()),
    }
}

// Agent
mod change_roles;
mod disconnect_agent;
mod get_agent;
mod get_agent_summary;
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

pub use change_roles::{ChangeRoles, ChangeRolesCommand};
pub use disconnect_agent::{DisconnectAgent, DisconnectAgentCommand};
pub use get_agent::GetAgent;
pub use get_agent_summary::{AgentSummary, GetAgentSummary, GetAgentSummaryCommand};
pub use heartbeat::{Heartbeat, HeartbeatCommand};
pub use list_agents::{ListAgents, ListAgentsCommand};
pub use register_agent::{RegisterAgent, RegisterAgentCommand};
pub use switch_context::{SwitchContext, SwitchContextCommand};

pub use assign_task::{AssignTask, AssignTaskCommand};
pub use cancel_task::{CancelTask, CancelTaskCommand};
pub use claim_task::{ClaimTask, ClaimTaskCommand};
pub use complete_task::{CompleteTask, CompleteTaskCommand};
pub use fail_task::{FailTask, FailTaskCommand};
pub use get_next_task::{GetNextTask, GetNextTaskCommand};
pub use get_task::GetTask;
pub use list_tasks::{ListTasks, ListTasksCommand};
pub use post_task::{PostTask, PostTaskCommand, ResourceRefInput};
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

pub struct Application {
    pub register_agent: RegisterAgent,
    pub switch_context: SwitchContext,
    pub disconnect_agent: DisconnectAgent,
    pub heartbeat: Heartbeat,
    pub change_roles: ChangeRoles,
    pub get_agent: GetAgent,
    pub get_agent_summary: GetAgentSummary,
    pub list_agents: ListAgents,

    pub post_task: PostTask,
    pub get_task: GetTask,
    pub list_tasks: ListTasks,
    pub get_next_task: GetNextTask,
    pub claim_task: ClaimTask,
    pub start_task: StartTask,
    pub complete_task: CompleteTask,
    pub fail_task: FailTask,
    pub cancel_task: CancelTask,
    pub release_task: ReleaseTask,
    pub update_task: UpdateTask,
    pub assign_task: AssignTask,
    pub unblock_task: UnblockTask,

    pub split_task: SplitTask,
    pub replace_task: ReplaceTask,
    pub merge_tasks: MergeTasks,
    pub delegate_task: DelegateTask,
    pub add_dependency: AddDependency,
    pub remove_dependency: RemoveDependency,

    pub add_task_note: AddTaskNote,
    pub tag_task: TagTask,
    pub untag_task: UntagTask,
    pub move_task: MoveTask,
    pub list_tags: ListTags,

    pub watch_task: WatchTask,
    pub unwatch_task: UnwatchTask,

    pub send_message: SendMessage,
    pub check_mailbox: CheckMailbox,
    pub check_sent_messages: CheckSentMessages,
    pub mark_read: MarkRead,
    pub list_conversation: ListConversation,

    pub write_knowledge: WriteKnowledge,
    pub read_knowledge: ReadKnowledge,
    pub list_knowledge: ListKnowledge,
    pub search_knowledge: SearchKnowledge,
    pub delete_knowledge: DeleteKnowledge,
    pub append_knowledge: AppendKnowledge,
    pub rename_knowledge: RenameKnowledge,
    pub move_knowledge: MoveKnowledge,
    pub change_knowledge_kind: ChangeKnowledgeKind,
    pub tag_knowledge: TagKnowledge,
    pub untag_knowledge: UntagKnowledge,
    pub patch_knowledge_metadata: PatchKnowledgeMetadata,
    pub import_knowledge: ImportKnowledge,

    pub get_project: GetProject,
    pub update_project: UpdateProject,
    pub set_project_metadata: SetProjectMetadata,
    pub list_namespaces: ListNamespaces,

    pub lock_resource: LockResource,
    pub unlock_resource: UnlockResource,
    pub check_lock: CheckLock,

    pub poll_updates: PollUpdates,
    pub get_project_overview: GetProjectOverview,
}

impl Application {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agents: Arc<dyn AgentStore>,
        tasks: Arc<dyn TaskStore>,
        projects: Arc<dyn ProjectStore>,
        knowledge: Arc<dyn KnowledgeStore>,
        messages: Arc<dyn MessageStore>,
        locks: Arc<dyn LockStore>,
        watchers: Arc<dyn WatcherStore>,
        namespaces: Arc<dyn NamespaceStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
        event_query: Arc<dyn EventQuery>,
    ) -> Self {
        Self {
            register_agent: RegisterAgent::new(agents.clone()),
            switch_context: SwitchContext::new(
                agents.clone(),
                projects.clone(),
                tasks.clone(),
                locks.clone(),
                watchers.clone(),
            ),
            disconnect_agent: DisconnectAgent::new(agents.clone()),
            heartbeat: Heartbeat::new(agents.clone()),
            change_roles: ChangeRoles::new(agents.clone()),
            get_agent: GetAgent::new(agents.clone()),
            get_agent_summary: GetAgentSummary::new(
                agents.clone(),
                projects.clone(),
                messages.clone(),
                tasks.clone(),
                knowledge.clone(),
            ),
            list_agents: ListAgents::new(agents.clone()),

            post_task: PostTask::new(tasks.clone()),
            get_task: GetTask::new(tasks.clone()),
            list_tasks: ListTasks::new(tasks.clone()),
            get_next_task: GetNextTask::new(tasks.clone()),
            claim_task: ClaimTask::new(tasks.clone()),
            start_task: StartTask::new(tasks.clone()),
            complete_task: CompleteTask::new(tasks.clone()),
            fail_task: FailTask::new(tasks.clone()),
            cancel_task: CancelTask::new(tasks.clone()),
            release_task: ReleaseTask::new(tasks.clone()),
            update_task: UpdateTask::new(tasks.clone()),
            assign_task: AssignTask::new(tasks.clone()),
            unblock_task: UnblockTask::new(tasks.clone()),

            split_task: SplitTask::new(tasks.clone()),
            replace_task: ReplaceTask::new(tasks.clone()),
            merge_tasks: MergeTasks::new(tasks.clone()),
            delegate_task: DelegateTask::new(tasks.clone()),
            add_dependency: AddDependency::new(tasks.clone()),
            remove_dependency: RemoveDependency::new(tasks.clone()),

            add_task_note: AddTaskNote::new(tasks.clone(), knowledge.clone()),
            tag_task: TagTask::new(tasks.clone()),
            untag_task: UntagTask::new(tasks.clone()),
            move_task: MoveTask::new(tasks.clone()),
            list_tags: ListTags::new(tasks.clone()),

            watch_task: WatchTask::new(tasks.clone(), watchers.clone()),
            unwatch_task: UnwatchTask::new(watchers.clone()),

            send_message: SendMessage::new(messages.clone()),
            check_mailbox: CheckMailbox::new(messages.clone(), agents.clone()),
            check_sent_messages: CheckSentMessages::new(messages.clone()),
            mark_read: MarkRead::new(messages.clone()),
            list_conversation: ListConversation::new(messages.clone()),

            write_knowledge: WriteKnowledge::new(knowledge.clone(), embeddings.clone()),
            read_knowledge: ReadKnowledge::new(knowledge.clone()),
            list_knowledge: ListKnowledge::new(knowledge.clone()),
            search_knowledge: SearchKnowledge::new(knowledge.clone(), embeddings.clone()),
            delete_knowledge: DeleteKnowledge::new(knowledge.clone()),
            append_knowledge: AppendKnowledge::new(knowledge.clone(), embeddings.clone()),
            rename_knowledge: RenameKnowledge::new(knowledge.clone()),
            move_knowledge: MoveKnowledge::new(knowledge.clone()),
            change_knowledge_kind: ChangeKnowledgeKind::new(knowledge.clone(), embeddings.clone()),
            tag_knowledge: TagKnowledge::new(knowledge.clone()),
            untag_knowledge: UntagKnowledge::new(knowledge.clone()),
            patch_knowledge_metadata: PatchKnowledgeMetadata::new(knowledge.clone()),
            import_knowledge: ImportKnowledge::new(knowledge.clone(), embeddings),

            get_project: GetProject::new(projects.clone()),
            update_project: UpdateProject::new(projects.clone()),
            set_project_metadata: SetProjectMetadata::new(projects.clone()),
            list_namespaces: ListNamespaces::new(namespaces),

            lock_resource: LockResource::new(locks.clone()),
            unlock_resource: UnlockResource::new(locks.clone()),
            check_lock: CheckLock::new(locks),

            poll_updates: PollUpdates::new(event_query),
            get_project_overview: GetProjectOverview::new(projects, agents, tasks, knowledge),
        }
    }
}
