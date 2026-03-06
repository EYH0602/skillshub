# Gist Support Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow `skillshub add` to install skills from GitHub Gist URLs.

**Architecture:** Detect `gist.github.com` URLs in the existing `add` command, fetch gist via GitHub REST API (`GET /gists/{id}`), discover skills from gist files, and install them under `owner/gists/skill-name` namespace. Add `gist_updated_at` field to `InstalledSkill` for update tracking.

**Tech Stack:** Rust, reqwest (HTTP), serde_json (API parsing), existing GitHub API helpers (`build_client`, `with_auth`, `send_with_retry`)

---

### Task 1: Add `gist_updated_at` field to `InstalledSkill`

**Files:**
- Modify: `src/registry/models.rs:50-72` (InstalledSkill struct)

**Step 1: Write the failing test**

Add to the existing test module in `src/registry/models.rs`:

```rust
#[test]
fn test_installed_skill_gist_updated_at_field() {
    let skill = InstalledSkill {
        tap: "garrytan/gists".to_string(),
        skill: "plan-exit-review".to_string(),
        commit: None,
        installed_at: chrono::Utc::now(),
        source_url: Some("https://gist.github.com/garrytan/001f9074cab1a8f545ebecbc73a813df".to_string()),
        source_path: None,
        gist_updated_at: Some("2025-01-15T10:30:00Z".to_string()),
    };

    // Roundtrip serialize/deserialize
    let json = serde_json::to_string(&skill).unwrap();
    let restored: InstalledSkill = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.gist_updated_at, Some("2025-01-15T10:30:00Z".to_string()));
}

#[test]
fn test_installed_skill_without_gist_updated_at_deserializes() {
    // Simulate loading old database entry without the new field
    let json = r#"{
        "tap": "owner/repo",
        "skill": "my-skill",
        "commit": "abc1234",
        "installed_at": "2025-01-01T00:00:00Z",
        "source_url": null,
        "source_path": null
    }"#;
    let skill: InstalledSkill = serde_json::from_str(json).unwrap();
    assert!(skill.gist_updated_at.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib registry::models::tests::test_installed_skill_gist_updated_at_field -- --exact`
Expected: FAIL — `gist_updated_at` field doesn't exist

**Step 3: Write minimal implementation**

In `InstalledSkill` struct (`src/registry/models.rs:50-72`), add:

```rust
/// Gist updated_at timestamp for tracking gist skill freshness (None for non-gist skills)
#[serde(default, skip_serializing_if = "Option::is_none")]
pub gist_updated_at: Option<String>,
```

Then fix every location that constructs an `InstalledSkill` to include `gist_updated_at: None`. There are several:
- `src/registry/skill.rs:128` (install_skill_internal)
- `src/registry/skill.rs:219` (add_skill_from_url)
- `src/registry/db.rs` tests (multiple)

**Step 4: Run test to verify it passes**

Run: `cargo test --lib registry::models -- --nocapture`
Expected: PASS

**Step 5: Commit**

```
feat: add gist_updated_at field to InstalledSkill model
```

---

### Task 2: Add gist URL parsing to `github.rs`

**Files:**
- Modify: `src/registry/github.rs` (add `parse_gist_url` function and `GistInfo` struct)

**Step 1: Write the failing tests**

Add to the test module in `src/registry/github.rs`:

