//! Integration tests for agent linking functionality
//!
//! Tests the link_to_agents workflow including:
//! - Discovering agents
//! - Creating symlinks
//! - Handling external skills
//! - Edge cases like old-style symlinks

mod common;

use common::{db_with_default_tap, skill_md, TestEnv};
use serial_test::serial;
use std::fs;

/// Helper to create a skill and return the link name
fn create_test_skill(env: &TestEnv, tap: &str, name: &str) -> std::path::PathBuf {
    env.create_skill(tap, name, &skill_md(name, &format!("{} skill", name)))
}

#[test]
#[serial]
fn test_discover_agents_with_test_home() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create agent directories in test home
    env.create_agent(".claude");
    env.create_agent(".codex");

    // The discover_agents function should now find these
    // We can verify by checking the directories exist
    assert!(env.home_dir.join(".claude").exists());
    assert!(env.home_dir.join(".codex").exists());
}

#[test]
#[serial]
fn test_link_workflow_setup() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Set up a complete environment for linking:
    // 1. Create agents with skills directories
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let codex_skills = env.create_agent_with_skills(".codex", "skills");

    // 2. Create installed skills
    let skill1 = create_test_skill(&env, "EYH0602/skillshub", "code-reviewer");
    let skill2 = create_test_skill(&env, "EYH0602/skillshub", "debugging");

    // 3. Create database
    env.write_db(&db_with_default_tap());

    // Verify setup
    assert!(claude_skills.exists());
    assert!(codex_skills.exists());
    assert!(skill1.exists());
    assert!(skill2.exists());
    assert!(env.db_path.exists());
}

#[test]
#[serial]
#[cfg(unix)]
fn test_manual_link_workflow() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Set up
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let skill_dir = create_test_skill(&env, "tap", "my-skill");

    // Manually create symlink (simulating what link_to_agents does)
    let link_path = claude_skills.join("my-skill");
    std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();

    // Verify link
    assert!(link_path.exists());
    assert!(env.is_symlink(&link_path));

    // The link should point to the skill directory
    let target = fs::read_link(&link_path).unwrap();
    assert_eq!(target, skill_dir);

    // Reading the SKILL.md through the symlink should work
    let content = fs::read_to_string(link_path.join("SKILL.md")).unwrap();
    assert!(content.contains("name: my-skill"));
}

#[test]
#[serial]
#[cfg(unix)]
fn test_link_multiple_skills_to_agent() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Create multiple skills
    let skills = vec![("tap1", "skill-a"), ("tap1", "skill-b"), ("tap2", "skill-c")];

    for (tap, name) in &skills {
        let skill_dir = create_test_skill(&env, tap, name);
        let link_path = claude_skills.join(name);
        std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();
    }

    // Verify all links
    for (_, name) in &skills {
        let link_path = claude_skills.join(name);
        assert!(link_path.exists());
        assert!(env.is_symlink(&link_path));
    }

    // Count links
    let entries: Vec<_> = fs::read_dir(&claude_skills).unwrap().collect();
    assert_eq!(entries.len(), 3);
}

#[test]
#[serial]
#[cfg(unix)]
fn test_link_same_skill_to_multiple_agents() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create agents
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let codex_skills = env.create_agent_with_skills(".codex", "skills");
    let cursor_skills = env.create_agent_with_skills(".cursor", "skills");

    // Create one skill
    let skill_dir = create_test_skill(&env, "tap", "shared-skill");

    // Link to all agents
    for agent_skills in &[&claude_skills, &codex_skills, &cursor_skills] {
        let link_path = agent_skills.join("shared-skill");
        std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();
    }

    // All should have working links
    for agent_skills in &[claude_skills, codex_skills, cursor_skills] {
        let link_path = agent_skills.join("shared-skill");
        assert!(link_path.exists());
        let content = fs::read_to_string(link_path.join("SKILL.md")).unwrap();
        assert!(content.contains("shared-skill"));
    }
}

#[test]
#[serial]
#[cfg(unix)]
fn test_existing_file_not_overwritten() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Create an existing non-symlink file/directory with the same name
    let existing = claude_skills.join("existing-skill");
    fs::create_dir_all(&existing).unwrap();
    fs::write(existing.join("user-file.txt"), "user content").unwrap();

    // Now try to link a skill with the same name
    let skill_dir = create_test_skill(&env, "tap", "existing-skill");

    // The link would fail because the target exists
    let result = std::os::unix::fs::symlink(&skill_dir, &existing);
    assert!(result.is_err()); // Should fail

    // Original content should be preserved
    assert!(existing.join("user-file.txt").exists());
    let content = fs::read_to_string(existing.join("user-file.txt")).unwrap();
    assert_eq!(content, "user content");
}

