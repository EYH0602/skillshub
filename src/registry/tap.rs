use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use super::db::{self, DEFAULT_TAP_NAME};
use super::github::{fetch_tap_registry, parse_github_url};
use super::models::{Database, TapInfo, TapRegistry};

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
pub fn add_tap(url: &str) -> Result<()> {
    let github_url = parse_github_url(url)?;
    let tap_name = github_url.tap_name().to_string();

    let mut db = db::init_db()?;

    // Check if tap already exists
    if db.taps.contains_key(&tap_name) {
        anyhow::bail!(
            "Tap '{}' already exists. Use 'skillshub tap remove {}' first.",
            tap_name,
            tap_name
        );
    }

    println!(
        "{} Adding tap '{}' from {}",
        "=>".green().bold(),
        tap_name,
        url
    );

    // Verify the tap has a valid registry.json
    println!("  {} Fetching registry...", "○".yellow());
    let registry = fetch_tap_registry(&github_url, "registry.json")
        .with_context(|| format!("Failed to fetch registry from {}", url))?;

    let tap_info = TapInfo {
        url: url.to_string(),
        skills_path: "skills".to_string(), // Default, could be configured
        updated_at: Some(Utc::now()),
        is_default: false,
    };

    db::add_tap(&mut db, &tap_name, tap_info);
    db::save_db(&db)?;

    println!(
        "  {} Added tap '{}' with {} skills",
        "✓".green(),
        tap_name,
        registry.skills.len()
    );

    // Show available skills
    if !registry.skills.is_empty() {
        println!("\n  Available skills:");
        for (name, entry) in registry.skills.iter().take(10) {
            let desc = entry.description.as_deref().unwrap_or("No description");
            println!("    {} {}/{} - {}", "•".cyan(), tap_name, name, desc);
        }
        if registry.skills.len() > 10 {
            println!(
                "    {} ... and {} more",
                "•".cyan(),
                registry.skills.len() - 10
            );
        }
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
        let available_count = if tap.is_default {
            // For default tap, count local skills
            count_local_skills().ok()
        } else {
            // For remote taps, try to get from registry
            get_tap_skill_count(tap).ok()
        };
        let skills_count = format_skills_count(installed_count, available_count);

        rows.push(TapRow {
            name: name.clone(),
            url: truncate_url(&tap.url, 50),
            skills_count,
            is_default: if tap.is_default { "✓" } else { "" },
        });
    }

    // Sort with default tap first
    rows.sort_by(|a, b| {
        if a.is_default == "✓" {
            std::cmp::Ordering::Less
        } else if b.is_default == "✓" {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
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
        let tap = db.taps.get(&tap_name).unwrap();

        if tap.is_default {
            println!("  {} {} (default tap, skipped)", "○".yellow(), tap_name);
            continue;
        }

        print!("  {} Updating {}...", "○".yellow(), tap_name);

        match update_single_tap(&tap_name, tap) {
            Ok(count) => {
                // Update timestamp
                if let Some(t) = db.taps.get_mut(&tap_name) {
                    t.updated_at = Some(Utc::now());
                }
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

/// Update a single tap and return skill count
fn update_single_tap(name: &str, tap: &TapInfo) -> Result<usize> {
    let github_url = parse_github_url(&tap.url)?;
    let registry = fetch_tap_registry(&github_url, "registry.json")?;

    // Verify name matches
    if registry.name != name {
        anyhow::bail!(
            "Registry name mismatch: expected '{}', got '{}'",
            name,
            registry.name
        );
    }

    Ok(registry.skills.len())
}

/// Get skill count from a tap's registry
fn get_tap_skill_count(tap: &TapInfo) -> Result<usize> {
    let github_url = parse_github_url(&tap.url)?;
    let registry = fetch_tap_registry(&github_url, "registry.json")?;
    Ok(registry.skills.len())
}

/// Count local skills in the embedded directory
fn count_local_skills() -> Result<usize> {
    use crate::paths::get_embedded_skills_dir;
    use crate::skill::discover_skills;

    let skills_dir = get_embedded_skills_dir()?;
    let skills = discover_skills(&skills_dir)?;
    Ok(skills.len())
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

/// Truncate URL for display
fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        url.to_string()
    } else {
        format!("{}...", &url[..max_len.saturating_sub(3)])
    }
}

/// Get the registry for a tap (fetches from remote or generates for default)
pub fn get_tap_registry(db: &Database, tap_name: &str) -> Result<TapRegistry> {
    let tap = db::get_tap(db, tap_name).with_context(|| format!("Tap '{}' not found", tap_name))?;

    if tap.is_default {
        // Generate registry from local skills
        generate_local_registry()
    } else {
        // Fetch from remote
        let github_url = parse_github_url(&tap.url)?;
        fetch_tap_registry(&github_url, "registry.json")
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
        assert_eq!(truncate_url("https://short.url", 50), "https://short.url");
    }

    #[test]
    fn test_truncate_url_long() {
        let long_url = "https://github.com/very/long/path/to/repository/that/exceeds/limit";
        let truncated = truncate_url(long_url, 30);
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
                local: false,
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
                local: false,
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
                local: false,
                source_url: None,
                source_path: None,
            },
        );

        assert_eq!(count_installed_skills(&db, "tap1"), 2);
        assert_eq!(count_installed_skills(&db, "tap2"), 1);
        assert_eq!(count_installed_skills(&db, "missing"), 0);
    }
}
