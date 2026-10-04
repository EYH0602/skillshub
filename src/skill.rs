use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Optional versioning/authorship metadata nested under `metadata:` in SKILL.md frontmatter
#[derive(Debug, Deserialize, Default)]
pub struct SkillVersionMetadata {
    pub author: Option<String>,
    pub version: Option<String>,
}

/// Skill metadata parsed from SKILL.md frontmatter
#[derive(Debug, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "allowed-tools")]
    #[serde(default)]
    #[allow(dead_code)]
    pub allowed_tools: AllowedTools,
    pub license: Option<String>,
    #[serde(default)]
    pub metadata: Option<SkillVersionMetadata>,
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
                Ok(AllowedTools(value.split(',').map(|s| s.trim().to_string()).collect()))
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

/// Check whether a skill directory contains a `scripts/` subdirectory.
pub fn has_scripts_dir(skill_dir: &Path) -> bool {
    skill_dir.join("scripts").exists()
}

/// Check whether a skill directory contains a `references/` or `resources/` subdirectory.
pub fn has_references_dir(skill_dir: &Path) -> bool {
    skill_dir.join("references").exists() || skill_dir.join("resources").exists()
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

/// Frontmatter parsed from a SKILL.md file
#[derive(Debug)]
pub struct ParsedFrontmatter {
    pub metadata: SkillMetadata,
    /// The strict YAML error, set when the lenient fallback parser was used
    pub lenient_reason: Option<String>,
}

/// Parse SKILL.md content into metadata.
///
/// Tries strict YAML first. If that fails, falls back to a line-based parser for
/// `name`, `description` and `license`, so common hand-written frontmatter such as
/// an unquoted description containing `": "` still works.
pub fn parse_frontmatter(content: &str) -> Result<ParsedFrontmatter> {
    // Extract YAML frontmatter between --- markers
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        anyhow::bail!("Invalid SKILL.md format: missing YAML frontmatter");
    }

    // Keep the leading newline so YAML error line numbers match the file
    let yaml_content = parts[1].trim_end();
    match serde_yaml::from_str::<SkillMetadata>(yaml_content) {
        Ok(metadata) => Ok(ParsedFrontmatter {
            metadata,
            lenient_reason: None,
        }),
        Err(err) => match parse_frontmatter_lenient(yaml_content) {
            Some(metadata) => Ok(ParsedFrontmatter {
                metadata,
                lenient_reason: Some(err.to_string()),
            }),
            None => Err(anyhow::anyhow!("Failed to parse YAML frontmatter: {}", err)),
        },
    }
}

/// Line-based fallback for frontmatter that is not valid YAML.
///
/// Reads top-level `key: value` lines; the value is everything after the first
/// colon, so later `": "` sequences stay in the value. Indented lines continue the
/// previous key. Only `name`, `description` and `license` are extracted.
fn parse_frontmatter_lenient(yaml: &str) -> Option<SkillMetadata> {
    let mut fields: Vec<(String, Vec<String>)> = Vec::new();

    for line in yaml.lines() {
        let is_indented = line.starts_with(' ') || line.starts_with('\t');
        if is_indented || line.trim().is_empty() {
            if let Some((_, parts)) = fields.last_mut() {
                parts.push(line.trim().to_string());
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }

        match split_top_level_key(line) {
            Some((key, value)) => fields.push((key.to_string(), vec![value.trim().to_string()])),
            // Unindented non-key line: stop attaching continuation lines
            None => fields.push((String::new(), Vec::new())),
        }
    }

    let get = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, parts)| join_lenient_value(parts))
            .filter(|v| !v.is_empty())
    };

    Some(SkillMetadata {
        name: get("name")?,
        description: get("description"),
        allowed_tools: AllowedTools::default(),
        license: get("license"),
        metadata: None,
    })
}

/// Split `key: value` (or `key:`) where key is a simple identifier.
fn split_top_level_key(line: &str) -> Option<(&str, &str)> {
    let (key, rest) = line.split_once(':')?;
    let key_is_valid = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !key_is_valid || !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    Some((key, rest))
}

