pub mod gitignore;

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

/// Returns the XDG config directory for a combust project.
pub fn xdg_project_dir(abs_base: &Path) -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("getting config directory")?;
    let name = project_name(abs_base);
    Ok(config_dir.join("combust").join(name))
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
        let xdg = xdg_project_dir(&self.base_dir)?;
        Ok(xdg.join("state"))
    }

    /// Saves the configuration to the XDG config location.
    pub fn save(&self, base: &Path) -> Result<()> {
        let abs_base = fs::canonicalize(base).or_else(|_| {
            Ok::<PathBuf, std::io::Error>(if base.is_absolute() {
                base.to_path_buf()
            } else {
                std::env::current_dir()?.join(base)
            })
        })?;
        let xdg = xdg_project_dir(&abs_base)?;
        fs::create_dir_all(&xdg).context("creating XDG config directory")?;
        let path = xdg.join(CONFIG_FILE);
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

    // Always ensure XDG state dirs exist
    let xdg = xdg_project_dir(&abs_base)?;
    let state_dir = xdg.join("state");
    fs::create_dir_all(&state_dir).context("creating XDG state directory")?;

    let cfg = Config {
        source_repo_url: source_repo_url.to_string(),
        private,
        theme: default_theme(),
        base_dir: abs_base.clone(),
        state_dir_override: None,
    };

    // Only save config if not already initialized
    if !already_initialized {
        cfg.save(&abs_base)?;
    }

    Ok(cfg)
}

/// Returns the private data directory path.
fn private_data_dir(abs_base: &Path) -> Result<PathBuf> {
    let home = dirs::home_dir().context("getting home directory")?;
    let name = project_name(abs_base);
    Ok(home.join(".local/share/combust").join(name))
}

/// Reads the configuration from the XDG config location for the given `base` directory.
pub fn load(base: &Path) -> Result<Config> {
    let abs_base = fs::canonicalize(base).context("resolving base path")?;
    let xdg = xdg_project_dir(&abs_base)?;
    let config_path = xdg.join(CONFIG_FILE);
    let data = fs::read_to_string(&config_path).context("reading config")?;
    let mut cfg: Config = serde_json::from_str(&data).context("parsing config")?;
    cfg.base_dir = abs_base;
    Ok(cfg)
}

