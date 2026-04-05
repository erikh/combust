use anyhow::{Context, Result};

use super::document::{
    commit_instructions, conflict_resolution_section, mission_reminder,
    notification_section, rebase_and_push_section, timeout_section,
    verification_section, PLAN_MODE_INSTRUCTION,
};
use super::{run_shell_command, ClaudeRunConfig, Runner};
use combust_db::task::TaskState;
use combust_db::lock::Lock;

impl Runner {
    /// Interactive review on a task in review state.
    pub fn review(&self, task_name: &str) -> Result<()> {
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)
            .context("finding task in review")?;

        let branch = task.branch_name();
        let work_dir = self.config.work_dir().join(
            task.label().replace('/', "--"),
        );

        // Acquire lock.
        let lock = Lock::new(&self.config.base_dir.join(combust_db::config::COMBUST_DIR), &task.label());
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

        // Assemble review document.
        let sign = repo.has_signing_key();
        let cmds = self.commands_map(&work_dir);
        let doc = assemble_review_document(
            &self.design, &task, sign, &cmds, &conflict_files,
            self.notify, self.timeout.as_ref(),
        )?;

        // Invoke Claude.
        let cfg = ClaudeRunConfig {
            repo_dir: work_dir.clone(),
            document: doc,
            model: self.model.clone(),
            auto_accept: self.auto_accept,
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
                &format!("Review of {} completed", task.label()),
            );
        }

        println!("Review of {} completed and pushed.", task.label());
        lock.release().context("releasing lock")?;
        Ok(())
    }

    /// Runs the dev command in a task's work directory.
    pub fn review_dev(&self, task_name: &str) -> Result<()> {
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)
            .context("finding task in review")?;

        let work_dir = self.config.work_dir().join(
            task.label().replace('/', "--"),
        );

        if !work_dir.is_dir() {
            anyhow::bail!("work directory does not exist: {}", work_dir.display());
        }

        if let Some(ref cmds) = self.commands {
            if let Some(ref dev_cmd) = cmds.dev {
                run_shell_command(dev_cmd, &work_dir)?;
            } else {
                anyhow::bail!("no dev command configured in combust.yml");
            }
        } else {
            anyhow::bail!("no commands configured");
        }

        Ok(())
    }

    /// Lists tasks in review state.
    pub fn review_list(&self) -> Result<Vec<combust_db::task::Task>> {
        self.design.tasks_by_state(TaskState::Review)
    }

    /// Shows the content of a task in review.
    pub fn review_view(&self, task_name: &str) -> Result<String> {
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)?;
        task.content()
    }

    /// Shows the diff for a task in review.
    pub fn review_diff(&self, task_name: &str) -> Result<String> {
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)?;

        let work_dir = self.config.work_dir().join(
            task.label().replace('/', "--"),
        );

        let repo = crate::git::Repo::open(&work_dir);
        repo.pull().context("pulling repository")?;
        let default_branch = self.detect_default_branch(&repo)?;
        repo.diff(&format!("origin/{}", default_branch))
    }

    /// Removes a task from review (moves to abandoned).
    pub fn review_remove(&self, task_name: &str) -> Result<()> {
        let mut task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)?;
        self.design.move_task(&mut task, TaskState::Abandoned)?;
        println!("Task {} moved to abandoned.", task.label());
        Ok(())
    }
}

/// Builds the prompt for the review workflow.
fn assemble_review_document(
    design: &combust_db::design::DesignDir,
    task: &combust_db::task::Task,
    sign: bool,
    cmds: &std::collections::HashMap<String, String>,
    conflict_files: &[String],
    notify: bool,
    timeout: Option<&std::time::Duration>,
) -> Result<String> {
    let task_content = task.content()?;
    let group_content = design.group_content(&task.group)?;
    let base_doc = design.assemble_document(&task_content, &group_content)?;

    let mut doc = base_doc;

    doc.push_str("# Review Instructions\n\n");
    doc.push_str(
        "You are reviewing existing code that was written for the task above. \
         The code is already implemented on this branch. Your job is to:\n\n\
         1. Read the task specification carefully\n\
         2. Review the existing code changes\n\
         3. Fix any bugs, missing edge cases, or specification violations\n\
         4. Ensure adequate test coverage\n\
         5. Ensure all tests pass\n\n",
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
