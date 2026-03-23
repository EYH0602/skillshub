# Git-Based Tap Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace GitHub API-based tap management with local git clone/pull operations to eliminate rate limiting and improve performance (Issue #51).

**Architecture:** Taps are shallow-cloned to `~/.skillshub/taps/<owner>/<repo>/` on `tap add`. Updates use `git pull`. Skill installation copies files from the local clone instead of downloading tarballs via API. Gist taps and star list imports retain API-based access. If a clone doesn't exist when needed (upgrade from older version), it's created automatically.

**Tech Stack:** `std::process::Command` for git CLI, `walkdir` crate for filesystem traversal.

**Behavioral changes:**
- **`@commit` specifier is dropped for non-gist taps.** With shallow clones (`--depth 1`), checking out arbitrary historical commits is not supported. The `@commit` syntax will be silently ignored. Update CLI help text accordingly.
- **Branch/commit in URL for `add` is ignored.** `skillshub add https://github.com/owner/repo/tree/abc1234/skills/my-skill` will clone the default branch and copy from HEAD, not from the specified commit. The old behavior downloaded a specific commit.
- **Private repos require git credential helpers.** The old API path used `GITHUB_TOKEN` for auth. Git clone uses the system's configured credential helpers or SSH keys instead.

---

## File Structure

### New Files
- `src/registry/git.rs` — Git CLI wrapper: clone, pull, HEAD SHA, ensure-clone

### Modified Files
- `src/paths.rs:19` — Add `get_taps_dir()` and `get_tap_clone_dir()`
- `src/registry/mod.rs:1` — Add `pub mod git`
- `src/registry/tap.rs:30-324` — Refactor `add_tap`, `update_single_tap`, `remove_tap`; add `discover_skills_from_clone()`
- `src/registry/skill.rs:51-604` — Refactor `install_skill_internal`, `install_from_remote`, `update_skill`, `add_skill_from_url`
- `src/registry/github.rs:460-714` — Remove `discover_skills_from_repo`, `download_skill`, `get_latest_commit`, `get_default_branch`, `TreeResponse`, `TreeEntry`, `RepoInfo`; make `parse_skill_md_content` and `copy_dir_contents` `pub(crate)`

---

## Task 1: Add Taps Directory Paths

**Files:**
- Modify: `src/paths.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// Add to the existing #[cfg(test)] mod tests block in src/paths.rs

#[test]
#[serial]
fn test_get_taps_dir() {
    let dir = get_taps_dir().unwrap();
    assert!(dir.ends_with("taps"));
    assert!(dir.parent().unwrap().ends_with(".skillshub"));
}

#[test]
#[serial]
fn test_get_tap_clone_dir() {
    let dir = get_tap_clone_dir("owner/repo").unwrap();
    assert!(dir.ends_with("repo"));
    assert!(dir.parent().unwrap().ends_with("owner"));
    assert!(dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .ends_with("taps"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib paths::tests::test_get_taps_dir paths::tests::test_get_tap_clone_dir`
Expected: FAIL — functions don't exist

- [ ] **Step 3: Implement the path functions**

Add after `get_skills_install_dir()` (line 21) in `src/paths.rs`:

```rust
/// Get the taps directory (~/.skillshub/taps)
pub fn get_taps_dir() -> Result<PathBuf> {
    Ok(get_skillshub_home()?.join("taps"))
}

/// Get the clone directory for a specific tap (~/.skillshub/taps/owner/repo)
pub fn get_tap_clone_dir(tap_name: &str) -> Result<PathBuf> {
    Ok(get_taps_dir()?.join(tap_name))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib paths::tests`
Expected: all PASS

- [ ] **Step 5: Commit**

```
feat: add taps directory path helpers
```

---

## Task 2: Create Git Module

**Files:**
- Create: `src/registry/git.rs`
- Modify: `src/registry/mod.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/registry/git.rs` with tests only:

