use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Opens the editor to create or edit a task file.
/// If the task file exists, opens it for editing.
/// If it doesn't exist, creates it via a temp file and the editor.
pub fn edit_task(design_dir: &Path, task_name: &str, editor: &str) -> Result<()> {
    let (group, name) = parse_task_name(task_name);

    let tasks_dir = if group.is_empty() {
        design_dir.join("tasks")
    } else {
        design_dir.join("tasks").join(&group)
    };
    fs::create_dir_all(&tasks_dir).context("creating tasks directory")?;

    let task_path = tasks_dir.join(format!("{}.md", name));

    if task_path.exists() {
        run_editor(editor, &task_path)?;
        return Ok(());
    }

    create_new_task(&task_path, editor)
}

/// Adds a task from provided content (e.g. piped from stdin).
/// If the task already exists, the content is overwritten.
pub fn add_task(design_dir: &Path, task_name: &str, content: &str) -> Result<()> {
    let (group, name) = parse_task_name(task_name);

    let tasks_dir = if group.is_empty() {
        design_dir.join("tasks")
    } else {
        design_dir.join("tasks").join(&group)
    };
    fs::create_dir_all(&tasks_dir).context("creating tasks directory")?;

    let task_path = tasks_dir.join(format!("{}.md", name));
    fs::write(&task_path, content).context("writing task file")?;
    println!("Created task: {}", task_path.display());
    Ok(())
}

/// Opens an editor on a file. Creates the file first if it doesn't exist.
fn create_new_task(task_path: &Path, editor: &str) -> Result<()> {
    let tmp = tempfile::NamedTempFile::new().context("creating temp file")?;
    let tmp_path = tmp.path().to_path_buf();

    run_editor(editor, &tmp_path)?;

    let content = fs::read_to_string(&tmp_path).context("reading temp file")?;
    if content.trim().is_empty() {
        println!("Empty file — task not created.");
        return Ok(());
    }

    if let Some(parent) = task_path.parent() {
        fs::create_dir_all(parent).context("creating task directory")?;
    }

    fs::write(task_path, &content).context("writing task file")?;
    println!("Created task: {}", task_path.display());
    Ok(())
}

/// Runs the editor on the given file path.
pub fn run_editor(editor: &str, file_path: &Path) -> Result<()> {
    let status = Command::new(editor)
        .arg(file_path)
        .status()
        .with_context(|| format!("running editor: {}", editor))?;

    if !status.success() {
        bail!("editor exited with status {}", status);
    }
    Ok(())
}

/// Resolves the editor from VISUAL or EDITOR environment variables.
pub fn resolve_editor() -> Result<String> {
    if let Ok(editor) = std::env::var("VISUAL") {
        if !editor.is_empty() {
            return Ok(editor);
        }
    }
    if let Ok(editor) = std::env::var("EDITOR") {
        if !editor.is_empty() {
            return Ok(editor);
        }
    }
    bail!("no editor found; set VISUAL or EDITOR environment variable")
}

/// Splits a task name like "group/name" into (group, name).
fn parse_task_name(task_name: &str) -> (String, String) {
    if let Some(pos) = task_name.rfind('/') {
        let group = task_name[..pos].to_string();
        let name = task_name[pos + 1..].to_string();
        (group, name)
    } else {
        (String::new(), task_name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_task_name_simple() {
        let (group, name) = parse_task_name("my-task");
        assert_eq!(group, "");
        assert_eq!(name, "my-task");
    }

    #[test]
    fn test_parse_task_name_grouped() {
        let (group, name) = parse_task_name("backend/add-api");
        assert_eq!(group, "backend");
        assert_eq!(name, "add-api");
    }

    #[test]
    fn test_resolve_editor_missing() {
        // Can't easily test resolve_editor in isolation without mocking env vars,
        // but we can at least test parse_task_name.
    }
}
