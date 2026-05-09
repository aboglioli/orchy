use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::api_key::{ApiKeyGenerator, ApiKeyStore};
use orchy_core::error::Result;
use orchy_core::graph::EdgeStore;
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, NamespaceStore};
use orchy_core::organization::OrganizationStore;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::{OrgMembershipStore, PasswordHasher, TokenEncoder, UserStore};

pub mod dto;
pub mod embeddings;
pub mod error;

pub use embeddings::EmbeddingsProvider;
pub use error::{ApplicationError, ApplicationResult};

mod bootstrap_admin;
mod change_password;
mod decode_token;
mod get_current_user;
mod invite_user;
mod login_user;
mod register_user;

// Edges
mod add_edge;
mod assemble_context;
mod list_edges;
pub mod materialize_neighborhood;
mod remove_edge;

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
mod archive_task;
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
mod unarchive_task;
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
pub mod claim_message;
mod list_conversation;
mod mark_read;
mod send_message;
pub mod unclaim_message;

// Knowledge
mod append_knowledge;
mod archive_knowledge;
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
mod unarchive_knowledge;
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

// Namespace
mod register_namespace;

// Organization
mod create_organization;
mod generate_api_key;
mod get_organization;
mod list_api_keys;
mod list_organizations;
mod resolve_api_key;
mod resolve_token;
mod revoke_api_key;

// Events/overview
mod get_project_overview;
mod poll_updates;

pub use change_roles::{ChangeRoles, ChangeRolesCommand};
pub use check_timed_out_agents::{CheckTimedOutAgents, CheckTimedOutAgentsResult};
pub use dto::RegisterAgentDto;
pub use get_agent::{GetAgent, GetAgentCommand, GetAgentDto};
pub use get_agent_summary::{GetAgentSummary, GetAgentSummaryCommand};
pub use heartbeat::{Heartbeat, HeartbeatCommand};
pub use list_agents::{ListAgents, ListAgentsCommand};
pub use register_agent::{RegisterAgent, RegisterAgentCommand};
pub use rename_alias::{RenameAlias, RenameAliasCommand};
pub use resolve_agent::{ResolveAgent, ResolveAgentCommand};
pub use suggest_roles::{SuggestRoles, SuggestRolesCommand};
pub use switch_context::{SwitchContext, SwitchContextCommand};

pub use archive_task::{ArchiveTask, ArchiveTaskCommand};
pub use assign_task::{AssignTask, AssignTaskCommand};
pub use cancel_task::{CancelTask, CancelTaskCommand};
pub use claim_task::{ClaimTask, ClaimTaskCommand};
pub use complete_task::{CompleteTask, CompleteTaskCommand};
pub use fail_task::{FailTask, FailTaskCommand};
pub use get_next_task::{GetNextTask, GetNextTaskCommand};
pub use get_task::{GetTask, GetTaskCommand, GetTaskDto};
pub use get_task_with_context::{GetTaskWithContext, GetTaskWithContextCommand};
pub use list_tasks::{ListTasks, ListTasksCommand};
pub use post_task::{PostTask, PostTaskCommand};
pub use release_task::{ReleaseTask, ReleaseTaskCommand};
pub use start_task::{StartTask, StartTaskCommand};
pub use touch_task::{TouchTask, TouchTaskCommand};
pub use unarchive_task::{UnarchiveTask, UnarchiveTaskCommand};
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
pub use claim_message::ClaimMessage;
pub use list_conversation::{ListConversation, ListConversationCommand};
pub use mark_read::{MarkRead, MarkReadCommand};
pub use send_message::{SendMessage, SendMessageCommand};
pub use unclaim_message::UnclaimMessage;