```rust
#[test]
fn test_parse_gist_url_full() {
    let result = parse_gist_url("https://gist.github.com/garrytan/001f9074cab1a8f545ebecbc73a813df");
    assert!(result.is_some());
    let (owner, gist_id) = result.unwrap();
    assert_eq!(owner, "garrytan");
    assert_eq!(gist_id, "001f9074cab1a8f545ebecbc73a813df");
}

#[test]
fn test_parse_gist_url_http() {
    let result = parse_gist_url("http://gist.github.com/user/abc123def456");
    assert!(result.is_some());
    let (owner, gist_id) = result.unwrap();
    assert_eq!(owner, "user");
    assert_eq!(gist_id, "abc123def456");
}

#[test]
fn test_parse_gist_url_no_protocol() {
    let result = parse_gist_url("gist.github.com/user/abc123");
    assert!(result.is_some());
}

#[test]
fn test_parse_gist_url_not_a_gist() {
    assert!(parse_gist_url("https://github.com/user/repo").is_none());
    assert!(parse_gist_url("https://example.com/user/abc").is_none());
    assert!(parse_gist_url("user/repo").is_none());
}

#[test]
fn test_parse_gist_url_trailing_slash() {
    let result = parse_gist_url("https://gist.github.com/garrytan/abc123/");
    assert!(result.is_some());
    let (owner, gist_id) = result.unwrap();
    assert_eq!(owner, "garrytan");
    assert_eq!(gist_id, "abc123");
}

#[test]
fn test_parse_gist_url_with_revision() {
    // Gist URLs sometimes have a revision hash appended
    let result = parse_gist_url("https://gist.github.com/garrytan/abc123/def456");
    assert!(result.is_some());
    let (owner, gist_id) = result.unwrap();
    assert_eq!(owner, "garrytan");
    assert_eq!(gist_id, "abc123");
}

#[test]
fn test_is_gist_url() {
    assert!(is_gist_url("https://gist.github.com/user/abc123"));
    assert!(is_gist_url("http://gist.github.com/user/abc123"));
    assert!(is_gist_url("gist.github.com/user/abc123"));
    assert!(!is_gist_url("https://github.com/user/repo"));
    assert!(!is_gist_url("user/repo"));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib registry::github::tests::test_parse_gist_url -- --nocapture`
Expected: FAIL — functions don't exist

**Step 3: Write minimal implementation**

Add to `src/registry/github.rs`:

```rust
/// Check if a URL points to a GitHub Gist
pub fn is_gist_url(url: &str) -> bool {
    let url = url.trim_end_matches('/');
    url.starts_with("https://gist.github.com/")
        || url.starts_with("http://gist.github.com/")
        || url.starts_with("gist.github.com/")
}

/// Parse a GitHub Gist URL into (owner, gist_id)
///
/// Supports formats:
/// - https://gist.github.com/owner/gist_id
/// - http://gist.github.com/owner/gist_id
/// - gist.github.com/owner/gist_id
/// - URLs with trailing slash or revision suffix
///
/// Returns None if the URL is not a gist URL.
pub fn parse_gist_url(url: &str) -> Option<(String, String)> {
    let url = url.trim_end_matches('/');

    let path = url
        .strip_prefix("https://gist.github.com/")
        .or_else(|| url.strip_prefix("http://gist.github.com/"))
        .or_else(|| url.strip_prefix("gist.github.com/"))?;

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }

    Some((parts[0].to_string(), parts[1].to_string()))
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib registry::github::tests::test_parse_gist -- --nocapture`
Expected: PASS

**Step 5: Commit**

```
feat: add gist URL parsing (is_gist_url, parse_gist_url)
```

---

### Task 3: Add `fetch_gist` API function

**Files:**
- Modify: `src/registry/github.rs` (add `GistFile`, `GistResponse`, `fetch_gist`)

**Step 1: Write the failing test**

Add a test that uses wiremock to mock the gist API response:

```rust
// In src/registry/github.rs test module (which uses #[cfg(test)])
// Note: wiremock tests need to be async, place in tests at bottom of file

#[cfg(test)]
mod gist_tests {
    use super::*;

    #[test]
    fn test_gist_response_deserialize() {
        let json = r#"{
            "id": "abc123",
            "owner": { "login": "garrytan" },
            "updated_at": "2025-01-15T10:30:00Z",
            "files": {
                "SKILL.md": {
                    "filename": "SKILL.md",
                    "content": "---\nname: my-skill\ndescription: A skill\n---\n# My Skill"
                }
            }
        }"#;

        let gist: GistResponse = serde_json::from_str(json).unwrap();
        assert_eq!(gist.id, "abc123");
        assert_eq!(gist.owner.login, "garrytan");
        assert_eq!(gist.updated_at, "2025-01-15T10:30:00Z");
        assert_eq!(gist.files.len(), 1);
        assert!(gist.files.contains_key("SKILL.md"));
    }

    #[test]
    fn test_gist_response_multiple_files() {
        let json = r#"{
            "id": "abc123",
            "owner": { "login": "user" },
            "updated_at": "2025-01-15T10:30:00Z",
            "files": {
                "Garry's plan-exit-review skill": {
                    "filename": "Garry's plan-exit-review skill",
                    "content": "---\nname: plan-exit-review\ndescription: Review plans\n---\n# Content"
                },
                "another-skill": {
                    "filename": "another-skill",
                    "content": "---\nname: another-skill\ndescription: Another skill\n---\n# Another"
                }
            }
        }"#;

        let gist: GistResponse = serde_json::from_str(json).unwrap();
        assert_eq!(gist.files.len(), 2);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib registry::github::gist_tests -- --nocapture`
