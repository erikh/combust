use std::collections::HashMap;

/// Appended to every workflow document so Claude starts in plan mode.
pub const PLAN_MODE_INSTRUCTION: &str = "\nPlease enter plan mode immediately.\n";

/// Returns a markdown section listing the test and lint commands Claude should run.
pub fn verification_section(commands: &HashMap<String, String>) -> String {
    let test_cmd = commands.get("test").map(|s| s.as_str()).unwrap_or("");
    let lint_cmd = commands.get("lint").map(|s| s.as_str()).unwrap_or("");

    if test_cmd.is_empty() && lint_cmd.is_empty() {
        return String::new();
    }

    let mut b = String::new();
    b.push_str("\n## Verification\n\n");
    b.push_str(
        "Before committing, ensure all checks pass. \
         The commands below are the project's official test and lint commands from combust.yml. \
         Do not run other commands to perform testing or linting. \
         Only run the exact commands listed below, fix any issues they report, and repeat until they pass.\n\n",
    );

    if !test_cmd.is_empty() {
        b.push_str(&format!("- Run tests: `{}`\n", test_cmd));
    }
    if !lint_cmd.is_empty() {
        b.push_str(&format!("- Run linter: `{}`\n", lint_cmd));
    }

    b.push_str(
        "\nIMPORTANT: Multiple combust tasks may run concurrently, each in its own \
         work directory. Do not modify these commands to use fixed ports, shared temp files, \
         or any global state that would conflict with parallel runs. \
         All test and lint operations must be fully isolated to the current working tree.\n",
    );
    b
}

/// Returns commit instructions for Claude.
pub fn commit_instructions(sign: bool, commands: &HashMap<String, String>) -> String {
    let mut b = String::new();
    b.push_str("\n\n# Commit Instructions\n\n");

    b.push_str(
        "IMPORTANT: Do NOT run any individual test files, test functions, \
         lint checks, or any other testing/linting tools manually. \
         The ONLY test and lint commands you may run are the exact commands listed below \
         from combust.yml. Do not invoke test runners, linters, or type checkers in any other way.\n\n",
    );

    b.push_str("After making all code changes, follow the steps below.\n\n");

    let mut step = 1;
    if let Some(test_cmd) = commands.get("test") {
        if !test_cmd.is_empty() {
            b.push_str(&format!(
                "{}. Run the test suite: `{}`\n",
                step, test_cmd
            ));
            step += 1;
        }
    }
    if let Some(lint_cmd) = commands.get("lint") {
        if !lint_cmd.is_empty() {
            b.push_str(&format!("{}. Run the linter: `{}`\n", step, lint_cmd));
            step += 1;
        }
    }

    b.push_str(&format!("{}. Stage all changes: `git add -A`\n", step));
    step += 1;
    b.push_str(&format!(
        "{}. Commit with a descriptive message. ",
        step
    ));

    if sign {
        b.push_str("Sign the commit: `git commit -S -m \"<descriptive message>\"`\n");
    } else {
        b.push_str("Commit: `git commit -m \"<descriptive message>\"`\n");
    }

    b.push_str(
        "\nIMPORTANT: You MUST commit your changes before finishing. \
         The commit message should describe what was done, not just the task name. \
         Do NOT add Co-Authored-By or any other trailers to the commit message.\n",
    );

    b
}

/// Returns a markdown section instructing Claude to fetch, rebase, test, and loop.
pub fn rebase_and_push_section(commands: &HashMap<String, String>) -> String {
    let mut b = String::new();
    b.push_str("\n\n# Final Sync\n\n");
    b.push_str(
        "After committing your changes, you must sync with origin before pushing. \
         Repeat the following steps until no new changes arrive from origin and all tests pass:\n\n",
    );
    b.push_str("1. Fetch origin: `git fetch origin`\n");
    b.push_str("2. Rebase against origin/main: `git rebase origin/main`\n");
    b.push_str("3. If the rebase produces conflicts, resolve them\n");

    if let Some(test_cmd) = commands.get("test") {
        if !test_cmd.is_empty() {
            b.push_str(&format!("4. Run the test suite: `{}`\n", test_cmd));
        } else {
            b.push_str("4. Run the tests\n");
        }
    } else {
        b.push_str("4. Run the tests\n");
    }

    b.push_str("5. Fix any failures and commit the fixes\n");
    b.push_str(
        "6. Go back to step 1 and repeat until `git fetch` brings nothing new and all tests pass\n\n",
    );
    b.push_str("Once stable, push the feature branch. Force push if needed.\n\n");
    b.push_str(
        "Whenever the term \"rebase loop\" is used elsewhere in this document, it refers to this procedure.\n",
    );
    b
}

/// Returns a section listing files with merge conflicts.
pub fn conflict_resolution_section(files: &[String]) -> String {
    if files.is_empty() {
        return String::new();
    }

    let mut b = String::new();
    b.push_str("\n## Conflict Resolution\n\n");
    b.push_str(
        "The rebase produced merge conflicts in the following files. \
         Resolve each conflict, keeping the intent of both sides, then \
         stage the resolved files and continue the rebase.\n\n",
    );

    for f in files {
        b.push_str(&format!("- `{}`\n", f));
    }

    b.push_str(
        "\nAfter resolving all conflicts:\n\
         1. `git add` the resolved files\n\
         2. `git rebase --continue`\n\
         3. Run the test suite to verify nothing is broken\n",
    );

    b
}

/// Returns a notification section to append to documents.
pub fn notification_section(title: &str) -> String {
    let mut b = String::new();
    b.push_str("\n## Notification\n\n");
    b.push_str(&format!(
        "When finished, a desktop notification titled \"{}\" will be sent.\n",
        title
    ));
    b
}

