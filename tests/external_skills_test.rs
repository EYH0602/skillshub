//! Integration tests for external skill discovery and syncing
//!
//! External skills are skills found in agent directories that weren't
//! installed via skillshub (e.g., from Claude marketplace or manual installation).

mod common;

use common::{skill_md, TestEnv};
use serial_test::serial;
use std::fs;

#[test]
#[serial]
fn test_external_skill_creation() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Create agent with skills directory
    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Create an external skill (real directory, not a symlink)
    let ext_skill = env.create_external_skill(
        &claude_skills,
        "marketplace-skill",
        &skill_md("marketplace-skill", "From marketplace"),
    );

    assert!(ext_skill.exists());
    assert!(ext_skill.is_dir());
    assert!(!env.is_symlink(&ext_skill));
}

#[test]
#[serial]
fn test_external_skill_has_skill_md() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let ext_skill = env.create_external_skill(&claude_skills, "ext-skill", &skill_md("ext-skill", "External"));

    let skill_md_path = ext_skill.join("SKILL.md");
    assert!(skill_md_path.exists());

    let content = fs::read_to_string(skill_md_path).unwrap();
    assert!(content.contains("name: ext-skill"));
}

#[test]
#[serial]
fn test_multiple_external_skills() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Create multiple external skills
    let skill1 = env.create_external_skill(&claude_skills, "ext-skill-1", &skill_md("ext-skill-1", "First"));
    let skill2 = env.create_external_skill(&claude_skills, "ext-skill-2", &skill_md("ext-skill-2", "Second"));
    let skill3 = env.create_external_skill(&claude_skills, "ext-skill-3", &skill_md("ext-skill-3", "Third"));

    assert!(skill1.exists());
    assert!(skill2.exists());
    assert!(skill3.exists());

    // All should be directories
    assert!(skill1.is_dir());
    assert!(skill2.is_dir());
    assert!(skill3.is_dir());
}

#[test]
#[serial]
fn test_external_skills_in_multiple_agents() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Different agents might have different external skills
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let codex_skills = env.create_agent_with_skills(".codex", "skills");

    let claude_ext = env.create_external_skill(
        &claude_skills,
        "claude-marketplace-skill",
        &skill_md("claude-marketplace-skill", "From Claude"),
    );
    let codex_ext = env.create_external_skill(&codex_skills, "codex-tool", &skill_md("codex-tool", "Codex specific"));

    assert!(claude_ext.exists());
    assert!(codex_ext.exists());

    // Each should be in its respective agent directory
    assert!(claude_skills.join("claude-marketplace-skill").exists());
    assert!(codex_skills.join("codex-tool").exists());
}

#[test]
#[serial]
#[cfg(unix)]
fn test_external_skill_sync_via_symlink() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Source agent has the external skill
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let ext_skill = env.create_external_skill(
        &claude_skills,
        "synced-skill",
        &skill_md("synced-skill", "Will be synced"),
    );

    // Target agent receives a symlink
    let codex_skills = env.create_agent_with_skills(".codex", "skills");
    let sync_link = codex_skills.join("synced-skill");
    std::os::unix::fs::symlink(&ext_skill, &sync_link).unwrap();

    // Verify sync
    assert!(sync_link.exists());
    assert!(env.is_symlink(&sync_link));

    // Content accessible through symlink
    let content = fs::read_to_string(sync_link.join("SKILL.md")).unwrap();
    assert!(content.contains("synced-skill"));
}

#[test]
#[serial]
#[cfg(unix)]
fn test_external_skill_sync_to_all_agents() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Source: Claude has an external skill
    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let ext_skill = env.create_external_skill(&claude_skills, "shared-ext", &skill_md("shared-ext", "Shared"));

    // Targets: other agents get symlinks
    let codex_skills = env.create_agent_with_skills(".codex", "skills");
    let cursor_skills = env.create_agent_with_skills(".cursor", "skills");
    let aider_skills = env.create_agent_with_skills(".aider", "skills");

    for target in &[&codex_skills, &cursor_skills, &aider_skills] {
        let link = target.join("shared-ext");
        std::os::unix::fs::symlink(&ext_skill, &link).unwrap();
    }

    // All should have access
    for target in &[codex_skills, cursor_skills, aider_skills] {
        let link = target.join("shared-ext");
        assert!(link.exists());
        assert!(env.is_symlink(&link));
    }
}

#[test]
#[serial]
fn test_external_skill_db_tracking() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Database with external skill tracked
    let db_content = r#"{
        "taps": {},
        "installed": {},
        "external": {
            "marketplace-skill": {
                "name": "marketplace-skill",
                "source_agent": ".claude",
                "source_path": "/test/home/.claude/skills/marketplace-skill",
                "discovered_at": "2024-01-01T00:00:00Z"
            }
        },
        "linked_agents": [".claude"]
    }"#;

    env.write_db(db_content);

    let content = env.read_db().unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert!(json["external"]["marketplace-skill"].is_object());
    assert_eq!(json["external"]["marketplace-skill"]["source_agent"], ".claude");
}

