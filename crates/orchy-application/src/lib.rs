use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::edge::EdgeStore;
use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, NamespaceStore};
use orchy_core::organization::OrganizationStore;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::{OrgMembershipStore, UserStore};

pub mod dto;

// User/Auth
mod bootstrap_admin;
mod change_password;
mod get_current_user;
mod invite_user;
mod list_org_users;
mod login_user;
mod register_user;

// Edges
mod add_edge;
mod assemble_context;
pub mod materialize_neighborhood;
mod remove_edge;

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
mod check_timed_out_agents;
mod get_agent;
mod get_agent_summary;
mod heartbeat;
mod list_agents;
mod register_agent;
mod rename_alias;
mod resolve_agent;
mod suggest_roles;
mod switch_context;

// Task lifecycle
mod assign_task;
mod cancel_task;
mod claim_task;
mod complete_task;
mod fail_task;
mod get_next_task;
mod get_task;
mod get_task_with_context;
mod list_tasks;
mod post_task;
mod release_task;
mod start_task;
mod touch_task;
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
mod list_tags;
mod move_task;
mod tag_task;
mod untag_task;

// Messages
mod check_mailbox;
mod check_sent_messages;
mod list_conversation;
mod mark_read;
mod send_message;

// Knowledge
mod append_knowledge;
mod change_knowledge_kind;
mod consolidate_knowledge;
mod delete_knowledge;
mod import_knowledge;
mod list_knowledge;
mod move_knowledge;
mod patch_knowledge_metadata;
mod promote_knowledge;
mod read_knowledge;
mod rename_knowledge;
mod search_knowledge;
mod tag_knowledge;
mod untag_knowledge;
mod write_knowledge;

// Knowledge (inheritance)
mod list_overviews;
pub(crate) mod list_skills;

// Project
mod get_project;
mod list_namespaces;
mod set_project_metadata;
mod update_project;

// Locks
mod check_lock;
mod lock_resource;
mod unlock_resource;

// Organization
mod add_api_key;
mod create_organization;
mod get_organization;
mod list_organizations;
mod resolve_api_key;
mod revoke_api_key;

// Events/overview
mod get_project_overview;
mod poll_updates;

pub use change_roles::{ChangeRoles, ChangeRolesCommand};
pub use check_timed_out_agents::CheckTimedOutAgents;
pub use dto::RegisterAgentResponse;
pub use get_agent::{GetAgent, GetAgentCommand, GetAgentResponse};
pub use get_agent_summary::{GetAgentSummary, GetAgentSummaryCommand};
pub use heartbeat::{Heartbeat, HeartbeatCommand};
pub use list_agents::{ListAgents, ListAgentsCommand};
pub use register_agent::{RegisterAgent, RegisterAgentCommand};
pub use rename_alias::{RenameAlias, RenameAliasCommand};
pub use resolve_agent::resolve_agent;
pub use suggest_roles::{SuggestRoles, SuggestRolesCommand};
pub use switch_context::{SwitchContext, SwitchContextCommand};

pub use assign_task::{AssignTask, AssignTaskCommand};
pub use cancel_task::{CancelTask, CancelTaskCommand};
pub use claim_task::{ClaimTask, ClaimTaskCommand};
pub use complete_task::{CompleteTask, CompleteTaskCommand};
pub use fail_task::{FailTask, FailTaskCommand};
pub use get_next_task::{GetNextTask, GetNextTaskCommand};
pub use get_task::{GetTask, GetTaskCommand, GetTaskResponse};
pub use get_task_with_context::{GetTaskWithContext, GetTaskWithContextCommand};
pub use list_tasks::{ListTasks, ListTasksCommand};
pub use post_task::{PostTask, PostTaskCommand};
pub use release_task::{ReleaseTask, ReleaseTaskCommand};
pub use start_task::{StartTask, StartTaskCommand};
pub use touch_task::{TouchTask, TouchTaskCommand};
pub use unblock_task::{UnblockTask, UnblockTaskCommand};
pub use update_task::{UpdateTask, UpdateTaskCommand};

pub use add_dependency::{AddDependency, AddDependencyCommand};
pub use delegate_task::{DelegateTask, DelegateTaskCommand};
pub use merge_tasks::{MergeTasks, MergeTasksCommand};
pub use remove_dependency::{RemoveDependency, RemoveDependencyCommand};
pub use replace_task::{ReplaceTask, ReplaceTaskCommand};
pub use split_task::{SplitTask, SplitTaskCommand, SubtaskInput};

