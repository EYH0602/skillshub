use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tar::Archive;

use super::models::{GitHubUrl, SkillEntry, TapRegistry};
use crate::skill::SkillMetadata;

/// User agent for API requests
const USER_AGENT: &str = "skillshub";

/// Maximum number of retries for transient errors
const MAX_RETRIES: u32 = 5;

/// Initial backoff duration in milliseconds (overridden in tests)
#[cfg(not(test))]
const INITIAL_BACKOFF_MS: u64 = 1000;
#[cfg(test)]
const INITIAL_BACKOFF_MS: u64 = 10;

/// Maximum backoff duration in milliseconds
const MAX_BACKOFF_MS: u64 = 60_000;

/// Maximum time to wait for a rate limit reset (seconds)
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 300;

/// Parsed rate limit information from GitHub response headers
struct RateLimitInfo {
    remaining: Option<u64>,
    reset: Option<i64>,
}

impl RateLimitInfo {
    /// Parse rate limit headers from a response
    fn from_response(resp: &Response) -> Self {
        let remaining = resp
            .headers()
            .get("X-RateLimit-Remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        let reset = resp
            .headers()
            .get("X-RateLimit-Reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok());

        Self { remaining, reset }
    }

    /// Compute the duration to wait until the rate limit resets
    fn wait_duration(&self) -> Option<Duration> {
        self.reset.map(|reset_ts| {
            let now = chrono::Utc::now().timestamp();
            let wait = reset_ts - now;
            if wait > 0 {
                Duration::from_secs(wait as u64)
            } else {
                // Reset time already passed, retry immediately
                Duration::from_secs(1)
            }
        })
    }
}

/// Compute exponential backoff duration for a given attempt (1-based)
fn backoff_duration(attempt: u32) -> Duration {
    let base_ms = INITIAL_BACKOFF_MS.saturating_mul(1u64 << (attempt.saturating_sub(1)));
    let jitter = simple_jitter_ms();
    let total_ms = base_ms.saturating_add(jitter).min(MAX_BACKOFF_MS);
    Duration::from_millis(total_ms)
}

/// Generate a simple jitter value (0-499ms) without requiring a random number crate
fn simple_jitter_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 % 500)
        .unwrap_or(0)
}

/// Determine how long to wait before retrying based on response headers or backoff
fn retry_after_from_response(resp: &Response, attempt: u32) -> Duration {
    // Check Retry-After header first
    if let Some(retry_after) = resp
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
    {
        return Duration::from_secs(retry_after);
    }

    // Check X-RateLimit-Reset header
    let rate_info = RateLimitInfo::from_response(resp);
    if let Some(wait) = rate_info.wait_duration() {
        return wait;
    }

    // Fall back to exponential backoff
    backoff_duration(attempt)
}

/// Print a rate limit wait message to stderr
fn print_rate_limit_wait(reason: &str, wait_secs: u64, attempt: u32) {
    eprint!(
        "  {} Waiting {}s before retrying (attempt {}/{})...",
        reason, wait_secs, attempt, MAX_RETRIES
    );
    if std::env::var("GITHUB_TOKEN").is_err() {
        eprint!("\n  Tip: Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour).");
    }
    eprintln!();
}

