use orchy_core::agent::Agent;
use orchy_core::knowledge::Knowledge;
use orchy_core::project::Project;
use orchy_core::task::Task;

pub struct ProjectOverview {
    pub project: Option<Project>,
    pub agents: Vec<Agent>,
    pub tasks: Vec<Task>,
    pub overviews: Vec<Knowledge>,
}