pub use list_tags::{ListTags, ListTagsCommand};
pub use move_task::{MoveTask, MoveTaskCommand};
pub use tag_task::{TagTask, TagTaskCommand};
pub use untag_task::{UntagTask, UntagTaskCommand};

pub use check_mailbox::{CheckMailbox, CheckMailboxCommand};
pub use check_sent_messages::{CheckSentMessages, CheckSentMessagesCommand};
pub use list_conversation::{ListConversation, ListConversationCommand};
pub use mark_read::{MarkRead, MarkReadCommand};
pub use send_message::{SendMessage, SendMessageCommand};

pub use append_knowledge::{AppendKnowledge, AppendKnowledgeCommand};
pub use change_knowledge_kind::{ChangeKnowledgeKind, ChangeKnowledgeKindCommand};
pub use consolidate_knowledge::{ConsolidateKnowledge, ConsolidateKnowledgeCommand};
pub use delete_knowledge::{DeleteKnowledge, DeleteKnowledgeCommand};
pub use import_knowledge::{ImportKnowledge, ImportKnowledgeCommand};
pub use list_knowledge::{ListKnowledge, ListKnowledgeCommand};
pub use materialize_neighborhood::{MaterializeNeighborhood, MaterializeNeighborhoodCommand};
pub use move_knowledge::{MoveKnowledge, MoveKnowledgeCommand};
pub use patch_knowledge_metadata::{PatchKnowledgeMetadata, PatchKnowledgeMetadataCommand};
pub use promote_knowledge::{PromoteKnowledge, PromoteKnowledgeCommand};
pub use read_knowledge::{ReadKnowledge, ReadKnowledgeCommand, ReadKnowledgeResponse};
pub use rename_knowledge::{RenameKnowledge, RenameKnowledgeCommand};
pub use search_knowledge::{SearchKnowledge, SearchKnowledgeCommand};
pub use tag_knowledge::{TagKnowledge, TagKnowledgeCommand};
pub use untag_knowledge::{UntagKnowledge, UntagKnowledgeCommand};
pub use write_knowledge::{WriteKnowledge, WriteKnowledgeCommand};

pub use list_overviews::{ListOverviews, ListOverviewsCommand};
pub use list_skills::{ListSkills, ListSkillsCommand};

pub use get_project::{GetProject, GetProjectCommand};
pub use list_namespaces::{ListNamespaces, ListNamespacesCommand};
pub use set_project_metadata::{SetProjectMetadata, SetProjectMetadataCommand};
pub use update_project::{UpdateProject, UpdateProjectCommand};

pub use check_lock::{CheckLock, CheckLockCommand};
pub use lock_resource::{LockResource, LockResourceCommand};
pub use unlock_resource::{UnlockResource, UnlockResourceCommand};

pub use add_api_key::{AddApiKey, AddApiKeyCommand};
pub use create_organization::{CreateOrganization, CreateOrganizationCommand};
pub use get_organization::{GetOrganization, GetOrganizationCommand};
pub use list_organizations::{ListOrganizations, ListOrganizationsCommand};
pub use resolve_api_key::{ResolveApiKey, ResolveApiKeyCommand};
pub use revoke_api_key::{RevokeApiKey, RevokeApiKeyCommand};

pub use add_edge::{AddEdge, AddEdgeCommand};
pub use assemble_context::{AssembleContext, AssembleContextCommand};
pub use remove_edge::{RemoveEdge, RemoveEdgeCommand};

pub use bootstrap_admin::BootstrapAdmin;
pub use change_password::{ChangePassword, ChangePasswordCommand};
pub use get_current_user::{GetCurrentUser, GetCurrentUserCommand};
pub use invite_user::{InviteUser, InviteUserCommand, InviteUserResponse};
pub use list_org_users::{ListOrgUsers, ListOrgUsersCommand};
pub use login_user::{LoginUser, LoginUserCommand};
pub use register_user::{RegisterUser, RegisterUserCommand, RegisterUserResponse};

pub use dto::{
    AgentResponse, AgentSummaryResponse, ApiKeyResponse, AssembleContextResponse, AuthResponse,
    EdgeResponse, KnowledgeResponse, MessageResponse, OrgMembershipResponse, OrganizationResponse,
    PageResponse, ProjectOverviewResponse, ProjectResponse, ResourceLockResponse, TaskResponse,
    TaskWithContextResponse, UserResponse,
};
pub use get_project_overview::{GetProjectOverview, GetProjectOverviewCommand};
pub use poll_updates::{EventQuery, PollUpdates, PollUpdatesCommand};

