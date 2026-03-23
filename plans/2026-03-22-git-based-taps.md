# Git-Based Tap Management — Implementation Plan (v2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the migration from GitHub API-based tap management to local git clone/pull operations. PR #52 partially implemented this; this plan covers the remaining work for a major version release (0.3.0+).

**Architecture:** Taps are shallow-cloned to `~/.skillshub/taps/<owner>/<repo>/` on `tap add`. Updates use `git pull` with delete-and-reclone fallback. Skill installation copies files from the local clone. Gist taps and star list imports retain API-based access. If a clone doesn't exist when needed (upgrade from older version), it's created automatically via `ensure_clone`.

**Tech Stack:** `std::process::Command` for git CLI, `walkdir` crate for filesystem traversal.

**Breaking changes (major version bump):**
- **`@commit` specifier produces a hard error for non-gist taps.** Shallow clones cannot checkout arbitrary commits. Message: "Pinned commits are not supported for git-based taps. Remove the @commit specifier." Gist taps retain `@commit` via the API.
- **Private repos require git credential helpers.** The old API path used `GITHUB_TOKEN`. Git clone uses system credential helpers or SSH keys instead.
- **`git` is now a hard requirement.** A `check_git()` pre-flight runs before any git operation.

**Dependencies:**
- **Add:** `walkdir` (recursive directory scanning)
- **Remove:** `flate2`, `tar` (tarball extraction — no longer used)
- **Keep:** `reqwest` (still needed for gist API and star-list GraphQL)

---

## Review Status

This plan incorporates decisions from:
- **CEO Review** (2026-03-23): Approach C (Full Removal + Resilience), 5 cherry-picks accepted, 7 issues resolved
- **Eng Review** (2026-03-23): 5 issues resolved (pull fallback design, copy_dir_contents move, shared test helpers, registry cache composability, discovery scope)
- **Codex Review** (2026-03-23): 2 new findings addressed (registry cache, discovery validation)
- **Spec Review** (2026-03-23): Clarifications on ensure_clone, pull fallback, @commit, db migration

---

## What's Already Done (PR #52)

Tasks 1–7 from the original plan were implemented in commit `b0326d8`:
- [x] Taps directory paths (`get_taps_clone_dir` in paths.rs)
- [x] Git module (`git_clone`, `git_pull`, `git_head_sha`, `tap_clone_path` in git.rs)
- [x] Local skill discovery (`discover_skills_from_local` in github.rs)
- [x] `tap add` uses git clone (with gist fallback to API)
- [x] `tap update` uses git pull (with auto-clone for legacy taps)
- [x] `tap remove` cleans up clone directory
- [x] `install_skill_internal` tries local clone first (API fallback retained)
- [x] `update_skill` tries local clone first (API fallback retained)

**What PR #52 did NOT do (this plan's scope):**
- `add_skill_from_url` still uses API tarball download
- API fallback paths still exist in `install_skill_internal` and `update_skill`
- Dead API functions still in github.rs (~400-500 LOC)
- `flate2`/`tar` still in dependencies
- No `check_git`, `ensure_clone`, or pull fallback
- No `walkdir` (manual `walk_dir_for_skills` instead)
- No `--branch` flag, `doctor` command, or progress streaming

---

## File Structure

### New Files
- `src/commands/doctor.rs` — `skillshub doctor` diagnostic command

### Modified Files
- `src/registry/git.rs` — Add `check_git`, `ensure_clone`, `pull_or_reclone`; change `git_clone`/`git_pull` to stream progress
- `src/paths.rs` — Add `get_tap_clone_dir(tap_name)`
- `src/registry/tap.rs` — Move `discover_skills_from_local` + `walk_dir_for_skills` here from github.rs; use `walkdir` crate; add broader directory filters + duplicate warning
- `src/registry/skill.rs` — Convert `add_skill_from_url` for git repos; remove API fallback from `install_skill_internal` and `update_skill`; add path containment check and copy cleanup to `install_from_clone`
- `src/registry/github.rs` — Remove dead functions: `discover_skills_from_repo`, `download_skill`, `get_default_branch`, `get_latest_commit`, `extract_skill_paths`, `TreeResponse`, `TreeEntry`, `RepoInfo`
- `src/registry/models.rs` — Add `branch: Option<String>` to `TapInfo`
- `src/util.rs` — Move `copy_dir_contents` here from github.rs
- `src/cli.rs` — Add `--branch` flag to `tap add`, add `doctor` subcommand
- `src/registry/mod.rs` — No changes needed (git module already declared)
- `Cargo.toml` — Add `walkdir`, remove `flate2`/`tar`
- `tests/common/mod.rs` — Add shared `init_test_repo` helpers
- `docs/architecture.md` — Update for new taps directory structure

