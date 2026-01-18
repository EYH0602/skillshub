//! Integration tests for local skill installation and linking
//!
//! Tests the end-to-end workflow of installing bundled skills
//! and linking them to mock agents.

mod common;

use common::{skill_md, TestEnv};
use serial_test::serial;
use std::fs;

#[test]
#[serial]
fn test_skill_directory_structure() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create a skill in the installed skills directory
    let skill_dir = env.create_skill(
        "EYH0602/skillshub",
        "code-reviewer",
        &skill_md("code-reviewer", "Reviews code"),
    );

    // Verify structure
    assert!(skill_dir.exists());
    assert!(skill_dir.join("SKILL.md").exists());

    // Verify it's in the right location
    let expected_path = env.skills_dir.join("EYH0602/skillshub").join("code-reviewer");
    assert_eq!(skill_dir, expected_path);
}

#[test]
#[serial]
fn test_multiple_skills_from_same_tap() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create multiple skills from the same tap
    let skill1 = env.create_skill(
        "EYH0602/skillshub",
        "code-reviewer",
        &skill_md("code-reviewer", "Reviews code"),
    );
    let skill2 = env.create_skill("EYH0602/skillshub", "debugging", &skill_md("debugging", "Debug code"));
    let skill3 = env.create_skill("EYH0602/skillshub", "testing", &skill_md("testing", "Write tests"));

    assert!(skill1.exists());
    assert!(skill2.exists());
    assert!(skill3.exists());

    // All should be under the same tap directory
    let tap_dir = env.skills_dir.join("EYH0602/skillshub");
    assert!(tap_dir.join("code-reviewer").exists());
    assert!(tap_dir.join("debugging").exists());
    assert!(tap_dir.join("testing").exists());
}

#[test]
#[serial]
fn test_skills_from_multiple_taps() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create skills from different taps
    let skill1 = env.create_skill(
        "EYH0602/skillshub",
        "code-reviewer",
        &skill_md("code-reviewer", "Reviews code"),
    );
    let skill2 = env.create_skill("anthropics/skills", "debugging", &skill_md("debugging", "Debug code"));
    let skill3 = env.create_skill("user/custom-tap", "my-skill", &skill_md("my-skill", "Custom skill"));

    assert!(skill1.exists());
    assert!(skill2.exists());
    assert!(skill3.exists());

    // Verify they're in different tap directories
    assert!(env.skills_dir.join("EYH0602/skillshub").exists());
    assert!(env.skills_dir.join("anthropics/skills").exists());
    assert!(env.skills_dir.join("user/custom-tap").exists());
}

#[test]
#[serial]
fn test_skill_with_scripts_directory() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create a skill with scripts
    let skill_dir = env.create_skill(
        "tap",
        "skill-with-scripts",
        &skill_md("skill-with-scripts", "Has scripts"),
    );

    // Add scripts directory
    let scripts_dir = skill_dir.join("scripts");
    fs::create_dir_all(&scripts_dir).unwrap();
    fs::write(scripts_dir.join("run.sh"), "#!/bin/bash\necho 'Hello'").unwrap();

    assert!(scripts_dir.exists());
    assert!(scripts_dir.join("run.sh").exists());
}

#[test]
#[serial]
fn test_skill_with_references_directory() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create a skill with references
    let skill_dir = env.create_skill("tap", "skill-with-refs", &skill_md("skill-with-refs", "Has references"));

    // Add references directory
    let refs_dir = skill_dir.join("references");
    fs::create_dir_all(&refs_dir).unwrap();
    fs::write(refs_dir.join("docs.md"), "# Documentation\n\nSome docs here.").unwrap();

    assert!(refs_dir.exists());
    assert!(refs_dir.join("docs.md").exists());
}

#[test]
#[serial]
fn test_agent_directory_creation() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create mock agents
    let claude = env.create_agent(".claude");
    let codex = env.create_agent(".codex");

    assert!(claude.exists());
    assert!(codex.exists());
    assert!(claude.is_dir());
    assert!(codex.is_dir());
}

#[test]
#[serial]
fn test_agent_with_skills_subdirectory() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create agent with skills subdirectory
    let skills_path = env.create_agent_with_skills(".claude", "skills");

    assert!(skills_path.exists());
    assert!(skills_path.is_dir());
    assert!(skills_path.ends_with("skills"));
}

