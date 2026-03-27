pub mod github;
pub mod gitea;

use anyhow::{Context, Result};
use combust_db::milestone::slugify;
use std::fs;
use std::path::Path;

/// Represents a single remote issue.
#[derive(Debug, Clone)]
pub struct Issue {
    pub number: i64,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub url: String,
}

/// Trait for fetching issues from a remote source.
pub trait Source: Send + Sync {
    fn fetch_open_issues(&self, labels: &[String]) -> Result<Vec<Issue>>;
}

/// Trait for closing issues on a remote source.
pub trait Closer: Send + Sync {
    fn close_issue(&self, number: i64, comment: &str) -> Result<()>;
}

/// Imports open issues into design/tasks/issues/.
/// Returns (created_count, skipped_count).
pub fn sync(design_dir: &Path, source: &dyn Source, labels: &[String]) -> Result<(usize, usize)> {
    let issues = source.fetch_open_issues(labels)?;

    let issues_dir = design_dir.join("tasks/issues");
    fs::create_dir_all(&issues_dir).context("creating issues task directory")?;

    // Ensure group.md exists.
    let group_md = issues_dir.join("group.md");
    if !group_md.exists() {
        fs::write(
            &group_md,
            "Tasks imported from remote issue tracker.\n",
        )
        .context("creating group.md for issues")?;
    }

    let mut created = 0;
    let mut skipped = 0;

    for issue in &issues {
        if issue_file_exists(&issues_dir, issue.number) {
            skipped += 1;
            continue;
        }

        let slug = slugify(&issue.title);
        let filename = format!("{}-{}.md", issue.number, slug);
        let content = format_issue_content(issue);

        fs::write(issues_dir.join(&filename), &content)
            .with_context(|| format!("writing issue file {}", filename))?;
        created += 1;
    }

    Ok((created, skipped))
}

/// Checks if a task file already exists for the given issue number.
fn issue_file_exists(dir: &Path, number: i64) -> bool {
    let prefix = format!("{}-", number);
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.ends_with(".md") {
            return true;
        }
    }
    false
}

/// Formats an issue into task file content.
fn format_issue_content(issue: &Issue) -> String {
    let mut content = String::new();
    content.push_str(&format!("# Issue #{}: {}\n\n", issue.number, issue.title));
    content.push_str(&format!("URL: {}\n\n", issue.url));

    if !issue.labels.is_empty() {
        content.push_str(&format!("Labels: {}\n\n", issue.labels.join(", ")));
    }

    if !issue.body.is_empty() {
        content.push_str(&issue.body);
        content.push('\n');
    }

    content
}

/// Returns true if the task belongs to the "issues" group.
pub fn is_issue_task(task: &combust_db::task::Task) -> bool {
    task.group == "issues"
}

/// Extracts the issue number from a task name like "42-fix-bug".
pub fn parse_issue_number(task_name: &str) -> Option<i64> {
    let first = task_name.split('-').next()?;
    first.parse::<i64>().ok().filter(|&n| n > 0)
}

/// Resolves an issue source from a repository URL.
pub fn resolve_source(repo_url: &str, api_type: &str, gitea_url: &str) -> Result<Box<dyn Source>> {
    match api_type {
        "github" => {
            let (owner, repo) = github::parse_github_url(repo_url)?;
            Ok(Box::new(github::GitHubSource::new(&owner, &repo)))
        }
        "gitea" => {
            let (base_url, owner, repo) = if gitea_url.is_empty() {
                gitea::parse_gitea_url(repo_url)?
            } else {
                let (_, owner, repo) = gitea::parse_gitea_url(repo_url)?;
                (gitea_url.to_string(), owner, repo)
            };
            Ok(Box::new(gitea::GiteaSource::new(&base_url, &owner, &repo)))
        }
        "" => {
            // Auto-detect.
            if repo_url.contains("github.com") {
                let (owner, repo) = github::parse_github_url(repo_url)?;
                Ok(Box::new(github::GitHubSource::new(&owner, &repo)))
            } else {
                let (base_url, owner, repo) = gitea::parse_gitea_url(repo_url)?;
                Ok(Box::new(gitea::GiteaSource::new(&base_url, &owner, &repo)))
            }
        }
        other => anyhow::bail!("unknown API type: {}", other),
    }
}

