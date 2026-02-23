use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::agent::discover_agents;
use crate::paths::{display_path_with_tilde, get_skills_install_dir, get_skillshub_home};
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

    let mut total_removed = 0;

    for agent in &agents {
        let agent_name = agent.path.file_name().unwrap().to_string_lossy();
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
                if is_skillshub_managed_link(&path, &skills_dir_canonical) {
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
    let skillshub_home = get_skillshub_home()?;
    let skills_dir = get_skills_install_dir()?;
    let db_path = get_db_path()?;
    let agents = discover_agents();

    // --- Print warning and summary ---
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
        let agent_name = agent.path.file_name().unwrap().to_string_lossy();
        let skills_path = agent.path.join(agent.skills_subdir);
        println!("      {} ({})", agent_name, display_path_with_tilde(&skills_path));
    }
    println!("  - Installed skills: {}", display_path_with_tilde(&skills_dir));
    println!("  - Database: {}", display_path_with_tilde(&db_path));
    println!(
        "  - Skillshub home directory: {}",
        display_path_with_tilde(&skillshub_home)
    );

    // --- Confirmation prompt ---
    if !confirm {
        println!();
        print!("Confirm: Type 'yes' to confirm: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        if trimmed != "yes" {
            println!("{}", "Cancelled. Nothing was removed.".yellow());
            return Ok(());
        }
    }

    println!();
    println!("{} Starting full uninstall...", "=>".green().bold());

    // --- Remove symlinks ---
    println!("  {} Removing skillshub-managed symlinks...", "=>".green().bold());

    let skills_dir_canonical = skills_dir.canonicalize().unwrap_or_else(|_| skills_dir.clone());
    let mut total_removed = 0;

    for agent in &agents {
        let agent_name = agent.path.file_name().unwrap().to_string_lossy();
        let skills_path = agent.path.join(agent.skills_subdir);

        if !skills_path.exists() {
            continue;
        }

        let mut removed_count = 0;

        if let Ok(entries) = fs::read_dir(&skills_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                if !path.is_symlink() {
                    continue;
                }

                if is_skillshub_managed_link(&path, &skills_dir_canonical) {
                    if let Err(e) = fs::remove_file(&path) {
                        eprintln!("     {} Failed to remove {}: {}", "!".red(), path.display(), e);
                    } else {
                        removed_count += 1;
                    }
                }
            }
        }

        if removed_count > 0 {
            println!(
                "     {} {} ({} link(s) removed)",
                "✓".green(),
                agent_name,
                removed_count
            );
            total_removed += removed_count;
        }
    }

    println!("  {} Removed {} symlink(s) total", "✓".green(), total_removed);

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
    use std::fs;
    use tempfile::TempDir;

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
