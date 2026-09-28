use std::sync::Arc;

use orchy_application::claim_task::ClaimTaskCommand;
use orchy_application::complete_task::CompleteTaskCommand;
use orchy_application::create_task::CreateTaskCommand;
use orchy_application::get_task::GetTaskCommand;
use orchy_application::replace_task::ReplaceTaskCommand;
use orchy_application::split_task::SplitTaskCommand;
use orchy_application::{Application, ApplicationDeps};
use orchy_store_memory::MemoryBackend;

const ACTOR: &str = "claude@01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn app() -> Application {
    let backend = MemoryBackend::new();
    Application::new(ApplicationDeps {
        documents: Arc::clone(&backend.documents) as _,
        skills: Arc::clone(&backend.skills) as _,
        tasks: Arc::clone(&backend.tasks) as _,
        messages: Arc::clone(&backend.messages) as _,
        edges: Arc::clone(&backend.edges) as _,
        actors: Arc::clone(&backend.actors) as _,
        leases: Arc::clone(&backend.leases) as _,
        watermarks: Arc::clone(&backend.watermarks) as _,
        search: Arc::clone(&backend.search) as _,
        log: Arc::clone(&backend.log) as _,
        clock: Arc::clone(&backend.clock) as _,
        ids: Arc::clone(&backend.ids) as _,
    })
}

async fn create(app: &Application, title: &str) -> String {
    app.create_task
        .execute(CreateTaskCommand {
            title: title.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
}

async fn finish(app: &Application, id: &str) {
    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: id.to_owned(),
            actor: ACTOR.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: id.to_owned(),
            note: None,
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();
}

async fn status(app: &Application, id: &str) -> String {
    app.get_task
        .execute(GetTaskCommand {
            task_id: id.to_owned(),
        })
        .await
        .unwrap()
        .task
        .status
}

#[tokio::test]
async fn split_keeps_the_goal_open_as_an_umbrella() {
    let app = app();
    let goal = create(&app, "ship auth").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["design".to_owned(), "build".to_owned()],
        })
        .await
        .unwrap()
        .created;

    assert_eq!(status(&app, &goal).await, "pending", "the goal waits");
    for child in &children {
        assert_eq!(
            child.parent.as_deref(),
            Some(goal.as_str()),
            "a subtask points at its umbrella"
        );
    }

    finish(&app, &children[0].id).await;
    assert_eq!(status(&app, &goal).await, "pending", "one is not enough");
    finish(&app, &children[1].id).await;
    assert_eq!(status(&app, &goal).await, "completed");
}

#[tokio::test]
async fn replace_retires_the_original_and_leaves_the_new_tasks_free() {
    let app = app();
    let original = create(&app, "rewrite auth").await;
    let response = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: original.clone(),
            titles: vec!["rotate keys".to_owned(), "drop hs256".to_owned()],
            reason: Some("too coarse".to_owned()),
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(response.replaced.status, "superseded");
    assert_eq!(status(&app, &original).await, "superseded");

    for created in &response.created {
        assert_eq!(
            created.parent, None,
            "a replacement is not a subtask of what it replaced"
        );
        assert_eq!(created.status, "pending");
    }
}

#[tokio::test]
async fn a_replacement_records_what_it_superseded() {
    let app = app();
    let original = create(&app, "rewrite auth").await;
    let response = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: original.clone(),
            titles: vec!["rotate keys".to_owned()],
            reason: None,
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();

    let links = app
        .get_task
        .execute(GetTaskCommand {
            task_id: response.created[0].id.clone(),
        })
        .await
        .unwrap()
        .edges;

    assert!(
        links
            .iter()
            .any(|e| e.relation == "supersedes" && e.to.ends_with(&original)),
        "provenance must survive: {links:?}"
    );
}

#[tokio::test]
async fn replacing_a_subtask_keeps_its_work_under_the_same_goal() {
    let app = app();
    let goal = create(&app, "ship auth").await;
    let subtask = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["do everything".to_owned()],
        })
        .await
        .unwrap()
        .created[0]
        .id
        .clone();

    let response = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: subtask,
            titles: vec!["step one".to_owned(), "step two".to_owned()],
            reason: None,
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();

    for created in &response.created {
        assert_eq!(
            created.parent.as_deref(),
            Some(goal.as_str()),
            "the branch must not detach from the goal above it"
        );
    }
    assert_eq!(
        status(&app, &goal).await,
        "pending",
        "the goal now waits for the replacements instead"
    );
}

#[tokio::test]
async fn a_superseded_subtask_does_not_spoil_its_parents_completion() {
    let app = app();
    let goal = create(&app, "ship auth").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["keep".to_owned(), "rework".to_owned()],
        })
        .await
        .unwrap()
        .created;

    let replacements = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: children[1].id.clone(),
            titles: vec!["reworked".to_owned()],
            reason: None,
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap()
        .created;

    finish(&app, &children[0].id).await;
    assert_eq!(
        status(&app, &goal).await,
        "pending",
        "the replacement is still open"
    );

    finish(&app, &replacements[0].id).await;
    assert_eq!(
        status(&app, &goal).await,
        "completed",
        "superseded carries no verdict; the real completions decide"
    );
}

#[tokio::test]
async fn replacing_every_subtask_keeps_the_goal_open_for_the_replacements() {
    let app = app();
    let goal = create(&app, "abandoned approach").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["a".to_owned(), "b".to_owned()],
        })
        .await
        .unwrap()
        .created;

    for child in &children {
        app.replace_task
            .execute(ReplaceTaskCommand {
                task_id: child.id.clone(),
                titles: vec![format!("{} redone", child.title)],
                reason: None,
                actor: ACTOR.to_owned(),
            })
            .await
            .unwrap();
    }

    assert_eq!(
        status(&app, &goal).await,
        "pending",
        "the replacements inherited the goal, so it still waits"
    );
}

