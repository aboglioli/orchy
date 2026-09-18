use orchy_core::{ActorId, EntityKind, Id, Namespace, TaskStatus};

/// Placement is a projection of frontmatter, never a source of it: a task is done because
/// its frontmatter says so and lands in `tasks/done/` as a consequence. A file in the wrong
/// directory is a placement error to move, never a reason to rewrite frontmatter.
#[derive(Debug, Clone, Default)]
pub struct Layout;

pub const TASKS: &str = "tasks";
pub const MESSAGES: &str = "messages";
pub const AGENTS: &str = "agents";
pub const EVENTS: &str = "events";
pub const RUNTIME: &str = ".orchy";
pub const CANDIDATES: &str = "_candidates";

/// Everything else in the tree belongs to the user.
pub const RESERVED: [&str; 7] = [
    TASKS, MESSAGES, AGENTS, EVENTS, RUNTIME, CANDIDATES, "skills",
];

impl Layout {
    pub fn task_key(&self, id: &Id, status: TaskStatus) -> String {
        let bucket = if status.is_terminal() { "done" } else { "open" };
        format!("{TASKS}/{bucket}/{id}.md")
    }

    pub fn message_key(&self, thread: &Id, id: &Id) -> String {
        format!("{MESSAGES}/{thread}/{id}.md")
    }

    pub fn actor_key(&self, actor: &ActorId) -> String {
        format!("{AGENTS}/{actor}.md")
    }

    pub fn document_key(&self, namespace: &Namespace, id: &Id) -> String {
        let folder = namespace.as_str().trim_start_matches('/');
        if folder.is_empty() {
            format!("{id}.md")
        } else {
            format!("{folder}/{id}.md")
        }
    }

    pub fn key_for(&self, kind: EntityKind, id: &Id, namespace: &Namespace) -> String {
        match kind {
            EntityKind::Task => self.task_key(id, TaskStatus::Pending),
            EntityKind::Message => self.message_key(id, id),
            EntityKind::Document => self.document_key(namespace, id),
            EntityKind::Actor => format!("{AGENTS}/{id}.md"),
        }
    }

    pub fn is_reserved(&self, key: &str) -> bool {
        RESERVED
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

    fn id(s: &str) -> Id {
        Id::new(s).unwrap()
    }

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    #[test]
    fn a_tasks_folder_follows_its_status() {
        let layout = Layout;
        assert_eq!(
            layout.task_key(&id(A), TaskStatus::Pending),
            format!("tasks/open/{A}.md")
        );
        assert_eq!(
            layout.task_key(&id(A), TaskStatus::InProgress),
            format!("tasks/open/{A}.md"),
            "only a terminal status moves a task to done"
        );
        for terminal in [
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            assert_eq!(
                layout.task_key(&id(A), terminal),
                format!("tasks/done/{A}.md")
            );
        }
    }

    #[test]
    fn the_filename_is_the_id_and_nothing_else() {
        let layout = Layout;
        let key = layout.message_key(&id(A), &id(B));
        assert!(key.ends_with(&format!("{B}.md")), "{key}");
        assert!(
            !key.contains("claude"),
            "no sender, no slug: the id is the only thing in the name (D42)"
        );
    }

    #[test]
    fn a_message_lives_under_its_thread() {
        assert_eq!(
            Layout.message_key(&id(A), &id(B)),
            format!("messages/{A}/{B}.md")
        );
    }

    #[test]
    fn documents_follow_their_namespace_and_the_root_has_no_leading_slash() {
        let layout = Layout;
        assert_eq!(
            layout.document_key(&Namespace::new("/backend/auth").unwrap(), &id(A)),
            format!("backend/auth/{A}.md")
        );
        assert_eq!(
            layout.document_key(&Namespace::root(), &id(A)),
            format!("{A}.md")
        );
    }

    #[test]
    fn reserved_directories_are_recognised_without_catching_lookalikes() {
        let layout = Layout;
        assert!(layout.is_reserved("tasks/open/x.md"));
        assert!(layout.is_reserved("messages/a/b.md"));
        assert!(layout.is_reserved(".orchy/read/x.json"));
        assert!(
            !layout.is_reserved("tasksy/x.md"),
            "prefix must be a whole segment"
        );
        assert!(!layout.is_reserved("my-notes/tasks/x.md"));
    }

    #[test]
    fn runtime_paths_are_the_ones_that_never_get_committed() {
        let layout = Layout;
        assert!(layout.is_runtime(".orchy/locks/build.lock"));
        assert!(layout.is_runtime("events/01ARZ/0000.log"));
        assert!(!layout.is_runtime("tasks/open/x.md"));
    }
}