Expected: FAIL — structs don't exist

**Step 3: Write minimal implementation**

Add to `src/registry/github.rs`:

```rust
/// GitHub Gist API response
#[derive(Debug, Deserialize)]
pub struct GistResponse {
    pub id: String,
    pub owner: GistOwner,
    pub updated_at: String,
    pub files: HashMap<String, GistFile>,
}

/// Gist owner info
#[derive(Debug, Deserialize)]
pub struct GistOwner {
    pub login: String,
}

/// A file within a gist
#[derive(Debug, Deserialize)]
pub struct GistFile {
    pub filename: String,
    pub content: Option<String>,
}

/// Fetch a gist from the GitHub API
///
/// Returns the parsed gist response including all file contents.
pub fn fetch_gist(gist_id: &str) -> Result<GistResponse> {
    let client = build_client()?;
    let api_base = GitHubUrl::github_api_base();
    let url = format!("{}/gists/{}", api_base, gist_id);

    let response = send_with_retry(|| with_auth(client.get(&url)), &url)?;

    let status = response.status();
    if !status.is_success() {
        if status == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!(
                "Gist not found: {}\n\
                 Please check that the gist ID is correct and the gist is public \
                 (or GITHUB_TOKEN is set for secret gists)",
                gist_id
            );
        }
        anyhow::bail!("Failed to fetch gist: HTTP {}", status);
    }

    let gist: GistResponse = response
        .json()
        .with_context(|| "Failed to parse gist API response")?;

    Ok(gist)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib registry::github::gist_tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```
feat: add gist API types and fetch_gist function
```

---

### Task 4: Add `discover_skills_from_gist` function

**Files:**
- Modify: `src/registry/github.rs` (add skill discovery from gist files)

This function implements the two-level discovery logic:
1. If any file is named `SKILL.md` → single skill
2. Otherwise → scan all files for valid frontmatter

**Step 1: Write the failing tests**

```rust
#[cfg(test)]
// Add to gist_tests module:

#[test]
fn test_discover_skills_from_gist_with_skill_md() {
    let mut files = HashMap::new();
    files.insert("SKILL.md".to_string(), GistFile {
        filename: "SKILL.md".to_string(),
        content: Some("---\nname: my-skill\ndescription: A cool skill\n---\n# My Skill\nInstructions here.".to_string()),
    });
    files.insert("notes.txt".to_string(), GistFile {
        filename: "notes.txt".to_string(),
        content: Some("Some notes".to_string()),
    });

    let gist = GistResponse {
        id: "abc123".to_string(),
        owner: GistOwner { login: "user".to_string() },
        updated_at: "2025-01-15T10:30:00Z".to_string(),
        files,
    };

    let skills = discover_skills_from_gist(&gist);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].0, "my-skill"); // skill name
    assert!(skills[0].1.contains("# My Skill")); // full content
}

#[test]
fn test_discover_skills_from_gist_multiple_valid_files() {
    let mut files = HashMap::new();
    files.insert("Garry's skill".to_string(), GistFile {
        filename: "Garry's skill".to_string(),
        content: Some("---\nname: plan-exit-review\ndescription: Review plans\n---\n# Content".to_string()),
    });
    files.insert("another-skill".to_string(), GistFile {
        filename: "another-skill".to_string(),
        content: Some("---\nname: code-helper\ndescription: Help with code\n---\n# Helper".to_string()),
    });
    files.insert("readme.txt".to_string(), GistFile {
        filename: "readme.txt".to_string(),
        content: Some("This is not a skill file".to_string()),
    });

    let gist = GistResponse {
        id: "abc123".to_string(),
        owner: GistOwner { login: "user".to_string() },
        updated_at: "2025-01-15T10:30:00Z".to_string(),
        files,
    };

    let skills = discover_skills_from_gist(&gist);
    assert_eq!(skills.len(), 2);
    let names: Vec<&str> = skills.iter().map(|s| s.0.as_str()).collect();
    assert!(names.contains(&"plan-exit-review"));
    assert!(names.contains(&"code-helper"));
}

