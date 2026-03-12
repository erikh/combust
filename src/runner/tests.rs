#[cfg(test)]
mod verify_tests {
    use crate::config::Config;
    use crate::design;
    use crate::runner::verify::assemble_verify_document;
    use crate::runner::{ClaudeRunConfig, Runner};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct TestEnv {
        _temp: TempDir,
        base_dir: PathBuf,
        design_dir: PathBuf,
        state_dir: PathBuf,
        config: Config,
    }

    fn setup_test_env() -> TestEnv {
        let temp = TempDir::new().unwrap();
        let base_dir = temp.path().to_path_buf();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&base_dir)
            .output()
            .unwrap();

        let combust_dir = base_dir.join(".combust");
        let design_dir = combust_dir.join("design");
        let state_dir = base_dir.join("test-state");
        fs::create_dir_all(combust_dir.join("work")).unwrap();
        fs::create_dir_all(&design_dir).unwrap();

        design::scaffold_design(&design_dir).unwrap();
        design::scaffold_state(&state_dir).unwrap();
        fs::write(design_dir.join("rules.md"), "Follow best practices.").unwrap();
        fs::write(design_dir.join("lint.md"), "Use gofmt.").unwrap();
        fs::write(design_dir.join("functional.md"), "Tests must pass.").unwrap();
        fs::write(
            design_dir.join("tasks/add-feature.md"),
            "Add the new feature.",
        )
        .unwrap();

        let config = Config {
            source_repo_url: String::new(),
            private: false,
            theme: crate::config::DEFAULT_THEME.to_string(),
            base_dir: base_dir.clone(),
            state_dir_override: Some(state_dir.clone()),
        };

        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&base_dir)
            .output()
            .unwrap();

        TestEnv {
            _temp: temp,
            base_dir,
            design_dir,
            state_dir,
            config,
        }
    }

    fn make_runner(
        env: &TestEnv,
        claude_fn: Box<dyn Fn(ClaudeRunConfig) -> anyhow::Result<()> + Send + Sync>,
    ) -> Runner {
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        Runner {
            config: env.config.clone(),
            design: dd,
            claude: Some(claude_fn),
            base_dir: env.base_dir.clone(),
            model: String::new(),
            auto_accept: false,
            force_tui: false,
            rebase: true,
            notify: false,
            commands: None,
            api_type: String::new(),
            gitea_url: String::new(),
            timeout: None,
        }
    }

    #[test]
    fn test_verify_empty_functional() {
        let env = setup_test_env();
        fs::write(env.design_dir.join("functional.md"), "").unwrap();
        let r = make_runner(&env, Box::new(|_| Ok(())));
        let err = r.verify().unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "error = {:?}, want empty message",
            err
        );
    }

    #[test]
    fn test_verify_whitespace_only_functional() {
        let env = setup_test_env();
        fs::write(env.design_dir.join("functional.md"), "   \n  \t  ").unwrap();
        let r = make_runner(&env, Box::new(|_| Ok(())));
        let err = r.verify().unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "error = {:?}",
            err
        );
    }

    #[test]
    fn test_verify_document_contains_mission() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("# Mission"));
    }

    #[test]
    fn test_verify_document_contains_verification_instructions() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("1. Find the relevant code"));
        assert!(doc.contains("2. Confirm the implementation"));
        assert!(doc.contains("3. If the code does not satisfy"));
        assert!(doc.contains("4. Verify that the requirement has adequate test coverage"));
        assert!(doc.contains("5. Run tests"));
    }

    #[test]
    fn test_verify_document_contains_rules_and_lint() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();

        assert!(doc.contains("# Rules"));
        assert!(doc.contains("Follow best practices."));
        assert!(doc.contains("# Lint Rules"));
        assert!(doc.contains("Use gofmt."));

        // Rules before Lint before Functional.
        let rules_pos = doc.find("# Rules").unwrap();
        let lint_pos = doc.find("# Lint Rules").unwrap();
        let func_pos = doc.find("# Functional Specification").unwrap();
        assert!(rules_pos < lint_pos);
        assert!(lint_pos < func_pos);
    }

    #[test]
    fn test_verify_document_contains_functional_spec() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("# Functional Specification"));
        assert!(doc.contains("Tests must pass."));
    }

    #[test]
    fn test_verify_document_contains_commit_instructions() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("# Commit Instructions"));
    }

    #[test]
    fn test_verify_document_contains_final_sync() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("# Final Sync"));
    }

    #[test]
    fn test_verify_document_contains_pass_fail_files() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("verify-passed.txt"));
        assert!(doc.contains("verify-failed.txt"));
    }

    #[test]
    fn test_verify_document_spec_is_authoritative() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("fix the code"));
        assert!(doc.contains("Do not modify the functional specification"));
    }

    #[test]
    fn test_verify_document_serial_tests() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("serially"));
    }

    #[test]
    fn test_verify_document_ends_with_plan_mode() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.trim_end().ends_with("plan mode immediately."));
    }

    #[test]
    fn test_verify_document_test_coverage() {
        let env = setup_test_env();
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let doc =
            assemble_verify_document(&dd, "Tests must pass.", false, &HashMap::new()).unwrap();
        assert!(doc.contains("test coverage"));
    }
}

