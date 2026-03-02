use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::agent::{discover_agents, AgentInfo};
use crate::paths::{display_path_with_tilde, get_home_dir, get_skills_install_dir, get_skillshub_home};
use crate::registry::db::{get_db_path, init_db, save_db};

/// Clear cached registry data from all taps
pub fn clean_cache() -> Result<()> {
    let mut db = init_db()?;
    let mut cleared_count = 0;

    for (name, tap) in db.taps.iter_mut() {
        if tap.cached_registry.is_some() {
            tap.cached_registry = None;
            cleared_count += 1;
            println!("  {} Cleared cache for {}", "✓".green(), name);
        }
    }

    if cleared_count > 0 {
        save_db(&db)?;
        println!(
            "\n{} Cleared cache from {} tap(s)",
            "Done!".green().bold(),
            cleared_count
        );
    } else {
        println!("{} No cached data to clear", "Info:".cyan());
    }

    Ok(())
}

/// Remove all skillshub-managed symlinks from all detected agent directories.
/// Returns the total number of symlinks removed.
fn remove_managed_symlinks(agents: &[AgentInfo], skills_dir_canonical: &Path) -> usize {
    let mut total_removed = 0;

    for agent in agents {
        let agent_name = agent
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| agent.path.display().to_string());
        let skills_path = agent.path.join(agent.skills_subdir);

        if !skills_path.exists() {
            continue;
        }

        let mut removed_count = 0;

        // Scan entries in the agent's skills directory
        if let Ok(entries) = fs::read_dir(&skills_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Only process symlinks
                if !path.is_symlink() {
                    continue;
                }

                // Check if symlink points to skillshub-managed directory
                if is_skillshub_managed_link(&path, skills_dir_canonical) {
                    if let Err(e) = fs::remove_file(&path) {
                        eprintln!("  {} Failed to remove {}: {}", "!".red(), path.display(), e);
                    } else {
                        removed_count += 1;
                    }
                }
            }
        }

        if removed_count > 0 {
            println!("  {} {} (removed {} link(s))", "✓".green(), agent_name, removed_count);
            total_removed += removed_count;
        }
    }

    total_removed
}

/// Remove all skillshub-managed symlinks from agent directories
/// If remove_skills is true, also delete all installed skills
pub fn clean_links(remove_skills: bool) -> Result<()> {
    let mut db = init_db()?;
    let skills_dir = get_skills_install_dir()?;
    let skills_dir_canonical = skills_dir.canonicalize().unwrap_or_else(|_| skills_dir.clone());

    let agents = discover_agents();

    if agents.is_empty() {
        println!("{} No coding agents found", "Info:".cyan());
        return Ok(());
    }

    println!(
        "{} Removing skillshub-managed symlinks from {} agent(s)",
        "=>".green().bold(),
        agents.len()
    );

    let total_removed = remove_managed_symlinks(&agents, &skills_dir_canonical);

    // Clear linked_agents from database
    db.linked_agents.clear();

    if remove_skills {
        // Also remove all installed skills
        println!("\n{} Removing installed skills", "=>".green().bold());

        if skills_dir.exists() {
            let skill_count = db.installed.len();
            fs::remove_dir_all(&skills_dir)?;
            println!(
                "  {} Removed {} ({})",
                "✓".green(),
                display_path_with_tilde(&skills_dir),
                if skill_count > 0 {
                    format!("{} skill(s)", skill_count)
                } else {
                    "empty".to_string()
                }
            );

            // Clear installed skills from database
            db.installed.clear();
        } else {
            println!("  {} No installed skills to remove", "Info:".cyan());
        }
    }

    save_db(&db)?;

    if remove_skills {
        println!(
            "\n{} Removed {} link(s) and all installed skills",
            "Done!".green().bold(),
            total_removed
        );
    } else if total_removed > 0 {
        println!("\n{} Removed {} link(s)", "Done!".green().bold(), total_removed);
        println!(
            "{} Skills are still installed at {}. Use --remove-skills to delete them.",
            "Note:".cyan(),
            display_path_with_tilde(&skills_dir)
        );
    } else {
        println!("\n{} No skillshub-managed links to remove", "Info:".cyan());
    }

    Ok(())
}

