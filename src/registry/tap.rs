use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use super::db::{self, DEFAULT_TAP_NAME};
use super::github::{discover_skills_from_repo, parse_github_url};
use super::models::{Database, TapInfo, TapRegistry};
use crate::util::truncate_string;

const TAP_URL_MAX_LEN: usize = 50;

/// Table row for displaying taps
#[derive(Tabled)]
pub struct TapRow {
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "URL")]
    pub url: String,
    #[tabled(rename = "Skills")]
    pub skills_count: String,
    #[tabled(rename = "Default")]
    pub is_default: &'static str,
}

/// Add a new tap from a GitHub URL
pub fn add_tap(url: &str, install: bool) -> Result<()> {
    let github_url = parse_github_url(url)?;
    let tap_name = github_url.tap_name();

    let mut db = db::init_db()?;

    // Check if tap already exists
    if db.taps.contains_key(&tap_name) {
        anyhow::bail!(
            "Tap '{}' already exists. Use 'skillshub tap remove {}' first.",
            tap_name,
            tap_name
        );
    }

    let base_url = github_url.base_url();
    println!("{} Adding tap '{}' from {}", "=>".green().bold(), tap_name, base_url);

    // Discover skills by scanning for SKILL.md files
    println!("  {} Discovering skills...", "○".yellow());
    let registry = discover_skills_from_repo(&github_url, &tap_name)
        .with_context(|| format!("Failed to discover skills from {}", base_url))?;

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

    // Show available skills (only if not installing)
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

    // Install all skills if requested
    if install && !registry.skills.is_empty() {
        println!();
        super::skill::install_all_from_tap(&tap_name)?;
    }

    Ok(())
}

/// Remove a tap
pub fn remove_tap(name: &str) -> Result<()> {
    let mut db = db::init_db()?;

    // Check if tap exists
    let tap = db::get_tap(&db, name).with_context(|| format!("Tap '{}' not found", name))?;

    // Prevent removing default tap
    if tap.is_default {
        anyhow::bail!("Cannot remove the default tap '{}'", name);
    }

    // Check for installed skills from this tap
    let installed_from_tap = db::get_skills_from_tap(&db, name);
    if !installed_from_tap.is_empty() {
        let skill_names: Vec<_> = installed_from_tap.iter().map(|(n, _)| n.as_str()).collect();
        anyhow::bail!(
            "Cannot remove tap '{}': {} skills are installed from it.\n\
             Uninstall these skills first: {}",
            name,
            installed_from_tap.len(),
            skill_names.join(", ")
        );
    }

    db::remove_tap(&mut db, name);
    db::save_db(&db)?;

    println!("{} Removed tap '{}'", "✓".green(), name);

    Ok(())
}

/// List all configured taps
pub fn list_taps() -> Result<()> {
    let db = db::init_db()?;

    if db.taps.is_empty() {
        println!("No taps configured.");
        return Ok(());
    }

    let mut rows: Vec<TapRow> = Vec::new();

    for (name, tap) in &db.taps {
        let installed_count = count_installed_skills(&db, name);
        let available_count = get_tap_registry(&db, name).ok().map(|registry| registry.skills.len());
        let skills_count = format_skills_count(installed_count, available_count);

        rows.push(TapRow {
            name: name.clone(),
            url: truncate_string(&tap.url, TAP_URL_MAX_LEN),
            skills_count,
            is_default: if tap.is_default { "✓" } else { "" },
        });
    }

    // Sort with default tap first
    rows.sort_by(|a, b| match (a.is_default == "✓", b.is_default == "✓") {
        (true, true) => a.name.cmp(&b.name),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => a.name.cmp(&b.name),
    });

    let table = Table::new(rows)
        .with(Style::rounded())
        .with(Padding::new(1, 1, 0, 1))
        .to_string();

    println!("{}", table);
    println!();
    println!("{} taps configured", db.taps.len());

    Ok(())
}

/// Update tap registries (fetch latest from remote)
pub fn update_tap(name: Option<&str>) -> Result<()> {
    let mut db = db::init_db()?;

    let taps_to_update: Vec<String> = match name {
        Some(n) => {
            if !db.taps.contains_key(n) {
                anyhow::bail!("Tap '{}' not found", n);
            }
            vec![n.to_string()]
        }
        None => db.taps.keys().cloned().collect(),
    };

    for tap_name in taps_to_update {
        let tap = db.taps.get(&tap_name).unwrap().clone();

        print!("  {} Updating {}...", "○".yellow(), tap_name);

        match update_single_tap(&mut db, &tap_name, &tap) {
            Ok(count) => {
                println!("\r  {} {} ({} skills)", "✓".green(), tap_name, count);
            }
            Err(e) => {
                println!("\r  {} {} ({})", "✗".red(), tap_name, e);
            }
        }
    }

    db::save_db(&db)?;

    Ok(())
}

