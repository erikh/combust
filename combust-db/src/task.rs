use anyhow::{Context, Result};
use std::fmt;
use std::fs;
use std::path::PathBuf;

/// Represents the lifecycle state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Review,
    Merge,
    Completed,
    Abandoned,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Review => "review",
            TaskState::Merge => "merge",
            TaskState::Completed => "completed",
            TaskState::Abandoned => "abandoned",
        }
    }

    pub fn all() -> &'static [TaskState] {
        &[
            TaskState::Pending,
            TaskState::Review,
            TaskState::Merge,
            TaskState::Completed,
            TaskState::Abandoned,
        ]
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Represents a single design task.
#[derive(Debug, Clone)]
pub struct Task {
    pub name: String,
    pub file_path: PathBuf,
    pub group: String,
    pub state: TaskState,
}

impl Task {
    /// Reads and returns the task's markdown content.
    pub fn content(&self) -> Result<String> {
        fs::read_to_string(&self.file_path)
            .with_context(|| format!("reading task {}", self.name))
    }

    /// Returns the normalized git branch name for this task.
    pub fn branch_name(&self) -> String {
        let name = if self.group.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.group, self.name)
        };
        let normalized = name.to_lowercase().replace(' ', "-");
        format!("combust/{}", normalized)
    }

    /// Returns the display label (group/name or just name).
    pub fn label(&self) -> String {
        if self.group.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.group, self.name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_task_state_as_str() {
        assert_eq!(TaskState::Pending.as_str(), "pending");
        assert_eq!(TaskState::Review.as_str(), "review");
        assert_eq!(TaskState::Merge.as_str(), "merge");
        assert_eq!(TaskState::Completed.as_str(), "completed");
        assert_eq!(TaskState::Abandoned.as_str(), "abandoned");
    }

    #[test]
    fn test_task_state_display() {
        assert_eq!(format!("{}", TaskState::Pending), "pending");
        assert_eq!(format!("{}", TaskState::Completed), "completed");
    }

    #[test]
    fn test_task_state_all() {
        let all = TaskState::all();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0], TaskState::Pending);
        assert_eq!(all[4], TaskState::Abandoned);
    }

    #[test]
    fn test_task_content() {
        let tmp = TempDir::new().unwrap();
        let task_file = tmp.path().join("my-task.md");
        std::fs::write(&task_file, "Implement the feature.").unwrap();

        let task = Task {
            name: "my-task".to_string(),
            file_path: task_file,
            group: String::new(),
            state: TaskState::Pending,
        };

        assert_eq!(task.content().unwrap(), "Implement the feature.");
    }

    #[test]
    fn test_task_branch_name_simple() {
        let task = Task {
            name: "add-auth".to_string(),
            file_path: PathBuf::from("/tmp/add-auth.md"),
            group: String::new(),
            state: TaskState::Pending,
        };
        assert_eq!(task.branch_name(), "combust/add-auth");
    }

    #[test]
    fn test_task_branch_name_with_spaces() {
        let task = Task {
            name: "Add Auth".to_string(),
            file_path: PathBuf::from("/tmp/Add Auth.md"),
            group: String::new(),
            state: TaskState::Pending,
        };
        assert_eq!(task.branch_name(), "combust/add-auth");
    }

    #[test]
    fn test_task_branch_name_grouped() {
        let task = Task {
            name: "add-api".to_string(),
            file_path: PathBuf::from("/tmp/add-api.md"),
            group: "backend".to_string(),
            state: TaskState::Pending,
        };
        assert_eq!(task.branch_name(), "combust/backend/add-api");
    }

    #[test]
    fn test_task_branch_name_grouped_with_spaces() {
        let task = Task {
            name: "My Task".to_string(),
            file_path: PathBuf::from("/tmp/My Task.md"),
            group: "Frontend".to_string(),
            state: TaskState::Pending,
        };
        assert_eq!(task.branch_name(), "combust/frontend/my-task");
    }

    #[test]
    fn test_task_label_ungrouped() {
        let task = Task {
            name: "add-auth".to_string(),
            file_path: PathBuf::from("/tmp/add-auth.md"),
            group: String::new(),
            state: TaskState::Pending,
        };
        assert_eq!(task.label(), "add-auth");
    }

    #[test]
    fn test_task_label_grouped() {
        let task = Task {
            name: "add-api".to_string(),
            file_path: PathBuf::from("/tmp/add-api.md"),
            group: "backend".to_string(),
            state: TaskState::Pending,
        };
        assert_eq!(task.label(), "backend/add-api");
    }
}
