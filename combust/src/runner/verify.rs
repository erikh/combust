use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;

use super::document::{
    commit_instructions, rebase_and_push_section, verification_section,
    PLAN_MODE_INSTRUCTION,
};
use super::{ClaudeRunConfig, Runner};

impl Runner {
    /// Verifies that all items in functional.md are satisfied by the current codebase.
    pub fn verify(&self) -> Result<()> {
        let base_dir = &self.base_dir;

        // Read functional.md.
        let functional = self.design.functional().context("reading functional.md")?;
        if functional.trim().is_empty() {
            bail!("functional.md is empty; nothing to verify");
        }

        // Prepare work directory.
        let wd = base_dir
            .join(combust_db::config::COMBUST_DIR)
            .join("work")
            .join("_verify");
        let verify_repo = self
            .prepare_repo(&wd, "combust/_verify")
            .context("preparing work directory")?;

        // Fetch and reset to a clean state.
        verify_repo.fetch().context("fetching origin")?;
        let default_branch = self
            .detect_default_branch(&verify_repo)
            .context("detecting default branch")?;
        self.reset_worktree(&verify_repo, &format!("origin/{}", default_branch))
            .context("resetting work directory")?;

        // Run before hook.
        self.run_before_hook(&wd).context("before hook")?;

        // Assemble document.
        let sign = verify_repo.has_signing_key();
        let cmds = self.commands_map(&wd);
        let doc = assemble_verify_document(&self.design, &functional, sign, &cmds)?;

        // Capture HEAD before invoking Claude.
        let before_sha = verify_repo
            .last_commit_sha()
            .context("getting HEAD SHA")?;

        // Invoke Claude.
        let cfg = ClaudeRunConfig {
            repo_dir: wd.clone(),
            document: doc,
            model: self.model.clone(),
            auto_accept: self.auto_accept,
            force_tui: self.force_tui,
        };
        self.invoke_claude(cfg).context("claude failed")?;

        // Check for verify-passed.txt or verify-failed.txt.
        let passed_path = wd.join("verify-passed.txt");
        let failed_path = wd.join("verify-failed.txt");

        if passed_path.exists() {
            println!("All functional requirements verified.");

            push_verify_fixes(self, &verify_repo, &before_sha)?;

            return Ok(());
        }

        if failed_path.exists() {
            let data =
                fs::read_to_string(&failed_path).context("reading verify-failed.txt")?;
            println!("Verification failed:");
            println!("{}", data);
            bail!("functional requirements verification failed");
        }

        bail!("claude did not produce verify-passed.txt or verify-failed.txt");
    }
}

/// Builds the prompt for the verify workflow.
pub(crate) fn assemble_verify_document(
    design: &combust_db::design::DesignDir,
    functional: &str,
    sign: bool,
    cmds: &HashMap<String, String>,
) -> Result<String> {
    let rules = design.rules()?;
    let lint = design.lint()?;

    let mut b = String::new();

    b.push_str(
        "# Mission\n\nYour objective is to verify that every requirement in the functional specification \
         below is satisfied by the current codebase. If code does not match the specification, fix the code.\n\n",
    );

    if !rules.is_empty() {
        b.push_str("# Rules\n\n");
        b.push_str(&rules);
        b.push_str("\n\n");
    }
    if !lint.is_empty() {
        b.push_str("# Lint Rules\n\n");
        b.push_str(&lint);
        b.push_str("\n\n");
    }

    b.push_str("# Functional Specification\n\n");
    b.push_str(functional);
    b.push_str("\n\n");

    b.push_str("# Verification Instructions\n\n");
    b.push_str("For each requirement in the specification above:\n");
    b.push_str("1. Find the relevant code that implements it\n");
    b.push_str("2. Confirm the implementation matches the specification\n");
    b.push_str(
        "3. If the code does not satisfy a requirement, fix the code to match the specification\n",
    );
    b.push_str(
        "4. Verify that the requirement has adequate test coverage — there should be tests that exercise the described behavior, including edge cases and error paths\n",
    );
    b.push_str("5. Run tests according to the combust.yml test task, serially\n\n");

    b.push_str(&verification_section(cmds));

    b.push_str(
        "\nIf ALL requirements are satisfied, all have adequate test coverage, and all tests pass, \
         create a file called `verify-passed.txt` containing \"PASS\" and nothing else.\n\n",
    );

    b.push_str(
        "If ANY requirement is NOT satisfied or lacks adequate test coverage, \
         create a file called `verify-failed.txt` listing each failed requirement and why it failed \
         (including any that lack tests).\n\n",
    );

    b.push_str(
        "Do not modify the functional specification. \
         The specification is the source of truth — if code does not match the specification, fix the code.\n",
    );

    b.push_str(&commit_instructions(sign, cmds));
    b.push_str(&rebase_and_push_section(cmds));

    b.push_str("\n# Reminder\n\n");
    b.push_str(
        "The functional specification is authoritative. Fix code to match it, never the reverse. \
         Commit your changes, then create verify-passed.txt or verify-failed.txt when done.\n",
    );

    b.push_str(PLAN_MODE_INSTRUCTION);
    Ok(b)
}

/// Rebases and pushes if Claude committed changes during verify.
fn push_verify_fixes(
    runner: &Runner,
    verify_repo: &crate::git::Repo,
    before_sha: &str,
) -> Result<()> {
    let after_sha = verify_repo
        .last_commit_sha()
        .context("getting HEAD SHA after verify")?;
    if after_sha == before_sha {
        return Ok(());
    }

    verify_repo
        .fetch()
        .context("fetching origin before push")?;
    let default_branch = runner
        .detect_default_branch(verify_repo)
        .context("detecting default branch")?;
    verify_repo
        .rebase(&format!("origin/{}", default_branch))
        .with_context(|| {
            format!(
                "rebasing against origin/{} before push",
                default_branch
            )
        })?;
    verify_repo
        .push_main()
        .context("pushing")?;
    println!("Pushed verify fixes to origin.");
    Ok(())
}
