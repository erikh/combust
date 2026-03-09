use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Repo wraps git operations using a hybrid of git2 and CLI fallback.
#[derive(Debug, Clone)]
pub struct Repo {
    pub dir: PathBuf,
}

impl Repo {
    /// Opens a repository at the given path.
    pub fn open(dir: &Path) -> Self {
        Repo {
            dir: dir.to_path_buf(),
        }
    }

    /// Returns true if the given path contains a .git directory or file.
    pub fn is_git_repo(path: &Path) -> bool {
        path.join(".git").exists()
    }

    /// Initializes a new git repository.
    pub fn init_repo(path: &Path) -> Result<()> {
        run_git(path, &["init"])?;
        Ok(())
    }

    /// Fetches from origin.
    pub fn fetch(&self) -> Result<()> {
        run_git(&self.dir, &["fetch", "origin"])?;
        Ok(())
    }

    /// Returns the remote URL for origin.
    pub fn remote_url(&self) -> Result<String> {
        let output = run_git(&self.dir, &["remote", "get-url", "origin"])?;
        Ok(output.trim().to_string())
    }

    /// Returns true if the given branch exists (local or remote).
    pub fn branch_exists(&self, name: &str) -> bool {
        // Check local branches.
        if run_git(&self.dir, &["rev-parse", "--verify", name]).is_ok() {
            return true;
        }
        // Check remote branches.
        let remote_ref = format!("refs/remotes/{}", name);
        run_git(&self.dir, &["rev-parse", "--verify", &remote_ref]).is_ok()
    }

    /// Returns the current branch name.
    pub fn current_branch(&self) -> Result<String> {
        let output = run_git(&self.dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        Ok(output.trim().to_string())
    }

    /// Checks out the given branch.
    pub fn checkout(&self, branch: &str) -> Result<()> {
        run_git(&self.dir, &["checkout", branch])?;
        Ok(())
    }

    /// Creates a new branch from HEAD.
    pub fn create_branch(&self, branch: &str) -> Result<()> {
        run_git(&self.dir, &["checkout", "-b", branch])?;
        Ok(())
    }

    /// Returns true if the working tree has uncommitted changes.
    pub fn has_changes(&self) -> Result<bool> {
        let output = run_git(&self.dir, &["status", "--porcelain"])?;
        Ok(!output.trim().is_empty())
    }

    /// Returns the SHA of the last commit.
    pub fn last_commit_sha(&self) -> Result<String> {
        let output = run_git(&self.dir, &["rev-parse", "HEAD"])?;
        Ok(output.trim().to_string())
    }

    /// Returns true if git is configured with a signing key.
    pub fn has_signing_key(&self) -> bool {
        run_git(&self.dir, &["config", "user.signingkey"]).is_ok()
    }

    /// Hard resets to the given ref.
    pub fn reset_hard(&self, reference: &str) -> Result<()> {
        run_git(&self.dir, &["reset", "--hard", reference])?;
        Ok(())
    }

    /// Cleans the working tree (removes untracked files and directories).
    pub fn clean(&self) -> Result<()> {
        run_git(&self.dir, &["clean", "-fd"])?;
        Ok(())
    }

    /// Aborts a rebase in progress. Returns Ok even if no rebase is in progress.
    pub fn rebase_abort(&self) -> Result<()> {
        let _ = run_git(&self.dir, &["rebase", "--abort"]);
        Ok(())
    }

    /// Rebases the current branch onto the given ref.
    pub fn rebase(&self, onto: &str) -> Result<()> {
        run_git(&self.dir, &["rebase", onto])?;
        Ok(())
    }

    /// Pushes the current branch to origin, force-with-lease.
    pub fn push(&self, branch: &str) -> Result<()> {
        run_git(
            &self.dir,
            &["push", "--force-with-lease", "origin", branch],
        )?;
        Ok(())
    }

    /// Pushes the current branch to origin/main.
    pub fn push_main(&self) -> Result<()> {
        run_git(&self.dir, &["push", "origin", "HEAD"])?;
        Ok(())
    }

    /// Creates a git worktree.
    pub fn worktree_add(&self, path: &Path, branch: &str) -> Result<()> {
        run_git(
            &self.dir,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                &path.to_string_lossy(),
            ],
        )?;
        Ok(())
    }

    /// Creates a git worktree for an existing branch.
    pub fn worktree_add_existing(&self, path: &Path, branch: &str) -> Result<()> {
        run_git(
            &self.dir,
            &["worktree", "add", &path.to_string_lossy(), branch],
        )?;
        Ok(())
    }

    /// Removes a git worktree.
    pub fn worktree_remove(&self, path: &Path) -> Result<()> {
        run_git(
            &self.dir,
            &["worktree", "remove", "--force", &path.to_string_lossy()],
        )?;
        Ok(())
    }

