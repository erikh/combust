use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;

use super::Runner;
use combust_db::task::TaskState;

/// Represents the full project status.
#[derive(Debug, Serialize)]
pub struct ProjectStatus {
    pub tasks: BTreeMap<String, Vec<TaskInfo>>,
    pub running: Vec<RunningTaskInfo>,
}

/// A version of ProjectStatus that skips empty fields during serialization.
#[derive(Debug, Serialize)]
pub struct CompactProjectStatus {
    #[serde(flatten)]
    pub tasks: BTreeMap<String, Vec<TaskInfo>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub running: Vec<RunningTaskInfo>,
}

impl From<&ProjectStatus> for CompactProjectStatus {
    fn from(status: &ProjectStatus) -> Self {
        let tasks = status
            .tasks
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        CompactProjectStatus {
            tasks,
            running: status.running.clone(),
        }
    }
}

/// Information about a single task.
#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub group: String,
}

/// Information about a running task.
#[derive(Debug, Clone, Serialize)]
pub struct RunningTaskInfo {
    pub task_name: String,
    pub pid: u32,
}

impl Runner {
    /// Returns the full project status.
    pub fn status(&self) -> Result<ProjectStatus> {
        let mut tasks: BTreeMap<String, Vec<TaskInfo>> = BTreeMap::new();

        for state in TaskState::all() {
            let state_tasks = self.design.tasks_by_state(*state)?;
            let infos: Vec<TaskInfo> = state_tasks
                .iter()
                .map(|t| TaskInfo {
                    name: t.label(),
                    group: t.group.clone(),
                })
                .collect();
            tasks.insert(state.as_str().to_string(), infos);
        }

        // Get running tasks.
        let combust_dir = self.config.base_dir.join(combust_db::config::COMBUST_DIR);
        let running_locks = combust_db::lock::read_all(&combust_dir)
            .context("reading running tasks")?;

        let running: Vec<RunningTaskInfo> = running_locks
            .into_iter()
            .map(|r| RunningTaskInfo {
                task_name: r.task_name,
                pid: r.pid,
            })
            .collect();

        Ok(ProjectStatus { tasks, running })
    }

    /// Lists pending tasks.
    pub fn list_pending(&self) -> Result<Vec<combust_db::task::Task>> {
        self.design.pending_tasks()
    }

    /// Parses a RunningTaskInfo from lock data.
    pub fn parse_running_task(task_name: &str, pid: u32) -> RunningTaskInfo {
        RunningTaskInfo {
            task_name: task_name.to_string(),
            pid,
        }
    }

    /// Syncs issues from a remote source.
    pub fn sync_issues(&self, labels: &[String]) -> Result<()> {
        let repo = crate::git::Repo::open(&self.config.base_dir);
        let remote_url = repo.remote_url().context("getting remote URL")?;

        let source = crate::issues::resolve_source(
            &remote_url,
            &self.api_type,
            &self.gitea_url,
        )?;
        let (created, skipped) =
            crate::issues::sync(&self.design.path, source.as_ref(), labels)?;

        println!("Synced issues: {} created, {} skipped", created, skipped);

        // Cleanup completed/abandoned tasks.
        let closer = crate::issues::resolve_closer(
            &remote_url,
            &self.api_type,
            &self.gitea_url,
        )?;
        let record = combust_db::record::Record::new(&self.design.state_path);
        let (branches, issues) = crate::issues::cleanup(
            &self.design,
            &repo,
            closer.as_deref(),
            &record,
        )?;

        if branches > 0 || issues > 0 {
            println!(
                "Cleanup: {} remote branches deleted, {} issues closed",
                branches, issues
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_output_yaml() {
        let status = ProjectStatus {
            tasks: {
                let mut m = BTreeMap::new();
                m.insert(
                    "pending".to_string(),
                    vec![TaskInfo {
                        name: "task-a".to_string(),
                        group: String::new(),
                    }],
                );
                m.insert("review".to_string(), vec![]);
                m
            },
            running: vec![],
        };

        let yaml = serde_yaml::to_string(&status).unwrap();
        assert!(yaml.contains("pending"));
        assert!(yaml.contains("task-a"));
    }

    #[test]
    fn test_status_output_json() {
        let status = ProjectStatus {
            tasks: {
                let mut m = BTreeMap::new();
                m.insert(
                    "pending".to_string(),
                    vec![TaskInfo {
                        name: "task-b".to_string(),
                        group: "mygroup".to_string(),
                    }],
                );
                m
            },
            running: vec![RunningTaskInfo {
                task_name: "task-b".to_string(),
                pid: 12345,
            }],
        };

        let json = serde_json::to_string_pretty(&status).unwrap();
        assert!(json.contains("task-b"));
        assert!(json.contains("12345"));
        assert!(json.contains("mygroup"));
    }

    #[test]
    fn test_status_output_empty_omitted() {
        let status = ProjectStatus {
            tasks: {
                let mut m = BTreeMap::new();
                m.insert("pending".to_string(), vec![]);
                m.insert(
                    "completed".to_string(),
                    vec![TaskInfo {
                        name: "done".to_string(),
                        group: String::new(),
                    }],
                );
                m
            },
            running: vec![],
        };

        let compact = CompactProjectStatus::from(&status);
        // Empty states should be filtered out.
        assert!(!compact.tasks.contains_key("pending"));
        assert!(compact.tasks.contains_key("completed"));

        let json = serde_json::to_string(&compact).unwrap();
        assert!(!json.contains("pending"));
        assert!(json.contains("completed"));
    }

    #[test]
    fn test_parse_running_task() {
        let info = Runner::parse_running_task("my-task", 42);
        assert_eq!(info.task_name, "my-task");
        assert_eq!(info.pid, 42);
    }
}
