use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::task::{Task, TaskState};

/// Default content for a new combust.yml.
pub const DEFAULT_COMBUST_YML: &str = r#"# Commands that Claude runs before committing.
#
# IMPORTANT: These commands may run concurrently across multiple combust tasks,
# each in its own work directory (cloned repo). Make sure your test and lint
# commands are safe to run in parallel without trampling each other. Avoid
# commands that write to shared global state, fixed file paths outside the
# work directory, or shared network ports. Each invocation should be fully
# isolated to its own working tree.
commands:
  # before: "make deps"
  # clean: "make clean"
  # dev: "npm run dev"
  # lint: "golangci-lint run ./..."
  # test: "go test ./... -count=1"
"#;

/// Mission preamble prepended to assembled documents.
pub const MISSION_PREAMBLE: &str = "# Mission\n\n\
Your sole objective is to implement the task described in the \"Task\" section below. \
Every action you take — reading files, writing code, running commands — must directly \
serve that task. Do not make changes unrelated to the task, do not refactor surrounding \
code, do not \"improve\" things you notice along the way. If the task says to add a \
feature, add exactly that feature. If it says to fix a bug, fix exactly that bug. \
Stay focused.\n\n";

/// Represents a design directory containing rules, lint, functional specs, and tasks.
#[derive(Debug, Clone)]
pub struct DesignDir {
    pub path: PathBuf,
    pub state_path: PathBuf,
}

impl DesignDir {
    /// Opens and validates a design directory at the given path.
    pub fn new(path: &Path, state_path: &Path) -> Result<Self> {
        let abs = fs::canonicalize(path)
            .or_else(|_| Ok::<PathBuf, std::io::Error>(path.to_path_buf()))
            .context("resolving design dir")?;

        if !abs.is_dir() {
            anyhow::bail!("{} is not a directory", abs.display());
        }

        let abs_state = fs::canonicalize(state_path)
            .or_else(|_| Ok::<PathBuf, std::io::Error>(state_path.to_path_buf()))
            .context("resolving state dir")?;

        Ok(DesignDir {
            path: abs,
            state_path: abs_state,
        })
    }

    fn read_file(&self, name: &str) -> Result<String> {
        let p = self.path.join(name);
        match fs::read_to_string(&p) {
            Ok(content) => Ok(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e).with_context(|| format!("reading {}", name)),
        }
    }

    /// Returns the content of rules.md, or empty string if it doesn't exist.
    pub fn rules(&self) -> Result<String> {
        self.read_file("rules.md")
    }

    /// Returns the content of lint.md, or empty string if it doesn't exist.
    pub fn lint(&self) -> Result<String> {
        self.read_file("lint.md")
    }

    /// Returns the content of functional.md, or empty string if it doesn't exist.
    pub fn functional(&self) -> Result<String> {
        self.read_file("functional.md")
    }

    /// Returns the content of the group heading file (tasks/{group}/group.md).
    pub fn group_content(&self, group: &str) -> Result<String> {
        if group.is_empty() {
            return Ok(String::new());
        }
        self.read_file(&format!("tasks/{}/group.md", group))
    }

    /// Returns all tasks in the tasks/ directory (pending state).
    pub fn pending_tasks(&self) -> Result<Vec<Task>> {
        self.discover_tasks(&self.path.join("tasks"), "", TaskState::Pending)
    }

    /// Returns all tasks in the given state.
    pub fn tasks_by_state(&self, state: TaskState) -> Result<Vec<Task>> {
        match state {
            TaskState::Pending => self.pending_tasks(),
            _ => self.discover_tasks(
                &self.state_path.join(state.as_str()),
                "",
                state,
            ),
        }
    }

    /// Returns tasks across all states.
    pub fn all_tasks(&self) -> Result<Vec<Task>> {
        let mut all = Vec::new();
        for state in TaskState::all() {
            all.extend(self.tasks_by_state(*state)?);
        }
        Ok(all)
    }

