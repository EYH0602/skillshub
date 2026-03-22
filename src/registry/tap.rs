use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use super::db::{self, DEFAULT_TAP_NAME};
use super::github::{discover_skills_from_repo, fetch_star_list_repos, parse_github_url, parse_star_list_url};
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

/// Remove a tap, optionally keeping its installed skills
pub fn remove_tap(name: &str, keep_skills: bool) -> Result<()> {
    let mut db = db::init_db()?;

    // Check if tap exists
    let tap = db::get_tap(&db, name).with_context(|| format!("Tap '{}' not found", name))?;

    // Prevent removing default tap
    if tap.is_default {
        anyhow::bail!("Cannot remove the default tap '{}'", name);
    }

    // Handle installed skills from this tap
    let installed_from_tap = db::get_skills_from_tap(&db, name);
    if !installed_from_tap.is_empty() {
        let skill_names: Vec<String> = installed_from_tap.iter().map(|(n, _)| (*n).clone()).collect();

        if keep_skills {
            println!(
                "  {} {} skill(s) kept but can no longer be updated (tap removed):",
                "!".yellow().bold(),
                skill_names.len()
            );
            for full_name in &skill_names {
                println!("      {}", full_name);
            }
        } else {
            println!(
                "{} Uninstalling {} skill(s) from tap '{}'",
                "=>".green().bold(),
                skill_names.len(),
                name
            );

            for full_name in &skill_names {
                super::skill::uninstall_skill(full_name)?;
            }

            // Re-init db since uninstall_skill saves after each removal
            db = db::init_db()?;
        }
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
        let available_count = get_tap_registry(&db, name)
            .ok()
            .and_then(|opt| opt)
            .map(|registry| registry.skills.len());
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

        // Skip synthetic gist taps — they have no backing repository to update from
        if tap.url.contains("gist.github.com") {
            let count = count_installed_skills(&db, &tap_name);
            println!("  {} {} ({} skills, gist)", "✓".green(), tap_name, count);
            continue;
        }

        print!("  {} Updating {}...", "○".yellow(), tap_name);

        match update_single_tap(&mut db, &tap_name, &tap) {
            Ok(result) => {
                println!("\r  {} {} ({} skills)", "✓".green(), tap_name, result.total);

                if !result.new_skills.is_empty() {
                    println!("    {} new:", "+".green());
                    for skill in &result.new_skills {
                        println!("      {} {}/{}", "+".green(), tap_name, skill);
                    }
                }

                if !result.removed_skills.is_empty() {
                    println!("    {} removed:", "-".red());
                    for skill in &result.removed_skills {
                        println!("      {} {}/{}", "-".red(), tap_name, skill);
                    }
                }

                if !result.removed_installed.is_empty() {
                    println!(
                        "\n    {} {} installed skill(s) no longer in tap:",
                        "!".yellow().bold(),
                        result.removed_installed.len()
                    );
                    for skill in &result.removed_installed {
                        println!("      skillshub uninstall {}/{}", tap_name, skill);
                    }
                }
            }
            Err(e) => {
                println!("\r  {} {} ({})", "✗".red(), tap_name, e);
            }
        }
    }

    db::save_db(&db)?;

    Ok(())
}

/// Result of updating a single tap, describing what changed
struct TapUpdateResult {
    /// Total number of skills in the updated registry
    total: usize,
    /// Skills newly added to the tap since last update
    new_skills: Vec<String>,
    /// Skills removed from the tap since last update
    removed_skills: Vec<String>,
    /// Subset of removed_skills that are currently installed (need user action)
    removed_installed: Vec<String>,
}