/// Completely remove all skillshub-managed state (full uninstall/purge).
/// Removes all managed symlinks from agent directories, then deletes ~/.skillshub/ entirely.
/// If confirm is false, prints a summary and prompts the user to type 'yes' before proceeding.
pub fn clean_all(confirm: bool) -> Result<()> {
    clean_all_with_input(confirm, &mut io::stdin().lock())
}

/// Inner implementation that accepts a reader, enabling tests to supply mock input.
fn clean_all_with_input(confirm: bool, input: &mut impl BufRead) -> Result<()> {
    let skillshub_home = get_skillshub_home()?;
    let skills_dir = get_skills_install_dir()?;
    let db_path = get_db_path()?;
    let agents = discover_agents();

    // --- Interactive confirmation (only when --confirm is NOT passed) ---
    if !confirm {
        println!(
            "{}",
            "WARNING: This will completely remove skillshub from your system."
                .yellow()
                .bold()
        );
        println!();
        println!("{} The following will be deleted:", "=>".green().bold());
        println!(
            "  - All skillshub-managed symlinks from {} detected agent(s)",
            agents.len()
        );
        for agent in &agents {
            let agent_name = agent
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| agent.path.display().to_string());
            let skills_path = agent.path.join(agent.skills_subdir);
            println!("      {} ({})", agent_name, display_path_with_tilde(&skills_path));
        }
        println!("  - Installed skills: {}", display_path_with_tilde(&skills_dir));
        println!("  - Database: {}", display_path_with_tilde(&db_path));
        println!(
            "  - Skillshub home directory: {}",
            display_path_with_tilde(&skillshub_home)
        );

        println!();
        print!("Confirm: Type 'yes' to confirm: ");
        io::stdout().flush()?;

        let mut user_input = String::new();
        input.read_line(&mut user_input)?;
        let trimmed = user_input.trim();

        if trimmed != "yes" {
            println!("{}", "Cancelled. Nothing was removed.".yellow());
            return Ok(());
        }
    }

    println!();
    println!("{} Starting full uninstall...", "=>".green().bold());

    // --- Remove symlinks ---
    // Derive canonical skills path from the home directory (which should exist)
    // rather than canonicalizing the skills dir itself, which may not exist in a
    // partially-cleaned state.
    let home = get_home_dir().context("Could not determine home directory")?;
    let home_canonical = home.canonicalize().unwrap_or_else(|_| home.clone());
    let skills_dir_canonical = home_canonical.join(".skillshub").join("skills");

    println!("  {} Removing skillshub-managed symlinks...", "=>".green().bold());
    let total_removed = remove_managed_symlinks(&agents, &skills_dir_canonical);
    println!("  {} Removed {} symlink(s) total", "✓".green(), total_removed);

    // --- Save a clean database before destructive deletion ---
    // This keeps db.json consistent with the filesystem if remove_dir_all fails
    // partway (e.g. permission error).
    if let Ok(mut db) = init_db() {
        db.linked_agents.clear();
        db.installed.clear();
        let _ = save_db(&db);
    }

    // --- Remove ~/.skillshub/ directory entirely ---
    println!(
        "  {} Removing {} ...",
        "=>".green().bold(),
        display_path_with_tilde(&skillshub_home)
    );

    if skillshub_home.exists() {
        fs::remove_dir_all(&skillshub_home)?;
        println!("  {} Removed {}", "✓".green(), display_path_with_tilde(&skillshub_home));
    } else {
        println!(
            "  {} {} does not exist, nothing to remove",
            "Info:".cyan(),
            display_path_with_tilde(&skillshub_home)
        );
    }

    println!();
    println!(
        "{} Skillshub has been completely removed from your system.",
        "Done!".green().bold()
    );

    Ok(())
}

