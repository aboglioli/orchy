pub mod announce_actor;
pub mod block_task;
pub mod cancel_task;
pub mod claim_task;
pub mod complete_task;
pub mod create_document;
pub mod create_task;
pub mod dto;
pub mod edit_document;
pub mod error;
pub mod fail_task;
pub mod find_documents;
pub mod get_task;
pub mod link_entities;
pub mod list_actors;
pub mod list_sent;
pub mod list_tasks;
pub mod manage_dependencies;
pub mod manage_lease;
pub mod next_task;
pub mod promote_document;
pub mod promote_message;
pub mod read_document;
pub mod read_events;
pub mod read_inbox;
pub mod read_message;
pub mod read_thread;
pub mod recall;
pub mod release_task;
pub mod replace_task;
pub mod resolve_thread;
pub mod rollup_ancestors;
pub mod send_message;
pub mod set_document_field;
pub mod split_task;
pub mod start_task;
pub mod supersede_document;
pub mod traverse_graph;
pub mod unblock_task;
pub mod update_document;
pub mod update_task;

use std::sync::Arc;

use orchy_core::{
    ActorStore, Clock, DocumentStore, EdgeStore, EventLog, IdGenerator, LeaseStore, MessageStore,
    ReadWatermarks, Search, TaskStore,
};

pub use error::{ApplicationError, ApplicationResult};

use announce_actor::AnnounceActor;
use block_task::BlockTask;
use cancel_task::CancelTask;
use claim_task::ClaimTask;
use complete_task::CompleteTask;
use create_document::CreateDocument;
use create_task::CreateTask;
use edit_document::EditDocument;
use fail_task::FailTask;
use find_documents::FindDocuments;
use get_task::GetTask;
use link_entities::LinkEntities;
use list_actors::ListActors;
use list_sent::ListSent;
use list_tasks::ListTasks;
use manage_dependencies::ManageDependencies;
use manage_lease::ManageLease;
use next_task::NextTask;
use promote_document::PromoteDocument;
use promote_message::PromoteMessage;
use read_document::ReadDocument;
use read_events::ReadEvents;
use read_inbox::ReadInbox;
use read_message::ReadMessage;
use read_thread::ReadThread;
use recall::Recall;
use release_task::ReleaseTask;
use replace_task::ReplaceTask;
use resolve_thread::ResolveThread;
use rollup_ancestors::RollupAncestors;
use send_message::SendMessage;
use set_document_field::SetDocumentField;
use split_task::SplitTask;
use start_task::StartTask;
use supersede_document::SupersedeDocument;
use traverse_graph::TraverseGraph;
use unblock_task::UnblockTask;
use update_document::UpdateDocument;
use update_task::UpdateTask;

pub struct ApplicationDeps {
    pub documents: Arc<dyn DocumentStore>,
    pub tasks: Arc<dyn TaskStore>,
    pub messages: Arc<dyn MessageStore>,
    pub edges: Arc<dyn EdgeStore>,
    pub actors: Arc<dyn ActorStore>,
    pub leases: Arc<dyn LeaseStore>,
    pub watermarks: Arc<dyn ReadWatermarks>,
    pub search: Arc<dyn Search>,
    pub log: Arc<dyn EventLog>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
}

pub struct Application {
    pub announce_actor: AnnounceActor,
    pub list_actors: ListActors,
    pub manage_lease: ManageLease,

    pub create_document: CreateDocument,
    pub read_document: ReadDocument,
    pub edit_document: EditDocument,
    pub set_document_field: SetDocumentField,
    pub update_document: UpdateDocument,
    pub find_documents: FindDocuments,
    pub promote_document: PromoteDocument,
    pub supersede_document: SupersedeDocument,

    pub create_task: CreateTask,
    pub get_task: GetTask,
    pub list_tasks: ListTasks,
    pub next_task: NextTask,
    pub update_task: UpdateTask,
    pub claim_task: Arc<ClaimTask>,
    pub release_task: ReleaseTask,
    pub start_task: StartTask,
    pub complete_task: CompleteTask,
    pub fail_task: FailTask,
    pub cancel_task: CancelTask,
    pub block_task: BlockTask,
    pub unblock_task: UnblockTask,
    pub split_task: SplitTask,
    pub replace_task: ReplaceTask,
    pub manage_dependencies: ManageDependencies,
    pub rollup_ancestors: Arc<RollupAncestors>,

    pub send_message: SendMessage,
    pub read_inbox: ReadInbox,
    pub read_message: ReadMessage,
    pub read_thread: ReadThread,
    pub list_sent: ListSent,
    pub resolve_thread: ResolveThread,
    pub promote_message: PromoteMessage,

    pub link_entities: LinkEntities,
    pub traverse_graph: TraverseGraph,
    pub recall: Recall,
    pub read_events: ReadEvents,
}

