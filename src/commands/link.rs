use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::agent::{discover_agents, known_agent_names, AgentInfo};
use crate::paths::get_skills_install_dir;
use crate::registry::db::{add_external_skill, init_db, is_external_skill, save_db};
use crate::registry::models::{Database, ExternalSkill};
use crate::skill::{discover_skills, Skill};

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

    if skills.is_empty() && all_external.is_empty() {
        println!(
            "{} No skills found. Install skills or use agents with existing skills.",
            "Info:".cyan()
        );
        return Ok(());
    }

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
                        "!".red(),
                        agent_name,
                        agent.skills_subdir
                    );
                    continue;
                }
            } else if !link_path.is_dir() {
                println!(
                    "  {} {} ({} exists but is not a directory)",
                    "!".red(),
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
        let mut external_synced = 0;

        // Link skillshub-managed skills
        for skill in &skills {
            let link_name = skill_link_name(skill);
            let skill_link_path = link_path.join(&link_name);

            if skill_link_path.exists() {
                if skill_link_path.is_symlink() {
                    linked_count += 1;
                } else {
                    skipped_count += 1;
                }
                continue;
            }

            #[cfg(unix)]
            std::os::unix::fs::symlink(&skill.path, &skill_link_path)?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(&skill.path, &skill_link_path)?;

            linked_count += 1;
        }

        // Sync external skills to this agent (from their source agents)
        for ext_skill in &all_external {
            let skill_link_path = link_path.join(&ext_skill.name);

            // Skip if this is the source agent (skill already exists there)
            let current_agent_name = format!(".{}", agent_name);
            if ext_skill.source_agent == current_agent_name || ext_skill.source_agent == agent_name.to_string() {
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

        // Print status
        let mut parts = vec![format!("linked {}", linked_count)];
        if external_synced > 0 {
            parts.push(format!("synced {} external", external_synced));
        }
        if skipped_count > 0 {
            parts.push(format!("skipped {}", skipped_count));
        }
        println!("  {} {} ({})", "✓".green(), agent_name, parts.join(", "));
    }

    println!("\n{} Skills linked successfully!", "Done!".green().bold());

    Ok(())
}

/// Discover external skills from agent directories
/// Returns (newly_discovered_names, all_external_skills)
fn discover_external_skills(
    agents: &[AgentInfo],
    db: &mut Database,
    skillshub_skills_dir: &Path,
) -> Result<(Vec<String>, Vec<ExternalSkill>)> {
    let mut new_external = Vec::new();
    let mut seen_skills: HashSet<String> = HashSet::new();

    // Collect names of skillshub-managed skills to exclude them
    let managed_skill_names: HashSet<String> = db.installed.values().map(|s| s.skill.clone()).collect();

    // Process agents in order (Claude first due to KNOWN_AGENTS order)
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

            // Skip if already seen (another agent already owns this skill)
            if seen_skills.contains(&skill_name) {
                continue;
            }

            // Skip if it's a skillshub-managed skill
            if managed_skill_names.contains(&skill_name) {
                continue;
            }

            // Skip if already tracked as external
            if is_external_skill(db, &skill_name) {
                seen_skills.insert(skill_name);
                continue;
            }

            // Check if this is a symlink pointing to skillshub's skills directory
            if path.is_symlink() {
                if let Ok(target) = fs::read_link(&path) {
                    let target_canonical = target.canonicalize().unwrap_or(target);
                    if target_canonical.starts_with(skillshub_skills_dir) {
                        // This is a skillshub-managed symlink, skip
                        continue;
                    }
                }
            }

            // This is an external skill - either a real directory or symlink to elsewhere
            // Get the canonical path (resolve symlinks to find the real source)
            let source_path = if path.is_symlink() {
                fs::read_link(&path)
                    .ok()
                    .and_then(|p| p.canonicalize().ok())
                    .unwrap_or_else(|| path.clone())
            } else {
                path.canonicalize().unwrap_or_else(|_| path.clone())
            };

            // Only track if it's a directory (skills are directories)
            if !source_path.is_dir() {
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
            seen_skills.insert(skill_name);
        }
    }

    // Collect all external skills (including previously discovered ones)
    let all_external: Vec<ExternalSkill> = db.external.values().cloned().collect();

    Ok((new_external, all_external))
}

fn skill_link_name(skill: &Skill) -> String {
    skill
        .path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| skill.name.clone())
}

fn collect_installed_skills(skills_dir: &Path) -> Result<Vec<Skill>> {
    let mut skills = discover_skills(skills_dir)?;

    if skills_dir.exists() {
        for entry in fs::read_dir(skills_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            if path.join("SKILL.md").exists() {
                continue;
            }

            let mut tap_skills = discover_skills(&path)?;
            skills.append(&mut tap_skills);
        }
    }

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
}