---

## Task 8: Add Git Resilience to `git.rs`

**Files:** `src/registry/git.rs`, `src/paths.rs`

- [ ] **Step 1: Add `check_git()` function**

```rust
/// Pre-flight check that git is available.
pub fn check_git() -> Result<()> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .context("git is not installed or not in PATH")?;
    if !output.status.success() {
        anyhow::bail!("git is not working properly");
    }
    Ok(())
}
```

- [ ] **Step 2: Add `get_tap_clone_dir` to `paths.rs`**

```rust
/// Get the clone directory for a specific tap (~/.skillshub/taps/owner/repo)
pub fn get_tap_clone_dir(tap_name: &str) -> Result<PathBuf> {
    let taps_dir = get_taps_clone_dir()?;
    Ok(super::registry::git::tap_clone_path(&taps_dir, tap_name))
}
```

- [ ] **Step 3: Add `ensure_clone()` function**

Corruption check: `.git` directory exists AND `git rev-parse HEAD` succeeds AND remote URL matches expected URL. If any fail, delete directory and re-clone.

```rust
/// Ensure a tap clone exists and is healthy. Clone if missing or corrupted.
pub fn ensure_clone(clone_dir: &Path, url: &str, branch: Option<&str>) -> Result<PathBuf> {
    if clone_dir.join(".git").exists() {
        // Verify the clone is functional
        let rev_check = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(clone_dir)
            .output();

        let rev_ok = matches!(rev_check, Ok(output) if output.status.success());

        // Verify remote URL matches expected
        let remote_ok = if rev_ok {
            let remote = Command::new("git")
                .args(["remote", "get-url", "origin"])
                .current_dir(clone_dir)
                .output();
            matches!(remote, Ok(output) if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == url)
        } else {
            false
        };

        if rev_ok && remote_ok {
            return Ok(clone_dir.to_path_buf());
        }

        // Corrupted or wrong remote — remove and re-clone
        eprintln!("  Re-cloning tap (clone was corrupted or remote changed)...");
        std::fs::remove_dir_all(clone_dir)?;
    }

    // Clone from scratch
    if let Some(parent) = clone_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    git_clone(url, clone_dir, branch)?;
    Ok(clone_dir.to_path_buf())
}
```

- [ ] **Step 4: Add `pull_or_reclone()` wrapper**

If `git pull --ff-only` fails (force-push, diverged), delete clone and re-clone. The clone is disposable — it's just a cache.

```rust
/// Pull latest changes, falling back to delete + re-clone on failure.
pub fn pull_or_reclone(clone_dir: &Path, url: &str, branch: Option<&str>) -> Result<()> {
    match git_pull(clone_dir) {
        Ok(()) => Ok(()),
        Err(_) => {
            eprintln!("  Pull failed, re-cloning...");
            std::fs::remove_dir_all(clone_dir)?;
            if let Some(parent) = clone_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            git_clone(url, clone_dir, branch)?;
            Ok(())
        }
    }
}
```

- [ ] **Step 5: Change `git_clone` and `git_pull` to stream progress**

Replace `.output()` with `.spawn()` + `.wait()` so git's progress output streams to the terminal:

```rust
pub fn git_clone(url: &str, dest: &Path, branch: Option<&str>) -> Result<()> {
    check_git()?;
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1"]);
    if let Some(b) = branch {
        cmd.args(["-b", b]);
    }
    cmd.arg(url).arg(dest);

    let status = cmd.status().context("Failed to run git clone (is git installed?)")?;
    if !status.success() {
        anyhow::bail!("git clone failed");
    }
    Ok(())
}
```