/// Send an HTTP request with retry logic for rate limits, server errors, and network errors.
///
/// The `build_request` closure is called on each attempt since `RequestBuilder` is consumed
/// on `.send()`.
fn send_with_retry<F>(build_request: F, url: &str) -> Result<Response>
where
    F: Fn() -> RequestBuilder,
{
    let mut attempt = 0u32;

    loop {
        attempt += 1;

        let result = build_request().send();

        match result {
            Ok(resp) => {
                let status = resp.status();

                // 429 Too Many Requests
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    if attempt >= MAX_RETRIES {
                        anyhow::bail!("Rate limited (HTTP 429) after {} retries for {}", MAX_RETRIES, url);
                    }
                    let wait = retry_after_from_response(&resp, attempt);
                    let wait_secs = wait.as_secs();
                    print_rate_limit_wait("Rate limited (429).", wait_secs, attempt);
                    std::thread::sleep(wait);
                    continue;
                }

                // 403 with rate limit exhausted
                if status == reqwest::StatusCode::FORBIDDEN {
                    let rate_info = RateLimitInfo::from_response(&resp);
                    if rate_info.remaining == Some(0) {
                        if attempt >= MAX_RETRIES {
                            anyhow::bail!(
                                "Rate limit exceeded (HTTP 403) after {} retries for {}",
                                MAX_RETRIES,
                                url
                            );
                        }
                        if let Some(wait) = rate_info.wait_duration() {
                            if wait.as_secs() > MAX_RATE_LIMIT_WAIT_SECS {
                                anyhow::bail!(
                                    "Rate limit reset is {}s away (>{} max). Set GITHUB_TOKEN for higher limits.",
                                    wait.as_secs(),
                                    MAX_RATE_LIMIT_WAIT_SECS
                                );
                            }
                            print_rate_limit_wait("Rate limit exceeded (403).", wait.as_secs(), attempt);
                            std::thread::sleep(wait);
                            continue;
                        }
                        // No reset header — fall through to return the 403
                    }
                    // Regular 403 (not rate limit) — return immediately
                    return Ok(resp);
                }

                // 5xx server errors
                if status.is_server_error() {
                    if attempt >= MAX_RETRIES {
                        anyhow::bail!(
                            "Server error (HTTP {}) after {} retries for {}",
                            status.as_u16(),
                            MAX_RETRIES,
                            url
                        );
                    }
                    let wait = backoff_duration(attempt);
                    eprintln!(
                        "  Server error (HTTP {}). Retrying in {}s... (attempt {}/{})",
                        status.as_u16(),
                        wait.as_secs(),
                        attempt,
                        MAX_RETRIES
                    );
                    std::thread::sleep(wait);
                    continue;
                }

                // 200 with remaining=0: proactive warning
                if status.is_success() {
                    let rate_info = RateLimitInfo::from_response(&resp);
                    if rate_info.remaining == Some(0) {
                        if let Some(wait) = rate_info.wait_duration() {
                            eprintln!(
                                "  Warning: Rate limit exhausted. Next request will wait {}s for reset.",
                                wait.as_secs()
                            );
                        }
                    }
                }

                // All other responses (200, 404, other 4xx) — return for caller to handle
                return Ok(resp);
            }
            Err(e) => {
                // Network errors
                if attempt >= MAX_RETRIES {
                    anyhow::bail!("Network error after {} retries for {}: {}", MAX_RETRIES, url, e);
                }
                let wait = backoff_duration(attempt);
                eprintln!(
                    "  Network error: {}. Retrying in {}s... (attempt {}/{})",
                    e,
                    wait.as_secs(),
                    attempt,
                    MAX_RETRIES
                );
                std::thread::sleep(wait);
            }
        }
    }
}

/// Build an HTTP client with GitHub token if available
fn build_client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .context("Failed to build HTTP client")
}

/// Add GitHub token authentication to a request if GITHUB_TOKEN is set
fn with_auth(request: RequestBuilder) -> RequestBuilder {
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        request.bearer_auth(token)
    } else {
        request
    }
}

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

/// GitHub Repository API response (partial)
#[derive(Debug, Deserialize)]
struct RepoInfo {
    default_branch: String,
}

/// Get the default branch for a repository from GitHub API
pub fn get_default_branch(owner: &str, repo: &str) -> Result<String> {
    let client = build_client()?;
    let api_base = std::env::var("SKILLSHUB_GITHUB_API_BASE").unwrap_or_else(|_| "https://api.github.com".to_string());
    let url = format!("{}/repos/{}/{}", api_base, owner, repo);

    let response = send_with_retry(|| with_auth(client.get(&url)), &url)?;

    let status = response.status();
    if !status.is_success() {
        if status == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!(
                "Repository not found on GitHub: {}/{}\n\
                 Please check that:\n\
                 - The repository exists and is spelled correctly\n\
                 - The repository is public (or GITHUB_TOKEN is set for private repos)",
                owner,
                repo
            );
        }
        anyhow::bail!("Failed to fetch repo info: HTTP {}", status);
    }

    let info: RepoInfo = response
        .json()
        .with_context(|| "Failed to parse repository info response")?;
    Ok(info.default_branch)
}

