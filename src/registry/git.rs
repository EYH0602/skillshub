use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pre-flight check that git is available.
pub fn check_git() -> Result<()> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .context("git is not installed or not in PATH")?;
    if !output.status.success() {
        anyhow::bail!("git is not working properly");
    }
    Ok(())
}

/// Compute the local clone path for a tap: `<taps_dir>/<owner>/<repo>`
pub fn tap_clone_path(taps_dir: &Path, tap_name: &str) -> PathBuf {
    let parts: Vec<&str> = tap_name.splitn(2, '/').collect();
    match parts.as_slice() {
        [owner, repo] => taps_dir.join(owner).join(repo),
        _ => taps_dir.join(tap_name),
    }
}

/// Clone a git repository (shallow, depth 1) to the given destination directory.
/// If `branch` is provided, clones that specific branch.
/// Uses `.status()` so git's progress output streams to the terminal.
pub fn git_clone(url: &str, dest: &Path, branch: Option<&str>) -> Result<()> {
    check_git()?;
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1"]);

    if let Some(b) = branch {
        cmd.args(["-b", b]);
    }

    cmd.arg(url).arg(dest);

    let status = cmd.status().context("Failed to run git clone (is git installed?)")?;

    if !status.success() {
        anyhow::bail!("git clone failed");
    }

    Ok(())
}

/// Pull latest changes in an existing clone (fast-forward only).
/// Uses `.status()` so git's progress output streams to the terminal.
pub fn git_pull(repo_path: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(repo_path)
        .status()
        .context("Failed to run git pull")?;

    if !status.success() {
        anyhow::bail!("git pull failed");
    }

    Ok(())
}