Note: `.status()` inherits stdin/stdout/stderr, so git progress output is visible. The tradeoff is we lose the stderr capture for error messages — the user sees git's error directly.

- [ ] **Step 6: Write tests**

Tests use local tempdir repos (no network). Add to `src/registry/git.rs` `#[cfg(test)]` block:
- `test_check_git` — git available → Ok
- `test_ensure_clone_creates_missing` — clone dir doesn't exist → creates it
- `test_ensure_clone_repairs_corrupted` — .git missing → re-clones
- `test_ensure_clone_repairs_wrong_remote` — remote URL doesn't match → re-clones
- `test_ensure_clone_noop_healthy` — healthy clone → returns path
- `test_pull_or_reclone_happy_path` — fast-forward → Ok
- `test_pull_or_reclone_force_push` — ff-only fails → re-clones
- `test_git_clone_with_branch_local` — clone specific branch of local repo
- `test_git_pull_local` — pull new commit from local origin

Replace existing `#[ignore]` network tests with these local equivalents.

- [ ] **Step 7: Commit**

```
feat: add git resilience (check_git, ensure_clone, pull_or_reclone)
```

---

## Task 9: Improve Discovery and Move to `tap.rs`

**Files:** `src/registry/tap.rs`, `src/registry/github.rs`, `Cargo.toml`

- [ ] **Step 1: Add `walkdir` dependency**

```bash
cargo add walkdir
```

- [ ] **Step 2: Move `discover_skills_from_local` and `walk_dir_for_skills` from `github.rs` to `tap.rs`**

Rewrite using `walkdir` crate with broader directory filters:

```rust
use walkdir::WalkDir;

/// Discover skills by walking a local clone directory for SKILL.md files.
pub(crate) fn discover_skills_from_local(clone_dir: &Path, tap_name: &str) -> Result<TapRegistry> {
    let mut skills = HashMap::new();
    let skip_dirs = [".git", "node_modules", "target", "test", "tests",
                     "examples", "fixtures", "vendor", "benchmark"];

    for entry in WalkDir::new(clone_dir)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && (name.starts_with('.') || skip_dirs.contains(&name.as_ref())))
        })
        .filter_map(|e| e.ok())
    {
        if entry.file_name() == "SKILL.md" && entry.file_type().is_file() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                match parse_skill_md_content(&content) {
                    Some((name, description)) => {
                        let skill_path = entry.path().parent()
                            .and_then(|p| p.strip_prefix(clone_dir).ok())
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();

                        // Warn on duplicate skill names
                        if skills.contains_key(&name) {
                            eprintln!(
                                "  {} Duplicate skill name '{}' at {}, keeping first occurrence",
                                "!".yellow(), name, skill_path
                            );
                        } else {
                            skills.insert(name.clone(), SkillEntry {
                                path: skill_path,
                                description,
                                homepage: None,
                            });
                        }
                    }
                    None => {
                        // Warn about malformed SKILL.md
                        let rel_path = entry.path().strip_prefix(clone_dir)
                            .unwrap_or(entry.path());
                        eprintln!(
                            "  {} Skipping {}: invalid frontmatter (missing name field)",
                            "!".yellow(), rel_path.display()
                        );
                    }
                }
            }
        }
    }

    if skills.is_empty() {
        anyhow::bail!("No skills found in local clone (no valid SKILL.md files detected)");
    }

    Ok(TapRegistry {
        name: tap_name.to_string(),
        description: Some(format!("Skills from {}", tap_name)),
        skills,
    })
}
```

- [ ] **Step 3: Remove `discover_skills_from_local` and `walk_dir_for_skills` from `github.rs`**

Update imports in `tap.rs` accordingly — no longer importing these from `github.rs`.

- [ ] **Step 4: Update callers**

In `tap.rs`, `add_tap` and `update_single_tap` should call the local `discover_skills_from_local` instead of `super::github::discover_skills_from_local`.

- [ ] **Step 5: Write tests**

Add to `src/registry/tap.rs` test module:
- `test_discover_finds_skills_in_subdirs`
- `test_discover_finds_root_level_skill`
- `test_discover_skips_git_dir`
- `test_discover_skips_test_fixtures_dirs`
- `test_discover_warns_malformed_skill_md`
- `test_discover_warns_duplicate_names`
- `test_discover_empty_repo_bails`