```rust
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::paths::get_tap_clone_dir;

/// Check if git is available on the system
fn check_git() -> Result<()> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .context("git is not installed or not in PATH")?;
    if !output.status.success() {
        anyhow::bail!("git is not working properly");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    fn init_test_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
        fs::write(dir.join("README.md"), "# test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn test_clone_repo() {
        let origin = tempdir().unwrap();
        init_test_repo(origin.path());

        let dest = tempdir().unwrap();
        let clone_path = dest.path().join("clone");

        clone_repo(origin.path().to_str().unwrap(), &clone_path).unwrap();

        assert!(clone_path.join("README.md").exists());
        assert!(clone_path.join(".git").exists());
    }

    #[test]
    fn test_pull_repo() {
        let origin = tempdir().unwrap();
        init_test_repo(origin.path());

        let dest = tempdir().unwrap();
        let clone_path = dest.path().join("clone");
        clone_repo(origin.path().to_str().unwrap(), &clone_path).unwrap();

        // Add a new file in origin
        fs::write(origin.path().join("new.txt"), "new content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(origin.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "add new file"])
            .current_dir(origin.path())
            .output()
            .unwrap();

        // Pull in clone
        pull_repo(&clone_path).unwrap();

        assert!(clone_path.join("new.txt").exists());
    }

    #[test]
    fn test_get_head_sha() {
        let origin = tempdir().unwrap();
        init_test_repo(origin.path());

        let sha = get_head_sha(origin.path()).unwrap();
        assert_eq!(sha.len(), 7);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_ensure_clone_creates_if_missing() {
        let origin = tempdir().unwrap();
        init_test_repo(origin.path());

        let dest = tempdir().unwrap();
        let clone_path = dest.path().join("taps/owner/repo");

        let result = ensure_clone(&clone_path, origin.path().to_str().unwrap()).unwrap();
        assert_eq!(result, clone_path);
        assert!(clone_path.join(".git").exists());
    }

    #[test]
    fn test_ensure_clone_noop_if_exists() {
        let origin = tempdir().unwrap();
        init_test_repo(origin.path());

        let dest = tempdir().unwrap();
        let clone_path = dest.path().join("clone");
        clone_repo(origin.path().to_str().unwrap(), &clone_path).unwrap();

        // Should succeed without re-cloning
        let result = ensure_clone(&clone_path, origin.path().to_str().unwrap()).unwrap();
        assert_eq!(result, clone_path);
    }
}
```

- [ ] **Step 2: Add module declaration**

Add `pub mod git;` to `src/registry/mod.rs` (after `pub mod github;`).

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib registry::git::tests`
Expected: FAIL — functions don't exist

- [ ] **Step 4: Implement the git functions**

Add the implementations above the `#[cfg(test)]` block in `src/registry/git.rs`:

```rust
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Check that git is available
fn check_git() -> Result<()> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .context("git is not installed or not in PATH")?;
    if !output.status.success() {
        anyhow::bail!("git is not working properly");
    }
    Ok(())
}

/// Shallow-clone a repository to dest
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    check_git()?;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--single-branch",
            url,
            &dest.to_string_lossy(),
        ])
        .output()
        .context("Failed to run git clone")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {}", stderr.trim());
    }

    Ok(())
}

/// Pull latest changes in a cloned repository.
/// Falls back to delete + re-clone if fast-forward fails (e.g., force-push).
pub fn pull_repo(repo_path: &Path) -> Result<()> {
    check_git()?;

    let output = Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git pull")?;

    if !output.status.success() {
        // Fast-forward failed (force-push, diverged history, etc.)
        // Fall back to fetch + reset for shallow clones
        let fetch = Command::new("git")
            .args(["fetch", "--depth", "1", "origin"])
            .current_dir(repo_path)
            .output()
            .context("Failed to run git fetch")?;

        if !fetch.status.success() {
            let stderr = String::from_utf8_lossy(&fetch.stderr);
            anyhow::bail!("git fetch failed: {}", stderr.trim());
        }

        let reset = Command::new("git")
            .args(["reset", "--hard", "origin/HEAD"])
            .current_dir(repo_path)
            .output()
            .context("Failed to run git reset")?;

        if !reset.status.success() {
            let stderr = String::from_utf8_lossy(&reset.stderr);
            anyhow::bail!("git reset failed: {}", stderr.trim());
        }
    }

    Ok(())
}

/// Get the short (7-char) HEAD commit SHA
pub fn get_head_sha(repo_path: &Path) -> Result<String> {
    check_git()?;

    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        anyhow::bail!("git rev-parse failed");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Ensure a tap clone exists. Clone if missing or corrupted, return the path.
pub fn ensure_clone(clone_dir: &Path, url: &str) -> Result<PathBuf> {
    if clone_dir.join(".git").exists() {
        // Quick sanity check: verify the clone is functional
        let check = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(clone_dir)
            .output();

        match check {
            Ok(output) if output.status.success() => return Ok(clone_dir.to_path_buf()),
            _ => {
                // Corrupted clone — remove and re-clone
                std::fs::remove_dir_all(clone_dir)?;
            }
        }
    }

    clone_repo(url, clone_dir)?;
    Ok(clone_dir.to_path_buf())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib registry::git::tests`
Expected: all PASS

- [ ] **Step 6: Commit**

```
feat: add git module for clone/pull/sha operations
```

---

## Task 3: Add Local Skill Discovery

