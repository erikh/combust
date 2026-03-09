use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct LockData {
    pid: u32,
    task_name: String,
}

/// Describes a currently-running combust task.
#[derive(Debug)]
pub struct RunningTask {
    pub task_name: String,
    pub pid: u32,
}

/// Provides mutual exclusion for combust task runs using a file-based lock.
pub struct Lock {
    path: PathBuf,
    task_name: String,
}

impl Lock {
    /// Creates a new Lock for the given combust directory and task name.
    pub fn new(combust_dir: &Path, task_name: &str) -> Self {
        let safe = task_name.replace('/', "--");
        let filename = format!("combust-{}.lock", safe);
        Lock {
            path: combust_dir.join(filename),
            task_name: task_name.to_string(),
        }
    }

    /// Attempts to acquire the lock.
    pub fn acquire(&self) -> Result<()> {
        if let Ok(existing) = self.read() {
            if process_alive(existing.pid) {
                bail!(
                    "task {:?} is already running (PID {})",
                    existing.task_name,
                    existing.pid
                );
            }
            // Stale lock, remove it.
            if let Err(e) = fs::remove_file(&self.path) {
                eprintln!(
                    "Warning: could not remove stale lock {}: {}",
                    self.path.display(),
                    e
                );
            }
        }

        let data = serde_json::to_string(&LockData {
            pid: std::process::id(),
            task_name: self.task_name.clone(),
        })
        .context("marshaling lock data")?;

        fs::write(&self.path, data).context("writing lock file")?;
        Ok(())
    }

    /// Releases the lock file.
    pub fn release(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).context("removing lock file"),
        }
    }

    /// Returns true if the lock file exists and is held by a live process.
    pub fn is_held(&self) -> bool {
        match self.read() {
            Ok(data) => process_alive(data.pid),
            Err(_) => false,
        }
    }

    fn read(&self) -> Result<LockData> {
        let data = fs::read_to_string(&self.path)?;
        let ld: LockData = serde_json::from_str(&data)?;
        Ok(ld)
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Scans the combust directory for per-task lock files and returns
/// all tasks currently held by live processes.
pub fn read_all(combust_dir: &Path) -> Result<Vec<RunningTask>> {
    let pattern = combust_dir.join("combust-*.lock");
    let pattern_str = pattern.to_string_lossy();

    let mut running = Vec::new();
    for entry in glob::glob(&pattern_str).context("globbing lock files")? {
        let path = match entry {
            Ok(p) => p,
            Err(_) => continue,
        };
        let data = match fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let ld: LockData = match serde_json::from_str(&data) {
            Ok(l) => l,
            Err(_) => continue,
        };
        if process_alive(ld.pid) {
            running.push(RunningTask {
                task_name: ld.task_name,
                pid: ld.pid,
            });
        }
    }

    Ok(running)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_acquire_and_release() {
        let tmp = TempDir::new().unwrap();
        let lock = Lock::new(tmp.path(), "my-task");

        lock.acquire().unwrap();
        assert!(lock.path.exists());

        // Verify lock file content.
        let data = fs::read_to_string(&lock.path).unwrap();
        let ld: LockData = serde_json::from_str(&data).unwrap();
        assert_eq!(ld.pid, std::process::id());
        assert_eq!(ld.task_name, "my-task");

        lock.release().unwrap();
        assert!(!lock.path.exists());
    }

    #[test]
    fn test_acquire_blocked_by_same_task() {
        let tmp = TempDir::new().unwrap();
        let lock1 = Lock::new(tmp.path(), "same-task");
        lock1.acquire().unwrap();

        let lock2 = Lock::new(tmp.path(), "same-task");
        let result = lock2.acquire();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already running"));

        lock1.release().unwrap();
    }

    #[test]
    fn test_acquire_not_blocked_by_different_task() {
        let tmp = TempDir::new().unwrap();
        let lock1 = Lock::new(tmp.path(), "task-a");
        lock1.acquire().unwrap();

        let lock2 = Lock::new(tmp.path(), "task-b");
        // Different task names get different files, so this should succeed.
        lock2.acquire().unwrap();

        lock1.release().unwrap();
        lock2.release().unwrap();
    }

    #[test]
    fn test_acquire_stale_lock() {
        let tmp = TempDir::new().unwrap();
        // Write a lock file with a dead PID.
        let stale = LockData {
            pid: 999999999, // Almost certainly not running.
            task_name: "stale-task".to_string(),
        };
        let lock_path = tmp.path().join("combust-stale-task.lock");
        fs::write(&lock_path, serde_json::to_string(&stale).unwrap()).unwrap();

        let lock = Lock::new(tmp.path(), "stale-task");
        // Should succeed because the PID is dead.
        lock.acquire().unwrap();

        // Verify it's now our PID.
        let data = fs::read_to_string(&lock.path).unwrap();
        let ld: LockData = serde_json::from_str(&data).unwrap();
        assert_eq!(ld.pid, std::process::id());

        lock.release().unwrap();
    }

    #[test]
    fn test_release_idempotent() {
        let tmp = TempDir::new().unwrap();
        let lock = Lock::new(tmp.path(), "no-acquire");
        // Release without acquire should not error.
        lock.release().unwrap();
        // Double release should also be fine.
        lock.release().unwrap();
    }

    #[test]
    fn test_is_held() {
        let tmp = TempDir::new().unwrap();
        let lock = Lock::new(tmp.path(), "held-task");

        assert!(!lock.is_held());
        lock.acquire().unwrap();
        assert!(lock.is_held());
        lock.release().unwrap();
        assert!(!lock.is_held());
    }

    #[test]
    fn test_read_all() {
        let tmp = TempDir::new().unwrap();

        let lock1 = Lock::new(tmp.path(), "task-1");
        lock1.acquire().unwrap();
        let lock2 = Lock::new(tmp.path(), "task-2");
        lock2.acquire().unwrap();

        let running = read_all(tmp.path()).unwrap();
        assert_eq!(running.len(), 2);

        let names: Vec<&str> = running.iter().map(|r| r.task_name.as_str()).collect();
        assert!(names.contains(&"task-1"));
        assert!(names.contains(&"task-2"));

        lock1.release().unwrap();
        lock2.release().unwrap();
    }

    #[test]
    fn test_read_all_no_locks() {
        let tmp = TempDir::new().unwrap();
        let running = read_all(tmp.path()).unwrap();
        assert!(running.is_empty());
    }

    #[test]
    fn test_read_all_stale_lock() {
        let tmp = TempDir::new().unwrap();
        // Write a stale lock.
        let stale = LockData {
            pid: 999999999,
            task_name: "dead-task".to_string(),
        };
        fs::write(
            tmp.path().join("combust-dead-task.lock"),
            serde_json::to_string(&stale).unwrap(),
        )
        .unwrap();

        let running = read_all(tmp.path()).unwrap();
        assert!(running.is_empty(), "stale lock should be filtered out");
    }

    #[test]
    fn test_lock_file_name_grouped_task() {
        let tmp = TempDir::new().unwrap();
        let lock = Lock::new(tmp.path(), "group/task-name");
        // Slashes should be replaced with --.
        assert!(lock.path.to_string_lossy().contains("combust-group--task-name.lock"));
    }

    #[test]
    fn test_drop_releases() {
        let tmp = TempDir::new().unwrap();
        let lock_path;
        {
            let lock = Lock::new(tmp.path(), "drop-task");
            lock.acquire().unwrap();
            lock_path = lock.path.clone();
            assert!(lock_path.exists());
        }
        // After drop, the lock file should be removed.
        assert!(!lock_path.exists());
    }
}