- [ ] **Step 6: Commit**

```
refactor: move discovery to tap.rs with walkdir, broader filters, and warnings
```

---

## Task 10: Convert `add_skill_from_url` for Git Repos

**Files:** `src/registry/skill.rs`

- [ ] **Step 1: Replace API download with clone-based install**

In `add_skill_from_url`, after the gist check, replace the `download_skill` call with:

```rust
    // Ensure tap clone exists
    let base_url = github_url.base_url();
    let clone_dir = crate::paths::get_tap_clone_dir(&tap_name)?;
    super::git::ensure_clone(&clone_dir, &base_url, github_url.branch.as_deref())?;

    let dest = install_dir.join(&tap_name).join(&skill_name);
    std::fs::create_dir_all(&dest)?;

    // Copy from clone with path containment check
    let source = clone_dir.join(skill_path);
    let canonical_source = source.canonicalize()
        .with_context(|| format!("Skill path '{}' not found in repository", skill_path))?;
    let canonical_clone = clone_dir.canonicalize()?;
    if !canonical_source.starts_with(&canonical_clone) {
        anyhow::bail!("Skill path escapes clone directory");
    }
    if !canonical_source.join("SKILL.md").exists() {
        anyhow::bail!("No SKILL.md found at '{}'", skill_path);
    }
    copy_dir_contents(&source, &dest)?;

    let commit_sha = super::git::git_head_sha(&clone_dir)?;

    // Populate cached_registry so `update` works without manual `tap update`
    if db::get_tap(&db, &tap_name).is_none() {
        let registry = super::tap::discover_skills_from_local(&clone_dir, &tap_name)
            .ok(); // Non-fatal: registry cache is a convenience
        let tap_info = super::models::TapInfo {
            url: base_url,
            skills_path: "skills".to_string(),
            updated_at: Some(Utc::now()),
            is_default: false,
            cached_registry: registry,
            branch: github_url.branch.clone(),
        };
        db::add_tap(&mut db, &tap_name, tap_info);
    }
```

- [ ] **Step 2: Remove `download_skill` from imports**

Remove `download_skill`, `get_default_branch`, `get_latest_commit` from the import block.

- [ ] **Step 3: Build and verify**

```bash
cargo build
```

- [ ] **Step 4: Commit**

```
feat: add-from-url uses local clone instead of API download
```

---

## Task 11: Remove API Fallback from `install_skill_internal` and `update_skill`

**Files:** `src/registry/skill.rs`

- [ ] **Step 1: Update `install_skill_internal`**

Remove the API fallback branch. The install path becomes:
1. Default tap → `install_from_local` (bundled skills)
2. `@commit` on non-gist → **hard error**: "Pinned commits are not supported for git-based taps."
3. All other taps → `install_from_clone` (no fallback to `install_from_remote`)

- [ ] **Step 2: Add path containment + copy cleanup to `install_from_clone`**

```rust
fn install_from_clone(tap_name: &str, tap_url: &str, skill_path: &str, dest: &std::path::Path) -> Result<Option<String>> {
    let clone_dir = crate::paths::get_tap_clone_dir(tap_name)?;
    super::git::ensure_clone(&clone_dir, tap_url, None)?;

    let source = clone_dir.join(skill_path);

    // Path containment check
    let canonical_source = source.canonicalize()
        .with_context(|| format!("Skill path '{}' not found in local clone", skill_path))?;
    let canonical_clone = clone_dir.canonicalize()?;
    if !canonical_source.starts_with(&canonical_clone) {
        anyhow::bail!("Skill path escapes clone directory");
    }
    if !canonical_source.join("SKILL.md").exists() {
        anyhow::bail!("No SKILL.md found in '{}'", skill_path);
    }

    // Clean destination and copy with cleanup on failure
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::create_dir_all(dest)?;
    if let Err(e) = copy_dir_contents(&source, dest) {
        // Clean up partial copy before propagating error
        let _ = std::fs::remove_dir_all(dest);
        return Err(e.context("Failed to copy skill from clone"));
    }

    let commit = super::git::git_head_sha(&clone_dir).ok();
    Ok(commit)
}
```

