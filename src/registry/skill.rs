use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use super::db;
use super::github::{download_skill, get_default_branch, get_latest_commit, parse_github_url};
use super::models::{InstalledSkill, SkillId};
use super::tap::get_tap_registry;
use crate::commands::link_to_agents;
use crate::paths::get_skills_install_dir;
use crate::skill::discover_skills;
use crate::util::truncate_string;

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
    #[tabled(rename = "Commit")]
    pub commit: String,
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
    let registry = get_tap_registry(&db, &skill_id.tap)?;
    let skill_entry = registry.skills.get(&skill_id.skill).with_context(|| {
        format!(
            "Skill '{}' not found in tap '{}'. Run 'skillshub search {}' to find it.",
            skill_id.skill, skill_id.tap, skill_id.skill
        )
    })?;

    println!("{} Installing '{}'", "=>".green().bold(), skill_id.full_name());

    let dest = install_dir.join(&skill_id.tap).join(&skill_id.skill);
    std::fs::create_dir_all(&dest)?;

    let (commit, _) = install_from_remote(&tap.url, &skill_entry.path, &dest, requested_commit.as_deref())?;

    // Record in database
    let installed = InstalledSkill {
        tap: skill_id.tap.clone(),
        skill: skill_id.skill.clone(),
        commit,
        installed_at: Utc::now(),
        source_url: Some(tap.url.clone()),
        source_path: Some(skill_entry.path.clone()),
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

    println!("{} Adding '{}' from {}", "=>".green().bold(), full_name, url);

    // Determine commit to use (if branch looks like a commit SHA, use it as the commit)
    let commit = if github_url.is_commit_sha() {
        github_url.branch.clone()
    } else {
        None
    };

    let dest = install_dir.join(&tap_name).join(&skill_name);
    std::fs::create_dir_all(&dest)?;

    // Download the skill
    let commit_sha = download_skill(&github_url, skill_path, &dest, commit.as_deref())?;

    // Add tap if it doesn't exist
    if db::get_tap(&db, &tap_name).is_none() {
        let tap_url = format!("https://github.com/{}/{}", github_url.owner, github_url.repo);
        let tap_info = super::models::TapInfo {
            url: tap_url,
            skills_path: "skills".to_string(),
            updated_at: Some(Utc::now()),
            is_default: false,
            cached_registry: None, // Cache will be populated on next tap update
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

/// Install from remote tap
fn install_from_remote(
    tap_url: &str,
    skill_path: &str,
    dest: &std::path::Path,
    commit: Option<&str>,
) -> Result<(Option<String>, bool)> {
    let github_url = parse_github_url(tap_url)?;

    // Remove dest if it exists (reinstall)
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }

    let commit_sha = download_skill(&github_url, skill_path, dest, commit)?;

    Ok((Some(commit_sha), false))
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

    let skill_path = install_dir.join(&skill_id.tap).join(&skill_id.skill);

    if skill_path.exists() {
        std::fs::remove_dir_all(&skill_path)?;
    }

    // Clean up empty tap directory
    let tap_dir = install_dir.join(&skill_id.tap);
    if tap_dir.exists() && tap_dir.read_dir()?.next().is_none() {
        std::fs::remove_dir(&tap_dir)?;
    }

    db::remove_installed_skill(&mut db, &skill_id.full_name());
    db::save_db(&db)?;

    println!("{} Uninstalled '{}'", "✓".green(), skill_id.full_name());

    Ok(())
}

/// Update a skill (or all skills) to latest version
pub fn update_skill(full_name: Option<&str>) -> Result<()> {
    let mut db = db::init_db()?;

    let skills_to_update: Vec<String> = match full_name {
        Some(name) => {
            let skill_id = SkillId::parse(name)
                .with_context(|| format!("Invalid skill name '{}'. Use format: tap/skill", name))?;

            if !db::is_skill_installed(&db, &skill_id.full_name()) {
                anyhow::bail!("Skill '{}' is not installed", skill_id.full_name());
            }

            vec![skill_id.full_name()]
        }
        None => db.installed.keys().cloned().collect(),
    };

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

    for skill_name in skills_to_update {
        let installed = db.installed.get(&skill_name).unwrap().clone();

        let tap = match db::get_tap(&db, &installed.tap) {
            Some(t) => t.clone(),
            None => {
                println!("  {} {} (tap not found)", "✗".red(), skill_name);
                continue;
            }
        };

        // Get latest commit
        let github_url = match parse_github_url(&tap.url) {
            Ok(u) => u,
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
                continue;
            }
        };

        let registry = match get_tap_registry(&db, &installed.tap) {
            Ok(r) => r,
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

        // Resolve branch for the tap
        let resolved_branch = match &github_url.branch {
            Some(b) => b.clone(),
            None => match get_default_branch(&github_url.owner, &github_url.repo) {
                Ok(b) => b,
                Err(e) => {
                    println!("  {} {} ({})", "✗".red(), skill_name, e);
                    continue;
                }
            },
        };

        let latest_commit = match get_latest_commit(&github_url, Some(&skill_entry.path), &resolved_branch) {
            Ok(c) => c,
            Err(e) => {
                println!("  {} {} ({})", "✗".red(), skill_name, e);
                continue;
            }
        };

        // Check if update needed
        if installed.commit.as_deref() == Some(&latest_commit) {
            println!("  {} {} (up to date)", "✓".green(), skill_name);
            continue;
        }

        // Perform update
        let install_dir = get_skills_install_dir()?;
        let dest = install_dir.join(&installed.tap).join(&installed.skill);

        match install_from_remote(&tap.url, &skill_entry.path, &dest, Some(&latest_commit)) {
            Ok((new_commit, _)) => {
                // Update database
                if let Some(skill) = db.installed.get_mut(&skill_name) {
                    skill.commit = new_commit;
                    skill.installed_at = Utc::now();
                }

                println!(
                    "  {} {} ({} -> {})",
                    "✓".green(),
                    skill_name,
                    installed.commit.as_deref().unwrap_or("unknown"),
                    latest_commit
                );
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
    for tap_name in db.taps.keys() {
        let registry = match get_tap_registry(&db, tap_name) {
            Ok(r) => r,
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

            rows.push(SkillListRow {
                status,
                name: skill_name.clone(),
                tap: tap_name.clone(),
                description: truncate_string(
                    entry.description.as_deref().unwrap_or("No description"),
                    DESCRIPTION_MAX_LEN,
                ),
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

        rows.push(SkillListRow {
            status: "✓",
            name: installed.skill.clone(),
            tap: installed.tap.clone(),
            description: truncate_string(&description, DESCRIPTION_MAX_LEN),
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
            Ok(r) => r,
            Err(_) => continue,
        };

        for (skill_name, entry) in &registry.skills {
            let name_lower = skill_name.to_lowercase();
            let desc_lower = entry.description.as_deref().unwrap_or("").to_lowercase();

            if name_lower.contains(&query_lower) || desc_lower.contains(&query_lower) {
                let full_name = format!("{}/{}", tap_name, skill_name);
                let installed = db.installed.get(&full_name);

                results.push(SkillListRow {
                    status: if installed.is_some() { "✓" } else { "○" },
                    name: skill_name.clone(),
                    tap: tap_name.clone(),
                    description: truncate_string(entry.description.as_deref().unwrap_or("No description"), 50),
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
        let skill_path = install_dir.join(&skill_id.tap).join(&skill_id.skill);
        println!("  {}: {}", "Local path".cyan(), skill_path.display());
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

/// Install all skills from default taps
pub fn install_all() -> Result<()> {
    let db = db::init_db()?;

    let mut default_taps: Vec<String> = db
        .taps
        .iter()
        .filter(|(_, tap)| tap.is_default)
        .map(|(name, _)| name.clone())
        .collect();
    default_taps.sort();

    if default_taps.is_empty() {
        println!("No default taps configured.");
        return Ok(());
    }

    let mut installed_count = 0;

    for tap_name in default_taps {
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
    let registry =
        get_tap_registry(db, tap_name).with_context(|| format!("Failed to get registry for tap '{}'", tap_name))?;

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