/// Check if a symlink points to a skillshub-managed directory
fn is_skillshub_managed_link(link_path: &Path, skillshub_skills_dir: &Path) -> bool {
    if let Ok(target) = fs::read_link(link_path) {
        // Resolve the target path (handle relative symlinks)
        let resolved = if target.is_absolute() {
            target
        } else {
            link_path.parent().map(|p| p.join(&target)).unwrap_or(target)
        };

        // Canonicalize to resolve any ../ components
        let resolved = resolved.canonicalize().unwrap_or(resolved);

        // Check if target starts with skillshub skills directory
        resolved.starts_with(skillshub_skills_dir)
    } else {
        // Broken symlink - check if the raw target path looks like skillshub
        if let Ok(target) = fs::read_link(link_path) {
            let target_str = target.to_string_lossy();
            target_str.contains(".skillshub/skills") || target_str.contains(".skillshub\\skills")
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    /// RAII guard that restores `SKILLSHUB_TEST_HOME` on drop, even if the test
    /// panics between `set_test_home` and cleanup.
    struct TestHomeGuard(Option<String>);

    impl TestHomeGuard {
        /// Set `SKILLSHUB_TEST_HOME` to `home` and capture the previous value.
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

    // ---------------------------------------------------------------------------
    // clean_all tests
    // ---------------------------------------------------------------------------

    /// `clean_all(true)` with --confirm removes managed symlinks and deletes the
    /// skillshub home directory.
    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_clean_all_confirm_removes_symlinks_and_skillshub_home() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        // Create fake ~/.skillshub/skills/tap/skill
        let skillshub_home = home.join(".skillshub");
        let skills_dir = skillshub_home.join("skills");
        let skill_dir = skills_dir.join("tap").join("skill");
        fs::create_dir_all(&skill_dir).unwrap();

        // Create a minimal db.json so init_db() doesn't fail
        fs::write(
            skillshub_home.join("db.json"),
            r#"{"taps":{},"installed":{},"linked_agents":[],"external":{}}"#,
        )
        .unwrap();

        // Create fake ~/.claude/skills with a managed symlink
        let claude_skills = home.join(".claude").join("skills");
        fs::create_dir_all(&claude_skills).unwrap();
        let link_path = claude_skills.join("skill");
        std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();
        assert!(link_path.is_symlink());

        let _guard = TestHomeGuard::set(&home);
        let result = clean_all(true);

        assert!(result.is_ok(), "clean_all returned error: {:?}", result);

        // The managed symlink should be gone
        assert!(
            !link_path.exists() && !link_path.is_symlink(),
            "managed symlink should be removed"
        );

        // The skillshub home directory should be deleted
        assert!(!skillshub_home.exists(), "skillshub home should be deleted");
    }

    /// `clean_all(true)` gracefully handles a missing `~/.skillshub/` directory
    /// (should not error out).
    #[test]
    #[serial]
    fn test_clean_all_missing_skillshub_home_does_not_error() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        // Do NOT create ~/.skillshub at all; only create the home directory itself
        fs::create_dir_all(&home).unwrap();

        let _guard = TestHomeGuard::set(&home);
        let result = clean_all(true);

        assert!(
            result.is_ok(),
            "clean_all should not error when skillshub home is missing: {:?}",
            result
        );
    }

    /// Symlinks that point to non-skillshub targets are preserved by `clean_all`.
    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_clean_all_preserves_non_skillshub_symlinks() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        // Create fake ~/.skillshub/skills
        let skillshub_home = home.join(".skillshub");
        let skills_dir = skillshub_home.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create a minimal db.json
        fs::write(
            skillshub_home.join("db.json"),
            r#"{"taps":{},"installed":{},"linked_agents":[],"external":{}}"#,
        )
        .unwrap();

        // Create an external (non-skillshub) skill directory
        let external_skill = temp.path().join("external").join("my-skill");
        fs::create_dir_all(&external_skill).unwrap();

        // Create fake ~/.claude/skills with a symlink to the external skill
        let claude_skills = home.join(".claude").join("skills");
        fs::create_dir_all(&claude_skills).unwrap();
        let link_path = claude_skills.join("my-skill");
        std::os::unix::fs::symlink(&external_skill, &link_path).unwrap();
        assert!(link_path.is_symlink());

        let _guard = TestHomeGuard::set(&home);
        let result = clean_all(true);

        assert!(result.is_ok(), "clean_all returned error: {:?}", result);

        // The external symlink should still be present
        assert!(
            link_path.is_symlink(),
            "external symlink should NOT be removed by clean_all"
        );
    }

    // ---------------------------------------------------------------------------
    // Interactive confirmation tests (clean_all_with_input)
    // ---------------------------------------------------------------------------

    /// Non-`yes` input cancels the operation and leaves state untouched.
    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_clean_all_interactive_cancel_leaves_state_untouched() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        // Create fake ~/.skillshub/skills/tap/skill
        let skillshub_home = home.join(".skillshub");
        let skills_dir = skillshub_home.join("skills");
        let skill_dir = skills_dir.join("tap").join("skill");
        fs::create_dir_all(&skill_dir).unwrap();

        // Create a minimal db.json
        fs::write(
            skillshub_home.join("db.json"),
            r#"{"taps":{},"installed":{},"linked_agents":[],"external":{}}"#,
        )
        .unwrap();

        // Create fake ~/.claude/skills with a managed symlink
        let claude_skills = home.join(".claude").join("skills");
        fs::create_dir_all(&claude_skills).unwrap();
        let link_path = claude_skills.join("skill");
        std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();

        let _guard = TestHomeGuard::set(&home);
        // Simulate typing "no" at the prompt
        let mut input = io::Cursor::new(b"no\n" as &[u8]);
        let result = clean_all_with_input(false, &mut input);

        assert!(result.is_ok());

        // Everything should still be present
        assert!(skillshub_home.exists(), "skillshub home should still exist");
        assert!(link_path.is_symlink(), "managed symlink should still exist");
    }

    /// Typing `yes` at the interactive prompt proceeds with deletion.
    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_clean_all_interactive_confirm_removes_state() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");

        // Create fake ~/.skillshub/skills/tap/skill
        let skillshub_home = home.join(".skillshub");
        let skills_dir = skillshub_home.join("skills");
        let skill_dir = skills_dir.join("tap").join("skill");
        fs::create_dir_all(&skill_dir).unwrap();

        // Create a minimal db.json
        fs::write(
            skillshub_home.join("db.json"),
            r#"{"taps":{},"installed":{},"linked_agents":[],"external":{}}"#,
        )
        .unwrap();

        // Create fake ~/.claude/skills with a managed symlink
        let claude_skills = home.join(".claude").join("skills");
        fs::create_dir_all(&claude_skills).unwrap();
        let link_path = claude_skills.join("skill");
        std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();

        let _guard = TestHomeGuard::set(&home);
        // Simulate typing "yes" at the prompt
        let mut input = io::Cursor::new(b"yes\n" as &[u8]);
        let result = clean_all_with_input(false, &mut input);

        assert!(result.is_ok());

        // Managed symlink should be gone
        assert!(
            !link_path.exists() && !link_path.is_symlink(),
            "managed symlink should be removed"
        );

        // Skillshub home should be deleted
        assert!(!skillshub_home.exists(), "skillshub home should be deleted");
    }

    // ---------------------------------------------------------------------------
    // is_skillshub_managed_link tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_is_skillshub_managed_link_true() {
        let temp = TempDir::new().unwrap();
        let skillshub_dir = temp.path().join(".skillshub/skills");
        let skill_dir = skillshub_dir.join("tap/skill");
        fs::create_dir_all(&skill_dir).unwrap();

        let agent_skills = temp.path().join(".claude/skills");
        fs::create_dir_all(&agent_skills).unwrap();

        let link_path = agent_skills.join("skill");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&skill_dir, &link_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&skill_dir, &link_path).unwrap();

        let canonical = skillshub_dir.canonicalize().unwrap();
        assert!(is_skillshub_managed_link(&link_path, &canonical));
    }

    #[test]
    fn test_is_skillshub_managed_link_false() {
        let temp = TempDir::new().unwrap();
        let external_dir = temp.path().join("external/skill");
        fs::create_dir_all(&external_dir).unwrap();

        let skillshub_dir = temp.path().join(".skillshub/skills");
        fs::create_dir_all(&skillshub_dir).unwrap();

        let agent_skills = temp.path().join(".claude/skills");
        fs::create_dir_all(&agent_skills).unwrap();

        let link_path = agent_skills.join("external-skill");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&external_dir, &link_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&external_dir, &link_path).unwrap();

        let canonical = skillshub_dir.canonicalize().unwrap();
        assert!(!is_skillshub_managed_link(&link_path, &canonical));
    }

    #[test]
    fn test_is_skillshub_managed_link_not_symlink() {
        let temp = TempDir::new().unwrap();
        let skillshub_dir = temp.path().join(".skillshub/skills");
        fs::create_dir_all(&skillshub_dir).unwrap();

        let regular_dir = temp.path().join("regular");
        fs::create_dir_all(&regular_dir).unwrap();

        let canonical = skillshub_dir.canonicalize().unwrap();
        // Regular directory, not a symlink
        assert!(!is_skillshub_managed_link(&regular_dir, &canonical));
    }
}