#[tokio::test]
async fn replacing_a_finished_task_is_refused() {
    let app = app();
    let done = create(&app, "already done").await;
    finish(&app, &done).await;

    let refused = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: done,
            titles: vec!["too late".to_owned()],
            reason: None,
            actor: ACTOR.to_owned(),
        })
        .await;
    assert!(refused.is_err(), "work already done was not replaced");
}

#[tokio::test]
async fn blocking_on_a_task_records_a_real_dependency() {
    let app = app();
    let blocker = create(&app, "upstream").await;
    let blocked = create(&app, "downstream").await;

    let task = app
        .block_task
        .execute(orchy_application::block_task::BlockTaskCommand {
            task_id: blocked.clone(),
            reason: None,
            on: vec![blocker.clone()],
        })
        .await
        .unwrap();

    assert_eq!(task.status, "blocked");
    assert_eq!(
        task.depends_on,
        vec![blocker],
        "the blocker must stay queryable, not live in prose"
    );
}

#[tokio::test]
async fn blocking_needs_a_reason_or_a_blocker() {
    let app = app();
    let id = create(&app, "parked").await;
    let refused = app
        .block_task
        .execute(orchy_application::block_task::BlockTaskCommand {
            task_id: id,
            reason: None,
            on: vec![],
        })
        .await;
    assert!(
        refused.is_err(),
        "blocked-for-no-stated-reason is not a state"
    );
}

#[tokio::test]
async fn re_parenting_into_a_task_own_subtree_is_refused() {
    let app = app();
    let goal = create(&app, "goal").await;
    let child = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["child".to_owned()],
        })
        .await
        .unwrap()
        .created[0]
        .id
        .clone();

    let refused = app
        .update_task
        .execute(orchy_application::update_task::UpdateTaskCommand {
            task_id: goal,
            parent: Some(child),
            ..Default::default()
        })
        .await;
    assert!(refused.is_err(), "a goal cannot become its own descendant");
}

#[tokio::test]
async fn detaching_frees_a_subtask_from_its_goal() {
    let app = app();
    let goal = create(&app, "goal").await;
    let child = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["child".to_owned()],
        })
        .await
        .unwrap()
        .created[0]
        .id
        .clone();

    let detached = app
        .update_task
        .execute(orchy_application::update_task::UpdateTaskCommand {
            task_id: child,
            detach: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(detached.parent, None);
}

#[tokio::test]
async fn link_refuses_the_relations_that_carry_consequences() {
    let app = app();
    let a = create(&app, "a").await;
    let b = create(&app, "b").await;

    for relation in ["parent", "depends_on", "supersedes", "spawned_by"] {
        let refused = app
            .link_entities
            .execute(orchy_application::link_entities::LinkEntitiesCommand {
                from: format!("task:{a}"),
                to: format!("task:{b}"),
                relation: relation.to_owned(),
                remove: false,
            })
            .await;
        assert!(
            refused.is_err(),
            "`{relation}` must go through its own command, not a raw link"
        );
    }
}

#[tokio::test]
async fn link_accepts_an_inert_relation_and_rejects_a_bad_endpoint() {
    let app = app();
    let a = create(&app, "a").await;
    let b = create(&app, "b").await;

    assert!(
        app.link_entities
            .execute(orchy_application::link_entities::LinkEntitiesCommand {
                from: format!("task:{a}"),
                to: format!("task:{b}"),
                relation: "related_to".to_owned(),
                remove: false,
            })
            .await
            .is_ok()
    );

    let refused = app
        .link_entities
        .execute(orchy_application::link_entities::LinkEntitiesCommand {
            from: format!("task:{a}"),
            to: format!("message:{b}"),
            relation: "produces".to_owned(),
            remove: false,
        })
        .await;
    assert!(
        refused.is_err(),
        "produces points at a document, not a message"
    );
}

#[tokio::test]
async fn asking_to_link_a_projected_name_says_which_side_to_store() {
    let app = app();
    let a = create(&app, "a").await;
    let b = create(&app, "b").await;

    let err = app
        .link_entities
        .execute(orchy_application::link_entities::LinkEntitiesCommand {
            from: format!("task:{a}"),
            to: format!("task:{b}"),
            relation: "subtasks".to_owned(),
            remove: false,
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("parent"), "{err}");
}

#[tokio::test]
async fn next_hands_each_agent_a_different_task_rather_than_failing() {
    use orchy_application::next_task::NextTaskCommand;

    let app = app();
    for title in ["one", "two", "three"] {
        create(&app, title).await;
    }

    let mut handed = Vec::new();
    for agent in ["claude", "codex", "gemini"] {
        let actor = format!("{agent}@01ARZ3NDEKTSV4RRFFQ69G5FAV");
        let task = app
            .next_task
            .execute(NextTaskCommand {
                actor,
                claim: true,
                ..Default::default()
            })
            .await
            .expect("a free task remains, so nobody is turned away")
            .expect("three tasks, three agents");
        handed.push(task.id);
    }

    handed.sort();
    handed.dedup();
    assert_eq!(handed.len(), 3, "each agent must get a task of its own");
}

#[tokio::test]
async fn next_returns_nothing_once_every_task_is_taken() {
    use orchy_application::next_task::NextTaskCommand;

    let app = app();
    create(&app, "only one").await;

    let first = app
        .next_task
        .execute(NextTaskCommand {
            actor: ACTOR.to_owned(),
            claim: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(first.is_some());

    let second = app
        .next_task
        .execute(NextTaskCommand {
            actor: "codex@01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
            claim: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        second.is_none(),
        "an empty queue is nothing to do, not an error"
    );
}
