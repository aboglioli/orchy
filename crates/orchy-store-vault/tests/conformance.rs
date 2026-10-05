use std::sync::Arc;

use chrono::Duration;
use orchy_core::{
    Body, Clock, Document, DocumentStore, Edge, EdgeStore, EntityKind, EntityRef, EventLog, Kind,
    Namespace, Priority, Relation, Role, Search, SearchQuery, Skill, SkillName, SkillStore,
    Summary, Tag, Task, TaskStore, Title,
};
use orchy_store_memory::{FixedClock, MemoryBackend, SeqIdGenerator};
use orchy_store_vault::blob::{BlobStore, FsBlobStore};
use orchy_store_vault::documents::VaultDocumentStore;
use orchy_store_vault::edges::VaultEdgeStore;
use orchy_store_vault::search::VaultSearch;
use orchy_store_vault::skills::VaultSkillStore;
use orchy_store_vault::tasks::VaultTaskStore;
use orchy_store_vault::vault::Vault;
use serde_json::json;

struct Ports {
    name: &'static str,
    documents: Arc<dyn DocumentStore>,
    tasks: Arc<dyn TaskStore>,
    skills: Arc<dyn SkillStore>,
    edges: Arc<dyn EdgeStore>,
    search: Arc<dyn Search>,
    _root: Option<tempfile::TempDir>,
}

fn memory() -> Ports {
    let backend = MemoryBackend::new();
    Ports {
        name: "memory",
        documents: Arc::clone(&backend.documents) as _,
        tasks: Arc::clone(&backend.tasks) as _,
        skills: Arc::clone(&backend.skills) as _,
        edges: Arc::clone(&backend.edges) as _,
        search: Arc::clone(&backend.search) as _,
        _root: None,
    }
}

async fn vault() -> Ports {
    let root = tempfile::tempdir().unwrap();
    let blobs: Arc<dyn BlobStore> = Arc::new(FsBlobStore::new(root.path()));
    let vault = Arc::new(Vault::open(blobs).await.unwrap());
    let log: Arc<dyn EventLog> = Arc::new(orchy_store_memory::MemoryEventLog::new());
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::at(1_700_000_000));
    let documents = Arc::new(VaultDocumentStore::new(
        Arc::clone(&vault),
        Arc::clone(&log),
    ));
    let skills = Arc::new(VaultSkillStore::new(Arc::clone(&vault), Arc::clone(&log)));
    Ports {
        name: "vault",
        search: Arc::new(VaultSearch::new(
            Arc::clone(&documents),
            Arc::clone(&skills),
        )),
        documents,
        skills,
        tasks: Arc::new(VaultTaskStore::new(Arc::clone(&vault), Arc::clone(&log))),
        edges: Arc::new(VaultEdgeStore::new(vault, log, clock)),
        _root: Some(root),
    }
}

async fn both() -> Vec<Ports> {
    vec![memory(), vault().await]
}

#[tokio::test]
async fn a_document_reads_back_as_it_was_saved() {
    for ports in both().await {
        let clock = FixedClock::at(1_700_000_000);
        let ids = SeqIdGenerator::new();
        let mut document = Document::create(
            Kind::Decision,
            Title::new("Rotate keys").unwrap(),
            Namespace::new("/backend").unwrap(),
            Body::new("Intro.\n\n## Decision\nRS256."),
            &ids,
            &clock,
        );
        clock.advance(Duration::hours(1));
        document.append("More.", &clock);
        document
            .set_field("reviewer", json!("alan"), &clock)
            .unwrap();
        document.retag(vec![Tag::new("auth").unwrap()], &[], &clock);
        ports.documents.save(&mut document).await.unwrap();

        let read = ports.documents.require(document.id()).await.unwrap();
        let name = ports.name;
        assert_eq!(read.title(), document.title(), "{name}");
        assert_eq!(read.status(), document.status(), "{name}");
        assert_eq!(read.tags(), document.tags(), "{name}");
        assert_eq!(read.body(), document.body(), "{name}");
        assert_eq!(
            read.frontmatter().string("reviewer"),
            Some("alan"),
            "{name}"
        );
        assert_eq!(
            read.updated_at(),
            document.updated_at(),
            "{name}: `updated` survives"
        );
        assert_eq!(read.content_hash(), document.content_hash(), "{name}");
    }
}

