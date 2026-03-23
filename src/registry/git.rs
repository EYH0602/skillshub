use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

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
pub fn git_clone(url: &str, dest: &Path, branch: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1"]);

    if let Some(b) = branch {
        cmd.args(["-b", b]);
    }

    cmd.arg(url).arg(dest);

    let output = cmd.output().context("Failed to run git clone (is git installed?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {}", stderr.trim());
    }

    Ok(())
}

/// Pull latest changes in an existing clone (fast-forward only).
pub fn git_pull(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git pull")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git pull failed: {}", stderr.trim());
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    #[ignore] // Requires network access and git
    fn test_git_clone_and_pull() {
        let temp = tempfile::TempDir::new().unwrap();
        let dest = temp.path().join("repo");

        // Clone
        let result = git_clone("https://github.com/octocat/Hello-World.git", &dest, None);
        assert!(result.is_ok(), "clone failed: {:?}", result);
        assert!(dest.join(".git").exists());

        // Pull
        let result = git_pull(&dest);
        assert!(result.is_ok(), "pull failed: {:?}", result);

        // Head commit
        let sha = git_head_sha(&dest);
        assert!(sha.is_ok());
        let sha = sha.unwrap();
        assert!(!sha.is_empty());
        assert!(sha.len() >= 7);
    }

    #[test]
    #[ignore] // Requires network access and git
    fn test_git_clone_with_branch() {
        let temp = tempfile::TempDir::new().unwrap();
        let dest = temp.path().join("repo");

        // Clone the "test" branch
        let result = git_clone("https://github.com/octocat/Hello-World.git", &dest, Some("test"));
        assert!(result.is_ok(), "clone with branch failed: {:?}", result);
        assert!(dest.join(".git").exists());

        // Verify the checked-out branch is "test"
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&dest)
            .output()
            .expect("failed to run git rev-parse");
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(branch, "test", "expected branch 'test', got '{}'", branch);
    }

    #[test]
    #[ignore] // Requires network access and git
    fn test_git_clone_with_invalid_branch() {
        let temp = tempfile::TempDir::new().unwrap();
        let dest = temp.path().join("repo");

        let result = git_clone(
            "https://github.com/octocat/Hello-World.git",
            &dest,
            Some("nonexistent-branch-xyz"),
        );
        assert!(result.is_err(), "clone with invalid branch should fail");
    }

    #[test]
    fn test_git_clone_invalid_dest_parent() {
        // Attempting to clone to a path with non-existent parent should fail
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
