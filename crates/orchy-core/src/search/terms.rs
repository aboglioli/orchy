use std::sync::LazyLock;

use rust_stemmers::{Algorithm, Stemmer};

static ENGLISH: LazyLock<Stemmer> = LazyLock::new(|| Stemmer::create(Algorithm::English));

pub fn tokenise(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| ENGLISH.stem(&word.to_lowercase()).into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_and_its_inflections_reduce_to_one_term() {
        let stem = |w: &str| tokenise(w).pop().unwrap();
        assert_eq!(stem("migrate"), stem("migrations"));
        assert_eq!(stem("migration"), stem("migrating"));
        assert_eq!(stem("Deploy"), stem("deploys"));
    }

    #[test]
    fn punctuation_and_case_are_not_part_of_a_term() {
        assert_eq!(
            tokenise("Never edit an APPLIED migration; add a new one."),
            tokenise("never  edit an applied migration   add a new one")
        );
    }

    #[test]
    fn digits_survive_because_versions_and_codes_are_searched_for() {
        assert_eq!(
            tokenise("sqlx 0.8 rfc7231"),
            vec!["sqlx", "0", "8", "rfc7231"]
        );
    }

    #[test]
    fn text_with_nothing_to_match_yields_no_terms() {
        assert!(tokenise("").is_empty());
        assert!(tokenise("   ").is_empty());
        assert!(tokenise("--- ;; ---").is_empty());
    }
}
