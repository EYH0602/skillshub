use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tabled::{settings::Style, Table, Tabled};

use crate::agent::{discover_agents, AgentInfo};
use crate::paths::get_skills_install_dir;
use crate::registry::db::{
    add_external_skill, get_all_external_skills, init_db, is_external_skill, remove_external_skill, save_db,
};
use crate::registry::models::{Database, ExternalSkill};

#[derive(Tabled)]
struct ExternalSkillRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Source Agent")]
    source_agent: String,
    #[tabled(rename = "Source Path")]
    source_path: String,
    #[tabled(rename = "Discovered")]
    discovered: String,
}

/// List all discovered external skills
pub fn external_list() -> Result<()> {
    let db = init_db()?;
    let external_skills = get_all_external_skills(&db);

    if external_skills.is_empty() {
        println!("{} No external skills discovered yet.", "Info:".cyan());
        println!("Run 'skillshub link' or 'skillshub external scan' to discover external skills.");
        return Ok(());
    }

    println!(
        "{} External Skills (managed elsewhere, synced by skillshub):\n",
        "=>".green().bold()
    );

    let mut rows: Vec<ExternalSkillRow> = external_skills
        .iter()
        .map(|(_, skill)| ExternalSkillRow {
            name: skill.name.clone(),
            source_agent: skill.source_agent.clone(),
            source_path: skill.source_path.display().to_string(),
            discovered: skill.discovered_at.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect();

    rows.sort_by(|a, b| a.name.cmp(&b.name));

    let table = Table::new(rows).with(Style::rounded()).to_string();
    println!("{}", table);

    Ok(())
}

/// Scan agent directories for external skills
pub fn external_scan() -> Result<()> {
    let skills_dir = get_skills_install_dir()?;
    let skills_dir_canonical = skills_dir.canonicalize().unwrap_or_else(|_| skills_dir.clone());
    let mut db = init_db()?;

    let agents = discover_agents();

    if agents.is_empty() {
        println!("{} No coding agents found.", "Info:".cyan());
        return Ok(());
    }

    println!(
        "{} Scanning {} agent(s) for external skills...",
        "=>".green().bold(),
        agents.len()
    );

    let (new_external, all_external) = discover_external_skills_internal(&agents, &mut db, &skills_dir_canonical)?;

    if new_external.is_empty() {
        println!(
            "{} No new external skills discovered. Total tracked: {}",
            "Info:".cyan(),
            all_external.len()
        );
    } else {
        println!(
            "{} Discovered {} new external skill(s):",
            "=>".green().bold(),
            new_external.len()
        );
        for name in &new_external {
            if let Some(ext) = db.external.get(name) {
                println!("  {} {} (from {})", "+".green(), name, ext.source_agent);
            }
        }
        save_db(&db)?;
        println!(
            "\n{} Total external skills tracked: {}",
            "Done!".green().bold(),
            all_external.len()
        );
    }

    Ok(())
}

/// Stop tracking an external skill
pub fn external_forget(name: &str) -> Result<()> {
    let mut db = init_db()?;

    if !is_external_skill(&db, name) {
        anyhow::bail!("External skill '{}' not found", name);
    }

    let removed = remove_external_skill(&mut db, name);
    save_db(&db)?;

    if let Some(skill) = removed {
        println!(
            "{} Stopped tracking external skill '{}' (was from {})",
            "Done!".green().bold(),
            name,
            skill.source_agent
        );
        println!(
            "{} The skill itself was not deleted. Symlinks in other agents will remain until removed.",
            "Note:".cyan()
        );
    }

    Ok(())
}

/// Internal function to discover external skills (shared with link.rs logic)
///
/// External skills are real directories (not symlinks) in agent skill directories
/// that weren't installed by skillshub. They are tracked and synced to other agents.
fn discover_external_skills_internal(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_skill_dir(path: &std::path::Path) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            "---\nname: test\ndescription: Test\n---\n# Test\n",
        )
        .unwrap();
    }

    #[test]
    fn test_external_skill_row_creation() {
        let row = ExternalSkillRow {
            name: "test-skill".to_string(),
            source_agent: ".claude".to_string(),
            source_path: "/home/user/.claude/skills/test-skill".to_string(),
            discovered: "2024-01-17 10:00".to_string(),
        };

        assert_eq!(row.name, "test-skill");
        assert_eq!(row.source_agent, ".claude");
    }

    #[test]
    fn test_discover_external_skills_empty() {
        let temp = TempDir::new().unwrap();
        let skillshub_dir = temp.path().join("skillshub");
        fs::create_dir_all(&skillshub_dir).unwrap();

        let mut db = Database::default();
        let agents: Vec<AgentInfo> = vec![];

        let (new_external, all_external) = discover_external_skills_internal(&agents, &mut db, &skillshub_dir).unwrap();

        assert!(new_external.is_empty());
        assert!(all_external.is_empty());
    }

    #[test]
    fn test_discover_external_skills_finds_real_dirs() {
        let temp = TempDir::new().unwrap();
        let skillshub_dir = temp.path().join("skillshub");
        fs::create_dir_all(&skillshub_dir).unwrap();

        // Create a mock agent directory with an external skill
        let agent_path = temp.path().join(".claude");
        let skills_path = agent_path.join("skills");
        let external_skill_path = skills_path.join("my-external-skill");
        create_skill_dir(&external_skill_path);

        let agents = vec![AgentInfo {
            path: agent_path,
            skills_subdir: "skills",
        }];

        let mut db = Database::default();
        let (new_external, all_external) = discover_external_skills_internal(&agents, &mut db, &skillshub_dir).unwrap();

        assert_eq!(new_external.len(), 1);
        assert!(new_external.contains(&"my-external-skill".to_string()));
        assert_eq!(all_external.len(), 1);
    }
}