#[tokio::test]
async fn a_task_reads_back_as_it_was_saved() {
    for ports in both().await {
        let clock = FixedClock::at(1_700_000_000);
        let ids = SeqIdGenerator::new();
        let mut parent = Task::create(Title::new("goal").unwrap(), Namespace::root(), &ids, &clock);
        let mut dependency = Task::create(
            Title::new("first").unwrap(),
            Namespace::root(),
            &ids,
            &clock,
        );
        ports.tasks.save(&mut parent).await.unwrap();
        ports.tasks.save(&mut dependency).await.unwrap();

        let mut task = Task::create(
            Title::new("ship").unwrap(),
            Namespace::new("/web").unwrap(),
            &ids,
            &clock,
        );
        task.describe("Do it.".to_owned(), &clock);
        task.set_acceptance_criteria(Some("Tests pass.".to_owned()), &clock);
        task.set_priority(Priority::High, &clock);
        task.assign_roles(vec![Role::new("dev").unwrap()], &clock);
        task.retag(vec![Tag::new("release").unwrap()], &[], &clock);
        task.attach_to(&parent, &clock).unwrap();
        task.add_dependency(dependency.id().clone(), &clock)
            .unwrap();
        let holder = "claude@01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
        task.claim(holder, &clock).unwrap();
        clock.advance(Duration::minutes(5));
        task.complete(
            &"claude@01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            Some("Shipped in abc123.".to_owned()),
            &clock,
        )
        .unwrap();
        ports.tasks.save(&mut task).await.unwrap();

        let read = ports.tasks.require(task.id()).await.unwrap();
        let name = ports.name;
        assert_eq!(read.status(), task.status(), "{name}");
        assert_eq!(read.description(), task.description(), "{name}");
        assert_eq!(
            read.acceptance_criteria(),
            task.acceptance_criteria(),
            "{name}"
        );
        assert_eq!(read.note(), task.note(), "{name}: the outcome survives");
        assert_eq!(read.priority(), task.priority(), "{name}");
        assert_eq!(read.assigned_roles(), task.assigned_roles(), "{name}");
        assert_eq!(read.tags(), task.tags(), "{name}");
        assert_eq!(read.parent(), task.parent(), "{name}");
        assert_eq!(read.depends_on(), task.depends_on(), "{name}");
        assert_eq!(read.claimed_by(), task.claimed_by(), "{name}");
        assert_eq!(read.updated_at(), task.updated_at(), "{name}");
        assert_eq!(
            ports.tasks.children_of(parent.id()).await.unwrap().len(),
            1,
            "{name}"
        );
    }
}

#[tokio::test]
async fn a_link_from_any_kind_is_seen_from_both_ends() {
    for ports in both().await {
        let clock = FixedClock::at(1_700_000_000);
        let ids = SeqIdGenerator::new();
        let mut document = Document::create(
            Kind::Note,
            Title::new("target").unwrap(),
            Namespace::root(),
            Body::new("x"),
            &ids,
            &clock,
        );
        ports.documents.save(&mut document).await.unwrap();
        let mut skill = Skill::create(
            SkillName::new("commits").unwrap(),
            Summary::new("one line").unwrap(),
            Namespace::root(),
            Body::new("y"),
            &ids,
            &clock,
        );
        ports.skills.save(&mut skill).await.unwrap();
        let mut task = Task::create(Title::new("t").unwrap(), Namespace::root(), &ids, &clock);
        ports.tasks.save(&mut task).await.unwrap();

        let target = EntityRef::document(document.id().clone());
        for from in [
            EntityRef::new(EntityKind::Skill, skill.id().clone()),
            EntityRef::task(task.id().clone()),
        ] {
            let edge = Edge::new(from.clone(), target.clone(), Relation::RelatedTo).unwrap();
            ports.edges.add(&edge).await.unwrap();
            let name = ports.name;
            assert!(
                ports.edges.out(&from, None).await.unwrap().contains(&edge),
                "{name}"
            );
            assert!(
                ports
                    .edges
                    .incoming(&target, None)
                    .await
                    .unwrap()
                    .contains(&edge),
                "{name}"
            );
            let around = ports.edges.neighbourhood(&from, 1).await.unwrap();
            assert!(around.iter().any(|hop| hop.edge == edge), "{name}");
        }

        let name = ports.name;
        let related = ports.edges.of_relation(&Relation::RelatedTo).await.unwrap();
        assert_eq!(
            related.len(),
            2,
            "{name}: every stored link of the relation"
        );
        assert!(
            ports
                .edges
                .of_relation(&Relation::Supersedes)
                .await
                .unwrap()
                .is_empty(),
            "{name}: and none of another"
        );
    }
}

#[tokio::test]
async fn both_stores_find_the_same_passages() {
    let mut found = Vec::new();
    for ports in both().await {
        let clock = FixedClock::at(1_700_000_000);
        let ids = SeqIdGenerator::new();
        for (title, body) in [
            (
                "Deploys",
                "Intro about the pipeline.\n\n## Rollback\nundo a deploy",
            ),
            ("Caching", "no match here"),
            ("Tokens", "## Deploy keys\nrotate them"),
        ] {
            let mut document = Document::create(
                Kind::Note,
                Title::new(title).unwrap(),
                Namespace::root(),
                Body::new(body),
                &ids,
                &clock,
            );
            ports.documents.save(&mut document).await.unwrap();
        }
        let hits = ports
            .search
            .sections(&SearchQuery {
                text: "deploy".to_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        let mut headings: Vec<Option<String>> = hits.into_iter().map(|h| h.heading).collect();
        headings.sort();
        found.push((ports.name, headings));
    }
    assert_eq!(found[0].1, found[1].1, "{found:?}");
}
