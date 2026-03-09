use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;

use super::Runner;
use crate::design::task::TaskState;

/// A detected issue that can be fixed.
#[derive(Debug)]
pub struct Issue {
    pub description: String,
    pub fix_description: String,
}

/// Result of scanning for issues.
pub struct ScanResult {
    pub issues: Vec<Issue>,
}

impl Runner {
    /// Scans for project issues and optionally fixes them.
    pub fn fix(&self, auto_confirm: bool) -> Result<()> {
        let issues = self.scan_issues()?;

        if issues.issues.is_empty() {
            println!("No issues found.");
            return Ok(());
        }

        println!("Found {} issue(s):", issues.issues.len());
        for (i, issue) in issues.issues.iter().enumerate() {
            println!("  {}. {} — {}", i + 1, issue.description, issue.fix_description);
        }

        if !auto_confirm {
            println!("\nRun with --yes to apply fixes automatically.");
            return Ok(());
        }

        self.apply_fixes()?;
        println!("Fixes applied.");
        Ok(())
    }

    /// Scans for project issues.
    fn scan_issues(&self) -> Result<ScanResult> {
        let mut issues = Vec::new();

        // Check for duplicate task names.
        self.check_duplicate_tasks(&mut issues)?;

        // Check for stale locks.
        self.check_stale_locks(&mut issues)?;

        // Check for missing state directories.
        self.check_missing_state_dirs(&mut issues)?;

        // Check for orphaned work directories.
        self.check_orphaned_work_dirs(&mut issues)?;

        // Check for stuck merge tasks.
        self.check_stuck_merge_tasks(&mut issues)?;

        Ok(ScanResult { issues })
    }

    /// Checks for duplicate task names across all states.
    fn check_duplicate_tasks(&self, issues: &mut Vec<Issue>) -> Result<()> {
        let all_tasks = self.design.all_tasks()?;
        let mut seen: HashMap<String, Vec<String>> = HashMap::new();

        for task in &all_tasks {
            let label = task.label();
            seen.entry(label.clone())
                .or_default()
                .push(task.state.as_str().to_string());
        }

        for (name, states) in &seen {
            if states.len() > 1 {
                issues.push(Issue {
                    description: format!("duplicate task {:?} in states: {}", name, states.join(", ")),
                    fix_description: "remove duplicates manually".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Checks for stale lock files.
    fn check_stale_locks(&self, issues: &mut Vec<Issue>) -> Result<()> {
        let combust_dir = self.config.base_dir.join(crate::config::COMBUST_DIR);
        let pattern = combust_dir.join("combust-*.lock");
        let pattern_str = pattern.to_string_lossy();

        for entry in glob::glob(&pattern_str).context("globbing lock files")? {
            let path = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };
            let data = match fs::read_to_string(&path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            #[derive(serde::Deserialize)]
            struct LockData {
                pid: u32,
                task_name: String,
            }

            let ld: LockData = match serde_json::from_str(&data) {
                Ok(l) => l,
                Err(_) => continue,
            };

            if !process_alive(ld.pid) {
                issues.push(Issue {
                    description: format!(
                        "stale lock for {:?} (PID {} dead)",
                        ld.task_name, ld.pid
                    ),
                    fix_description: format!("remove {}", path.display()),
                });
            }
        }

        Ok(())
    }

    /// Checks for missing state directories.
    fn check_missing_state_dirs(&self, issues: &mut Vec<Issue>) -> Result<()> {
        let state_dirs = ["review", "merge", "completed", "abandoned"];
        let state_base = self.design.path.join("state");

        for dir_name in &state_dirs {
            let path = state_base.join(dir_name);
            if !path.is_dir() {
                issues.push(Issue {
                    description: format!("missing state directory: state/{}", dir_name),
                    fix_description: format!("create {}", path.display()),
                });
            }
        }

        Ok(())
    }

    /// Checks for orphaned work directories.
    fn check_orphaned_work_dirs(&self, issues: &mut Vec<Issue>) -> Result<()> {
        let work_dir = self.config.work_dir();
        let entries = match fs::read_dir(&work_dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        let all_tasks = self.design.all_tasks()?;
        let known_dirs: HashSet<String> = all_tasks
            .iter()
            .map(|t| t.label().replace('/', "--"))
            .collect();

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip special directories.
            if name.starts_with('_') {
                continue;
            }
            if !known_dirs.contains(&name) {
                issues.push(Issue {
                    description: format!("orphaned work directory: work/{}", name),
                    fix_description: "remove or investigate".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Checks for tasks stuck in merge state.
    fn check_stuck_merge_tasks(&self, issues: &mut Vec<Issue>) -> Result<()> {
        let merge_tasks = self.design.tasks_by_state(TaskState::Merge)?;
        for task in &merge_tasks {
            let combust_dir = self.config.base_dir.join(crate::config::COMBUST_DIR);
            let lock = crate::lock::Lock::new(&combust_dir, &task.label());
            if !lock.is_held() {
                issues.push(Issue {
                    description: format!("task {:?} stuck in merge state (no active lock)", task.label()),
                    fix_description: "move back to review".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Applies automatic fixes for detected issues.
    fn apply_fixes(&self) -> Result<()> {
        // Remove stale locks.
        let combust_dir = self.config.base_dir.join(crate::config::COMBUST_DIR);
        let pattern = combust_dir.join("combust-*.lock");
        let pattern_str = pattern.to_string_lossy();

        for entry in glob::glob(&pattern_str).context("globbing lock files")? {
            let path = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };
            let data = match fs::read_to_string(&path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            #[derive(serde::Deserialize)]
            struct LockData {
                pid: u32,
            }

            let ld: LockData = match serde_json::from_str(&data) {
                Ok(l) => l,
                Err(_) => continue,
            };

            if !process_alive(ld.pid) {
                let _ = fs::remove_file(&path);
                println!("  Removed stale lock: {}", path.display());
            }
        }

        // Create missing state directories.
        let state_dirs = ["review", "merge", "completed", "abandoned"];
        let state_base = self.design.path.join("state");
        for dir_name in &state_dirs {
            let path = state_base.join(dir_name);
            if !path.is_dir() {
                fs::create_dir_all(&path)?;
                println!("  Created: {}", path.display());
            }
        }

        // Move stuck merge tasks back to review.
        let merge_tasks = self.design.tasks_by_state(TaskState::Merge)?;
        for task in merge_tasks {
            let combust_dir = self.config.base_dir.join(crate::config::COMBUST_DIR);
            let lock = crate::lock::Lock::new(&combust_dir, &task.label());
            if !lock.is_held() {
                let mut task = task;
                // Move from merge back — we need to use a different approach since
                // move_task doesn't support moving backwards. We'll move to review
                // by direct file rename.
                let review_dir = if task.group.is_empty() {
                    self.design.path.join("state/review")
                } else {
                    self.design.path.join("state/review").join(&task.group)
                };
                fs::create_dir_all(&review_dir)?;
                let dest = review_dir.join(task.file_path.file_name().unwrap());
                fs::rename(&task.file_path, &dest)?;
                task.file_path = dest;
                task.state = TaskState::Review;
                println!("  Moved {} from merge to review", task.label());
            }
        }

        Ok(())
    }
}

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal;
        use nix::unistd::Pid;
        signal::kill(Pid::from_raw(pid as i32), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
