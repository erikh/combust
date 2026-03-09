use anyhow::{bail, Context, Result};
use std::fs;

use super::document::PLAN_MODE_INSTRUCTION;
use super::{ClaudeRunConfig, Runner};
use crate::design::task::TaskState;

/// Holds a task name and its content for document assembly.
struct TaskEntry {
    name: String,
    content: String,
}

impl Runner {
    /// Reads all completed tasks, uses Claude to merge their requirements
    /// into functional.md, then deletes the completed task files.
    pub fn reconcile(&self) -> Result<()> {
        let base_dir = &self.base_dir;

        // Get all completed tasks.
        let completed = self
            .design
            .tasks_by_state(TaskState::Completed)
            .context("listing completed tasks")?;
        if completed.is_empty() {
            bail!("no completed tasks to reconcile");
        }

        // Read current functional.md.
        let functional = self.design.functional().context("reading functional.md")?;

        // Read all completed task contents.
        let mut task_contents = Vec::new();
        for task in &completed {
            let content = task.content().with_context(|| {
                format!("reading task {}", task.name)
            })?;
            task_contents.push(TaskEntry {
                name: task.label(),
                content,
            });
        }

        // Prepare work directory.
        let wd = base_dir
            .join(crate::config::COMBUST_DIR)
            .join("work")
            .join("_reconcile");
        let reconcile_repo = self
            .prepare_repo(&wd, "combust/_reconcile")
            .context("preparing work directory")?;

        // Fetch and reset to a clean state.
        reconcile_repo.fetch().context("fetching origin")?;
        let default_branch = self
            .detect_default_branch(&reconcile_repo)
            .context("detecting default branch")?;
        self.reset_worktree(&reconcile_repo, &format!("origin/{}", default_branch))
            .context("resetting work directory")?;

        // Copy current functional.md into the work directory for Claude to edit.
        let functional_path = wd.join("functional.md");
        fs::write(&functional_path, &functional)
            .context("writing functional.md to work dir")?;

        // Assemble the document.
        let doc = assemble_reconcile_document(&functional, &task_contents);

        // Run before hook.
        self.run_before_hook(&wd).context("before hook")?;

        // Invoke Claude.
        let cfg = ClaudeRunConfig {
            repo_dir: wd.clone(),
            document: doc,
            model: self.model.clone(),
            auto_accept: self.auto_accept,
            plan_mode: self.plan_mode,
            force_tui: self.force_tui,
        };
        self.invoke_claude(cfg).context("claude failed")?;

        // Read updated functional.md from work dir.
        let updated =
            fs::read_to_string(&functional_path).context("reading updated functional.md")?;

        // Copy back to design dir if changed.
        if updated != functional {
            let design_functional_path = self.design.path.join("functional.md");
            fs::write(&design_functional_path, &updated)
                .context("writing functional.md to design dir")?;
            println!("Updated functional.md with reconciled requirements.");
        } else {
            println!("functional.md unchanged.");
        }

        // Delete completed task files.
        for task in &completed {
            self.design
                .delete_task(task)
                .with_context(|| format!("deleting completed task {}", task.name))?;
        }

        println!("Deleted {} completed task(s).", completed.len());
        Ok(())
    }
}

/// Public wrapper for testing reconcile document assembly.
#[cfg(test)]
pub(crate) struct TaskEntryPublic {
    pub name: String,
    pub content: String,
}

#[cfg(test)]
pub(crate) fn assemble_reconcile_document_pub(functional: &str, tasks: &[TaskEntryPublic]) -> String {
    let entries: Vec<TaskEntry> = tasks
        .iter()
        .map(|t| TaskEntry {
            name: t.name.clone(),
            content: t.content.clone(),
        })
        .collect();
    assemble_reconcile_document(functional, &entries)
}

fn assemble_reconcile_document(functional: &str, tasks: &[TaskEntry]) -> String {
    let mut b = String::new();

    b.push_str(
        "# Mission\n\nYour sole objective is to update the functional specification \
         based on the completed tasks listed below. Do not make any other changes. \
         Do not modify any source code files. Only edit functional.md.\n\n",
    );

    b.push_str("# Current Functional Specification\n\n");
    if !functional.is_empty() {
        b.push_str(functional);
    } else {
        b.push_str("No existing specification.");
    }
    b.push_str("\n\n");

    b.push_str("# Completed Tasks\n\n");
    for t in tasks {
        b.push_str("## ");
        b.push_str(&t.name);
        b.push_str("\n\n");
        b.push_str(&t.content);
        b.push_str("\n\n");
    }

    b.push_str("# Instructions\n\n");
    b.push_str(
        "Read the codebase to understand what was actually implemented for each completed task. \
         Then update the file `functional.md` in the current directory. This file is the project's \
         living functional specification. Merge the requirements from the completed tasks above \
         into it, removing duplicates and organizing by feature area. The result should be a \
         concise, accurate description of what the software does — not a list of tasks, but a \
         specification of behaviors and capabilities. Use the actual code as ground truth.\n\n",
    );

    b.push_str(
        "Do not make any other changes. Do not modify any source code files. Only edit functional.md.\n",
    );

    b.push_str("\n# Reminder\n\n");
    b.push_str("Your ONLY job is to update functional.md. Do not make any other changes.\n");

    b.push_str(PLAN_MODE_INSTRUCTION);
    b
}
