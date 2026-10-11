use std::sync::Arc;

use chrono::Utc;

use orchy_application::{Application, ApplicationDeps};
use orchy_core::{ActorStore, Clock, EventLog, IdGenerator, ReadWatermarks, Search, UnitOfWork};
use orchy_store_vault::blob::{BlobStore, FsBlobStore};
use orchy_store_vault::documents::VaultDocumentStore;
use orchy_store_vault::edges::VaultEdgeStore;
use orchy_store_vault::eventlog::EventuaryLog;
use orchy_store_vault::integrity::VaultIntegrity;
use orchy_store_vault::messages::VaultMessageStore;
use orchy_store_vault::roster::{FileLeaseStore, VaultActorStore};
use orchy_store_vault::search::VaultSearch;
use orchy_store_vault::sessions::{FileSessionStore, read_session};
use orchy_store_vault::skills::VaultSkillStore;
use orchy_store_vault::tasks::VaultTaskStore;
use orchy_store_vault::time::{SystemClock, UlidGenerator};
use orchy_store_vault::transaction::{StagedEventLog, VaultUnitOfWork};
use orchy_store_vault::vault::Vault;
use orchy_store_vault::watermarks::FileWatermarks;

use crate::config::Config;
use crate::error::{CliError, CliResult};

pub(crate) fn identify(config: &mut Config, announcing: bool) -> CliResult<()> {
    let Some(token) = config.session.clone() else {
        return Ok(());
    };
    let found = read_session(&config.runtime_root().join("sessions"), &token)?
        .filter(|session| session.is_live(Utc::now()));
    match found {
        Some(session) => {
            config.actor = session.actor().clone();
            if config.namespace.is_none() {
                config.namespace = Some(session.namespace().to_string());
            }
            Ok(())
        }
        None if announcing => {
            config.session = None;
            Ok(())
        }
        None => Err(CliError::UnknownSession(format!(
            "session {token} is unknown on this machine or has ended; run `orchy announce` for a new one"
        ))),
    }
}

pub(crate) async fn build(config: &Config) -> CliResult<Application> {
    let blobs: Arc<dyn BlobStore> = Arc::new(FsBlobStore::new(&config.vault));
    let vault = Arc::new(Vault::open(Arc::clone(&blobs)).await?);

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Arc<dyn IdGenerator> = Arc::new(UlidGenerator::new());
    let recorded: Arc<dyn EventLog> = Arc::new(
        EventuaryLog::open(
            config.events_root(),
            &config.organization,
            config.actor.clone(),
            config.machine.clone(),
            config.vault_config.events.partitions,
        )?
        .with_session(config.session.clone()),
    );
    let log: Arc<dyn EventLog> = Arc::new(StagedEventLog::new(Arc::clone(&recorded)));
    let unit_of_work: Arc<dyn UnitOfWork> =
        Arc::new(VaultUnitOfWork::new(Arc::clone(&vault), recorded));

    let actors: Arc<dyn ActorStore> =
        Arc::new(VaultActorStore::new(Arc::clone(&vault), Arc::clone(&log)));
    let documents = Arc::new(VaultDocumentStore::new(
        Arc::clone(&vault),
        Arc::clone(&log),
    ));

    let skills = Arc::new(VaultSkillStore::new(Arc::clone(&vault), Arc::clone(&log)));

    Ok(Application::new(ApplicationDeps {
        search: Arc::new(VaultSearch::new(
            Arc::clone(&documents),
            Arc::clone(&skills),
        )) as Arc<dyn Search>,
        documents: Arc::clone(&documents) as _,
        skills,
        tasks: Arc::new(VaultTaskStore::new(Arc::clone(&vault), Arc::clone(&log))),
        messages: Arc::new(VaultMessageStore::new(
            Arc::clone(&vault),
            Arc::clone(&actors),
            Arc::clone(&log),
        )),
        edges: Arc::new(VaultEdgeStore::new(
            Arc::clone(&vault),
            Arc::clone(&log),
            Arc::clone(&clock),
        )),
        integrity: Arc::new(VaultIntegrity::new(Arc::clone(&vault))),
        actors,
        sessions: Arc::new(FileSessionStore::new(
            config.runtime_root().join("sessions"),
            Arc::clone(&log),
        )),
        leases: Arc::new(FileLeaseStore::new(
            config.runtime_root().join("locks"),
            Arc::clone(&clock),
        )),
        watermarks: Arc::new(FileWatermarks::new(config.runtime_root().join("read")))
            as Arc<dyn ReadWatermarks>,
        log,
        clock,
        ids,
        unit_of_work,
    }))
}