#[test]
fn test_discover_skills_from_gist_no_valid_skills() {
    let mut files = HashMap::new();
    files.insert("notes.txt".to_string(), GistFile {
        filename: "notes.txt".to_string(),
        content: Some("Just some notes".to_string()),
    });

    let gist = GistResponse {
        id: "abc123".to_string(),
        owner: GistOwner { login: "user".to_string() },
        updated_at: "2025-01-15T10:30:00Z".to_string(),
        files,
    };

    let skills = discover_skills_from_gist(&gist);
    assert!(skills.is_empty());
}

#[test]
fn test_discover_skills_from_gist_skill_md_takes_priority() {
    // When SKILL.md exists, only that file is used (even if other files have valid frontmatter)
    let mut files = HashMap::new();
    files.insert("SKILL.md".to_string(), GistFile {
        filename: "SKILL.md".to_string(),
        content: Some("---\nname: main-skill\ndescription: The main one\n---\n# Main".to_string()),
    });
    files.insert("other".to_string(), GistFile {
        filename: "other".to_string(),
        content: Some("---\nname: other-skill\ndescription: Should be ignored\n---\n# Other".to_string()),
    });

    let gist = GistResponse {
        id: "abc123".to_string(),
        owner: GistOwner { login: "user".to_string() },
        updated_at: "2025-01-15T10:30:00Z".to_string(),
        files,
    };

    let skills = discover_skills_from_gist(&gist);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].0, "main-skill");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib registry::github::gist_tests::test_discover -- --nocapture`
Expected: FAIL — function doesn't exist

**Step 3: Write minimal implementation**

Add to `src/registry/github.rs`:

```rust
/// Discover skills from a fetched gist.
///
/// Returns a list of (skill_name, file_content) tuples.
///
/// Discovery logic:
/// 1. If any file is named "SKILL.md", use only that file (single skill).
/// 2. Otherwise, scan all files for valid SKILL.md frontmatter (requires `name` + `description`).
pub fn discover_skills_from_gist(gist: &GistResponse) -> Vec<(String, String)> {
    // Level 1: Check for a file literally named "SKILL.md"
    if let Some(skill_md) = gist.files.get("SKILL.md") {
        if let Some(content) = &skill_md.content {
            if let Some((name, _desc)) = try_parse_skill_frontmatter(content) {
                return vec![(name, content.clone())];
            }
        }
    }

    // Level 2: Scan all files for valid skill frontmatter
    let mut skills = Vec::new();
    for file in gist.files.values() {
        if let Some(content) = &file.content {
            if let Some((name, _desc)) = try_parse_skill_frontmatter(content) {
                skills.push((name, content.clone()));
            }
        }
    }

    skills
}
```

Note: `try_parse_skill_frontmatter` already exists at line ~530 of `github.rs`. It parses frontmatter and returns `Option<(String, Option<String>)>`. However, the spec requires both `name` AND `description` for Level 2 discovery. We need to adjust the Level 2 check:

```rust
// In discover_skills_from_gist, for Level 2, filter to only files where description is Some:
if let Some((name, desc)) = try_parse_skill_frontmatter(content) {
    if desc.is_some() {
        skills.push((name, content.clone()));
    }
}
```

For Level 1 (SKILL.md file), we only require `name` (matching existing behavior).

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib registry::github::gist_tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```
feat: add discover_skills_from_gist for two-level skill discovery
```

---

### Task 5: Add `add_skill_from_gist` function

**Files:**
- Modify: `src/registry/skill.rs` (add the main gist installation function)

**Step 1: Write the implementation**

This is the core integration function. Add to `src/registry/skill.rs`:

```rust
use super::github::{discover_skills_from_gist, fetch_gist, is_gist_url, parse_gist_url};

