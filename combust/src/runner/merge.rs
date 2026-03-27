use anyhow::{Context, Result};

use super::document::{
    commit_instructions, mission_reminder, rebase_and_push_section, PLAN_MODE_INSTRUCTION,
};
use super::{ClaudeRunConfig, Runner};
use combust_db::record::Record;
use combust_db::task::TaskState;
use combust_db::lock::Lock;

impl Runner {
    /// Full merge workflow: rebase → resolve conflicts → push → merge to main → finalize.
    pub fn merge_task(&self, task_name: &str) -> Result<()> {
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)
            .or_else(|_| self.design.find_task_by_state(task_name, TaskState::Merge))
            .context("finding task in review or merge state")?;

        let branch = task.branch_name();
        let work_dir = self.config.work_dir().join(
            task.label().replace('/', "--"),
        );

        // Acquire lock.
        let lock = Lock::new(&self.config.base_dir.join(combust_db::config::COMBUST_DIR), &task.label());
        lock.acquire()
            .with_context(|| format!("acquiring lock for task {:?}", task.label()))?;

        // Move to merge state if in review.
        let mut task = task;
        if task.state == TaskState::Review {
            self.design
                .move_task(&mut task, TaskState::Merge)
                .context("moving task to merge")?;
        }

        // Prepare work directory.
        let repo = self
            .prepare_repo(&work_dir, &branch)
            .context("preparing work directory")?;

        // Run before hook.
        self.run_before_hook(&work_dir).context("before hook")?;

        // Attempt rebase onto default branch.
        repo.fetch().context("fetching origin")?;
        let default_branch = self
            .detect_default_branch(&repo)
            .context("detecting default branch")?;

        let rebase_result = repo.rebase(&format!("origin/{}", default_branch));
        if rebase_result.is_err() {
            // Rebase has conflicts — invoke Claude to resolve.
            let sign = repo.has_signing_key();
            let cmds = self.commands_map(&work_dir);
            let doc = assemble_merge_document(&self.design, &task, sign, &cmds, &default_branch)?;

            let cfg = ClaudeRunConfig {
                repo_dir: work_dir.clone(),
                document: doc,
                model: self.model.clone(),
                auto_accept: self.auto_accept,
                force_tui: self.force_tui,
            };
            self.invoke_claude(cfg).context("claude failed during merge")?;
        }

        // Push feature branch.
        repo.push(&branch).context("pushing feature branch")?;

        // Rebase main against origin and feature branch, then push.
        self.rebase_and_push_main(&repo, &branch, &default_branch)?;

        // Finalize: record SHA, move to completed.
        self.finalize_merge(&repo, &mut task, &work_dir)?;

        if self.notify {
            let _ = crate::notify::send(
                "combust",
                &format!("Merge of {} completed", task.label()),
            );
        }

