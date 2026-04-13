use rmcp::schemars;
use serde::Deserialize;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RegisterAgentParams {
    pub project: String,
    pub namespace: Option<String>,
    pub roles: Option<Vec<String>>,
    pub description: String,
    pub agent_id: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ChangeRolesParams {
    pub roles: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListAgentsParams {}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveAgentParams {
    pub namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PostTaskParams {
    pub namespace: Option<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetNextTaskParams {
    pub namespace: Option<String>,
    pub role: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListTasksParams {
    pub namespace: Option<String>,
    pub status: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ClaimTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CompleteTaskParams {
    pub task_id: String,
    pub summary: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct StartTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct FailTaskParams {
    pub task_id: String,
    pub reason: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AssignTaskParams {
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddTaskNoteParams {
    pub task_id: String,
    pub body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SplitTaskParams {
    pub task_id: String,
    pub subtasks: Vec<SubtaskParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SubtaskParam {
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReplaceTaskParams {
    pub task_id: String,
    pub reason: Option<String>,
    pub replacements: Vec<SubtaskParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddDependencyParams {
    pub task_id: String,
    pub dependency_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RemoveDependencyParams {
    pub task_id: String,
    pub dependency_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveTaskParams {
    pub task_id: String,
    pub new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct WriteMemoryParams {
    pub namespace: Option<String>,
    pub key: String,
    pub value: String,
    pub version: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReadMemoryParams {
    pub namespace: Option<String>,
    pub key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListMemoryParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchMemoryParams {
    pub query: String,
    pub namespace: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DeleteMemoryParams {
    pub namespace: Option<String>,
    pub key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveMemoryParams {
    pub namespace: Option<String>,
    pub key: String,
    pub new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SendMessageParams {
    pub to: String,
    pub body: String,
    pub namespace: Option<String>,
    pub reply_to: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CheckMailboxParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MarkReadParams {
    pub message_ids: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CheckSentMessagesParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListConversationParams {
    pub message_id: String,
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SaveContextParams {
    pub summary: String,
    pub namespace: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct LoadContextParams {
    pub agent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListContextsParams {
    pub agent_id: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchContextsParams {
    pub query: String,
    pub namespace: Option<String>,
    pub agent_id: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct WriteSkillParams {
    pub name: String,
    pub description: String,
    pub content: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReadSkillParams {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListSkillsParams {
    pub namespace: Option<String>,
    pub inherited: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DeleteSkillParams {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveSkillParams {
    pub namespace: Option<String>,
    pub name: String,
    pub new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetBootstrapPromptParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetProjectParams {}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateProjectParams {
    pub description: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddProjectNoteParams {
    pub body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListNamespacesParams {}