#[test]
#[serial]
fn test_external_skill_forget_tracking() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Start with external skill tracked
    let db_with_external = r#"{
        "taps": {},
        "installed": {},
        "external": {
            "to-forget": {
                "name": "to-forget",
                "source_agent": ".claude",
                "source_path": "/test/path",
                "discovered_at": "2024-01-01T00:00:00Z"
            }
        },
        "linked_agents": []
    }"#;

    env.write_db(db_with_external);

    // Simulate "forget" by removing from external
    let db_after_forget = r#"{
        "taps": {},
        "installed": {},
        "external": {},
        "linked_agents": []
    }"#;

    env.write_db(db_after_forget);

    let content = env.read_db().unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert!(json["external"].as_object().unwrap().is_empty());
}

#[test]
#[serial]
fn test_multiple_external_skills_db() {
    let mut env = TestEnv::new();
    env.configure_env();

    let db_content = r#"{
        "taps": {},
        "installed": {},
        "external": {
            "skill-from-claude": {
                "name": "skill-from-claude",
                "source_agent": ".claude",
                "source_path": "/home/.claude/skills/skill-from-claude",
                "discovered_at": "2024-01-01T00:00:00Z"
            },
            "skill-from-codex": {
                "name": "skill-from-codex",
                "source_agent": ".codex",
                "source_path": "/home/.codex/skills/skill-from-codex",
                "discovered_at": "2024-01-02T00:00:00Z"
            }
        },
        "linked_agents": [".claude", ".codex"]
    }"#;

    env.write_db(db_content);

    let content = env.read_db().unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    let external = json["external"].as_object().unwrap();
    assert_eq!(external.len(), 2);
    assert!(external.contains_key("skill-from-claude"));
    assert!(external.contains_key("skill-from-codex"));
}

#[test]
#[serial]
#[cfg(unix)]
fn test_external_skill_not_symlink() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Create external skill (real directory)
    let ext_skill = env.create_external_skill(&claude_skills, "real-dir-skill", &skill_md("real-dir-skill", "Real"));

    // It's NOT a symlink - that's what makes it "external"
    assert!(!env.is_symlink(&ext_skill));

    // It's a real directory
    assert!(ext_skill.is_dir());
}

#[test]
#[serial]
#[cfg(unix)]
fn test_distinguish_external_from_linked() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // External skill: real directory
    let external = env.create_external_skill(
        &claude_skills,
        "external-skill",
        &skill_md("external-skill", "External"),
    );

    // Linked skill: symlink to skillshub
    let linked_source = env.create_skill("tap", "linked-skill", &skill_md("linked-skill", "Linked"));
    let linked = claude_skills.join("linked-skill");
    std::os::unix::fs::symlink(&linked_source, &linked).unwrap();

    // Can distinguish them
    assert!(!env.is_symlink(&external)); // External: NOT a symlink
    assert!(env.is_symlink(&linked)); // Linked: IS a symlink
}

#[test]
#[serial]
fn test_external_skill_with_scripts() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let ext_skill = env.create_external_skill(
        &claude_skills,
        "scripted-skill",
        &skill_md("scripted-skill", "Has scripts"),
    );

    // Add scripts directory (some marketplace skills have these)
    let scripts = ext_skill.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(scripts.join("helper.py"), "print('hello')").unwrap();

    assert!(scripts.exists());
    assert!(scripts.join("helper.py").exists());
}

#[test]
#[serial]
fn test_external_skill_naming_convention() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");

    // Various naming conventions used by marketplace/manual skills
    let names = vec![
        "simple-name",
        "CamelCaseName",
        "name_with_underscores",
        "name.with.dots",
        "123-starts-with-number",
    ];

    for name in &names {
        let skill = env.create_external_skill(&claude_skills, name, &skill_md(name, "Test"));
        assert!(skill.exists(), "Skill {} should be created", name);
    }
}

#[test]
#[serial]
#[cfg(unix)]
fn test_broken_sync_link_detection() {
    let mut env = TestEnv::new();
    env.configure_env();

    let claude_skills = env.create_agent_with_skills(".claude", "skills");
    let codex_skills = env.create_agent_with_skills(".codex", "skills");

    // Create external skill in claude
    let ext_skill = env.create_external_skill(&claude_skills, "temp-skill", &skill_md("temp-skill", "Temporary"));

    // Sync to codex
    let sync_link = codex_skills.join("temp-skill");
    std::os::unix::fs::symlink(&ext_skill, &sync_link).unwrap();

    // Now "remove" the source (simulating user deleted the external skill)
    fs::remove_dir_all(&ext_skill).unwrap();

    // The sync link is now broken
    assert!(!ext_skill.exists());
    assert!(env.is_symlink(&sync_link)); // Still a symlink
                                         // But following it would fail - the target doesn't exist
    let target = fs::read_link(&sync_link).unwrap();
    assert!(!target.exists());
}
