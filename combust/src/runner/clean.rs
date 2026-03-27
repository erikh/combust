use anyhow::{Context, Result};

use super::{run_shell_command, Runner};
use combust_db::task::TaskState;

impl Runner {
    /// Runs the clean command in a task's work directory.
    pub fn clean(&self, task_name: &str) -> Result<()> {
        // Find the task in any state except pending.
        let task = self
            .design
            .find_task_by_state(task_name, TaskState::Review)
            .or_else(|_| self.design.find_task_by_state(task_name, TaskState::Merge))
            .context("finding task in review or merge state")?;

        let work_dir = self.config.work_dir().join(
            task.label().replace('/', "--"),
        );

        if !work_dir.is_dir() {
            anyhow::bail!(
                "work directory does not exist: {}",
                work_dir.display()
            );
        }

        if let Some(ref cmds) = self.commands {
            if let Some(ref clean_cmd) = cmds.clean {
                run_shell_command(clean_cmd, &work_dir)?;
                println!("Clean command completed for {}.", task.label());
            } else {
                anyhow::bail!("no clean command configured in combust.yml");
            }
        } else {
            anyhow::bail!("no commands configured");
        }

        Ok(())
    }
}
