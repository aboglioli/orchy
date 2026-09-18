use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Frontmatter(Vec<(String, Value)>);

impl Frontmatter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        let key = key.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some((_, slot)) => *slot = value,
            None => self.0.push((key, value)),
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let index = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(index).1)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(k, _)| k.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.0.retain(|(k, _)| keep(k));
    }

    pub fn strings(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        }
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
}

impl FromIterator<(String, Value)> for Frontmatter {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
        let mut fm = Self::new();
        for (k, v) in iter {
            fm.set(k, v);
        }
        fm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Frontmatter {
        let mut fm = Frontmatter::new();
        fm.set("type", json!("decision"));
        fm.set("title", json!("Rotate keys"));
        fm.set("tags", json!(["rust", "auth"]));
        fm
    }

    #[test]
    fn insertion_order_is_preserved_so_rewrites_do_not_shuffle_the_file() {
        assert_eq!(
            sample().keys().collect::<Vec<_>>(),
            vec!["type", "title", "tags"]
        );
    }

    #[test]
    fn setting_an_existing_key_updates_in_place_without_moving_it() {
        let mut fm = sample();
        fm.set("title", json!("Rotate signing keys"));
        assert_eq!(
            fm.keys().collect::<Vec<_>>(),
            vec!["type", "title", "tags"],
            "an update must not move the key to the end"
        );
        assert_eq!(fm.string("title"), Some("Rotate signing keys"));
    }

    #[test]
    fn unknown_keys_from_the_author_survive_a_round_trip() {
        let mut fm = sample();
        fm.set("my-own-field", json!(42));
        assert_eq!(fm.get("my-own-field"), Some(&json!(42)));
        assert_eq!(fm.len(), 4);
    }

    #[test]
    fn strings_reads_both_a_scalar_and_a_list() {
        let mut fm = Frontmatter::new();
        fm.set("tags", json!(["a", "b"]));
        fm.set("one", json!("solo"));
        assert_eq!(fm.strings("tags"), vec!["a", "b"]);
        assert_eq!(fm.strings("one"), vec!["solo"]);
        assert!(fm.strings("missing").is_empty());
    }

    #[test]
    fn strings_ignores_non_string_members_rather_than_failing() {
        let mut fm = Frontmatter::new();
        fm.set("tags", json!(["a", 7, "b"]));
        assert_eq!(fm.strings("tags"), vec!["a", "b"]);
    }

    #[test]
    fn retain_strips_projected_fields_before_a_write() {
        let mut fm = sample();
        fm.set("superseded_by", json!(["01ARZ"]));
        fm.retain(|k| k != "superseded_by");
        assert!(!fm.contains("superseded_by"));
        assert_eq!(fm.len(), 3);
    }

    #[test]
    fn remove_returns_what_it_took() {
        let mut fm = sample();
        assert_eq!(fm.remove("title"), Some(json!("Rotate keys")));
        assert_eq!(fm.remove("title"), None);
    }
}
