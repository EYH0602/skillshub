use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use colored::Colorize;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use super::db::{self, DEFAULT_TAP_NAME};
use super::git::{ensure_clone, git_head_sha, tap_clone_path};
use super::github::{discover_skills_from_gist, fetch_gist, is_gist_url, parse_gist_url, parse_github_url};
use super::models::{Database, InstalledSkill, SkillId};
use super::tap::get_tap_registry;
use crate::commands::link_to_agents;
use crate::paths::{get_embedded_skills_dir, get_skills_install_dir, get_tap_clone_dir, get_taps_clone_dir};
use crate::skill::{discover_skills, has_references_dir, has_scripts_dir, parse_skill_metadata};
use crate::util::{copy_dir_contents, truncate_string};

const DESCRIPTION_MAX_LEN: usize = 50;

/// Table row for displaying skills
#[derive(Tabled)]
pub struct SkillListRow {
    #[tabled(rename = " ")]
    pub status: &'static str,
    #[tabled(rename = "Skill")]
    pub name: String,
    #[tabled(rename = "Tap")]
    pub tap: String,
    #[tabled(rename = "Description")]
    pub description: String,
    #[tabled(rename = "Extras")]
    pub extras: String,
    #[tabled(rename = "Commit")]
    pub commit: String,
}

/// Build a compact extras string from has_scripts/has_references flags.
/// Shows "scripts, refs" for both, "scripts" or "refs" for one, or "-" for neither.
fn format_extras(has_scripts: bool, has_references: bool) -> String {
    let mut parts = Vec::new();
    if has_scripts {
        parts.push("scripts");
    }
    if has_references {
        parts.push("refs");
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(", ")
    }
}

/// Install a skill by full name (tap/skill[@commit])
pub fn install_skill(full_name: &str) -> Result<()> {
    let installed = install_skill_internal(full_name)?;

    if installed {
        // Auto-link to all agents
        link_to_agents()?;
    }

    Ok(())
}

/// Internal skill installation without auto-linking (for batch operations)
fn install_skill_internal(full_name: &str) -> Result<bool> {
    let skill_id = SkillId::parse(full_name)
        .with_context(|| format!("Invalid skill name '{}'. Use format: tap/skill", full_name))?;

    let requested_commit = SkillId::parse_commit(full_name);

    let mut db = db::init_db()?;
    let install_dir = get_skills_install_dir()?;

    // Check if already installed
    if db::is_skill_installed(&db, &skill_id.full_name()) {
        let installed = db::get_installed_skill(&db, &skill_id.full_name()).unwrap();
        println!(
            "{} Skill '{}' is already installed (commit: {})",
            "Info:".cyan(),
            skill_id.full_name(),
            installed.commit.as_deref().unwrap_or("local")
        );
        return Ok(false);
    }

    // Get tap info
    let tap = db::get_tap(&db, &skill_id.tap)
        .with_context(|| {
            format!(
                "Tap '{}' not found. Add it with 'skillshub tap add <url>'",
                skill_id.tap
            )
        })?
        .clone();

    // Get registry to verify skill exists
    let registry = get_tap_registry(&db, &skill_id.tap)?.with_context(|| {
        format!(
            "No cached registry for tap '{}'. Run 'skillshub tap update {}' first.",
            skill_id.tap, skill_id.tap
        )
    })?;
    let skill_entry = registry.skills.get(&skill_id.skill).with_context(|| {
        format!(
            "Skill '{}' not found in tap '{}'. Run 'skillshub search {}' to find it.",
            skill_id.skill, skill_id.tap, skill_id.skill
        )
    })?;

    println!("{} Installing '{}'", "=>".green().bold(), skill_id.full_name());

    let dest = install_dir.join(&skill_id.tap).join(&skill_id.skill);
    std::fs::create_dir_all(&dest)?;

    // For the default (bundled) tap, install from local bundled skills directory.
    let commit = if tap.is_default || skill_id.tap == DEFAULT_TAP_NAME {
        if requested_commit.is_some() {
            println!(
                "  {} @commit specifier is ignored for bundled default tap skills (using local copy)",
                "!".yellow()
            );
        }
        install_from_local(&skill_id.skill, &dest)?;
        println!("  {} Installed from bundled skills (no network required)", "✓".green());
        None // local install has no remote commit SHA
    } else if requested_commit.is_some() && !is_gist_url(&tap.url) {
        // Pinned @commit is not supported for git-based taps
        anyhow::bail!("Pinned commits are not supported for git-based taps.");
    } else {
        // Install from local tap clone (no API fallback)
        let commit = install_from_clone(&skill_id.tap, &tap.url, &skill_entry.path, &dest, tap.branch.as_deref())?;
        println!("  {} Installed from local tap clone", "✓".green());
        commit
    };

    // Record in database
    let installed = InstalledSkill {
        tap: skill_id.tap.clone(),
        skill: skill_id.skill.clone(),
        commit,
        installed_at: Utc::now(),
        source_url: Some(tap.url.clone()),
        source_path: Some(skill_entry.path.clone()),
        gist_updated_at: None,
    };

    db::add_installed_skill(&mut db, &skill_id.full_name(), installed);
    db::save_db(&db)?;

    println!(
        "{} Installed '{}' to {}",
        "✓".green(),
        skill_id.full_name(),
        dest.display()
    );

    Ok(true)
}

