use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

/// The current schema revision. Bump this when adding a new migration.
pub const CURRENT_REVISION: u32 = 1;

/// Reads the revision number from `.combust/revision`.
/// Returns 0 if the file does not exist.
pub fn read_revision(combust_dir: &Path) -> Result<u32> {
    let path = combust_dir.join("revision");
    match fs::read_to_string(&path) {
        Ok(content) => {
            let rev: u32 = content
                .trim()
                .parse()
                .with_context(|| format!("parsing revision from {}", path.display()))?;
            Ok(rev)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e).context("reading revision file"),
    }
}

/// Writes the revision number to `.combust/revision`.
pub fn write_revision(combust_dir: &Path, rev: u32) -> Result<()> {
    let path = combust_dir.join("revision");
    fs::write(&path, format!("{}\n", rev)).context("writing revision file")?;
    Ok(())
}

/// Checks the current revision and runs migrations if needed.
pub fn migrate_if_needed(combust_dir: &Path) -> Result<()> {
    let current = read_revision(combust_dir)?;

    if current == CURRENT_REVISION {
        return Ok(());
    }

    if current > CURRENT_REVISION {
        bail!(
            ".combust/revision is {} but this binary only supports up to {}; upgrade combust",
            current,
            CURRENT_REVISION
        );
    }

    // Run migration pipeline
    crate::migration::run_migrations(combust_dir, current)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_revision_missing() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(read_revision(tmp.path()).unwrap(), 0);
    }

    #[test]
    fn test_write_and_read_revision() {
        let tmp = TempDir::new().unwrap();
        write_revision(tmp.path(), 1).unwrap();
        assert_eq!(read_revision(tmp.path()).unwrap(), 1);
    }

    #[test]
    fn test_write_and_read_revision_higher() {
        let tmp = TempDir::new().unwrap();
        write_revision(tmp.path(), 42).unwrap();
        assert_eq!(read_revision(tmp.path()).unwrap(), 42);
    }

    #[test]
    fn test_migrate_if_needed_current() {
        let tmp = TempDir::new().unwrap();
        write_revision(tmp.path(), CURRENT_REVISION).unwrap();
        migrate_if_needed(tmp.path()).unwrap();
    }

    #[test]
    fn test_migrate_if_needed_future() {
        let tmp = TempDir::new().unwrap();
        write_revision(tmp.path(), CURRENT_REVISION + 1).unwrap();
        let err = migrate_if_needed(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("upgrade combust"));
    }
}