**Files:**
- Modify: `src/registry/tap.rs`
- Modify: `src/registry/github.rs` (make `parse_skill_md_content` pub(crate))

- [ ] **Step 0: Add `walkdir` dependency**

Run: `cargo add walkdir`

- [ ] **Step 1: Make `parse_skill_md_content` accessible**

In `src/registry/github.rs:559`, change:
```rust
fn parse_skill_md_content(content: &str) -> Option<(String, Option<String>)> {
```
to:
```rust
pub(crate) fn parse_skill_md_content(content: &str) -> Option<(String, Option<String>)> {
```

- [ ] **Step 2: Write the failing test**

Add to `src/registry/tap.rs` test module:

```rust
#[cfg(test)]
mod tests {
    // ... existing tests ...

    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_discover_skills_from_clone_finds_skills() {
        let dir = tempdir().unwrap();
        let skills_dir = dir.path().join("skills").join("my-skill");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\n# My Skill",
        )
        .unwrap();

        let registry = discover_skills_from_clone(dir.path(), "test-tap").unwrap();
        assert_eq!(registry.skills.len(), 1);
        assert!(registry.skills.contains_key("my-skill"));
        assert_eq!(
            registry.skills["my-skill"].description.as_deref(),
            Some("A test skill")
        );
    }

    #[test]
    fn test_discover_skills_from_clone_skips_git_dir() {
        let dir = tempdir().unwrap();
        // Skill in .git should be ignored
        let git_skill = dir.path().join(".git").join("skill");
        std::fs::create_dir_all(&git_skill).unwrap();
        std::fs::write(
            git_skill.join("SKILL.md"),
            "---\nname: hidden\ndescription: hidden\n---\n",
        )
        .unwrap();

        // Real skill
        let real_skill = dir.path().join("skills").join("real");
        std::fs::create_dir_all(&real_skill).unwrap();
        std::fs::write(
            real_skill.join("SKILL.md"),
            "---\nname: real\ndescription: A real skill\n---\n",
        )
        .unwrap();

        let registry = discover_skills_from_clone(dir.path(), "test-tap").unwrap();
        assert_eq!(registry.skills.len(), 1);
        assert!(registry.skills.contains_key("real"));
    }

    #[test]
    fn test_discover_skills_from_clone_root_level_skill() {
        let dir = tempdir().unwrap();
        // SKILL.md at the root of the repo (no subdirectory)
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: root-skill\ndescription: A root skill\n---\n# Root",
        )
        .unwrap();

        let registry = discover_skills_from_clone(dir.path(), "test-tap").unwrap();
        assert_eq!(registry.skills.len(), 1);
        assert!(registry.skills.contains_key("root-skill"));
    }

    #[test]
    fn test_discover_skills_from_clone_empty_repo() {
        let dir = tempdir().unwrap();
        let registry = discover_skills_from_clone(dir.path(), "test-tap").unwrap();
        assert!(registry.skills.is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib registry::tap::tests::test_discover_skills_from_clone`
Expected: FAIL — function doesn't exist

- [ ] **Step 4: Implement `discover_skills_from_clone`**

Add to the top of `src/registry/tap.rs`:
```rust
use std::path::Path;
use super::github::parse_skill_md_content;
```

Then add the function before `get_tap_registry`:

```rust
/// Discover skills by walking a local clone directory for SKILL.md files.
///
/// Scans the entire directory tree (excluding .git, node_modules, target)
/// and builds a TapRegistry from the SKILL.md files found.
pub(crate) fn discover_skills_from_clone(clone_dir: &Path, tap_name: &str) -> Result<TapRegistry> {
    use std::collections::HashMap;
    use super::models::SkillEntry;
    use walkdir::WalkDir;

    let mut skills = HashMap::new();

    for entry in WalkDir::new(clone_dir)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            // Skip hidden dirs, node_modules, target
            !(e.file_type().is_dir()
                && (name.starts_with('.') || name == "node_modules" || name == "target"))
        })
        .filter_map(|e| e.ok())
    {
        if entry.file_name() == "SKILL.md" && entry.file_type().is_file() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Some((name, description)) = parse_skill_md_content(&content) {
                    // Path relative to clone root, parent of SKILL.md
                    let skill_path = entry
                        .path()
                        .parent()
                        .and_then(|p| p.strip_prefix(clone_dir).ok())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    skills.insert(
                        name.clone(),
                        SkillEntry {
                            path: skill_path,
                            description,
                            homepage: None,
                        },
                    );
                }
            }
        }
    }

    Ok(TapRegistry {
        name: tap_name.to_string(),
        description: None,
        skills,
    })
}
```

**Note:** This uses the `walkdir` crate. Add it to `Cargo.toml`:

