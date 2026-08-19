use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::{discover_agents, known_agent_names, AgentInfo};
use crate::paths::get_skills_install_dir;
use crate::registry::db::{add_external_skill, init_db, is_external_skill, save_db};
use crate::registry::models::{Database, ExternalSkill};
use crate::skill::{has_references_dir, has_scripts_dir, Skill};

/// Link installed skills to all discovered coding agents
pub fn link_to_agents() -> Result<()> {
    let skills_dir = get_skills_install_dir()?;
    let mut db = init_db()?;

    let agents = discover_agents();

    if agents.is_empty() {
        println!(
            "{} No coding agents found. Looked for: {}",
            "Info:".cyan(),
            known_agent_names()
        );
        return Ok(());
    }

    // Step 1: Discover external skills from agent directories
    let skills_dir_canonical = skills_dir.canonicalize().unwrap_or_else(|_| skills_dir.clone());
    let (new_external, all_external) = discover_external_skills(&agents, &mut db, &skills_dir_canonical)?;

    if !new_external.is_empty() {
        println!(
            "{} Discovered {} new external skill(s)",
            "=>".green().bold(),
            new_external.len()
        );
        for name in &new_external {
            if let Some(ext) = db.external.get(name) {
                println!("  {} {} (from {})", "+".green(), name, ext.source_agent);
            }
        }
        save_db(&db)?;
    }

    // Step 2: Collect skillshub-managed skills
    let skills = if skills_dir.exists() {
        collect_installed_skills(&skills_dir)?
    } else {
        Vec::new()
    };

    println!(
        "{} Linking skills to {} discovered agent(s)",
        "=>".green().bold(),
        agents.len()
    );

    // Step 3: Link skills to each agent
    for agent in &agents {
        let agent_name = agent.path.file_name().unwrap().to_string_lossy();
        let link_path = agent.path.join(agent.skills_subdir);

        // Ensure skills directory exists and is a directory (not a symlink to skillshub)
        if link_path.exists() {
            if link_path.is_symlink() {
                let link_target = fs::read_link(&link_path)?;
                let link_target = link_target.canonicalize().unwrap_or(link_target);

                if link_target == skills_dir_canonical {
                    // Old-style symlink to skillshub skills dir, convert to directory
                    fs::remove_file(&link_path)?;
                    fs::create_dir_all(&link_path)?;
                } else {
                    println!(
                        "  {} {} ({} exists but is not managed by skillshub)",
                        "!".yellow(),
                        agent_name,
                        agent.skills_subdir
                    );
                    continue;
                }
            } else if !link_path.is_dir() {
                println!(
                    "  {} {} ({} exists but is not a directory)",
                    "!".yellow(),
                    agent_name,
                    agent.skills_subdir
                );
                continue;
            }
        } else {
            fs::create_dir_all(&link_path)?;
        }

        let mut linked_count = 0;
        let mut skipped_count = 0;
        let mut failed_count = 0;
        let mut external_synced = 0;

        // Link skillshub-managed skills. A per-skill failure (EACCES, ENOSPC,
        // Windows privilege) is reported and skipped — not propagated — so one
        // bad link can't leave the remaining agents unlinked and the db unsaved.
        for skill in &skills {
            let link_name = skill_link_name(skill);
            let skill_link_path = link_path.join(&link_name);

            match create_or_refresh_symlink(&skill.path, &skill_link_path, &skills_dir) {
                Ok(LinkOutcome::Linked) => linked_count += 1,
                Ok(LinkOutcome::Skipped) => skipped_count += 1,
                Err(e) => {
                    eprintln!("  {} {:#}", "!".red(), e);
                    failed_count += 1;
                }
            }
        }

        // Sync external skills to this agent (from their source agents)
        for ext_skill in &all_external {
            let skill_link_path = link_path.join(&ext_skill.name);

            // Skip if this is the source agent (skill already exists there)
            let current_agent_name = format!(".{}", agent_name);
            if ext_skill.source_agent == current_agent_name || ext_skill.source_agent == agent_name {
                continue;
            }

            // Skip if skill already exists (either as file/dir or symlink)
            if skill_link_path.exists() {
                if skill_link_path.is_symlink() {
                    external_synced += 1;
                } else {
                    skipped_count += 1;
                }
                continue;
            }

            // Create symlink to the external skill's source
            #[cfg(unix)]
            std::os::unix::fs::symlink(&ext_skill.source_path, &skill_link_path)?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(&ext_skill.source_path, &skill_link_path)?;

            external_synced += 1;
        }

        // Mark agent as linked in the database
        db.linked_agents.insert(agent_name.to_string());

        // Print status
        let mut parts = vec![format!("linked {}", linked_count)];
        if external_synced > 0 {
            parts.push(format!("synced {} external", external_synced));
        }
        if skipped_count > 0 {
            parts.push(format!("skipped {}", skipped_count));
        }
        if failed_count > 0 {
            parts.push(format!("failed {}", failed_count));
        }
        println!("  {} {} ({})", "✓".green(), agent_name, parts.join(", "));
    }

    // Save the database with linked agents
    save_db(&db)?;

    println!("\n{} Skills linked successfully!", "Done!".green().bold());

    Ok(())
}