- [ ] **Step 3: Update `update_skill` to remove API fallback**

Remove the entire block at the end of `update_skill` (lines 619-675) that falls back to `parse_github_url` → `get_default_branch` → `get_latest_commit` → `install_from_remote`. Replace with a bail: "No local clone for tap '{}'. Run 'skillshub tap update' to create one."

Change the update flow to use `pull_or_reclone` instead of raw `git_pull`.

- [ ] **Step 4: Delete `install_from_remote` function**

Remove the entire function (lines 375-392).

- [ ] **Step 5: Build and verify**

```bash
cargo build
```

- [ ] **Step 6: Commit**

```
feat: remove API fallback, require local clone for install/update
```

---

## Task 12: Clean Up `github.rs` and Move `copy_dir_contents`

**Files:** `src/registry/github.rs`, `src/util.rs`, `Cargo.toml`

- [ ] **Step 1: Move `copy_dir_contents` to `src/util.rs`**

Move the function and update all callers (`skill.rs`, gist install code) to import from `crate::util`.

- [ ] **Step 2: Remove dead functions from `github.rs`**

Remove:
1. `TreeResponse` struct
2. `TreeEntry` struct
3. `RepoInfo` struct
4. `get_default_branch()` function
5. `discover_skills_from_repo()` function
6. `get_latest_commit()` function
7. `download_skill()` function
8. `extract_skill_paths()` function

Keep:
- All rate-limit/retry infrastructure (used by gist and star list APIs)
- `parse_github_url()`, `is_valid_repo_id()`, `is_gist_url()`
- `parse_skill_md_content()` (pub(crate), used by tap.rs discovery and gist discovery)
- All gist functions
- All star list functions
- `build_client()`, `with_auth()`

- [ ] **Step 3: Remove unused imports**

Remove `flate2`, `tar`, `Cursor` imports from `github.rs`.

- [ ] **Step 4: Clean up `Cargo.toml` dependencies**

Remove `flate2` and `tar` from `[dependencies]`.

```bash
cargo build  # Compiler will catch any missed references
```

- [ ] **Step 5: Commit**

```
refactor: remove dead API functions, move copy_dir_contents to util.rs
```

---

## Task 13: Add `--branch` Flag and `TapInfo.branch`

**Files:** `src/registry/models.rs`, `src/cli.rs`, `src/registry/tap.rs`

- [ ] **Step 1: Add `branch` field to `TapInfo`**

In `src/registry/models.rs`:
```rust
pub struct TapInfo {
    pub url: String,
    pub skills_path: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub is_default: bool,
    pub cached_registry: Option<TapRegistry>,
    #[serde(default)]
    pub branch: Option<String>,  // NEW — persists which branch was cloned
}
```

No database migration needed — `#[serde(default)]` handles legacy db.json files.

- [ ] **Step 2: Add `--branch` flag to CLI**

In `src/cli.rs`, add to the `TapCommands::Add` variant:
```rust
TapCommands::Add {
    url: String,
    #[arg(short, long)]
    branch: Option<String>,
    // ... existing fields
}
```

- [ ] **Step 3: Thread branch through `add_tap`**

Update `add_tap` signature: `pub fn add_tap(url: &str, branch: Option<&str>, install: bool) -> Result<()>`

Store the branch in `TapInfo` when creating the tap. Use it in `git_clone`.

- [ ] **Step 4: Use stored branch in `update_single_tap`**

When pulling or re-cloning, use `tap.branch.as_deref()` so the correct branch is maintained.

- [ ] **Step 5: Display branch in `tap list`**

If `tap.branch.is_some()`, show it in the tap list output (e.g., in the URL column: `https://github.com/org/repo [dev]`).

- [ ] **Step 6: Write tests**

- `test_tap_info_deserialize_without_branch` — legacy db.json → branch is None
- `test_tap_info_serialize_roundtrip_with_branch`
- `test_add_tap_with_branch` — clones correct branch

- [ ] **Step 7: Commit**

```
feat: add --branch flag to tap add, persist branch in TapInfo
```

---

## Task 14: Add `skillshub doctor` Command

**Files:** `src/commands/doctor.rs`, `src/commands/mod.rs`, `src/cli.rs`, `src/main.rs`