    /// Deletes a remote branch.
    pub fn delete_remote_branch(&self, branch: &str) -> Result<()> {
        run_git(
            &self.dir,
            &["push", "origin", "--delete", branch],
        )?;
        Ok(())
    }

    /// Returns the git diff between current HEAD and a given ref.
    pub fn diff(&self, reference: &str) -> Result<String> {
        run_git(&self.dir, &["diff", reference])
    }

    /// Returns the git log for a given range.
    pub fn log_oneline(&self, range: &str) -> Result<String> {
        run_git(&self.dir, &["log", "--oneline", range])
    }

    /// Stages all changes (git add .).
    pub fn add_all(&self) -> Result<()> {
        run_git(&self.dir, &["add", "."])?;
        Ok(())
    }

    /// Commits with a message, optionally signing.
    pub fn commit(&self, message: &str, sign: bool) -> Result<()> {
        if sign {
            run_git(&self.dir, &["commit", "-S", "-m", message])?;
        } else {
            run_git(&self.dir, &["commit", "-m", message])?;
        }
        Ok(())
    }

    /// Deletes a local branch.
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        run_git(&self.dir, &["branch", "-D", name])?;
        Ok(())
    }

    /// Lists worktrees in porcelain format.
    pub fn worktree_list(&self) -> Result<String> {
        run_git(&self.dir, &["worktree", "list", "--porcelain"])
    }

    /// Returns true if `ancestor` is an ancestor of `descendant`.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        run_git(
            &self.dir,
            &["merge-base", "--is-ancestor", ancestor, descendant],
        )
        .is_ok()
    }

    /// Merges a branch with fast-forward only.
    pub fn merge_ff_only(&self, branch: &str) -> Result<()> {
        run_git(&self.dir, &["merge", "--ff-only", branch])?;
        Ok(())
    }

    /// Returns the list of files with merge conflicts.
    pub fn conflict_files(&self) -> Result<Vec<String>> {
        let output = run_git(&self.dir, &["diff", "--name-only", "--diff-filter=U"]);
        match output {
            Ok(s) => Ok(s
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect()),
            Err(_) => Ok(Vec::new()),
        }
    }

    /// Returns true if there are merge conflicts.
    pub fn has_conflicts(&self) -> Result<bool> {
        Ok(!self.conflict_files()?.is_empty())
    }

    /// Continues a rebase in progress.
    pub fn rebase_continue(&self) -> Result<()> {
        run_git(&self.dir, &["rebase", "--continue"])?;
        Ok(())
    }

    /// Returns diff between two refs using merge-base.
    pub fn diff_range(&self, base: &str, head: &str) -> Result<String> {
        let merge_base = run_git(&self.dir, &["merge-base", base, head])?;
        let mb = merge_base.trim();
        run_git(&self.dir, &["diff", mb, head])
    }
}