/// Discover external skills from agent directories
/// Returns (newly_discovered_names, all_external_skills)
///
/// External skills are real directories (not symlinks) in agent skill directories
/// that weren't installed by skillshub. They are tracked and synced to other agents.
fn discover_external_skills(
    agents: &[AgentInfo],
    db: &mut Database,
    _skillshub_skills_dir: &Path,
) -> Result<(Vec<String>, Vec<ExternalSkill>)> {
    let mut new_external = Vec::new();
    // Track which canonical paths we've seen to avoid duplicates
    let mut seen_sources: HashSet<PathBuf> = HashSet::new();

    // Collect names of skillshub-managed skills to exclude them
    let managed_skill_names: HashSet<String> = db.installed.values().map(|s| s.skill.clone()).collect();

    // Scan all agents for external skills
    for agent in agents {
        let agent_name = agent
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let skills_path = agent.path.join(agent.skills_subdir);

        if !skills_path.exists() || !skills_path.is_dir() {
            continue;
        }

        // Iterate through entries in the agent's skills directory
        for entry in fs::read_dir(&skills_path)? {
            let entry = entry?;
            let path = entry.path();
            let skill_name = entry.file_name().to_string_lossy().to_string();

            // Skip if it's a skillshub-managed skill name
            if managed_skill_names.contains(&skill_name) {
                continue;
            }

            // Skip symlinks - we only track real directories as sources
            // Symlinks are either skillshub-managed or created by us for syncing
            if path.is_symlink() {
                continue;
            }

            // Skip if not a directory
            if !path.is_dir() {
                continue;
            }

            // Get canonical path to detect duplicates
            let source_path = path.canonicalize().unwrap_or_else(|_| path.clone());

            // Skip if we've already seen this source path
            if seen_sources.contains(&source_path) {
                continue;
            }
            seen_sources.insert(source_path.clone());

            // Skip if already tracked as external
            if is_external_skill(db, &skill_name) {
                continue;
            }

            let external = ExternalSkill {
                name: skill_name.clone(),
                source_agent: agent_name.clone(),
                source_path,
                discovered_at: Utc::now(),
            };

            add_external_skill(db, &skill_name, external);
            new_external.push(skill_name.clone());
        }
    }

    // Collect all external skills (including previously discovered ones)
    let all_external: Vec<ExternalSkill> = db.external.values().cloned().collect();

    Ok((new_external, all_external))
}

enum LinkOutcome {
    Linked,
    Skipped,
}

/// Create or refresh a symlink at `link_path` pointing to `target`.
///
/// An existing symlink is replaced only when it is dangling or already points
/// inside `install_dir` (i.e. skillshub-managed). A symlink pointing anywhere
/// else is user-created and left alone; a non-symlink path is left alone too.
fn create_or_refresh_symlink(target: &Path, link_path: &Path, install_dir: &Path) -> Result<LinkOutcome> {
    if let Ok(metadata) = fs::symlink_metadata(link_path) {
        if !metadata.file_type().is_symlink() {
            return Ok(LinkOutcome::Skipped);
        }

        let managed = match fs::read_link(link_path) {
            Ok(t) => {
                let abs = if t.is_absolute() {
                    t
                } else {
                    link_path.parent().map(|p| p.join(&t)).unwrap_or(t)
                };
                // `exists()` follows links, so it is false for dangling ones.
                !link_path.exists() || abs.starts_with(install_dir)
            }
            Err(_) => false,
        };
        if !managed {
            return Ok(LinkOutcome::Skipped);
        }
        fs::remove_file(link_path)
            .with_context(|| format!("Failed to replace existing link {}", link_path.display()))?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link_path)
        .with_context(|| format!("Failed to link {} -> {}", link_path.display(), target.display()))?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(target, link_path)
        .with_context(|| format!("Failed to link {} -> {}", link_path.display(), target.display()))?;

    Ok(LinkOutcome::Linked)
}

fn skill_link_name(skill: &Skill) -> String {
    skill
        .path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| skill.name.clone())
}