- [ ] **Step 1: Add CLI subcommand**

In `cli.rs`:
```rust
Commands::Doctor,
```

Wire in `main.rs`:
```rust
Commands::Doctor => commands::doctor::run_doctor(),
```

- [ ] **Step 2: Implement `run_doctor()`**

```rust
pub fn run_doctor() -> Result<()> {
    println!("{} Running diagnostics...\n", "=>".green().bold());
    let mut issues = 0;

    // 1. Git health
    match super::super::registry::git::check_git() {
        Ok(()) => println!("  {} git is installed", "✓".green()),
        Err(e) => { println!("  {} git: {}", "✗".red(), e); issues += 1; }
    }

    // 2. Clone health — for each tap, verify clone dir
    let db = db::init_db()?;
    for (name, tap) in &db.taps {
        if tap.url.contains("gist.github.com") { continue; }
        let clone_dir = crate::paths::get_tap_clone_dir(name)?;
        if !clone_dir.exists() {
            println!("  {} tap '{}': clone directory missing", "✗".red(), name);
            issues += 1;
        } else if !clone_dir.join(".git").exists() {
            println!("  {} tap '{}': .git directory missing (corrupted clone)", "✗".red(), name);
            issues += 1;
        } else {
            // Quick rev-parse check
            match super::super::registry::git::git_head_sha(&clone_dir) {
                Ok(_) => println!("  {} tap '{}': clone healthy", "✓".green(), name),
                Err(_) => { println!("  {} tap '{}': git rev-parse failed", "✗".red(), name); issues += 1; }
            }
        }
    }

    // 3. Skill health — for each installed skill, check files exist
    let install_dir = crate::paths::get_skills_install_dir()?;
    for (full_name, _) in &db.installed {
        let parts: Vec<&str> = full_name.splitn(2, '/').collect();
        if parts.len() == 2 {
            let skill_dir = install_dir.join(parts[0]).join(parts[1]);
            if !skill_dir.join("SKILL.md").exists() {
                println!("  {} skill '{}': SKILL.md missing", "✗".red(), full_name);
                issues += 1;
            } else {
                println!("  {} skill '{}': files present", "✓".green(), full_name);
            }
        }
    }

    // 4. Symlink health — check agent links
    // (Implementation depends on agent detection — iterate known agent paths)

    // 5. Orphan detection — clone dirs with no matching tap
    let taps_dir = crate::paths::get_taps_clone_dir()?;
    if taps_dir.exists() {
        for owner_entry in std::fs::read_dir(&taps_dir)?.flatten() {
            if owner_entry.path().is_dir() {
                for repo_entry in std::fs::read_dir(owner_entry.path())?.flatten() {
                    let tap_name = format!("{}/{}",
                        owner_entry.file_name().to_string_lossy(),
                        repo_entry.file_name().to_string_lossy());
                    if !db.taps.contains_key(&tap_name) {
                        println!("  {} orphan clone: {} (no matching tap in db)", "!".yellow(), tap_name);
                        issues += 1;
                    }
                }
            }
        }
    }

    println!();
    if issues == 0 {
        println!("{} All checks passed!", "✓".green().bold());
    } else {
        println!("{} {} issue(s) found", "!".yellow().bold(), issues);
    }
    Ok(())
}
```

- [ ] **Step 3: Write tests**

- `test_doctor_no_taps` — empty db → "All checks passed"
- `test_doctor_healthy_clone` — valid clone → pass
- `test_doctor_missing_clone` — tap exists, clone missing → reports issue
- `test_doctor_missing_skill_files` — installed but SKILL.md gone → reports issue
- `test_doctor_orphan_clone` — clone dir exists but no tap in db → warns

- [ ] **Step 4: Commit**

```
feat: add skillshub doctor command for diagnostics
```

---

## Task 15: Update Tests

**Files:** `tests/common/mod.rs`, `tests/common/mock_github.rs`, various test modules

- [ ] **Step 1: Create shared test helpers**

In `tests/common/mod.rs` (or a new `src/test_helpers.rs` behind `#[cfg(test)]`):

