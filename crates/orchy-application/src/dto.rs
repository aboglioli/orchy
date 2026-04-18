use std::collections::HashMap;

use serde::Serialize;

use orchy_core::agent::Agent;
use orchy_core::knowledge::Knowledge;
use orchy_core::message::Message;
use orchy_core::organization::Organization;
use orchy_core::pagination::Page;
use orchy_core::project::Project;
use orchy_core::resource_lock::ResourceLock;
use orchy_core::resource_ref::ResourceRef;
use orchy_core::task::{Task, TaskWithContext};

#[derive(Debug, Clone, Serialize)]
pub struct AgentResponse {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub namespace: String,
    pub parent_id: Option<String>,
    pub roles: Vec<String>,
    pub description: String,
    pub status: String,
    pub last_heartbeat: String,
    pub connected_at: String,
    pub metadata: HashMap<String, String>,
}

impl From<Agent> for AgentResponse {
    fn from(a: Agent) -> Self {
        Self::from(&a)
    }
}

impl From<&Agent> for AgentResponse {
    fn from(a: &Agent) -> Self {
        Self {
            id: a.id().to_string(),
            org_id: a.org_id().to_string(),
            project: a.project().to_string(),
            namespace: a.namespace().to_string(),
            parent_id: a.parent_id().map(|id| id.to_string()),
            roles: a.roles().to_vec(),
            description: a.description().to_string(),
            status: a.status().to_string(),
            last_heartbeat: a.last_heartbeat().to_rfc3339(),
            connected_at: a.connected_at().to_rfc3339(),
            metadata: a.metadata().clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub namespace: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub assigned_roles: Vec<String>,
    pub assigned_to: Option<String>,
    pub assigned_at: Option<String>,
    pub depends_on: Vec<String>,
    pub tags: Vec<String>,
    pub result_summary: Option<String>,
    pub refs: Vec<ResourceRefResponse>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Task> for TaskResponse {
    fn from(t: Task) -> Self {
        Self::from(&t)
    }
}

impl From<&Task> for TaskResponse {
    fn from(t: &Task) -> Self {
        Self {
            id: t.id().to_string(),
            org_id: t.org_id().to_string(),
            project: t.project().to_string(),
            namespace: t.namespace().to_string(),
            parent_id: t.parent_id().map(|id| id.to_string()),
            title: t.title().to_string(),
            description: t.description().to_string(),
            status: t.status().to_string(),
            priority: t.priority().to_string(),
            assigned_roles: t.assigned_roles().to_vec(),
            assigned_to: t.assigned_to().map(|id| id.to_string()),
            assigned_at: t.assigned_at().map(|dt| dt.to_rfc3339()),
            depends_on: t.depends_on().iter().map(|id| id.to_string()).collect(),
            tags: t.tags().to_vec(),
            result_summary: t.result_summary().map(|s| s.to_string()),
            refs: t.refs().iter().map(ResourceRefResponse::from).collect(),
            created_by: t.created_by().map(|id| id.to_string()),
            created_at: t.created_at().to_rfc3339(),
            updated_at: t.updated_at().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskWithContextResponse {
    #[serde(flatten)]
    pub task: TaskResponse,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ancestors: Vec<TaskResponse>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TaskResponse>,
}

impl From<TaskWithContext> for TaskWithContextResponse {
    fn from(ctx: TaskWithContext) -> Self {
        Self {
            task: TaskResponse::from(&ctx.task),
            ancestors: ctx.ancestors.iter().map(TaskResponse::from).collect(),
            children: ctx.children.iter().map(TaskResponse::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeResponse {
    pub id: String,
    pub org_id: String,
    pub project: Option<String>,
    pub namespace: String,
    pub path: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub version: u64,
    pub agent_id: Option<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Knowledge> for KnowledgeResponse {
    fn from(k: Knowledge) -> Self {
        Self::from(&k)
    }
}

impl From<&Knowledge> for KnowledgeResponse {
    fn from(k: &Knowledge) -> Self {
        Self {
            id: k.id().to_string(),
            org_id: k.org_id().to_string(),
            project: k.project().map(|p| p.to_string()),
            namespace: k.namespace().to_string(),
            path: k.path().to_string(),
            kind: k.kind().to_string(),
            title: k.title().to_string(),
            content: k.content().to_string(),
            tags: k.tags().to_vec(),
            version: k.version().as_u64(),
            agent_id: k.agent_id().map(|id| id.to_string()),
            metadata: k.metadata().clone(),
            created_at: k.created_at().to_rfc3339(),
            updated_at: k.updated_at().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub namespace: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub reply_to: Option<String>,
    pub status: String,
    pub refs: Vec<ResourceRefResponse>,
    pub created_at: String,
}

impl From<Message> for MessageResponse {
    fn from(m: Message) -> Self {
        Self::from(&m)
    }
}

impl From<&Message> for MessageResponse {
    fn from(m: &Message) -> Self {
        Self {
            id: m.id().to_string(),
            org_id: m.org_id().to_string(),
            project: m.project().to_string(),
            namespace: m.namespace().to_string(),
            from: m.from().to_string(),
            to: m.to().to_string(),
            body: m.body().to_string(),
            reply_to: m.reply_to().map(|id| id.to_string()),
            status: match m.status() {
                orchy_core::message::MessageStatus::Pending => "pending",
                orchy_core::message::MessageStatus::Delivered => "delivered",
                orchy_core::message::MessageStatus::Read => "read",
            }
            .to_string(),
            refs: m.refs().iter().map(ResourceRefResponse::from).collect(),
            created_at: m.created_at().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectResponse {
    pub org_id: String,
    pub id: String,
    pub description: String,
    pub metadata: HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Project> for ProjectResponse {
    fn from(p: Project) -> Self {
        Self::from(&p)
    }
}

impl From<&Project> for ProjectResponse {
    fn from(p: &Project) -> Self {
        Self {
            org_id: p.org_id().to_string(),
            id: p.id().to_string(),
            description: p.description().to_string(),
            metadata: p.metadata().clone(),
            created_at: p.created_at().to_rfc3339(),
            updated_at: p.updated_at().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceLockResponse {
    pub org_id: String,
    pub project: String,
    pub namespace: String,
    pub name: String,
    pub holder: String,
    pub acquired_at: String,
    pub expires_at: String,
}

impl From<ResourceLock> for ResourceLockResponse {
    fn from(l: ResourceLock) -> Self {
        Self::from(&l)
    }
}

impl From<&ResourceLock> for ResourceLockResponse {
    fn from(l: &ResourceLock) -> Self {
        Self {
            org_id: l.org_id().to_string(),
            project: l.project().to_string(),
            namespace: l.namespace().to_string(),
            name: l.name().to_string(),
            holder: l.holder().to_string(),
            acquired_at: l.acquired_at().to_rfc3339(),
            expires_at: l.expires_at().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceRefResponse {
    pub kind: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

impl From<&ResourceRef> for ResourceRefResponse {
    fn from(r: &ResourceRef) -> Self {
        Self {
            kind: r.kind().to_string(),
            id: r.id().to_string(),
            display: r.display().map(|s| s.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T: Serialize, R: Serialize> From<Page<T>> for PageResponse<R>
where
    R: From<T>,
{
    fn from(page: Page<T>) -> Self {
        Self {
            items: page.items.into_iter().map(R::from).collect(),
            next_cursor: page.next_cursor,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SummaryCounts {
    pub connected_agents: usize,
    pub inbox_messages: usize,
    pub pending_tasks: usize,
    pub skills: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSummaryResponse {
    pub agent: AgentResponse,
    pub project: Option<ProjectResponse>,
    pub counts: SummaryCounts,
    pub connected_agents: Vec<AgentResponse>,
    pub inbox: Vec<MessageResponse>,
    pub pending_tasks: Vec<TaskResponse>,
    pub skills: Vec<KnowledgeResponse>,
    pub handoff_context: Vec<KnowledgeResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrganizationResponse {
    pub id: String,
    pub name: String,
    pub api_keys: Vec<ApiKeyResponse>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Organization> for OrganizationResponse {
    fn from(o: Organization) -> Self {
        Self::from(&o)
    }
}

impl From<&Organization> for OrganizationResponse {
    fn from(o: &Organization) -> Self {
        Self {
            id: o.id().to_string(),
            name: o.name().to_string(),
            api_keys: o.api_keys().iter().map(ApiKeyResponse::from).collect(),
            created_at: o.created_at().to_rfc3339(),
            updated_at: o.updated_at().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub created_at: String,
}

impl From<&orchy_core::organization::ApiKey> for ApiKeyResponse {
    fn from(k: &orchy_core::organization::ApiKey) -> Self {
        Self {
            id: k.id().to_string(),
            name: k.name().to_string(),
            is_active: k.is_active(),
            created_at: k.created_at().to_rfc3339(),
        }
    }
}

pub struct ProjectOverviewResponse {
    pub project: Option<ProjectResponse>,
    pub agents: Vec<AgentResponse>,
    pub tasks: Vec<TaskResponse>,
    pub skills: Vec<KnowledgeResponse>,
    pub overviews: Vec<KnowledgeResponse>,
}
