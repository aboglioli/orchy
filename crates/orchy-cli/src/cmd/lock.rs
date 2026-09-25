use std::process::Command;

use orchy_application::Application;
use orchy_application::dto::LeaseDto;
use orchy_application::manage_lease::{LeaseAction, ManageLeaseCommand};

use crate::error::CliResult;
use crate::output::Output;

pub(crate) async fn manage(
    app: &Application,
    actor: &str,
    resource: String,
    ttl: Option<i64>,
    action: LeaseAction,
    out: &Output,
) -> CliResult<()> {
    let lease = run(app, actor, resource, ttl, action).await?;
    out.emit(&lease, |l| match l {
        Some(lease) => describe(lease),
        None => "not held".to_owned(),
    })
}

pub(crate) async fn list(app: &Application, out: &Output) -> CliResult<()> {
    let held = app.manage_lease.held().await?;
    out.emit(&held, |leases| {
        if leases.is_empty() {
            return "nothing is held".to_owned();
        }
        leases.iter().map(describe).collect::<Vec<_>>().join("\n")
    })
}

pub(crate) async fn with(
    app: &Application,
    actor: &str,
    resource: String,
    ttl: Option<i64>,
    command: Vec<String>,
    out: &Output,
) -> CliResult<()> {
    let lease = run(app, actor, resource.clone(), ttl, LeaseAction::Acquire).await?;

    let ran = Command::new(&command[0]).args(&command[1..]).status();
    let released = run(app, actor, resource, None, LeaseAction::Release).await;

    let status = ran?;
    released?;
    out.emit(&lease, |_| String::new())?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

async fn run(
    app: &Application,
    actor: &str,
    resource: String,
    ttl: Option<i64>,
    action: LeaseAction,
) -> CliResult<Option<LeaseDto>> {
    Ok(app
        .manage_lease
        .execute(ManageLeaseCommand {
            resource,
            actor: actor.to_owned(),
            ttl_seconds: ttl,
            action,
        })
        .await?)
}

fn describe(lease: &LeaseDto) -> String {
    format!(
        "{} held by {} until {} (generation {})",
        lease.resource,
        lease.holder,
        lease.expires_at.format("%H:%M:%S"),
        lease.generation
    )
}
