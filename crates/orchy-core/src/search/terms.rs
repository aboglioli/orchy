use std::sync::LazyLock;

use rust_stemmers::{Algorithm, Stemmer};

static ENGLISH: LazyLock<Stemmer> = LazyLock::new(|| Stemmer::create(Algorithm::English));

pub fn tokenise(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        terms.push(stem(word));
        let parts = parts_of(word);
        if parts.len() > 1 {
            terms.extend(parts.into_iter().map(stem));
        }
    }
    terms
}

fn stem(word: &str) -> String {
    ENGLISH.stem(&word.to_lowercase()).into_owned()
}

fn parts_of(word: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    let mut parts = Vec::new();
    let mut start = 0;
    for window in 1..chars.len() {
        let (_, previous) = chars[window - 1];
        let (at, current) = chars[window];
        let next = chars.get(window + 1).map(|(_, c)| *c);
        let lower_to_upper = previous.is_lowercase() && current.is_uppercase();
        let acronym_end = previous.is_uppercase()
            && current.is_uppercase()
            && next.is_some_and(char::is_lowercase);
        let digit_edge = previous.is_ascii_digit() != current.is_ascii_digit();
        if lower_to_upper || acronym_end || digit_edge {
            parts.push(&word[start..at]);
            start = at;
        }
    }
    parts.push(&word[start..]);
    parts
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
            vec!["sqlx", "0", "8", "rfc7231", "rfc", "7231"]
        );
    }

    #[test]
    fn an_identifier_is_found_by_the_words_it_is_made_of_and_by_itself() {
        let terms = tokenise("UserRepository");
        assert!(terms.contains(&stem("userrepository")), "{terms:?}");
        assert!(terms.contains(&stem("user")), "{terms:?}");
        assert!(terms.contains(&stem("repository")), "{terms:?}");
    }

    #[test]
    fn an_acronym_stays_one_part() {
        assert_eq!(parts_of("HTTPServer"), vec!["HTTP", "Server"]);
        assert_eq!(parts_of("parseJSONBody"), vec!["parse", "JSON", "Body"]);
        assert_eq!(parts_of("plain"), vec!["plain"]);
    }

    #[test]
    fn text_with_nothing_to_match_yields_no_terms() {
        assert!(tokenise("").is_empty());
        assert!(tokenise("   ").is_empty());
        assert!(tokenise("--- ;; ---").is_empty());
    }
}