#[test]
#[serial]
#[cfg(unix)]
fn test_symlink_creation() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create a skill
    let skill_dir = env.create_skill("tap", "my-skill", &skill_md("my-skill", "Test"));

    // Create an agent skills directory
    let agent_skills = env.create_agent_with_skills(".claude", "skills");

    // Create a symlink manually (simulating what link_to_agents does)
    let link_path = agent_skills.join("my-skill");
    std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();

    assert!(link_path.exists());
    assert!(env.is_symlink(&link_path));

    // Verify symlink target
    let target = env.read_link(&link_path).unwrap();
    assert_eq!(target, skill_dir);
}

#[test]
#[serial]
#[cfg(unix)]
fn test_symlink_to_multiple_agents() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create a skill
    let skill_dir = env.create_skill("tap", "shared-skill", &skill_md("shared-skill", "Shared"));

    // Create multiple agent skills directories
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let codex_skills = env.create_agent_with_skills(".codex", "skills");
    let cursor_skills = env.create_agent_with_skills(".cursor", "skills");

    // Create symlinks to each agent
    let agents = vec![&claude_skills, &codex_skills, &cursor_skills];
    for agent_skills in &agents {
        let link_path = agent_skills.join("shared-skill");
        std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();
    }

    // Verify all symlinks
    for agent_skills in agents {
        let link_path = agent_skills.join("shared-skill");
        assert!(link_path.exists());
        assert!(env.is_symlink(&link_path));
    }
}

#[test]
#[serial]
fn test_external_skill_in_agent_directory() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create agent with skills directory
    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Create an external skill (real directory, not symlink)
    let external_skill = env.create_external_skill(
        &claude_skills,
        "marketplace-skill",
        &skill_md("marketplace-skill", "From marketplace"),
    );

    assert!(external_skill.exists());
    assert!(external_skill.is_dir());
    assert!(!env.is_symlink(&external_skill));
    assert!(external_skill.join("SKILL.md").exists());
}

#[test]
#[serial]
#[cfg(unix)]
fn test_external_skill_sync_to_other_agents() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create source agent with external skill
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let external_skill = env.create_external_skill(&claude_skills, "ext-skill", &skill_md("ext-skill", "External"));

    // Create target agent
    let codex_skills = env.create_agent_with_skills(".codex", "skills");

    // Simulate syncing: create symlink from codex to claude's external skill
    let sync_link = codex_skills.join("ext-skill");
    std::os::unix::fs::symlink(&external_skill, &sync_link).unwrap();

    // Verify sync
    assert!(sync_link.exists());
    assert!(env.is_symlink(&sync_link));

    let target = env.read_link(&sync_link).unwrap();
    assert_eq!(target, external_skill);
}

#[test]
#[serial]
fn test_skill_md_content_parsing() {
    let mut env = TestEnv::new();
    env.configure_env();

    let content = skill_md("test-skill", "A description of the skill");
    let skill_dir = env.create_skill("tap", "test-skill", &content);

    let skill_md_path = skill_dir.join("SKILL.md");
    let read_content = fs::read_to_string(skill_md_path).unwrap();

    assert!(read_content.contains("name: test-skill"));
    assert!(read_content.contains("description: A description of the skill"));
    assert!(read_content.contains("# test-skill"));
}

#[test]
#[serial]
fn test_db_with_installed_skill_structure() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Write db with installed skill
    let db_content = r#"{
        "taps": {
            "EYH0602/skillshub": {
                "url": "https://github.com/EYH0602/skillshub",
                "skills_path": "skills",
                "updated_at": null,
                "is_default": true,
                "is_bundled": true
            }
        },
        "installed": {
            "EYH0602/skillshub/code-reviewer": {
                "tap": "EYH0602/skillshub",
                "skill": "code-reviewer",
                "commit": null,
                "installed_at": "2024-01-01T00:00:00Z",
                "local": true
            }
        },
        "external": {},
        "linked_agents": []
    }"#;

    env.write_db(db_content);

    // Also create the actual skill directory
    env.create_skill(
        "EYH0602/skillshub",
        "code-reviewer",
        &skill_md("code-reviewer", "Code review"),
    );

    // Verify both db and skill exist
    assert!(env.db_path.exists());
    assert!(env.skills_dir.join("EYH0602/skillshub/code-reviewer").exists());
}