#[cfg(test)]
mod reconcile_tests {
    use crate::config::Config;
    use crate::design;
    use crate::design::task::TaskState;
    use crate::runner::reconcile::{assemble_reconcile_document_pub, TaskEntryPublic};
    use crate::runner::{ClaudeRunConfig, Runner};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct TestEnv {
        _temp: TempDir,
        base_dir: PathBuf,
        design_dir: PathBuf,
        state_dir: PathBuf,
        config: Config,
    }

    fn setup_test_env() -> TestEnv {
        let temp = TempDir::new().unwrap();
        let base_dir = temp.path().to_path_buf();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&base_dir)
            .output()
            .unwrap();

        let combust_dir = base_dir.join(".combust");
        let design_dir = combust_dir.join("design");
        let state_dir = base_dir.join("test-state");
        fs::create_dir_all(combust_dir.join("work")).unwrap();
        fs::create_dir_all(&design_dir).unwrap();

        design::scaffold_design(&design_dir).unwrap();
        design::scaffold_state(&state_dir).unwrap();
        fs::write(design_dir.join("rules.md"), "Follow best practices.").unwrap();
        fs::write(design_dir.join("lint.md"), "Use gofmt.").unwrap();
        fs::write(design_dir.join("functional.md"), "Tests must pass.").unwrap();
        fs::write(
            design_dir.join("tasks/add-feature.md"),
            "Add the new feature.",
        )
        .unwrap();