fn collect_installed_skills(skills_dir: &Path) -> Result<Vec<Skill>> {
    let mut skills = Vec::new();

    if !skills_dir.exists() {
        return Ok(skills);
    }

    // Recursively find all SKILL.md files in the skills directory
    fn find_skills_recursive(dir: &Path, skills: &mut Vec<Skill>) -> Result<()> {
        if !dir.exists() || !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                // Found a skill directory
                match crate::skill::parse_skill_metadata(&skill_md) {
                    Ok(metadata) => {
                        let has_scripts = has_scripts_dir(&path);
                        let has_references = has_references_dir(&path);

                        skills.push(Skill {
                            name: metadata.name,
                            description: metadata.description.unwrap_or_else(|| "No description".to_string()),
                            path,
                            has_scripts,
                            has_references,
                        });
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Failed to parse skill at {}: {}",
                            "Warning:".yellow(),
                            path.display(),
                            e
                        );
                    }
                }
            } else {
                // Not a skill directory, recurse into it
                find_skills_recursive(&path, skills)?;
            }
        }

        Ok(())
    }

    find_skills_recursive(skills_dir, &mut skills)?;

    let mut seen = HashSet::new();
    let mut unique = Vec::new();

    for skill in skills {
        let link_name = skill_link_name(&skill);
        if !seen.insert(link_name.clone()) {
            println!(
                "{} Duplicate skill name '{}' at {}",
                "Warning:".yellow(),
                link_name,
                skill.path.display()
            );
            continue;
        }
        unique.push(skill);
    }

    unique.sort_by_key(skill_link_name);

    Ok(unique)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_skill(path: &Path, name: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {}\ndescription: Test skill\n---\n# {}\n", name, name),
        )
        .unwrap();
    }

    #[test]
    fn test_collect_installed_skills_flattened() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path();

        write_skill(&skills_dir.join("legacy-skill"), "legacy-skill");
        write_skill(&skills_dir.join("tap-a").join("nested-skill"), "nested-skill");

        let skills = collect_installed_skills(skills_dir).unwrap();
        let names: Vec<String> = skills.iter().map(skill_link_name).collect();

        assert_eq!(names.len(), 2);
        assert!(names.contains(&"legacy-skill".to_string()));
        assert!(names.contains(&"nested-skill".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn test_create_or_refresh_symlink_replaces_dangling_symlink() {
        let temp = TempDir::new().unwrap();
        let install_dir = temp.path().join("install");
        let target = install_dir.join("target-skill");
        write_skill(&target, "target-skill");
        let stale_target = install_dir.join("stale-target");
        let link_path = temp.path().join("link");

        std::os::unix::fs::symlink(&stale_target, &link_path).unwrap();
        assert!(!link_path.exists());

        let outcome = create_or_refresh_symlink(&target, &link_path, &install_dir).unwrap();
        assert!(matches!(outcome, LinkOutcome::Linked));
        assert_eq!(fs::read_link(&link_path).unwrap(), target);
    }

    #[cfg(unix)]
    #[test]
    fn test_create_or_refresh_symlink_refreshes_valid_symlink() {
        let temp = TempDir::new().unwrap();
        let install_dir = temp.path().join("install");
        let target = install_dir.join("target-skill");
        write_skill(&target, "target-skill");
        let link_path = temp.path().join("link");

        std::os::unix::fs::symlink(&target, &link_path).unwrap();

        let outcome = create_or_refresh_symlink(&target, &link_path, &install_dir).unwrap();
        assert!(matches!(outcome, LinkOutcome::Linked));
        assert_eq!(fs::read_link(&link_path).unwrap(), target);
    }

    #[cfg(unix)]
    #[test]
    fn test_create_or_refresh_symlink_leaves_foreign_symlink_alone() {
        let temp = TempDir::new().unwrap();
        let install_dir = temp.path().join("install");
        let target = install_dir.join("target-skill");
        write_skill(&target, "target-skill");
        // A symlink the user created, pointing at their own checkout outside install_dir.
        let foreign_target = temp.path().join("my-own-checkout");
        write_skill(&foreign_target, "target-skill");
        let link_path = temp.path().join("link");

        std::os::unix::fs::symlink(&foreign_target, &link_path).unwrap();

        let outcome = create_or_refresh_symlink(&target, &link_path, &install_dir).unwrap();
        assert!(matches!(outcome, LinkOutcome::Skipped));
        assert_eq!(
            fs::read_link(&link_path).unwrap(),
            foreign_target,
            "user-owned symlink must not be redirected"
        );
    }

    #[test]
    fn test_create_or_refresh_symlink_skips_real_directory() {
        let temp = TempDir::new().unwrap();
        let install_dir = temp.path().join("install");
        let target = install_dir.join("target-skill");
        write_skill(&target, "target-skill");
        let link_path = temp.path().join("link");
        fs::create_dir_all(&link_path).unwrap();

        let outcome = create_or_refresh_symlink(&target, &link_path, &install_dir).unwrap();
        assert!(matches!(outcome, LinkOutcome::Skipped));
        assert!(link_path.is_dir());
        assert!(!link_path.is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn test_create_or_refresh_symlink_creates_new_symlink() {
        let temp = TempDir::new().unwrap();
        let install_dir = temp.path().join("install");
        let target = install_dir.join("target-skill");
        write_skill(&target, "target-skill");
        let link_path = temp.path().join("link");

        let outcome = create_or_refresh_symlink(&target, &link_path, &install_dir).unwrap();
        assert!(matches!(outcome, LinkOutcome::Linked));
        assert_eq!(fs::read_link(&link_path).unwrap(), target);
    }
}