    /// Looks up a pending task by name or group/name.
    pub fn find_task(&self, name: &str) -> Result<Task> {
        let tasks = self.pending_tasks()?;
        find_in_tasks(&tasks, name)
            .ok_or_else(|| anyhow::anyhow!("task {:?} not found in pending tasks", name))
    }

    /// Looks up a task by name in the given state.
    pub fn find_task_by_state(&self, name: &str, state: TaskState) -> Result<Task> {
        let tasks = self.tasks_by_state(state)?;
        find_in_tasks(&tasks, name)
            .ok_or_else(|| anyhow::anyhow!("task {:?} not found in {} state", name, state.as_str()))
    }

    /// Looks up a task by name across all states.
    pub fn find_task_any(&self, name: &str) -> Result<Task> {
        let tasks = self.all_tasks()?;
        find_in_tasks(&tasks, name)
            .ok_or_else(|| anyhow::anyhow!("task {:?} not found in any state", name))
    }

    /// Moves a task file to the given state directory.
    pub fn move_task(&self, task: &mut Task, new_state: TaskState) -> Result<()> {
        let dest_dir = match new_state {
            TaskState::Pending => anyhow::bail!("cannot move task to pending state"),
            _ => {
                let mut d = self.state_path.join(new_state.as_str());
                if !task.group.is_empty() {
                    d = d.join(&task.group);
                }
                d
            }
        };

        fs::create_dir_all(&dest_dir).context("creating state directory")?;

        let file_name = task
            .file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let dest_path = dest_dir.join(&file_name);
        // Use rename when possible, but fall back to copy+remove for cross-device moves
        // (rename fails with EXDEV when source and destination are on different filesystems).
        if fs::rename(&task.file_path, &dest_path).is_err() {
            fs::copy(&task.file_path, &dest_path).context("copying task file (cross-device)")?;
            fs::remove_file(&task.file_path).context("removing original task file")?;
        }

        task.file_path = dest_path;
        task.state = new_state;
        Ok(())
    }

    /// Removes a task file from disk.
    pub fn delete_task(&self, task: &Task) -> Result<()> {
        fs::remove_file(&task.file_path)
            .with_context(|| format!("deleting task {}", task.name))?;
        Ok(())
    }

    /// Builds a single markdown document from rules, lint, group heading, task content, and functional specs.
    pub fn assemble_document(&self, task_content: &str, group_content: &str) -> Result<String> {
        let rules = self.rules()?;
        let lint = self.lint()?;
        let functional = self.functional()?;

        let mut doc = MISSION_PREAMBLE.to_string();
        if !rules.is_empty() {
            doc += &format!("# Rules\n\n{}\n\n", rules);
        }
        if !lint.is_empty() {
            doc += &format!("# Lint Rules\n\n{}\n\n", lint);
        }
        if !group_content.is_empty() {
            doc += &format!("# Group\n\n{}\n\n", group_content);
        }
        doc += &format!("# Task\n\n{}\n\n", task_content);
        if !functional.is_empty() {
            doc += &format!("# Functional Tests\n\n{}\n\n", functional);
        }

        Ok(doc)
    }

