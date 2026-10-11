use chrono::{DateTime, Utc};
use deunicode::deunicode;
use orchy_core::{ActorId, Namespace, SkillName, TaskStatus};

#[derive(Debug, Clone, Default)]
pub struct Layout;

pub const DOCS: &str = "docs";
pub const TASKS: &str = "tasks";
pub const MESSAGES: &str = "messages";
pub const SKILLS: &str = "skills";
pub const AGENTS: &str = "agents";
pub const EVENTS: &str = "events";
pub const RUNTIME: &str = ".orchy";

pub const ROOTS: [&str; 7] = [DOCS, TASKS, MESSAGES, SKILLS, AGENTS, EVENTS, RUNTIME];

const MAX_SLUG: usize = 60;
const TOPIC_WORDS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    folder: String,
    stem: String,
    numbered: bool,
}

impl Slot {
    pub fn exact(folder: impl Into<String>, stem: impl Into<String>) -> Self {
        Self {
            folder: folder.into(),
            stem: stem.into(),
            numbered: false,
        }
    }

    pub fn numbered(folder: impl Into<String>, stem: impl Into<String>) -> Self {
        Self {
            folder: folder.into(),
            stem: stem.into(),
            numbered: true,
        }
    }

    pub fn is_numbered(&self) -> bool {
        self.numbered
    }

    pub fn path(&self, n: usize) -> String {
        if n <= 1 {
            return format!("{}/{}", self.folder, self.stem);
        }
        format!("{}/{}-{n}", self.folder, self.stem)
    }

    pub fn key(&self, n: usize) -> String {
        format!("{}.md", self.path(n))
    }

    pub fn fits(&self, key: &str) -> bool {
        key.strip_suffix(".md")
            .is_some_and(|path| self.fits_path(path))
    }

    pub fn fits_path(&self, path: &str) -> bool {
        let Some((folder, name)) = path.rsplit_once('/') else {
            return false;
        };
        if folder != self.folder {
            return false;
        }
        if name == self.stem {
            return true;
        }
        self.numbered
            && name
                .strip_prefix(self.stem.as_str())
                .and_then(|rest| rest.strip_prefix('-'))
                .filter(|n| !n.starts_with('0'))
                .and_then(|n| n.parse::<usize>().ok())
                .is_some_and(|n| n >= 2)
    }
}

pub fn slug(text: &str, fallback: &str) -> String {
    let ascii = deunicode(text).to_lowercase();
    let words: Vec<&str> = ascii
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut slug = String::new();
    for word in words {
        let extra = if slug.is_empty() { 0 } else { 1 };
        if slug.len() + extra + word.len() > MAX_SLUG {
            if slug.is_empty() {
                slug.push_str(&word[..MAX_SLUG]);
            }
            break;
        }
        if extra == 1 {
            slug.push('-');
        }
        slug.push_str(word);
    }
    if slug.is_empty() {
        return fallback.to_owned();
    }
    slug
}

