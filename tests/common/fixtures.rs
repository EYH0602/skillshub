//! Test fixtures and data generators for integration tests

#![allow(dead_code)]

/// Create a valid SKILL.md content with frontmatter
pub fn skill_md(name: &str, description: &str) -> String {
    format!(
        r#"---
name: {}
description: {}
---
# {}

Skill instructions here.
"#,
        name, description, name
    )
}

/// Create a minimal SKILL.md with just required fields
pub fn skill_md_minimal(name: &str) -> String {
    format!(
        r#"---
name: {}
---
# {}
"#,
        name, name
    )
}

/// Create a SKILL.md with allowed-tools
pub fn skill_md_with_tools(name: &str, description: &str, tools: &[&str]) -> String {
    let tools_str = tools
        .iter()
        .map(|t| format!("\"{}\"", t))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"---
name: {}
description: {}
allowed-tools: [{}]
---
# {}

Skill instructions here.
"#,
        name, description, tools_str, name
    )
}

/// Create a simple database JSON for testing
pub fn simple_db_json() -> String {
    r#"{
        "taps": {},
        "installed": {},
        "external": {},
        "linked_agents": []
    }"#
    .to_string()
}

/// Create a database JSON with a default tap
pub fn db_with_default_tap() -> String {
    r#"{
        "taps": {
            "EYH0602/skillshub": {
                "url": "https://github.com/EYH0602/skillshub",
                "skills_path": "skills",
                "updated_at": null,
                "is_default": true,
                "is_bundled": true
            }
        },
        "installed": {},
        "external": {},
        "linked_agents": []
    }"#
    .to_string()
}

/// Create a database JSON with an installed skill
pub fn db_with_installed_skill(tap: &str, skill: &str) -> String {
    format!(
        r#"{{
        "taps": {{}},
        "installed": {{
            "{}/{}": {{
                "tap": "{}",
                "skill": "{}",
                "commit": null,
                "installed_at": "2024-01-01T00:00:00Z",
                "local": true
            }}
        }},
        "external": {{}},
        "linked_agents": []
    }}"#,
        tap, skill, tap, skill
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_md_generation() {
        let content = skill_md("test-skill", "A test skill");
        assert!(content.contains("name: test-skill"));
        assert!(content.contains("description: A test skill"));
    }

    #[test]
    fn test_skill_md_with_tools() {
        let content = skill_md_with_tools("test", "Test", &["Bash", "Read"]);
        assert!(content.contains("allowed-tools:"));
        assert!(content.contains("Bash"));
        assert!(content.contains("Read"));
    }
}