pub use append_knowledge::{AppendKnowledge, AppendKnowledgeCommand};
pub use archive_knowledge::{ArchiveKnowledge, ArchiveKnowledgeCommand};
pub use change_knowledge_kind::{ChangeKnowledgeKind, ChangeKnowledgeKindCommand};
pub use consolidate_knowledge::{ConsolidateKnowledge, ConsolidateKnowledgeCommand};
pub use delete_knowledge::{DeleteKnowledge, DeleteKnowledgeCommand};
pub use import_knowledge::{ImportKnowledge, ImportKnowledgeCommand};
pub use list_knowledge::{ListKnowledge, ListKnowledgeCommand};
pub use materialize_neighborhood::{MaterializeNeighborhood, MaterializeNeighborhoodCommand};
pub use move_knowledge::{MoveKnowledge, MoveKnowledgeCommand};
pub use patch_knowledge_metadata::{PatchKnowledgeMetadata, PatchKnowledgeMetadataCommand};
pub use promote_knowledge::{PromoteKnowledge, PromoteKnowledgeCommand};
pub use read_knowledge::{ReadKnowledge, ReadKnowledgeCommand, ReadKnowledgeDto};
pub use rename_knowledge::{RenameKnowledge, RenameKnowledgeCommand};
pub use search_knowledge::{SearchKnowledge, SearchKnowledgeCommand};
pub use tag_knowledge::{TagKnowledge, TagKnowledgeCommand};
pub use unarchive_knowledge::{UnarchiveKnowledge, UnarchiveKnowledgeCommand};
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

pub use create_organization::{CreateOrganization, CreateOrganizationCommand};
pub use generate_api_key::{GenerateApiKey, GenerateApiKeyCommand, GenerateApiKeyResponse};
pub use get_organization::{GetOrganization, GetOrganizationCommand};
pub use list_api_keys::{ListApiKeys, ListApiKeysCommand};
pub use list_organizations::{ListOrganizations, ListOrganizationsCommand};
pub use register_namespace::{RegisterNamespace, RegisterNamespaceCommand};
pub use resolve_api_key::{ApiKeyPrincipal, ResolveApiKey, ResolveApiKeyCommand};
pub use resolve_token::{ResolveToken, ResolveTokenCommand, TokenPrincipal};
pub use revoke_api_key::{RevokeApiKey, RevokeApiKeyCommand};

pub use add_edge::{AddEdge, AddEdgeCommand};
pub use assemble_context::{AssembleContext, AssembleContextCommand};
pub use list_edges::{ListEdges, ListEdgesCommand};
pub use remove_edge::{RemoveEdge, RemoveEdgeCommand};

pub use bootstrap_admin::BootstrapAdmin;
pub use change_password::{ChangePassword, ChangePasswordCommand};
pub use decode_token::{DecodeToken, DecodeTokenCommand, DecodeTokenResponse};
pub use get_current_user::{GetCurrentUser, GetCurrentUserCommand};
pub use invite_user::{InviteUser, InviteUserCommand, InviteUserDto};
pub use login_user::{LoginUser, LoginUserCommand, LoginUserResponse};
pub use register_user::{RegisterUser, RegisterUserCommand, RegisterUserDto};

pub use dto::{
    AgentDto, AgentSummaryResponse, ApiKeyDto, AssembleContextResponse, AuthResponse, EdgeDto,
    KnowledgeDto, MessageDto, OrgMembershipDto, OrganizationDto, PageResponse, ProjectDto,
    ProjectOverviewResponse, ResourceLockDto, TaskDto, TaskWithContextResponse, UserDto,
};
pub use get_project_overview::{GetProjectOverview, GetProjectOverviewCommand};
pub use poll_updates::{PollUpdates, PollUpdatesCommand, ReaderFactory};

pub(crate) fn parse_namespace(ns: Option<&str>) -> Result<Namespace> {
    Ok(Namespace::new(ns.unwrap_or(""))?)
}