/// Parse a GitHub URL or repository identifier into components
///
/// Supports formats:
/// - owner/repo (short format, uses repo's default branch)
/// - https://github.com/owner/repo (uses repo's default branch)
/// - https://github.com/owner/repo/tree/branch
/// - https://github.com/owner/repo/tree/branch/path/to/folder
///
/// When no branch is specified in the URL, `branch` will be `None`,
/// indicating that the repository's default branch should be used.
pub fn parse_github_url(url: &str) -> Result<GitHubUrl> {
    let url = url.trim_end_matches('/');

    // Try to strip protocol prefixes
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("github.com/"));

    // If no prefix was stripped, check if it's a valid owner/repo format
    let path = match path {
        Some(p) => p,
        None => {
            // Check if it looks like owner/repo (no protocol, no dots in the first segment)
            if is_valid_repo_id(url) {
                url
            } else {
                anyhow::bail!(
                    "Invalid GitHub URL or repository ID: {}\n\
                     Expected formats:\n\
                     - owner/repo\n\
                     - https://github.com/owner/repo",
                    url
                );
            }
        }
    };

    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() < 2 {
        anyhow::bail!("Invalid repository ID: must be in 'owner/repo' format");
    }

    let owner = parts[0].to_string();
    let repo = parts[1].to_string();

    // Check for /tree/branch/path format
    let (branch, subpath) = if parts.len() > 3 && parts[2] == "tree" {
        let branch = Some(parts[3].to_string());
        let subpath = if parts.len() > 4 {
            Some(parts[4..].join("/"))
        } else {
            None
        };
        (branch, subpath)
    } else {
        // No branch specified - use None to indicate "use default branch"
        (None, None)
    };

    Ok(GitHubUrl {
        owner,
        repo,
        branch,
        path: subpath,
    })
}

/// Check if a string looks like a valid owner/repo identifier
/// Valid: "owner/repo", "my-org/my-repo", "user123/repo_name"
/// Invalid: "https://...", "gitlab.com/...", "just-one-part"
fn is_valid_repo_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('/').collect();

    // Must have exactly 2 parts for owner/repo
    if parts.len() != 2 {
        return false;
    }

    let owner = parts[0];
    let repo = parts[1];

    // Both parts must be non-empty
    if owner.is_empty() || repo.is_empty() {
        return false;
    }

    // Owner and repo should only contain valid GitHub username/repo characters
    // GitHub allows alphanumeric, hyphens, underscores, and dots
    let is_valid_part = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
            && !part.starts_with('-')
            && !part.starts_with('.')
    };

    is_valid_part(owner) && is_valid_part(repo)
}

