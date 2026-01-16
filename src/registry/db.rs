use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::models::{Database, InstalledSkill, TapInfo};
use crate::paths::get_skillshub_home;

/// Default tap name for bundled skills
pub const DEFAULT_TAP_NAME: &str = "skillshub";

/// Default tap URL (this repository)
pub const DEFAULT_TAP_URL: &str = "https://github.com/yfhe/skillshub";

/// Default tap name for Anthropics skills
pub const ANTHROPIC_TAP_NAME: &str = "anthropics";

/// Default tap URL for Anthropics skills (pinned commit)
pub const ANTHROPIC_TAP_URL: &str =
    "https://github.com/anthropics/skills/tree/69c0b1a0674149f27b61b2635f935524b6add202";

/// Get the path to the database file (~/.skillshub/db.json)
pub fn get_db_path() -> Result<PathBuf> {
    Ok(get_skillshub_home()?.join("db.json"))
}

/// Load the database from disk, or return a default if it doesn't exist
pub fn load_db() -> Result<Database> {
    let db_path = get_db_path()?;

    if !db_path.exists() {
        return Ok(Database::default());
    }

    let content = fs::read_to_string(&db_path)
        .with_context(|| format!("Failed to read database at {}", db_path.display()))?;

    let db: Database = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse database at {}", db_path.display()))?;

    Ok(db)
}

/// Save the database to disk
pub fn save_db(db: &Database) -> Result<()> {
    let db_path = get_db_path()?;

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(db)?;
    fs::write(&db_path, content)
        .with_context(|| format!("Failed to write database to {}", db_path.display()))?;

    Ok(())
}

fn default_taps() -> Vec<(&'static str, TapInfo)> {
    vec![
        (
            DEFAULT_TAP_NAME,
            TapInfo {
                url: DEFAULT_TAP_URL.to_string(),
                skills_path: "skills".to_string(),
                updated_at: None,
                is_default: true,
                is_bundled: true,
            },
        ),
        (
            ANTHROPIC_TAP_NAME,
            TapInfo {
                url: ANTHROPIC_TAP_URL.to_string(),
                skills_path: "skills".to_string(),
                updated_at: None,
                is_default: true,
                is_bundled: false,
            },
        ),
    ]
}

fn ensure_default_taps(db: &mut Database) -> bool {
    let mut changed = false;

    for (name, tap) in default_taps() {
        if !db.taps.contains_key(name) {
            db.taps.insert(name.to_string(), tap);
            changed = true;
        }
    }

    changed
}

/// Initialize the database with the default tap if it doesn't exist
pub fn init_db() -> Result<Database> {
    let mut db = load_db()?;

    if ensure_default_taps(&mut db) {
        save_db(&db)?;
    }

    Ok(db)
}

/// Check if a skill is installed
pub fn is_skill_installed(db: &Database, full_name: &str) -> bool {
    db.installed.contains_key(full_name)
}

/// Get installed skill info
pub fn get_installed_skill<'a>(db: &'a Database, full_name: &str) -> Option<&'a InstalledSkill> {
    db.installed.get(full_name)
}

/// Add an installed skill to the database
pub fn add_installed_skill(db: &mut Database, full_name: &str, skill: InstalledSkill) {
    db.installed.insert(full_name.to_string(), skill);
}

/// Remove an installed skill from the database
pub fn remove_installed_skill(db: &mut Database, full_name: &str) -> Option<InstalledSkill> {
    db.installed.remove(full_name)
}

/// Get tap info by name
pub fn get_tap<'a>(db: &'a Database, name: &str) -> Option<&'a TapInfo> {
    db.taps.get(name)
}

/// Add a tap to the database
pub fn add_tap(db: &mut Database, name: &str, tap: TapInfo) {
    db.taps.insert(name.to_string(), tap);
}

/// Remove a tap from the database
pub fn remove_tap(db: &mut Database, name: &str) -> Option<TapInfo> {
    db.taps.remove(name)
}