        println!("Task {} merged and moved to completed.", task.label());
        lock.release().context("releasing lock")?;
        Ok(())
    }

    /// Merges all review/merge tasks in a group.
    pub fn merge_group(&self, group_name: &str) -> Result<()> {
        let review_tasks = self.design.tasks_by_state(TaskState::Review)?;
        let merge_tasks = self.design.tasks_by_state(TaskState::Merge)?;

        let tasks: Vec<_> = review_tasks
            .into_iter()
            .chain(merge_tasks)
            .filter(|t| t.group == group_name)
            .collect();

        if tasks.is_empty() {
            anyhow::bail!("no tasks to merge in group {:?}", group_name);
        }

        for task in &tasks {
            println!("Merging task: {}", task.label());
            self.merge_task(&task.label())?;
        }

        Ok(())
    }

    /// Lists tasks in merge state.
    pub fn merge_list(&self) -> Result<Vec<combust_db::task::Task>> {
        self.design.tasks_by_state(TaskState::Merge)
    }

    /// Shows the content of a task in merge state.
    pub fn merge_view(&self, task_name: &str) -> Result<String> {
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Merge)?;
        task.content()
    }

    /// Removes a task from merge (moves to abandoned).
    pub fn merge_remove(&self, task_name: &str) -> Result<()> {
        let mut task = self
            .design
            .find_task_by_state(task_name, TaskState::Merge)?;
        self.design.move_task(&mut task, TaskState::Abandoned)?;
        println!("Task {} moved to abandoned.", task.label());
        Ok(())
    }

    /// Rebases main against origin and the feature branch, then pushes main.
    fn rebase_and_push_main(
        &self,
        repo: &crate::git::Repo,
        branch: &str,
        default_branch: &str,
    ) -> Result<()> {
        // Switch to main.
        repo.checkout(default_branch)
            .context("checking out default branch")?;

        // Pull latest.
        repo.fetch().context("fetching origin")?;
        repo.reset_hard(&format!("origin/{}", default_branch))
            .context("resetting main to origin")?;

        // Merge the feature branch (fast-forward).
        let merge_ref = format!("origin/{}", branch);
        repo.rebase(&merge_ref)
            .with_context(|| format!("rebasing main onto {}", merge_ref))?;

        // Push main.
        repo.push_main().context("pushing main")?;

        // Switch back to feature branch.
        repo.checkout(branch).context("switching back to feature branch")?;

        Ok(())
    }

    /// Records the merge SHA and moves the task to completed.
    fn finalize_merge(
        &self,
        repo: &crate::git::Repo,
        task: &mut combust_db::task::Task,
        work_dir: &std::path::Path,
    ) -> Result<()> {
        let sha = repo.last_commit_sha().context("getting merge SHA")?;

        let record = Record::new(&self.design.state_path);
        record
            .add(&sha, &format!("merge:{}", task.label()))
            .context("recording merge commit")?;

        self.design
            .move_task(task, TaskState::Completed)
            .context("moving task to completed")?;

        // Run teardown.
        if let Some(ref cmds) = self.commands {
            cmds.run_teardown(work_dir);
        }

        Ok(())
    }
}

/// Builds the prompt for the merge workflow.
fn assemble_merge_document(
    design: &combust_db::design::DesignDir,
    task: &combust_db::task::Task,
    sign: bool,
    cmds: &std::collections::HashMap<String, String>,
    default_branch: &str,
) -> Result<String> {
    let task_content = task.content()?;
    let group_content = design.group_content(&task.group)?;
    let rules = design.rules()?;
    let lint = design.lint()?;

    let mut doc = String::new();
    doc.push_str("# Mission\n\n");
    doc.push_str(&format!(
        "Your sole objective is to resolve merge conflicts from rebasing this branch \
         onto origin/{}. The task description and code should be preserved — only resolve \
         conflicts and ensure the code works.\n\n",
        default_branch,
    ));

    if !rules.is_empty() {
        doc.push_str("# Rules\n\n");
        doc.push_str(&rules);
        doc.push_str("\n\n");
    }
    if !lint.is_empty() {
        doc.push_str("# Lint Rules\n\n");
        doc.push_str(&lint);
        doc.push_str("\n\n");
    }

    doc.push_str("# Task (for reference)\n\n");
    if !group_content.is_empty() {
        doc.push_str("## Group\n\n");
        doc.push_str(&group_content);
        doc.push_str("\n\n");
    }
    doc.push_str(&task_content);
    doc.push_str("\n\n");

    doc.push_str("# Conflict Resolution Instructions\n\n");
    doc.push_str(
        "1. Resolve all merge conflicts\n\
         2. Ensure the code compiles and all tests pass\n\
         3. Do not remove or alter any features — only fix conflicts\n\
         4. Stage resolved files and continue the rebase\n\n",
    );

    doc.push_str(&commit_instructions(sign, cmds));
    doc.push_str(&rebase_and_push_section(cmds));
    doc.push_str(&mission_reminder());
    doc.push_str(PLAN_MODE_INSTRUCTION);

    Ok(doc)
}