/// Update a single tap, refresh cache, and return skill count
fn update_single_tap(db: &mut Database, name: &str, tap: &TapInfo) -> Result<usize> {
    let github_url = parse_github_url(&tap.url)?;
    let registry = discover_skills_from_repo(&github_url, name)?;
    let count = registry.skills.len();

    // Update cache and timestamp in database
    if let Some(t) = db.taps.get_mut(name) {
        t.cached_registry = Some(registry);
        t.updated_at = Some(Utc::now());
    }

    Ok(count)
}

/// Count installed skills for a given tap
fn count_installed_skills(db: &Database, tap_name: &str) -> usize {
    db::get_skills_from_tap(db, tap_name).len()
}

/// Format installed/available skill counts for display
fn format_skills_count(installed: usize, available: Option<usize>) -> String {
    let available_display = available
        .map(|count| count.to_string())
        .unwrap_or_else(|| "?".to_string());
    format!("{}/{}", installed, available_display)
}

/// Get the registry for a tap (uses cache if available, otherwise fetches from remote)
pub fn get_tap_registry(db: &Database, tap_name: &str) -> Result<TapRegistry> {
    let tap = db::get_tap(db, tap_name).with_context(|| format!("Tap '{}' not found", tap_name))?;

    // Return cached registry if available
    if let Some(ref registry) = tap.cached_registry {
        return Ok(registry.clone());
    }

    // No cache available, fetch from remote
    // This shouldn't normally happen since we cache on add_tap and update_tap,
    // but handles edge cases like database migration from older versions
    let github_url = parse_github_url(&tap.url)?;
    match discover_skills_from_repo(&github_url, tap_name) {
        Ok(registry) => Ok(registry),
        Err(e) => {
            // For the default tap, fall back to local registry if remote fails
            // (e.g., no network on first run)
            if tap.is_default {
                return generate_local_registry();
            }
            Err(e)
        }
    }
}

/// Generate a registry from local/bundled skills
pub fn generate_local_registry() -> Result<TapRegistry> {
    use crate::paths::get_embedded_skills_dir;
    use crate::skill::discover_skills;
    use std::collections::HashMap;

    use super::models::SkillEntry;

    let skills_dir = get_embedded_skills_dir()?;
    let skills = discover_skills(&skills_dir)?;

    let mut skill_entries = HashMap::new();
    for skill in skills {
        skill_entries.insert(
            skill.name.clone(),
            SkillEntry {
                path: format!("skills/{}", skill.name),
                description: Some(skill.description),
                homepage: None,
            },
        );
    }

    Ok(TapRegistry {
        name: DEFAULT_TAP_NAME.to_string(),
        description: Some("Default skillshub tap with bundled skills".to_string()),
        skills: skill_entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::InstalledSkill;
    use chrono::Utc;

    #[test]
    fn test_truncate_url_short() {
        assert_eq!(
            truncate_string("https://short.url", TAP_URL_MAX_LEN),
            "https://short.url"
        );
    }

    #[test]
    fn test_truncate_url_long() {
        let long_url = "https://github.com/very/long/path/to/repository/that/exceeds/limit";
        let truncated = truncate_string(long_url, 30);
        assert!(truncated.len() <= 30);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_format_skills_count_known() {
        assert_eq!(format_skills_count(2, Some(10)), "2/10");
    }

    #[test]
    fn test_format_skills_count_unknown() {
        assert_eq!(format_skills_count(1, None), "1/?");
    }

    #[test]
    fn test_count_installed_skills() {
        let mut db = Database::default();
        db.installed.insert(
            "tap1/skill1".to_string(),
            InstalledSkill {
                tap: "tap1".to_string(),
                skill: "skill1".to_string(),
                commit: None,
                installed_at: Utc::now(),
                source_url: None,
                source_path: None,
            },
        );
        db.installed.insert(
            "tap1/skill2".to_string(),
            InstalledSkill {
                tap: "tap1".to_string(),
                skill: "skill2".to_string(),
                commit: None,
                installed_at: Utc::now(),
                source_url: None,
                source_path: None,
            },
        );
        db.installed.insert(
            "tap2/skill1".to_string(),
            InstalledSkill {
                tap: "tap2".to_string(),
                skill: "skill1".to_string(),
                commit: None,
                installed_at: Utc::now(),
                source_url: None,
                source_path: None,
            },
        );

        assert_eq!(count_installed_skills(&db, "tap1"), 2);
        assert_eq!(count_installed_skills(&db, "tap2"), 1);
        assert_eq!(count_installed_skills(&db, "missing"), 0);
    }
}