/// Update a single tap, refresh cache, and return what changed
fn update_single_tap(db: &mut Database, name: &str, tap: &TapInfo) -> Result<TapUpdateResult> {
    let github_url = parse_github_url(&tap.url)?;
    let new_registry = discover_skills_from_repo(&github_url, name)?;

    // Compare old vs new registries to detect changes
    let old_skills: std::collections::HashSet<&String> = tap
        .cached_registry
        .as_ref()
        .map(|r| r.skills.keys().collect())
        .unwrap_or_default();
    let new_skills_set: std::collections::HashSet<&String> = new_registry.skills.keys().collect();

    let added: Vec<String> = new_skills_set.difference(&old_skills).map(|s| (*s).clone()).collect();
    let removed: Vec<String> = old_skills.difference(&new_skills_set).map(|s| (*s).clone()).collect();

    // Check which removed skills are currently installed
    let removed_installed: Vec<String> = removed
        .iter()
        .filter(|skill_name| {
            let full_name = format!("{}/{}", name, skill_name);
            db.installed.contains_key(&full_name)
        })
        .cloned()
        .collect();

    let total = new_registry.skills.len();

    // Update cache and timestamp in database
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

/// Count installed skills for a given tap
fn count_installed_skills(db: &Database, tap_name: &str) -> usize {
    db::get_skills_from_tap(db, tap_name).len()
}

/// Format installed/available skill counts for display.
///
/// When the installed count exceeds the available count the cache is likely
/// stale (the remote tap has had skills removed since the last `tap update`).
/// In that case we show "?" for the available count rather than an impossible
/// "installed > available" display such as "2/1".
fn format_skills_count(installed: usize, available: Option<usize>) -> String {
    let available_display = match available {
        // Cache is stale or count is inconsistent — show "?" so the user
        // knows to run `skillshub tap update` to refresh the registry.
        Some(count) if installed > count => "?".to_string(),
        Some(count) => count.to_string(),
        None => "?".to_string(),
    };
    format!("{}/{}", installed, available_display)
}

/// Get the registry for a tap (uses cache only, never fetches from remote).
///
/// If the cache is empty, falls back to local bundled skills for the default tap,
/// or returns `None` for non-default taps. Use `tap update` to populate the cache.
pub fn get_tap_registry(db: &Database, tap_name: &str) -> Result<Option<TapRegistry>> {
    let tap = db::get_tap(db, tap_name).with_context(|| format!("Tap '{}' not found", tap_name))?;

    // Return cached registry if available
    if let Some(ref registry) = tap.cached_registry {
        return Ok(Some(registry.clone()));
    }

    // No cache available — use local bundled skills for the default tap,
    // return None for non-default taps (user should run `tap update`)
    if tap.is_default {
        return Ok(Some(generate_local_registry()?));
    }

    Ok(None)
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

/// Import taps from a GitHub star list URL
///
/// Parses the star list URL, fetches all repositories from it, and adds
/// each one as a tap. Skips repos already added as taps.
pub fn import_star_list(url: &str, install: bool) -> Result<()> {
    let (username, list_name) = parse_star_list_url(url)?;

    println!(
        "{} Fetching star list '{}' from user '{}'...",
        "=>".green().bold(),
        list_name,
        username
    );

    let repos = fetch_star_list_repos(&username, &list_name)?;

    if repos.is_empty() {
        println!("  {} No repositories found in star list '{}'", "!".yellow(), list_name);
        return Ok(());
    }

    println!("  {} Found {} repositories", "✓".green(), repos.len());

    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for repo in &repos {
        // Reload DB each iteration since add_tap() modifies it internally
        let db = db::init_db()?;
        if db.taps.contains_key(repo) {
            println!("  {} {} (already added)", "–".dimmed(), repo);
            skipped += 1;
            continue;
        }

        println!();
        match add_tap(repo, install) {
            Ok(()) => {
                added += 1;
            }
            Err(e) => {
                eprintln!("  {} Failed to add {}: {}", "✗".red(), repo, e);
                failed += 1;
            }
        }
    }

    println!();
    println!(
        "{} Star list import complete: {} added, {} skipped, {} failed",
        "=>".green().bold(),
        added,
        skipped,
        failed
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::InstalledSkill;
    use chrono::Utc;
    use serial_test::serial;

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
    fn test_format_skills_count_inconsistent() {
        // installed > available means the cache is stale — show "?" for available
        assert_eq!(format_skills_count(2, Some(1)), "2/?");
        assert_eq!(format_skills_count(17, Some(15)), "17/?");
    }

    #[test]
    fn test_format_skills_count_equal() {
        // installed == available is fine
        assert_eq!(format_skills_count(5, Some(5)), "5/5");
    }

    #[test]
    fn test_format_skills_count_zero_installed() {
        assert_eq!(format_skills_count(0, Some(3)), "0/3");
        assert_eq!(format_skills_count(0, None), "0/?");
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
                gist_updated_at: None,
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
                gist_updated_at: None,
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
                gist_updated_at: None,
            },
        );

        assert_eq!(count_installed_skills(&db, "tap1"), 2);
        assert_eq!(count_installed_skills(&db, "tap2"), 1);
        assert_eq!(count_installed_skills(&db, "missing"), 0);
    }

    /// Helper to build a TapRegistry with the given skill names
    fn make_registry(name: &str, skill_names: &[&str]) -> TapRegistry {
        use crate::registry::models::SkillEntry;
        let mut skills = std::collections::HashMap::new();
        for &s in skill_names {
            skills.insert(
                s.to_string(),
                SkillEntry {
                    path: format!("skills/{}", s),
                    description: Some(format!("{} skill", s)),
                    homepage: None,
                },
            );
        }
        TapRegistry {
            name: name.to_string(),
            description: None,
            skills,
        }
    }

    #[test]
    fn test_tap_update_detects_new_skills() {
        let old_registry = make_registry("test/tap", &["alpha", "beta"]);
        let new_registry = make_registry("test/tap", &["alpha", "beta", "gamma"]);

        let old_keys: std::collections::HashSet<&String> = old_registry.skills.keys().collect();
        let new_keys: std::collections::HashSet<&String> = new_registry.skills.keys().collect();

        let added: Vec<String> = new_keys.difference(&old_keys).map(|s| (*s).clone()).collect();
        let removed: Vec<String> = old_keys.difference(&new_keys).map(|s| (*s).clone()).collect();

        assert_eq!(added, vec!["gamma".to_string()]);
        assert!(removed.is_empty());
    }

    #[test]
    fn test_tap_update_detects_removed_skills() {
        let old_registry = make_registry("test/tap", &["alpha", "beta", "gamma"]);
        let new_registry = make_registry("test/tap", &["alpha"]);

        let old_keys: std::collections::HashSet<&String> = old_registry.skills.keys().collect();
        let new_keys: std::collections::HashSet<&String> = new_registry.skills.keys().collect();

        let added: Vec<String> = new_keys.difference(&old_keys).map(|s| (*s).clone()).collect();
        let mut removed: Vec<String> = old_keys.difference(&new_keys).map(|s| (*s).clone()).collect();
        removed.sort();

        assert!(added.is_empty());
        assert_eq!(removed, vec!["beta".to_string(), "gamma".to_string()]);
    }

    #[test]
    fn test_tap_update_detects_removed_installed_skills() {
        let old_registry = make_registry("test/tap", &["alpha", "beta"]);
        let new_registry = make_registry("test/tap", &["alpha"]);

        let mut db = Database::default();
        db.installed.insert(
            "test/tap/beta".to_string(),
            InstalledSkill {
                tap: "test/tap".to_string(),
                skill: "beta".to_string(),
                commit: None,
                installed_at: Utc::now(),
                source_url: None,
                source_path: None,
                gist_updated_at: None,
            },
        );

        let old_keys: std::collections::HashSet<&String> = old_registry.skills.keys().collect();
        let new_keys: std::collections::HashSet<&String> = new_registry.skills.keys().collect();

        let removed: Vec<String> = old_keys.difference(&new_keys).map(|s| (*s).clone()).collect();
        let removed_installed: Vec<String> = removed
            .iter()
            .filter(|skill_name| {
                let full_name = format!("{}/{}", "test/tap", skill_name);
                db.installed.contains_key(&full_name)
            })
            .cloned()
            .collect();

        assert_eq!(removed, vec!["beta".to_string()]);
        assert_eq!(removed_installed, vec!["beta".to_string()]);
    }

    #[test]
    fn test_tap_update_no_change() {
        let old_registry = make_registry("test/tap", &["alpha", "beta"]);
        let new_registry = make_registry("test/tap", &["alpha", "beta"]);

        let old_keys: std::collections::HashSet<&String> = old_registry.skills.keys().collect();
        let new_keys: std::collections::HashSet<&String> = new_registry.skills.keys().collect();

        let added: Vec<String> = new_keys.difference(&old_keys).map(|s| (*s).clone()).collect();
        let removed: Vec<String> = old_keys.difference(&new_keys).map(|s| (*s).clone()).collect();

        assert!(added.is_empty());
        assert!(removed.is_empty());
    }

    #[test]
    fn test_tap_update_from_empty_cache() {
        // First update (no cached registry) — all skills are "new"
        let new_registry = make_registry("test/tap", &["alpha", "beta"]);

        let old_keys: std::collections::HashSet<&String> = std::collections::HashSet::new();
        let new_keys: std::collections::HashSet<&String> = new_registry.skills.keys().collect();

        let mut added: Vec<String> = new_keys.difference(&old_keys).map(|s| (*s).clone()).collect();
        added.sort();
        let removed: Vec<String> = old_keys.difference(&new_keys).map(|s| (*s).clone()).collect();

        assert_eq!(added, vec!["alpha".to_string(), "beta".to_string()]);
        assert!(removed.is_empty());
    }

    /// RAII guard that restores `SKILLSHUB_TEST_HOME` on drop
    struct TestHomeGuard(Option<String>);

    impl TestHomeGuard {
        fn set(home: &std::path::Path) -> Self {
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

    /// Removing a non-default tap should also uninstall all its installed skills
    #[test]
    #[serial]
    fn test_remove_tap_uninstalls_skills() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        // Create fake ~/.skillshub/skills/test-user/test-repo/skill-{a,b}
        let skillshub_home = home.join(".skillshub");
        let skills_dir = skillshub_home.join("skills");
        let skill_a_dir = skills_dir.join("test-user/test-repo").join("skill-a");
        let skill_b_dir = skills_dir.join("test-user/test-repo").join("skill-b");
        fs::create_dir_all(&skill_a_dir).unwrap();
        fs::create_dir_all(&skill_b_dir).unwrap();

        // Create db.json with the tap and two installed skills
        let db_json = serde_json::json!({
            "taps": {
                "EYH0602/skillshub": {
                    "url": "https://github.com/EYH0602/skillshub",
                    "skills_path": "skills",
                    "updated_at": null,
                    "is_default": true,
                    "cached_registry": null
                },
                "test-user/test-repo": {
                    "url": "https://github.com/test-user/test-repo",
                    "skills_path": "skills",
                    "updated_at": null,
                    "is_default": false,
                    "cached_registry": null
                }
            },
            "installed": {
                "test-user/test-repo/skill-a": {
                    "tap": "test-user/test-repo",
                    "skill": "skill-a",
                    "commit": null,
                    "installed_at": "2026-01-01T00:00:00Z",
                    "source_url": null,
                    "source_path": null,
                    "gist_updated_at": null
                },
                "test-user/test-repo/skill-b": {
                    "tap": "test-user/test-repo",
                    "skill": "skill-b",
                    "commit": null,
                    "installed_at": "2026-01-01T00:00:00Z",
                    "source_url": null,
                    "source_path": null,
                    "gist_updated_at": null
                }
            },
            "linked_agents": [],
            "external": {}
        });
        fs::write(skillshub_home.join("db.json"), db_json.to_string()).unwrap();

        let _guard = TestHomeGuard::set(&home);
        let result = remove_tap("test-user/test-repo", false);

        assert!(result.is_ok(), "remove_tap failed: {:?}", result);

        // Skill directories should be removed
        assert!(!skill_a_dir.exists(), "skill-a dir should be removed");
        assert!(!skill_b_dir.exists(), "skill-b dir should be removed");

        // DB should have no installed skills from this tap and no tap entry
        let db = db::load_db().unwrap();
        assert!(
            db::get_skills_from_tap(&db, "test-user/test-repo").is_empty(),
            "no skills should remain from removed tap"
        );
        assert!(
            db::get_tap(&db, "test-user/test-repo").is_none(),
            "tap should be removed from db"
        );
    }

    /// Removing a tap with no installed skills should still work
    #[test]
    #[serial]
    fn test_remove_tap_no_skills() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        let skillshub_home = home.join(".skillshub");
        let db_json = serde_json::json!({
            "taps": {
                "EYH0602/skillshub": {
                    "url": "https://github.com/EYH0602/skillshub",
                    "skills_path": "skills",
                    "updated_at": null,
                    "is_default": true,
                    "cached_registry": null
                },
                "empty-user/empty-repo": {
                    "url": "https://github.com/empty-user/empty-repo",
                    "skills_path": "skills",
                    "updated_at": null,
                    "is_default": false,
                    "cached_registry": null
                }
            },
            "installed": {},
            "linked_agents": [],
            "external": {}
        });
        fs::create_dir_all(&skillshub_home).unwrap();
        fs::write(skillshub_home.join("db.json"), db_json.to_string()).unwrap();

        let _guard = TestHomeGuard::set(&home);
        let result = remove_tap("empty-user/empty-repo", false);

        assert!(result.is_ok(), "remove_tap failed: {:?}", result);

        let db = db::load_db().unwrap();
        assert!(db::get_tap(&db, "empty-user/empty-repo").is_none());
    }

    /// Removing a tap with --keep-skills should remove the tap but keep skills installed
    #[test]
    #[serial]
    fn test_remove_tap_keep_skills() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        let skillshub_home = home.join(".skillshub");
        let skills_dir = skillshub_home.join("skills");
        let skill_a_dir = skills_dir.join("test-user/test-repo").join("skill-a");
        fs::create_dir_all(&skill_a_dir).unwrap();

        let db_json = serde_json::json!({
            "taps": {
                "EYH0602/skillshub": {
                    "url": "https://github.com/EYH0602/skillshub",
                    "skills_path": "skills",
                    "updated_at": null,
                    "is_default": true,
                    "cached_registry": null
                },
                "test-user/test-repo": {
                    "url": "https://github.com/test-user/test-repo",
                    "skills_path": "skills",
                    "updated_at": null,
                    "is_default": false,
                    "cached_registry": null
                }
            },
            "installed": {
                "test-user/test-repo/skill-a": {
                    "tap": "test-user/test-repo",
                    "skill": "skill-a",
                    "commit": null,
                    "installed_at": "2026-01-01T00:00:00Z",
                    "source_url": null,
                    "source_path": null,
                    "gist_updated_at": null
                }
            },
            "linked_agents": [],
            "external": {}
        });
        fs::write(skillshub_home.join("db.json"), db_json.to_string()).unwrap();

        let _guard = TestHomeGuard::set(&home);
        let result = remove_tap("test-user/test-repo", true);

        assert!(result.is_ok(), "remove_tap failed: {:?}", result);

        // Tap should be removed
        let db = db::load_db().unwrap();
        assert!(db::get_tap(&db, "test-user/test-repo").is_none());

        // Skill should still be installed (files and db entry)
        assert!(skill_a_dir.exists(), "skill-a dir should still exist");
        assert!(
            db::is_skill_installed(&db, "test-user/test-repo/skill-a"),
            "skill-a should still be in db"
        );
    }
}
