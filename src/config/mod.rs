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
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
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

    /// Saves the configuration to the .combust directory in `base`.
    pub fn save(&self, base: &Path) -> Result<()> {
        let path = config_path(base);
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).context("creating config directory")?;
        let data = serde_json::to_string_pretty(self).context("marshaling config")?;
        fs::write(&path, data).context("writing config")?;
        Ok(())
    }
}

/// Returns the path to the .combust directory within `base`.
pub fn combust_path(base: &Path) -> PathBuf {
    base.join(COMBUST_DIR)
}

/// Returns the path to the config file within `base`.
fn config_path(base: &Path) -> PathBuf {
    combust_path(base).join(CONFIG_FILE)
}

/// Creates a new combust configuration in the given base directory.
/// If `private` is true, data lives under ~/.local/share/combust/<basename>/
/// and a symlink is created at .combust.
pub fn init(base: &Path, source_repo_url: &str, private: bool) -> Result<Config> {
    let abs_base = fs::canonicalize(base).or_else(|_| {
        // If the path doesn't exist yet, use the absolute path directly
        Ok::<PathBuf, std::io::Error>(
            if base.is_absolute() {
                base.to_path_buf()
            } else {
                std::env::current_dir()?.join(base)
            },
        )
    })?;

    let combust = combust_path(&abs_base);

    if private {
        let data_dir = private_data_dir(&abs_base)?;

        for sub in &["design", "work"] {
            fs::create_dir_all(data_dir.join(sub))
                .with_context(|| format!("creating {} directory", sub))?;
        }

        // Remove existing symlink if present.
        if let Ok(meta) = fs::symlink_metadata(&combust) {
            if meta.file_type().is_symlink() {
                fs::remove_file(&combust).context("removing existing .combust symlink")?;
            } else {
                bail!(".combust already exists and is not a symlink");
            }
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(&data_dir, &combust)
            .context("creating .combust symlink")?;
    } else {
        for sub in &["design", "work"] {
            fs::create_dir_all(combust.join(sub))
                .with_context(|| format!("creating {} directory", sub))?;
        }
    }

    let cfg = Config {
        source_repo_url: source_repo_url.to_string(),
        private,
        theme: default_theme(),
        base_dir: abs_base.clone(),
    };

    cfg.save(&abs_base)?;
    Ok(cfg)
}

/// Returns the private data directory path.
fn private_data_dir(abs_base: &Path) -> Result<PathBuf> {
    let home = dirs::home_dir().context("getting home directory")?;
    let name = abs_base
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(home.join(".local/share/combust").join(name))
}

/// Reads the configuration from the .combust directory in `base`.
pub fn load(base: &Path) -> Result<Config> {
    let abs_base = fs::canonicalize(base).context("resolving base path")?;
    let data = fs::read_to_string(config_path(&abs_base)).context("reading config")?;
    let mut cfg: Config = serde_json::from_str(&data).context("parsing config")?;
    cfg.base_dir = abs_base;
    Ok(cfg)
}

/// Searches upward from the current working directory for a .combust/config.json file.
pub fn discover() -> Result<Config> {
    let mut dir = std::env::current_dir().context("getting working directory")?;

    loop {
        let cp = config_path(&dir);
        if cp.exists() {
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
    fn test_init_creates_directory_and_config() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let cfg = init(base, "https://github.com/example/repo", false).unwrap();

        assert!(base.join(".combust").is_dir());
        assert!(base.join(".combust/design").is_dir());
        assert!(base.join(".combust/work").is_dir());
        assert!(base.join(".combust/config.json").is_file());
        assert_eq!(cfg.source_repo_url, "https://github.com/example/repo");
        assert!(!cfg.private);

        // Verify config.json is valid JSON.
        let data = fs::read_to_string(base.join(".combust/config.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["source_repo_url"], "https://github.com/example/repo");
    }

    #[test]
    fn test_init_private() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let cfg = init(base, "https://example.com/repo", true).unwrap();

        assert!(cfg.private);
        // The .combust path should be a symlink.
        let meta = fs::symlink_metadata(base.join(".combust")).unwrap();
        assert!(meta.file_type().is_symlink());
    }

    #[test]
    fn test_design_dir_method() {
        let cfg = Config {
            source_repo_url: String::new(),
            private: false,
            theme: default_theme(),
            base_dir: PathBuf::from("/project"),
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
            result.unwrap_err().to_string().contains("no combust configuration found"),
        );
    }

    #[test]
    fn test_base_dir_not_serialized() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        init(base, "https://example.com/repo", false).unwrap();

        let data = fs::read_to_string(config_path(base)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(
            parsed.get("base_dir").is_none(),
            "base_dir should not be serialized"
        );
    }
}
