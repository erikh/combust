use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::revision::{write_revision, CURRENT_REVISION};

/// In-memory snapshot of the `.combust/` directory for migration.
pub struct Snapshot {
    pub revision: u32,
    pub config: serde_json::Value,
    pub design_files: BTreeMap<PathBuf, Vec<u8>>,
    pub state_files: BTreeMap<PathBuf, Vec<u8>>,
    pub design_is_symlink: bool,
    pub design_symlink_target: Option<PathBuf>,
}

/// Runs all pending migrations from `from_rev` up to CURRENT_REVISION.
pub fn run_migrations(combust_dir: &Path, from_rev: u32) -> Result<()> {
    let mut snapshot = ingest(combust_dir)?;

    for rev in (from_rev + 1)..=CURRENT_REVISION {
        apply_migration(&mut snapshot, rev)?;
    }

    write_back(combust_dir, &snapshot)?;

    Ok(())
}

/// Reads the entire `.combust/` directory into a Snapshot.
fn ingest(combust_dir: &Path) -> Result<Snapshot> {
    let revision = crate::revision::read_revision(combust_dir)?;

    // Read config
    let config_path = combust_dir.join("config.json");
    let config: serde_json::Value = if config_path.exists() {
        let data = fs::read_to_string(&config_path).context("reading config.json for migration")?;
        serde_json::from_str(&data).context("parsing config.json for migration")?
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    // Check if design is a symlink
    let design_path = combust_dir.join("design");
    let design_is_symlink = fs::symlink_metadata(&design_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let design_symlink_target = if design_is_symlink {
        fs::read_link(&design_path).ok()
    } else {
        None
    };

    // Read design files (from the resolved path)
    let design_files = if design_path.exists() {
        read_dir_recursive(&design_path, &design_path)?
    } else {
        BTreeMap::new()
    };

    // Read state files
    let state_path = combust_dir.join("state");
    let state_files = if state_path.exists() {
        read_dir_recursive(&state_path, &state_path)?
    } else {
        BTreeMap::new()
    };

    Ok(Snapshot {
        revision,
        config,
        design_files,
        state_files,
        design_is_symlink,
        design_symlink_target,
    })
}

/// Reads a directory recursively into a map of relative paths → contents.
/// Skips symlinks at the file level (not at the root).
fn read_dir_recursive(base: &Path, dir: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };

    for entry in entries {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let sub = read_dir_recursive(base, &entry.path())?;
            files.extend(sub);
        } else if ft.is_file() {
            let rel = entry.path().strip_prefix(base).unwrap().to_path_buf();
            let content = fs::read(entry.path())
                .with_context(|| format!("reading {}", entry.path().display()))?;
            files.insert(rel, content);
        }
    }

    Ok(files)
}

/// Applies a single migration step.
fn apply_migration(snapshot: &mut Snapshot, rev: u32) -> Result<()> {
    match rev {
        1 => migrate_v0_to_v1(snapshot),
        _ => anyhow::bail!("unknown migration revision: {}", rev),
    }
}

/// v0 → v1: Baseline migration.
/// Ensures state subdirectories and record.json exist. No structural changes.
fn migrate_v0_to_v1(snapshot: &mut Snapshot) -> Result<()> {
    // Ensure state subdirectories exist by adding placeholder entries
    // (the write_back will create the directories).
    let state_dirs = ["review", "merge", "completed", "abandoned"];
    for dir_name in &state_dirs {
        // We don't need to add files, just ensure the directories exist during write_back.
        // Check if there's any file under this path already.
        let prefix = PathBuf::from(dir_name);
        let has_files = snapshot
            .state_files
            .keys()
            .any(|k| k.starts_with(&prefix));
        if !has_files {
            // We'll handle directory creation in write_back
        }
    }

    // Ensure record.json exists
    let record_path = PathBuf::from("record.json");
    snapshot
        .state_files
        .entry(record_path)
        .or_insert_with(|| b"[]\n".to_vec());

    snapshot.revision = 1;
    Ok(())
}

/// Atomically writes the snapshot back to disk.
fn write_back(combust_dir: &Path, snapshot: &Snapshot) -> Result<()> {
    let new_dir = combust_dir.with_file_name(".combust.new");
    let old_dir = combust_dir.with_file_name(".combust.old");

    // Clean up any leftover temp dirs from a previous failed migration
    let _ = fs::remove_dir_all(&new_dir);
    let _ = fs::remove_dir_all(&old_dir);

    fs::create_dir_all(&new_dir).context("creating .combust.new")?;

    // Write config.json
    let config_data =
        serde_json::to_string_pretty(&snapshot.config).context("marshaling config")?;
    fs::write(new_dir.join("config.json"), &config_data).context("writing config.json")?;

    // Write design files
    let design_dir = new_dir.join("design");
    if snapshot.design_is_symlink {
        // Recreate symlink
        if let Some(ref target) = snapshot.design_symlink_target {
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &design_dir)
                .context("recreating design symlink")?;
            // Write design files to the resolved target
            for (rel, content) in &snapshot.design_files {
                let dest = target.join(rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&dest, content)?;
            }
        }
    } else {
        for (rel, content) in &snapshot.design_files {
            let dest = design_dir.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, content)?;
        }
    }

    // Write state files
    let state_dir = new_dir.join("state");
    fs::create_dir_all(&state_dir)?;
    // Ensure state subdirectories exist
    for dir_name in &["review", "merge", "completed", "abandoned"] {
        fs::create_dir_all(state_dir.join(dir_name))?;
    }
    for (rel, content) in &snapshot.state_files {
        let dest = state_dir.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, content)?;
    }

    // Write revision file
    write_revision(&new_dir, snapshot.revision)?;

    // Move work/ from old tree to new tree (same filesystem, just rename)
    let old_work = combust_dir.join("work");
    let new_work = new_dir.join("work");
    if old_work.exists() {
        if fs::rename(&old_work, &new_work).is_err() {
            // Cross-device: copy instead
            copy_dir_recursive(&old_work, &new_work)?;
        }
    } else {
        fs::create_dir_all(&new_work)?;
    }

    // Atomic swap
    fs::rename(combust_dir, &old_dir).context("renaming .combust to .combust.old")?;
    fs::rename(&new_dir, combust_dir).context("renaming .combust.new to .combust")?;
    let _ = fs::remove_dir_all(&old_dir);

    Ok(())
}

