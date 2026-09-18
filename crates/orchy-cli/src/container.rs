use std::sync::Arc;

use orchy_application::{Application, ApplicationDeps};
use orchy_core::{
    ActorStore, Clock, EventLog, IdGenerator, ReadWatermarks, Search, StaticRelationRegistry,
    StaticTypeRegistry,
};
use orchy_store_vault::blob::{BlobStore, FsBlobStore};
use orchy_store_vault::documents::VaultDocumentStore;
use orchy_store_vault::edges::VaultEdgeStore;
use orchy_store_vault::eventlog::EventuaryLog;
use orchy_store_vault::messages::VaultMessageStore;
use orchy_store_vault::roster::{FileLeaseStore, VaultActorStore};
use orchy_store_vault::search::VaultSearch;
use orchy_store_vault::tasks::VaultTaskStore;
use orchy_store_vault::time::{SystemClock, UlidGenerator};
use orchy_store_vault::vault::Vault;
use orchy_store_vault::watermarks::FileWatermarks;

use crate::config::Config;
use crate::error::CliResult;

/// The only place in the workspace that names a concrete store.
pub(crate) async fn build(config: &Config) -> CliResult<Application> {
    let blobs: Arc<dyn BlobStore> = Arc::new(FsBlobStore::new(&config.vault));
    let vault = Arc::new(Vault::open(Arc::clone(&blobs)).await?);

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Arc<dyn IdGenerator> = Arc::new(UlidGenerator::new());
    let log: Arc<dyn EventLog> = Arc::new(EventuaryLog::open(
        config.events_root(),
        &config.organization,
        config.actor.clone(),
        config.machine.clone(),
    )?);

    let actors: Arc<dyn ActorStore> = Arc::new(VaultActorStore::new(Arc::clone(&vault)));
    let documents = Arc::new(VaultDocumentStore::new(
        Arc::clone(&vault),
        Arc::clone(&log),
    ));
    let relations = Arc::new(StaticRelationRegistry::builtin());

    Ok(Application::new(ApplicationDeps {
        search: Arc::new(VaultSearch::new(Arc::clone(&documents))) as Arc<dyn Search>,
        documents: Arc::clone(&documents) as _,
        tasks: Arc::new(VaultTaskStore::new(Arc::clone(&vault), Arc::clone(&log))),
        messages: Arc::new(VaultMessageStore::new(
            Arc::clone(&vault),
            Arc::clone(&actors),
            Arc::clone(&log),
        )),
        edges: Arc::new(VaultEdgeStore::new(
            Arc::clone(&vault),
            Arc::clone(&relations) as _,
        )),
        actors,
        leases: Arc::new(FileLeaseStore::new(
            config.runtime_root().join("locks"),
            Arc::clone(&clock),
        )),
        watermarks: Arc::new(FileWatermarks::new(config.runtime_root().join("read")))
            as Arc<dyn ReadWatermarks>,
        log,
        types: Arc::new(StaticTypeRegistry::builtin()),
        relations,
        clock,
        ids,
    }))
}
