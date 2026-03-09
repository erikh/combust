use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::config;
use crate::runner::Runner;

/// Local pull request workflow where Claude is the only contributor.
#[derive(Parser, Debug)]
#[command(name = "combust", version, about, long_about = None)]
#[command(
    long_about = "Combust turns markdown design documents into branches, code, and commits. \
    It assembles context from your design docs, hands it to Claude, runs tests and \
    linting, and pushes a branch ready for your review."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Shared flags for autonomous commands.
#[derive(Parser, Debug, Clone)]
pub struct AutonomousFlags {
    /// Model name override
    #[arg(long)]
    pub model: Option<String>,

    /// Disable auto-accept for tool calls
    #[arg(long, short = 'Y')]
    pub no_auto_accept: bool,

    /// Disable plan mode
    #[arg(long, short = 'P')]
    pub no_plan: bool,

    /// Disable notifications
    #[arg(long, short = 'N')]
    pub no_notify: bool,

    /// Force built-in TUI instead of Claude Code CLI
    #[arg(long, short = 'T')]
    pub tui: bool,
}

/// Shared flags for commands that support --no-rebase.
#[derive(Parser, Debug, Clone)]
pub struct ReviewFlags {
    #[command(flatten)]
    pub autonomous: AutonomousFlags,

    /// Skip rebasing before the operation
    #[arg(long)]
    pub no_rebase: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a combust project
    Init {
        /// Store design data in ~/.local/share/combust/ (symlink .combust)
        #[arg(long)]
        private: bool,

        /// Copy Makefile.tmux into the project for tmux-based parallel task execution
        #[arg(long)]
        tmux: bool,

        /// Path to an existing repository (default: current directory)
        #[arg(long)]
        existing: Option<String>,
    },

    /// Execute a design task
    Run {
        /// Task name (e.g., "add-auth" or "group/task")
        task_name: String,

        #[command(flatten)]
        flags: AutonomousFlags,
    },

    /// Manage task groups
    #[command(subcommand)]
    Group(GroupCommands),

    /// Create or edit a design task
    Edit {
        /// Task name (e.g., "add-auth" or "group/task")
        task_name: String,
    },

    /// Manage supplemental design files
    #[command(subcommand)]
    Other(OtherCommands),

    /// Code review workflow
    #[command(subcommand)]
    Review(ReviewCommands),

    /// Add tests for a task in review
    Test {
        /// Task name
        task_name: String,

        #[command(flatten)]
        flags: ReviewFlags,
    },

    /// Run the clean command in a task's work directory
    Clean {
        /// Task name
        task_name: String,
    },

    /// Merge workflow
    #[command(subcommand)]
    Merge(MergeCommands),

    /// Merge completed tasks into functional.md and clean up
    Reconcile {
        #[command(flatten)]
        flags: AutonomousFlags,
    },

    /// Verify all functional.md requirements against the codebase
    Verify {
        #[command(flatten)]
        flags: AutonomousFlags,
    },