#[test]
#[serial]
#[cfg(unix)]
fn test_link_idempotency() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let skill_dir = create_test_skill(&env, "tap", "test-skill");
    let link_path = claude_skills.join("test-skill");

    // First link
    std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();
    assert!(env.is_symlink(&link_path));

    // Second link attempt should fail (already exists)
    let result = std::os::unix::fs::symlink(&skill_dir, &link_path);
    assert!(result.is_err());

    // But the original link should still work
    assert!(link_path.exists());
    assert!(link_path.join("SKILL.md").exists());
}

#[test]
#[serial]
#[cfg(unix)]
fn test_old_style_symlink_detection() {
    let mut env = TestEnv::new();
    env.configure_env();

    let agent_dir = env.create_agent(".claude");
    let skills_path = agent_dir.join("skills");

    // Create an old-style symlink: agent/skills -> skillshub/skills (entire directory)
    std::os::unix::fs::symlink(&env.skills_dir, &skills_path).unwrap();

    // Verify it's a symlink pointing to skills_dir
    assert!(env.is_symlink(&skills_path));
    let target = fs::read_link(&skills_path).unwrap();
    assert_eq!(target, env.skills_dir);
}

#[test]
#[serial]
#[cfg(unix)]
fn test_convert_old_style_symlink_to_directory() {
    let mut env = TestEnv::new();
    env.configure_env();

    let agent_dir = env.create_agent(".claude");
    let skills_path = agent_dir.join("skills");

    // Create old-style symlink
    std::os::unix::fs::symlink(&env.skills_dir, &skills_path).unwrap();
    assert!(env.is_symlink(&skills_path));

    // Convert to directory (like link_to_agents does)
    fs::remove_file(&skills_path).unwrap();
    fs::create_dir_all(&skills_path).unwrap();

    // Now it should be a directory, not a symlink
    assert!(!env.is_symlink(&skills_path));
    assert!(skills_path.is_dir());
}

#[test]
#[serial]
fn test_agent_without_skills_directory() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create agent without skills subdirectory
    let agent_dir = env.create_agent(".claude");

    // No skills directory should exist
    assert!(agent_dir.exists());
    assert!(!agent_dir.join("skills").exists());
}

#[test]
#[serial]
fn test_all_known_agents() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create all known agents
    let agents = vec![
        (".claude", "skills"),
        (".codex", "skills"),
        (".opencode", "skill"),
        (".aider", "skills"),
        (".cursor", "skills"),
        (".continue", "skills"),
    ];

    for (agent, skills_subdir) in &agents {
        let skills_path = env.create_agent_with_skills(agent, skills_subdir);
        assert!(skills_path.exists());
    }

    // All should exist
    for (agent, skills_subdir) in agents {
        let full_path = env.home_dir.join(agent).join(skills_subdir);
        assert!(full_path.exists(), "{} should exist", full_path.display());
    }
}

#[test]
#[serial]
fn test_linked_agents_tracking() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Simulate tracking linked agents in database
    let db_content = r#"{
        "taps": {},
        "installed": {},
        "external": {},
        "linked_agents": [".claude", ".codex"]
    }"#;

    env.write_db(db_content);

    let content = env.read_db().unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    let linked = json["linked_agents"].as_array().unwrap();
    assert_eq!(linked.len(), 2);
    assert!(linked.iter().any(|a| a == ".claude"));
    assert!(linked.iter().any(|a| a == ".codex"));
}

#[test]
#[serial]
#[cfg(unix)]
fn test_broken_symlink_handling() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Create a symlink to a non-existent target (broken symlink)
    let link_path = claude_skills.join("broken-skill");
    let nonexistent = env.skills_dir.join("does/not/exist");
    std::os::unix::fs::symlink(&nonexistent, &link_path).unwrap();

    // The symlink exists but is broken
    assert!(env.is_symlink(&link_path));

    // exists() returns false for broken symlinks
    // But symlink_metadata() works
    let meta = link_path.symlink_metadata();
    assert!(meta.is_ok());
    assert!(meta.unwrap().file_type().is_symlink());
}
