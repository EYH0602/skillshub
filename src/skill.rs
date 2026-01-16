use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use tabled::Tabled;

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
    pub has_scripts: bool,
    pub has_references: bool,
}

/// Table row for displaying skills
#[derive(Tabled)]
pub struct SkillRow {
    #[tabled(rename = " ")]
    pub status: &'static str,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Description")]
    pub description: String,
    #[tabled(rename = "Extras")]
    pub extras: String,
}

/// Parse skill metadata from SKILL.md file
pub fn parse_skill_metadata(skill_md_path: &PathBuf) -> Result<SkillMetadata> {
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
pub fn discover_skills(skills_dir: &PathBuf) -> Result<Vec<Skill>> {
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
