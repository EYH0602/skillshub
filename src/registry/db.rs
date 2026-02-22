use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::models::{Database, ExternalSkill, InstalledSkill, TapInfo};
use crate::paths::get_skillshub_home;

/// Default tap name for bundled skills (owner/repo format)
pub const DEFAULT_TAP_NAME: &str = "EYH0602/skillshub";

/// Default tap URL (this repository)
pub const DEFAULT_TAP_URL: &str = "https://github.com/EYH0602/skillshub";

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

    let content =
        fs::read_to_string(&db_path).with_context(|| format!("Failed to read database at {}", db_path.display()))?;

    let mut db: Database =
        serde_json::from_str(&content).with_context(|| format!("Failed to parse database at {}", db_path.display()))?;

    if normalize_default_taps(&mut db) {
        // Persist the fix so the corrupt state is not re-applied on every load
        let _ = save_db(&db);
    }

    Ok(db)
}

/// Ensure exactly one tap is marked as default.
///
/// If multiple taps have `is_default = true`, only the canonical default tap
/// (`EYH0602/skillshub`) keeps the flag. If that tap is not present, the
/// lexicographically-first tap with `is_default = true` is kept; all others are cleared.
///
/// Returns `true` if any tap flags were changed, `false` if no changes were needed.
fn normalize_default_taps(db: &mut Database) -> bool {
    let default_count = db.taps.values().filter(|t| t.is_default).count();

    // Nothing to fix if zero or one default
    if default_count <= 1 {
        return false;
    }

    // More than one default: determine which tap should be THE default.
    // Prefer the canonical DEFAULT_TAP_NAME; otherwise keep the lexicographically-first one.
    let canonical_is_present = db.taps.get(DEFAULT_TAP_NAME).map(|t| t.is_default).unwrap_or(false);

    if canonical_is_present {
        // Clear every default tap that isn't the canonical one
        let to_clear: Vec<String> = db
            .taps
            .iter()
            .filter(|(name, tap)| tap.is_default && name.as_str() != DEFAULT_TAP_NAME)
            .map(|(name, _)| name.clone())
            .collect();
        for name in to_clear {
            if let Some(tap) = db.taps.get_mut(&name) {
                tap.is_default = false;
            }
        }
    } else {
        // No canonical tap — keep the lexicographically-first default tap
        let mut default_names: Vec<String> = db
            .taps
            .iter()
            .filter(|(_, tap)| tap.is_default)
            .map(|(name, _)| name.clone())
            .collect();
        default_names.sort();

        // Clear all but the first
        for name in default_names.into_iter().skip(1) {
            if let Some(tap) = db.taps.get_mut(&name) {
                tap.is_default = false;
            }
        }
    }

    true
}

/// Save the database to disk
pub fn save_db(db: &Database) -> Result<()> {
    let db_path = get_db_path()?;

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(db)?;
    fs::write(&db_path, content).with_context(|| format!("Failed to write database to {}", db_path.display()))?;

    Ok(())
}

fn default_taps() -> Vec<(&'static str, TapInfo)> {
    vec![(
        DEFAULT_TAP_NAME,
        TapInfo {
            url: DEFAULT_TAP_URL.to_string(),
            skills_path: "skills".to_string(),
            updated_at: None,
            is_default: true,
            cached_registry: None,
        },
    )]
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
pub fn get_skills_from_tap<'a>(db: &'a Database, tap_name: &str) -> Vec<(&'a String, &'a InstalledSkill)> {
    db.installed.iter().filter(|(_, skill)| skill.tap == tap_name).collect()
}

/// Check if a skill is tracked as external
pub fn is_external_skill(db: &Database, name: &str) -> bool {
    db.external.contains_key(name)
}

/// Get external skill info
#[allow(dead_code)]
pub fn get_external_skill<'a>(db: &'a Database, name: &str) -> Option<&'a ExternalSkill> {
    db.external.get(name)
}

/// Add an external skill to the database
pub fn add_external_skill(db: &mut Database, name: &str, skill: ExternalSkill) {
    db.external.insert(name.to_string(), skill);
}

/// Remove an external skill from the database
pub fn remove_external_skill(db: &mut Database, name: &str) -> Option<ExternalSkill> {
    db.external.remove(name)
}