/// Resolves a Closer from a Source, if it supports closing.
pub fn resolve_closer(
    repo_url: &str,
    api_type: &str,
    gitea_url: &str,
) -> Result<Option<Box<dyn Closer>>> {
    match api_type {
        "github" | "" if repo_url.contains("github.com") => {
            let (owner, repo) = github::parse_github_url(repo_url)?;
            Ok(Some(Box::new(github::GitHubSource::new(&owner, &repo))))
        }
        "gitea" => {
            let (base_url, owner, repo) = if gitea_url.is_empty() {
                gitea::parse_gitea_url(repo_url)?
            } else {
                let (_, owner, repo) = gitea::parse_gitea_url(repo_url)?;
                (gitea_url.to_string(), owner, repo)
            };
            Ok(Some(Box::new(gitea::GiteaSource::new(
                &base_url, &owner, &repo,
            ))))
        }
        _ => Ok(None),
    }
}

/// Cleans up completed/abandoned tasks by deleting remote branches and closing issues.
pub fn cleanup(
    design: &combust_db::design::DesignDir,
    source_repo: &crate::git::Repo,
    closer: Option<&dyn Closer>,
    record: &combust_db::record::Record,
) -> Result<(usize, usize)> {
    use combust_db::task::TaskState;

    let mut branches_deleted = 0;
    let mut issues_closed = 0;

    for state in &[TaskState::Completed, TaskState::Abandoned] {
        let tasks = design.tasks_by_state(*state)?;
        for task in &tasks {
            let branch = task.branch_name();
            // Try to delete the remote branch.
            if source_repo
                .delete_remote_branch(&branch)
                .is_ok()
            {
                branches_deleted += 1;
            }

            // Close the issue if applicable.
            if let Some(closer) = closer {
                if is_issue_task(task) {
                    if let Some(number) = parse_issue_number(&task.name) {
                        let sha = record
                            .find_sha(&task.label())
                            .unwrap_or(None)
                            .unwrap_or_default();
                        let comment = if sha.is_empty() {
                            format!("Completed by combust task: {}", task.label())
                        } else {
                            format!("Completed in commit {} by combust", sha)
                        };
                        if let Err(e) = closer.close_issue(number, &comment) {
                            eprintln!("Warning: failed to close issue #{}: {}", number, e);
                        } else {
                            issues_closed += 1;
                        }
                    }
                }
            }
        }
    }

    Ok((branches_deleted, issues_closed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Fix the Login Bug"), "fix-the-login-bug");
        assert_eq!(slugify("Add Feature #42"), "add-feature-42");
        assert_eq!(slugify("  leading spaces  "), "leading-spaces");
    }

    #[test]
    fn test_parse_issue_number() {
        assert_eq!(parse_issue_number("42-fix-bug"), Some(42));
        assert_eq!(parse_issue_number("not-a-number"), None);
        assert_eq!(parse_issue_number("0-zero"), None);
        assert_eq!(parse_issue_number("123"), Some(123));
    }

    #[test]
    fn test_is_issue_task() {
        use combust_db::task::{Task, TaskState};
        use std::path::PathBuf;

        let task = Task {
            name: "42-fix-bug".to_string(),
            file_path: PathBuf::new(),
            group: "issues".to_string(),
            state: TaskState::Pending,
        };
        assert!(is_issue_task(&task));

        let task2 = Task {
            name: "my-task".to_string(),
            file_path: PathBuf::new(),
            group: "".to_string(),
            state: TaskState::Pending,
        };
        assert!(!is_issue_task(&task2));
    }

    #[test]
    fn test_format_issue_content() {
        let issue = Issue {
            number: 42,
            title: "Fix login".to_string(),
            body: "The login page is broken.".to_string(),
            labels: vec!["bug".to_string(), "urgent".to_string()],
            url: "https://github.com/org/repo/issues/42".to_string(),
        };
        let content = format_issue_content(&issue);
        assert!(content.contains("# Issue #42: Fix login"));
        assert!(content.contains("URL: https://github.com/org/repo/issues/42"));
        assert!(content.contains("Labels: bug, urgent"));
        assert!(content.contains("The login page is broken."));
    }

    // --- Mock Source/Closer for testing ---

    struct MockSource {
        issues: Vec<Issue>,
    }

    impl Source for MockSource {
        fn fetch_open_issues(&self, _labels: &[String]) -> Result<Vec<Issue>> {
            Ok(self.issues.clone())
        }
    }

    struct MockCloser {
        closed: std::sync::Mutex<Vec<(i64, String)>>,
    }

    impl MockCloser {
        fn new() -> Self {
            MockCloser {
                closed: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl Closer for MockCloser {
        fn close_issue(&self, number: i64, comment: &str) -> Result<()> {
            self.closed.lock().unwrap().push((number, comment.to_string()));
            Ok(())
        }
    }

    #[test]
    fn test_sync_creates_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let design_dir = tmp.path();

        let source = MockSource {
            issues: vec![
                Issue {
                    number: 1,
                    title: "First Issue".to_string(),
                    body: "Body 1".to_string(),
                    labels: vec![],
                    url: "https://example.com/1".to_string(),
                },
                Issue {
                    number: 2,
                    title: "Second Issue".to_string(),
                    body: "Body 2".to_string(),
                    labels: vec![],
                    url: "https://example.com/2".to_string(),
                },
            ],
        };

        let (created, skipped) = sync(design_dir, &source, &[]).unwrap();
        assert_eq!(created, 2);
        assert_eq!(skipped, 0);

        // Verify files exist.
        let issues_dir = design_dir.join("tasks/issues");
        assert!(issues_dir.join("group.md").exists());
        let entries: Vec<_> = std::fs::read_dir(&issues_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy() != "group.md")
            .collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_sync_skips_duplicates() {
        let tmp = tempfile::TempDir::new().unwrap();
        let design_dir = tmp.path();

        let source = MockSource {
            issues: vec![Issue {
                number: 1,
                title: "First Issue".to_string(),
                body: "Body 1".to_string(),
                labels: vec![],
                url: "https://example.com/1".to_string(),
            }],
        };

        // First sync creates the file.
        let (created, _) = sync(design_dir, &source, &[]).unwrap();
        assert_eq!(created, 1);

        // Second sync skips it.
        let (created, skipped) = sync(design_dir, &source, &[]).unwrap();
        assert_eq!(created, 0);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_sync_group_md_created() {
        let tmp = tempfile::TempDir::new().unwrap();
        let design_dir = tmp.path();

        let source = MockSource {
            issues: vec![Issue {
                number: 1,
                title: "Test".to_string(),
                body: "".to_string(),
                labels: vec![],
                url: "https://example.com/1".to_string(),
            }],
        };

        sync(design_dir, &source, &[]).unwrap();

        let group_md = design_dir.join("tasks/issues/group.md");
        assert!(group_md.exists());
        let content = std::fs::read_to_string(&group_md).unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_file_content_format() {
        let tmp = tempfile::TempDir::new().unwrap();
        let design_dir = tmp.path();

        let source = MockSource {
            issues: vec![Issue {
                number: 42,
                title: "Add Login Feature".to_string(),
                body: "We need login.".to_string(),
                labels: vec!["feature".to_string()],
                url: "https://example.com/42".to_string(),
            }],
        };

        sync(design_dir, &source, &[]).unwrap();

        // Find the created file.
        let issues_dir = design_dir.join("tasks/issues");
        let mut task_file = None;
        for entry in std::fs::read_dir(&issues_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("42-") && name.ends_with(".md") {
                task_file = Some(entry.path());
            }
        }
        let content = std::fs::read_to_string(task_file.unwrap()).unwrap();
        assert!(content.contains("# Issue #42: Add Login Feature"));
        assert!(content.contains("URL: https://example.com/42"));
        assert!(content.contains("Labels: feature"));
        assert!(content.contains("We need login."));
    }

    #[test]
    fn test_resolve_source_github() {
        let source = resolve_source("https://github.com/owner/repo", "", "");
        assert!(source.is_ok());
    }

    #[test]
    fn test_resolve_source_gitea() {
        let source = resolve_source("https://gitea.example.com/owner/repo", "gitea", "");
        assert!(source.is_ok());
    }

    #[test]
    fn test_resolve_source_explicit_type() {
        let source = resolve_source("https://github.com/owner/repo", "github", "");
        assert!(source.is_ok());
    }

    #[test]
    fn test_resolve_source_invalid() {
        let source = resolve_source("https://example.com/repo", "invalid_type", "");
        assert!(source.is_err());
    }

    #[test]
    fn test_close_issue_mock() {
        let closer = MockCloser::new();
        closer.close_issue(42, "Done!").unwrap();
        let closed = closer.closed.lock().unwrap();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].0, 42);
        assert_eq!(closed[0].1, "Done!");
    }

    #[test]
    fn test_close_issue_non_issue_task() {
        use combust_db::task::{Task, TaskState};

        let task = Task {
            name: "my-regular-task".to_string(),
            file_path: std::path::PathBuf::new(),
            group: "backend".to_string(),
            state: TaskState::Completed,
        };
        // Non-issue tasks should not have a parseable issue number.
        assert!(!is_issue_task(&task));
        assert!(parse_issue_number(&task.name).is_none());
    }
}
