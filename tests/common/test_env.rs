//! Test environment utilities for integration tests
//!
//! Provides isolated filesystem environments for testing skillshub operations
//! without affecting the user's real ~/.skillshub directory.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test environment that provides isolated directories for testing
///
/// All paths are within a temporary directory that is automatically
/// cleaned up when the TestEnv is dropped.
pub struct TestEnv {
    /// Temp directory holding all test data (auto-cleaned on drop)
    _temp_dir: TempDir,

    /// Mock home directory (~)
    pub home_dir: PathBuf,

    /// Mock skillshub home (~/.skillshub)
    pub skillshub_home: PathBuf,

    /// Mock skills install dir (~/.skillshub/skills)
    pub skills_dir: PathBuf,

    /// Mock database path (~/.skillshub/db.json)
    pub db_path: PathBuf,

    /// Saved original env var value for restoration
    original_test_home: Option<String>,
    original_api_base: Option<String>,
    original_raw_base: Option<String>,
}

impl TestEnv {
    /// Create a new isolated test environment
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let home_dir = temp_dir.path().join("home");

        let skillshub_home = home_dir.join(".skillshub");
        let skills_dir = skillshub_home.join("skills");
        let db_path = skillshub_home.join("db.json");

        // Create directories
        fs::create_dir_all(&skills_dir).unwrap();

        Self {
            _temp_dir: temp_dir,
            home_dir,
            skillshub_home,
            skills_dir,
            db_path,
            original_test_home: None,
            original_api_base: None,
            original_raw_base: None,
        }
    }

    /// Set up environment variables to use mock paths
    ///
    /// This should be called before running any skillshub operations.
    /// The original environment variables are saved and restored when
    /// the TestEnv is dropped.
    pub fn configure_env(&mut self) {
        // Save original values
        self.original_test_home = std::env::var("SKILLSHUB_TEST_HOME").ok();
        self.original_api_base = std::env::var("SKILLSHUB_GITHUB_API_BASE").ok();
        self.original_raw_base = std::env::var("SKILLSHUB_GITHUB_RAW_BASE").ok();

        // Set test overrides
        std::env::set_var("SKILLSHUB_TEST_HOME", &self.home_dir);
    }

    /// Configure environment to use a mock GitHub server
    pub fn configure_github_mock(&mut self, mock_url: &str) {
        std::env::set_var("SKILLSHUB_GITHUB_API_BASE", mock_url);
        std::env::set_var("SKILLSHUB_GITHUB_RAW_BASE", mock_url);
    }

    /// Create a mock agent directory (e.g., ".claude", ".codex")
    ///
    /// Returns the full path to the agent directory.
    pub fn create_agent(&self, name: &str) -> PathBuf {
        let agent_dir = self.home_dir.join(name);
        fs::create_dir_all(&agent_dir).unwrap();
        agent_dir
    }

    /// Create a mock agent with skills subdirectory
    ///
    /// Returns the path to the skills subdirectory.
    pub fn create_agent_with_skills(&self, agent_name: &str, skills_subdir: &str) -> PathBuf {
        let agent_dir = self.create_agent(agent_name);
        let skills_path = agent_dir.join(skills_subdir);
        fs::create_dir_all(&skills_path).unwrap();
        skills_path
    }

    /// Create a mock installed skill in the skillshub skills directory
    ///
    /// Creates the directory structure: ~/.skillshub/skills/<tap>/<skill>/SKILL.md
    pub fn create_skill(&self, tap: &str, skill: &str, skill_md_content: &str) -> PathBuf {
        let skill_dir = self.skills_dir.join(tap).join(skill);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), skill_md_content).unwrap();
        skill_dir
    }

    /// Create an external skill directly in an agent's skills directory
    ///
    /// This simulates a skill installed via marketplace or manual copy.
    pub fn create_external_skill(&self, agent_skills_path: &PathBuf, skill_name: &str, content: &str) -> PathBuf {
        let skill_dir = agent_skills_path.join(skill_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
        skill_dir
    }

    /// Write a database file with the given content
    pub fn write_db(&self, content: &str) {
        fs::write(&self.db_path, content).unwrap();
    }

    /// Read the database file content
    pub fn read_db(&self) -> Option<String> {
        fs::read_to_string(&self.db_path).ok()
    }

    /// Check if a file exists relative to the test home
    pub fn exists(&self, path: &str) -> bool {
        self.home_dir.join(path).exists()
    }

    /// Check if a path is a symlink
    pub fn is_symlink(&self, path: &PathBuf) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    /// Get the symlink target
    pub fn read_link(&self, path: &PathBuf) -> Option<PathBuf> {
        fs::read_link(path).ok()
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // Restore original environment variables
        match &self.original_test_home {
            Some(val) => std::env::set_var("SKILLSHUB_TEST_HOME", val),
            None => std::env::remove_var("SKILLSHUB_TEST_HOME"),
        }
        match &self.original_api_base {
            Some(val) => std::env::set_var("SKILLSHUB_GITHUB_API_BASE", val),
            None => std::env::remove_var("SKILLSHUB_GITHUB_API_BASE"),
        }
        match &self.original_raw_base {
            Some(val) => std::env::set_var("SKILLSHUB_GITHUB_RAW_BASE", val),
            None => std::env::remove_var("SKILLSHUB_GITHUB_RAW_BASE"),
        }
    }
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_creates_directories() {
        let env = TestEnv::new();
        assert!(env.skillshub_home.exists());
        assert!(env.skills_dir.exists());
    }

    #[test]
    fn test_create_agent() {
        let env = TestEnv::new();
        let agent_dir = env.create_agent(".claude");
        assert!(agent_dir.exists());
        assert!(agent_dir.ends_with(".claude"));
    }

    #[test]
    fn test_create_skill() {
        let env = TestEnv::new();
        let skill_dir = env.create_skill("owner/repo", "my-skill", "---\nname: my-skill\n---\n# Test");
        assert!(skill_dir.exists());
        assert!(skill_dir.join("SKILL.md").exists());
    }
}