/// Get all external skills
pub fn get_all_external_skills(db: &Database) -> Vec<(&String, &ExternalSkill)> {
    db.external.iter().collect()
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
        assert_eq!(db.taps.len(), 1);

        let default_tap = db.taps.get(DEFAULT_TAP_NAME).unwrap();
        assert!(default_tap.is_default);

        // Calling again should return false (no changes)
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
            cached_registry: None,
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
            source_url: None,
            source_path: None,
        };
        let skill2 = InstalledSkill {
            tap: "tap1".to_string(),
            skill: "skill2".to_string(),
            commit: None,
            installed_at: Utc::now(),
            source_url: None,
            source_path: None,
        };
        let skill3 = InstalledSkill {
            tap: "tap2".to_string(),
            skill: "skill3".to_string(),
            commit: None,
            installed_at: Utc::now(),
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

    #[test]
    fn test_external_skill_operations() {
        let mut db = Database::default();
        assert!(!is_external_skill(&db, "my-external-skill"));

        let external = ExternalSkill {
            name: "my-external-skill".to_string(),
            source_agent: ".claude".to_string(),
            source_path: PathBuf::from("/home/user/.claude/skills/my-external-skill"),
            discovered_at: Utc::now(),
        };

        add_external_skill(&mut db, "my-external-skill", external);
        assert!(is_external_skill(&db, "my-external-skill"));

        let retrieved = get_external_skill(&db, "my-external-skill");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().source_agent, ".claude");

        let all_external = get_all_external_skills(&db);
        assert_eq!(all_external.len(), 1);

        let removed = remove_external_skill(&mut db, "my-external-skill");
        assert!(removed.is_some());
        assert!(!is_external_skill(&db, "my-external-skill"));
    }

    fn make_tap(is_default: bool) -> TapInfo {
        TapInfo {
            url: "https://github.com/user/repo".to_string(),
            skills_path: "skills".to_string(),
            updated_at: None,
            is_default,
            cached_registry: None,
        }
    }

    #[test]
    fn test_normalize_default_taps_single_default() {
        let mut db = Database::default();
        db.taps.insert(DEFAULT_TAP_NAME.to_string(), make_tap(true));
        db.taps.insert("other/tap".to_string(), make_tap(false));

        normalize_default_taps(&mut db);

        // Should remain unchanged
        assert!(db.taps[DEFAULT_TAP_NAME].is_default);
        assert!(!db.taps["other/tap"].is_default);
    }

    #[test]
    fn test_normalize_default_taps_multiple_defaults_with_canonical() {
        let mut db = Database::default();
        db.taps.insert(DEFAULT_TAP_NAME.to_string(), make_tap(true));
        db.taps.insert("anthropics/skills".to_string(), make_tap(true));
        db.taps.insert("other/tap".to_string(), make_tap(false));

        normalize_default_taps(&mut db);

        // Only the canonical default tap should remain marked as default
        assert!(db.taps[DEFAULT_TAP_NAME].is_default);
        assert!(!db.taps["anthropics/skills"].is_default);
        assert!(!db.taps["other/tap"].is_default);
    }

    #[test]
    fn test_normalize_default_taps_multiple_defaults_without_canonical() {
        let mut db = Database::default();
        db.taps.insert("alpha/tap".to_string(), make_tap(true));
        db.taps.insert("beta/tap".to_string(), make_tap(true));
        db.taps.insert("gamma/tap".to_string(), make_tap(true));

        normalize_default_taps(&mut db);

        // Exactly one tap should be default (lexicographically first: "alpha/tap")
        let defaults: Vec<&str> = db
            .taps
            .iter()
            .filter(|(_, t)| t.is_default)
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(defaults.len(), 1);
        assert!(db.taps["alpha/tap"].is_default);
        assert!(!db.taps["beta/tap"].is_default);
        assert!(!db.taps["gamma/tap"].is_default);
    }

    #[test]
    fn test_normalize_default_taps_no_defaults() {
        let mut db = Database::default();
        db.taps.insert("user/tap".to_string(), make_tap(false));

        normalize_default_taps(&mut db);

        // Nothing changes when there are zero defaults
        assert!(!db.taps["user/tap"].is_default);
    }
}
