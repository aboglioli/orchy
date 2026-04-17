use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskId, TaskStatus, TaskStore};

pub struct AddDependencyCommand {
    pub task_id: String,
    pub dependency_id: String,
}

pub struct AddDependency {
    tasks: Arc<dyn TaskStore>,
}

impl AddDependency {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: AddDependencyCommand) -> Result<Task> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let dependency_id = cmd
            .dependency_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        self.tasks
            .find_by_id(&dependency_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("dependency task {dependency_id}")))?;

        if self.would_create_cycle(&task_id, &dependency_id).await? {
            return Err(Error::Conflict(format!(
                "adding dependency {dependency_id} to task {task_id} would create a cycle"
            )));
        }

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        if matches!(
            task.status(),
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            return Err(Error::InvalidInput(format!(
                "cannot add dependency to task {} with status {}",
                task_id,
                task.status()
            )));
        }

        task.add_dependency(dependency_id)?;

        if !self.all_deps_completed(task.depends_on()).await?
            && task.status() == TaskStatus::Pending
        {
            task.block()?;
        }

        self.tasks.save(&mut task).await?;
        Ok(task)
    }

    async fn would_create_cycle(&self, task_id: &TaskId, new_dep_id: &TaskId) -> Result<bool> {
        if task_id == new_dep_id {
            return Ok(true);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*new_dep_id);

        const MAX_TRAVERSAL: usize = 100;

        while let Some(current) = queue.pop_front() {
            if current == *task_id {
                return Ok(true);
            }

            if !visited.insert(current) {
                continue;
            }

            if visited.len() > MAX_TRAVERSAL {
                return Err(Error::InvalidInput(
                    "dependency graph too large to validate".into(),
                ));
            }

            if let Some(task) = self.tasks.find_by_id(&current).await? {
                for dep in task.depends_on() {
                    if !visited.contains(dep) {
                        queue.push_back(*dep);
                    }
                }
            }
        }

        Ok(false)
    }

    async fn all_deps_completed(&self, deps: &[TaskId]) -> Result<bool> {
        for dep_id in deps {
            let dep = self
                .tasks
                .find_by_id(dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            if dep.status() != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
