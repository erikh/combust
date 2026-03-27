use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Name of the combust configuration directory.
pub const COMBUST_DIR: &str = ".combust";
/// Name of the configuration file within the combust directory.
const CONFIG_FILE: &str = "config.json";

/// Default syntax highlighting theme.
pub const DEFAULT_THEME: &str = "base16-ocean.dark";

/// Holds the combust project configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub source_repo_url: String,
    pub private: bool,

    /// Syntax highlighting theme name (syntect built-in theme).
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Project root directory (set at load time, not serialized).
    #[serde(skip)]
    pub base_dir: PathBuf,

    /// Override for the state directory (used in tests).
    #[serde(skip)]
    pub state_dir_override: Option<PathBuf>,
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}

/// Extracts the project name from an absolute base path (repo basename).
pub fn project_name(abs_base: &Path) -> String {
    abs_base
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

impl Config {
    /// Returns the path to the design directory.
    pub fn design_dir(&self) -> PathBuf {
        self.base_dir.join(COMBUST_DIR).join("design")
    }

    /// Returns the path to the work directory root.
    pub fn work_dir(&self) -> PathBuf {
        self.base_dir.join(COMBUST_DIR).join("work")
    }

    /// Returns the path to the state directory.
    pub fn state_dir(&self) -> Result<PathBuf> {
        if let Some(ref override_dir) = self.state_dir_override {
            return Ok(override_dir.clone());
        }
        Ok(self.base_dir.join(COMBUST_DIR).join("state"))
    }

    /// Returns the path to the config file inside .combust/.
    fn config_path(base: &Path) -> PathBuf {
        combust_path(base).join(CONFIG_FILE)
    }

    /// Saves the configuration to .combust/config.json.
    pub fn save(&self, base: &Path) -> Result<()> {
        let abs_base = fs::canonicalize(base).or_else(|_| {
            Ok::<PathBuf, std::io::Error>(if base.is_absolute() {
                base.to_path_buf()
            } else {
                std::env::current_dir()?.join(base)
            })
        })?;
        let combust = combust_path(&abs_base);
        fs::create_dir_all(&combust).context("creating .combust directory")?;
        let path = combust.join(CONFIG_FILE);
        let data = serde_json::to_string_pretty(self).context("marshaling config")?;
        fs::write(&path, data).context("writing config")?;
        Ok(())
    }
}

/// Returns the path to the .combust directory within `base`.
pub fn combust_path(base: &Path) -> PathBuf {
    base.join(COMBUST_DIR)
}

/// Creates a new combust configuration in the given base directory.
/// If `private` is true, design data lives under ~/.local/share/combust/<basename>/design
/// and a symlink is created at .combust/design.
pub fn init(base: &Path, source_repo_url: &str, private: bool) -> Result<Config> {
    let abs_base = fs::canonicalize(base).or_else(|_| {
        // If the path doesn't exist yet, use the absolute path directly
        Ok::<PathBuf, std::io::Error>(if base.is_absolute() {
            base.to_path_buf()
        } else {
            std::env::current_dir()?.join(base)
        })
    })?;

    let combust = combust_path(&abs_base);
    let design_dir = combust.join("design");
    let work_dir = combust.join("work");

    // Check if already initialized (idempotent re-init)
    let already_initialized = design_dir.exists();

    if !already_initialized {
        if private {
            let data_dir = private_data_dir(&abs_base)?;
            let private_design = data_dir.join("design");
            fs::create_dir_all(&private_design).context("creating private design directory")?;
            fs::create_dir_all(&work_dir).context("creating work directory")?;

            // Remove existing symlink if present.
            if let Ok(meta) = fs::symlink_metadata(&design_dir) {
                if meta.file_type().is_symlink() {
                    fs::remove_file(&design_dir)
                        .context("removing existing design symlink")?;
                } else {
                    bail!(".combust/design already exists and is not a symlink");
                }
            }

            // Ensure .combust directory exists for the symlink
            fs::create_dir_all(&combust).context("creating .combust directory")?;

            #[cfg(unix)]
            std::os::unix::fs::symlink(&private_design, &design_dir)
                .context("creating .combust/design symlink")?;
        } else {
            fs::create_dir_all(&design_dir).context("creating design directory")?;
            fs::create_dir_all(&work_dir).context("creating work directory")?;
        }
    }

    // Always ensure state dir exists inside .combust/
    let state_dir = combust.join("state");
    fs::create_dir_all(&state_dir).context("creating state directory")?;

    let cfg = Config {
        source_repo_url: source_repo_url.to_string(),
        private,
        theme: default_theme(),
        base_dir: abs_base.clone(),
        state_dir_override: None,
    };

    cfg.save(&abs_base)?;

    Ok(cfg)
}

/// Returns the private data directory path.
fn private_data_dir(abs_base: &Path) -> Result<PathBuf> {
    let home = dirs::home_dir().context("getting home directory")?;
    let name = project_name(abs_base);
    Ok(home.join(".local/share/combust").join(name))
}

/// Returns the old XDG config directory for a project (`~/.config/combust/<project_name>/`).
fn xdg_config_dir(abs_base: &Path) -> Result<PathBuf> {
    let home = dirs::home_dir().context("getting home directory")?;
    let name = project_name(abs_base);
    Ok(home.join(".config/combust").join(name))
}

/// Attempts to migrate config and state from the old XDG location to `.combust/`.
/// Returns Ok(true) if migration happened, Ok(false) if no XDG config found.
fn migrate_from_xdg(abs_base: &Path) -> Result<bool> {
    let xdg_dir = xdg_config_dir(abs_base)?;
    let xdg_config = xdg_dir.join(CONFIG_FILE);

    if !xdg_config.exists() {
        return Ok(false);
    }

    let combust = combust_path(abs_base);
    fs::create_dir_all(&combust).context("creating .combust directory for migration")?;

    // Copy config.json
    let dest_config = combust.join(CONFIG_FILE);
    fs::copy(&xdg_config, &dest_config).context("copying config.json from XDG location")?;

    // Copy state/ directory if it exists
    let xdg_state = xdg_dir.join("state");
    if xdg_state.is_dir() {
        let dest_state = combust.join("state");
        copy_dir_recursive(&xdg_state, &dest_state)
            .context("copying state directory from XDG location")?;
    }

    eprintln!(
        "Migrated config from {} to {}",
        xdg_dir.display(),
        combust.display()
    );

    Ok(true)
}

/// Recursively copies a directory tree from `src` to `dst`.
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

/// Reads the configuration from .combust/config.json for the given `base` directory.
/// If `.combust/config.json` is missing, attempts to migrate from the old XDG location.
pub fn load(base: &Path) -> Result<Config> {
    let abs_base = fs::canonicalize(base).context("resolving base path")?;
    let config_path = Config::config_path(&abs_base);

    if !config_path.exists() {
        migrate_from_xdg(&abs_base)?;
    }

    let data = fs::read_to_string(&config_path).context("reading config")?;
    let mut cfg: Config = serde_json::from_str(&data).context("parsing config")?;
    cfg.base_dir = abs_base;
    Ok(cfg)
}

/// Searches upward from the current working directory for a .combust directory,
/// then loads config from .combust/config.json.
pub fn discover() -> Result<Config> {
    let mut dir = std::env::current_dir().context("getting working directory")?;

    loop {
        let cp = combust_path(&dir);
        if cp.exists() && cp.is_dir() {
            return load(&dir);
        }

        let parent = dir.parent().map(|p| p.to_path_buf());
        match parent {
            Some(p) if p != dir => dir = p,
            _ => bail!("no combust configuration found"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_combust_path() {
        let base = Path::new("/tmp/myproject");
        let got = combust_path(base);
        assert_eq!(got, PathBuf::from("/tmp/myproject/.combust"));
    }

    #[test]
    fn test_project_name() {
        assert_eq!(project_name(Path::new("/home/user/myproject")), "myproject");
        assert_eq!(project_name(Path::new("/tmp/foo")), "foo");
    }

    #[test]
    fn test_state_dir_method() {
        let cfg = Config {
            source_repo_url: String::new(),
            private: false,
            theme: default_theme(),
            base_dir: PathBuf::from("/project"),
            state_dir_override: None,
        };
        let state = cfg.state_dir().unwrap();
        assert_eq!(state, PathBuf::from("/project/.combust/state"));
    }

    #[test]
    fn test_state_dir_override() {
        let cfg = Config {
            source_repo_url: String::new(),
            private: false,
            theme: default_theme(),
            base_dir: PathBuf::from("/project"),
            state_dir_override: Some(PathBuf::from("/custom/state")),
        };
        assert_eq!(cfg.state_dir().unwrap(), PathBuf::from("/custom/state"));
    }

    #[test]
    fn test_init_creates_directory_and_config() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let cfg = init(base, "https://github.com/example/repo", false).unwrap();

        assert!(base.join(".combust").is_dir());
        assert!(base.join(".combust/design").is_dir());
        assert!(base.join(".combust/work").is_dir());
        assert!(base.join(".combust/config.json").is_file());
        assert!(base.join(".combust/state").is_dir());
        assert_eq!(cfg.source_repo_url, "https://github.com/example/repo");
        assert!(!cfg.private);
    }

    #[test]
    fn test_init_private() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let cfg = init(base, "https://example.com/repo", true).unwrap();

        assert!(cfg.private);
        // The .combust/design path should be a symlink.
        let meta = fs::symlink_metadata(base.join(".combust/design")).unwrap();
        assert!(meta.file_type().is_symlink());
        // .combust/work should be a real directory
        assert!(base.join(".combust/work").is_dir());
    }

    #[test]
    fn test_init_creates_state_dir() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        init(base, "https://example.com/repo", false).unwrap();

        assert!(base.join(".combust/state").is_dir());
    }

    #[test]
    fn test_init_idempotent() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let cfg1 = init(base, "https://example.com/repo", false).unwrap();

        // Write some design content
        let design_dir = base.join(".combust/design");
        fs::create_dir_all(&design_dir).unwrap();
        fs::write(design_dir.join("rules.md"), "Custom rules").unwrap();

        // Re-init should not overwrite design files
        let cfg2 = init(base, "https://example.com/repo", false).unwrap();

        assert_eq!(cfg1.source_repo_url, cfg2.source_repo_url);
        // Design content should be preserved
        let content = fs::read_to_string(design_dir.join("rules.md")).unwrap();
        assert_eq!(content, "Custom rules");
    }

    #[test]
    fn test_design_dir_method() {
        let cfg = Config {
            source_repo_url: String::new(),
            private: false,
            theme: default_theme(),
            base_dir: PathBuf::from("/project"),
            state_dir_override: None,
        };
        assert_eq!(cfg.design_dir(), PathBuf::from("/project/.combust/design"));
    }

    #[test]
    fn test_work_dir_method() {
        let cfg = Config {
            source_repo_url: String::new(),
            private: false,
            theme: default_theme(),
            base_dir: PathBuf::from("/project"),
            state_dir_override: None,
        };
        assert_eq!(cfg.work_dir(), PathBuf::from("/project/.combust/work"));
    }

    #[test]
    fn test_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let original = init(base, "https://github.com/example/repo", false).unwrap();
        let loaded = load(base).unwrap();

        assert_eq!(loaded.source_repo_url, original.source_repo_url);
        assert_eq!(loaded.private, original.private);
    }

    #[test]
    fn test_load_missing_config() {
        let tmp = TempDir::new().unwrap();
        // Create .combust dir but no config.json
        fs::create_dir_all(tmp.path().join(".combust")).unwrap();
        let err = load(tmp.path()).unwrap_err();
        assert!(
            err.to_string().contains("reading config"),
            "error = {:?}",
            err
        );
    }

    #[test]
    fn test_save_overwrite() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let mut cfg = init(base, "https://original.com/repo", false).unwrap();
        cfg.source_repo_url = "https://updated.com/repo".to_string();
        cfg.save(base).unwrap();

        let loaded = load(base).unwrap();
        assert_eq!(loaded.source_repo_url, "https://updated.com/repo");
    }

    #[test]
    fn test_xdg_migration() {
        let tmp = TempDir::new().unwrap();
        let project_dir = tmp.path().join("myproject");
        fs::create_dir_all(&project_dir).unwrap();

        // Create .combust dir (so discover finds it) but no config.json
        fs::create_dir_all(project_dir.join(".combust")).unwrap();

        // Simulate old XDG config
        let home = dirs::home_dir().unwrap();
        let xdg_dir = home.join(".config/combust/myproject");
        fs::create_dir_all(&xdg_dir).unwrap();

        let config_data = serde_json::to_string_pretty(&serde_json::json!({
            "source_repo_url": "https://example.com/repo",
            "private": false,
            "theme": "base16-ocean.dark"
        }))
        .unwrap();
        fs::write(xdg_dir.join("config.json"), &config_data).unwrap();

        // Also create some state files at old location
        let xdg_state = xdg_dir.join("state/completed");
        fs::create_dir_all(&xdg_state).unwrap();
        fs::write(xdg_state.join("old-task.md"), "Old task content").unwrap();

        // load() should migrate and succeed
        let cfg = load(&project_dir).unwrap();
        assert_eq!(cfg.source_repo_url, "https://example.com/repo");

        // Verify files were copied to .combust/
        assert!(project_dir.join(".combust/config.json").exists());
        assert!(project_dir
            .join(".combust/state/completed/old-task.md")
            .exists());
        let task_content = fs::read_to_string(
            project_dir.join(".combust/state/completed/old-task.md"),
        )
        .unwrap();
        assert_eq!(task_content, "Old task content");

        // Clean up XDG test dir
        let _ = fs::remove_dir_all(&xdg_dir);
    }

    #[test]
    fn test_discover_from_subdir() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        init(base, "https://example.com/repo", false).unwrap();

        let subdir = base.join("deep/nested/dir");
        fs::create_dir_all(&subdir).unwrap();

        // Change into the subdir and discover.
        let old_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&subdir).unwrap();
        let result = discover();
        std::env::set_current_dir(old_dir).unwrap();

        let cfg = result.unwrap();
        assert_eq!(cfg.source_repo_url, "https://example.com/repo");
    }

    #[test]
    fn test_discover_not_found() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("empty");
        fs::create_dir_all(&subdir).unwrap();

        let old_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&subdir).unwrap();
        let result = discover();
        std::env::set_current_dir(old_dir).unwrap();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("no combust configuration found"),
        );
    }

    #[test]
    fn test_discover_by_combust_dir() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        init(base, "https://example.com/repo", false).unwrap();

        let old_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(base).unwrap();
        let result = discover();
        std::env::set_current_dir(old_dir).unwrap();

        assert!(result.is_ok());
    }

    #[test]
    fn test_base_dir_not_serialized() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        init(base, "https://example.com/repo", false).unwrap();

        let data = fs::read_to_string(base.join(".combust/config.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(
            parsed.get("base_dir").is_none(),
            "base_dir should not be serialized"
        );
    }

    #[test]
    fn test_private_mode_design_symlink() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        init(base, "https://example.com/repo", true).unwrap();

        // .combust/design should be a symlink
        let meta = fs::symlink_metadata(base.join(".combust/design")).unwrap();
        assert!(meta.file_type().is_symlink());

        // .combust/work should be a real directory (not a symlink)
        let work_meta = fs::symlink_metadata(base.join(".combust/work")).unwrap();
        assert!(work_meta.file_type().is_dir());
        assert!(!work_meta.file_type().is_symlink());
    }
}
