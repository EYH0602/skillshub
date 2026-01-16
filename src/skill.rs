use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Skill metadata parsed from SKILL.md frontmatter
#[derive(Debug, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "allowed-tools")]
    #[serde(default)]
    #[allow(dead_code)]
    pub allowed_tools: AllowedTools,
}

/// Flexible deserializer for allowed-tools (can be string or array)
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct AllowedTools(pub Vec<String>);

impl<'de> Deserialize<'de> for AllowedTools {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct AllowedToolsVisitor;

        impl<'de> Visitor<'de> for AllowedToolsVisitor {
            type Value = AllowedTools;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or array of strings")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(AllowedTools(
                    value.split(',').map(|s| s.trim().to_string()).collect(),
                ))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut tools = Vec::new();
                while let Some(value) = seq.next_element::<String>()? {
                    tools.push(value);
                }
                Ok(AllowedTools(tools))
            }
        }

        deserializer.deserialize_any(AllowedToolsVisitor)
    }
}

/// Represents a discovered skill
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    #[allow(dead_code)]
    pub has_scripts: bool,
    #[allow(dead_code)]
    pub has_references: bool,
}

/// Parse skill metadata from SKILL.md file
pub fn parse_skill_metadata(skill_md_path: &Path) -> Result<SkillMetadata> {
    let content = fs::read_to_string(skill_md_path)
        .with_context(|| format!("Failed to read {}", skill_md_path.display()))?;

    // Extract YAML frontmatter between --- markers
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        anyhow::bail!(
            "Invalid SKILL.md format: missing YAML frontmatter in {}",
            skill_md_path.display()
        );
    }

    let yaml_content = parts[1].trim();
    let metadata: SkillMetadata = serde_yaml::from_str(yaml_content).with_context(|| {
        format!(
            "Failed to parse YAML frontmatter in {}",
            skill_md_path.display()
        )
    })?;

    Ok(metadata)
}

/// Discover all skills in a directory
pub fn discover_skills(skills_dir: &Path) -> Result<Vec<Skill>> {
    let mut skills = Vec::new();

    if !skills_dir.exists() {
        return Ok(skills);
    }

    for entry in fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        match parse_skill_metadata(&skill_md) {
            Ok(metadata) => {
                let has_scripts = path.join("scripts").exists();
                let has_references =
                    path.join("references").exists() || path.join("resources").exists();

                skills.push(Skill {
                    name: metadata.name,
                    description: metadata
                        .description
                        .unwrap_or_else(|| "No description".to_string()),
                    path,
                    has_scripts,
                    has_references,
                });
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to parse skill at {}: {}",
                    colored::Colorize::yellow("Warning:"),
                    path.display(),
                    e
                );
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_skill_metadata_basic() {
        let dir = TempDir::new().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        fs::write(
            &skill_md,
            r#"---
name: test-skill
description: A test skill
---
# Test Skill
Some content here.
"#,
        )
        .unwrap();

        let metadata = parse_skill_metadata(&skill_md).unwrap();
        assert_eq!(metadata.name, "test-skill");
        assert_eq!(metadata.description, Some("A test skill".to_string()));
    }

    #[test]
    fn test_parse_skill_metadata_with_allowed_tools_string() {
        let dir = TempDir::new().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        fs::write(
            &skill_md,
            r#"---
name: test-skill
description: A test skill
allowed-tools: Tool1, Tool2, Tool3
---
# Test
"#,
        )
        .unwrap();

        let metadata = parse_skill_metadata(&skill_md).unwrap();
        assert_eq!(metadata.allowed_tools.0, vec!["Tool1", "Tool2", "Tool3"]);
    }

    #[test]
    fn test_parse_skill_metadata_with_allowed_tools_array() {
        let dir = TempDir::new().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        fs::write(
            &skill_md,
            r#"---
name: test-skill
allowed-tools:
  - Tool1
  - Tool2
---
# Test
"#,
        )
        .unwrap();

        let metadata = parse_skill_metadata(&skill_md).unwrap();
        assert_eq!(metadata.allowed_tools.0, vec!["Tool1", "Tool2"]);
    }

    #[test]
    fn test_parse_skill_metadata_missing_frontmatter() {
        let dir = TempDir::new().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        fs::write(&skill_md, "# No frontmatter here").unwrap();

        let result = parse_skill_metadata(&skill_md);
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_skills_empty_dir() {
        let dir = TempDir::new().unwrap();
        let skills = discover_skills(dir.path()).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_discover_skills_with_skills() {
        let dir = TempDir::new().unwrap();

        // Create skill1
        let skill1_dir = dir.path().join("skill1");
        fs::create_dir(&skill1_dir).unwrap();
        fs::write(
            skill1_dir.join("SKILL.md"),
            r#"---
name: skill1
description: First skill
---
# Skill 1
"#,
        )
        .unwrap();

        // Create skill2 with scripts
        let skill2_dir = dir.path().join("skill2");
        fs::create_dir(&skill2_dir).unwrap();
        fs::write(
            skill2_dir.join("SKILL.md"),
            r#"---
name: skill2
description: Second skill
---
# Skill 2
"#,
        )
        .unwrap();
        fs::create_dir(skill2_dir.join("scripts")).unwrap();

        // Create skill3 with references
        let skill3_dir = dir.path().join("skill3");
        fs::create_dir(&skill3_dir).unwrap();
        fs::write(
            skill3_dir.join("SKILL.md"),
            r#"---
name: skill3
---
# Skill 3
"#,
        )
        .unwrap();
        fs::create_dir(skill3_dir.join("references")).unwrap();

        let skills = discover_skills(dir.path()).unwrap();
        assert_eq!(skills.len(), 3);

        // Skills should be sorted by name
        assert_eq!(skills[0].name, "skill1");
        assert_eq!(skills[0].description, "First skill");
        assert!(!skills[0].has_scripts);
        assert!(!skills[0].has_references);

        assert_eq!(skills[1].name, "skill2");
        assert!(skills[1].has_scripts);
        assert!(!skills[1].has_references);

        assert_eq!(skills[2].name, "skill3");
        assert_eq!(skills[2].description, "No description");
        assert!(!skills[2].has_scripts);
        assert!(skills[2].has_references);
    }

    #[test]
    fn test_discover_skills_nonexistent_dir() {
        let path = PathBuf::from("/nonexistent/path");
        let skills = discover_skills(&path).unwrap();
        assert!(skills.is_empty());
    }
}
