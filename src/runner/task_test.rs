use anyhow::{Context, Result};

use super::document::{
    commit_instructions, conflict_resolution_section, mission_reminder,
    notification_section, rebase_and_push_section, timeout_section,
    verification_section, PLAN_MODE_INSTRUCTION,
};
use super::{ClaudeRunConfig, Runner};
use crate::design::task::TaskState;
use crate::lock::Lock;

impl Runner {
    /// Test-focused session: adds tests for a task in review state.
    pub fn test_task(&self, task_name: &str) -> Result<()> {
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)
            .context("finding task in review")?;

        let branch = task.branch_name();
        let work_dir = self.config.work_dir().join(
            task.label().replace('/', "--"),
        );

        // Acquire lock.
        let lock = Lock::new(&self.config.base_dir.join(crate::config::COMBUST_DIR), &task.label());
        lock.acquire()
            .with_context(|| format!("acquiring lock for task {:?}", task.label()))?;

        // Prepare work directory.
        let repo = self
            .prepare_repo(&work_dir, &branch)
            .context("preparing work directory")?;

        // Rebase onto default branch if requested.
        let conflict_files = if self.rebase {
            repo.fetch().context("fetching origin")?;
            let default_branch = self
                .detect_default_branch(&repo)
                .context("detecting default branch")?;
            if repo.rebase(&format!("origin/{}", default_branch)).is_err() {
                repo.conflict_files().unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Run before hook.
        self.run_before_hook(&work_dir).context("before hook")?;

        // Assemble test document.
        let sign = repo.has_signing_key();
        let cmds = self.commands_map(&work_dir);
        let doc = assemble_test_document(
            &self.design, &task, sign, &cmds, &conflict_files,
            self.notify, self.timeout.as_ref(),
        )?;

        // Invoke Claude.
        let cfg = ClaudeRunConfig {
            repo_dir: work_dir.clone(),
            document: doc,
            model: self.model.clone(),
            auto_accept: self.auto_accept,
            plan_mode: self.plan_mode,
            force_tui: self.force_tui,
        };
        self.invoke_claude(cfg).context("claude failed")?;

        // Push.
        repo.push(&branch).context("pushing branch")?;

        // Run teardown.
        if let Some(ref cmds) = self.commands {
            cmds.run_teardown(&work_dir);
        }

        if self.notify {
            let _ = crate::notify::send(
                "combust",
                &format!("Test session for {} completed", task.label()),
            );
        }

        println!("Test session for {} completed and pushed.", task.label());
        lock.release().context("releasing lock")?;
        Ok(())
    }
}

/// Builds the prompt for the test workflow.
fn assemble_test_document(
    design: &crate::design::Dir,
    task: &crate::design::task::Task,
    sign: bool,
    cmds: &std::collections::HashMap<String, String>,
    conflict_files: &[String],
    notify: bool,
    timeout: Option<&std::time::Duration>,
) -> Result<String> {
    let task_content = task.content()?;
    let group_content = design.group_content(&task.group)?;
    let base_doc = design.assemble_document(&task_content, &group_content)?;

    let mut doc = String::new();
    doc.push_str("# Mission\n\n");
    doc.push_str(
        "Your sole objective is to add comprehensive tests for the code changes \
         on this branch. Do not modify any production code — only add or improve tests.\n\n",
    );

    doc.push_str("# Task (for reference)\n\n");
    doc.push_str(&base_doc);
    doc.push_str("\n\n");

    doc.push_str("# Test Instructions\n\n");
    doc.push_str(
        "1. Read the task specification and the code changes on this branch\n\
         2. Add unit tests that exercise the described behavior\n\
         3. Add edge case and error path tests\n\
         4. Ensure all tests pass\n\
         5. Do NOT modify production code — only add tests\n\n",
    );

    doc.push_str(&conflict_resolution_section(conflict_files));
    doc.push_str(&verification_section(cmds));
    doc.push_str(&commit_instructions(sign, cmds));
    doc.push_str(&rebase_and_push_section(cmds));
    if notify {
        doc.push_str(&notification_section(&task.label()));
    }
    if let Some(t) = timeout {
        doc.push_str(&timeout_section(&format!("{}s", t.as_secs())));
    }
    doc.push_str(&mission_reminder());
    doc.push_str(PLAN_MODE_INSTRUCTION);

    Ok(doc)
}