/// Runs a git command in the given directory and returns stdout.
fn run_git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_git_repo() -> (TempDir, Repo) {
        let tmp = TempDir::new().unwrap();
        Repo::init_repo(tmp.path()).unwrap();
        // Configure user for commits and disable GPG signing.
        run_git(tmp.path(), &["config", "user.email", "test@test.com"]).unwrap();
        run_git(tmp.path(), &["config", "user.name", "Test"]).unwrap();
        run_git(tmp.path(), &["config", "commit.gpgsign", "false"]).unwrap();
        let repo = Repo::open(tmp.path());
        (tmp, repo)
    }

    fn make_commit(repo: &Repo) {
        fs::write(repo.dir.join("file.txt"), "content").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "initial commit"]).unwrap();
    }

    #[test]
    fn test_is_git_repo_true() {
        let (tmp, _repo) = setup_git_repo();
        assert!(Repo::is_git_repo(tmp.path()));
    }

    #[test]
    fn test_is_git_repo_false() {
        let tmp = TempDir::new().unwrap();
        assert!(!Repo::is_git_repo(tmp.path()));
    }

    #[test]
    fn test_init_and_open() {
        let tmp = TempDir::new().unwrap();
        Repo::init_repo(tmp.path()).unwrap();
        assert!(tmp.path().join(".git").exists());
        let repo = Repo::open(tmp.path());
        assert_eq!(repo.dir, tmp.path());
    }

    #[test]
    fn test_last_commit_sha() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        let sha = repo.last_commit_sha().unwrap();
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_has_changes_clean() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        assert!(!repo.has_changes().unwrap());
    }

    #[test]
    fn test_has_changes_dirty() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        fs::write(repo.dir.join("new-file.txt"), "dirty").unwrap();
        assert!(repo.has_changes().unwrap());
    }

    #[test]
    fn test_current_branch() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        let branch = repo.current_branch().unwrap();
        // Default branch is usually "main" or "master".
        assert!(!branch.is_empty());
    }

    #[test]
    fn test_create_and_checkout_branch() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);

        repo.create_branch("feature-branch").unwrap();
        assert_eq!(repo.current_branch().unwrap(), "feature-branch");

        // Checkout back to the default branch.
        let default = run_git(&repo.dir, &["rev-parse", "--abbrev-ref", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(default, "feature-branch");
    }

    #[test]
    fn test_branch_exists() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);

        assert!(!repo.branch_exists("nonexistent-branch"));

        repo.create_branch("test-branch").unwrap();
        // Go back to default to test existence of created branch.
        run_git(&repo.dir, &["checkout", "-"]).unwrap();
        assert!(repo.branch_exists("test-branch"));
    }

    #[test]
    fn test_has_signing_key_false() {
        let (_tmp, repo) = setup_git_repo();
        // By default, no signing key is configured.
        assert!(!repo.has_signing_key());
    }

    #[test]
    fn test_has_signing_key_true() {
        let (_tmp, repo) = setup_git_repo();
        run_git(&repo.dir, &["config", "user.signingkey", "ABCD1234"]).unwrap();
        assert!(repo.has_signing_key());
    }

    #[test]
    fn test_fetch_no_remote() {
        let (_tmp, repo) = setup_git_repo();
        // No remote configured, fetch should fail.
        assert!(repo.fetch().is_err());
    }

    fn setup_bare_remote() -> (TempDir, TempDir, Repo) {
        // Create a bare repo as remote.
        let bare_tmp = TempDir::new().unwrap();
        run_git(bare_tmp.path(), &["init", "--bare"]).unwrap();

        // Create a working repo that points to the bare repo.
        let work_tmp = TempDir::new().unwrap();
        Repo::init_repo(work_tmp.path()).unwrap();
        run_git(work_tmp.path(), &["config", "user.email", "test@test.com"]).unwrap();
        run_git(work_tmp.path(), &["config", "user.name", "Test"]).unwrap();
        run_git(work_tmp.path(), &["config", "commit.gpgsign", "false"]).unwrap();
        run_git(
            work_tmp.path(),
            &["remote", "add", "origin", &bare_tmp.path().to_string_lossy()],
        )
        .unwrap();

        // Make initial commit and push.
        fs::write(work_tmp.path().join("README.md"), "# Test").unwrap();
        run_git(work_tmp.path(), &["add", "-A"]).unwrap();
        run_git(work_tmp.path(), &["commit", "-m", "initial"]).unwrap();
        // Ensure main branch name.
        let _ = run_git(work_tmp.path(), &["branch", "-M", "main"]);
        run_git(work_tmp.path(), &["push", "-u", "origin", "main"]).unwrap();

        let repo = Repo::open(work_tmp.path());
        (bare_tmp, work_tmp, repo)
    }

    #[test]
    fn test_fetch_with_remote() {
        let (_bare, _work, repo) = setup_bare_remote();
        repo.fetch().unwrap();
    }

    #[test]
    fn test_push_and_push_main() {
        let (_bare, _work, repo) = setup_bare_remote();

        // Create a branch, make a change, and push.
        repo.create_branch("test-push").unwrap();
        fs::write(repo.dir.join("new.txt"), "data").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "test push"]).unwrap();
        repo.push("test-push").unwrap();

        // Push main.
        repo.checkout("main").unwrap();
        repo.push_main().unwrap();
    }

    #[test]
    fn test_reset_hard() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        let sha1 = repo.last_commit_sha().unwrap();

        fs::write(repo.dir.join("another.txt"), "more").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "second"]).unwrap();

        repo.reset_hard(&sha1).unwrap();
        assert_eq!(repo.last_commit_sha().unwrap(), sha1);
    }

    #[test]
    fn test_rebase() {
        let (_bare, _work, repo) = setup_bare_remote();

        // Create feature branch.
        repo.create_branch("feature").unwrap();
        fs::write(repo.dir.join("feature.txt"), "feat").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "feature commit"]).unwrap();

        // Rebase onto main (should be no-op since feature is ahead).
        repo.rebase("main").unwrap();
    }

    #[test]
    fn test_rebase_conflict_and_abort() {
        let (_bare, _work, repo) = setup_bare_remote();

        // Create conflicting branches.
        repo.create_branch("branch-a").unwrap();
        fs::write(repo.dir.join("conflict.txt"), "branch-a content").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "branch-a"]).unwrap();

        repo.checkout("main").unwrap();
        repo.create_branch("branch-b").unwrap();
        fs::write(repo.dir.join("conflict.txt"), "branch-b content").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "branch-b"]).unwrap();

        // Rebase should fail due to conflict.
        assert!(repo.rebase("branch-a").is_err());
        // Abort should succeed.
        repo.rebase_abort().unwrap();
    }

    #[test]
    fn test_log_oneline() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        fs::write(repo.dir.join("second.txt"), "two").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "second commit"]).unwrap();

        let log = repo.log_oneline("HEAD~1..HEAD").unwrap();
        assert!(log.contains("second commit"));
    }

    #[test]
    fn test_delete_remote_branch() {
        let (_bare, _work, repo) = setup_bare_remote();

        repo.create_branch("to-delete").unwrap();
        fs::write(repo.dir.join("del.txt"), "data").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "to delete"]).unwrap();
        repo.push("to-delete").unwrap();

        repo.checkout("main").unwrap();
        repo.delete_remote_branch("to-delete").unwrap();
    }

    #[test]
    fn test_worktree_add_and_remove() {
        let (_bare, _work, repo) = setup_bare_remote();

        let wt_path = repo.dir.join("..").join("worktree-test");
        repo.worktree_add(&wt_path, "wt-branch").unwrap();
        assert!(wt_path.exists());

        repo.worktree_remove(&wt_path).unwrap();
        assert!(!wt_path.exists());
    }

    #[test]
    fn test_worktree_add_existing() {
        let (_bare, _work, repo) = setup_bare_remote();

        // Create a branch first.
        repo.create_branch("existing-branch").unwrap();
        repo.checkout("main").unwrap();

        let wt_path = repo.dir.join("..").join("worktree-existing");
        repo.worktree_add_existing(&wt_path, "existing-branch").unwrap();
        assert!(wt_path.exists());

        repo.worktree_remove(&wt_path).unwrap();
    }

    #[test]
    fn test_worktree_list() {
        let (_bare, _work, repo) = setup_bare_remote();

        let list = repo.worktree_list().unwrap();
        // Should at least contain the main worktree.
        assert!(list.contains("worktree"));
    }

    #[test]
    fn test_add_all() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        fs::write(repo.dir.join("new-file.txt"), "new").unwrap();
        repo.add_all().unwrap();
        let status = run_git(&repo.dir, &["status", "--porcelain"]).unwrap();
        assert!(status.contains("new-file.txt"));
    }

    #[test]
    fn test_commit() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        fs::write(repo.dir.join("commit-test.txt"), "test").unwrap();
        repo.add_all().unwrap();
        repo.commit("test commit message", false).unwrap();
        let log = repo.log_oneline("HEAD~1..HEAD").unwrap();
        assert!(log.contains("test commit message"));
    }

    #[test]
    fn test_delete_branch() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        repo.create_branch("deletable").unwrap();
        repo.checkout("master").unwrap_or_else(|_| {
            // Try main if master doesn't exist.
            let _ = run_git(&repo.dir, &["checkout", "-"]);
        });
        // Go back to default branch.
        let branches = run_git(&repo.dir, &["branch"]).unwrap();
        let default = if branches.contains("main") { "main" } else { "master" };
        let _ = repo.checkout(default);
        repo.delete_branch("deletable").unwrap();
        assert!(!repo.branch_exists("deletable"));
    }

    #[test]
    fn test_is_ancestor() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        let sha1 = repo.last_commit_sha().unwrap();

        fs::write(repo.dir.join("second.txt"), "two").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "second"]).unwrap();
        let sha2 = repo.last_commit_sha().unwrap();

        assert!(repo.is_ancestor(&sha1, &sha2));
        assert!(!repo.is_ancestor(&sha2, &sha1));
    }

    #[test]
    fn test_merge_ff_only() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);

        // Create and switch to a feature branch.
        repo.create_branch("ff-feature").unwrap();
        fs::write(repo.dir.join("ff.txt"), "ff").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "ff commit"]).unwrap();

        // Go back to default.
        let _ = repo.checkout("master").or_else(|_| repo.checkout("main"));
        // Can't determine which is default, find it.
        let branches = run_git(&repo.dir, &["branch"]).unwrap();
        if branches.contains("main") {
            let _ = repo.checkout("main");
        }

        repo.merge_ff_only("ff-feature").unwrap();
    }

    #[test]
    fn test_conflict_files_no_conflicts() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        let files = repo.conflict_files().unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_diff_range() {
        let (_tmp, repo) = setup_git_repo();
        make_commit(&repo);
        let sha1 = repo.last_commit_sha().unwrap();

        repo.create_branch("diff-branch").unwrap();
        fs::write(repo.dir.join("diff-file.txt"), "diff content").unwrap();
        run_git(&repo.dir, &["add", "-A"]).unwrap();
        run_git(&repo.dir, &["commit", "-m", "diff commit"]).unwrap();

        let diff = repo.diff_range(&sha1, "HEAD").unwrap();
        assert!(diff.contains("diff-file.txt"));
    }
}
