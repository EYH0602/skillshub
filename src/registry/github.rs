use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use tar::Archive;

use super::models::{GitHubUrl, SkillEntry, TapRegistry};
use crate::skill::SkillMetadata;

/// User agent for API requests
const USER_AGENT: &str = "skillshub";

/// GitHub Tree API response
#[derive(Debug, Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
}

/// Entry in GitHub Tree API response
#[derive(Debug, Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
}

/// Parse a GitHub URL into components
///
/// Supports formats:
/// - https://github.com/owner/repo
/// - https://github.com/owner/repo/tree/branch
/// - https://github.com/owner/repo/tree/branch/path/to/folder
pub fn parse_github_url(url: &str) -> Result<GitHubUrl> {
    let url = url.trim_end_matches('/');

    // Remove protocol prefix
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("github.com/"))
        .with_context(|| format!("Invalid GitHub URL: {}", url))?;

    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() < 2 {
        anyhow::bail!("Invalid GitHub URL: must include owner/repo");
    }

    let owner = parts[0].to_string();
    let repo = parts[1].to_string();

    // Check for /tree/branch/path format
    let (branch, subpath) = if parts.len() > 3 && parts[2] == "tree" {
        let branch = parts[3].to_string();
        let subpath = if parts.len() > 4 {
            Some(parts[4..].join("/"))
        } else {
            None
        };
        (branch, subpath)
    } else {
        ("main".to_string(), None)
    };

    Ok(GitHubUrl {
        owner,
        repo,
        branch,
        path: subpath,
    })
}

/// Discover skills from a GitHub repository by scanning for SKILL.md files
///
/// Uses the GitHub Tree API to recursively find all SKILL.md files in the repo,
/// then fetches each one to extract metadata.
pub fn discover_skills_from_repo(github_url: &GitHubUrl, tap_name: &str) -> Result<TapRegistry> {
    let client = reqwest::blocking::Client::builder().user_agent(USER_AGENT).build()?;

    // Fetch the full repo tree with recursive=1
    let tree_url = format!("{}/git/trees/{}?recursive=1", github_url.api_url(), github_url.branch);

    let response = client
        .get(&tree_url)
        .send()
        .with_context(|| format!("Failed to fetch repo tree from {}", tree_url))?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Failed to fetch repo tree: HTTP {} from {}",
            response.status(),
            tree_url
        );
    }

    let tree_response: TreeResponse = response.json().with_context(|| "Failed to parse tree response")?;

    // Find all SKILL.md files
    let skill_paths: Vec<String> = tree_response
        .tree
        .iter()
        .filter(|entry| entry.entry_type == "blob" && entry.path.ends_with("/SKILL.md"))
        .map(|entry| {
            // Extract parent directory path: "skills/code-reviewer/SKILL.md" -> "skills/code-reviewer"
            entry
                .path
                .rsplit_once('/')
                .map(|(parent, _)| parent.to_string())
                .unwrap_or_default()
        })
        .filter(|path| !path.is_empty())
        .collect();

    if skill_paths.is_empty() {
        anyhow::bail!("No skills found in repository (no SKILL.md files detected)");
    }

    // Fetch metadata for each skill
    let mut skills = HashMap::new();
    for skill_path in &skill_paths {
        let skill_md_url = github_url.raw_url(&format!("{}/SKILL.md", skill_path));

        match client.get(&skill_md_url).send() {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(content) = resp.text() {
                    if let Some((name, description)) = parse_skill_md_content(&content) {
                        skills.insert(
                            name.clone(),
                            SkillEntry {
                                path: skill_path.clone(),
                                description,
                                homepage: None,
                            },
                        );
                    }
                }
            }
            _ => {
                // If we can't fetch metadata, use directory name as skill name
                if let Some(skill_name) = skill_path.rsplit('/').next() {
                    skills.insert(
                        skill_name.to_string(),
                        SkillEntry {
                            path: skill_path.clone(),
                            description: None,
                            homepage: None,
                        },
                    );
                }
            }
        }
    }

    let description = Some(format!("Skills from {}/{}", github_url.owner, github_url.repo));

    Ok(TapRegistry {
        name: tap_name.to_string(),
        description,
        skills,
    })
}

/// Parse SKILL.md content to extract name and description from YAML frontmatter
fn parse_skill_md_content(content: &str) -> Option<(String, Option<String>)> {
    // Extract YAML frontmatter between --- markers
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return None;
    }

    let yaml_content = parts[1].trim();
    let metadata: SkillMetadata = serde_yaml::from_str(yaml_content).ok()?;

    Some((metadata.name, metadata.description))
}