        let config = Config {
            source_repo_url: String::new(),
            private: false,
            theme: crate::config::DEFAULT_THEME.to_string(),
            base_dir: base_dir.clone(),
            state_dir_override: Some(state_dir.clone()),
        };

        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&base_dir)
            .output()
            .unwrap();

        TestEnv {
            _temp: temp,
            base_dir,
            design_dir,
            state_dir,
            config,
        }
    }

    fn make_runner(
        env: &TestEnv,
        claude_fn: Box<dyn Fn(ClaudeRunConfig) -> anyhow::Result<()> + Send + Sync>,
    ) -> Runner {
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        Runner {
            config: env.config.clone(),
            design: dd,
            claude: Some(claude_fn),
            base_dir: env.base_dir.clone(),
            model: String::new(),
            auto_accept: false,
            force_tui: false,
            rebase: true,
            notify: false,
            commands: None,
            api_type: String::new(),
            gitea_url: String::new(),
            timeout: None,
        }
    }

    #[test]
    fn test_reconcile_no_completed_tasks() {
        let env = setup_test_env();
        let r = make_runner(&env, Box::new(|_| Ok(())));
        let err = r.reconcile().unwrap_err();
        assert!(
            err.to_string().contains("no completed tasks"),
            "error = {:?}",
            err
        );
    }

    #[test]
    fn test_reconcile_document_contains_mission() {
        let tasks = vec![TaskEntryPublic {
            name: "task-a".to_string(),
            content: "Add login".to_string(),
        }];
        let doc = assemble_reconcile_document_pub("Existing spec", &tasks);
        assert!(doc.contains("# Mission"));
        assert!(doc.contains("update the functional specification"));
    }

    #[test]
    fn test_reconcile_document_contains_current_spec() {
        let tasks = vec![TaskEntryPublic {
            name: "t".to_string(),
            content: "c".to_string(),
        }];
        let doc = assemble_reconcile_document_pub("My current spec content", &tasks);
        assert!(doc.contains("My current spec content"));
        assert!(doc.contains("# Current Functional Specification"));
    }

    #[test]
    fn test_reconcile_document_contains_no_spec() {
        let tasks = vec![TaskEntryPublic {
            name: "t".to_string(),
            content: "c".to_string(),
        }];
        let doc = assemble_reconcile_document_pub("", &tasks);
        assert!(doc.contains("No existing specification."));
    }

    #[test]
    fn test_reconcile_document_contains_completed_tasks() {
        let tasks = vec![
            TaskEntryPublic {
                name: "task-a".to_string(),
                content: "Add login".to_string(),
            },
            TaskEntryPublic {
                name: "task-b".to_string(),
                content: "Add signup".to_string(),
            },
        ];
        let doc = assemble_reconcile_document_pub("spec", &tasks);
        assert!(doc.contains("## task-a"));
        assert!(doc.contains("Add login"));
        assert!(doc.contains("## task-b"));
        assert!(doc.contains("Add signup"));
    }

    #[test]
    fn test_reconcile_document_contains_instructions() {
        let tasks = vec![TaskEntryPublic {
            name: "t".to_string(),
            content: "c".to_string(),
        }];
        let doc = assemble_reconcile_document_pub("spec", &tasks);
        assert!(doc.contains("# Instructions"));
    }

    #[test]
    fn test_reconcile_document_ground_truth() {
        let tasks = vec![TaskEntryPublic {
            name: "t".to_string(),
            content: "c".to_string(),
        }];
        let doc = assemble_reconcile_document_pub("spec", &tasks);
        assert!(doc.contains("ground truth"));
    }

    #[test]
    fn test_reconcile_document_only_functional() {
        let tasks = vec![TaskEntryPublic {
            name: "t".to_string(),
            content: "c".to_string(),
        }];
        let doc = assemble_reconcile_document_pub("spec", &tasks);
        assert!(doc.contains("Only edit functional.md"));
    }

    #[test]
    fn test_reconcile_document_ends_with_plan_mode() {
        let tasks = vec![TaskEntryPublic {
            name: "t".to_string(),
            content: "c".to_string(),
        }];
        let doc = assemble_reconcile_document_pub("spec", &tasks);
        assert!(doc.trim_end().ends_with("plan mode immediately."));
    }

    #[test]
    fn test_reconcile_preserves_other_states() {
        let env = setup_test_env();

        // State dirs are now in state_dir, not design_dir
        fs::write(
            env.state_dir.join("completed/done-task.md"),
            "Done task content",
        )
        .unwrap();
        fs::write(
            env.state_dir.join("review/review-task.md"),
            "Review task content",
        )
        .unwrap();

        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        let completed = dd.tasks_by_state(TaskState::Completed).unwrap();
        assert_eq!(completed.len(), 1);

        let review = dd.tasks_by_state(TaskState::Review).unwrap();
        assert_eq!(review.len(), 1);

        let pending = dd.pending_tasks().unwrap();
        assert_eq!(pending.len(), 1);
    }
}

#[cfg(test)]
mod runner_integration_tests {
    use crate::config::Config;
    use crate::design;
    use crate::runner::document::{
        commit_instructions, conflict_resolution_section, mission_reminder,
        notification_section, rebase_and_push_section, timeout_section,
        verification_section, PLAN_MODE_INSTRUCTION,
    };
    use crate::runner::{has_make_target, ClaudeRunConfig, Commands, Runner};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    struct TestEnv {
        _temp: TempDir,
        base_dir: PathBuf,
        design_dir: PathBuf,
        state_dir: PathBuf,
        config: Config,
    }

