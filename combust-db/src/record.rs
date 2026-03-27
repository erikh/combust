use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A single record entry mapping a commit SHA to a task name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub sha: String,
    pub task_name: String,
}

/// Manages the record of completed task SHAs stored in state/record.json.
pub struct Record {
    path: PathBuf,
}

impl Record {
    /// Creates a new Record for the given state directory.
    pub fn new(state_dir: &Path) -> Self {
        Record {
            path: state_dir.join("record.json"),
        }
    }

    /// Reads all record entries.
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let data = match fs::read_to_string(&self.path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("reading record.json"),
        };
        let entries: Vec<Entry> = serde_json::from_str(&data).context("parsing record.json")?;
        Ok(entries)
    }

    /// Adds a new entry mapping a SHA to a task name.
    pub fn add(&self, sha: &str, task_name: &str) -> Result<()> {
        let mut entries = self.entries()?;
        entries.push(Entry {
            sha: sha.to_string(),
            task_name: task_name.to_string(),
        });
        let data = serde_json::to_string_pretty(&entries).context("marshaling record")?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("creating record directory")?;
        }
        fs::write(&self.path, data).context("writing record.json")?;
        Ok(())
    }

    /// Returns the SHA for a given task name, if found.
    pub fn find_sha(&self, task_name: &str) -> Result<Option<String>> {
        let entries = self.entries()?;
        for entry in entries.iter().rev() {
            let name = entry
                .task_name
                .strip_prefix("merge:")
                .unwrap_or(&entry.task_name);
            if name == task_name {
                return Ok(Some(entry.sha.clone()));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Record) {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("record.json"), "[]\n").unwrap();
        let record = Record::new(tmp.path());
        (tmp, record)
    }

    #[test]
    fn test_entries_empty() {
        let (_tmp, record) = setup();
        let entries = record.entries().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_add_and_entries() {
        let (_tmp, record) = setup();
        record.add("abc123", "my-task").unwrap();
        record.add("def456", "other-task").unwrap();

        let entries = record.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].sha, "abc123");
        assert_eq!(entries[0].task_name, "my-task");
        assert_eq!(entries[1].sha, "def456");
        assert_eq!(entries[1].task_name, "other-task");
    }

    #[test]
    fn test_find_sha() {
        let (_tmp, record) = setup();
        record.add("abc123", "my-task").unwrap();
        record.add("def456", "merge:my-task").unwrap();

        assert_eq!(record.find_sha("my-task").unwrap(), Some("def456".to_string()));
    }

    #[test]
    fn test_find_sha_not_found() {
        let (_tmp, record) = setup();
        assert_eq!(record.find_sha("nonexistent").unwrap(), None);
    }

    #[test]
    fn test_entries_missing_file() {
        let tmp = TempDir::new().unwrap();
        let record = Record::new(tmp.path());
        let entries = record.entries().unwrap();
        assert!(entries.is_empty());
    }
}