    /// Lists files in the other/ directory.
    pub fn other_files(&self) -> Result<Vec<String>> {
        let other_dir = self.path.join("other");
        if !other_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&other_dir).context("reading other/ directory")? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                files.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        files.sort();
        Ok(files)
    }

    /// Returns the content of a file in other/.
    pub fn other_content(&self, name: &str) -> Result<String> {
        validate_other_filename(name)?;
        let path = self.path.join("other").join(name);
        if !path.exists() {
            bail!("other file {:?} not found", name);
        }
        fs::read_to_string(&path).with_context(|| format!("reading other/{}", name))
    }

    /// Removes a file from other/.
    pub fn remove_other_file(&self, name: &str) -> Result<()> {
        validate_other_filename(name)?;
        let path = self.path.join("other").join(name);
        if !path.exists() {
            bail!("other file {:?} not found", name);
        }
        fs::remove_file(&path).with_context(|| format!("removing other/{}", name))
    }

    /// Adds a file to other/ with the given content.
    pub fn add_other_file(&self, name: &str, content: &str) -> Result<()> {
        validate_other_filename(name)?;
        let other_dir = self.path.join("other");
        fs::create_dir_all(&other_dir).context("creating other/ directory")?;
        let path = other_dir.join(name);
        if path.exists() {
            bail!("other file {:?} already exists", name);
        }
        fs::write(&path, content).with_context(|| format!("writing other/{}", name))
    }

    /// Returns a list of group names from the tasks/ directory.
    pub fn groups(&self) -> Result<Vec<String>> {
        let tasks_dir = self.path.join("tasks");
        let mut groups = Vec::new();
        let entries = match fs::read_dir(&tasks_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(groups),
            Err(e) => return Err(e).context("reading tasks directory"),
        };

        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                groups.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        groups.sort();
        Ok(groups)
    }

    /// Returns all tasks in a given group (pending state).
    pub fn group_tasks(&self, group: &str) -> Result<Vec<Task>> {
        let group_dir = self.path.join("tasks").join(group);
        self.discover_tasks(&group_dir, group, TaskState::Pending)
    }

    fn discover_tasks(&self, dir: &Path, group: &str, state: TaskState) -> Result<Vec<Task>> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("reading directory {}", dir.display()));
            }
        };

        let mut tasks = Vec::new();
        let mut dirs_to_recurse = Vec::new();

        for entry in entries {
            let entry = entry?;
            let ft = entry.file_type()?;

            if ft.is_dir() {
                dirs_to_recurse.push(entry);
                continue;
            }

            let name_os = entry.file_name();
            let name = name_os.to_string_lossy();

            if !name.ends_with(".md") {
                continue;
            }
            if name == "group.md" {
                continue;
            }

            let task_name = name.trim_end_matches(".md").to_string();
            tasks.push(Task {
                name: task_name,
                file_path: entry.path(),
                group: group.to_string(),
                state,
            });
        }

        for dir_entry in dirs_to_recurse {
            let sub_group = dir_entry.file_name().to_string_lossy().to_string();
            let sub_tasks = self.discover_tasks(&dir_entry.path(), &sub_group, state)?;
            tasks.extend(sub_tasks);
        }

        Ok(tasks)
    }
}

/// Creates hydra.yml (combust.yml) with placeholder content if it does not exist.
pub fn ensure_combust_yml(path: &Path) -> Result<()> {
    let p = path.join("combust.yml");
    if p.exists() {
        return Ok(());
    }
    fs::write(&p, DEFAULT_COMBUST_YML).context("creating combust.yml")?;
    Ok(())
}

/// Creates the design directory skeleton tree at the given path.
pub fn scaffold_design(path: &Path) -> Result<()> {
    // If rules.md already exists, assume scaffolded; just ensure combust.yml.
    if path.join("rules.md").exists() {
        return ensure_combust_yml(path);
    }

    let dirs = [
        "tasks",
        "other",
        "milestone/history",
        "milestone/delivered",
    ];

    for d in &dirs {
        fs::create_dir_all(path.join(d))
            .with_context(|| format!("creating directory {}", d))?;
    }

    let placeholders: &[(&str, &str)] = &[
        ("rules.md", ""),
        ("lint.md", ""),
        ("functional.md", ""),
        ("combust.yml", DEFAULT_COMBUST_YML),
    ];

    for (name, content) in placeholders {
        let p = path.join(name);
        fs::write(&p, content)
            .with_context(|| format!("creating {}", name))?;
    }

    Ok(())
}

/// Creates the state directory skeleton tree at the given path.
pub fn scaffold_state(state_path: &Path) -> Result<()> {
    let dirs = [
        "review",
        "merge",
        "completed",
        "abandoned",
    ];

    for d in &dirs {
        fs::create_dir_all(state_path.join(d))
            .with_context(|| format!("creating state directory {}", d))?;
    }

    let record_path = state_path.join("record.json");
    if !record_path.exists() {
        fs::write(&record_path, "[]\n")
            .context("creating record.json")?;
    }

    Ok(())
}