/// Recursively copies a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_v0_combust_dir() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let combust_dir = tmp.path().join(".combust");
        fs::create_dir_all(combust_dir.join("design/tasks")).unwrap();
        fs::create_dir_all(combust_dir.join("work")).unwrap();
        fs::create_dir_all(combust_dir.join("state")).unwrap();

        // Write a config
        let config = serde_json::json!({
            "source_repo_url": "https://example.com/repo",
            "private": false,
            "theme": "base16-ocean.dark"
        });
        fs::write(
            combust_dir.join("config.json"),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();

        // Write some design files
        fs::write(combust_dir.join("design/rules.md"), "Rule content").unwrap();
        fs::write(combust_dir.join("design/tasks/task-a.md"), "Task A").unwrap();

        // No revision file (v0)
        (tmp, combust_dir)
    }

    #[test]
    fn test_ingest_reads_snapshot() {
        let (_tmp, combust_dir) = setup_v0_combust_dir();
        let snapshot = ingest(&combust_dir).unwrap();
        assert_eq!(snapshot.revision, 0);
        assert!(!snapshot.design_is_symlink);
        assert!(snapshot.design_files.contains_key(&PathBuf::from("rules.md")));
        assert!(snapshot.design_files.contains_key(&PathBuf::from("tasks/task-a.md")));
    }

    #[test]
    fn test_v0_to_v1_migration() {
        let (_tmp, combust_dir) = setup_v0_combust_dir();
        run_migrations(&combust_dir, 0).unwrap();

        // Revision should now be 1
        assert_eq!(crate::revision::read_revision(&combust_dir).unwrap(), 1);

        // State dirs should exist
        assert!(combust_dir.join("state/review").is_dir());
        assert!(combust_dir.join("state/merge").is_dir());
        assert!(combust_dir.join("state/completed").is_dir());
        assert!(combust_dir.join("state/abandoned").is_dir());

        // record.json should exist
        assert!(combust_dir.join("state/record.json").is_file());

        // Design files should be preserved
        assert_eq!(
            fs::read_to_string(combust_dir.join("design/rules.md")).unwrap(),
            "Rule content"
        );
        assert_eq!(
            fs::read_to_string(combust_dir.join("design/tasks/task-a.md")).unwrap(),
            "Task A"
        );

        // Config should be preserved
        let config_data = fs::read_to_string(combust_dir.join("config.json")).unwrap();
        let config: serde_json::Value = serde_json::from_str(&config_data).unwrap();
        assert_eq!(
            config["source_repo_url"].as_str().unwrap(),
            "https://example.com/repo"
        );

        // Work dir should still exist
        assert!(combust_dir.join("work").is_dir());
    }

    #[test]
    fn test_migration_preserves_work_dir() {
        let (_tmp, combust_dir) = setup_v0_combust_dir();

        // Create some work content
        fs::create_dir_all(combust_dir.join("work/my-task")).unwrap();
        fs::write(combust_dir.join("work/my-task/file.txt"), "work content").unwrap();

        run_migrations(&combust_dir, 0).unwrap();

        // Work content should be preserved
        assert_eq!(
            fs::read_to_string(combust_dir.join("work/my-task/file.txt")).unwrap(),
            "work content"
        );
    }

    #[test]
    fn test_migration_preserves_state_files() {
        let (_tmp, combust_dir) = setup_v0_combust_dir();

        // Create some state content
        fs::create_dir_all(combust_dir.join("state/completed")).unwrap();
        fs::write(
            combust_dir.join("state/completed/done-task.md"),
            "Done content",
        )
        .unwrap();

        run_migrations(&combust_dir, 0).unwrap();

        // State content should be preserved
        assert_eq!(
            fs::read_to_string(combust_dir.join("state/completed/done-task.md")).unwrap(),
            "Done content"
        );
    }

    #[test]
    fn test_no_leftover_temp_dirs() {
        let (tmp, combust_dir) = setup_v0_combust_dir();

        run_migrations(&combust_dir, 0).unwrap();

        assert!(!tmp.path().join(".combust.new").exists());
        assert!(!tmp.path().join(".combust.old").exists());
    }
}