/// Add skill(s) from a GitHub Gist URL
///
/// Fetches the gist, discovers skills, and installs each one under `owner/gists/skill-name`.
pub fn add_skill_from_gist(url: &str) -> Result<()> {
    let (owner, gist_id) = parse_gist_url(url)
        .with_context(|| format!("Invalid gist URL: {}", url))?;

    println!("{} Fetching gist from {}", "=>".green().bold(), url);

    let gist = fetch_gist(&gist_id)?;

    let skills = discover_skills_from_gist(&gist);
    if skills.is_empty() {
        anyhow::bail!(
            "No valid skills found in gist.\n\
             A gist skill needs a file named SKILL.md, or files with valid SKILL.md frontmatter \
             (requires 'name' and 'description' fields)."
        );
    }

    let mut db = db::init_db()?;
    let install_dir = get_skills_install_dir()?;
    let tap_name = format!("{}/gists", owner);

    // Create synthetic tap if needed
    if db::get_tap(&db, &tap_name).is_none() {
        let tap_info = super::models::TapInfo {
            url: format!("https://gist.github.com/{}", owner),
            skills_path: String::new(),
            updated_at: Some(Utc::now()),
            is_default: false,
            cached_registry: None,
        };
        db::add_tap(&mut db, &tap_name, tap_info);
    }

    let mut installed_count = 0;

    for (skill_name, content) in &skills {
        let full_name = format!("{}/{}", tap_name, skill_name);

        // Check if already installed
        if db::is_skill_installed(&db, &full_name) {
            println!(
                "{} Skill '{}' is already installed. Use '{}' to update.",
                "Info:".cyan(),
                full_name,
                format!("skillshub update {}", full_name).bold()
            );
            continue;
        }

        let dest = install_dir.join(&tap_name).join(skill_name);
        std::fs::create_dir_all(&dest)?;
        std::fs::write(dest.join("SKILL.md"), content)?;

        let installed = InstalledSkill {
            tap: tap_name.clone(),
            skill: skill_name.clone(),
            commit: None,
            installed_at: Utc::now(),
            source_url: Some(url.to_string()),
            source_path: Some(gist_id.clone()),
            gist_updated_at: Some(gist.updated_at.clone()),
        };

        db::add_installed_skill(&mut db, &full_name, installed);
        installed_count += 1;

        println!(
            "{} Added '{}' from gist to {}",
            "✓".green(),
            full_name,
            dest.display()
        );
    }

    db::save_db(&db)?;

    if installed_count > 0 {
        link_to_agents()?;
    }

    Ok(())
}
```

**Step 2: Run `cargo check`**

Run: `cargo check`
Expected: PASS (compiles)

**Step 3: Commit**

```
feat: add add_skill_from_gist for installing skills from gist URLs
```

---

### Task 6: Wire gist support into `add_skill_from_url` and exports

**Files:**
- Modify: `src/registry/skill.rs:150-155` (add_skill_from_url — add gist detection at top)
- Modify: `src/registry/mod.rs` (export the new function names from github.rs)

**Step 1: Modify `add_skill_from_url`**

At the top of `add_skill_from_url` in `src/registry/skill.rs:153`, before the existing `parse_github_url` call, add gist detection:

```rust
pub fn add_skill_from_url(url: &str) -> Result<()> {
    // Check if this is a gist URL — handle separately
    if is_gist_url(url) {
        return add_skill_from_gist(url);
    }

    // ... existing code for repo URLs ...
```

**Step 2: Update exports in `src/registry/mod.rs`**

The `github.rs` public functions (`is_gist_url`, `parse_gist_url`, `fetch_gist`, `discover_skills_from_gist`) don't need to be re-exported from `mod.rs` since they're only used within the `registry` module. No changes needed to `mod.rs` unless we want them public.

**Step 3: Build and test**

Run: `cargo build`
Expected: PASS

**Step 4: Manual test (if possible)**

Run: `cargo run -- add https://gist.github.com/garrytan/001f9074cab1a8f545ebecbc73a813df`
Expected: Installs `garrytan/gists/plan-exit-review` and links to agents

**Step 5: Commit**

```
feat: wire gist support into skillshub add command
```

---

### Task 7: Add gist update support

**Files:**
- Modify: `src/registry/skill.rs:324-471` (update_skill function)

**Step 1: Write the implementation**

In the `update_skill` function, after getting `installed` (line ~355), add a branch for gist skills. Gist skills are identified by having `gist_updated_at` set and `source_path` containing the gist ID.

Insert this block in `update_skill`, inside the `for skill_name in skills_to_update` loop, after getting the `installed` variable (~line 355), before the tap lookup:

```rust
// Handle gist-sourced skills separately
if installed.gist_updated_at.is_some() {
    if let Some(gist_id) = &installed.source_path {
        match fetch_gist(gist_id) {
            Ok(gist) => {
                if Some(&gist.updated_at) == installed.gist_updated_at.as_ref() {
                    println!("  {} {} (up to date)", "✓".green(), skill_name);
                    continue;
                }

                // Re-discover and update
                let skills_found = discover_skills_from_gist(&gist);
                let skill_content = skills_found
                    .iter()
                    .find(|(name, _)| *name == installed.skill);

                match skill_content {
                    Some((_, content)) => {
                        let dest = install_dir.join(&installed.tap).join(&installed.skill);
                        std::fs::create_dir_all(&dest)?;
                        std::fs::write(dest.join("SKILL.md"), content)?;

                        if let Some(skill) = db.installed.get_mut(&skill_name) {
                            skill.gist_updated_at = Some(gist.updated_at.clone());
                            skill.installed_at = Utc::now();
                        }

                        println!(
                            "  {} {} (gist updated: {} -> {})",
                            "✓".green(),
                            skill_name,
                            installed.gist_updated_at.as_deref().unwrap_or("unknown"),
                            gist.updated_at
                        );
                        updated_count += 1;
                    }
                    None => {
                        println!(
                            "  {} {} (skill no longer found in gist)",
                            "✗".red(),
                            skill_name
                        );
                    }
                }
            }
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
            }
        }
        continue;
    }
}
```

Also add the import at the top of `skill.rs`:
```rust
use super::github::{fetch_gist, discover_skills_from_gist};
```

**Step 2: Build**

Run: `cargo build`
Expected: PASS

**Step 3: Commit**

```
feat: add gist update support in skillshub update
```

---

### Task 8: Integration test with wiremock

**Files:**
- Modify: `src/registry/github.rs` (add integration test at bottom)

**Step 1: Write a wiremock-based integration test**

Add to the existing test infrastructure in `github.rs`. The project already uses `SKILLSHUB_GITHUB_API_BASE` for test overrides:

```rust
#[cfg(test)]
mod gist_integration_tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};

    #[tokio::test]
    #[serial_test::serial]
    async fn test_fetch_gist_via_api() {
        let mock_server = MockServer::start().await;

        let gist_body = serde_json::json!({
            "id": "abc123",
            "owner": { "login": "testuser" },
            "updated_at": "2025-06-01T12:00:00Z",
            "files": {
                "SKILL.md": {
                    "filename": "SKILL.md",
                    "content": "---\nname: test-skill\ndescription: A test\n---\n# Test"
                }
            }
        });

        Mock::given(method("GET"))
            .and(path("/gists/abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&gist_body))
            .mount(&mock_server)
            .await;

        std::env::set_var("SKILLSHUB_GITHUB_API_BASE", mock_server.uri());

        let gist = fetch_gist("abc123").unwrap();
        assert_eq!(gist.id, "abc123");
        assert_eq!(gist.owner.login, "testuser");
        assert_eq!(gist.files.len(), 1);

        let skills = discover_skills_from_gist(&gist);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].0, "test-skill");

        std::env::remove_var("SKILLSHUB_GITHUB_API_BASE");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_fetch_gist_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/gists/nonexistent"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        std::env::set_var("SKILLSHUB_GITHUB_API_BASE", mock_server.uri());

        let result = fetch_gist("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Gist not found"));

        std::env::remove_var("SKILLSHUB_GITHUB_API_BASE");
    }
}
```

**Step 2: Run tests**

Run: `cargo test --lib registry::github::gist_integration_tests -- --nocapture`
Expected: PASS

**Step 3: Commit**

```
test: add wiremock integration tests for gist API
```

---

### Task 9: Update documentation

**Files:**
- Modify: `CLAUDE.md` (add gist URL format to CLI Commands section)
- Modify: `README.md` (add gist usage example)

**Step 1: Update CLAUDE.md**

In the `### Adding Skills from URLs` section, add:

```markdown
### Adding Skills from URLs
```bash
skillshub add <github-url>                  # Add skill directly from GitHub URL
skillshub add <gist-url>                    # Add skill(s) from GitHub Gist
# Examples:
# skillshub add https://github.com/user/repo/tree/commit/path/to/skill
# skillshub add https://gist.github.com/user/gist_id
```

**Step 2: Update README.md**

Add a section about gist support with the example from the issue.

**Step 3: Commit**

```
docs: add gist support to CLI documentation
```

---

### Task 10: Final verification

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 3: Manual end-to-end test**

Run:
```bash
cargo run -- add https://gist.github.com/garrytan/001f9074cab1a8f545ebecbc73a813df
cargo run -- list
cargo run -- info garrytan/gists/plan-exit-review
cargo run -- update garrytan/gists/plan-exit-review
cargo run -- uninstall garrytan/gists/plan-exit-review
```

Expected: Full lifecycle works.