/// Add a skill directly from a GitHub URL
///
/// URL format: https://github.com/owner/repo/tree/commit/path/to/skill
pub fn add_skill_from_url(url: &str) -> Result<()> {
    // Check if this is a gist URL — handle separately
    if is_gist_url(url) {
        return add_skill_from_gist(url);
    }

    let github_url = parse_github_url(url)?;

    // Must have a path to the skill folder
    let skill_path = github_url
        .path
        .as_ref()
        .with_context(|| "URL must include path to skill folder (e.g., /tree/main/skills/my-skill)")?;

    // Get skill name from path
    let skill_name = github_url
        .skill_name()
        .with_context(|| "Could not determine skill name from URL path")?;

    // Use repo name as tap name
    let tap_name = github_url.tap_name().to_string();
    let full_name = format!("{}/{}", tap_name, skill_name);

    let mut db = db::init_db()?;
    let install_dir = get_skills_install_dir()?;

    // Check if already installed
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

    // Reject pinned commit SHAs for non-gist taps — git clone -b cannot checkout a SHA
    if github_url.is_commit_sha() {
        anyhow::bail!(
            "Pinned commits (@SHA) are not supported for git-based taps. \
             Use --branch with a branch or tag name instead."
        );
    }

    println!("{} Adding '{}' from {}", "=>".green().bold(), full_name, url);

    // Ensure tap clone exists
    let base_url = github_url.base_url();
    let clone_dir = get_tap_clone_dir(&tap_name)?;
    ensure_clone(&clone_dir, &base_url, github_url.branch.as_deref())?;

    let dest = install_dir.join(&tap_name).join(&skill_name);
    std::fs::create_dir_all(&dest)?;

    // Copy from clone with path containment check
    let source = clone_dir.join(skill_path);
    let canonical_source = source
        .canonicalize()
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
        let registry = super::tap::discover_skills_from_local(&clone_dir, &tap_name).ok(); // Non-fatal: registry cache is a convenience
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

    // Record installed skill in database
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

    // Auto-link to all agents
    link_to_agents()?;

    Ok(())
}

/// Add skill(s) from a GitHub Gist URL
///
/// Fetches the gist, discovers skills, and installs each one under `owner/gists/skill-name`.
pub fn add_skill_from_gist(url: &str) -> Result<()> {
    let (owner, gist_id) = parse_gist_url(url).with_context(|| format!("Invalid gist URL: {}", url))?;

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
            branch: None,
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

        println!("{} Added '{}' from gist to {}", "✓".green(), full_name, dest.display());
    }

    db::save_db(&db)?;

    if installed_count > 0 {
        link_to_agents()?;
    }

    Ok(())
}

/// Install from local bundled skills directory (for the default tap).
/// Copies the skill directory from the bundled skills path to the destination.
fn install_from_local(skill_name: &str, dest: &std::path::Path) -> Result<()> {
    let skills_dir = get_embedded_skills_dir()?;
    let source = skills_dir.join(skill_name);

    if !source.exists() {
        anyhow::bail!(
            "skill '{}' not found in bundled skills at {}",
            skill_name,
            source.display()
        );
    }

    // Remove destination if it exists (clean reinstall)
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::create_dir_all(dest)?;

    // Recursively copy the skill directory
    copy_dir_contents(&source, dest)?;

    Ok(())
}

/// Install a skill by copying from a local tap clone.
/// Ensures the clone exists (cloning if necessary), validates path containment,
/// and copies with cleanup on failure.
/// Returns the HEAD commit SHA of the clone.
fn install_from_clone(
    tap_name: &str,
    tap_url: &str,
    skill_path: &str,
    dest: &std::path::Path,
    branch: Option<&str>,
) -> Result<Option<String>> {
    let clone_dir = crate::paths::get_tap_clone_dir(tap_name)?;
    super::git::ensure_clone(&clone_dir, tap_url, branch)?;

    let source = clone_dir.join(skill_path);

    // Path containment check
    let canonical_source = source
        .canonicalize()
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

/// Uninstall a skill by full name
pub fn uninstall_skill(full_name: &str) -> Result<()> {
    let skill_id = SkillId::parse(full_name)
        .with_context(|| format!("Invalid skill name '{}'. Use format: tap/skill", full_name))?;

    let mut db = db::init_db()?;
    let install_dir = get_skills_install_dir()?;

    // Check if installed
    if !db::is_skill_installed(&db, &skill_id.full_name()) {
        anyhow::bail!("Skill '{}' is not installed", skill_id.full_name());
    }

    let results = remove_installed_skills_batch(&mut db, &install_dir, std::slice::from_ref(&skill_id.full_name()));
    let (_, result) = results
        .into_iter()
        .next()
        .expect("batch of one always yields one result");
    result?;

    db::save_db(&db)?;

    println!("{} Uninstalled '{}'", "✓".green(), skill_id.full_name());

    Ok(())
}

/// Remove multiple installed skills, collecting per-skill results.
///
/// Never aborts mid-batch: a failure for one skill is recorded in the returned
/// vector and the remaining skills are still processed. Only successfully
/// removed skills lose their `db.installed` entry. Does NOT init or save the
/// DB — the caller owns persistence.
pub(crate) fn remove_installed_skills_batch(
    db: &mut Database,
    install_dir: &Path,
    names: &[String],
) -> Vec<(String, Result<()>)> {
    let mut results = Vec::with_capacity(names.len());
    for name in names {
        let result = match SkillId::parse(name) {
            Some(skill_id) => remove_installed_skill_files(db, install_dir, &skill_id),
            None => Err(anyhow!("Invalid skill name '{}'. Use format: tap/skill", name)),
        };
        results.push((name.clone(), result));
    }
    results
}

/// Remove a skill's installed files and its `db.installed` entry, mutating `db` in place.
///
/// Does NOT init or save the DB — the caller owns persistence. This lets callers that
/// already hold a `Database` (e.g. the update flows) prune skills without a nested
/// init/save cycle, which would otherwise let the caller's own `save_db` resurrect the
/// just-removed entry from its stale in-memory copy.
pub(crate) fn remove_installed_skill_files(db: &mut Database, install_dir: &Path, skill_id: &SkillId) -> Result<()> {
    let skill_path = install_dir.join(&skill_id.tap).join(&skill_id.skill);

    // Remove agent symlinks too, so uninstall doesn't leave dangling links behind.
    remove_agent_symlinks(skill_id, install_dir);

    if skill_path.exists() {
        std::fs::remove_dir_all(&skill_path)?;
    }

    // Clean up empty tap directory
    let tap_dir = install_dir.join(&skill_id.tap);
    if tap_dir.exists() && tap_dir.read_dir()?.next().is_none() {
        std::fs::remove_dir(&tap_dir)?;
    }

    db::remove_installed_skill(db, &skill_id.full_name());

    Ok(())
}

/// Remove a skill's symlinks from all discovered agents' skills directories.
fn remove_agent_symlinks(skill_id: &SkillId, install_dir: &Path) {
    let agent_skill_dirs: Vec<PathBuf> = crate::agent::discover_agents()
        .iter()
        .map(|a| a.path.join(a.skills_subdir))
        .collect();
    remove_agent_symlinks_in(&agent_skill_dirs, &skill_id.skill, install_dir);
}

/// Remove skillshub-managed symlinks named `link_name` from the given directories.
///
/// Only entries that ARE symlinks AND whose (absolutized) target starts with
/// `install_dir` are removed — real directories and symlinks pointing elsewhere
/// (user-owned / external) are left untouched. Best-effort: individual removal
/// errors are ignored so a batch uninstall is never aborted.
fn remove_agent_symlinks_in(agent_skill_dirs: &[PathBuf], link_name: &str, install_dir: &Path) {
    for dir in agent_skill_dirs {
        let link_path = dir.join(link_name);

        // symlink_metadata does not follow links, so it also sees dangling ones.
        let Ok(meta) = std::fs::symlink_metadata(&link_path) else {
            continue;
        };
        if !meta.file_type().is_symlink() {
            continue;
        }

        let Ok(target) = std::fs::read_link(&link_path) else {
            continue;
        };
        // Do NOT canonicalize the target: after the skill files are deleted the
        // target dangles and canonicalize fails. Absolutize relative targets
        // against the link's directory and compare raw paths instead.
        let abs_target = if target.is_absolute() { target } else { dir.join(target) };
        if abs_target.starts_with(install_dir) {
            let _ = std::fs::remove_file(&link_path);
        }
    }
}

/// Handle a skill that no longer exists upstream: prune it (when `prune`) or
/// print a notice. Shared by the update flow's cache-miss (default/gist taps)
/// and fresh-clone-miss (clone-backed taps) paths so both behave identically.
fn prune_or_report_missing(
    db: &mut Database,
    install_dir: &Path,
    installed: &InstalledSkill,
    skill_name: &str,
    prune: bool,
) {
    if prune {
        let skill_id = SkillId {
            tap: installed.tap.clone(),
            skill: installed.skill.clone(),
        };
        match remove_installed_skill_files(db, install_dir, &skill_id) {
            Ok(()) => println!("  {} {} (pruned, no longer in tap)", "-".red(), skill_name),
            Err(e) => println!("  {} {} (prune failed: {})", "✗".red(), skill_name, e),
        }
    } else {
        println!("  {} {} (no longer in tap)", "✗".red(), skill_name);
    }
}

/// Which skills an update run should target.
///
/// An explicit enum rather than empty-slice-means-all: in the TUI an empty
/// selection means "update nothing", and an empty slice silently meaning
/// "update everything" is a latent bug one refactor away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateSelection {
    /// Update every installed skill.
    All,
    /// Update only the named skills (each in `tap/skill` format).
    Selected(Vec<String>),
}