```
cargo add walkdir
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib registry::tap::tests::test_discover_skills_from_clone`
Expected: all PASS

- [ ] **Step 6: Commit**

```
feat: add local skill discovery from cloned tap directories
```

---

## Task 4: Refactor `tap add` and `tap update` to Use Git

**Note:** These are combined into one task to avoid a build break (both reference `discover_skills_from_repo` which we're removing).

**Files:**
- Modify: `src/registry/tap.rs:30-90` (add_tap)
- Modify: `src/registry/tap.rs:269-324` (update_single_tap)

- [ ] **Step 1: Write integration test**

Add to tests in `src/registry/tap.rs`:

```rust
#[test]
fn test_add_tap_clones_repo() {
    // This test verifies the new flow creates a clone directory.
    // The actual `add_tap` function is tested via integration tests
    // since it writes to ~/.skillshub. Here we test the building blocks.
    let origin = tempdir().unwrap();

    // Set up a fake repo with a skill
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(origin.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(origin.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(origin.path())
        .output()
        .unwrap();

    let skill_dir = origin.path().join("skills").join("test-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: test-skill\ndescription: A test\n---\n# Test",
    )
    .unwrap();

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(origin.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(origin.path())
        .output()
        .unwrap();

    // Clone and discover
    let dest = tempdir().unwrap();
    let clone_path = dest.path().join("clone");
    super::super::git::clone_repo(origin.path().to_str().unwrap(), &clone_path).unwrap();

    let registry = discover_skills_from_clone(&clone_path, "test/repo").unwrap();
    assert_eq!(registry.skills.len(), 1);
    assert!(registry.skills.contains_key("test-skill"));
}
```

- [ ] **Step 2: Refactor `add_tap`**

Replace the body of `add_tap` in `src/registry/tap.rs:30-90`:

```rust
pub fn add_tap(url: &str, install: bool) -> Result<()> {
    let github_url = parse_github_url(url)?;
    let tap_name = github_url.tap_name();

    let mut db = db::init_db()?;

    if db.taps.contains_key(&tap_name) {
        anyhow::bail!(
            "Tap '{}' already exists. Use 'skillshub tap remove {}' first.",
            tap_name,
            tap_name
        );
    }

    let base_url = github_url.base_url();
    println!("{} Adding tap '{}' from {}", "=>".green().bold(), tap_name, base_url);

    // Clone the repository locally
    println!("  {} Cloning repository...", "○".yellow());
    let clone_dir = crate::paths::get_tap_clone_dir(&tap_name)?;
    super::git::clone_repo(&base_url, &clone_dir)
        .with_context(|| format!("Failed to clone {}", base_url))?;

    // Discover skills from the local clone
    println!("  {} Discovering skills...", "○".yellow());
    let registry = discover_skills_from_clone(&clone_dir, &tap_name)
        .with_context(|| format!("Failed to discover skills from clone"))?;

    let tap_info = TapInfo {
        url: base_url.clone(),
        skills_path: "skills".to_string(),
        updated_at: Some(Utc::now()),
        is_default: false,
        cached_registry: Some(registry.clone()),
    };

    db::add_tap(&mut db, &tap_name, tap_info);
    db::save_db(&db)?;

    println!(
        "  {} Added tap '{}' with {} skills",
        "✓".green(),
        tap_name,
        registry.skills.len()
    );

    if !install && !registry.skills.is_empty() {
        println!("\n  Available skills:");
        for (name, entry) in registry.skills.iter().take(10) {
            let desc = entry.description.as_deref().unwrap_or("No description");
            println!("    {} {}/{} - {}", "•".cyan(), tap_name, name, desc);
        }
        if registry.skills.len() > 10 {
            println!("    {} ... and {} more", "•".cyan(), registry.skills.len() - 10);
        }
    }

    if install && !registry.skills.is_empty() {
        println!();
        super::skill::install_all_from_tap(&tap_name)?;
    }

    Ok(())
}
```

- [ ] **Step 3: Refactor `update_single_tap`**

Replace the function body at `src/registry/tap.rs:269`:

```rust
fn update_single_tap(db: &mut Database, name: &str, tap: &TapInfo) -> Result<TapUpdateResult> {
    let tap_name_for_path = name.to_string();
    let clone_dir = crate::paths::get_tap_clone_dir(&tap_name_for_path)?;

    // Ensure clone exists (handles upgrade from API-based version)
    super::git::ensure_clone(&clone_dir, &tap.url)?;

    // Pull latest changes
    super::git::pull_repo(&clone_dir)?;

    // Re-discover skills from updated clone
    let new_registry = discover_skills_from_clone(&clone_dir, name)?;

    // Compare old vs new registries to detect changes
    let old_skills: std::collections::HashSet<&String> = tap
        .cached_registry
        .as_ref()
        .map(|r| r.skills.keys().collect())
        .unwrap_or_default();
    let new_skills_set: std::collections::HashSet<&String> = new_registry.skills.keys().collect();

    let has_baseline = tap.cached_registry.is_some();

    let mut added: Vec<String> = if has_baseline {
        new_skills_set.difference(&old_skills).map(|s| (*s).clone()).collect()
    } else {
        Vec::new()
    };
    let mut removed: Vec<String> = if has_baseline {
        old_skills.difference(&new_skills_set).map(|s| (*s).clone()).collect()
    } else {
        Vec::new()
    };

    added.sort();
    removed.sort();

    let mut removed_installed: Vec<String> = removed
        .iter()
        .filter(|skill_name| {
            let full_name = format!("{}/{}", name, skill_name);
            db.installed.contains_key(&full_name)
        })
        .cloned()
        .collect();
    removed_installed.sort();

    let total = new_registry.skills.len();

    if let Some(t) = db.taps.get_mut(name) {
        t.cached_registry = Some(new_registry);
        t.updated_at = Some(Utc::now());
    }

    Ok(TapUpdateResult {
        total,
        new_skills: added,
        removed_skills: removed,
        removed_installed,
    })
}
```

- [ ] **Step 4: Remove `discover_skills_from_repo` from imports**

Update the import line at the top of `src/registry/tap.rs`:

```rust
// Before:
use super::github::{discover_skills_from_repo, fetch_star_list_repos, parse_github_url, parse_star_list_url};

// After:
use super::github::{fetch_star_list_repos, parse_github_url, parse_star_list_url};
```

- [ ] **Step 5: Build and verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 6: Run tests**

Run: `cargo test --lib registry::tap`
Expected: PASS

- [ ] **Step 7: Commit**

```
feat: tap add/update uses git clone/pull instead of GitHub API
```

---

## Task 5: Refactor `tap remove` to Clean Up Clone

**Files:**
- Modify: `src/registry/tap.rs:93-141`

- [ ] **Step 1: Add clone directory cleanup to `remove_tap`**

In `remove_tap`, after `db::remove_tap(&mut db, name);` (line 135) and before `db::save_db`, add:

```rust
    // Remove the local clone directory
    let clone_dir = crate::paths::get_tap_clone_dir(name)?;
    if clone_dir.exists() {
        std::fs::remove_dir_all(&clone_dir)?;
        // Clean up empty parent directory (owner dir)
        if let Some(parent) = clone_dir.parent() {
            if parent.exists() && parent.read_dir()?.next().is_none() {
                std::fs::remove_dir(parent)?;
            }
        }
    }
```

- [ ] **Step 2: Build and run tests**

Run: `cargo build && cargo test --lib registry::tap`
Expected: PASS

- [ ] **Step 3: Commit**

```
feat: tap remove cleans up local clone directory
```

---

## Task 6: Refactor Skill Install to Copy from Clone

**Files:**
- Modify: `src/registry/skill.rs:51-152` (`install_skill_internal`)
- Modify: `src/registry/skill.rs:362-378` (`install_from_remote`)

- [ ] **Step 1: Delete `install_from_remote` and add `install_from_clone`**

Delete the `install_from_remote` function at `src/registry/skill.rs:362-378` and replace it with:

```rust
/// Install a skill by copying from the local tap clone
fn install_from_clone(
    tap_name: &str,
    tap_url: &str,
    skill_path: &str,
    dest: &std::path::Path,
) -> Result<Option<String>> {
    let clone_dir = crate::paths::get_tap_clone_dir(tap_name)?;

    // Ensure clone exists (handles first install or upgrade)
    super::git::ensure_clone(&clone_dir, tap_url)?;

    let source = clone_dir.join(skill_path);
    if !source.exists() {
        anyhow::bail!("Skill path '{}' not found in tap clone", skill_path);
    }
    if !source.join("SKILL.md").exists() {
        anyhow::bail!("No SKILL.md found at '{}'", skill_path);
    }

    // Clean and copy
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::create_dir_all(dest)?;
    copy_dir_contents(&source, dest)?;

    // Get commit SHA from the clone
    let sha = super::git::get_head_sha(&clone_dir)?;
    Ok(Some(sha))
}
```

- [ ] **Step 2: Update `install_skill_internal` to use `install_from_clone`**

In `install_skill_internal`, replace the non-default-tap branch (line 125-128):

```rust
    // Before:
    } else {
        let (commit, _) = install_from_remote(&tap.url, &skill_entry.path, &dest, requested_commit.as_deref())?;
        commit
    };

    // After:
    } else {
        install_from_clone(&skill_id.tap, &tap.url, &skill_entry.path, &dest)?
    };
```

And for the default tap fallback (line 121):

```rust
    // Before:
    let (commit, _) = install_from_remote(&tap.url, &skill_entry.path, &dest, requested_commit.as_deref())?;
    commit

    // After:
    install_from_clone(&skill_id.tap, &tap.url, &skill_entry.path, &dest)?
```

- [ ] **Step 3: Update imports in `src/registry/skill.rs`**

Remove unused imports from the top of the file:

```rust
// Remove these from the import:
// download_skill, get_default_branch, get_latest_commit

// Keep these:
use super::github::{
    copy_dir_contents, discover_skills_from_gist, fetch_gist,
    is_gist_url, parse_gist_url, parse_github_url,
};
```

- [ ] **Step 4: Build and verify**

Run: `cargo build`
Expected: PASS (there may be warnings about unused `download_skill` etc. in github.rs — that's fine, cleaned up in Task 9)

- [ ] **Step 5: Test manually**

Run: `cargo run -- install EYH0602/skillshub/code-reviewer`
Expected: installs from clone, shows commit SHA

- [ ] **Step 6: Commit**

```
feat: skill install copies from local tap clone instead of API download
```

---

## Task 7: Refactor Skill Update to Use Local Operations

**Files:**
- Modify: `src/registry/skill.rs:414-604` (`update_skill`)

- [ ] **Step 1: Replace the API-based update logic**

In `update_skill`, replace the block from line 489 (after gist handling, starting at `let tap = match...`) through line 597 with:

```rust
        let tap = match db::get_tap(&db, &installed.tap) {
            Some(t) => t.clone(),
            None => {
                println!("  {} {} (tap not found)", "✗".red(), skill_name);
                continue;
            }
        };

        // Skip gist-based taps (handled above)
        if tap.url.contains("gist.github.com") {
            continue;
        }

        let registry = match get_tap_registry(&db, &installed.tap) {
            Ok(Some(r)) => r,
            Ok(None) => {
                println!(
                    "  {} {} (no cached registry, run 'skillshub tap update')",
                    "✗".red(),
                    skill_name
                );
                continue;
            }
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
                continue;
            }
        };

        let skill_entry = match registry.skills.get(&installed.skill) {
            Some(e) => e,
            None => {
                println!("  {} {} (not in registry)", "✗".red(), skill_name);
                continue;
            }
        };

        let clone_dir = match crate::paths::get_tap_clone_dir(&installed.tap) {
            Ok(d) => d,
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
                continue;
            }
        };

        // Ensure clone exists and pull latest
        if let Err(e) = super::git::ensure_clone(&clone_dir, &tap.url) {
            println!("  {} {} ({})", "✗".red(), skill_name, e);
            continue;
        }
        if let Err(e) = super::git::pull_repo(&clone_dir) {
            println!("  {} {} ({})", "✗".red(), skill_name, e);
            continue;
        }

        // Compare HEAD SHA with installed commit
        let latest_sha = match super::git::get_head_sha(&clone_dir) {
            Ok(s) => s,
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
                continue;
            }
        };

        let install_dir = get_skills_install_dir()?;
        let dest = install_dir.join(&installed.tap).join(&installed.skill);
        let is_default_tap = tap.is_default || installed.tap == DEFAULT_TAP_NAME;

        // For default tap skills installed locally (commit=None), refresh from local bundled dir.
        if is_default_tap && installed.commit.is_none() {
            match install_from_local(&installed.skill, &dest) {
                Ok(()) => {
                    println!("  {} {} (bundled, refreshed)", "✓".green(), skill_name);
                    updated_count += 1;
                }
                Err(e) => {
                    println!("  {} {} ({})", "✗".red(), skill_name, e);
                }
            }
            continue;
        }

        // Check if update needed
        if installed.commit.as_deref() == Some(&latest_sha) {
            println!("  {} {} (up to date)", "✓".green(), skill_name);
            continue;
        }

        // Copy updated skill from clone
        let source = clone_dir.join(&skill_entry.path);
        if !source.exists() || !source.join("SKILL.md").exists() {
            println!("  {} {} (skill path not found in clone)", "✗".red(), skill_name);
            continue;
        }

        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        std::fs::create_dir_all(&dest)?;
        match copy_dir_contents(&source, &dest) {
            Ok(()) => {
                if let Some(skill) = db.installed.get_mut(&skill_name) {
                    skill.commit = Some(latest_sha.clone());
                    skill.installed_at = Utc::now();
                }
                println!(
                    "  {} {} ({} -> {})",
                    "✓".green(),
                    skill_name,
                    installed.commit.as_deref().unwrap_or("unknown"),
                    latest_sha
                );
                updated_count += 1;
            }
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
            }
        }
```

- [ ] **Step 2: Remove unused imports**

Remove `get_default_branch` and `get_latest_commit` from the import block if not already done.

- [ ] **Step 3: Build and verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Test manually**

Run: `cargo run -- update`
Expected: checks updates via local git operations

- [ ] **Step 5: Commit**

```
feat: skill update uses local git operations instead of API
```

---

## Task 8: Refactor `add_skill_from_url` to Use Clone

**Files:**
- Modify: `src/registry/skill.rs:154-253` (`add_skill_from_url`)

- [ ] **Step 1: Replace API download with clone-based install**

Replace the body of `add_skill_from_url` (keeping the gist redirect at the top):

```rust
pub fn add_skill_from_url(url: &str) -> Result<()> {
    if is_gist_url(url) {
        return add_skill_from_gist(url);
    }

    let github_url = parse_github_url(url)?;

    let skill_path = github_url
        .path
        .as_ref()
        .with_context(|| "URL must include path to skill folder (e.g., /tree/main/skills/my-skill)")?;

    let skill_name = github_url
        .skill_name()
        .with_context(|| "Could not determine skill name from URL path")?;

    let tap_name = github_url.tap_name().to_string();
    let full_name = format!("{}/{}", tap_name, skill_name);

    let mut db = db::init_db()?;
    let install_dir = get_skills_install_dir()?;

    if db::is_skill_installed(&db, &full_name) {
        let installed = db::get_installed_skill(&db, &full_name).unwrap();
        println!(
            "{} Skill '{}' is already installed (commit: {})",
            "Info:".cyan(),
            full_name,
            installed.commit.as_deref().unwrap_or("unknown")
        );
        println!(
            "Use '{}' to update it.",
            format!("skillshub update {}", full_name).bold()
        );
        return Ok(());
    }

    println!("{} Adding '{}' from {}", "=>".green().bold(), full_name, url);

    // Ensure tap clone exists
    let base_url = github_url.base_url();
    let clone_dir = crate::paths::get_tap_clone_dir(&tap_name)?;
    super::git::ensure_clone(&clone_dir, &base_url)?;

    let dest = install_dir.join(&tap_name).join(&skill_name);
    std::fs::create_dir_all(&dest)?;

    // Copy from clone
    let source = clone_dir.join(skill_path);
    if !source.exists() || !source.join("SKILL.md").exists() {
        anyhow::bail!("Skill path '{}' not found in repository", skill_path);
    }
    copy_dir_contents(&source, &dest)?;

    let commit_sha = super::git::get_head_sha(&clone_dir)?;

    // Add tap if it doesn't exist
    if db::get_tap(&db, &tap_name).is_none() {
        let tap_info = super::models::TapInfo {
            url: base_url,
            skills_path: "skills".to_string(),
            updated_at: Some(Utc::now()),
            is_default: false,
            cached_registry: None,
        };
        db::add_tap(&mut db, &tap_name, tap_info);
    }

    let installed = InstalledSkill {
        tap: tap_name.clone(),
        skill: skill_name.clone(),
        commit: Some(commit_sha.clone()),
        installed_at: Utc::now(),
        source_url: Some(url.to_string()),
        source_path: Some(skill_path.clone()),
        gist_updated_at: None,
    };

    db::add_installed_skill(&mut db, &full_name, installed);
    db::save_db(&db)?;

    println!(
        "{} Added '{}' (commit: {}) to {}",
        "✓".green(),
        full_name,
        commit_sha,
        dest.display()
    );

    link_to_agents()?;

    Ok(())
}
```

- [ ] **Step 2: Remove `download_skill` from imports**

The import line should now be:
```rust
use super::github::{
    copy_dir_contents, discover_skills_from_gist, fetch_gist,
    is_gist_url, parse_gist_url, parse_github_url,
};
```

- [ ] **Step 3: Build and verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```
feat: add-from-url uses local clone instead of API download
```

---

## Task 9: Clean Up `github.rs`

**Files:**
- Modify: `src/registry/github.rs`

- [ ] **Step 1: Remove now-unused functions and types**

Remove the following from `src/registry/github.rs`:

1. `TreeResponse` struct (~line 285-288)
2. `TreeEntry` struct (~line 291-296)
3. `RepoInfo` struct (~line 299-302)
4. `get_default_branch()` function (~line 330-356)
5. `discover_skills_from_repo()` function (~line 465-556)
6. `get_latest_commit()` function (~line 573-595)
7. `download_skill()` function (~line 600-679)
8. `extract_skill_paths()` function (~line 686-697)

Keep:
- All rate-limit/retry infrastructure (used by gist and star list APIs)
- `parse_github_url()`, `is_valid_repo_id()`
- `parse_skill_md_content()` (now `pub(crate)`, used by local discovery)
- `copy_dir_contents()` (used by install)
- All gist functions
- All star list functions
- `build_client()`, `with_auth()`

- [ ] **Step 2: Remove unused imports**

Remove `flate2`, `tar`, `Cursor` imports if they're only used by the removed functions. Check:

```rust
// Remove if unused:
use flate2::read::GzDecoder;
use tar::Archive;
use std::io::Cursor;
```

- [ ] **Step 3: Clean up `Cargo.toml` dependencies**

- Move `tempfile` from `[dependencies]` to `[dev-dependencies]` (only used in tests now)
- Remove `flate2` and `tar` from `[dependencies]` if no longer used elsewhere

Run: `cargo build` — the compiler will tell you about unused deps.

- [ ] **Step 4: Build and verify**

Run: `cargo build`
Expected: PASS (no warnings about unused functions)

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: PASS (tests referencing removed functions need updating — see Task 11)

- [ ] **Step 6: Commit**

```
refactor: remove unused GitHub API functions (tree, tarball, commits)
```

---

## Task 10: Update Tests

**Files:**
- Modify: `tests/common/mock_github.rs`
- Modify: any integration tests referencing removed functions

- [ ] **Step 1: Audit test files for references to removed functions**

Run: `grep -r "discover_skills_from_repo\|download_skill\|get_latest_commit\|get_default_branch\|TreeResponse\|TreeEntry" tests/`

Remove or update any tests that directly test these removed functions. Tests that mock the tree/tarball API endpoints may need to be replaced with tests that set up local git repos instead.

- [ ] **Step 2: Remove unused mock helpers**

In `tests/common/mock_github.rs`, remove mock helpers that are no longer needed:
- `mock_tree_response()` — no more tree API calls
- `mock_tarball()` — no more tarball downloads
- `mock_commits()` — no more commits API calls

Keep mock helpers used by gist tests.

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: all PASS

- [ ] **Step 4: Commit**

```
test: update tests for git-based tap management
```

---

## Task 11: Update Documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture.md`

- [ ] **Step 1: Update README.md**

Update the rate-limiting section to reflect that API calls are no longer the primary path:
- `tap add` and `tap update` now use git, no API rate limits
- Only gist imports and star list imports still use the GitHub API
- `GITHUB_TOKEN` is only needed for gist/star-list operations
- For private repos, configure git credential helpers or SSH keys (git clone uses system auth, not `GITHUB_TOKEN`)
- The `@commit` specifier for `install` is no longer supported for non-gist taps
- `skillshub add <url>` with branch/commit in URL now clones HEAD (not the specific commit)

- [ ] **Step 2: Update `docs/architecture.md`**

Add section about the taps directory structure:
```
~/.skillshub/
├── db.json
├── taps/                    # Cloned tap repositories
│   └── owner/
│       └── repo/            # Shallow git clone
│           ├── .git/
│           └── skills/
│               └── skill-name/
│                   └── SKILL.md
└── skills/                  # Installed skills (copied from taps/)
    └── owner/
        └── repo/
            └── skill-name/
                └── SKILL.md
```

Document the new flow: clone-based add, pull-based update, copy-based install.

- [ ] **Step 3: Move this plan to `docs/plans/`**

Per CLAUDE.md: "After the plan is fully implemented, rewrite it as a design doc in `docs/`, and remove it from `plans/`."

- [ ] **Step 4: Commit**

```
docs: update for git-based tap management
```

---

## Summary of Changes

| Operation | Before (API) | After (Git) |
|---|---|---|
| `tap add` | Tree API → discover SKILL.md files | `git clone --depth 1` → walk filesystem |
| `tap update` | Tree API → re-discover skills | `git pull` → re-walk filesystem |
| `tap remove` | Remove db entry only | Remove db entry + delete clone |
| `install` | Download tarball → extract | Copy from local clone |
| `update` (skill) | Commits API → compare SHA → download tarball | `git pull` → compare HEAD → copy |
| `add` (URL) | Download tarball | Ensure clone → copy |
| Gist operations | GitHub API (unchanged) | GitHub API (unchanged) |
| Star list | GraphQL API (unchanged) | GraphQL API (unchanged) |

**New dependency:** `walkdir` crate (for recursive directory scanning)

**Removed dependencies:** `flate2`, `tar` (moved to dev-deps or removed); `tempfile` moved to `[dev-dependencies]`