/// Returns a timeout section to append to documents.
pub fn timeout_section(duration: &str) -> String {
    let mut b = String::new();
    b.push_str("\n## Timeout\n\n");
    b.push_str(&format!(
        "This task has a timeout of {}. If the task is not completed within this \
         time, the process will be terminated.\n",
        duration
    ));
    b
}

/// Returns a closing section that reinforces task focus.
pub fn mission_reminder() -> String {
    "\n\n# Reminder\n\n\
     Your ONLY job is the task described in the document above. \
     Do not make unrelated changes, refactor other code, or work on anything \
     outside the scope of the task. Stay focused on the mission.\n"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_mode_instruction() {
        assert!(PLAN_MODE_INSTRUCTION.contains("plan mode"));
    }

    #[test]
    fn test_verification_section_with_commands() {
        let mut cmds = HashMap::new();
        cmds.insert("test".to_string(), "cargo test".to_string());
        cmds.insert("lint".to_string(), "cargo clippy".to_string());

        let section = verification_section(&cmds);
        assert!(section.contains("cargo test"));
        assert!(section.contains("cargo clippy"));
        assert!(section.contains("## Verification"));
    }

    #[test]
    fn test_verification_section_empty_commands() {
        let cmds = HashMap::new();
        let section = verification_section(&cmds);
        assert!(section.is_empty());
    }

    #[test]
    fn test_verification_section_test_only() {
        let mut cmds = HashMap::new();
        cmds.insert("test".to_string(), "go test ./...".to_string());

        let section = verification_section(&cmds);
        assert!(section.contains("go test ./..."));
        assert!(!section.contains("Run linter"));
    }

    #[test]
    fn test_verification_section_lint_only() {
        let mut cmds = HashMap::new();
        cmds.insert("lint".to_string(), "eslint .".to_string());

        let section = verification_section(&cmds);
        assert!(section.contains("eslint ."));
        assert!(!section.contains("Run tests"));
    }

    #[test]
    fn test_verification_section_parallel_warning() {
        let mut cmds = HashMap::new();
        cmds.insert("test".to_string(), "make test".to_string());

        let section = verification_section(&cmds);
        assert!(section.contains("concurrently"));
    }

    #[test]
    fn test_commit_instructions_unsigned() {
        let cmds = HashMap::new();
        let doc = commit_instructions(false, &cmds);
        assert!(!doc.contains("-S"));
        assert!(doc.contains("git commit -m"));
    }

    #[test]
    fn test_commit_instructions_signed() {
        let cmds = HashMap::new();
        let doc = commit_instructions(true, &cmds);
        assert!(doc.contains("-S"));
    }

    #[test]
    fn test_commit_instructions_with_test_and_lint() {
        let mut cmds = HashMap::new();
        cmds.insert("test".to_string(), "npm test".to_string());
        cmds.insert("lint".to_string(), "npm run lint".to_string());

        let doc = commit_instructions(false, &cmds);
        assert!(doc.contains("1. Run the test suite: `npm test`"));
        assert!(doc.contains("2. Run the linter: `npm run lint`"));
    }

    #[test]
    fn test_commit_instructions_no_commands() {
        let cmds = HashMap::new();
        let doc = commit_instructions(false, &cmds);
        assert!(doc.contains("1. Stage all changes"));
        assert!(doc.contains("2. Commit"));
    }

    #[test]
    fn test_commit_instructions_exclusive_commands() {
        let cmds = HashMap::new();
        let doc = commit_instructions(false, &cmds);
        assert!(doc.contains("Do NOT run any individual"));
    }

    #[test]
    fn test_commit_instructions_no_trailers() {
        let cmds = HashMap::new();
        let doc = commit_instructions(false, &cmds);
        assert!(doc.contains("Do NOT add Co-Authored-By"));
    }

    #[test]
    fn test_rebase_and_push_section_with_test() {
        let mut cmds = HashMap::new();
        cmds.insert("test".to_string(), "make test".to_string());

        let doc = rebase_and_push_section(&cmds);
        assert!(doc.contains("make test"));
    }

    #[test]
    fn test_rebase_and_push_section_without_test() {
        let cmds = HashMap::new();
        let doc = rebase_and_push_section(&cmds);
        assert!(doc.contains("Run the tests"));
    }

    #[test]
    fn test_rebase_and_push_section_rebase_loop() {
        let cmds = HashMap::new();
        let doc = rebase_and_push_section(&cmds);
        assert!(doc.contains("rebase loop"));
    }

    #[test]
    fn test_mission_reminder() {
        let doc = mission_reminder();
        assert!(doc.contains("focused"));
        assert!(doc.contains("ONLY job"));
    }

    #[test]
    fn test_conflict_resolution_section_with_files() {
        let files = vec!["src/main.rs".to_string(), "Cargo.toml".to_string()];
        let section = conflict_resolution_section(&files);
        assert!(section.contains("## Conflict Resolution"));
        assert!(section.contains("`src/main.rs`"));
        assert!(section.contains("`Cargo.toml`"));
        assert!(section.contains("rebase --continue"));
    }

    #[test]
    fn test_conflict_resolution_section_empty() {
        let section = conflict_resolution_section(&[]);
        assert!(section.is_empty());
    }

    #[test]
    fn test_notification_section() {
        let section = notification_section("Build Complete");
        assert!(section.contains("## Notification"));
        assert!(section.contains("Build Complete"));
    }

    #[test]
    fn test_timeout_section() {
        let section = timeout_section("30m");
        assert!(section.contains("## Timeout"));
        assert!(section.contains("30m"));
    }
}
