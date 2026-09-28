use orchy_core::{ActorId, EntityKind, Id, Namespace, SkillName, TaskStatus};

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

impl Layout {
    pub fn task_key(&self, id: &Id, status: TaskStatus) -> String {
        let bucket = if status.is_terminal() { "done" } else { "open" };
        format!("{TASKS}/{bucket}/{id}.md")
    }

    pub fn message_key(&self, thread: &Id, id: &Id) -> String {
        format!("{MESSAGES}/{thread}/{id}.md")
    }

    pub fn skill_key(&self, namespace: &Namespace, name: &SkillName) -> String {
        let folder = namespace.as_str().trim_start_matches('/');
        if folder.is_empty() {
            format!("{SKILLS}/{name}.md")
        } else {
            format!("{SKILLS}/{folder}/{name}.md")
        }
    }

    pub fn actor_key(&self, actor: &ActorId) -> String {
        format!("{AGENTS}/{actor}.md")
    }

    pub fn document_key(&self, namespace: &Namespace, id: &Id) -> String {
        let folder = namespace.as_str().trim_start_matches('/');
        if folder.is_empty() {
            format!("{DOCS}/{id}.md")
        } else {
            format!("{DOCS}/{folder}/{id}.md")
        }
    }

    pub fn key_for(&self, kind: EntityKind, id: &Id, namespace: &Namespace) -> String {
        match kind {
            EntityKind::Task => self.task_key(id, TaskStatus::Pending),
            EntityKind::Message => self.message_key(id, id),
            EntityKind::Document => self.document_key(namespace, id),
            EntityKind::Skill => format!("{SKILLS}/{id}.md"),
            EntityKind::Actor => format!("{AGENTS}/{id}.md"),
        }
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
    fn documents_live_under_docs_with_their_namespace_beneath() {
        let layout = Layout;
        assert_eq!(
            layout.document_key(&Namespace::new("/backend/auth").unwrap(), &id(A)),
            format!("docs/backend/auth/{A}.md")
        );
        assert_eq!(
            layout.document_key(&Namespace::root(), &id(A)),
            format!("docs/{A}.md")
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
