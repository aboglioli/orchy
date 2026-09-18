use orchy_application::Application;
use orchy_application::find_documents::FindDocumentsCommand;
use orchy_application::list_tasks::ListTasksCommand;
use orchy_core::{DomainError, Id};

use crate::error::{CliError, CliResult};

/// Humans type a prefix, a suffix or a title fragment; ports take an `Id`. Turning one into
/// the other is the CLI's job, and an ambiguous match is an error rather than a guess.
pub(crate) async fn task(app: &Application, input: &str) -> CliResult<String> {
    if let Ok(id) = Id::new(input) {
        return Ok(id.to_string());
    }
    let tasks = app
        .list_tasks
        .execute(ListTasksCommand {
            limit: Some(1000),
            ..Default::default()
        })
        .await?;
    let candidates: Vec<String> = tasks
        .items
        .iter()
        .filter(|t| matches(&t.id, &t.title, input))
        .map(|t| t.id.clone())
        .collect();
    pick(input, candidates)
}

pub(crate) async fn document(app: &Application, input: &str) -> CliResult<String> {
    if let Ok(id) = Id::new(input) {
        return Ok(id.to_string());
    }
    let documents = app
        .find_documents
        .execute(FindDocumentsCommand {
            limit: Some(1000),
            ..Default::default()
        })
        .await?;
    let candidates: Vec<String> = documents
        .items
        .iter()
        .filter(|d| matches(&d.id, &d.title, input))
        .map(|d| d.id.clone())
        .collect();
    pick(input, candidates)
}

fn matches(id: &str, title: &str, input: &str) -> bool {
    let needle = input.to_lowercase();
    id.to_lowercase().starts_with(&needle)
        || id.to_lowercase().ends_with(&needle)
        || title.to_lowercase().contains(&needle)
}

fn pick(input: &str, mut candidates: Vec<String>) -> CliResult<String> {
    candidates.sort();
    candidates.dedup();
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err(CliError::from(DomainError::not_found("entity", input))),
        n => Err(CliError::from(DomainError::Ambiguous {
            input: input.to_owned(),
            count: n,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_id_short_circuits_the_scan() {
        assert!(Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
    }

    #[test]
    fn a_prefix_a_suffix_and_a_title_fragment_all_match() {
        assert!(matches(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "Rotate keys",
            "01arz"
        ));
        assert!(matches(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "Rotate keys",
            "g5fav"
        ));
        assert!(matches(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "Rotate keys",
            "rotate"
        ));
        assert!(!matches("01ARZ3NDEKTSV4RRFFQ69G5FAV", "Rotate keys", "zzz"));
    }

    #[test]
    fn one_candidate_resolves_and_several_are_refused_as_ambiguous() {
        assert_eq!(pick("x", vec!["a".to_owned()]).unwrap(), "a");
        let err = pick("x", vec!["a".to_owned(), "b".to_owned()]).unwrap_err();
        assert_eq!(err.exit_code(), 7);
    }

    #[test]
    fn nothing_matching_is_a_not_found_not_an_ambiguity() {
        assert_eq!(pick("x", vec![]).unwrap_err().exit_code(), 4);
    }

    #[test]
    fn duplicates_of_one_id_are_not_an_ambiguity() {
        assert_eq!(
            pick("x", vec!["a".to_owned(), "a".to_owned()]).unwrap(),
            "a"
        );
    }
}