/// Get the latest commit SHA for a path in a repository
pub fn get_latest_commit(github_url: &GitHubUrl, path: Option<&str>) -> Result<String> {
    let client = reqwest::blocking::Client::builder().user_agent(USER_AGENT).build()?;

    let mut url = format!("{}/commits?sha={}&per_page=1", github_url.api_url(), github_url.branch);

    if let Some(p) = path {
        url.push_str(&format!("&path={}", p));
    }

    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to fetch commits from {}", url))?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch commits: HTTP {}", response.status());
    }

    let commits: Vec<serde_json::Value> = response.json()?;

    commits
        .first()
        .and_then(|c| c["sha"].as_str())
        .map(|s| s[..7].to_string()) // Short SHA
        .with_context(|| "No commits found")
}

/// Download and extract a skill from a GitHub repository
///
/// Downloads the tarball, extracts the specific skill folder, and copies to destination.
pub fn download_skill(github_url: &GitHubUrl, skill_path: &str, dest: &Path, commit: Option<&str>) -> Result<String> {
    let git_ref = commit.unwrap_or(&github_url.branch);

    let client = reqwest::blocking::Client::builder().user_agent(USER_AGENT).build()?;

    // Download tarball
    let tarball_url = github_url.tarball_url(git_ref);
    let response = client
        .get(&tarball_url)
        .send()
        .with_context(|| format!("Failed to download from {}", tarball_url))?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Failed to download tarball: HTTP {} from {}",
            response.status(),
            tarball_url
        );
    }

    let bytes = response.bytes()?;

    // Get the actual commit SHA from response headers or fetch it
    let commit_sha = commit.map(|s| s.to_string()).unwrap_or_else(|| {
        get_latest_commit(github_url, Some(skill_path)).unwrap_or_else(|err| {
            println!(
                "Warning: failed to resolve latest commit for {} ({}), using {}",
                github_url.repo, err, git_ref
            );
            git_ref.to_string()
        })
    });

    // Extract tarball
    let cursor = Cursor::new(bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = Archive::new(decoder);

    // Create temp directory for extraction
    let temp_dir = tempfile::tempdir()?;

    // Extract all files
    archive.unpack(temp_dir.path())?;

    // Find the extracted directory (GitHub tarballs have a prefix like "owner-repo-sha/")
    let extracted_dir = fs::read_dir(temp_dir.path())?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .with_context(|| "Failed to find extracted directory")?
        .path();

    // Find the skill within the extracted archive
    let skill_source = extracted_dir.join(skill_path);

    if !skill_source.exists() {
        anyhow::bail!("Skill path '{}' not found in repository", skill_path);
    }

    // Verify it has SKILL.md
    if !skill_source.join("SKILL.md").exists() {
        anyhow::bail!("Invalid skill: no SKILL.md found in '{}'", skill_path);
    }

    // Create destination and copy files
    fs::create_dir_all(dest)?;
    copy_dir_contents(&skill_source, dest)?;

    Ok(commit_sha)
}

/// Recursively copy directory contents
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            fs::create_dir_all(&dst_path)?;
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_md_content() {
        let content = r#"---
name: test-skill
description: A test skill
---
# Test Skill
Some content here.
"#;
        let result = parse_skill_md_content(content);
        assert!(result.is_some());
        let (name, desc) = result.unwrap();
        assert_eq!(name, "test-skill");
        assert_eq!(desc, Some("A test skill".to_string()));
    }

    #[test]
    fn test_parse_skill_md_content_no_description() {
        let content = r#"---
name: minimal-skill
---
# Minimal
"#;
        let result = parse_skill_md_content(content);
        assert!(result.is_some());
        let (name, desc) = result.unwrap();
        assert_eq!(name, "minimal-skill");
        assert!(desc.is_none());
    }

    #[test]
    fn test_parse_skill_md_content_invalid() {
        let content = "# No frontmatter here";
        let result = parse_skill_md_content(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_github_url_simple() {
        let url = parse_github_url("https://github.com/owner/repo").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert_eq!(url.branch, "main");
        assert!(url.path.is_none());
    }

    #[test]
    fn test_parse_github_url_with_branch() {
        let url = parse_github_url("https://github.com/owner/repo/tree/develop").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert_eq!(url.branch, "develop");
        assert!(url.path.is_none());
    }

    #[test]
    fn test_parse_github_url_with_path() {
        let url = parse_github_url("https://github.com/owner/repo/tree/main/path/to/folder").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert_eq!(url.branch, "main");
        assert_eq!(url.path, Some("path/to/folder".to_string()));
    }

    #[test]
    fn test_parse_github_url_no_protocol() {
        let url = parse_github_url("github.com/owner/repo").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
    }

    #[test]
    fn test_parse_github_url_trailing_slash() {
        let url = parse_github_url("https://github.com/owner/repo/").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
    }

    #[test]
    fn test_parse_github_url_invalid() {
        assert!(parse_github_url("https://gitlab.com/owner/repo").is_err());
        assert!(parse_github_url("https://github.com/owner").is_err());
        assert!(parse_github_url("not-a-url").is_err());
    }
}