impl Application {
    pub fn new(deps: ApplicationDeps) -> Self {
        let ApplicationDeps {
            documents,
            tasks,
            messages,
            edges,
            actors,
            leases,
            watermarks,
            search,
            log,
            clock,
            ids,
        } = deps;

        let rollup = Arc::new(RollupAncestors::new(
            Arc::clone(&tasks),
            Arc::clone(&leases),
            Arc::clone(&clock),
        ));
        let claim = Arc::new(ClaimTask::new(
            Arc::clone(&tasks),
            Arc::clone(&leases),
            Arc::clone(&clock),
        ));

        Self {
            announce_actor: AnnounceActor::new(Arc::clone(&actors), Arc::clone(&clock)),
            list_actors: ListActors::new(Arc::clone(&actors), Arc::clone(&clock)),
            manage_lease: ManageLease::new(Arc::clone(&leases)),

            create_document: CreateDocument::new(
                Arc::clone(&documents),
                Arc::clone(&ids),
                Arc::clone(&clock),
            ),
            read_document: ReadDocument::new(Arc::clone(&documents), Arc::clone(&edges)),
            edit_document: EditDocument::new(Arc::clone(&documents), Arc::clone(&clock)),
            set_document_field: SetDocumentField::new(Arc::clone(&documents), Arc::clone(&clock)),
            update_document: UpdateDocument::new(Arc::clone(&documents), Arc::clone(&clock)),
            find_documents: FindDocuments::new(Arc::clone(&documents)),
            promote_document: PromoteDocument::new(Arc::clone(&documents), Arc::clone(&clock)),
            supersede_document: SupersedeDocument::new(
                Arc::clone(&documents),
                Arc::clone(&edges),
                Arc::clone(&clock),
            ),

            create_task: CreateTask::new(Arc::clone(&tasks), Arc::clone(&ids), Arc::clone(&clock)),
            get_task: GetTask::new(Arc::clone(&tasks), Arc::clone(&edges)),
            list_tasks: ListTasks::new(Arc::clone(&tasks)),
            next_task: NextTask::new(Arc::clone(&tasks), Arc::clone(&claim)),
            update_task: UpdateTask::new(Arc::clone(&tasks), Arc::clone(&clock)),
            claim_task: Arc::clone(&claim),
            release_task: ReleaseTask::new(
                Arc::clone(&tasks),
                Arc::clone(&leases),
                Arc::clone(&clock),
            ),
            start_task: StartTask::new(Arc::clone(&tasks), Arc::clone(&clock)),
            complete_task: CompleteTask::new(
                Arc::clone(&tasks),
                Arc::clone(&leases),
                Arc::clone(&rollup),
                Arc::clone(&clock),
            ),
            fail_task: FailTask::new(
                Arc::clone(&tasks),
                Arc::clone(&leases),
                Arc::clone(&rollup),
                Arc::clone(&clock),
            ),
            cancel_task: CancelTask::new(
                Arc::clone(&tasks),
                Arc::clone(&leases),
                Arc::clone(&rollup),
                Arc::clone(&clock),
            ),
            block_task: BlockTask::new(Arc::clone(&tasks), Arc::clone(&clock)),
            unblock_task: UnblockTask::new(Arc::clone(&tasks), Arc::clone(&clock)),
            split_task: SplitTask::new(Arc::clone(&tasks), Arc::clone(&ids), Arc::clone(&clock)),
            replace_task: ReplaceTask::new(
                Arc::clone(&tasks),
                Arc::clone(&edges),
                Arc::clone(&rollup),
                Arc::clone(&ids),
                Arc::clone(&clock),
            ),
            manage_dependencies: ManageDependencies::new(Arc::clone(&tasks), Arc::clone(&clock)),
            rollup_ancestors: Arc::clone(&rollup),

            send_message: SendMessage::new(
                Arc::clone(&messages),
                Arc::clone(&ids),
                Arc::clone(&clock),
            ),
            read_inbox: ReadInbox::new(Arc::clone(&messages), Arc::clone(&watermarks)),
            read_message: ReadMessage::new(Arc::clone(&messages), Arc::clone(&watermarks)),
            read_thread: ReadThread::new(Arc::clone(&messages)),
            list_sent: ListSent::new(Arc::clone(&messages)),
            resolve_thread: ResolveThread::new(Arc::clone(&messages), Arc::clone(&clock)),
            promote_message: PromoteMessage::new(
                Arc::clone(&messages),
                Arc::clone(&tasks),
                Arc::clone(&edges),
                Arc::clone(&ids),
                Arc::clone(&clock),
            ),

            link_entities: LinkEntities::new(Arc::clone(&edges)),
            traverse_graph: TraverseGraph::new(Arc::clone(&edges)),
            recall: Recall::new(Arc::clone(&search), Arc::clone(&clock)),
            read_events: ReadEvents::new(Arc::clone(&log)),
        }
    }
}
