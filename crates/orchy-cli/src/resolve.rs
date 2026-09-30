use orchy_application::Application;
use orchy_application::read_skill::ReadSkillCommand;
use orchy_application::resolve_reference::{ReferenceKind, ResolveReferenceCommand};
use orchy_core::Id;

use crate::error::{CliError, CliResult};

pub(crate) async fn task(app: &Application, input: &str) -> CliResult<String> {
    reference(app, ReferenceKind::Task, input).await
}

pub(crate) async fn message(app: &Application, input: &str) -> CliResult<String> {
    reference(app, ReferenceKind::Message, input).await
}

pub(crate) async fn document(app: &Application, input: &str) -> CliResult<String> {
    if let Ok(id) = Id::new(input) {
        let is_skill = app
            .read_skill
            .execute(ReadSkillCommand {
                target: id.to_string(),
                namespace: None,
                actor: None,
            })
            .await
            .is_ok();
        if is_skill {
            return Err(CliError::skill_given_to_a_document_command(id));
        }
    }
    reference(app, ReferenceKind::Document, input).await
}

async fn reference(app: &Application, kind: ReferenceKind, input: &str) -> CliResult<String> {
    Ok(app
        .resolve_reference
        .execute(ResolveReferenceCommand {
            kind,
            input: input.to_owned(),
        })
        .await?)
}