pub struct ApplicationDeps {
    pub agents: Arc<dyn AgentStore>,
    pub tasks: Arc<dyn TaskStore>,
    pub projects: Arc<dyn ProjectStore>,
    pub knowledge: Arc<dyn KnowledgeStore>,
    pub messages: Arc<dyn MessageStore>,
    pub locks: Arc<dyn LockStore>,
    pub namespaces: Arc<dyn NamespaceStore>,
    pub orgs: Arc<dyn OrganizationStore>,
    pub edges: Arc<dyn EdgeStore>,
    pub embeddings: Option<Arc<dyn EmbeddingsProvider>>,
    pub reader_factory: Arc<dyn ReaderFactory>,
    pub users: Arc<dyn UserStore>,
    pub memberships: Arc<dyn OrgMembershipStore>,
    pub token_encoder: Option<Arc<dyn TokenEncoder>>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub api_keys: Arc<dyn ApiKeyStore>,
    pub api_key_generator: Arc<dyn ApiKeyGenerator>,
}

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
    pub resolve_agent: ResolveAgent,

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
    pub archive_task: ArchiveTask,
    pub unarchive_task: UnarchiveTask,
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
    pub list_edges: ListEdges,
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
    pub claim_message: ClaimMessage,
    pub unclaim_message: UnclaimMessage,
    pub list_conversation: ListConversation,

    pub write_knowledge: WriteKnowledge,
    pub read_knowledge: ReadKnowledge,
    pub list_knowledge: ListKnowledge,
    pub search_knowledge: SearchKnowledge,
    pub delete_knowledge: DeleteKnowledge,
    pub archive_knowledge: ArchiveKnowledge,
    pub unarchive_knowledge: UnarchiveKnowledge,
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
    pub generate_api_key: GenerateApiKey,
    pub list_api_keys: ListApiKeys,
    pub revoke_api_key: RevokeApiKey,
    pub resolve_api_key: ResolveApiKey,
    pub resolve_token: Option<ResolveToken>,
    pub decode_token: Option<DecodeToken>,
    pub register_namespace: RegisterNamespace,

    pub register_user: RegisterUser,
    pub login_user: Option<LoginUser>,
    pub get_current_user: GetCurrentUser,
    pub change_password: ChangePassword,
    pub invite_user: InviteUser,
    pub bootstrap_admin: BootstrapAdmin,
}

