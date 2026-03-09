use anyhow::{Context, Result};

use super::document::{
    commit_instructions, conflict_resolution_section, mission_reminder,
    notification_section, rebase_and_push_section, timeout_section,
    verification_section, PLAN_MODE_INSTRUCTION,
};
use super::{ClaudeRunConfig, Runner};
use crate::design::record::Record;
use crate::design::task::TaskState;
use crate::lock::Lock;

impl Runner {
    /// Executes the full task lifecycle: lock → branch → assemble → claude → test → commit → push → record → move to review.
    pub fn run_task(&self, task_name: &str) -> Result<()> {
        let task = self
            .design
            .find_task(task_name)
            .context("finding task")?;

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

        // Attempt rebase onto default branch.
        let conflict_files = if let Ok(()) = repo.fetch() {
            if let Ok(default_branch) = self.detect_default_branch(&repo) {
                if repo.rebase(&format!("origin/{}", default_branch)).is_err() {
                    repo.conflict_files().unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Run before hook.
        self.run_before_hook(&work_dir).context("before hook")?;

        // Assemble document.
        let task_content = task.content().context("reading task content")?;
        let group_content = self
            .design
            .group_content(&task.group)
            .context("reading group content")?;
        let base_doc = self
            .design
            .assemble_document(&task_content, &group_content)
            .context("assembling document")?;

        let sign = repo.has_signing_key();
        let cmds = self.commands_map(&work_dir);

        let mut doc = base_doc;
        doc.push_str(&conflict_resolution_section(&conflict_files));
        doc.push_str(&verification_section(&cmds));
        doc.push_str(&commit_instructions(sign, &cmds));
        doc.push_str(&rebase_and_push_section(&cmds));
        if self.notify {
            doc.push_str(&notification_section(&task.label()));
        }
        if let Some(ref timeout) = self.timeout {
            doc.push_str(&timeout_section(&format!("{}s", timeout.as_secs())));
        }
        doc.push_str(&mission_reminder());
        doc.push_str(PLAN_MODE_INSTRUCTION);

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

        // Record SHA and push.
        let sha = repo
            .last_commit_sha()
            .context("getting commit SHA")?;
        repo.push(&branch)
            .context("pushing branch")?;

        // Record the commit.
        let record = Record::new(&self.design.path);
        record
            .add(&sha, &task.label())
            .context("recording commit")?;

        // Move task to review.
        let mut task = task;
        self.design
            .move_task(&mut task, TaskState::Review)
            .context("moving task to review")?;

        // Run teardown.
        if let Some(ref cmds) = self.commands {
            cmds.run_teardown(&work_dir);
        }

        // Send notification.
        if self.notify {
            let _ = crate::notify::send(
                "combust",
                &format!("Task {} completed and moved to review", task.label()),
            );
        }

        println!("Task {} pushed to branch {} and moved to review.", task.label(), branch);
        lock.release().context("releasing lock")?;
        Ok(())
    }

    /// Runs all pending tasks in a group sequentially.
    pub fn run_group(&self, group_name: &str) -> Result<()> {
        let tasks = self.design.group_tasks(group_name)?;
        if tasks.is_empty() {
            anyhow::bail!("no pending tasks in group {:?}", group_name);
        }

        for task in &tasks {
            println!("Running task: {}", task.label());
            self.run_task(&task.label())?;
        }

        Ok(())
    }

    /// Lists task groups.
    pub fn group_list(&self) -> Result<Vec<String>> {
        self.design.groups()
    }

    /// Lists tasks in a group.
    pub fn group_tasks(&self, group_name: &str) -> Result<Vec<crate::design::task::Task>> {
        self.design.group_tasks(group_name)
    }
}