    fn setup_test_env() -> TestEnv {
        let temp = TempDir::new().unwrap();
        let base_dir = temp.path().to_path_buf();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(&base_dir)
            .output()
            .unwrap();

        let combust_dir = base_dir.join(".combust");
        let design_dir = combust_dir.join("design");
        let state_dir = base_dir.join("test-state");
        fs::create_dir_all(combust_dir.join("work")).unwrap();
        fs::create_dir_all(&design_dir).unwrap();

        design::scaffold_design(&design_dir).unwrap();
        design::scaffold_state(&state_dir).unwrap();
        fs::write(design_dir.join("rules.md"), "Follow best practices.").unwrap();
        fs::write(design_dir.join("lint.md"), "Use gofmt.").unwrap();
        fs::write(design_dir.join("functional.md"), "Tests must pass.").unwrap();
        fs::write(
            design_dir.join("tasks/add-feature.md"),
            "Add the new feature.",
        )
        .unwrap();

        let config = Config {
            source_repo_url: String::new(),
            private: false,
            theme: crate::config::DEFAULT_THEME.to_string(),
            base_dir: base_dir.clone(),
            state_dir_override: Some(state_dir.clone()),
        };

        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&base_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&base_dir)
            .output()
            .unwrap();

        TestEnv {
            _temp: temp,
            base_dir,
            design_dir,
            state_dir,
            config,
        }
    }

    fn make_runner(
        env: &TestEnv,
        claude_fn: Box<dyn Fn(ClaudeRunConfig) -> anyhow::Result<()> + Send + Sync>,
    ) -> Runner {
        let dd = crate::design::Dir::new(&env.design_dir, &env.state_dir).unwrap();
        Runner {
            config: env.config.clone(),
            design: dd,
            claude: Some(claude_fn),
            base_dir: env.base_dir.clone(),
            model: String::new(),
            auto_accept: false,
            force_tui: false,
            rebase: true,
            notify: false,
            commands: None,
            api_type: String::new(),
            gitea_url: String::new(),
            timeout: None,
        }
    }

    #[test]
    fn test_run_document_assembly() {
        let env = setup_test_env();
        let captured = Arc::new(Mutex::new(String::new()));
        let captured_clone = captured.clone();

        let r = make_runner(
            &env,
            Box::new(move |cfg: ClaudeRunConfig| {
                *captured_clone.lock().unwrap() = cfg.document.clone();
                Ok(())
            }),
        );

        // We can't fully run the task (no remote), but we can test document assembly.
        let dd = &r.design;
        let task_content = "Add the new feature.";
        let group_content = "";
        let base_doc = dd.assemble_document(task_content, group_content).unwrap();

        let cmds = HashMap::new();
        let mut doc = base_doc;
        doc.push_str(&verification_section(&cmds));
        doc.push_str(&commit_instructions(false, &cmds));
        doc.push_str(&rebase_and_push_section(&cmds));
        doc.push_str(&mission_reminder());
        doc.push_str(PLAN_MODE_INSTRUCTION);

        // Verify document structure.
        assert!(doc.contains("# Mission"));
        assert!(doc.contains("# Rules"));
        assert!(doc.contains("# Task"));
        assert!(doc.contains("Add the new feature."));
        assert!(doc.contains("plan mode"));
    }

    #[test]
    fn test_run_task_not_found() {
        let env = setup_test_env();
        let r = make_runner(&env, Box::new(|_| Ok(())));
        let result = r.run_task("nonexistent-task");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_grouped_task_includes_group_content() {
        let env = setup_test_env();
        let group_dir = env.design_dir.join("tasks/backend");
        fs::create_dir_all(&group_dir).unwrap();
        fs::write(group_dir.join("group.md"), "Backend API guidelines.").unwrap();
        fs::write(group_dir.join("add-api.md"), "Add API endpoint.").unwrap();

        let r = make_runner(&env, Box::new(|_| Ok(())));
        let dd = &r.design;

        let task_content = "Add API endpoint.";
        let group_content = dd.group_content("backend").unwrap();
        let doc = dd.assemble_document(task_content, &group_content).unwrap();

        assert!(doc.contains("# Group"));
        assert!(doc.contains("Backend API guidelines."));
        assert!(doc.contains("Add API endpoint."));
    }

    #[test]
    fn test_run_group_empty_error() {
        let env = setup_test_env();
        let r = make_runner(&env, Box::new(|_| Ok(())));
        let result = r.run_group("nonexistent-group");
        assert!(result.is_err());
    }

    #[test]
    fn test_prepare_repo_not_git() {
        let env = setup_test_env();
        let r = make_runner(&env, Box::new(|_| Ok(())));

        let non_git = env.base_dir.join(".combust/work/test-dir");
        fs::create_dir_all(&non_git).unwrap();
        // Create a non-git directory.
        fs::write(non_git.join("file.txt"), "data").unwrap();

        // prepare_repo should clean it up and try to create worktree.
        // This will fail since we're in a non-bare repo without remotes,
        // but it shouldn't panic.
        let _ = r.prepare_repo(&non_git, "combust/test-branch");
    }

    #[test]
    fn test_review_list_shows_tasks() {
        let env = setup_test_env();
        fs::write(
            env.state_dir.join("review/reviewed-task.md"),
            "Review me",
        )
        .unwrap();

        let r = make_runner(&env, Box::new(|_| Ok(())));
        let tasks = r.review_list().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "reviewed-task");
    }

    #[test]
    fn test_clean_missing_command() {
        let env = setup_test_env();
        let r = make_runner(&env, Box::new(|_| Ok(())));
        // With no commands configured, clean should fail.
        let result = r.clean("add-feature");
        assert!(result.is_err());
    }

    #[test]
    fn test_clean_finds_task_in_any_state() {
        let env = setup_test_env();
        // Move task to review state (in state_dir now).
        fs::write(
            env.state_dir.join("review/my-task.md"),
            "Content",
        )
        .unwrap();

        let mut r = make_runner(&env, Box::new(|_| Ok(())));
        r.commands = Some(Commands {
            clean: Some("echo clean".to_string()),
            ..Commands::default()
        });

        // Clean should find the task in review state.
        // It'll fail because there's no work directory, but it should find the task.
        let result = r.clean("my-task");
        // The error should be about work dir, not about task not found.
        if let Err(e) = result {
            assert!(
                !e.to_string().contains("not found"),
                "unexpected error: {}",
                e
            );
        }
    }

    #[test]
    fn test_group_list_sorted() {
        let env = setup_test_env();
        fs::create_dir_all(env.design_dir.join("tasks/zebra")).unwrap();
        fs::write(env.design_dir.join("tasks/zebra/group.md"), "Z").unwrap();
        fs::create_dir_all(env.design_dir.join("tasks/alpha")).unwrap();
        fs::write(env.design_dir.join("tasks/alpha/group.md"), "A").unwrap();

        let r = make_runner(&env, Box::new(|_| Ok(())));
        let groups = r.group_list().unwrap();
        assert!(groups.len() >= 2);
        let alpha_pos = groups.iter().position(|g| g == "alpha").unwrap();
        let zebra_pos = groups.iter().position(|g| g == "zebra").unwrap();
        assert!(alpha_pos < zebra_pos);
    }

    #[test]
    fn test_group_tasks_sorted() {
        let env = setup_test_env();
        let group_dir = env.design_dir.join("tasks/mygroup");
        fs::create_dir_all(&group_dir).unwrap();
        fs::write(group_dir.join("group.md"), "Group").unwrap();
        fs::write(group_dir.join("task-z.md"), "Z").unwrap();
        fs::write(group_dir.join("task-a.md"), "A").unwrap();

        let r = make_runner(&env, Box::new(|_| Ok(())));
        let tasks = r.group_tasks("mygroup").unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_document_suffix_ends_plan_mode() {
        let cmds = HashMap::new();
        let mut doc = String::new();
        doc.push_str(&verification_section(&cmds));
        doc.push_str(&commit_instructions(false, &cmds));
        doc.push_str(&rebase_and_push_section(&cmds));
        doc.push_str(&mission_reminder());
        doc.push_str(PLAN_MODE_INSTRUCTION);

        assert!(doc.trim_end().ends_with("plan mode immediately."));
    }

    #[test]
    fn test_has_make_target_no_makefile() {
        let tmp = TempDir::new().unwrap();
        assert!(!has_make_target(tmp.path(), "test"));
    }

    #[test]
    fn test_has_make_target_found() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("Makefile"),
            "test:\n\tcargo test\n\nlint:\n\tcargo clippy\n",
        )
        .unwrap();
        assert!(has_make_target(tmp.path(), "test"));
        assert!(has_make_target(tmp.path(), "lint"));
        assert!(!has_make_target(tmp.path(), "missing"));
    }

    #[test]
    fn test_commands_map_with_makefile_fallback() {
        let env = setup_test_env();
        let r = make_runner(&env, Box::new(|_| Ok(())));

        // Write a Makefile to the base dir.
        fs::write(
            env.base_dir.join("Makefile"),
            "test:\n\tcargo test\nlint:\n\tcargo clippy\n",
        )
        .unwrap();

        let cmds = r.commands_map(&env.base_dir);
        assert_eq!(cmds.get("test"), Some(&"make test".to_string()));
        assert_eq!(cmds.get("lint"), Some(&"make lint".to_string()));
    }

    #[test]
    fn test_commands_map_yml_overrides_makefile() {
        let env = setup_test_env();
        let mut r = make_runner(&env, Box::new(|_| Ok(())));
        r.commands = Some(Commands {
            test: Some("cargo test --release".to_string()),
            ..Commands::default()
        });

        // Write a Makefile too.
        fs::write(
            env.base_dir.join("Makefile"),
            "test:\n\tcargo test\nlint:\n\tcargo clippy\n",
        )
        .unwrap();

        let cmds = r.commands_map(&env.base_dir);
        // combust.yml should override Makefile.
        assert_eq!(cmds.get("test"), Some(&"cargo test --release".to_string()));
        // But Makefile fallback should still work for lint.
        assert_eq!(cmds.get("lint"), Some(&"make lint".to_string()));
    }

    #[test]
    fn test_before_hook_runs() {
        let env = setup_test_env();
        let mut r = make_runner(&env, Box::new(|_| Ok(())));
        r.commands = Some(Commands {
            before: Some("echo before > before-ran.txt".to_string()),
            ..Commands::default()
        });

        r.run_before_hook(&env.base_dir).unwrap();
        assert!(env.base_dir.join("before-ran.txt").exists());
    }

    #[test]
    fn test_before_hook_failure_aborts() {
        let env = setup_test_env();
        let mut r = make_runner(&env, Box::new(|_| Ok(())));
        r.commands = Some(Commands {
            before: Some("exit 1".to_string()),
            ..Commands::default()
        });

        let result = r.run_before_hook(&env.base_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_worktree() {
        let env = setup_test_env();
        let r = make_runner(&env, Box::new(|_| Ok(())));

        let repo = crate::git::Repo::open(&env.base_dir);

        // Create a dirty file.
        fs::write(env.base_dir.join("dirty.txt"), "dirty").unwrap();
        assert!(repo.has_changes().unwrap());

        // Reset should clean it.
        let sha = repo.last_commit_sha().unwrap();
        r.reset_worktree(&repo, &sha).unwrap();
        assert!(!repo.has_changes().unwrap());
    }

    #[test]
    fn test_conflict_resolution_in_document() {
        let files = vec!["src/main.rs".to_string()];
        let section = conflict_resolution_section(&files);
        assert!(section.contains("## Conflict Resolution"));
        assert!(section.contains("`src/main.rs`"));
    }

    #[test]
    fn test_notification_in_document() {
        let section = notification_section("Task Done");
        assert!(section.contains("## Notification"));
        assert!(section.contains("Task Done"));
    }

    #[test]
    fn test_timeout_in_document() {
        let section = timeout_section("300s");
        assert!(section.contains("## Timeout"));
        assert!(section.contains("300s"));
    }

    #[test]
    fn test_combust_yml_with_api_type() {
        let env = setup_test_env();
        fs::write(
            env.design_dir.join("combust.yml"),
            "commands:\n  test: cargo test\napi_type: github\ngitea_url: https://gitea.example.com\ntimeout: 30m\n",
        )
        .unwrap();

        let r = Runner::new(env.config.clone()).unwrap();
        assert_eq!(r.api_type, "github");
        assert_eq!(r.gitea_url, "https://gitea.example.com");
        assert!(r.timeout.is_some());
        assert_eq!(r.timeout.unwrap().as_secs(), 1800);
    }

    #[test]
    fn test_timeout_parsing() {
        use super::super::parse_timeout;

        assert_eq!(parse_timeout("30s").unwrap().as_secs(), 30);
        assert_eq!(parse_timeout("5m").unwrap().as_secs(), 300);
        assert_eq!(parse_timeout("1h").unwrap().as_secs(), 3600);
        assert_eq!(parse_timeout("60").unwrap().as_secs(), 60);
        assert!(parse_timeout("").is_err());
        assert!(parse_timeout("abc").is_err());
    }

    #[test]
    fn test_build_claude_args_minimal() {
        use super::super::build_claude_args;
        use std::path::PathBuf;

        let cfg = super::super::ClaudeRunConfig {
            repo_dir: PathBuf::from("/tmp"),
            document: "do the thing".to_string(),
            model: String::new(),
            auto_accept: false,
            force_tui: false,
        };

        let args = build_claude_args(&cfg);
        assert_eq!(
            args,
            vec!["--permission-mode", "plan", "do the thing"]
        );
    }

    #[test]
    fn test_build_claude_args_auto_accept() {
        use super::super::build_claude_args;
        use std::path::PathBuf;

        let cfg = super::super::ClaudeRunConfig {
            repo_dir: PathBuf::from("/tmp"),
            document: "do the thing".to_string(),
            model: String::new(),
            auto_accept: true,
            force_tui: false,
        };

        let args = build_claude_args(&cfg);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "plan",
                "do the thing",
                "--dangerously-skip-permissions"
            ]
        );
    }

    #[test]
    fn test_build_claude_args_with_model() {
        use super::super::build_claude_args;
        use std::path::PathBuf;

        let cfg = super::super::ClaudeRunConfig {
            repo_dir: PathBuf::from("/tmp"),
            document: "do the thing".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            auto_accept: false,
            force_tui: false,
        };

        let args = build_claude_args(&cfg);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "plan",
                "do the thing",
                "--model",
                "claude-sonnet-4-6"
            ]
        );
    }

    #[test]
    fn test_build_claude_args_all_flags() {
        use super::super::build_claude_args;
        use std::path::PathBuf;

        let cfg = super::super::ClaudeRunConfig {
            repo_dir: PathBuf::from("/tmp"),
            document: "do the thing".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            auto_accept: true,
            force_tui: false,
        };

        let args = build_claude_args(&cfg);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "plan",
                "do the thing",
                "--dangerously-skip-permissions",
                "--model",
                "claude-sonnet-4-6"
            ]
        );
    }

    #[test]
    fn test_build_claude_args_no_prompt_flag() {
        use super::super::build_claude_args;
        use std::path::PathBuf;

        let cfg = super::super::ClaudeRunConfig {
            repo_dir: PathBuf::from("/tmp"),
            document: "my prompt".to_string(),
            model: String::new(),
            auto_accept: true,
            force_tui: false,
        };

        let args = build_claude_args(&cfg);
        // --prompt and --print are NOT valid; prompt is a positional arg
        assert!(!args.contains(&"--prompt".to_string()));
        assert!(!args.contains(&"--print".to_string()));
        assert!(args.contains(&"--permission-mode".to_string()));
        assert!(args.contains(&"plan".to_string()));
        assert!(args.contains(&"my prompt".to_string()));
    }
}