/// Discover skills from a GitHub repository by scanning for SKILL.md files
///
/// Uses the GitHub Tree API to recursively find all SKILL.md files in the repo,
/// then fetches each one to extract metadata.
/// Set GITHUB_TOKEN environment variable to avoid rate limiting.
pub fn discover_skills_from_repo(github_url: &GitHubUrl, tap_name: &str) -> Result<TapRegistry> {
    let client = build_client()?;

    // Resolve branch: use specified branch or fetch the repository's default branch
    let branch = match &github_url.branch {
        Some(b) => b.clone(),
        None => get_default_branch(&github_url.owner, &github_url.repo)?,
    };

    // Fetch the full repo tree with recursive=1
    let tree_url = format!("{}/git/trees/{}?recursive=1", github_url.api_url(), branch);

    let response = send_with_retry(|| with_auth(client.get(&tree_url)), &tree_url)?;

    if !response.status().is_success() {
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!(
                "Branch '{}' not found in repository {}/{}\n\
                 Please check that the branch exists.",
                branch,
                github_url.owner,
                github_url.repo
            );
        }
        anyhow::bail!("Failed to fetch repo tree: HTTP {} from {}", status, tree_url);
    }

    let tree_response: TreeResponse = response.json().with_context(|| "Failed to parse tree response")?;

    // Find all SKILL.md files
    // A SKILL.md can be at the root (path == "SKILL.md") or in subdirectories (path ends with "/SKILL.md")
    let skill_paths = extract_skill_paths(&tree_response.tree);

    if skill_paths.is_empty() {
        anyhow::bail!("No skills found in repository (no SKILL.md files detected)");
    }

    // Fetch metadata for each skill
    let mut skills = HashMap::new();
    for skill_path in &skill_paths {
        let skill_md_url = if skill_path.is_empty() {
            // Root-level SKILL.md
            github_url.raw_url("SKILL.md", &branch)
        } else {
            github_url.raw_url(&format!("{}/SKILL.md", skill_path), &branch)
        };

        // Note: raw.githubusercontent.com doesn't need auth, but we add it anyway
        match send_with_retry(|| with_auth(client.get(&skill_md_url)), &skill_md_url) {
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
                // For root-level skills, use the repo name
                let skill_name = if skill_path.is_empty() {
                    &github_url.repo
                } else {
                    skill_path.rsplit('/').next().unwrap_or(skill_path)
                };
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
pub fn get_latest_commit(github_url: &GitHubUrl, path: Option<&str>, resolved_branch: &str) -> Result<String> {
    let client = build_client()?;

    let mut url = format!("{}/commits?sha={}&per_page=1", github_url.api_url(), resolved_branch);

    if let Some(p) = path {
        url.push_str(&format!("&path={}", p));
    }

    let response = send_with_retry(|| with_auth(client.get(&url)), &url)?;

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
    // Resolve branch: use specified branch or fetch the repository's default branch
    let resolved_branch = match &github_url.branch {
        Some(b) => b.clone(),
        None => get_default_branch(&github_url.owner, &github_url.repo)?,
    };

    let git_ref = commit.unwrap_or(&resolved_branch);

    let client = build_client()?;

    // Download tarball
    let tarball_url = github_url.tarball_url(git_ref);
    let response = send_with_retry(|| with_auth(client.get(&tarball_url)), &tarball_url)?;

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
        get_latest_commit(github_url, Some(skill_path), &resolved_branch).unwrap_or_else(|err| {
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
    // For root-level skills (empty path), the skill is the extracted directory itself
    let skill_source = if skill_path.is_empty() {
        extracted_dir.clone()
    } else {
        extracted_dir.join(skill_path)
    };

    if !skill_source.exists() {
        anyhow::bail!("Skill path '{}' not found in repository", skill_path);
    }

    // Verify it has SKILL.md
    if !skill_source.join("SKILL.md").exists() {
        anyhow::bail!(
            "Invalid skill: no SKILL.md found in '{}'",
            if skill_path.is_empty() { "(root)" } else { skill_path }
        );
    }

    // Create destination and copy files
    fs::create_dir_all(dest)?;
    copy_dir_contents(&skill_source, dest)?;

    Ok(commit_sha)
}

/// Extract skill directory paths from a list of tree entries.
///
/// Finds entries that are SKILL.md files (either at root or in subdirectories)
/// and returns the parent directory path for each. A root-level SKILL.md
/// produces an empty string path.
fn extract_skill_paths(tree: &[TreeEntry]) -> Vec<String> {
    tree.iter()
        .filter(|entry| entry.entry_type == "blob" && (entry.path == "SKILL.md" || entry.path.ends_with("/SKILL.md")))
        .map(|entry| {
            entry
                .path
                .rsplit_once('/')
                .map(|(parent, _)| parent.to_string())
                .unwrap_or_default()
        })
        .collect()
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
        assert!(url.branch.is_none()); // No branch specified = None (use repo's default)
        assert!(url.path.is_none());
    }

    #[test]
    fn test_parse_github_url_with_branch() {
        let url = parse_github_url("https://github.com/owner/repo/tree/develop").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert_eq!(url.branch, Some("develop".to_string()));
        assert!(url.path.is_none());
    }

    #[test]
    fn test_parse_github_url_with_path() {
        let url = parse_github_url("https://github.com/owner/repo/tree/main/path/to/folder").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert_eq!(url.branch, Some("main".to_string()));
        assert_eq!(url.path, Some("path/to/folder".to_string()));
    }

    #[test]
    fn test_parse_github_url_with_master_branch() {
        // Explicitly specifying master branch should work
        let url = parse_github_url("https://github.com/owner/repo/tree/master").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert_eq!(url.branch, Some("master".to_string()));
        assert!(url.path.is_none());
    }

    #[test]
    fn test_parse_github_url_no_protocol() {
        let url = parse_github_url("github.com/owner/repo").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert!(url.branch.is_none()); // No branch specified = None
    }

    #[test]
    fn test_parse_github_url_trailing_slash() {
        let url = parse_github_url("https://github.com/owner/repo/").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert!(url.branch.is_none());
    }

    #[test]
    fn test_parse_github_url_invalid() {
        assert!(parse_github_url("https://gitlab.com/owner/repo").is_err());
        assert!(parse_github_url("https://github.com/owner").is_err());
        assert!(parse_github_url("not-a-url").is_err());
    }

    #[test]
    fn test_parse_github_url_repo_id_simple() {
        let url = parse_github_url("owner/repo").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo");
        assert!(url.branch.is_none()); // No branch specified = None (use repo's default)
        assert!(url.path.is_none());
    }

    #[test]
    fn test_parse_github_url_repo_id_with_hyphens() {
        let url = parse_github_url("my-org/my-repo").unwrap();
        assert_eq!(url.owner, "my-org");
        assert_eq!(url.repo, "my-repo");
        assert!(url.branch.is_none());
    }

    #[test]
    fn test_parse_github_url_repo_id_with_underscores() {
        let url = parse_github_url("user_name/repo_name").unwrap();
        assert_eq!(url.owner, "user_name");
        assert_eq!(url.repo, "repo_name");
        assert!(url.branch.is_none());
    }

    #[test]
    fn test_parse_github_url_repo_id_with_dots() {
        let url = parse_github_url("owner/repo.js").unwrap();
        assert_eq!(url.owner, "owner");
        assert_eq!(url.repo, "repo.js");
        assert!(url.branch.is_none());
    }

    #[test]
    fn test_is_valid_repo_id() {
        assert!(is_valid_repo_id("owner/repo"));
        assert!(is_valid_repo_id("my-org/my-repo"));
        assert!(is_valid_repo_id("user123/repo_name"));
        assert!(is_valid_repo_id("Owner/Repo.js"));
    }

    #[test]
    fn test_is_valid_repo_id_invalid() {
        // Not enough parts
        assert!(!is_valid_repo_id("just-one-part"));
        // Too many parts
        assert!(!is_valid_repo_id("owner/repo/extra"));
        // Empty parts
        assert!(!is_valid_repo_id("/repo"));
        assert!(!is_valid_repo_id("owner/"));
        // Starts with invalid char
        assert!(!is_valid_repo_id("-owner/repo"));
        assert!(!is_valid_repo_id(".owner/repo"));
        // Invalid characters
        assert!(!is_valid_repo_id("owner/repo name"));
    }

    /// Helper to create a TreeEntry for tests
    fn tree_entry(path: &str, entry_type: &str) -> TreeEntry {
        TreeEntry {
            path: path.to_string(),
            entry_type: entry_type.to_string(),
        }
    }

    #[test]
    fn test_extract_skill_paths_subdirectory() {
        let tree = vec![
            tree_entry("skills/code-reviewer/SKILL.md", "blob"),
            tree_entry("skills/test-skill/SKILL.md", "blob"),
            tree_entry("README.md", "blob"),
        ];
        let paths = extract_skill_paths(&tree);
        assert_eq!(paths, vec!["skills/code-reviewer", "skills/test-skill"]);
    }

    #[test]
    fn test_extract_skill_paths_root_level() {
        // Repo that IS a skill (SKILL.md at root)
        let tree = vec![tree_entry("SKILL.md", "blob"), tree_entry("README.md", "blob")];
        let paths = extract_skill_paths(&tree);
        assert_eq!(paths, vec![""]);
    }

    #[test]
    fn test_extract_skill_paths_root_and_subdirectory() {
        // Repo with both root-level and subdirectory skills
        let tree = vec![
            tree_entry("SKILL.md", "blob"),
            tree_entry("skills/other-skill/SKILL.md", "blob"),
            tree_entry("README.md", "blob"),
        ];
        let paths = extract_skill_paths(&tree);
        assert_eq!(paths, vec!["", "skills/other-skill"]);
    }

    #[test]
    fn test_extract_skill_paths_no_skills() {
        let tree = vec![tree_entry("README.md", "blob"), tree_entry("src/main.rs", "blob")];
        let paths = extract_skill_paths(&tree);
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_skill_paths_ignores_trees() {
        // Directories (type "tree") should be ignored even if named SKILL.md
        let tree = vec![
            tree_entry("SKILL.md", "tree"),
            tree_entry("skills/test/SKILL.md", "blob"),
        ];
        let paths = extract_skill_paths(&tree);
        assert_eq!(paths, vec!["skills/test"]);
    }

    #[test]
    fn test_extract_skill_paths_deep_nesting() {
        let tree = vec![tree_entry("a/b/c/SKILL.md", "blob")];
        let paths = extract_skill_paths(&tree);
        assert_eq!(paths, vec!["a/b/c"]);
    }

    // --- Rate limit and retry tests ---

    #[test]
    fn test_backoff_duration_exponential() {
        // With INITIAL_BACKOFF_MS=10 in test mode, backoff should grow exponentially
        let d1 = backoff_duration(1);
        let d2 = backoff_duration(2);
        let d3 = backoff_duration(3);

        // attempt 1: base=10ms, attempt 2: base=20ms, attempt 3: base=40ms
        // Plus jitter (0-499ms), so just check ordering and reasonable bounds
        assert!(
            d1.as_millis() >= 10,
            "attempt 1 should be >= 10ms, got {}ms",
            d1.as_millis()
        );
        assert!(
            d2.as_millis() >= 20,
            "attempt 2 should be >= 20ms, got {}ms",
            d2.as_millis()
        );
        assert!(
            d3.as_millis() >= 40,
            "attempt 3 should be >= 40ms, got {}ms",
            d3.as_millis()
        );
    }

    #[test]
    fn test_backoff_capped_at_max() {
        // Very high attempt number should still be capped at MAX_BACKOFF_MS
        let d = backoff_duration(30);
        assert!(
            d.as_millis() <= MAX_BACKOFF_MS as u128,
            "backoff should be capped at {}ms, got {}ms",
            MAX_BACKOFF_MS,
            d.as_millis()
        );
    }

    #[test]
    fn test_simple_jitter_ms_in_range() {
        let jitter = simple_jitter_ms();
        assert!(jitter < 500, "jitter should be < 500, got {}", jitter);
    }

    /// Helper: start a tokio runtime, start a wiremock server, and return its URI.
    /// The closure receives the mock server to set up mocks, then we run blocking code.
    fn with_mock_server<F, G, R>(setup: F, test: G) -> R
    where
        F: FnOnce(&wiremock::MockServer) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>>,
        G: FnOnce(String) -> R,
    {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(wiremock::MockServer::start());
        rt.block_on(setup(&server));
        let url = server.uri();
        test(url)
    }

    #[test]
    fn test_send_with_retry_success() {
        with_mock_server(
            |server| {
                Box::pin(async move {
                    wiremock::Mock::given(wiremock::matchers::method("GET"))
                        .and(wiremock::matchers::path("/test"))
                        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("ok"))
                        .mount(server)
                        .await;
                })
            },
            |base_url| {
                let url = format!("{}/test", base_url);
                let client = build_client().unwrap();
                let result = send_with_retry(|| client.get(&url), &url);
                assert!(result.is_ok());
                let resp = result.unwrap();
                assert_eq!(resp.status(), 200);
                assert_eq!(resp.text().unwrap(), "ok");
            },
        );
    }

    #[test]
    fn test_retry_on_server_error() {
        // Use an atomic counter to track calls and return 500 on first, 200 on second
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(wiremock::MockServer::start());

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = call_count.clone();

        rt.block_on(async {
            // First response: 500
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(wiremock::ResponseTemplate::new(500))
                .up_to_n_times(1)
                .mount(&server)
                .await;

            // Second response: 200
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("recovered"))
                .mount(&server)
                .await;
        });

        let url = format!("{}/test", server.uri());
        let client = build_client().unwrap();
        let result = send_with_retry(
            || {
                counter.fetch_add(1, Ordering::SeqCst);
                client.get(&url)
            },
            &url,
        );

        assert!(result.is_ok(), "should succeed after retry");
        let resp = result.unwrap();
        assert_eq!(resp.status(), 200);
        assert!(
            call_count.load(Ordering::SeqCst) >= 2,
            "should have retried at least once"
        );
    }

    #[test]
    fn test_retry_on_429() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(wiremock::MockServer::start());

        rt.block_on(async {
            // First response: 429 with Retry-After
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(
                    wiremock::ResponseTemplate::new(429)
                        .insert_header("Retry-After", "0")
                        .set_body_string("rate limited"),
                )
                .up_to_n_times(1)
                .mount(&server)
                .await;

            // Second response: 200
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("ok"))
                .mount(&server)
                .await;
        });

        let url = format!("{}/test", server.uri());
        let client = build_client().unwrap();
        let result = send_with_retry(|| client.get(&url), &url);

        assert!(result.is_ok(), "should succeed after 429 retry");
        assert_eq!(result.unwrap().status(), 200);
    }

    #[test]
    fn test_retry_on_403_rate_limit() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(wiremock::MockServer::start());
        let reset_ts = (chrono::Utc::now().timestamp() + 1).to_string();

        rt.block_on(async {
            // First response: 403 with X-RateLimit-Remaining: 0
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(
                    wiremock::ResponseTemplate::new(403)
                        .insert_header("X-RateLimit-Remaining", "0")
                        .insert_header("X-RateLimit-Reset", &reset_ts)
                        .set_body_string("rate limited"),
                )
                .up_to_n_times(1)
                .mount(&server)
                .await;

            // Second response: 200
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("ok"))
                .mount(&server)
                .await;
        });

        let url = format!("{}/test", server.uri());
        let client = build_client().unwrap();
        let result = send_with_retry(|| client.get(&url), &url);

        assert!(result.is_ok(), "should succeed after 403 rate limit retry");
        assert_eq!(result.unwrap().status(), 200);
    }

    #[test]
    fn test_no_retry_on_404() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(wiremock::MockServer::start());

        rt.block_on(async {
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(wiremock::ResponseTemplate::new(404))
                .mount(&server)
                .await;
        });

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = call_count.clone();

        let url = format!("{}/test", server.uri());
        let client = build_client().unwrap();
        let result = send_with_retry(
            || {
                counter.fetch_add(1, Ordering::SeqCst);
                client.get(&url)
            },
            &url,
        );

        assert!(result.is_ok(), "404 should be returned, not an error");
        assert_eq!(result.unwrap().status(), 404);
        assert_eq!(call_count.load(Ordering::SeqCst), 1, "should NOT retry on 404");
    }

    #[test]
    fn test_no_retry_on_regular_403() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(wiremock::MockServer::start());

        rt.block_on(async {
            // 403 without rate limit headers — should not retry
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(wiremock::ResponseTemplate::new(403).set_body_string("forbidden"))
                .mount(&server)
                .await;
        });

        let call_count = Arc::new(AtomicU32::new(0));
        let counter = call_count.clone();

        let url = format!("{}/test", server.uri());
        let client = build_client().unwrap();
        let result = send_with_retry(
            || {
                counter.fetch_add(1, Ordering::SeqCst);
                client.get(&url)
            },
            &url,
        );

        assert!(result.is_ok(), "regular 403 should be returned");
        assert_eq!(result.unwrap().status(), 403);
        assert_eq!(call_count.load(Ordering::SeqCst), 1, "should NOT retry on regular 403");
    }

    #[test]
    fn test_gives_up_after_max_retries() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(wiremock::MockServer::start());

        rt.block_on(async {
            // Always return 500
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path("/test"))
                .respond_with(wiremock::ResponseTemplate::new(500))
                .mount(&server)
                .await;
        });

        let url = format!("{}/test", server.uri());
        let client = build_client().unwrap();
        let result = send_with_retry(|| client.get(&url), &url);

        assert!(result.is_err(), "should fail after max retries");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("after 5 retries"),
            "error should mention retry count: {}",
            err_msg
        );
    }
}
