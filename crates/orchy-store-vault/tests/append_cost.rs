use std::time::Instant;

use orchy_core::task::TaskCreated;
use orchy_core::{ActorId, DomainEvent, EventLog, Id, MachineId, Namespace};
use orchy_store_vault::eventlog::EventuaryLog;

const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn event(n: usize) -> Box<dyn DomainEvent> {
    Box::new(TaskCreated {
        id: Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap(),
        namespace: Namespace::new("/backend").unwrap(),
        title: format!("event {n}"),
        parent: None,
        at: chrono::Utc::now(),
    })
}

/// Measures what the per-append writer open costs. orchy opens one writer per append as a
/// workaround for eventuary locking every partition at construction; once that is released
/// this can go back to a writer held for the process and this file can be deleted.
async fn time_appends(partitions: u32, count: usize) -> (u128, u128) {
    let temp = tempfile::tempdir().unwrap();
    let events = temp.path().join("events");
    std::fs::create_dir_all(&events).unwrap();
    let log = EventuaryLog::open(
        &events,
        "orchy",
        ActorId::new("bench", MACHINE).unwrap(),
        MachineId::new(MACHINE).unwrap(),
        partitions,
    )
    .unwrap();

    let warm = Instant::now();
    for n in 0..20 {
        log.append(&[event(n)]).await.unwrap();
    }
    let first_20 = warm.elapsed().as_micros() / 20;

    for n in 20..count {
        log.append(&[event(n)]).await.unwrap();
    }

    let late = Instant::now();
    for n in 0..20 {
        log.append(&[event(n)]).await.unwrap();
    }
    (first_20, late.elapsed().as_micros() / 20)
}

#[tokio::test]
#[ignore = "timing report, not an assertion"]
async fn report_append_cost() {
    println!("\n  append cost, one event per call (opens + locks + recovers the writer)\n");
    for partitions in [1u32, 10, 32] {
        let (early, late) = time_appends(partitions, 500).await;
        println!(
            "    partitions={partitions:<3} first 20: {early:>5} µs/append    after 500: {late:>5} µs/append"
        );
    }
    println!();
}
