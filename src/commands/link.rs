use anyhow::Result;
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::agent::{discover_agents, known_agent_names};
use crate::paths::get_skills_install_dir;
use crate::skill::{discover_skills, Skill};

/// Link installed skills to all discovered coding agents
pub fn link_to_agents() -> Result<()> {
    let skills_dir = get_skills_install_dir()?;

    if !skills_dir.exists() {
        anyhow::bail!("No skills installed. Run 'skillshub install-all' first.");
    }

    let skills = collect_installed_skills(&skills_dir)?;
    if skills.is_empty() {
        println!("{} No skills found. Install skills before linking.", "Info:".cyan());
        return Ok(());
    }

    let agents = discover_agents();

    if agents.is_empty() {
        println!(
            "{} No coding agents found. Looked for: {}",
            "Info:".cyan(),
            known_agent_names()
        );
        return Ok(());
    }

    println!(
        "{} Linking skills to {} discovered agent(s)",
        "=>".green().bold(),
        agents.len()
    );

    let skills_dir_canonical = skills_dir.canonicalize().unwrap_or_else(|_| skills_dir.clone());

    for agent in &agents {
        let agent_name = agent.path.file_name().unwrap().to_string_lossy();
        let link_path = agent.path.join(agent.skills_subdir);

        if link_path.exists() {
            if link_path.is_symlink() {
                let link_target = fs::read_link(&link_path)?;
                let link_target = link_target.canonicalize().unwrap_or(link_target);

                if link_target == skills_dir_canonical {
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

        if skipped_count > 0 {
            println!(
                "  {} {} (linked {}, skipped {})",
                "✓".green(),
                agent_name,
                linked_count,
                skipped_count
            );
        } else {
            println!("  {} {} (linked {})", "✓".green(), agent_name, linked_count);
        }
    }

    println!("\n{} Skills linked successfully!", "Done!".green().bold());

    Ok(())
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