/// Creates the full design + state directory skeleton tree at the given path.
/// This is a convenience wrapper for backward compatibility.
pub fn scaffold(path: &Path) -> Result<()> {
    scaffold_design(path)?;
    scaffold_state(&path.join("state"))?;
    Ok(())
}

/// Validates an other/ file name to prevent path traversal.
fn validate_other_filename(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("file name cannot be empty");
    }
    if name.contains('/') || name.contains("..") {
        bail!("invalid file name: {:?}", name);
    }
    Ok(())
}

fn find_in_tasks(tasks: &[Task], name: &str) -> Option<Task> {
    for t in tasks {
        if t.name == name {
            return Some(t.clone());
        }
        if !t.group.is_empty() && format!("{}/{}", t.group, t.name) == name {
            return Some(t.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_design_dir() -> (TempDir, DesignDir) {
        let tmp = TempDir::new().unwrap();
        scaffold(tmp.path()).unwrap();
        let state_path = tmp.path().join("state");
        let dir = DesignDir::new(tmp.path(), &state_path).unwrap();
        (tmp, dir)
    }

    #[test]
    fn test_scaffold_creates_structure() {
        let tmp = TempDir::new().unwrap();
        scaffold(tmp.path()).unwrap();

        assert!(tmp.path().join("tasks").is_dir());
        assert!(tmp.path().join("other").is_dir());
        assert!(tmp.path().join("state/review").is_dir());
        assert!(tmp.path().join("state/merge").is_dir());
        assert!(tmp.path().join("state/completed").is_dir());
        assert!(tmp.path().join("state/abandoned").is_dir());
        assert!(tmp.path().join("milestone/history").is_dir());
        assert!(tmp.path().join("milestone/delivered").is_dir());
        assert!(tmp.path().join("rules.md").is_file());
        assert!(tmp.path().join("lint.md").is_file());
        assert!(tmp.path().join("functional.md").is_file());
        assert!(tmp.path().join("combust.yml").is_file());
        assert!(tmp.path().join("state/record.json").is_file());
    }

    #[test]
    fn test_scaffold_design_only() {
        let tmp = TempDir::new().unwrap();
        scaffold_design(tmp.path()).unwrap();

        assert!(tmp.path().join("tasks").is_dir());
        assert!(tmp.path().join("other").is_dir());
        assert!(tmp.path().join("milestone/history").is_dir());
        assert!(tmp.path().join("milestone/delivered").is_dir());
        assert!(tmp.path().join("rules.md").is_file());
        assert!(tmp.path().join("lint.md").is_file());
        assert!(tmp.path().join("functional.md").is_file());
        assert!(tmp.path().join("combust.yml").is_file());
        // State dirs should NOT exist
        assert!(!tmp.path().join("state").exists());
    }

    #[test]
    fn test_scaffold_state_only() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("state");
        scaffold_state(&state_dir).unwrap();

        assert!(state_dir.join("review").is_dir());
        assert!(state_dir.join("merge").is_dir());
        assert!(state_dir.join("completed").is_dir());
        assert!(state_dir.join("abandoned").is_dir());
        assert!(state_dir.join("record.json").is_file());
        // Design dirs should NOT exist
        assert!(!tmp.path().join("tasks").exists());
        assert!(!tmp.path().join("rules.md").exists());
    }

    #[test]
    fn test_dir_with_separate_state_path() {
        let tmp = TempDir::new().unwrap();
        let design_dir = tmp.path().join("design");
        let state_dir = tmp.path().join("state");
        scaffold_design(&design_dir).unwrap();
        scaffold_state(&state_dir).unwrap();

        let dir = DesignDir::new(&design_dir, &state_dir).unwrap();

        // Create a pending task
        fs::write(dir.path.join("tasks/my-task.md"), "Content").unwrap();
        let pending = dir.pending_tasks().unwrap();
        assert_eq!(pending.len(), 1);

        // Move task to review (uses state_path)
        let mut task = dir.find_task("my-task").unwrap();
        dir.move_task(&mut task, TaskState::Review).unwrap();
        assert!(state_dir.join("review/my-task.md").exists());
        assert!(!design_dir.join("tasks/my-task.md").exists());

        // Find task by state (uses state_path)
        let review_tasks = dir.tasks_by_state(TaskState::Review).unwrap();
        assert_eq!(review_tasks.len(), 1);
    }

    #[test]
    fn test_scaffold_skips_existing() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path()).unwrap();
        fs::write(tmp.path().join("rules.md"), "Custom rules").unwrap();

        scaffold(tmp.path()).unwrap();

        // rules.md should be untouched.
        let content = fs::read_to_string(tmp.path().join("rules.md")).unwrap();
        assert_eq!(content, "Custom rules");
        // But combust.yml should be created.
        assert!(tmp.path().join("combust.yml").is_file());
    }

    #[test]
    fn test_new_dir() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        let dir = DesignDir::new(tmp.path(), &state_dir);
        assert!(dir.is_ok());
    }

    #[test]
    fn test_new_dir_not_exist() {
        let result = DesignDir::new(
            std::path::Path::new("/nonexistent/path/that/does/not/exist"),
            std::path::Path::new("/nonexistent/state"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_new_dir_is_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("afile");
        fs::write(&file_path, "content").unwrap();
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        let result = DesignDir::new(&file_path, &state_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_rules() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("rules.md"), "No panics allowed.").unwrap();
        assert_eq!(dir.rules().unwrap(), "No panics allowed.");
    }

    #[test]
    fn test_lint() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("lint.md"), "Use clippy.").unwrap();
        assert_eq!(dir.lint().unwrap(), "Use clippy.");
    }

    #[test]
    fn test_functional() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("functional.md"), "Must support login.").unwrap();
        assert_eq!(dir.functional().unwrap(), "Must support login.");
    }

    #[test]
    fn test_missing_optional_files() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        // Don't scaffold - just create the bare directory.
        let dir = DesignDir::new(tmp.path(), &state_dir).unwrap();
        assert_eq!(dir.rules().unwrap(), "");
        assert_eq!(dir.lint().unwrap(), "");
        assert_eq!(dir.functional().unwrap(), "");
    }

    #[test]
    fn test_assemble_document_full() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("rules.md"), "Rule content").unwrap();
        fs::write(dir.path.join("lint.md"), "Lint content").unwrap();
        fs::write(dir.path.join("functional.md"), "Functional content").unwrap();

        let doc = dir.assemble_document("Task content", "").unwrap();

        assert!(doc.contains("# Mission"));
        assert!(doc.contains("# Rules\n\nRule content"));
        assert!(doc.contains("# Lint Rules\n\nLint content"));
        assert!(doc.contains("# Task\n\nTask content"));
        assert!(doc.contains("# Functional Tests\n\nFunctional content"));

        // Check order: Rules before Lint before Task before Functional.
        let rules_pos = doc.find("# Rules").unwrap();
        let lint_pos = doc.find("# Lint Rules").unwrap();
        let task_pos = doc.find("# Task").unwrap();
        let func_pos = doc.find("# Functional Tests").unwrap();
        assert!(rules_pos < lint_pos);
        assert!(lint_pos < task_pos);
        assert!(task_pos < func_pos);
    }

    #[test]
    fn test_assemble_document_minimal() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        let dir = DesignDir::new(tmp.path(), &state_dir).unwrap();
        // No rules.md, lint.md, or functional.md exist.
        let doc = dir.assemble_document("Do the thing", "").unwrap();

        assert!(doc.contains("# Task\n\nDo the thing"));
        assert!(!doc.contains("# Rules"));
        assert!(!doc.contains("# Lint Rules"));
        assert!(!doc.contains("# Functional Tests"));
    }

    #[test]
    fn test_assemble_document_with_group() {
        let (_tmp, dir) = setup_design_dir();
        let doc = dir.assemble_document("Task content", "Group heading").unwrap();
        assert!(doc.contains("# Group\n\nGroup heading"));
    }

    #[test]
    fn test_assemble_document_without_group() {
        let (_tmp, dir) = setup_design_dir();
        let doc = dir.assemble_document("Task content", "").unwrap();
        assert!(!doc.contains("# Group"));
    }

    #[test]
    fn test_pending_tasks() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("tasks/task-a.md"), "A").unwrap();
        fs::write(dir.path.join("tasks/task-b.md"), "B").unwrap();

        // Also create a grouped task.
        fs::create_dir_all(dir.path.join("tasks/mygroup")).unwrap();
        fs::write(dir.path.join("tasks/mygroup/task-c.md"), "C").unwrap();

        let tasks = dir.pending_tasks().unwrap();
        assert_eq!(tasks.len(), 3);

        let names: Vec<&str> = tasks.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"task-a"));
        assert!(names.contains(&"task-b"));
        assert!(names.contains(&"task-c"));

        let grouped = tasks.iter().find(|t| t.name == "task-c").unwrap();
        assert_eq!(grouped.group, "mygroup");
    }

    #[test]
    fn test_pending_tasks_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        // No tasks/ dir at all.
        let dir = DesignDir::new(tmp.path(), &state_dir).unwrap();
        let tasks = dir.pending_tasks().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_pending_tasks_skips_group_md() {
        let (_tmp, dir) = setup_design_dir();
        fs::create_dir_all(dir.path.join("tasks/mygroup")).unwrap();
        fs::write(dir.path.join("tasks/mygroup/group.md"), "Group heading").unwrap();
        fs::write(dir.path.join("tasks/mygroup/real-task.md"), "Content").unwrap();

        let tasks = dir.pending_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "real-task");
    }

    #[test]
    fn test_tasks_by_state() {
        let (_tmp, dir) = setup_design_dir();

        // Create tasks in review state.
        fs::create_dir_all(dir.path.join("state/review")).unwrap();
        fs::write(dir.path.join("state/review/reviewed.md"), "Content").unwrap();

        let tasks = dir.tasks_by_state(TaskState::Review).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "reviewed");
        assert_eq!(tasks[0].state, TaskState::Review);
    }

    #[test]
    fn test_all_tasks() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("tasks/pending-task.md"), "Pending").unwrap();
        fs::create_dir_all(dir.path.join("state/completed")).unwrap();
        fs::write(dir.path.join("state/completed/done-task.md"), "Done").unwrap();

        let all = dir.all_tasks().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_find_task() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("tasks/my-task.md"), "Content").unwrap();

        let task = dir.find_task("my-task").unwrap();
        assert_eq!(task.name, "my-task");
    }

    #[test]
    fn test_find_task_grouped() {
        let (_tmp, dir) = setup_design_dir();
        fs::create_dir_all(dir.path.join("tasks/grp")).unwrap();
        fs::write(dir.path.join("tasks/grp/sub-task.md"), "Content").unwrap();

        let task = dir.find_task("grp/sub-task").unwrap();
        assert_eq!(task.name, "sub-task");
        assert_eq!(task.group, "grp");
    }

    #[test]
    fn test_find_task_not_found() {
        let (_tmp, dir) = setup_design_dir();
        let result = dir.find_task("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_task_by_state() {
        let (_tmp, dir) = setup_design_dir();
        fs::create_dir_all(dir.path.join("state/review")).unwrap();
        fs::write(dir.path.join("state/review/in-review.md"), "Content").unwrap();

        let task = dir.find_task_by_state("in-review", TaskState::Review).unwrap();
        assert_eq!(task.name, "in-review");
        assert_eq!(task.state, TaskState::Review);
    }

    #[test]
    fn test_find_task_any() {
        let (_tmp, dir) = setup_design_dir();
        fs::create_dir_all(dir.path.join("state/completed")).unwrap();
        fs::write(dir.path.join("state/completed/done.md"), "Content").unwrap();

        let task = dir.find_task_any("done").unwrap();
        assert_eq!(task.name, "done");
        assert_eq!(task.state, TaskState::Completed);
    }

    #[test]
    fn test_move_task() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("tasks/moveme.md"), "Move me").unwrap();

        let mut task = dir.find_task("moveme").unwrap();
        assert_eq!(task.state, TaskState::Pending);

        dir.move_task(&mut task, TaskState::Review).unwrap();
        assert_eq!(task.state, TaskState::Review);
        assert!(task.file_path.exists());
        assert!(!dir.path.join("tasks/moveme.md").exists());
    }

    #[test]
    fn test_move_task_all_states() {
        for state in &[TaskState::Review, TaskState::Merge, TaskState::Completed, TaskState::Abandoned] {
            let (_tmp, dir) = setup_design_dir();
            fs::write(dir.path.join("tasks/t.md"), "Task").unwrap();
            let mut task = dir.find_task("t").unwrap();
            dir.move_task(&mut task, *state).unwrap();
            assert_eq!(task.state, *state);
            assert!(task.file_path.exists());
        }
    }

    #[test]
    fn test_move_task_group_preserved() {
        let (_tmp, dir) = setup_design_dir();
        fs::create_dir_all(dir.path.join("tasks/mygrp")).unwrap();
        fs::write(dir.path.join("tasks/mygrp/grouped.md"), "Content").unwrap();

        let mut task = dir.find_task("mygrp/grouped").unwrap();
        dir.move_task(&mut task, TaskState::Review).unwrap();

        assert_eq!(task.group, "mygrp");
        assert!(task.file_path.to_string_lossy().contains("mygrp"));
    }

    #[test]
    fn test_move_task_cross_device_fallback() {
        // Use separate TempDirs for design and state to exercise the copy+remove
        // fallback path (mirrors real XDG layout where state may be on a different device).
        let design_tmp = TempDir::new().unwrap();
        let state_tmp = TempDir::new().unwrap();

        scaffold_design(design_tmp.path()).unwrap();
        scaffold_state(state_tmp.path()).unwrap();

        let dir = DesignDir::new(design_tmp.path(), state_tmp.path()).unwrap();

        let task_content = "Cross-device task content";
        fs::write(dir.path.join("tasks/xdev-task.md"), task_content).unwrap();

        let mut task = dir.find_task("xdev-task").unwrap();
        dir.move_task(&mut task, TaskState::Review).unwrap();

        // Task should be in the new location with content preserved.
        assert_eq!(task.state, TaskState::Review);
        assert!(task.file_path.exists());
        assert!(!design_tmp.path().join("tasks/xdev-task.md").exists());
        assert!(state_tmp.path().join("review/xdev-task.md").exists());

        let moved_content = fs::read_to_string(&task.file_path).unwrap();
        assert_eq!(moved_content, task_content);
    }

    #[test]
    fn test_move_task_cross_device_grouped() {
        let design_tmp = TempDir::new().unwrap();
        let state_tmp = TempDir::new().unwrap();

        scaffold_design(design_tmp.path()).unwrap();
        scaffold_state(state_tmp.path()).unwrap();

        let dir = DesignDir::new(design_tmp.path(), state_tmp.path()).unwrap();

        fs::create_dir_all(dir.path.join("tasks/mygrp")).unwrap();
        fs::write(dir.path.join("tasks/mygrp/grouped.md"), "Grouped content").unwrap();

        let mut task = dir.find_task("mygrp/grouped").unwrap();
        dir.move_task(&mut task, TaskState::Review).unwrap();

        assert_eq!(task.state, TaskState::Review);
        assert_eq!(task.group, "mygrp");
        assert!(state_tmp.path().join("review/mygrp/grouped.md").exists());
        assert!(!design_tmp.path().join("tasks/mygrp/grouped.md").exists());

        let content = fs::read_to_string(&task.file_path).unwrap();
        assert_eq!(content, "Grouped content");
    }

    #[test]
    fn test_move_task_to_pending_rejected() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("tasks/t.md"), "Task").unwrap();
        let mut task = dir.find_task("t").unwrap();
        let result = dir.move_task(&mut task, TaskState::Pending);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot move task to pending"));
    }

    #[test]
    fn test_delete_task() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("tasks/deleteme.md"), "Delete me").unwrap();

        let task = dir.find_task("deleteme").unwrap();
        assert!(task.file_path.exists());

        dir.delete_task(&task).unwrap();
        assert!(!task.file_path.exists());
    }

    #[test]
    fn test_non_md_files_ignored() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("tasks/real-task.md"), "Content").unwrap();
        fs::write(dir.path.join("tasks/not-a-task.txt"), "Ignored").unwrap();
        fs::write(dir.path.join("tasks/also-not.json"), "{}").unwrap();

        let tasks = dir.pending_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "real-task");
    }

    #[test]
    fn test_other_files() {
        let (_tmp, dir) = setup_design_dir();
        fs::write(dir.path.join("other/notes.txt"), "Notes").unwrap();
        fs::write(dir.path.join("other/diagram.png"), "PNG").unwrap();

        let files = dir.other_files().unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"notes.txt".to_string()));
        assert!(files.contains(&"diagram.png".to_string()));
    }

    #[test]
    fn test_other_files_empty() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        let dir = DesignDir::new(tmp.path(), &state_dir).unwrap();
        let files = dir.other_files().unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_group_content() {
        let (_tmp, dir) = setup_design_dir();
        fs::create_dir_all(dir.path.join("tasks/mygroup")).unwrap();
        fs::write(dir.path.join("tasks/mygroup/group.md"), "Group docs").unwrap();

        let content = dir.group_content("mygroup").unwrap();
        assert_eq!(content, "Group docs");
    }

    #[test]
    fn test_group_content_missing() {
        let (_tmp, dir) = setup_design_dir();
        let content = dir.group_content("nonexistent").unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_group_content_empty_group() {
        let (_tmp, dir) = setup_design_dir();
        let content = dir.group_content("").unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_ensure_combust_yml_creates() {
        let tmp = TempDir::new().unwrap();
        assert!(!tmp.path().join("combust.yml").exists());
        ensure_combust_yml(tmp.path()).unwrap();
        assert!(tmp.path().join("combust.yml").is_file());
    }

    #[test]
    fn test_ensure_combust_yml_preserves() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("combust.yml"), "custom: true").unwrap();
        ensure_combust_yml(tmp.path()).unwrap();
        let content = fs::read_to_string(tmp.path().join("combust.yml")).unwrap();
        assert_eq!(content, "custom: true");
    }

    #[test]
    fn test_other_files_no_dir() {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&state_dir).unwrap();
        let dir = DesignDir::new(tmp.path(), &state_dir).unwrap();
        // No other/ directory exists — should return empty.
        let files = dir.other_files().unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_other_content_not_found() {
        let (_tmp, dir) = setup_design_dir();
        let result = dir.other_content("nonexistent.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_other_file_not_found() {
        let (_tmp, dir) = setup_design_dir();
        let result = dir.remove_other_file("nonexistent.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_other_file_validation_path_traversal() {
        let (_tmp, dir) = setup_design_dir();
        // Path traversal should be rejected.
        assert!(dir.other_content("../secret.txt").is_err());
        assert!(dir.other_content("sub/file.txt").is_err());
        assert!(dir.other_content("").is_err());
        assert!(dir.add_other_file("../escape.txt", "data").is_err());
        assert!(dir.remove_other_file("..%2F..%2Fetc").is_err());
    }

    #[test]
    fn test_add_other_file() {
        let (_tmp, dir) = setup_design_dir();
        dir.add_other_file("notes.txt", "some notes").unwrap();
        let content = dir.other_content("notes.txt").unwrap();
        assert_eq!(content, "some notes");

        let files = dir.other_files().unwrap();
        assert!(files.contains(&"notes.txt".to_string()));
    }

    #[test]
    fn test_add_other_file_duplicate() {
        let (_tmp, dir) = setup_design_dir();
        dir.add_other_file("notes.txt", "some notes").unwrap();
        let result = dir.add_other_file("notes.txt", "different");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_other_file() {
        let (_tmp, dir) = setup_design_dir();
        dir.add_other_file("to-remove.txt", "content").unwrap();
        assert!(dir.other_content("to-remove.txt").is_ok());

        dir.remove_other_file("to-remove.txt").unwrap();
        assert!(dir.other_content("to-remove.txt").is_err());
    }
}
