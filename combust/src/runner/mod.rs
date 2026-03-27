pub mod clean;
pub mod document;
pub mod fix;
pub mod merge;
pub mod reconcile;
pub mod review;
pub mod run;
pub mod status;
pub mod task_test;
#[cfg(test)]
mod tests;
pub mod verify;

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use combust_db::config::Config;
use combust_db::design::{self, DesignDir};
use crate::git::Repo;

/// Parameters for a Claude invocation.
#[derive(Debug, Clone)]
pub struct ClaudeRunConfig {
    pub repo_dir: PathBuf,
    pub document: String,
    pub model: String,
    pub auto_accept: bool,
    pub force_tui: bool,
}

/// Function signature for invoking claude.
pub type ClaudeFn = Box<dyn Fn(ClaudeRunConfig) -> Result<()> + Send + Sync>;

/// Shared flags for autonomous commands (verify, reconcile, etc.).
#[derive(Debug, Clone, Default)]
pub struct SharedFlags {
    pub model: String,
    pub auto_accept: bool,
    pub force_tui: bool,
}

/// Orchestrates the full combust run workflow.
pub struct Runner {
    pub config: Config,
    pub design: DesignDir,
    pub claude: Option<ClaudeFn>,
    pub base_dir: PathBuf,
    pub model: String,
    pub auto_accept: bool,
    pub force_tui: bool,
    pub rebase: bool,
    pub notify: bool,
    pub commands: Option<Commands>,
    pub api_type: String,
    pub gitea_url: String,
    pub timeout: Option<std::time::Duration>,
}

/// Parsed commands from combust.yml.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CombustYml {
    #[serde(default)]
    pub commands: Option<CommandsConfig>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub api_type: Option<String>,
    #[serde(default)]
    pub gitea_url: Option<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub notify: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CommandsConfig {
    pub before: Option<String>,
    pub clean: Option<String>,
    pub dev: Option<String>,
    pub lint: Option<String>,
    pub test: Option<String>,
    pub teardown: Option<String>,
}

/// Resolved command set (from combust.yml or Makefile fallback).
#[derive(Debug, Clone, Default)]
pub struct Commands {
    pub before: Option<String>,
    pub clean: Option<String>,
    pub dev: Option<String>,
    pub lint: Option<String>,
    pub test: Option<String>,
    pub teardown: Option<String>,
}

impl Commands {
    /// Returns the effective commands map for document assembly.
    pub fn as_map(&self) -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        if let Some(ref v) = self.test {
            m.insert("test".to_string(), v.clone());
        }
        if let Some(ref v) = self.lint {
            m.insert("lint".to_string(), v.clone());
        }
        if let Some(ref v) = self.before {
            m.insert("before".to_string(), v.clone());
        }
        if let Some(ref v) = self.clean {
            m.insert("clean".to_string(), v.clone());
        }
        if let Some(ref v) = self.dev {
            m.insert("dev".to_string(), v.clone());
        }
        m
    }

    /// Runs the "before" command in the given work directory.
    pub fn run_before(&self, work_dir: &Path) -> Result<()> {
        if let Some(ref cmd) = self.before {
            run_shell_command(cmd, work_dir)?;
        }
        Ok(())
    }

    /// Runs the "teardown" command in the given work directory.
    pub fn run_teardown(&self, work_dir: &Path) {
        if let Some(ref cmd) = self.teardown {
            if let Err(e) = run_shell_command(cmd, work_dir) {
                eprintln!(
                    "Warning: teardown failed in {}: {}",
                    work_dir.display(),
                    e
                );
            }
        }
    }
}

impl Runner {
    /// Creates a Runner from the given config.
    pub fn new(cfg: Config) -> Result<Self> {
        let state_dir = cfg.state_dir()?;
        fs::create_dir_all(&state_dir).context("creating state directory")?;
        let dd = DesignDir::new(&cfg.design_dir(), &state_dir)?;

        let (commands, api_type, gitea_url, timeout) = load_combust_yml(&cfg)?;

        let model = String::new();

        Ok(Runner {
            config: cfg,
            design: dd,
            claude: None,
            base_dir: PathBuf::from("."),
            model,
            auto_accept: false,
            force_tui: false,
            rebase: true,
            notify: false,
            commands,
            api_type,
            gitea_url,
            timeout,
        })
    }

    /// Returns the configured timeout duration, if any.
    pub fn timeout(&self) -> Option<std::time::Duration> {
        self.timeout
    }

    /// Returns the commands map for document assembly.
    /// Falls back to Makefile targets for commands not configured in combust.yml.
    pub fn commands_map(&self, work_dir: &Path) -> std::collections::HashMap<String, String> {
        let mut m = self
            .commands
            .as_ref()
            .map(|c| c.as_map())
            .unwrap_or_default();

        // Makefile fallback for missing commands.
        let makefile_targets = ["before", "clean", "dev", "test", "lint"];
        for target in &makefile_targets {
            if !m.contains_key(*target) && has_make_target(work_dir, target) {
                m.insert(target.to_string(), format!("make {}", target));
            }
        }

        m
    }

