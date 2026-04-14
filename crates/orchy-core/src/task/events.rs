use serde::Serialize;

pub const NAMESPACE: &str = "task";

pub const TOPIC_CREATED: &str = "task.created";
pub const TOPIC_CLAIMED: &str = "task.claimed";
pub const TOPIC_STARTED: &str = "task.started";
pub const TOPIC_COMPLETED: &str = "task.completed";
pub const TOPIC_AUTO_COMPLETED: &str = "task.auto_completed";
pub const TOPIC_FAILED: &str = "task.failed";
pub const TOPIC_RELEASED: &str = "task.released";
pub const TOPIC_ASSIGNED: &str = "task.assigned";
pub const TOPIC_BLOCKED: &str = "task.blocked";
pub const TOPIC_UNBLOCKED: &str = "task.unblocked";
pub const TOPIC_CANCELLED: &str = "task.cancelled";
pub const TOPIC_DEPENDENCY_ADDED: &str = "task.dependency_added";
pub const TOPIC_DEPENDENCY_REMOVED: &str = "task.dependency_removed";
pub const TOPIC_TAGGED: &str = "task.tagged";
pub const TOPIC_TAG_REMOVED: &str = "task.tag_removed";
pub const TOPIC_NOTE_ADDED: &str = "task.note_added";
pub const TOPIC_PARENT_CHANGED: &str = "task.parent_changed";
pub const TOPIC_DEPENDENCY_REPLACED: &str = "task.dependency_replaced";
pub const TOPIC_MOVED: &str = "task.moved";

#[derive(Serialize)]
pub struct TaskCreatedPayload {
    pub task_id: String,
    pub project: String,
    pub namespace: String,
    pub title: String,
    pub parent_id: Option<String>,
}

#[derive(Serialize)]
pub struct TaskClaimedPayload {
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Serialize)]
pub struct TaskStartedPayload {
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Serialize)]
pub struct TaskCompletedPayload {
    pub task_id: String,
    pub summary: Option<String>,
}

#[derive(Serialize)]
pub struct TaskFailedPayload {
    pub task_id: String,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct TaskReleasedPayload {
    pub task_id: String,
}

#[derive(Serialize)]
pub struct TaskAssignedPayload {
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Serialize)]
pub struct TaskBlockedPayload {
    pub task_id: String,
}

#[derive(Serialize)]
pub struct TaskUnblockedPayload {
    pub task_id: String,
}

#[derive(Serialize)]
pub struct TaskCancelledPayload {
    pub task_id: String,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct TaskDependencyAddedPayload {
    pub task_id: String,
    pub dependency_id: String,
}

#[derive(Serialize)]
pub struct TaskDependencyRemovedPayload {
    pub task_id: String,
    pub dependency_id: String,
}

#[derive(Serialize)]
pub struct TaskTaggedPayload {
    pub task_id: String,
    pub tag: String,
}

#[derive(Serialize)]
pub struct TaskTagRemovedPayload {
    pub task_id: String,
    pub tag: String,
}

#[derive(Serialize)]
pub struct TaskNoteAddedPayload {
    pub task_id: String,
    pub body: String,
}

#[derive(Serialize)]
pub struct TaskParentChangedPayload {
    pub task_id: String,
    pub parent_id: Option<String>,
}

#[derive(Serialize)]
pub struct TaskDependencyReplacedPayload {
    pub task_id: String,
    pub old_dependency_id: String,
    pub new_dependency_id: String,
}

#[derive(Serialize)]
pub struct TaskMovedPayload {
    pub task_id: String,
    pub from_namespace: String,
    pub to_namespace: String,
}

pub const TOPIC_WATCHER_ADDED: &str = "task.watcher_added";
pub const TOPIC_WATCHER_REMOVED: &str = "task.watcher_removed";
pub const TOPIC_REVIEW_REQUESTED: &str = "task.review_requested";
pub const TOPIC_REVIEW_APPROVED: &str = "task.review_approved";
pub const TOPIC_REVIEW_REJECTED: &str = "task.review_rejected";

#[derive(Serialize)]
pub struct TaskWatcherAddedPayload {
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Serialize)]
pub struct TaskWatcherRemovedPayload {
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Serialize)]
pub struct ReviewRequestedPayload {
    pub review_id: String,
    pub task_id: String,
    pub requester: String,
}

#[derive(Serialize)]
pub struct ReviewApprovedPayload {
    pub review_id: String,
    pub task_id: String,
}

#[derive(Serialize)]
pub struct ReviewRejectedPayload {
    pub review_id: String,
    pub task_id: String,
}
