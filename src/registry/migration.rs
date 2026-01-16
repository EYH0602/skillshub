use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use std::fs;

use super::db::{self, DEFAULT_TAP_NAME};
use super::models::InstalledSkill;
use crate::paths::get_skills_install_dir;
use crate::skill::discover_skills;

/// Migrate old-style installations to the new registry format
///
/// Old format: ~/.skillshub/skills/<skill-name>/
/// New format: ~/.skillshub/skills/<tap-name>/<skill-name>/
///
/// This function:
/// 1. Detects old-style installations (skills directly in skills/)
/// 2. Moves them to skillshub/<skill-name>/
/// 3. Records them in the database
pub fn migrate_old_installations() -> Result<()> {
    let install_dir = get_skills_install_dir()?;

    if !install_dir.exists() {
        return Ok(());
    }

    // Discover skills directly in the install directory (old format)
    let old_skills = discover_skills(&install_dir)?;

    if old_skills.is_empty() {
        return Ok(());
    }

    println!(
        "{} Found {} old-style installation(s), migrating...",
        "=>".green().bold(),
        old_skills.len()
    );

    let mut db = db::init_db()?;

    // Create the new tap directory
    let new_tap_dir = install_dir.join(DEFAULT_TAP_NAME);
    fs::create_dir_all(&new_tap_dir)?;

    for skill in old_skills {
        let old_path = &skill.path;
        let new_path = new_tap_dir.join(&skill.name);
        let full_name = format!("{}/{}", DEFAULT_TAP_NAME, skill.name);

        // Skip if already migrated or in new format
        if old_path.parent() == Some(&new_tap_dir) {
            continue;
        }

        // Tap directories already follow the new layout and should not be moved.
        if is_tap_directory(old_path) {
            continue;
        }

        // Move the skill to the new location
        if new_path.exists() {
            println!("  {} {} (already exists at new location)", "○".yellow(), skill.name);
            // Remove old location
            fs::remove_dir_all(old_path)?;
        } else {
            fs::rename(old_path, &new_path)?;
            println!("  {} {} (migrated)", "✓".green(), skill.name);
        }

        // Record in database if not already there
        if !db::is_skill_installed(&db, &full_name) {
            let installed = InstalledSkill {
                tap: DEFAULT_TAP_NAME.to_string(),
                skill: skill.name.clone(),
                commit: None,
                installed_at: Utc::now(),
                local: true,
                source_url: None,
                source_path: None,
            };
            db::add_installed_skill(&mut db, &full_name, installed);
        }
    }

    db::save_db(&db)?;

    println!("{} Migration complete!", "Done!".green().bold());

    Ok(())
}

/// Check if a directory is a tap directory (contains skill subdirectories)
fn is_tap_directory(path: &std::path::Path) -> bool {
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                // Check if this subdirectory has a SKILL.md
                if entry_path.join("SKILL.md").exists() {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if migration is needed
pub fn needs_migration() -> Result<bool> {
    let install_dir = get_skills_install_dir()?;

    if !install_dir.exists() {
        return Ok(false);
    }

    // Check for old-style skills (SKILL.md directly in a subdirectory)
    if let Ok(entries) = fs::read_dir(&install_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // If it has SKILL.md directly, it's old format
                if path.join("SKILL.md").exists() {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_tap_directory_empty() {
        let dir = TempDir::new().unwrap();
        assert!(!is_tap_directory(dir.path()));
    }

    #[test]
    fn test_is_tap_directory_with_skill() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: test\n---").unwrap();

        assert!(is_tap_directory(dir.path()));
    }

    #[test]
    fn test_is_tap_directory_without_skill() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("some-dir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("README.md"), "hello").unwrap();

        assert!(!is_tap_directory(dir.path()));
    }
}