/// Get all skills installed from a specific tap
pub fn get_skills_from_tap<'a>(
    db: &'a Database,
    tap_name: &str,
) -> Vec<(&'a String, &'a InstalledSkill)> {
    db.installed
        .iter()
        .filter(|(_, skill)| skill.tap == tap_name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_database_default_has_no_taps() {
        let db = Database::default();
        assert!(db.taps.is_empty());
        assert!(db.installed.is_empty());
    }

    #[test]
    fn test_ensure_default_taps() {
        let mut db = Database::default();
        assert!(ensure_default_taps(&mut db));
        assert!(db.taps.contains_key(DEFAULT_TAP_NAME));
        assert!(db.taps.contains_key(ANTHROPIC_TAP_NAME));

        let bundled = db.taps.get(DEFAULT_TAP_NAME).unwrap();
        assert!(bundled.is_default);
        assert!(bundled.is_bundled);

        let anthropic = db.taps.get(ANTHROPIC_TAP_NAME).unwrap();
        assert!(anthropic.is_default);
        assert!(!anthropic.is_bundled);

        assert!(!ensure_default_taps(&mut db));
    }

    #[test]
    fn test_is_skill_installed() {
        let mut db = Database::default();
        assert!(!is_skill_installed(&db, "tap/skill"));

        db.installed.insert(
            "tap/skill".to_string(),
            InstalledSkill {
                tap: "tap".to_string(),
                skill: "skill".to_string(),
                commit: None,
                installed_at: Utc::now(),
                local: false,
                source_url: None,
                source_path: None,
            },
        );

        assert!(is_skill_installed(&db, "tap/skill"));
    }

    #[test]
    fn test_add_and_remove_skill() {
        let mut db = Database::default();

        let skill = InstalledSkill {
            tap: "tap".to_string(),
            skill: "skill".to_string(),
            commit: Some("abc123".to_string()),
            installed_at: Utc::now(),
            local: false,
            source_url: None,
            source_path: None,
        };

        add_installed_skill(&mut db, "tap/skill", skill);
        assert!(is_skill_installed(&db, "tap/skill"));

        let removed = remove_installed_skill(&mut db, "tap/skill");
        assert!(removed.is_some());
        assert!(!is_skill_installed(&db, "tap/skill"));
    }

    #[test]
    fn test_add_and_remove_tap() {
        let mut db = Database::default();

        let tap = TapInfo {
            url: "https://github.com/user/repo".to_string(),
            skills_path: "skills".to_string(),
            updated_at: None,
            is_default: false,
            is_bundled: false,
        };

        add_tap(&mut db, "my-tap", tap);
        assert!(get_tap(&db, "my-tap").is_some());

        let removed = remove_tap(&mut db, "my-tap");
        assert!(removed.is_some());
        assert!(get_tap(&db, "my-tap").is_none());
    }

    #[test]
    fn test_get_skills_from_tap() {
        let mut db = Database::default();

        let skill1 = InstalledSkill {
            tap: "tap1".to_string(),
            skill: "skill1".to_string(),
            commit: None,
            installed_at: Utc::now(),
            local: false,
            source_url: None,
            source_path: None,
        };
        let skill2 = InstalledSkill {
            tap: "tap1".to_string(),
            skill: "skill2".to_string(),
            commit: None,
            installed_at: Utc::now(),
            local: false,
            source_url: None,
            source_path: None,
        };
        let skill3 = InstalledSkill {
            tap: "tap2".to_string(),
            skill: "skill3".to_string(),
            commit: None,
            installed_at: Utc::now(),
            local: false,
            source_url: None,
            source_path: None,
        };

        add_installed_skill(&mut db, "tap1/skill1", skill1);
        add_installed_skill(&mut db, "tap1/skill2", skill2);
        add_installed_skill(&mut db, "tap2/skill3", skill3);

        let tap1_skills = get_skills_from_tap(&db, "tap1");
        assert_eq!(tap1_skills.len(), 2);

        let tap2_skills = get_skills_from_tap(&db, "tap2");
        assert_eq!(tap2_skills.len(), 1);
    }
}