/// Join a key's inline value and continuation lines into one string.
fn join_lenient_value(parts: &[String]) -> String {
    let (first, rest) = match parts.split_first() {
        Some(split) => split,
        None => return String::new(),
    };

    // Block scalars: `|` keeps newlines, `>` folds them into spaces
    let indicator = first.trim_end_matches(['-', '+']);
    if indicator == "|" || indicator == ">" {
        let separator = if indicator == "|" { "\n" } else { " " };
        let lines: Vec<&str> = rest.iter().map(String::as_str).collect();
        return lines.join(separator).trim().to_string();
    }

    let joined = std::iter::once(first)
        .chain(rest)
        .filter(|p| !p.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    strip_matching_quotes(&joined).to_string()
}

fn strip_matching_quotes(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

/// Print a warning that a SKILL.md was parsed with the lenient fallback.
pub fn warn_lenient_frontmatter(location: &str, reason: &str) {
    eprintln!(
        "  {} {}: frontmatter is not valid YAML ({}); parsed leniently",
        "!".yellow(),
        location,
        reason
    );
}

/// Print a warning that a SKILL.md was skipped because its frontmatter could not be parsed.
pub fn warn_invalid_frontmatter(location: &str, err: &anyhow::Error) {
    eprintln!("  {} Skipping {}: {}", "!".yellow(), location, err);
}

/// Parse skill metadata from SKILL.md file
///
/// Falls back to lenient parsing silently; warnings are printed at discovery time.
pub fn parse_skill_metadata(skill_md_path: &Path) -> Result<SkillMetadata> {
    let content =
        fs::read_to_string(skill_md_path).with_context(|| format!("Failed to read {}", skill_md_path.display()))?;

    let parsed =
        parse_frontmatter(&content).with_context(|| format!("Invalid SKILL.md: {}", skill_md_path.display()))?;
    Ok(parsed.metadata)
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
                let has_scripts = has_scripts_dir(&path);
                let has_references = has_references_dir(&path);

                skills.push(Skill {
                    name: metadata.name,
                    description: metadata.description.unwrap_or_else(|| "No description".to_string()),
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
    fn test_parse_skill_metadata_unquoted_colon_in_description() {
        let dir = TempDir::new().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        fs::write(
            &skill_md,
            "---\nname: answer-me-with-html\ndescription: Renders a one-page HTML explainer: the model writes a short draft.\n---\n# Body\n",
        )
        .unwrap();

        let metadata = parse_skill_metadata(&skill_md).expect("unquoted ': ' in description should be tolerated");
        assert_eq!(metadata.name, "answer-me-with-html");
        assert_eq!(
            metadata.description.as_deref(),
            Some("Renders a one-page HTML explainer: the model writes a short draft.")
        );
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
    fn test_parse_skill_metadata_with_license_and_version_metadata() {
        let dir = TempDir::new().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        fs::write(
            &skill_md,
            r#"---
name: pdf-processing
description: Extract text from PDF files.
license: Apache-2.0
metadata:
  author: example-org
  version: "1.0"
---
# PDF Processing
"#,
        )
        .unwrap();

        let metadata = parse_skill_metadata(&skill_md).unwrap();
        assert_eq!(metadata.name, "pdf-processing");
        assert_eq!(metadata.description, Some("Extract text from PDF files.".to_string()));
        assert_eq!(metadata.license, Some("Apache-2.0".to_string()));
        let vm = metadata.metadata.unwrap();
        assert_eq!(vm.author, Some("example-org".to_string()));
        assert_eq!(vm.version, Some("1.0".to_string()));
    }

    #[test]
    fn test_parse_skill_metadata_optional_fields_absent() {
        let dir = TempDir::new().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        fs::write(
            &skill_md,
            r#"---
name: minimal-skill
---
# Minimal
"#,
        )
        .unwrap();

        let metadata = parse_skill_metadata(&skill_md).unwrap();
        assert_eq!(metadata.name, "minimal-skill");
        assert!(metadata.license.is_none());
        assert!(metadata.metadata.is_none());
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
    fn test_parse_frontmatter_valid_yaml_is_strict() {
        let content = "---\nname: s\ndescription: \"quoted: ok\"\nallowed-tools: Read, Write\n---\n";
        let parsed = parse_frontmatter(content).unwrap();
        assert!(parsed.lenient_reason.is_none());
        assert_eq!(parsed.metadata.description.as_deref(), Some("quoted: ok"));
        assert_eq!(parsed.metadata.allowed_tools.0, vec!["Read", "Write"]);
    }

    #[test]
    fn test_parse_frontmatter_lenient_sets_reason() {
        let content = "---\nname: s\ndescription: a: b\n---\n";
        let parsed = parse_frontmatter(content).unwrap();
        let reason = parsed.lenient_reason.expect("lenient fallback should be reported");
        assert!(
            reason.contains("line 3"),
            "error line should match the file: {}",
            reason
        );
        assert_eq!(parsed.metadata.description.as_deref(), Some("a: b"));
    }

    #[test]
    fn test_parse_frontmatter_lenient_continuation_and_other_keys() {
        let content =
            "---\nname: s\ndescription: first: part\n  second part\nlicense: MIT\nmetadata:\n  author: x: y\n---\n";
        let parsed = parse_frontmatter(content).unwrap();
        assert!(parsed.lenient_reason.is_some());
        assert_eq!(parsed.metadata.name, "s");
        assert_eq!(parsed.metadata.description.as_deref(), Some("first: part second part"));
        assert_eq!(parsed.metadata.license.as_deref(), Some("MIT"));
        assert!(parsed.metadata.metadata.is_none());
    }

    #[test]
    fn test_parse_frontmatter_lenient_strips_quotes() {
        // The stray "]" makes strict YAML fail; the quoted name must still be unwrapped
        let content = "---\nname: \"s\"\ndescription: use it: now ]\n---\n";
        let parsed = parse_frontmatter(content).unwrap();
        assert!(parsed.lenient_reason.is_some());
        assert_eq!(parsed.metadata.name, "s");
        assert_eq!(parsed.metadata.description.as_deref(), Some("use it: now ]"));
    }

    #[test]
    fn test_parse_frontmatter_lenient_block_scalars() {
        let content = "---\nname: s\nlicense: a: b\ndescription: >-\n  folded\n  text\n---\n";
        let parsed = parse_frontmatter(content).unwrap();
        assert!(parsed.lenient_reason.is_some());
        assert_eq!(parsed.metadata.description.as_deref(), Some("folded text"));

        let content = "---\nname: s\nlicense: a: b\ndescription: |\n  line one\n  line two\n---\n";
        let parsed = parse_frontmatter(content).unwrap();
        assert_eq!(parsed.metadata.description.as_deref(), Some("line one\nline two"));
    }

    #[test]
    fn test_parse_frontmatter_lenient_requires_name() {
        let content = "---\ndescription: a: b\n---\n";
        let err = parse_frontmatter(content).unwrap_err();
        assert!(err.to_string().contains("Failed to parse YAML frontmatter"));
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