pub struct Application {
    pub register_agent: RegisterAgent,
    pub switch_context: SwitchContext,
    pub heartbeat: Heartbeat,
    pub change_roles: ChangeRoles,
    pub get_agent: GetAgent,
    pub get_agent_summary: GetAgentSummary,
    pub list_agents: ListAgents,
    pub suggest_roles: SuggestRoles,
    pub check_timed_out_agents: CheckTimedOutAgents,
    pub rename_alias: RenameAlias,

    pub post_task: PostTask,
    pub get_task: GetTask,
    pub get_task_with_context: GetTaskWithContext,
    pub list_tasks: ListTasks,
    pub get_next_task: GetNextTask,
    pub claim_task: ClaimTask,
    pub start_task: StartTask,
    pub touch_task: TouchTask,
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

    pub add_edge: AddEdge,
    pub assemble_context: AssembleContext,
    pub remove_edge: RemoveEdge,
    pub materialize_neighborhood: Arc<MaterializeNeighborhood>,
    pub tag_task: TagTask,
    pub untag_task: UntagTask,
    pub move_task: MoveTask,
    pub list_tags: ListTags,

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
    pub promote_knowledge: PromoteKnowledge,
    pub consolidate_knowledge: ConsolidateKnowledge,
    pub import_knowledge: ImportKnowledge,
    pub list_skills: ListSkills,
    pub list_overviews: ListOverviews,

    pub get_project: GetProject,
    pub update_project: UpdateProject,
    pub set_project_metadata: SetProjectMetadata,
    pub list_namespaces: ListNamespaces,

    pub lock_resource: LockResource,
    pub unlock_resource: UnlockResource,
    pub check_lock: CheckLock,

    pub poll_updates: PollUpdates,
    pub get_project_overview: GetProjectOverview,

    pub create_organization: CreateOrganization,
    pub get_organization: GetOrganization,
    pub list_organizations: ListOrganizations,
    pub add_api_key: AddApiKey,
    pub revoke_api_key: RevokeApiKey,
    pub resolve_api_key: ResolveApiKey,

