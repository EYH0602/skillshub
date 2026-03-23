// Test utilities for integration tests
pub mod fixtures;
pub mod mock_github;
pub mod test_env;

#[allow(unused_imports)]
pub use fixtures::*;
#[allow(unused_imports)]
pub use mock_github::*;
#[allow(unused_imports)]
pub use test_env::*;

use std::fs;
use std::path::Path;
use std::process::Command;

/// Initialize a bare git repo with a single commit.
///
/// Sets up git config (user.email, user.name) and creates an initial
/// commit with a README.md so the repo has a valid HEAD.
#[allow(dead_code)]
pub fn init_test_repo(dir: &Path) {
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .expect("git init failed");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .output()
        .expect("git config user.email failed");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .expect("git config user.name failed");
    fs::write(dir.join("README.md"), "# test").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()
        .expect("git add failed");
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .output()
        .expect("git commit failed");
}

/// Initialize a git repo containing a skill.
///
/// Builds on [`init_test_repo`] by adding a `skills/<skill_name>/SKILL.md`
/// file with the given description, then committing the result.
#[allow(dead_code)]
pub fn init_test_repo_with_skill(dir: &Path, skill_name: &str, description: &str) {
    init_test_repo(dir);
    let skill_dir = dir.join("skills").join(skill_name);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: {}\ndescription: {}\n---\n# {}",
            skill_name, description, skill_name
        ),
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()
        .expect("git add failed");
    Command::new("git")
        .args(["commit", "-m", "add skill"])
        .current_dir(dir)
        .output()
        .expect("git commit failed");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_init_test_repo_creates_valid_git_repo() {
        let tmp = TempDir::new().unwrap();
        init_test_repo(tmp.path());

        // Should have a .git directory
        assert!(tmp.path().join(".git").exists());

        // Should have at least one commit
        let output = Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(log.contains("init"), "expected 'init' commit, got: {}", log);
    }

    #[test]
    fn test_init_test_repo_with_skill_creates_skill_md() {
        let tmp = TempDir::new().unwrap();
        init_test_repo_with_skill(tmp.path(), "my-skill", "A test skill");

        let skill_md = tmp.path().join("skills/my-skill/SKILL.md");
        assert!(skill_md.exists());

        let content = fs::read_to_string(skill_md).unwrap();
        assert!(content.contains("name: my-skill"));
        assert!(content.contains("description: A test skill"));

        // Should have two commits (init + add skill)
        let output = Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(log.lines().count() >= 2, "expected at least 2 commits, got: {}", log);
    }
}