/// Resolve an `UpdateSelection` to the concrete list of installed skill names.
///
/// `All` yields every installed skill; `Selected` validates each name exactly
/// as the CLI always has: unparseable names get an "Invalid skill name"
/// context error, well-formed but uninstalled names bail.
pub(crate) fn resolve_update_names(db: &Database, selection: &UpdateSelection) -> Result<Vec<String>> {
    match selection {
        UpdateSelection::All => Ok(db.installed.keys().cloned().collect()),
        UpdateSelection::Selected(names) => {
            let mut resolved = Vec::with_capacity(names.len());
            for name in names {
                let skill_id = SkillId::parse(name)
                    .with_context(|| format!("Invalid skill name '{}'. Use format: tap/skill", name))?;

                if !db::is_skill_installed(db, &skill_id.full_name()) {
                    anyhow::bail!("Skill '{}' is not installed", skill_id.full_name());
                }

                resolved.push(skill_id.full_name());
            }
            Ok(resolved)
        }
    }
}

/// Update a skill (or all skills) to latest version
pub fn update_skill(selection: UpdateSelection, prune: bool) -> Result<()> {
    let mut db = db::init_db()?;
    let install_dir = get_skills_install_dir()?;

    let skills_to_update = resolve_update_names(&db, &selection)?;

    if skills_to_update.is_empty() {
        println!("No skills installed to update.");
        return Ok(());
    }

    println!(
        "{} Checking {} skill(s) for updates...",
        "=>".green().bold(),
        skills_to_update.len()
    );

    let mut updated_count = 0;
    // Taps already pulled this run: several skills can share one tap clone,
    // and pulling it once per skill would repeat the same fetch.
    let mut pulled_taps: std::collections::HashSet<String> = std::collections::HashSet::new();

    for skill_name in skills_to_update {
        let installed = db.installed.get(&skill_name).unwrap().clone();

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
                        let skill_content = skills_found.iter().find(|(name, _)| *name == installed.skill);

                        match skill_content {
                            Some((_, content)) => {
                                let install_dir = get_skills_install_dir()?;
                                let dest = install_dir.join(&installed.tap).join(&installed.skill);
                                std::fs::create_dir_all(&dest)?;
                                std::fs::write(dest.join("SKILL.md"), content)?;

                                if let Some(skill) = db.installed.get_mut(&skill_name) {
                                    skill.gist_updated_at = Some(gist.updated_at.clone());
                                    skill.installed_at = Utc::now();
                                }

                                println!("  {} {} (gist updated)", "✓".green(), skill_name,);
                                updated_count += 1;
                            }
                            None => {
                                if prune {
                                    let skill_id = SkillId {
                                        tap: installed.tap.clone(),
                                        skill: installed.skill.clone(),
                                    };
                                    match remove_installed_skill_files(&mut db, &install_dir, &skill_id) {
                                        Ok(()) => {
                                            println!("  {} {} (pruned, no longer in gist)", "-".red(), skill_name)
                                        }
                                        Err(e) => println!("  {} {} (prune failed: {})", "✗".red(), skill_name, e),
                                    }
                                } else {
                                    println!("  {} {} (skill no longer found in gist)", "✗".red(), skill_name);
                                }
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

        let tap = match db::get_tap(&db, &installed.tap) {
            Some(t) => t.clone(),
            None => {
                println!("  {} {} (tap not found)", "✗".red(), skill_name);
                continue;
            }
        };

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

        let is_default_tap = tap.is_default || installed.tap == DEFAULT_TAP_NAME;
        let is_gist_tap = is_gist_url(&tap.url);
        let dest = install_dir.join(&installed.tap).join(&installed.skill);

        // The default (bundled) tap and gist taps carry an authoritative cached
        // registry here, so a cache miss is a real "removed upstream" signal and
        // we decide prune/skip immediately. Clone-backed taps (non-default,
        // non-gist) may have a stale cache, so their membership is decided
        // against the freshly pulled clone below — never from the cache.
        if (is_default_tap || is_gist_tap) && !registry.skills.contains_key(&installed.skill) {
            prune_or_report_missing(&mut db, &install_dir, &installed, &skill_name, prune);
            continue;
        }

        // For default tap skills installed locally (commit=None), refresh from local bundled dir.
        // These are never compared by commit SHA, so always attempt a local-first refresh.
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

        // Gist taps without gist_updated_at shouldn't reach here, but guard anyway
        if is_gist_tap {
            println!("  {} {} (unexpected state for gist skill)", "✗".red(), skill_name);
            continue;
        }

        let taps_dir = get_taps_clone_dir()?;
        let clone_dir = tap_clone_path(&taps_dir, &installed.tap);

        if !clone_dir.exists() {
            println!(
                "  {} {} (No local clone for tap '{}'. Run 'skillshub tap update' to create one.)",
                "✗".red(),
                skill_name,
                installed.tap
            );
            continue;
        }

        // Pull latest using resilient pull_or_reclone (once per tap per run).
        // Mark the tap pulled only on success, so a failed pull is retried
        // for the next skill of the same tap.
        if !pulled_taps.contains(&installed.tap) {
            if let Err(e) = super::git::pull_or_reclone(&clone_dir, &tap.url, tap.branch.as_deref()) {
                println!("  {} {} (pull failed: {})", "✗".red(), skill_name, e);
                continue;
            }
            pulled_taps.insert(installed.tap.clone());
        }

        // Re-discover skills from the freshly pulled clone — the authoritative
        // upstream state. Deciding membership here (rather than from the possibly
        // stale cache) is what lets `update --prune` honor its promise to remove
        // skills actually removed upstream, and avoids a cryptic copy error when
        // the cached path no longer exists in the clone.
        let fresh_registry = match super::tap::discover_skills_from_local(&clone_dir, &installed.tap) {
            Ok(r) => r,
            Err(e) => {
                println!("  {} {} (failed to scan clone: {})", "✗".red(), skill_name, e);
                continue;
            }
        };
        let skill_path = match fresh_registry.skills.get(&installed.skill) {
            Some(e) => e.path.clone(),
            None => {
                prune_or_report_missing(&mut db, &install_dir, &installed, &skill_name, prune);
                continue;
            }
        };
        // Refresh the tap's cached registry so `list` and later runs reflect the
        // freshly observed upstream state.
        if let Some(t) = db.taps.get_mut(&installed.tap) {
            t.cached_registry = Some(fresh_registry);
        }

        let new_commit = git_head_sha(&clone_dir).unwrap_or_default();

        if installed.commit.as_deref() == Some(&new_commit) {
            println!("  {} {} (up to date)", "✓".green(), skill_name);
            continue;
        }

        // Copy updated files from clone
        match install_from_clone(&installed.tap, &tap.url, &skill_path, &dest, tap.branch.as_deref()) {
            Ok(commit) => {
                let old_commit = installed.commit.as_deref().unwrap_or("unknown");
                if let Some(skill) = db.installed.get_mut(&skill_name) {
                    skill.commit = commit;
                    skill.installed_at = Utc::now();
                }
                println!("  {} {} ({} -> {})", "✓".green(), skill_name, old_commit, new_commit);
                updated_count += 1;
            }
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
            }
        }
    }

    db::save_db(&db)?;

    println!("\n{} {} skill(s) updated", "Done!".green().bold(), updated_count);

    Ok(())
}

/// List all available and installed skills
pub fn list_skills() -> Result<()> {
    let db = db::init_db()?;

    let mut rows: Vec<SkillListRow> = Vec::new();
    let mut seen_skills: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Collect skills from all taps (available skills)
    let mut uncached_taps: Vec<String> = Vec::new();
    for tap_name in db.taps.keys() {
        let registry = match get_tap_registry(&db, tap_name) {
            Ok(Some(r)) => r,
            Ok(None) => {
                uncached_taps.push(tap_name.clone());
                continue;
            }
            Err(_) => continue,
        };

        for (skill_name, entry) in &registry.skills {
            let full_name = format!("{}/{}", tap_name, skill_name);
            seen_skills.insert(full_name.clone());
            let installed = db.installed.get(&full_name);

            let status = if installed.is_some() { "✓" } else { "○" };
            let commit = installed.and_then(|i| i.commit.clone()).unwrap_or_else(|| {
                if installed.is_some() {
                    "local".to_string()
                } else {
                    "-".to_string()
                }
            });

            // Check has_scripts/has_references for installed skills
            let extras = if installed.is_some() {
                if let Ok(idir) = get_skills_install_dir() {
                    let skill_dir = idir.join(tap_name).join(skill_name);
                    format_extras(has_scripts_dir(&skill_dir), has_references_dir(&skill_dir))
                } else {
                    "-".to_string()
                }
            } else {
                "-".to_string()
            };

            rows.push(SkillListRow {
                status,
                name: skill_name.clone(),
                tap: tap_name.clone(),
                description: truncate_string(
                    entry.description.as_deref().unwrap_or("No description"),
                    DESCRIPTION_MAX_LEN,
                ),
                extras,
                commit,
            });
        }
    }

    // Add installed skills that aren't from tap registries (directly added via URL)
    for (full_name, installed) in &db.installed {
        if seen_skills.contains(full_name) {
            continue;
        }

        // Get description from installed skill's SKILL.md if available
        let install_dir = get_skills_install_dir()?;
        let skill_md_path = install_dir.join(&installed.tap).join(&installed.skill).join("SKILL.md");

        let description = if skill_md_path.exists() {
            crate::skill::parse_skill_metadata(&skill_md_path)
                .ok()
                .and_then(|m| m.description)
                .unwrap_or_else(|| "Added from URL".to_string())
        } else {
            "Added from URL".to_string()
        };

        let skill_dir = install_dir.join(&installed.tap).join(&installed.skill);

        rows.push(SkillListRow {
            status: "✓",
            name: installed.skill.clone(),
            tap: installed.tap.clone(),
            description: truncate_string(&description, DESCRIPTION_MAX_LEN),
            extras: format_extras(has_scripts_dir(&skill_dir), has_references_dir(&skill_dir)),
            commit: installed.commit.clone().unwrap_or_else(|| "-".to_string()),
        });
    }

    if rows.is_empty() {
        println!("No skills available.");
        println!("  - Add a skill from URL: skillshub add <github-url>");
        println!("  - Install from default tap: skillshub install skillshub/<skill>");
        return Ok(());
    }

    // Sort by tap, then name
    rows.sort_by(|a, b| (&a.tap, &a.name).cmp(&(&b.tap, &b.name)));

    let installed_count = rows.iter().filter(|r| r.status == "✓").count();
    let total_count = rows.len();

    let table = Table::new(rows)
        .with(Style::rounded())
        .with(Padding::new(1, 1, 0, 1))
        .to_string();

    println!("{}", table);
    println!();
    println!(
        "{} installed, {} total",
        installed_count.to_string().green(),
        total_count
    );

    if !uncached_taps.is_empty() {
        println!(
            "\n{} {} tap(s) have no cached registry: {}.\n  Run 'skillshub tap update' to fetch the full registry.",
            "Note:".yellow().bold(),
            uncached_taps.len(),
            uncached_taps.join(", ")
        );
    }

    Ok(())
}

/// Search for skills across all taps
pub fn search_skills(query: &str) -> Result<()> {
    let db = db::init_db()?;

    if db.taps.is_empty() {
        println!("No taps configured. Run 'skillshub tap add <url>' to add one.");
        return Ok(());
    }

    let query_lower = query.to_lowercase();
    let mut results: Vec<SkillListRow> = Vec::new();

    for tap_name in db.taps.keys() {
        let registry = match get_tap_registry(&db, tap_name) {
            Ok(Some(r)) => r,
            Ok(None) | Err(_) => continue,
        };

        for (skill_name, entry) in &registry.skills {
            let name_lower = skill_name.to_lowercase();
            let desc_lower = entry.description.as_deref().unwrap_or("").to_lowercase();

            if name_lower.contains(&query_lower) || desc_lower.contains(&query_lower) {
                let full_name = format!("{}/{}", tap_name, skill_name);
                let installed = db.installed.get(&full_name);

                let extras = if installed.is_some() {
                    if let Ok(idir) = get_skills_install_dir() {
                        let skill_dir = idir.join(tap_name).join(skill_name);
                        format_extras(has_scripts_dir(&skill_dir), has_references_dir(&skill_dir))
                    } else {
                        "-".to_string()
                    }
                } else {
                    "-".to_string()
                };

                results.push(SkillListRow {
                    status: if installed.is_some() { "✓" } else { "○" },
                    name: skill_name.clone(),
                    tap: tap_name.clone(),
                    description: truncate_string(entry.description.as_deref().unwrap_or("No description"), 50),
                    extras,
                    commit: installed
                        .and_then(|i| i.commit.clone())
                        .unwrap_or_else(|| "-".to_string()),
                });
            }
        }
    }

    if results.is_empty() {
        println!("No skills found matching '{}'", query);
        return Ok(());
    }

    let table = Table::new(&results)
        .with(Style::rounded())
        .with(Padding::new(1, 1, 0, 1))
        .to_string();

    println!("{}", table);
    println!();
    println!("{} result(s) for '{}'", results.len(), query);

    Ok(())
}

/// Show detailed info about a skill
pub fn show_skill_info(full_name: &str) -> Result<()> {
    let skill_id = SkillId::parse(full_name)
        .with_context(|| format!("Invalid skill name '{}'. Use format: tap/skill", full_name))?;

    let db = db::init_db()?;
    let install_dir = get_skills_install_dir()?;

    // Check if installed
    let installed = db::get_installed_skill(&db, &skill_id.full_name());

    // Try to get info from tap registry first
    let tap_entry = db::get_tap(&db, &skill_id.tap)
        .and_then(|_| get_tap_registry(&db, &skill_id.tap).ok())
        .and_then(|opt| opt)
        .and_then(|r| r.skills.get(&skill_id.skill).cloned());

    // If not in tap registry, check if it's installed (directly added skill)
    if tap_entry.is_none() && installed.is_none() {
        anyhow::bail!(
            "Skill '{}' not found. It's neither in a tap registry nor installed.",
            full_name
        );
    }

    println!("{}", skill_id.full_name().bold());
    println!();

    // Get description from tap entry or from installed skill's SKILL.md
    let description = if let Some(entry) = &tap_entry {
        entry.description.clone()
    } else if installed.is_some() {
        // Try to read from installed skill's SKILL.md
        let skill_path = install_dir.join(&skill_id.tap).join(&skill_id.skill);
        discover_skills(&install_dir.join(&skill_id.tap))
            .ok()
            .and_then(|skills| {
                skills
                    .into_iter()
                    .find(|s| s.name == skill_id.skill || s.path == skill_path)
                    .map(|s| s.description)
            })
    } else {
        None
    };

    if let Some(desc) = description {
        println!("  {}: {}", "Description".cyan(), desc);
    }

    println!("  {}: {}", "Tap".cyan(), skill_id.tap);

    if let Some(entry) = &tap_entry {
        println!("  {}: {}", "Path".cyan(), entry.path);
        if let Some(homepage) = &entry.homepage {
            println!("  {}: {}", "Homepage".cyan(), homepage);
        }
    }

    // Read versioning metadata from installed SKILL.md when available.
    // Note: these fields (license, author, version) are only shown for locally installed
    // skills; they are not available for tap-available skills that have not been installed.
    let skill_md_path = install_dir.join(&skill_id.tap).join(&skill_id.skill).join("SKILL.md");
    let version_meta = if skill_md_path.exists() {
        parse_skill_metadata(&skill_md_path).ok()
    } else {
        None
    };

    if let Some(ref meta) = version_meta {
        if let Some(ref license) = meta.license {
            println!("  {}: {}", "License".cyan(), license);
        }
        if let Some(ref vm) = meta.metadata {
            if let Some(ref author) = vm.author {
                println!("  {}: {}", "Author".cyan(), author);
            }
            if let Some(ref version) = vm.version {
                println!("  {}: {}", "Version".cyan(), version);
            }
        }
    }

    // Show has_scripts and has_references for installed skills
    let skill_dir = install_dir.join(&skill_id.tap).join(&skill_id.skill);
    if skill_dir.exists() {
        // Use discover_skills to build a Skill with populated has_scripts/has_references
        let tap_skills_dir = install_dir.join(&skill_id.tap);
        let discovered = discover_skills(&tap_skills_dir).unwrap_or_default();
        let skill_info = discovered
            .into_iter()
            .find(|s| s.name == skill_id.skill || s.path == skill_dir);
        match skill_info {
            Some(s) => {
                println!(
                    "  {}: {}",
                    "Scripts".cyan(),
                    if s.has_scripts {
                        "Yes".green().to_string()
                    } else {
                        "No".to_string()
                    }
                );
                println!(
                    "  {}: {}",
                    "References".cyan(),
                    if s.has_references {
                        "Yes".green().to_string()
                    } else {
                        "No".to_string()
                    }
                );
            }
            None => {
                // Fallback to direct filesystem check
                println!(
                    "  {}: {}",
                    "Scripts".cyan(),
                    if has_scripts_dir(&skill_dir) {
                        "Yes".green().to_string()
                    } else {
                        "No".to_string()
                    }
                );
                println!(
                    "  {}: {}",
                    "References".cyan(),
                    if has_references_dir(&skill_dir) {
                        "Yes".green().to_string()
                    } else {
                        "No".to_string()
                    }
                );
            }
        }
    }

    println!(
        "  {}: {}",
        "Status".cyan(),
        if installed.is_some() {
            "Installed".green().to_string()
        } else {
            "Not installed".yellow().to_string()
        }
    );

    if let Some(inst) = installed {
        if let Some(commit) = &inst.commit {
            println!("  {}: {}", "Commit".cyan(), commit);
        }
        println!(
            "  {}: {}",
            "Installed".cyan(),
            inst.installed_at.format("%Y-%m-%d %H:%M")
        );

        // Show source URL for directly added skills
        if let Some(url) = &inst.source_url {
            println!("  {}: {}", "Source".cyan(), url);
        }

        // Show local path
        println!("  {}: {}", "Local path".cyan(), skill_dir.display());
    }

    // Show installation command if not installed
    if installed.is_none() {
        println!();
        println!(
            "Install with: {}",
            format!("skillshub install {}", skill_id.full_name()).bold()
        );
    }

    Ok(())
}

/// Install all skills from all added taps
pub fn install_all() -> Result<()> {
    let db = db::init_db()?;

    let mut all_taps: Vec<String> = db.taps.keys().cloned().collect();
    all_taps.sort();

    if all_taps.is_empty() {
        println!("No taps configured. Add one with 'skillshub tap add <url>'.");
        return Ok(());
    }

    let mut installed_count = 0;

    for tap_name in all_taps {
        installed_count += install_all_from_tap_internal(&db, &tap_name)?;
    }

    println!("\n{} Installed {} skills", "Done!".green().bold(), installed_count);

    // Auto-link to all agents (once after all installations)
    if installed_count > 0 {
        link_to_agents()?;
    }

    Ok(())
}

/// Install all skills from a specific tap
pub fn install_all_from_tap(tap_name: &str) -> Result<()> {
    let db = db::init_db()?;

    // Verify tap exists
    if db::get_tap(&db, tap_name).is_none() {
        anyhow::bail!("Tap '{}' not found. Add it with 'skillshub tap add <url>'", tap_name);
    }

    let installed_count = install_all_from_tap_internal(&db, tap_name)?;

    println!("\n{} Installed {} skills", "Done!".green().bold(), installed_count);

    // Auto-link to all agents (once after all installations)
    if installed_count > 0 {
        link_to_agents()?;
    }

    Ok(())
}

/// Internal helper to install all skills from a tap (used by both install_all and install_all_from_tap)
fn install_all_from_tap_internal(db: &super::models::Database, tap_name: &str) -> Result<usize> {
    // Skip gist taps — their skills are installed at add-time and have no registry
    if let Some(tap) = db::get_tap(db, tap_name) {
        if tap.url.contains("gist.github.com") {
            let count = db::get_skills_from_tap(db, tap_name).len();
            println!("  {} {} ({} skills, gist — skipped)", "○".yellow(), tap_name, count);
            return Ok(0);
        }
    }

    let registry = get_tap_registry(db, tap_name)
        .with_context(|| format!("Failed to get registry for tap '{}'", tap_name))?
        .with_context(|| {
            format!(
                "No cached registry for tap '{}'. Run 'skillshub tap update {}' first.",
                tap_name, tap_name
            )
        })?;

    if registry.skills.is_empty() {
        println!("No skills available in tap '{}'.", tap_name);
        return Ok(0);
    }

    println!(
        "{} Installing {} skills from '{}'",
        "=>".green().bold(),
        registry.skills.len(),
        tap_name
    );

    let mut installed_count = 0;

    for skill_name in registry.skills.keys() {
        let full_name = format!("{}/{}", tap_name, skill_name);

        if db::is_skill_installed(db, &full_name) {
            println!("  {} {} (already installed)", "○".yellow(), full_name);
            continue;
        }

        match install_skill_internal(&full_name) {
            Ok(true) => installed_count += 1,
            Ok(false) => {}
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), full_name, e);
            }
        }
    }

    Ok(installed_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build an installed skill and its on-disk directory under `install_dir`.
    fn seed_installed(db: &mut Database, install_dir: &Path, tap: &str, skill: &str) {
        fs::create_dir_all(install_dir.join(tap).join(skill)).unwrap();
        db.installed.insert(
            format!("{}/{}", tap, skill),
            InstalledSkill {
                tap: tap.to_string(),
                skill: skill.to_string(),
                commit: None,
                installed_at: Utc::now(),
                source_url: None,
                source_path: None,
                gist_updated_at: None,
            },
        );
    }

    /// Create a symlink `link` -> `target` (unix-only; tests using it are cfg'd).
    #[cfg(unix)]
    fn make_symlink(target: &Path, link: &Path) {
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_remove_agent_symlinks_in_removes_only_managed_links() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();
        let agent_dir = TempDir::new().unwrap();
        let skill_dir = install_dir.path().join("tap/myskill");
        fs::create_dir_all(&skill_dir).unwrap();

        let agent_skills = agent_dir.path().join("skills");

        // Managed symlink (points into install_dir) -> removed
        make_symlink(&skill_dir, &agent_skills.join("managed"));
        // Dangling managed symlink (target gone) -> still removed
        make_symlink(&install_dir.path().join("tap/gone"), &agent_skills.join("dangling"));
        // External symlink (points outside install_dir) -> kept
        let outside = TempDir::new().unwrap();
        make_symlink(outside.path(), &agent_skills.join("external"));
        // Real directory with same name -> kept
        fs::create_dir_all(agent_skills.join("realdir")).unwrap();

        remove_agent_symlinks_in(std::slice::from_ref(&agent_skills), "managed", install_dir.path());
        remove_agent_symlinks_in(std::slice::from_ref(&agent_skills), "dangling", install_dir.path());
        remove_agent_symlinks_in(std::slice::from_ref(&agent_skills), "external", install_dir.path());
        remove_agent_symlinks_in(std::slice::from_ref(&agent_skills), "realdir", install_dir.path());

        assert!(agent_skills.join("managed").symlink_metadata().is_err());
        assert!(agent_skills.join("dangling").symlink_metadata().is_err());
        assert!(agent_skills.join("external").symlink_metadata().is_ok());
        assert!(agent_skills.join("realdir").is_dir());
    }

    #[test]
    fn test_batch_removes_files_and_keeps_unlisted_entries() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "tap", "a");
        seed_installed(&mut db, install_dir.path(), "tap", "b");
        seed_installed(&mut db, install_dir.path(), "other", "c");

        let results =
            remove_installed_skills_batch(&mut db, install_dir.path(), &["tap/a".to_string(), "tap/b".to_string()]);

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, r)| r.is_ok()));
        assert!(!install_dir.path().join("tap/a").exists());
        assert!(!install_dir.path().join("tap/b").exists());
        assert!(
            !install_dir.path().join("tap").exists(),
            "empty tap dir should be cleaned up"
        );
        assert!(!db.installed.contains_key("tap/a"));
        assert!(!db.installed.contains_key("tap/b"));
        assert!(
            db.installed.contains_key("other/c"),
            "skills not in the removal list must stay"
        );
        assert!(install_dir.path().join("other/c").exists());
    }

    #[test]
    fn test_batch_continues_after_failure() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "tap", "good");
        // Force a deterministic failure: a regular *file* at the skill's
        // install path makes remove_dir_all error on all platforms.
        seed_installed(&mut db, install_dir.path(), "tap", "bad");
        fs::remove_dir_all(install_dir.path().join("tap/bad")).unwrap();
        fs::write(install_dir.path().join("tap/bad"), b"not a dir").unwrap();

        let results = remove_installed_skills_batch(
            &mut db,
            install_dir.path(),
            &["tap/bad".to_string(), "tap/good".to_string()],
        );

        assert_eq!(results.len(), 2);
        assert!(results[0].1.is_err(), "file-instead-of-dir should fail");
        assert!(results[1].1.is_ok(), "batch must continue past the failure");
        assert!(!install_dir.path().join("tap/good").exists());
        assert!(
            !db.installed.contains_key("tap/good"),
            "succeeded entry should be removed from db"
        );
        assert!(db.installed.contains_key("tap/bad"), "failed entry should stay in db");
    }

    #[test]
    fn test_batch_records_error_for_unparseable_name() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "tap", "a");

        let results = remove_installed_skills_batch(
            &mut db,
            install_dir.path(),
            &["no-slash-here".to_string(), "tap/a".to_string()],
        );

        assert_eq!(results.len(), 2);
        assert!(results[0].1.is_err());
        assert!(results[1].1.is_ok());
        assert!(!db.installed.contains_key("tap/a"));
    }

    /// With `prune = true`, a skill missing upstream is removed from disk and db.
    #[test]
    fn test_prune_or_report_missing_prunes_when_flag_set() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "test/tap", "beta");
        let installed = db.installed.get("test/tap/beta").unwrap().clone();

        prune_or_report_missing(&mut db, install_dir.path(), &installed, "test/tap/beta", true);

        assert!(
            !install_dir.path().join("test/tap/beta").exists(),
            "orphan dir should be removed"
        );
        assert!(
            !db.installed.contains_key("test/tap/beta"),
            "orphan db entry should be removed"
        );
    }

    /// With `prune = false`, the skill is reported but left installed (files and db entry).
    #[test]
    fn test_prune_or_report_missing_keeps_when_flag_unset() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "test/tap", "beta");
        let installed = db.installed.get("test/tap/beta").unwrap().clone();

        prune_or_report_missing(&mut db, install_dir.path(), &installed, "test/tap/beta", false);

        assert!(
            install_dir.path().join("test/tap/beta").exists(),
            "dir should remain without --prune"
        );
        assert!(
            db.installed.contains_key("test/tap/beta"),
            "db entry should remain without --prune"
        );
    }

    #[test]
    fn test_install_from_local_nonexistent_skill_returns_error() {
        // A definitely-nonexistent skill name: install_from_local should error
        let tmp = std::env::temp_dir().join("skillshub_test_dest_nonexistent");
        let result = install_from_local("__nonexistent_test_skill_xyz__", &tmp);
        // Either the embedded dir is not found (Ok path fails) or skill is not in it
        assert!(
            result.is_err(),
            "install_from_local should fail for a nonexistent skill"
        );
    }

    #[test]
    fn test_copy_dir_contents_copies_tree() {
        use tempfile::TempDir;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Create a nested structure in src
        fs::create_dir_all(src.path().join("subdir")).unwrap();
        fs::write(src.path().join("file.txt"), b"hello").unwrap();
        fs::write(src.path().join("subdir/nested.txt"), b"world").unwrap();

        copy_dir_contents(src.path(), dst.path()).unwrap();

        assert!(dst.path().join("file.txt").exists());
        assert!(dst.path().join("subdir/nested.txt").exists());
        assert_eq!(fs::read(dst.path().join("file.txt")).unwrap(), b"hello");
        assert_eq!(fs::read(dst.path().join("subdir/nested.txt")).unwrap(), b"world");
    }

    #[test]
    fn test_install_all_from_tap_internal_skips_gist_taps() {
        use super::super::models::{Database, TapInfo};
        use std::collections::HashMap;

        let mut taps = HashMap::new();
        taps.insert(
            "garrytan/gists".to_string(),
            TapInfo {
                url: "https://gist.github.com/garrytan".to_string(),
                skills_path: String::new(),
                updated_at: None,
                is_default: false,
                cached_registry: None,
                branch: None,
            },
        );

        let db = Database {
            taps,
            ..Default::default()
        };

        // Should return Ok(0) instead of erroring about missing registry
        let result = install_all_from_tap_internal(&db, "garrytan/gists");
        assert!(
            result.is_ok(),
            "gist taps should be skipped, not error: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_copy_dir_contents_handles_empty_dir() {
        use tempfile::TempDir;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Empty source should produce no error and empty destination
        copy_dir_contents(src.path(), dst.path()).unwrap();

        let entries: Vec<_> = fs::read_dir(dst.path()).unwrap().collect();
        assert!(
            entries.is_empty(),
            "destination should be empty after copying empty source"
        );
    }

    #[test]
    fn test_format_extras_neither() {
        assert_eq!(format_extras(false, false), "-");
    }

    #[test]
    fn test_format_extras_scripts_only() {
        assert_eq!(format_extras(true, false), "scripts");
    }

    #[test]
    fn test_format_extras_refs_only() {
        assert_eq!(format_extras(false, true), "refs");
    }

    #[test]
    fn test_format_extras_both() {
        assert_eq!(format_extras(true, true), "scripts, refs");
    }

    /// RAII guard that restores `SKILLSHUB_TEST_HOME` on drop, so tests that
    /// exercise the real home-based paths (init_db, get_skills_install_dir)
    /// stay hermetic. Mirrors the guard in registry::tap tests.
    struct TestHomeGuard(Option<String>);

    impl TestHomeGuard {
        fn set(home: &Path) -> Self {
            let prev = std::env::var("SKILLSHUB_TEST_HOME").ok();
            std::env::set_var("SKILLSHUB_TEST_HOME", home);
            Self(prev)
        }
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => std::env::set_var("SKILLSHUB_TEST_HOME", v),
                None => std::env::remove_var("SKILLSHUB_TEST_HOME"),
            }
        }
    }

    #[test]
    fn test_resolve_update_names_all_returns_everything_installed() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "tap", "a");
        seed_installed(&mut db, install_dir.path(), "other/tap", "b");

        let mut names = resolve_update_names(&db, &UpdateSelection::All).unwrap();
        names.sort();
        assert_eq!(names, vec!["other/tap/b".to_string(), "tap/a".to_string()]);
    }

    #[test]
    fn test_resolve_update_names_selected_valid_name() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "tap", "a");

        let names = resolve_update_names(&db, &UpdateSelection::Selected(vec!["tap/a".to_string()])).unwrap();
        assert_eq!(names, vec!["tap/a".to_string()]);
    }

    #[test]
    fn test_resolve_update_names_selected_invalid_format_bails() {
        let db = Database::default();

        let err = resolve_update_names(&db, &UpdateSelection::Selected(vec!["noslash".to_string()])).unwrap_err();
        assert!(
            err.to_string().contains("Invalid skill name 'noslash'"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_resolve_update_names_selected_not_installed_bails() {
        use tempfile::TempDir;
        let install_dir = TempDir::new().unwrap();

        let mut db = Database::default();
        seed_installed(&mut db, install_dir.path(), "tap", "a");

        let err = resolve_update_names(&db, &UpdateSelection::Selected(vec!["tap/missing".to_string()])).unwrap_err();
        assert!(
            err.to_string().contains("Skill 'tap/missing' is not installed"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_uninstall_skill_not_installed_bails() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let _guard = TestHomeGuard::set(&home);

        let err = uninstall_skill("tap/missing").unwrap_err();
        assert!(
            err.to_string().contains("Skill 'tap/missing' is not installed"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_uninstall_skill_removes_files_and_db_entry() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let _guard = TestHomeGuard::set(&home);

        let install_dir = get_skills_install_dir().unwrap();
        let mut db = Database::default();
        seed_installed(&mut db, &install_dir, "tap", "a");
        seed_installed(&mut db, &install_dir, "tap", "b");
        db::save_db(&db).unwrap();

        uninstall_skill("tap/a").unwrap();

        assert!(!install_dir.join("tap/a").exists(), "skill dir should be removed");
        assert!(install_dir.join("tap/b").exists(), "other skills must stay");

        let db = db::load_db().unwrap();
        assert!(!db.installed.contains_key("tap/a"), "db entry should be removed");
        assert!(db.installed.contains_key("tap/b"), "other db entries must stay");
    }
}