pub fn topic_of(subject: Option<&str>, body: &str) -> String {
    match subject {
        Some(subject) => subject.to_owned(),
        None => body
            .split_whitespace()
            .take(TOPIC_WORDS)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn under(root: &str, namespace: &Namespace) -> String {
    let folder = namespace.as_str().trim_start_matches('/');
    if folder.is_empty() {
        return root.to_owned();
    }
    format!("{root}/{folder}")
}

impl Layout {
    pub fn task_slot(&self, status: TaskStatus, title: &str) -> Slot {
        let bucket = if status.is_terminal() { "done" } else { "open" };
        Slot::numbered(format!("{TASKS}/{bucket}"), slug(title, "task"))
    }

    pub fn document_slot(&self, namespace: &Namespace, title: &str) -> Slot {
        Slot::numbered(under(DOCS, namespace), slug(title, "document"))
    }

    pub fn skill_slot(&self, namespace: &Namespace, name: &SkillName) -> Slot {
        Slot::exact(under(SKILLS, namespace), name.as_str())
    }

    pub fn thread_slot(&self, topic: &str) -> Slot {
        Slot::numbered(MESSAGES, slug(topic, "thread"))
    }

    pub fn message_slot(&self, thread_folder: &str, sent: DateTime<Utc>, from: &ActorId) -> Slot {
        let stem = format!(
            "{}-{}",
            sent.format("%Y-%m-%d-%H%M"),
            slug(from.alias().as_str(), "agent")
        );
        Slot::numbered(thread_folder, stem)
    }

    pub fn actor_key(&self, actor: &ActorId) -> String {
        format!("{AGENTS}/{actor}.md")
    }

    pub fn is_root(&self, key: &str) -> bool {
        ROOTS
            .iter()
            .any(|dir| key == *dir || key.starts_with(&format!("{dir}/")))
    }

    pub fn is_runtime(&self, key: &str) -> bool {
        key.starts_with(&format!("{RUNTIME}/")) || key.starts_with(&format!("{EVENTS}/"))
    }

    pub fn is_markdown(&self, key: &str) -> bool {
        key.ends_with(".md")
    }

    pub fn watermark_key(&self, actor: &ActorId) -> String {
        format!("{RUNTIME}/read/{actor}.json")
    }

    pub fn presence_key(&self, actor: &ActorId) -> String {
        format!("{RUNTIME}/presence/{actor}.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn a_slug_is_the_title_in_lowercase_ascii_words() {
        assert_eq!(slug("Rotate JWT keys", "x"), "rotate-jwt-keys");
        assert_eq!(
            slug("  Fix: login → redirect!! ", "x"),
            "fix-login-redirect"
        );
        assert_eq!(slug("Migración de índices", "x"), "migracion-de-indices");
        assert_eq!(slug("Straße", "x"), "strasse");
    }

    #[test]
    fn a_slug_with_nothing_readable_falls_back_to_the_kind() {
        assert_eq!(slug("!!!", "note"), "note");
        assert_eq!(slug("", "task"), "task");
    }

    #[test]
    fn a_long_title_is_cut_at_a_word_boundary() {
        let title = "a ".repeat(10) + &"word ".repeat(30);
        let cut = slug(&title, "x");
        assert!(cut.len() <= MAX_SLUG, "{cut}");
        assert!(!cut.ends_with('-'), "{cut}");
        assert!(cut.ends_with("word"), "{cut}");
        assert_eq!(slug(&"x".repeat(100), "y").len(), MAX_SLUG);
    }

    #[test]
    fn a_tasks_folder_follows_its_status() {
        assert_eq!(
            Layout.task_slot(TaskStatus::InProgress, "Ship it").key(1),
            "tasks/open/ship-it.md"
        );
        for terminal in [
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            assert_eq!(
                Layout.task_slot(terminal, "Ship it").key(1),
                "tasks/done/ship-it.md"
            );
        }
    }

    #[test]
    fn documents_live_under_docs_with_their_namespace_beneath() {
        let backend = Namespace::new("/backend/auth").unwrap();
        assert_eq!(
            Layout.document_slot(&backend, "Rotate keys").key(1),
            "docs/backend/auth/rotate-keys.md"
        );
        assert_eq!(
            Layout
                .document_slot(&Namespace::root(), "Rotate keys")
                .key(2),
            "docs/rotate-keys-2.md"
        );
    }

    #[test]
    fn a_numbered_slot_takes_its_stem_or_a_suffix_from_two_on() {
        let slot = Layout.document_slot(&Namespace::root(), "Rotate keys");
        assert!(slot.fits("docs/rotate-keys.md"));
        assert!(slot.fits("docs/rotate-keys-2.md"));
        assert!(slot.fits("docs/rotate-keys-17.md"));
        assert!(!slot.fits("docs/rotate-keys-1.md"));
        assert!(!slot.fits("docs/rotate-keys-02.md"));
        assert!(!slot.fits("docs/rotate-keys-x.md"));
        assert!(!slot.fits("docs/backend/rotate-keys.md"));
        assert!(!slot.fits("docs/rotate.md"));
    }

    #[test]
    fn a_skill_is_filed_by_its_name_exactly() {
        let slot = Layout.skill_slot(
            &Namespace::new("/web").unwrap(),
            &SkillName::new("commits").unwrap(),
        );
        assert_eq!(slot.key(1), "skills/web/commits.md");
        assert!(!slot.fits("skills/web/commits-2.md"));
    }

    #[test]
    fn a_message_is_named_by_when_and_by_whom_inside_its_thread() {
        let thread = Layout.thread_slot(&topic_of(Some("Deploy freeze"), "ignored"));
        assert_eq!(thread.path(1), "messages/deploy-freeze");
        let sent = DateTime::from_timestamp(1_791_208_980, 0).unwrap();
        let from = ActorId::new("coder-1", MACHINE).unwrap();
        assert_eq!(
            Layout
                .message_slot("messages/deploy-freeze", sent, &from)
                .key(1),
            "messages/deploy-freeze/2026-10-05-1403-coder-1.md"
        );
    }

    #[test]
    fn a_thread_without_a_subject_is_named_after_its_first_words() {
        assert_eq!(
            topic_of(
                None,
                "please freeze merges to main for the next hour or two"
            ),
            "please freeze merges to main for the next"
        );
    }

    #[test]
    fn the_fixed_roots_are_recognised_without_catching_lookalikes() {
        let layout = Layout;
        assert!(layout.is_root("tasks/open/x.md"));
        assert!(layout.is_root("messages/a/b.md"));
        assert!(layout.is_root(".orchy/read/x.json"));
        assert!(
            !layout.is_root("tasksy/x.md"),
            "prefix must be a whole segment"
        );
        assert!(!layout.is_root("my-notes/tasks/x.md"));
    }

    #[test]
    fn runtime_paths_are_the_ones_that_never_get_committed() {
        let layout = Layout;
        assert!(layout.is_runtime(".orchy/locks/build.lock"));
        assert!(layout.is_runtime("events/01ARZ/0000.log"));
        assert!(!layout.is_runtime("tasks/open/x.md"));
    }
}
