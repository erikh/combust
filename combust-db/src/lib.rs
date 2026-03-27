#![deny(dead_code)]

pub mod config;
pub mod design;
pub mod gitignore;
pub mod lock;
pub mod milestone;
pub mod migration;
pub mod record;
pub mod revision;
pub mod task;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use config::Config;
use design::DesignDir;
use lock::{Lock, RunningTask};
use milestone::Milestones;
use record::Record;

/// Top-level facade for a `.combust/` directory tree.
pub struct CombustDir {
    pub config: Config,
    pub design: DesignDir,
    pub record: Record,
    pub milestones: Milestones,
    base_dir: PathBuf,
}

impl CombustDir {
    /// Opens an existing `.combust/` directory, running migrations if needed.
    pub fn open(base_dir: &Path) -> Result<Self> {
        let cfg = config::load(base_dir)?;

        let combust_path = config::combust_path(&cfg.base_dir);
        revision::migrate_if_needed(&combust_path)?;

        let state_dir = cfg.state_dir()?;
        std::fs::create_dir_all(&state_dir).context("creating state directory")?;
        let design = DesignDir::new(&cfg.design_dir(), &state_dir)?;
        let record = Record::new(&state_dir);
        let milestones = Milestones::new(&design.path);

        Ok(CombustDir {
            base_dir: cfg.base_dir.clone(),
            config: cfg,
            design,
            record,
            milestones,
        })
    }

    /// Initializes a new `.combust/` directory.
    pub fn init(base_dir: &Path, url: &str, private: bool) -> Result<Self> {
        let cfg = config::init(base_dir, url, private)?;

        let design_dir = cfg.design_dir();
        if !design_dir.join("rules.md").exists() {
            design::scaffold_design(&design_dir)?;
        }

        let state_dir = cfg.state_dir()?;
        design::scaffold_state(&state_dir)?;

        // Write initial revision file
        let combust_path = config::combust_path(&cfg.base_dir);
        revision::write_revision(&combust_path, revision::CURRENT_REVISION)?;

        let design = DesignDir::new(&design_dir, &state_dir)?;
        let record = Record::new(&state_dir);
        let milestones = Milestones::new(&design.path);

        Ok(CombustDir {
            base_dir: cfg.base_dir.clone(),
            config: cfg,
            design,
            record,
            milestones,
        })
    }

    /// Walks up from the current working directory to find and open a `.combust/` directory.
    pub fn discover() -> Result<Self> {
        let cfg = config::discover()?;
        let base_dir = cfg.base_dir.clone();
        drop(cfg);
        Self::open(&base_dir)
    }

    /// Returns a Lock for the given task name.
    pub fn lock(&self, task_name: &str) -> Lock {
        Lock::new(&self.combust_path(), task_name)
    }

    /// Returns all currently running tasks.
    pub fn running_tasks(&self) -> Result<Vec<RunningTask>> {
        lock::read_all(&self.combust_path())
    }

    /// Returns the work directory path.
    pub fn work_dir(&self) -> PathBuf {
        self.config.work_dir()
    }

    /// Returns the `.combust/` directory path.
    pub fn combust_path(&self) -> PathBuf {
        config::combust_path(&self.base_dir)
    }

    /// Returns the base directory path.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}
