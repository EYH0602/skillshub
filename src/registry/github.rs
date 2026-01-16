use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use tar::Archive;

use super::models::{GitHubUrl, TapRegistry};

/// User agent for API requests
const USER_AGENT: &str = "skillshub";

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

/// Fetch the registry.json from a tap repository
pub fn fetch_tap_registry(github_url: &GitHubUrl, registry_path: &str) -> Result<TapRegistry> {
    let raw_url = github_url.raw_url(registry_path);

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;

    let response = client
        .get(&raw_url)
        .send()
        .with_context(|| format!("Failed to fetch registry from {}", raw_url))?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Failed to fetch registry: HTTP {} from {}",
            response.status(),
            raw_url
        );
    }

    let registry: TapRegistry = response
        .json()
        .with_context(|| "Failed to parse registry.json")?;

    Ok(registry)
}

/// Get the latest commit SHA for a path in a repository
pub fn get_latest_commit(github_url: &GitHubUrl, path: Option<&str>) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;

    let mut url = format!(
        "{}/commits?sha={}&per_page=1",
        github_url.api_url(),
        github_url.branch
    );

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
pub fn download_skill(
    github_url: &GitHubUrl,
    skill_path: &str,
    dest: &Path,
    commit: Option<&str>,
) -> Result<String> {
    let git_ref = commit.unwrap_or(&github_url.branch);

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;

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
        let url =
            parse_github_url("https://github.com/owner/repo/tree/main/path/to/folder").unwrap();
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
