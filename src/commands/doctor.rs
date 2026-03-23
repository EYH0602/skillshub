use anyhow::Result;
use colored::Colorize;

use crate::paths::{get_skills_install_dir, get_taps_clone_dir};
use crate::registry::db;
use crate::registry::git;
use crate::registry::models::SkillId;

/// Run diagnostic checks on the skillshub installation.
/// Returns the number of issues found.
pub fn run_doctor() -> Result<usize> {
    println!("{} Running diagnostics...\n", "=>".green().bold());
    let mut issues = 0;

    // 1. Git health
    match git::check_git() {
        Ok(()) => println!("  {} git is installed", "\u{2713}".green()),
        Err(e) => {
            println!("  {} git: {}", "\u{2717}".red(), e);
            issues += 1;
        }
    }

    // 2. Clone health -- for each tap, verify clone dir
    let db = db::load_db()?;
    for (name, tap) in &db.taps {
        if tap.url.contains("gist.github.com") {
            continue;
        }
        let clone_dir = crate::paths::get_tap_clone_dir(name)?;
        if !clone_dir.exists() {
            println!("  {} tap '{}': clone directory missing", "\u{2717}".red(), name);
            issues += 1;
        } else if !clone_dir.join(".git").exists() {
            println!(
                "  {} tap '{}': .git directory missing (corrupted clone)",
                "\u{2717}".red(),
                name
            );
            issues += 1;
        } else {
            // Quick rev-parse check
            match git::git_head_sha(&clone_dir) {
                Ok(_) => println!("  {} tap '{}': clone healthy", "\u{2713}".green(), name),
                Err(_) => {
                    println!("  {} tap '{}': git rev-parse failed", "\u{2717}".red(), name);
                    issues += 1;
                }
            }
        }
    }

    // 3. Skill health -- for each installed skill, check files exist
    let install_dir = get_skills_install_dir()?;
    for (full_name, installed) in &db.installed {
        // Use SkillId::parse or fall back to the InstalledSkill fields directly
        let (tap, skill) = if let Some(id) = SkillId::parse(full_name) {
            (id.tap, id.skill)
        } else {
            (installed.tap.clone(), installed.skill.clone())
        };

        let skill_dir = install_dir.join(&tap).join(&skill);
        if !skill_dir.join("SKILL.md").exists() {
            println!("  {} skill '{}': SKILL.md missing", "\u{2717}".red(), full_name);
            issues += 1;
        } else {
            println!("  {} skill '{}': files present", "\u{2713}".green(), full_name);
        }
    }

    // 4. Orphan detection -- clone dirs with no matching tap
    let taps_dir = get_taps_clone_dir()?;
    if taps_dir.exists() {
        for owner_entry in std::fs::read_dir(&taps_dir)?.flatten() {
            if owner_entry.path().is_dir() {
                for repo_entry in std::fs::read_dir(owner_entry.path())?.flatten() {
                    if !repo_entry.path().is_dir() {
                        continue;
                    }
                    let tap_name = format!(
                        "{}/{}",
                        owner_entry.file_name().to_string_lossy(),
                        repo_entry.file_name().to_string_lossy()
                    );
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
        println!("{} All checks passed!", "\u{2713}".green().bold());
    } else {
        println!("{} {} issue(s) found", "!".yellow().bold(), issues);
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{InstalledSkill, TapInfo};
    use serial_test::serial;
    use std::fs;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    /// RAII guard that restores `SKILLSHUB_TEST_HOME` on drop.
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

    /// Helper: create a minimal db.json at the given skillshub home
    fn write_db_json(skillshub_home: &std::path::Path, db: &crate::registry::models::Database) {
        let db_path = skillshub_home.join("db.json");
        let content = serde_json::to_string_pretty(db).unwrap();
        fs::write(db_path, content).unwrap();
    }

    /// Helper: create a local git repo with one commit, return its path.
    fn create_local_repo(dir: &std::path::Path) -> std::path::PathBuf {
        let repo = dir.to_path_buf();
        fs::create_dir_all(&repo).unwrap();

        StdCommand::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&repo)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo)
            .output()
            .unwrap();

        fs::write(repo.join("README.md"), "# Test Repo\n").unwrap();

        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(&repo)
            .output()
            .unwrap();

        repo
    }

    #[test]
    #[serial]
    fn test_doctor_no_taps() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let skillshub_home = home.join(".skillshub");
        fs::create_dir_all(&skillshub_home).unwrap();

        // Empty database (no taps, no installed skills)
        let db = crate::registry::models::Database::default();
        write_db_json(&skillshub_home, &db);

        let _guard = TestHomeGuard::set(&home);
        let issues = run_doctor().unwrap();
        assert_eq!(issues, 0, "empty db should report zero issues");
    }

    #[test]
    #[serial]
    fn test_doctor_healthy_clone() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let skillshub_home = home.join(".skillshub");
        fs::create_dir_all(&skillshub_home).unwrap();

        // Create a tap entry pointing to a healthy clone
        let mut db = crate::registry::models::Database::default();
        db.taps.insert(
            "owner/repo".to_string(),
            TapInfo {
                url: "https://github.com/owner/repo".to_string(),
                skills_path: "skills".to_string(),
                updated_at: None,
                is_default: false,
                cached_registry: None,
                branch: None,
            },
        );
        write_db_json(&skillshub_home, &db);

        // Create a valid git clone at taps/owner/repo
        let clone_dir = skillshub_home.join("taps").join("owner").join("repo");
        create_local_repo(&clone_dir);

        let _guard = TestHomeGuard::set(&home);
        let issues = run_doctor().unwrap();
        assert_eq!(issues, 0, "healthy clone should report zero issues");
    }

    #[test]
    #[serial]
    fn test_doctor_missing_clone() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let skillshub_home = home.join(".skillshub");
        fs::create_dir_all(&skillshub_home).unwrap();

        // Create a tap entry but no actual clone directory
        let mut db = crate::registry::models::Database::default();
        db.taps.insert(
            "owner/repo".to_string(),
            TapInfo {
                url: "https://github.com/owner/repo".to_string(),
                skills_path: "skills".to_string(),
                updated_at: None,
                is_default: false,
                cached_registry: None,
                branch: None,
            },
        );
        write_db_json(&skillshub_home, &db);

        let _guard = TestHomeGuard::set(&home);
        let issues = run_doctor().unwrap();
        // Missing clone directory should be reported as an issue
        assert!(issues >= 1, "missing clone should report at least 1 issue");
    }

    #[test]
    #[serial]
    fn test_doctor_missing_skill_files() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let skillshub_home = home.join(".skillshub");
        fs::create_dir_all(&skillshub_home).unwrap();

        // Create an installed skill entry but no SKILL.md on disk
        let mut db = crate::registry::models::Database::default();
        db.installed.insert(
            "owner/repo/my-skill".to_string(),
            InstalledSkill {
                tap: "owner/repo".to_string(),
                skill: "my-skill".to_string(),
                commit: None,
                installed_at: chrono::Utc::now(),
                source_url: None,
                source_path: None,
                gist_updated_at: None,
            },
        );
        write_db_json(&skillshub_home, &db);

        // Create the skill directory but WITHOUT SKILL.md
        let skill_dir = skillshub_home.join("skills").join("owner/repo").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();

        let _guard = TestHomeGuard::set(&home);
        let issues = run_doctor().unwrap();
        assert!(issues >= 1, "missing SKILL.md should report at least 1 issue");
    }

    #[test]
    #[serial]
    fn test_doctor_orphan_clone() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let skillshub_home = home.join(".skillshub");
        fs::create_dir_all(&skillshub_home).unwrap();

        // Empty database (no taps)
        let db = crate::registry::models::Database::default();
        write_db_json(&skillshub_home, &db);

        // Create a clone directory that has no matching tap in the db
        let orphan_dir = skillshub_home.join("taps").join("orphan-owner").join("orphan-repo");
        fs::create_dir_all(&orphan_dir).unwrap();

        let _guard = TestHomeGuard::set(&home);
        let issues = run_doctor().unwrap();
        assert!(issues >= 1, "orphan clone should report at least 1 issue");
    }
}