    /// Runs the "before" hook from combust.yml.
    pub fn run_before_hook(&self, work_dir: &Path) -> Result<()> {
        if let Some(ref cmds) = self.commands {
            cmds.run_before(work_dir)?;
        }
        Ok(())
    }

    /// Detects the default branch (main or master).
    pub fn detect_default_branch(&self, repo: &Repo) -> Result<String> {
        if repo.branch_exists("origin/main") {
            return Ok("main".to_string());
        }
        if repo.branch_exists("origin/master") {
            return Ok("master".to_string());
        }
        anyhow::bail!("cannot detect default branch (neither main nor master found)");
    }

    /// Prepares the work directory for a task using git worktrees.
    pub fn prepare_repo(&self, work_dir: &Path, branch_name: &str) -> Result<Repo> {
        // Try to reuse existing work directory.
        if work_dir.is_dir() && Repo::is_git_repo(work_dir) {
            let repo = Repo::open(work_dir);
            if repo.fetch().is_ok() {
                return Ok(repo);
            }
            eprintln!(
                "Warning: resync of {} failed, re-creating worktree",
                work_dir.display()
            );
            // Clean up and fall through.
            if let Some(ref cmds) = self.commands {
                cmds.run_teardown(work_dir);
            }
            let main_repo = Repo::open(&self.config.base_dir);
            let _ = main_repo.worktree_remove(work_dir);
            let _ = fs::remove_dir_all(work_dir);
        } else if work_dir.is_dir() {
            // Directory exists but not a git repo.
            if let Some(ref cmds) = self.commands {
                cmds.run_teardown(work_dir);
            }
            let _ = fs::remove_dir_all(work_dir);
        }

        // Create parent directories.
        if let Some(parent) = work_dir.parent() {
            fs::create_dir_all(parent).context("creating work dir parent")?;
        }

        // Open the main repo and create a worktree.
        let main_repo = Repo::open(&self.config.base_dir);
        let _ = main_repo.fetch();

        if main_repo.branch_exists(branch_name) {
            main_repo
                .worktree_add_existing(work_dir, branch_name)
                .context("creating worktree for existing branch")?;
        } else {
            main_repo
                .worktree_add(work_dir, branch_name)
                .context("creating worktree")?;
        }

        Ok(Repo::open(work_dir))
    }

    /// Resets a worktree to a clean state on the given remote ref.
    pub fn reset_worktree(&self, repo: &Repo, remote_ref: &str) -> Result<()> {
        let _ = repo.rebase_abort();
        repo.reset_hard(remote_ref)
            .with_context(|| format!("resetting to {}", remote_ref))?;
        repo.clean().context("cleaning working tree")?;
        Ok(())
    }

    /// Invokes Claude with the given config.
    pub fn invoke_claude(&self, cfg: ClaudeRunConfig) -> Result<()> {
        if let Some(ref claude_fn) = self.claude {
            return claude_fn(cfg);
        }
        invoke_claude_cli(cfg)
    }
}

/// Returns true if the Makefile in the given directory has the specified target.
pub fn has_make_target(work_dir: &Path, name: &str) -> bool {
    let makefile = work_dir.join("Makefile");
    if !makefile.exists() {
        return false;
    }
    let content = match fs::read_to_string(&makefile) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Check for a line starting with "target:" (make target pattern).
    let target_pattern = format!("{}:", name);
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(&target_pattern)
    })
}

/// Parses a timeout string into a Duration.
/// Supports formats: "30s", "5m", "1h", plain seconds "60".
pub(crate) fn parse_timeout(s: &str) -> Result<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("empty timeout");
    }

    if let Some(secs) = s.strip_suffix('s') {
        let n: u64 = secs.parse().context("parsing timeout seconds")?;
        return Ok(std::time::Duration::from_secs(n));
    }
    if let Some(mins) = s.strip_suffix('m') {
        let n: u64 = mins.parse().context("parsing timeout minutes")?;
        return Ok(std::time::Duration::from_secs(n * 60));
    }
    if let Some(hours) = s.strip_suffix('h') {
        let n: u64 = hours.parse().context("parsing timeout hours")?;
        return Ok(std::time::Duration::from_secs(n * 3600));
    }

    // Assume plain seconds.
    let n: u64 = s.parse().context("parsing timeout as seconds")?;
    Ok(std::time::Duration::from_secs(n))
}

