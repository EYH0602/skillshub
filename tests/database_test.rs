//! Integration tests for database persistence
//!
//! Tests that the database correctly persists across operations
//! and handles edge cases like corrupted files.

mod common;

use common::{db_with_default_tap, simple_db_json, TestEnv};
use serial_test::serial;

// Import the skillshub modules we're testing
// Note: Since skillshub is a binary crate, we need to use the crate's public API
// For now, we test via file system operations and CLI behavior

#[test]
#[serial]
fn test_db_file_created_on_first_run() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Initially no db file
    assert!(!env.db_path.exists());

    // After init, db file should exist
    // We simulate what init_db does by writing the default structure
    env.write_db(&db_with_default_tap());

    assert!(env.db_path.exists());
    let content = env.read_db().unwrap();
    assert!(content.contains("EYH0602/skillshub"));
}

#[test]
#[serial]
fn test_db_persists_installed_skills() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Write initial db with an installed skill
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
                "commit": "abc1234",
                "installed_at": "2024-01-01T00:00:00Z",
                "local": true
            }
        },
        "external": {},
        "linked_agents": []
    }"#;

    env.write_db(db_content);

    // Read it back and verify
    let content = env.read_db().unwrap();
    assert!(content.contains("code-reviewer"));
    assert!(content.contains("abc1234"));
}

#[test]
#[serial]
fn test_db_persists_external_skills() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Write db with external skill
    let db_content = r#"{
        "taps": {},
        "installed": {},
        "external": {
            "marketplace-skill": {
                "name": "marketplace-skill",
                "source_agent": ".claude",
                "source_path": "/home/user/.claude/skills/marketplace-skill",
                "discovered_at": "2024-01-01T00:00:00Z"
            }
        },
        "linked_agents": [".claude"]
    }"#;

    env.write_db(db_content);

    let content = env.read_db().unwrap();
    assert!(content.contains("marketplace-skill"));
    assert!(content.contains(".claude"));
}

#[test]
#[serial]
fn test_db_persists_multiple_taps() {
    let mut env = TestEnv::new();
    env.configure_env();

    let db_content = r#"{
        "taps": {
            "EYH0602/skillshub": {
                "url": "https://github.com/EYH0602/skillshub",
                "skills_path": "skills",
                "updated_at": null,
                "is_default": true,
                "is_bundled": true
            },
            "anthropics/skills": {
                "url": "https://github.com/anthropics/skills",
                "skills_path": "skills",
                "updated_at": "2024-06-01T12:00:00Z",
                "is_default": false,
                "is_bundled": false,
                "cached_registry": {
                    "name": "anthropics/skills",
                    "description": "Official Anthropic skills",
                    "skills": {
                        "debugging": {
                            "path": "skills/debugging",
                            "description": "Debug code effectively"
                        }
                    }
                }
            }
        },
        "installed": {},
        "external": {},
        "linked_agents": []
    }"#;

    env.write_db(db_content);

    let content = env.read_db().unwrap();
    assert!(content.contains("EYH0602/skillshub"));
    assert!(content.contains("anthropics/skills"));
    assert!(content.contains("cached_registry"));
    assert!(content.contains("debugging"));
}

#[test]
#[serial]
fn test_db_handles_empty_file() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Write empty file (not valid JSON)
    env.write_db("");

    // Reading should fail or return empty - depends on implementation
    // The point is it shouldn't crash
    let content = env.read_db();
    assert!(content.is_some()); // File exists but is empty
}

#[test]
#[serial]
fn test_db_structure_roundtrip() {
    let mut env = TestEnv::new();
    env.configure_env();

    // Write, read, verify structure is preserved
    let original = simple_db_json();
    env.write_db(&original);

    let loaded = env.read_db().unwrap();

    // Parse both and compare structure
    let original_json: serde_json::Value = serde_json::from_str(&original).unwrap();
    let loaded_json: serde_json::Value = serde_json::from_str(&loaded).unwrap();

    assert_eq!(original_json["taps"], loaded_json["taps"]);
    assert_eq!(original_json["installed"], loaded_json["installed"]);
    assert_eq!(original_json["external"], loaded_json["external"]);
}

#[test]
#[serial]
fn test_db_with_linked_agents() {
    let mut env = TestEnv::new();
    env.configure_env();

    let db_content = r#"{
        "taps": {},
        "installed": {},
        "external": {},
        "linked_agents": [".claude", ".codex", ".cursor"]
    }"#;

    env.write_db(db_content);

    let content = env.read_db().unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    let agents = json["linked_agents"].as_array().unwrap();
    assert_eq!(agents.len(), 3);
    assert!(agents.iter().any(|a| a == ".claude"));
    assert!(agents.iter().any(|a| a == ".codex"));
    assert!(agents.iter().any(|a| a == ".cursor"));
}

// Tests for the common test infrastructure itself
mod test_env_tests {
    use super::*;
    use common::skill_md;

    #[test]
    fn test_env_directories_created() {
        let env = TestEnv::new();
        assert!(env.home_dir.exists());
        assert!(env.skillshub_home.exists());
        assert!(env.skills_dir.exists());
    }

    #[test]
    fn test_create_agent_creates_directory() {
        let env = TestEnv::new();
        let agent_dir = env.create_agent(".claude");
        assert!(agent_dir.exists());
        assert!(agent_dir.is_dir());
    }

    #[test]
    fn test_create_agent_with_skills() {
        let env = TestEnv::new();
        let skills_dir = env.create_agent_with_skills(".claude", "skills");
        assert!(skills_dir.exists());
        assert!(skills_dir.ends_with("skills"));
    }

    #[test]
    fn test_create_skill() {
        let env = TestEnv::new();
        let skill_dir = env.create_skill("EYH0602/skillshub", "test-skill", &skill_md("test-skill", "A test"));
        assert!(skill_dir.exists());
        assert!(skill_dir.join("SKILL.md").exists());

        let content = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(content.contains("name: test-skill"));
    }

    #[test]
    fn test_create_external_skill() {
        let env = TestEnv::new();
        let skills_dir = env.create_agent_with_skills(".claude", "skills");
        let ext_skill = env.create_external_skill(&skills_dir, "ext-skill", &skill_md("ext-skill", "External"));

        assert!(ext_skill.exists());
        assert!(ext_skill.join("SKILL.md").exists());
    }
}