    pub register_user: RegisterUser,
    pub login_user: LoginUser,
    pub get_current_user: GetCurrentUser,
    pub change_password: ChangePassword,
    pub invite_user: InviteUser,
    pub list_org_users: ListOrgUsers,
    pub bootstrap_admin: BootstrapAdmin,
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
        namespaces: Arc<dyn NamespaceStore>,
        orgs: Arc<dyn OrganizationStore>,
        edges: Arc<dyn EdgeStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
        event_query: Arc<dyn EventQuery>,
        users: Arc<dyn UserStore>,
        memberships: Arc<dyn OrgMembershipStore>,
    ) -> Self {
        let materializer = Arc::new(MaterializeNeighborhood::new(
            edges.clone(),
            tasks.clone(),
            knowledge.clone(),
            agents.clone(),
            messages.clone(),
        ));

        Self {
            register_agent: RegisterAgent::new(agents.clone(), messages.clone(), tasks.clone()),
            switch_context: SwitchContext::new(
                agents.clone(),
                projects.clone(),
                tasks.clone(),
                locks.clone(),
            ),
            heartbeat: Heartbeat::new(agents.clone()),
            rename_alias: RenameAlias::new(agents.clone()),
            change_roles: ChangeRoles::new(agents.clone()),
            get_agent: GetAgent::new(agents.clone(), Some(Arc::clone(&materializer))),
            get_agent_summary: GetAgentSummary::new(
                agents.clone(),
                projects.clone(),
                messages.clone(),
                tasks.clone(),
                knowledge.clone(),
            ),
            list_agents: ListAgents::new(agents.clone()),
            suggest_roles: SuggestRoles::new(tasks.clone()),
            check_timed_out_agents: CheckTimedOutAgents::new(agents.clone()),

            post_task: PostTask::new(tasks.clone()),
            get_task: GetTask::new(tasks.clone(), Some(Arc::clone(&materializer))),
            get_task_with_context: GetTaskWithContext::new(
                tasks.clone(),
                edges.clone(),
                knowledge.clone(),
            ),
            list_tasks: ListTasks::new(tasks.clone()),
            get_next_task: GetNextTask::new(tasks.clone(), edges.clone()),
            claim_task: ClaimTask::new(agents.clone(), tasks.clone(), edges.clone()),
            start_task: StartTask::new(agents.clone(), tasks.clone()),
            touch_task: TouchTask::new(tasks.clone()),
            complete_task: CompleteTask::new(tasks.clone(), edges.clone()),
            fail_task: FailTask::new(tasks.clone(), edges.clone()),
            cancel_task: CancelTask::new(tasks.clone(), edges.clone()),
            release_task: ReleaseTask::new(tasks.clone()),
            update_task: UpdateTask::new(tasks.clone()),
            assign_task: AssignTask::new(agents.clone(), tasks.clone()),
            unblock_task: UnblockTask::new(tasks.clone()),

            split_task: SplitTask::new(tasks.clone(), edges.clone()),
            replace_task: ReplaceTask::new(tasks.clone(), edges.clone()),
            merge_tasks: MergeTasks::new(tasks.clone(), edges.clone()),
            delegate_task: DelegateTask::new(tasks.clone(), edges.clone()),
            add_dependency: AddDependency::new(tasks.clone(), edges.clone()),
            remove_dependency: RemoveDependency::new(tasks.clone(), edges.clone()),

            add_edge: AddEdge::new(edges.clone()),
            assemble_context: AssembleContext::new(edges.clone(), tasks.clone(), knowledge.clone()),
            remove_edge: RemoveEdge::new(edges.clone()),
            materialize_neighborhood: Arc::clone(&materializer),
            tag_task: TagTask::new(tasks.clone()),
            untag_task: UntagTask::new(tasks.clone()),
            move_task: MoveTask::new(tasks.clone()),
            list_tags: ListTags::new(tasks.clone()),

            send_message: SendMessage::new(agents.clone(), messages.clone()),
            check_mailbox: CheckMailbox::new(messages.clone(), agents.clone()),
            check_sent_messages: CheckSentMessages::new(messages.clone(), agents.clone()),
            mark_read: MarkRead::new(messages.clone(), agents.clone()),
            list_conversation: ListConversation::new(messages.clone()),

            write_knowledge: WriteKnowledge::new(
                knowledge.clone(),
                edges.clone(),
                embeddings.clone(),
            ),
            read_knowledge: ReadKnowledge::new(knowledge.clone(), Some(Arc::clone(&materializer))),
            list_knowledge: ListKnowledge::new(knowledge.clone()),
            search_knowledge: SearchKnowledge::new(
                knowledge.clone(),
                embeddings.clone(),
                edges.clone(),
            ),
            delete_knowledge: DeleteKnowledge::new(knowledge.clone(), edges.clone()),
            append_knowledge: AppendKnowledge::new(knowledge.clone(), embeddings.clone()),
            rename_knowledge: RenameKnowledge::new(knowledge.clone()),
            move_knowledge: MoveKnowledge::new(knowledge.clone()),
            change_knowledge_kind: ChangeKnowledgeKind::new(knowledge.clone(), embeddings.clone()),
            tag_knowledge: TagKnowledge::new(knowledge.clone()),
            untag_knowledge: UntagKnowledge::new(knowledge.clone()),
            patch_knowledge_metadata: PatchKnowledgeMetadata::new(knowledge.clone()),
            promote_knowledge: PromoteKnowledge::new(knowledge.clone(), edges.clone()),
            consolidate_knowledge: ConsolidateKnowledge::new(knowledge.clone(), edges.clone()),
            import_knowledge: ImportKnowledge::new(knowledge.clone(), embeddings),
            list_skills: ListSkills::new(knowledge.clone()),
            list_overviews: ListOverviews::new(knowledge.clone()),

            get_project: GetProject::new(projects.clone()),
            update_project: UpdateProject::new(projects.clone()),
            set_project_metadata: SetProjectMetadata::new(projects.clone()),
            list_namespaces: ListNamespaces::new(namespaces),

            lock_resource: LockResource::new(agents.clone(), locks.clone()),
            unlock_resource: UnlockResource::new(agents.clone(), locks.clone()),
            check_lock: CheckLock::new(locks),

            poll_updates: PollUpdates::new(event_query),
            get_project_overview: GetProjectOverview::new(projects, agents, tasks, knowledge),

            create_organization: CreateOrganization::new(orgs.clone()),
            get_organization: GetOrganization::new(orgs.clone()),
            list_organizations: ListOrganizations::new(orgs.clone()),
            add_api_key: AddApiKey::new(orgs.clone()),
            revoke_api_key: RevokeApiKey::new(orgs.clone()),
            resolve_api_key: ResolveApiKey::new(orgs),

            register_user: RegisterUser::new(users.clone()),
            login_user: LoginUser::new(users.clone(), memberships.clone()),
            get_current_user: GetCurrentUser::new(users.clone(), memberships.clone()),
            change_password: ChangePassword::new(users.clone()),
            invite_user: InviteUser::new(users.clone(), memberships.clone()),
            list_org_users: ListOrgUsers::new(users.clone(), memberships.clone()),
            bootstrap_admin: BootstrapAdmin::new(users),
        }
    }
}