    /// Scan for and fix project issues
    Fix {
        /// Auto-confirm fixes without prompting
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Show task states and running tasks
    Status {
        /// Output as JSON instead of YAML
        #[arg(long, short = 'j')]
        json: bool,

        /// Show all fields, including empty states
        #[arg(long, short = 'a')]
        all: bool,
    },

    /// List pending tasks
    List,

    /// Import issues from GitHub/Gitea
    Sync {
        /// Filter by label (can be specified multiple times)
        #[arg(long)]
        label: Vec<String>,
    },

    /// Send a desktop notification
    Notify {
        /// Notification message
        message: String,

        /// Notification title
        #[arg(long, short = 't', default_value = "combust")]
        title: String,
    },

    /// Manage milestones
    #[command(subcommand)]
    Milestone(MilestoneCommands),

    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
pub enum GroupCommands {
    /// List task groups
    List,

    /// List tasks in a group
    Tasks {
        /// Group name
        group_name: String,
    },

    /// Run all pending tasks in a group
    Run {
        /// Group name
        group_name: String,

        #[command(flatten)]
        flags: AutonomousFlags,
    },

    /// Merge all tasks in a group
    Merge {
        /// Group name
        group_name: String,

        #[command(flatten)]
        flags: AutonomousFlags,
    },
}

#[derive(Subcommand, Debug)]
pub enum OtherCommands {
    /// List files in other/
    List,

    /// Add a file to other/
    Add {
        /// File name
        file_name: String,
    },

    /// View a file in other/
    View {
        /// File name
        file_name: String,
    },

    /// Edit a file in other/
    Edit {
        /// File name
        file_name: String,
    },

    /// Remove a file from other/
    Rm {
        /// File name
        file_name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReviewCommands {
    /// List tasks in review
    List,

    /// View a task in review
    View {
        /// Task name
        task_name: String,
    },

    /// Edit a task in review (opens in editor)
    Edit {
        /// Task name
        task_name: String,
    },

    /// Show the diff for a task in review
    Diff {
        /// Task name
        task_name: String,
    },

    /// Remove a task from review (abandon)
    Rm {
        /// Task name
        task_name: String,
    },

    /// Run the dev command in a task's work directory
    Dev {
        /// Task name
        task_name: String,
    },

    /// Run a review session on a task
    Run {
        /// Task name
        task_name: String,

        #[command(flatten)]
        flags: ReviewFlags,
    },
}

#[derive(Subcommand, Debug)]
pub enum MergeCommands {
    /// List tasks in merge state
    List,

    /// View a task in merge state
    View {
        /// Task name
        task_name: String,
    },

    /// Edit a task in merge state
    Edit {
        /// Task name
        task_name: String,
    },

    /// Remove a task from merge (abandon)
    Rm {
        /// Task name
        task_name: String,
    },

    /// Run the merge for a task
    Run {
        /// Task name
        task_name: String,

        #[command(flatten)]
        flags: AutonomousFlags,
    },
}

#[derive(Subcommand, Debug)]
pub enum MilestoneCommands {
    /// Create a new milestone
    New {
        /// Date for the milestone (e.g., 2024-01-15)
        date: Option<String>,
    },

    /// List milestones
    List {
        /// Show only outstanding milestones
        #[arg(long)]
        outstanding: bool,
    },

    /// View a milestone
    View {
        /// Milestone date
        date: String,
    },

    /// Edit a milestone
    Edit {
        /// Milestone date
        date: String,
    },

    /// Verify milestone promises
    Verify {
        /// Milestone date
        date: String,
    },

    /// Create missing task files for a milestone
    Repair {
        /// Milestone date
        date: String,
    },

    /// Mark a milestone as delivered
    Deliver {
        /// Milestone date
        date: String,
    },

    /// View delivery history for a milestone
    History {
        /// Milestone date
        date: String,
    },
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Commands::Init { private, tmux, existing } => {
                let base = existing
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap());
                let repo = crate::git::Repo::open(&base);
                let source_url = repo.remote_url().unwrap_or_default();
                let cfg = config::init(&base, &source_url, private)?;
                crate::design::scaffold(&cfg.design_dir())?;
                config::gitignore::sync_gitignore(&base, private)?;
                if tmux {
                    let makefile_dest = base.join("Makefile.tmux");
                    std::fs::write(&makefile_dest, include_str!("../../contrib/Makefile.tmux"))
                        .context("writing Makefile.tmux")?;
                    println!("Wrote Makefile.tmux");
                }
                println!("Initialized combust project in {}", base.display());
                Ok(())
            }

            Commands::Run { task_name, flags } => {
                let mut r = configure_runner()?;
                apply_autonomous_flags(&mut r, &flags);
                r.auto_accept = !flags.no_auto_accept;
                r.plan_mode = !flags.no_plan;
                r.notify = !flags.no_notify;
                r.run_task(&task_name)
            }

            Commands::Group(cmd) => match cmd {
                GroupCommands::List => {
                    let r = configure_runner()?;
                    let groups = r.group_list()?;
                    if groups.is_empty() {
                        println!("No groups found.");
                    } else {
                        for g in &groups {
                            println!("{}", g);
                        }
                    }
                    Ok(())
                }
                GroupCommands::Tasks { group_name } => {
                    let r = configure_runner()?;
                    let tasks = r.group_tasks(&group_name)?;
                    if tasks.is_empty() {
                        println!("No tasks in group {:?}.", group_name);
                    } else {
                        for t in &tasks {
                            println!("{}", t.label());
                        }
                    }
                    Ok(())
                }
                GroupCommands::Run { group_name, flags } => {
                    let mut r = configure_runner()?;
                    apply_autonomous_flags(&mut r, &flags);
                    r.auto_accept = !flags.no_auto_accept;
                    r.plan_mode = !flags.no_plan;
                    r.notify = !flags.no_notify;
                    r.run_group(&group_name)
                }
                GroupCommands::Merge { group_name, flags } => {
                    let mut r = configure_runner()?;
                    apply_autonomous_flags(&mut r, &flags);
                    r.auto_accept = !flags.no_auto_accept;
                    r.plan_mode = !flags.no_plan;
                    r.notify = !flags.no_notify;
                    r.merge_group(&group_name)
                }
            },

            Commands::Edit { task_name } => {
                let r = configure_runner()?;
                let editor = crate::design::edit::resolve_editor()?;
                crate::design::edit::edit_task(&r.design.path, &task_name, &editor)
            }

            Commands::Other(cmd) => {
                let r = configure_runner()?;
                match cmd {
                    OtherCommands::List => {
                        let files = r.design.other_files()?;
                        if files.is_empty() {
                            println!("No files in other/.");
                        } else {
                            for f in &files {
                                println!("{}", f);
                            }
                        }
                        Ok(())
                    }
                    OtherCommands::Add { file_name } => {
                        let editor = crate::design::edit::resolve_editor()?;
                        let tmp = tempfile::NamedTempFile::new()?;
                        crate::design::edit::run_editor(&editor, tmp.path())?;
                        let content = std::fs::read_to_string(tmp.path())?;
                        if content.trim().is_empty() {
                            println!("Empty file — not created.");
                            return Ok(());
                        }
                        r.design.add_other_file(&file_name, &content)?;
                        println!("Added other/{}", file_name);
                        Ok(())
                    }
                    OtherCommands::View { file_name } => {
                        let content = r.design.other_content(&file_name)?;
                        print!("{}", content);
                        Ok(())
                    }
                    OtherCommands::Edit { file_name } => {
                        let editor = crate::design::edit::resolve_editor()?;
                        let path = r.design.path.join("other").join(&file_name);
                        if !path.exists() {
                            anyhow::bail!("other file {:?} not found", file_name);
                        }
                        crate::design::edit::run_editor(&editor, &path)
                    }
                    OtherCommands::Rm { file_name } => {
                        r.design.remove_other_file(&file_name)?;
                        println!("Removed other/{}", file_name);
                        Ok(())
                    }
                }
            }

            Commands::Review(cmd) => {
                let mut r = configure_runner()?;
                match cmd {
                    ReviewCommands::List => {
                        let tasks = r.review_list()?;
                        if tasks.is_empty() {
                            println!("No tasks in review.");
                        } else {
                            for t in &tasks {
                                println!("{}", t.label());
                            }
                        }
                        Ok(())
                    }
                    ReviewCommands::View { task_name } => {
                        let content = r.review_view(&task_name)?;
                        print!("{}", content);
                        Ok(())
                    }
                    ReviewCommands::Edit { task_name } => {
                        let task = r
                            .design
                            .find_task_by_state(
                                &task_name,
                                crate::design::task::TaskState::Review,
                            )?;
                        let editor = crate::design::edit::resolve_editor()?;
                        crate::design::edit::run_editor(&editor, &task.file_path)
                    }
                    ReviewCommands::Diff { task_name } => {
                        let diff = r.review_diff(&task_name)?;
                        print!("{}", diff);
                        Ok(())
                    }
                    ReviewCommands::Rm { task_name } => {
                        r.review_remove(&task_name)
                    }
                    ReviewCommands::Dev { task_name } => {
                        r.review_dev(&task_name)
                    }
                    ReviewCommands::Run { task_name, flags } => {
                        apply_autonomous_flags(&mut r, &flags.autonomous);
                        r.auto_accept = !flags.autonomous.no_auto_accept;
                        r.plan_mode = !flags.autonomous.no_plan;
                        r.notify = !flags.autonomous.no_notify;
                        r.rebase = !flags.no_rebase;
                        r.review(&task_name)
                    }
                }
            }

            Commands::Test { task_name, flags } => {
                let mut r = configure_runner()?;
                apply_autonomous_flags(&mut r, &flags.autonomous);
                r.auto_accept = !flags.autonomous.no_auto_accept;
                r.plan_mode = !flags.autonomous.no_plan;
                r.notify = !flags.autonomous.no_notify;
                r.rebase = !flags.no_rebase;
                r.test_task(&task_name)
            }

            Commands::Clean { task_name } => {
                let r = configure_runner()?;
                r.clean(&task_name)
            }

            Commands::Merge(cmd) => {
                let mut r = configure_runner()?;
                match cmd {
                    MergeCommands::List => {
                        let tasks = r.merge_list()?;
                        if tasks.is_empty() {
                            println!("No tasks in merge state.");
                        } else {
                            for t in &tasks {
                                println!("{}", t.label());
                            }
                        }
                        Ok(())
                    }
                    MergeCommands::View { task_name } => {
                        let content = r.merge_view(&task_name)?;
                        print!("{}", content);
                        Ok(())
                    }
                    MergeCommands::Edit { task_name } => {
                        let task = r
                            .design
                            .find_task_by_state(
                                &task_name,
                                crate::design::task::TaskState::Merge,
                            )?;
                        let editor = crate::design::edit::resolve_editor()?;
                        crate::design::edit::run_editor(&editor, &task.file_path)
                    }
                    MergeCommands::Rm { task_name } => {
                        r.merge_remove(&task_name)
                    }
                    MergeCommands::Run { task_name, flags } => {
                        apply_autonomous_flags(&mut r, &flags);
                        r.auto_accept = !flags.no_auto_accept;
                        r.plan_mode = !flags.no_plan;
                        r.notify = !flags.no_notify;
                        r.merge_task(&task_name)
                    }
                }
            }

            Commands::Reconcile { flags } => {
                let mut r = configure_runner()?;
                apply_autonomous_flags(&mut r, &flags);
                r.auto_accept = !flags.no_auto_accept;
                r.plan_mode = !flags.no_plan;
                r.notify = !flags.no_notify;
                r.reconcile()
            }

            Commands::Verify { flags } => {
                let mut r = configure_runner()?;
                apply_autonomous_flags(&mut r, &flags);
                r.auto_accept = !flags.no_auto_accept;
                r.plan_mode = !flags.no_plan;
                r.notify = !flags.no_notify;
                r.verify()
            }

            Commands::Fix { yes } => {
                let r = configure_runner()?;
                r.fix(yes)
            }

            Commands::Status { json, all } => {
                let r = configure_runner()?;
                let theme = &r.config.theme;
                let status = r.status()?;
                if all {
                    if json {
                        let output = serde_json::to_string_pretty(&status)?;
                        crate::highlight::print_highlighted(&output, "json", theme);
                        println!();
                    } else {
                        let output = serde_yaml::to_string(&status)?;
                        crate::highlight::print_highlighted(&output, "yaml", theme);
                    }
                } else {
                    let compact = crate::runner::status::CompactProjectStatus::from(&status);
                    if json {
                        let output = serde_json::to_string_pretty(&compact)?;
                        crate::highlight::print_highlighted(&output, "json", theme);
                        println!();
                    } else {
                        let output = serde_yaml::to_string(&compact)?;
                        crate::highlight::print_highlighted(&output, "yaml", theme);
                    }
                }
                Ok(())
            }

            Commands::List => {
                let r = configure_runner()?;
                let tasks = r.list_pending()?;
                if tasks.is_empty() {
                    println!("No pending tasks.");
                } else {
                    for t in &tasks {
                        println!("{}", t.label());
                    }
                }
                Ok(())
            }

            Commands::Sync { label } => {
                let r = configure_runner()?;
                r.sync_issues(&label)
            }

            Commands::Notify { message, title } => {
                crate::notify::send(&title, &message)?;
                println!("Notification sent.");
                Ok(())
            }

            Commands::Milestone(cmd) => {
                let r = configure_runner()?;
                let milestones = crate::design::milestone::Milestones::new(&r.design.path);

                match cmd {
                    MilestoneCommands::New { date } => {
                        let date = date.unwrap_or_else(|| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap();
                            // Simple date from timestamp.
                            format!("{}", now.as_secs())
                        });
                        let date = crate::design::milestone::normalize_date(&date)?;

                        let editor = crate::design::edit::resolve_editor()?;
                        let tmp = tempfile::NamedTempFile::new()?;
                        std::fs::write(tmp.path(), format!(
                            "# Milestone {}\n\n\
                             ## Promises\n\n\
                             - task-name-1\n\
                             - task-name-2\n",
                            date
                        ))?;
                        crate::design::edit::run_editor(&editor, tmp.path())?;
                        let content = std::fs::read_to_string(tmp.path())?;
                        if content.trim().is_empty() {
                            println!("Empty milestone — not created.");
                            return Ok(());
                        }
                        milestones.create(&date, &content)?;
                        println!("Created milestone: {}", date);
                        Ok(())
                    }

                    MilestoneCommands::List { outstanding } => {
                        if outstanding {
                            let dates = milestones.list()?;
                            if dates.is_empty() {
                                println!("No outstanding milestones.");
                            } else {
                                for d in &dates {
                                    println!("{}", d);
                                }
                            }
                        } else {
                            let outstanding = milestones.list()?;
                            let delivered = milestones.delivered()?;

                            if !outstanding.is_empty() {
                                println!("Outstanding:");
                                for d in &outstanding {
                                    println!("  {}", d);
                                }
                            }
                            if !delivered.is_empty() {
                                println!("Delivered:");
                                for d in &delivered {
                                    println!("  {}", d);
                                }
                            }
                            if outstanding.is_empty() && delivered.is_empty() {
                                println!("No milestones.");
                            }
                        }
                        Ok(())
                    }

                    MilestoneCommands::View { date } => {
                        let content = milestones.view(&date)?;
                        print!("{}", content);
                        Ok(())
                    }

                    MilestoneCommands::Edit { date } => {
                        let path = milestones.path(&date)?;
                        let editor = crate::design::edit::resolve_editor()?;
                        crate::design::edit::run_editor(&editor, &path)
                    }

                    MilestoneCommands::Verify { date } => {
                        let result = milestones.verify(&date, &r.design)?;
                        for promise in &result.promises {
                            let status = if promise.completed { "OK" } else { "MISSING" };
                            println!("  [{}] {}", status, promise.heading);
                        }
                        if result.all_met {
                            println!("\nAll promises met.");
                        } else {
                            println!("\nSome promises are not yet completed.");
                        }
                        Ok(())
                    }

                    MilestoneCommands::Repair { date } => {
                        let result = milestones.repair(&date, &r.design)?;
                        if result.created.is_empty() {
                            println!("No missing tasks — nothing to repair.");
                        } else {
                            println!("Created {} task file(s):", result.created.len());
                            for name in &result.created {
                                println!("  {}", name);
                            }
                        }
                        Ok(())
                    }

                    MilestoneCommands::Deliver { date } => {
                        milestones.deliver(&date)?;
                        println!("Milestone {} marked as delivered.", date);
                        Ok(())
                    }

                    MilestoneCommands::History { date } => {
                        let h = milestones.history(&date)?;
                        println!("Date: {}", h.date);
                        println!("Delivered at: {}", h.delivered_at);
                        println!("---");
                        print!("{}", h.content);
                        Ok(())
                    }
                }
            }

            Commands::Completion { shell } => {
                use clap::CommandFactory;
                clap_complete::generate(
                    shell,
                    &mut Cli::command(),
                    "combust",
                    &mut std::io::stdout(),
                );
                Ok(())
            }
        }
    }
}

fn configure_runner() -> Result<Runner> {
    let cfg = config::discover()?;
    let mut r = Runner::new(cfg)?;
    r.base_dir = std::env::current_dir()?;
    Ok(r)
}

fn apply_autonomous_flags(r: &mut Runner, flags: &AutonomousFlags) {
    if let Some(ref m) = flags.model {
        r.model = m.clone();
    }
    r.force_tui = flags.tui;
}