/// Get the HEAD commit SHA (short, 7 chars) of a local repository.
pub fn git_head_sha(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-parse failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Ensure a tap clone exists and is healthy. Clone if missing or corrupted.
#[allow(dead_code)]
pub fn ensure_clone(clone_dir: &Path, url: &str, branch: Option<&str>) -> Result<PathBuf> {
    if clone_dir.join(".git").exists() {
        // Verify the clone is functional
        let rev_check = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(clone_dir)
            .output();

        let rev_ok = matches!(rev_check, Ok(ref output) if output.status.success());

        // Verify remote URL matches expected
        let remote_ok = if rev_ok {
            let remote = Command::new("git")
                .args(["remote", "get-url", "origin"])
                .current_dir(clone_dir)
                .output();
            matches!(remote, Ok(ref output) if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == url)
        } else {
            false
        };

        // Verify checked-out branch matches requested branch (if specified)
        let branch_ok = if rev_ok && remote_ok {
            match branch {
                Some(expected) => {
                    let current = Command::new("git")
                        .args(["rev-parse", "--abbrev-ref", "HEAD"])
                        .current_dir(clone_dir)
                        .output();
                    matches!(current, Ok(ref output) if output.status.success()
                        && String::from_utf8_lossy(&output.stdout).trim() == expected)
                }
                None => true, // No specific branch requested, any branch is fine
            }
        } else {
            false
        };

        if rev_ok && remote_ok && branch_ok {
            return Ok(clone_dir.to_path_buf());
        }

        // Corrupted, wrong remote, or wrong branch — remove and re-clone
        eprintln!("  Re-cloning tap (clone was corrupted or remote changed)...");
        std::fs::remove_dir_all(clone_dir)?;
    }

    // Clone from scratch
    if let Some(parent) = clone_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    git_clone(url, clone_dir, branch)?;
    Ok(clone_dir.to_path_buf())
}

/// Pull latest changes, falling back to delete + re-clone on failure.
pub fn pull_or_reclone(clone_dir: &Path, url: &str, branch: Option<&str>) -> Result<()> {
    match git_pull(clone_dir) {
        Ok(()) => Ok(()),
        Err(_) => {
            eprintln!("  Pull failed, re-cloning...");
            std::fs::remove_dir_all(clone_dir)?;
            if let Some(parent) = clone_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            git_clone(url, clone_dir, branch)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    /// Helper: create a local git repo with one commit, return its path.
    fn create_local_repo(dir: &Path) -> PathBuf {
        let repo = dir.join("origin-repo");
        std::fs::create_dir_all(&repo).unwrap();

        StdCommand::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&repo)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo)
            .output()
            .unwrap();

        std::fs::write(repo.join("README.md"), "# Test Repo\n").unwrap();

        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(&repo)
            .output()
            .unwrap();

        repo
    }

    /// Helper: create a local git repo with a named branch, return its path.
    fn create_local_repo_with_branch(dir: &Path, branch_name: &str) -> PathBuf {
        let repo = create_local_repo(dir);

        StdCommand::new("git")
            .args(["checkout", "-b", branch_name])
            .current_dir(&repo)
            .output()
            .unwrap();

        std::fs::write(repo.join("BRANCH.md"), format!("# Branch: {}\n", branch_name)).unwrap();

        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["commit", "-m", "branch commit"])
            .current_dir(&repo)
            .output()
            .unwrap();

        repo
    }

    /// Helper: get the file:// URL for a local repo path.
    fn file_url(path: &Path) -> String {
        format!("file://{}", path.display())
    }

    // --- Existing unit tests for tap_clone_path ---

    #[test]
    fn test_clone_dir_path_owner_repo() {
        let base = PathBuf::from("/home/user/.skillshub/taps");
        let result = tap_clone_path(&base, "owner/repo");
        assert_eq!(result, base.join("owner").join("repo"));
    }

    #[test]
    fn test_clone_dir_path_single_name() {
        let base = PathBuf::from("/home/user/.skillshub/taps");
        let result = tap_clone_path(&base, "single-name");
        assert_eq!(result, base.join("single-name"));
    }

    #[test]
    fn test_clone_dir_path_nested() {
        let base = PathBuf::from("/tmp/taps");
        let result = tap_clone_path(&base, "EYH0602/skillshub");
        assert_eq!(result, base.join("EYH0602").join("skillshub"));
    }

    // --- New resilience tests ---

    #[test]
    fn test_check_git() {
        // git should be available in the test environment
        assert!(check_git().is_ok());
    }

    #[test]
    fn test_ensure_clone_creates_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo(temp.path());
        let url = file_url(&origin);

        let clone_dir = temp.path().join("clone");
        assert!(!clone_dir.exists());

        let result = ensure_clone(&clone_dir, &url, None);
        assert!(result.is_ok(), "ensure_clone failed: {:?}", result);
        assert!(clone_dir.join(".git").exists());
        assert!(clone_dir.join("README.md").exists());
    }

    #[test]
    fn test_ensure_clone_repairs_corrupted() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo(temp.path());
        let url = file_url(&origin);

        let clone_dir = temp.path().join("clone");

        // First, create a valid clone
        let result = ensure_clone(&clone_dir, &url, None);
        assert!(result.is_ok());

        // Corrupt the clone by removing .git/HEAD
        let head_path = clone_dir.join(".git").join("HEAD");
        std::fs::remove_file(&head_path).unwrap();

        // ensure_clone should detect corruption and re-clone
        let result = ensure_clone(&clone_dir, &url, None);
        assert!(
            result.is_ok(),
            "ensure_clone should repair corrupted clone: {:?}",
            result
        );
        assert!(clone_dir.join(".git").exists());
        assert!(head_path.exists(), ".git/HEAD should be restored after re-clone");
    }

    #[test]
    fn test_ensure_clone_repairs_wrong_remote() {
        let temp = tempfile::TempDir::new().unwrap();

        // Create two different repos
        let origin1_dir = temp.path().join("origin1");
        std::fs::create_dir_all(&origin1_dir).unwrap();
        let origin1 = create_local_repo(&origin1_dir);

        let origin2_dir = temp.path().join("origin2");
        std::fs::create_dir_all(&origin2_dir).unwrap();
        let origin2 = create_local_repo(&origin2_dir);

        let url1 = file_url(&origin1);
        let url2 = file_url(&origin2);

        let clone_dir = temp.path().join("clone");

        // Clone from origin1
        let result = ensure_clone(&clone_dir, &url1, None);
        assert!(result.is_ok());

        // Now call ensure_clone with a different URL (origin2)
        // It should detect the remote mismatch and re-clone
        let result = ensure_clone(&clone_dir, &url2, None);
        assert!(
            result.is_ok(),
            "ensure_clone should re-clone for wrong remote: {:?}",
            result
        );

        // Verify the remote now points to origin2
        let remote = StdCommand::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&clone_dir)
            .output()
            .unwrap();
        let remote_url = String::from_utf8_lossy(&remote.stdout).trim().to_string();
        assert_eq!(remote_url, url2);
    }

    #[test]
    fn test_ensure_clone_noop_healthy() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo(temp.path());
        let url = file_url(&origin);

        let clone_dir = temp.path().join("clone");

        // Create a valid clone
        let result = ensure_clone(&clone_dir, &url, None);
        assert!(result.is_ok());

        // Get the HEAD sha before second call
        let sha_before = git_head_sha(&clone_dir).unwrap();

        // Call ensure_clone again — should be a no-op
        let result = ensure_clone(&clone_dir, &url, None);
        assert!(result.is_ok());

        // HEAD sha should be the same (no re-clone happened)
        let sha_after = git_head_sha(&clone_dir).unwrap();
        assert_eq!(sha_before, sha_after);
    }

    #[test]
    fn test_ensure_clone_reclones_wrong_branch() {
        let temp = tempfile::TempDir::new().unwrap();

        // Create origin with a "feature" branch that has a unique file
        let origin = create_local_repo_with_branch(temp.path(), "feature");
        let url = file_url(&origin);

        // Clone specifically on "feature" branch
        let clone_dir = temp.path().join("clone");
        git_clone(&url, &clone_dir, Some("feature")).unwrap();
        assert!(clone_dir.join("BRANCH.md").exists());

        // Detect what branch the clone is on
        let branch_out = StdCommand::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&clone_dir)
            .output()
            .unwrap();
        let current_branch = String::from_utf8_lossy(&branch_out.stdout).trim().to_string();
        assert_eq!(current_branch, "feature");

        // Create a "release" branch on origin with different content
        StdCommand::new("git")
            .args(["checkout", "-b", "release"])
            .current_dir(&origin)
            .output()
            .unwrap();
        std::fs::write(origin.join("release.txt"), "release\n").unwrap();
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&origin)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", "release commit"])
            .current_dir(&origin)
            .output()
            .unwrap();

        // ensure_clone with branch="release" should detect branch mismatch and re-clone
        let result = ensure_clone(&clone_dir, &url, Some("release"));
        assert!(
            result.is_ok(),
            "ensure_clone should re-clone for wrong branch: {:?}",
            result
        );
        assert!(
            clone_dir.join("release.txt").exists(),
            "should have release branch content after re-clone"
        );
    }

    #[test]
    fn test_pull_or_reclone_happy_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo(temp.path());
        let url = file_url(&origin);

        let clone_dir = temp.path().join("clone");
        git_clone(&url, &clone_dir, None).unwrap();

        // Add a new commit to origin
        std::fs::write(origin.join("new-file.txt"), "new content\n").unwrap();
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&origin)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", "second commit"])
            .current_dir(&origin)
            .output()
            .unwrap();

        // Unshallow the clone so pull can work
        // (shallow clones from file:// can pull if origin has new commits)
        StdCommand::new("git")
            .args(["fetch", "--unshallow"])
            .current_dir(&clone_dir)
            .output()
            .unwrap();

        let result = pull_or_reclone(&clone_dir, &url, None);
        assert!(result.is_ok(), "pull_or_reclone happy path failed: {:?}", result);

        // The new file should be present after pull
        assert!(clone_dir.join("new-file.txt").exists());
    }

    #[test]
    fn test_pull_or_reclone_force_push() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo(temp.path());
        let url = file_url(&origin);

        let clone_dir = temp.path().join("clone");
        git_clone(&url, &clone_dir, None).unwrap();

        // Simulate a force-push by resetting origin to a new root commit
        // that diverges from what the clone has
        std::fs::write(origin.join("diverged.txt"), "diverged content\n").unwrap();
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&origin)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "--amend", "-m", "amended commit"])
            .current_dir(&origin)
            .output()
            .unwrap();

        // Make a local commit in the clone so pull --ff-only will fail
        std::fs::write(clone_dir.join("local.txt"), "local content\n").unwrap();
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&clone_dir)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", "local commit"])
            .current_dir(&clone_dir)
            .output()
            .unwrap();

        // Unshallow for the pull attempt
        StdCommand::new("git")
            .args(["fetch", "--unshallow"])
            .current_dir(&clone_dir)
            .output()
            .ok(); // may fail if already unshallowed, that's fine

        // pull_or_reclone should fall back to re-clone
        let result = pull_or_reclone(&clone_dir, &url, None);
        assert!(
            result.is_ok(),
            "pull_or_reclone should succeed via re-clone: {:?}",
            result
        );

        // After re-clone, the diverged file from origin should be present
        assert!(clone_dir.join("diverged.txt").exists());
        // And the local-only file should be gone (fresh clone)
        assert!(!clone_dir.join("local.txt").exists());
    }

    #[test]
    fn test_git_clone_with_branch_local() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo_with_branch(temp.path(), "feature-branch");
        let url = file_url(&origin);

        let clone_dir = temp.path().join("clone");
        let result = git_clone(&url, &clone_dir, Some("feature-branch"));
        assert!(result.is_ok(), "clone with branch failed: {:?}", result);
        assert!(clone_dir.join(".git").exists());
        assert!(clone_dir.join("BRANCH.md").exists());

        // Verify the checked-out branch
        let output = StdCommand::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&clone_dir)
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(branch, "feature-branch");
    }

    #[test]
    fn test_git_pull_local() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo(temp.path());
        let url = file_url(&origin);

        let clone_dir = temp.path().join("clone");
        git_clone(&url, &clone_dir, None).unwrap();

        // Add a new commit to origin
        std::fs::write(origin.join("pulled-file.txt"), "pull me\n").unwrap();
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&origin)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", "commit to pull"])
            .current_dir(&origin)
            .output()
            .unwrap();

        // Unshallow first so pull can work
        StdCommand::new("git")
            .args(["fetch", "--unshallow"])
            .current_dir(&clone_dir)
            .output()
            .unwrap();

        let result = git_pull(&clone_dir);
        assert!(result.is_ok(), "git_pull failed: {:?}", result);
        assert!(clone_dir.join("pulled-file.txt").exists());
    }

    #[test]
    fn test_git_clone_with_invalid_branch_local() {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = create_local_repo(temp.path());
        let url = file_url(&origin);
        let dest = temp.path().join("clone");

        let result = git_clone(&url, &dest, Some("nonexistent-branch-xyz"));
        assert!(result.is_err(), "clone with invalid branch should fail");
    }

    // --- Preserved non-network tests ---

    #[test]
    fn test_git_clone_invalid_dest_parent() {
        let result = git_clone(
            "https://github.com/octocat/Hello-World.git",
            std::path::Path::new("/nonexistent/parent/dir/repo"),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_git_head_sha_non_repo() {
        let temp = tempfile::TempDir::new().unwrap();
        let result = git_head_sha(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_git_pull_non_repo() {
        let temp = tempfile::TempDir::new().unwrap();
        let result = git_pull(temp.path());
        assert!(result.is_err());
    }
}