impl Application {
    pub fn new(deps: ApplicationDeps) -> Self {
        let agents = deps.agents;
        let tasks = deps.tasks;
        let projects = deps.projects;
        let knowledge = deps.knowledge;
        let messages = deps.messages;
        let locks = deps.locks;
        let namespaces = deps.namespaces;
        let orgs = deps.orgs;
        let edges = deps.edges;
        let embeddings = deps.embeddings;
        let reader_factory = deps.reader_factory;
        let users = deps.users;
        let memberships = deps.memberships;
        let token_encoder = deps.token_encoder;
        let hasher = deps.hasher;
        let api_keys = deps.api_keys;
        let generator = deps.api_key_generator;
        let materializer = Arc::new(MaterializeNeighborhood::new(
            Arc::clone(&edges),
            Arc::clone(&tasks),
            Arc::clone(&knowledge),
            Arc::clone(&agents),
            Arc::clone(&messages),
        ));

        Self {
            register_agent: RegisterAgent::new(
                Arc::clone(&agents),
                Arc::clone(&messages),
                Arc::clone(&tasks),
            ),
            switch_context: SwitchContext::new(
                Arc::clone(&agents),
                Arc::clone(&projects),
                Arc::clone(&tasks),
                Arc::clone(&locks),
            ),
            heartbeat: Heartbeat::new(Arc::clone(&agents)),
            rename_alias: RenameAlias::new(Arc::clone(&agents)),
            resolve_agent: ResolveAgent::new(Arc::clone(&agents)),
            change_roles: ChangeRoles::new(Arc::clone(&agents)),
            get_agent: GetAgent::new(Arc::clone(&agents), Some(Arc::clone(&materializer))),
            get_agent_summary: GetAgentSummary::new(
                Arc::clone(&agents),
                Arc::clone(&projects),
                Arc::clone(&messages),
                Arc::clone(&tasks),
                Arc::clone(&knowledge),
            ),
            list_agents: ListAgents::new(Arc::clone(&agents)),
            suggest_roles: SuggestRoles::new(Arc::clone(&tasks)),
            check_timed_out_agents: CheckTimedOutAgents::new(
                Arc::clone(&agents),
                Arc::clone(&tasks),
                Arc::clone(&locks),
            ),

            post_task: PostTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            get_task: GetTask::new(Arc::clone(&tasks), Some(Arc::clone(&materializer))),
            get_task_with_context: GetTaskWithContext::new(
                Arc::clone(&tasks),
                Arc::clone(&edges),
                Arc::clone(&knowledge),
            ),
            list_tasks: ListTasks::new(Arc::clone(&tasks)),
            get_next_task: GetNextTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            claim_task: ClaimTask::new(Arc::clone(&agents), Arc::clone(&tasks), Arc::clone(&edges)),
            start_task: StartTask::new(Arc::clone(&agents), Arc::clone(&tasks)),
            touch_task: TouchTask::new(Arc::clone(&tasks)),
            complete_task: CompleteTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            fail_task: FailTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            cancel_task: CancelTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            release_task: ReleaseTask::new(Arc::clone(&tasks)),
            archive_task: ArchiveTask::new(Arc::clone(&tasks)),
            unarchive_task: UnarchiveTask::new(Arc::clone(&tasks)),
            update_task: UpdateTask::new(Arc::clone(&tasks)),
            assign_task: AssignTask::new(Arc::clone(&agents), Arc::clone(&tasks)),
            unblock_task: UnblockTask::new(Arc::clone(&tasks)),

            split_task: SplitTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            replace_task: ReplaceTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            merge_tasks: MergeTasks::new(Arc::clone(&tasks), Arc::clone(&edges)),
            delegate_task: DelegateTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            add_dependency: AddDependency::new(Arc::clone(&tasks), Arc::clone(&edges)),
            remove_dependency: RemoveDependency::new(Arc::clone(&tasks), Arc::clone(&edges)),

            add_edge: AddEdge::new(Arc::clone(&edges), Arc::clone(&knowledge)),
            assemble_context: AssembleContext::new(
                Arc::clone(&edges),
                Arc::clone(&tasks),
                Arc::clone(&knowledge),
            ),
            list_edges: ListEdges::new(Arc::clone(&edges)),
            remove_edge: RemoveEdge::new(Arc::clone(&edges)),
            materialize_neighborhood: Arc::clone(&materializer),
            tag_task: TagTask::new(Arc::clone(&tasks)),
            untag_task: UntagTask::new(Arc::clone(&tasks)),
            move_task: MoveTask::new(Arc::clone(&tasks)),
            list_tags: ListTags::new(Arc::clone(&tasks)),

            send_message: SendMessage::new(
                Arc::clone(&agents),
                Arc::clone(&messages),
                Arc::clone(&users),
                Arc::clone(&memberships),
            ),
            check_mailbox: CheckMailbox::new(Arc::clone(&messages), Arc::clone(&agents)),
            check_sent_messages: CheckSentMessages::new(Arc::clone(&messages), Arc::clone(&agents)),
            mark_read: MarkRead::new(Arc::clone(&messages), Arc::clone(&agents)),
            claim_message: ClaimMessage::new(Arc::clone(&messages)),
            unclaim_message: UnclaimMessage::new(Arc::clone(&messages)),
            list_conversation: ListConversation::new(Arc::clone(&messages)),

            write_knowledge: WriteKnowledge::new(
                Arc::clone(&knowledge),
                Arc::clone(&edges),
                embeddings.clone(),
            ),
            read_knowledge: ReadKnowledge::new(
                Arc::clone(&knowledge),
                Some(Arc::clone(&materializer)),
            ),
            list_knowledge: ListKnowledge::new(Arc::clone(&knowledge)),
            search_knowledge: SearchKnowledge::new(
                Arc::clone(&knowledge),
                embeddings.clone(),
                Arc::clone(&edges),
            ),
            delete_knowledge: DeleteKnowledge::new(Arc::clone(&knowledge), Arc::clone(&edges)),
            archive_knowledge: ArchiveKnowledge::new(Arc::clone(&knowledge)),
            unarchive_knowledge: UnarchiveKnowledge::new(Arc::clone(&knowledge)),
            append_knowledge: AppendKnowledge::new(Arc::clone(&knowledge), embeddings.clone()),
            rename_knowledge: RenameKnowledge::new(Arc::clone(&knowledge)),
            move_knowledge: MoveKnowledge::new(Arc::clone(&knowledge)),
            change_knowledge_kind: ChangeKnowledgeKind::new(
                Arc::clone(&knowledge),
                embeddings.clone(),
            ),
            tag_knowledge: TagKnowledge::new(Arc::clone(&knowledge)),
            untag_knowledge: UntagKnowledge::new(Arc::clone(&knowledge)),
            patch_knowledge_metadata: PatchKnowledgeMetadata::new(Arc::clone(&knowledge)),
            promote_knowledge: PromoteKnowledge::new(Arc::clone(&knowledge), Arc::clone(&edges)),
            consolidate_knowledge: ConsolidateKnowledge::new(
                Arc::clone(&knowledge),
                Arc::clone(&edges),
            ),
            import_knowledge: ImportKnowledge::new(Arc::clone(&knowledge), embeddings),
            list_skills: ListSkills::new(Arc::clone(&knowledge)),
            list_overviews: ListOverviews::new(Arc::clone(&knowledge)),

            get_project: GetProject::new(Arc::clone(&projects)),
            update_project: UpdateProject::new(Arc::clone(&projects)),
            set_project_metadata: SetProjectMetadata::new(Arc::clone(&projects)),
            list_namespaces: ListNamespaces::new(Arc::clone(&namespaces)),
            register_namespace: RegisterNamespace::new(namespaces),

            lock_resource: LockResource::new(Arc::clone(&agents), Arc::clone(&locks)),
            unlock_resource: UnlockResource::new(Arc::clone(&agents), Arc::clone(&locks)),
            check_lock: CheckLock::new(locks),

            poll_updates: PollUpdates::new(reader_factory),
            get_project_overview: GetProjectOverview::new(projects, agents, tasks, knowledge),

            create_organization: CreateOrganization::new(Arc::clone(&orgs)),
            get_organization: GetOrganization::new(Arc::clone(&orgs)),
            list_organizations: ListOrganizations::new(Arc::clone(&orgs)),
            generate_api_key: GenerateApiKey::new(Arc::clone(&api_keys), Arc::clone(&generator)),
            list_api_keys: ListApiKeys::new(Arc::clone(&api_keys)),
            revoke_api_key: RevokeApiKey::new(Arc::clone(&api_keys)),
            resolve_api_key: ResolveApiKey::new(api_keys, Arc::clone(&orgs), generator),
            resolve_token: token_encoder.as_ref().map(|te| {
                ResolveToken::new(Arc::clone(te), Arc::clone(&memberships), Arc::clone(&orgs))
            }),
            decode_token: token_encoder
                .as_ref()
                .map(|te| DecodeToken::new(Arc::clone(te))),

            register_user: RegisterUser::new(Arc::clone(&users), Arc::clone(&hasher)),
            login_user: token_encoder.map(|te| {
                LoginUser::new(
                    Arc::clone(&users),
                    Arc::clone(&memberships),
                    te,
                    Arc::clone(&hasher),
                )
            }),
            get_current_user: GetCurrentUser::new(Arc::clone(&users), Arc::clone(&memberships)),
            change_password: ChangePassword::new(Arc::clone(&users), Arc::clone(&hasher)),
            invite_user: InviteUser::new(
                Arc::clone(&users),
                Arc::clone(&memberships),
                Arc::clone(&hasher),
            ),
            bootstrap_admin: BootstrapAdmin::new(users, orgs, memberships, hasher),
        }
    }
}
