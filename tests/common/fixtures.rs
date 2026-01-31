//! Test fixtures and data generators for integration tests

#![allow(dead_code)]

use flate2::write::GzEncoder;
use flate2::Compression;
use tar::Builder;

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

/// Create a mock tarball containing skill files
///
/// The tarball has the structure GitHub uses:
/// `{owner}-{repo}-{short_commit}/path/to/skill/SKILL.md`
///
/// When a skill path is empty (""), the SKILL.md is placed at the
/// tarball root (i.e., `{prefix}/SKILL.md`), representing a repo
/// that itself is a skill.
pub fn create_skill_tarball(owner: &str, repo: &str, commit: &str, skills: &[(&str, &str)]) -> Vec<u8> {
    let mut archive_data = Vec::new();
    {
        let encoder = GzEncoder::new(&mut archive_data, Compression::default());
        let mut builder = Builder::new(encoder);

        // GitHub tarball prefix: owner-repo-shortcommit
        let short_commit = if commit.len() >= 7 { &commit[..7] } else { commit };
        let prefix = format!("{}-{}-{}", owner, repo, short_commit);

        for (skill_path, skill_md_content) in skills {
            // Add SKILL.md file
            // For root-level skills (empty path), place SKILL.md directly under prefix
            let file_path = if skill_path.is_empty() {
                format!("{}/SKILL.md", prefix)
            } else {
                format!("{}/{}/SKILL.md", prefix, skill_path)
            };
            let mut header = tar::Header::new_gnu();
            header.set_path(&file_path).unwrap();
            header.set_size(skill_md_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, skill_md_content.as_bytes()).unwrap();
        }

        builder.finish().unwrap();
    }
    archive_data
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

    #[test]
    fn test_create_tarball() {
        let tarball = create_skill_tarball(
            "owner",
            "repo",
            "abc1234def",
            &[("skills/my-skill", &skill_md("my-skill", "Test skill"))],
        );
        assert!(!tarball.is_empty());

        // Verify it's valid gzip
        use flate2::read::GzDecoder;
        use std::io::Read;
        let mut decoder = GzDecoder::new(&tarball[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        assert!(!decompressed.is_empty());
    }
}