```rust
use std::path::Path;
use std::process::Command;
use std::fs;

/// Initialize a bare git repo with a single commit
pub fn init_test_repo(dir: &Path) {
    Command::new("git").args(["init"]).current_dir(dir).output().unwrap();
    Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(dir).output().unwrap();
    Command::new("git").args(["config", "user.name", "Test"]).current_dir(dir).output().unwrap();
    fs::write(dir.join("README.md"), "# test").unwrap();
    Command::new("git").args(["add", "."]).current_dir(dir).output().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(dir).output().unwrap();
}

/// Initialize a git repo with a skill
pub fn init_test_repo_with_skill(dir: &Path, skill_name: &str, description: &str) {
    init_test_repo(dir);
    let skill_dir = dir.join("skills").join(skill_name);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {}\ndescription: {}\n---\n# {}", skill_name, description, skill_name),
    ).unwrap();
    Command::new("git").args(["add", "."]).current_dir(dir).output().unwrap();
    Command::new("git").args(["commit", "-m", "add skill"]).current_dir(dir).output().unwrap();
}
```

- [ ] **Step 2: Remove unused mock helpers**

In `tests/common/mock_github.rs`, remove:
- `mock_tree_response()` — no more tree API calls
- `mock_tarball()` — no more tarball downloads
- `mock_commits()` — no more commits API calls

Keep mock helpers used by gist and star-list tests.

- [ ] **Step 3: Audit and update remaining tests**

Run: `grep -r "discover_skills_from_repo\|download_skill\|get_latest_commit\|get_default_branch\|install_from_remote" tests/`

Remove or update any tests that reference removed functions.

- [ ] **Step 4: Run full test suite**

```bash
cargo test
```

- [ ] **Step 5: Commit**

```
test: update tests for git-based tap management
```

---

## Task 16: Update Documentation

**Files:** `README.md`, `docs/architecture.md`, `CLAUDE.md`

- [ ] **Step 1: Update `docs/architecture.md`**

Add taps directory structure:
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
Add section on `skillshub doctor`.

- [ ] **Step 2: Update `README.md`**

- `tap add` and `tap update` use git — no API rate limits
- Only gist/star-list operations use GitHub API
- `GITHUB_TOKEN` only needed for gist/star-list
- Private repos: configure git credential helpers or SSH keys
- `@commit` no longer supported for non-gist taps
- New `--branch` flag for `tap add`
- New `skillshub doctor` command

- [ ] **Step 3: Update `CLAUDE.md`**

Add `doctor` to the "Testing locally" section. Update any references to API-based operations.

- [ ] **Step 4: Move this plan to `docs/`**

Per CLAUDE.md: "After the plan is fully implemented, rewrite it as a design doc in `docs/`, and remove it from `plans/`."

- [ ] **Step 5: Commit**

```
docs: update for git-based tap management
```

---

## Summary of Changes

| Operation | Before (Hybrid) | After (Git-only) |
|---|---|---|
| `tap add` | git clone (gist: API) | git clone with --branch (gist: API) |
| `tap update` | git pull (gist: API) | pull_or_reclone with branch (gist: API) |
| `tap remove` | Remove db + clone | Same |
| `install` | Clone → API fallback | Clone only (ensure_clone) |
| `update` (skill) | Clone → API fallback | Clone only (pull_or_reclone) |
| `add` (URL) | API tarball download | Ensure clone → copy |
| `@commit` (non-gist) | API tarball at commit | **Hard error** |
| Gist operations | GitHub API | Same |
| Star list | GraphQL API | Same |
| **New: `doctor`** | — | Git + clone + skill + symlink health |
| **New: `--branch`** | — | Clone specific branch, persist in TapInfo |

**Added dependency:** `walkdir`
**Removed dependencies:** `flate2`, `tar`
**Kept:** `reqwest` (gist API, star-list GraphQL)

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | 6 proposals, 5 accepted, 1 deferred |
| Codex Review | `/codex review` | Independent 2nd opinion | 1 | CLEAR | 10 findings, 2 new (registry cache, discovery scope) |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 5 issues, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |

**CODEX:** Found composability gap (add_skill_from_url + update) and discovery validation weakness (test fixtures, duplicates). Both fixed.
**UNRESOLVED:** 0
**VERDICT:** CEO + ENG + CODEX CLEARED — ready to implement.