/// Loads commands from combust.yml in the design directory.
/// Returns (commands, api_type, gitea_url, timeout).
fn load_combust_yml(
    cfg: &Config,
) -> Result<(Option<Commands>, String, String, Option<std::time::Duration>)> {
    let yml_path = cfg.design_dir().join("combust.yml");

    // Ensure combust.yml exists.
    design::ensure_combust_yml(&cfg.design_dir())?;

    let content = match fs::read_to_string(&yml_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((None, String::new(), String::new(), None));
        }
        Err(e) => return Err(e).context("reading combust.yml"),
    };

    let parsed: CombustYml = serde_yaml::from_str(&content).context("parsing combust.yml")?;

    let cmds = parsed.commands.map(|c| Commands {
        before: c.before,
        clean: c.clean,
        dev: c.dev,
        lint: c.lint,
        test: c.test,
        teardown: c.teardown,
    });

    let api_type = parsed.api_type.unwrap_or_default();
    let gitea_url = parsed.gitea_url.unwrap_or_default();
    let timeout = match parsed.timeout {
        Some(ref t) if !t.is_empty() => Some(parse_timeout(t)?),
        _ => None,
    };

    Ok((cmds, api_type, gitea_url, timeout))
}

/// Build the argument list for a Claude CLI invocation.
fn build_claude_args(cfg: &ClaudeRunConfig) -> Vec<String> {
    let mut args = vec![
        "--permission-mode".to_string(),
        "plan".to_string(),
        cfg.document.clone(),
    ];

    if cfg.auto_accept {
        args.push("--dangerously-skip-permissions".to_string());
    }

    if !cfg.model.is_empty() {
        args.push("--model".to_string());
        args.push(cfg.model.clone());
    }

    args
}

/// Default Claude CLI invocation.
fn invoke_claude_cli(cfg: ClaudeRunConfig) -> Result<()> {
    let claude_bin = which::which("claude").context(
        "claude binary not found; install Claude Code CLI or set PATH",
    )?;

    let args = build_claude_args(&cfg);
    let mut cmd = std::process::Command::new(claude_bin);
    cmd.current_dir(&cfg.repo_dir);
    cmd.args(&args);

    let status = cmd.status().context("running claude")?;

    if !status.success() {
        anyhow::bail!("claude exited with status {}", status);
    }

    Ok(())
}

/// Runs a shell command in the given work directory.
pub fn run_shell_command(cmd: &str, work_dir: &Path) -> Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let status = std::process::Command::new(&shell)
        .arg("-c")
        .arg(cmd)
        .current_dir(work_dir)
        .status()
        .with_context(|| format!("running command: {}", cmd))?;

    if !status.success() {
        anyhow::bail!("command failed: {}", cmd);
    }
    Ok(())
}

#[cfg(test)]
mod runner_unit_tests {
    use super::*;

    #[test]
    fn test_commands_as_map() {
        let cmds = Commands {
            before: Some("make deps".to_string()),
            clean: Some("make clean".to_string()),
            dev: Some("npm run dev".to_string()),
            lint: Some("cargo clippy".to_string()),
            test: Some("cargo test".to_string()),
            teardown: None,
        };
        let m = cmds.as_map();
        assert_eq!(m.get("before").unwrap(), "make deps");
        assert_eq!(m.get("clean").unwrap(), "make clean");
        assert_eq!(m.get("dev").unwrap(), "npm run dev");
        assert_eq!(m.get("lint").unwrap(), "cargo clippy");
        assert_eq!(m.get("test").unwrap(), "cargo test");
        // teardown is not included in as_map.
        assert!(!m.contains_key("teardown"));
    }

    #[test]
    fn test_commands_as_map_empty() {
        let cmds = Commands::default();
        let m = cmds.as_map();
        assert!(m.is_empty());
    }

    #[test]
    fn test_commands_as_map_partial() {
        let cmds = Commands {
            test: Some("pytest".to_string()),
            ..Default::default()
        };
        let m = cmds.as_map();
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("test").unwrap(), "pytest");
    }

    #[test]
    fn test_combust_yml_parsing() {
        let yaml = r#"
commands:
  before: "make deps"
  lint: "cargo clippy"
  test: "cargo test"
"#;
        let parsed: CombustYml = serde_yaml::from_str(yaml).unwrap();
        let cmds = parsed.commands.unwrap();
        assert_eq!(cmds.before.unwrap(), "make deps");
        assert_eq!(cmds.lint.unwrap(), "cargo clippy");
        assert_eq!(cmds.test.unwrap(), "cargo test");
    }

    #[test]
    fn test_combust_yml_parsing_empty() {
        let yaml = "# empty config\n";
        let parsed: CombustYml = serde_yaml::from_str(yaml).unwrap();
        assert!(parsed.commands.is_none());
    }

    #[test]
    fn test_combust_yml_default_creation() {
        let tmp = tempfile::TempDir::new().unwrap();
        let design_dir = tmp.path();
        design::ensure_combust_yml(design_dir).unwrap();

        let content = fs::read_to_string(design_dir.join("combust.yml")).unwrap();
        assert!(content.contains("commands:"));
        assert!(content.contains("lint:"));
        assert!(content.contains("test:"));
    }
}