/// Searches upward from the current working directory for a .combust directory,
/// then loads config from XDG.
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

    /// Helper that inits with a state_dir_override so tests don't pollute real XDG dirs.
    fn test_init(base: &Path, url: &str, private: bool) -> Result<Config> {
        let abs_base = fs::canonicalize(base).or_else(|_| {
            Ok::<PathBuf, std::io::Error>(if base.is_absolute() {
                base.to_path_buf()
            } else {
                std::env::current_dir()?.join(base)
            })
        })?;

        let combust = combust_path(&abs_base);
        let design_dir = combust.join("design");
        let work_dir = combust.join("work");

        if private {
            // For tests, create private design in a subdir of base instead of real ~/.local/share
            let private_design = abs_base.join("private-data/design");
            fs::create_dir_all(&private_design)?;
            fs::create_dir_all(&work_dir)?;
            fs::create_dir_all(&combust)?;

            if let Ok(meta) = fs::symlink_metadata(&design_dir) {
                if meta.file_type().is_symlink() {
                    fs::remove_file(&design_dir)?;
                }
            }

            #[cfg(unix)]
            std::os::unix::fs::symlink(&private_design, &design_dir)?;
        } else {
            fs::create_dir_all(&design_dir)?;
            fs::create_dir_all(&work_dir)?;
        }

        // Use a test-local state dir
        let state_dir = abs_base.join("test-xdg/state");
        fs::create_dir_all(&state_dir)?;

        let cfg = Config {
            source_repo_url: url.to_string(),
            private,
            theme: default_theme(),
            base_dir: abs_base.clone(),
            state_dir_override: Some(state_dir),
        };

        // Save config to test-local XDG dir
        let xdg = abs_base.join("test-xdg");
        fs::create_dir_all(&xdg)?;
        let path = xdg.join(CONFIG_FILE);
        let data = serde_json::to_string_pretty(&cfg)?;
        fs::write(&path, data)?;

        Ok(cfg)
    }

    /// Loads config from test-local XDG dir.
    fn test_load(base: &Path) -> Result<Config> {
        let abs_base = fs::canonicalize(base).context("resolving base path")?;
        let xdg = abs_base.join("test-xdg");
        let config_path = xdg.join(CONFIG_FILE);
        let data = fs::read_to_string(&config_path).context("reading config")?;
        let mut cfg: Config = serde_json::from_str(&data).context("parsing config")?;
        cfg.base_dir = abs_base.clone();
        cfg.state_dir_override = Some(abs_base.join("test-xdg/state"));
        Ok(cfg)
    }

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
    fn test_xdg_project_dir() {
        let dir = xdg_project_dir(Path::new("/home/user/myproject")).unwrap();
        let config_dir = dirs::config_dir().unwrap();
        assert_eq!(dir, config_dir.join("combust/myproject"));
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
        let config_dir = dirs::config_dir().unwrap();
        assert_eq!(state, config_dir.join("combust/project/state"));
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

        let cfg = test_init(base, "https://github.com/example/repo", false).unwrap();

        assert!(base.join(".combust").is_dir());
        assert!(base.join(".combust/design").is_dir());
        assert!(base.join(".combust/work").is_dir());
        assert!(base.join("test-xdg/config.json").is_file());
        assert!(base.join("test-xdg/state").is_dir());
        assert_eq!(cfg.source_repo_url, "https://github.com/example/repo");
        assert!(!cfg.private);
    }

    #[test]
    fn test_init_private() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let cfg = test_init(base, "https://example.com/repo", true).unwrap();

        assert!(cfg.private);
        // The .combust/design path should be a symlink.
        let meta = fs::symlink_metadata(base.join(".combust/design")).unwrap();
        assert!(meta.file_type().is_symlink());
        // .combust/work should be a real directory
        assert!(base.join(".combust/work").is_dir());
    }

    #[test]
    fn test_init_creates_xdg_state() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        test_init(base, "https://example.com/repo", false).unwrap();

        assert!(base.join("test-xdg/state").is_dir());
    }

    #[test]
    fn test_init_idempotent() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let cfg1 = test_init(base, "https://example.com/repo", false).unwrap();

        // Write some design content
        let design_dir = base.join(".combust/design");
        fs::create_dir_all(&design_dir).unwrap();
        fs::write(design_dir.join("rules.md"), "Custom rules").unwrap();

        // Re-init should not overwrite design files
        let cfg2 = test_init(base, "https://example.com/repo", false).unwrap();

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

        let original = test_init(base, "https://github.com/example/repo", false).unwrap();
        let loaded = test_load(base).unwrap();

        assert_eq!(loaded.source_repo_url, original.source_repo_url);
        assert_eq!(loaded.private, original.private);
    }

    #[test]
    fn test_load_missing_config() {
        let tmp = TempDir::new().unwrap();
        let err = test_load(tmp.path()).unwrap_err();
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

        let mut cfg = test_init(base, "https://original.com/repo", false).unwrap();
        cfg.source_repo_url = "https://updated.com/repo".to_string();

        // Save to test-local XDG
        let xdg = base.join("test-xdg");
        fs::create_dir_all(&xdg).unwrap();
        let path = xdg.join(CONFIG_FILE);
        let data = serde_json::to_string_pretty(&cfg).unwrap();
        fs::write(&path, data).unwrap();

        let loaded = test_load(base).unwrap();
        assert_eq!(loaded.source_repo_url, "https://updated.com/repo");
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

        // Create .combust dir and XDG config
        init(base, "https://example.com/repo", false).unwrap();

        // Verify discover finds it via .combust directory
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

        test_init(base, "https://example.com/repo", false).unwrap();

        let data = fs::read_to_string(base.join("test-xdg/config.json")).unwrap();
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

        test_init(base, "https://example.com/repo", true).unwrap();

        // .combust/design should be a symlink
        let meta = fs::symlink_metadata(base.join(".combust/design")).unwrap();
        assert!(meta.file_type().is_symlink());

        // .combust/work should be a real directory (not a symlink)
        let work_meta = fs::symlink_metadata(base.join(".combust/work")).unwrap();
        assert!(work_meta.file_type().is_dir());
        assert!(!work_meta.file_type().is_symlink());
    }
}
