use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const BEGIN_MARKER: &str = "# --- combust managed (do not edit) ---";
const END_MARKER: &str = "# --- end combust managed ---";

/// Ensures the .gitignore at `base_dir` contains the combust-managed section.
pub fn sync_gitignore(base_dir: &Path, private: bool) -> Result<()> {
    let gitignore_path = base_dir.join(".gitignore");

    let existing = match fs::read_to_string(&gitignore_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).context("reading .gitignore"),
    };

    let managed_section = build_managed_section(private);

    let new_content = if existing.contains(BEGIN_MARKER) {
        replace_managed_section(&existing, &managed_section)
    } else {
        let mut content = existing;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&managed_section);
        content
    };

    fs::write(&gitignore_path, &new_content).context("writing .gitignore")?;
    Ok(())
}

fn build_managed_section(private: bool) -> String {
    let mut section = String::new();
    section.push_str(BEGIN_MARKER);
    section.push('\n');

    if private {
        section.push_str(".combust/design\n");
    }
    section.push_str(".combust/work/\n");
    section.push_str(".combust/state/\n");
    section.push_str(".combust/config.json\n");
    section.push_str(".combust/*.lock\n");

    section.push_str(END_MARKER);
    section.push('\n');
    section
}

fn replace_managed_section(content: &str, new_section: &str) -> String {
    let mut result = String::new();
    let mut in_managed = false;

    for line in content.lines() {
        if line.trim() == BEGIN_MARKER {
            in_managed = true;
            result.push_str(new_section);
            continue;
        }
        if line.trim() == END_MARKER {
            in_managed = false;
            continue;
        }
        if !in_managed {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sync_gitignore_creates_new() {
        let tmp = TempDir::new().unwrap();
        sync_gitignore(tmp.path(), false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(BEGIN_MARKER));
        assert!(content.contains(END_MARKER));
        assert!(content.contains(".combust/work/"));
        assert!(content.contains(".combust/state/"));
        assert!(content.contains(".combust/config.json"));
        assert!(content.contains(".combust/*.lock"));
        assert!(!content.contains(".combust/design"));
    }

    #[test]
    fn test_sync_gitignore_private() {
        let tmp = TempDir::new().unwrap();
        sync_gitignore(tmp.path(), true).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".combust/design\n"));
        assert!(content.contains(".combust/work/"));
        assert!(content.contains(".combust/state/"));
        assert!(content.contains(".combust/config.json"));
        assert!(content.contains(".combust/*.lock"));
    }

    #[test]
    fn test_sync_gitignore_appends() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), "*.log\n").unwrap();
        sync_gitignore(tmp.path(), false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains("*.log"));
        assert!(content.contains(BEGIN_MARKER));
    }

    #[test]
    fn test_sync_gitignore_replaces_existing() {
        let tmp = TempDir::new().unwrap();
        let initial = format!(
            "*.log\n{}\n.combust\n{}\nother\n",
            BEGIN_MARKER, END_MARKER
        );
        fs::write(tmp.path().join(".gitignore"), &initial).unwrap();

        sync_gitignore(tmp.path(), false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains("*.log"));
        assert!(content.contains("other"));
        assert!(content.contains(".combust/work/"));
        // Old content should be replaced.
        let marker_count = content.matches(BEGIN_MARKER).count();
        assert_eq!(marker_count, 1);
    }

    #[test]
    fn test_sync_gitignore_idempotent() {
        let tmp = TempDir::new().unwrap();
        sync_gitignore(tmp.path(), false).unwrap();
        let content1 = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();

        // Run again — content should be identical.
        sync_gitignore(tmp.path(), false).unwrap();
        let content2 = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();

        assert_eq!(content1, content2);

        // Only one managed section.
        let marker_count = content2.matches(BEGIN_MARKER).count();
        assert_eq!(marker_count, 1);
    }
}
