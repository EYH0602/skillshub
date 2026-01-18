use anyhow::Result;
use colored::Colorize;
use std::fs;
use tabled::{
    settings::{Padding, Style},
    Table,
};

use crate::agent::{discover_agents, known_agent_names, AgentRow};
use crate::paths::display_path_with_tilde;
use crate::registry::db::load_db;

/// Count skills in an agent's skills directory
/// Returns (total, managed_by_skillshub, external)
fn count_skills_in_dir(skills_path: &std::path::Path, db: &crate::registry::models::Database) -> (usize, usize, usize) {
    if !skills_path.exists() || !skills_path.is_dir() {
        return (0, 0, 0);
    }

    let entries: Vec<_> = match fs::read_dir(skills_path) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                // Count directories and symlinks (skills are either real dirs or symlinks)
                path.is_dir() || path.is_symlink()
            })
            .collect(),
        Err(_) => return (0, 0, 0),
    };

    let total = entries.len();
    let mut managed = 0;
    let mut external = 0;

    for entry in entries {
        let skill_name = entry.file_name().to_string_lossy().to_string();

        // Check if this skill is managed by skillshub (exists in db.installed)
        let is_managed = db.installed.values().any(|s| s.skill == skill_name);

        // Check if this skill is tracked as external
        let is_external = db.external.contains_key(&skill_name);

        if is_managed {
            managed += 1;
        } else if is_external {
            external += 1;
        } else {
            // Untracked skill - count as external (not managed by skillshub)
            external += 1;
        }
    }

    (total, managed, external)
}

/// Show discovered coding agents
pub fn show_agents() -> Result<()> {
    let agents = discover_agents();

    if agents.is_empty() {
        println!("No coding agents found.");
        println!();
        println!("Looked for: {}", known_agent_names());
        return Ok(());
    }

    // Load database to check which skills are managed
    let db = load_db().unwrap_or_default();

    let rows: Vec<AgentRow> = agents
        .iter()
        .map(|agent| {
            let agent_name = agent.path.file_name().unwrap().to_string_lossy().to_string();
            let skills_path = agent.path.join(agent.skills_subdir);

            // Count skills in the directory
            let (total, managed, external) = count_skills_in_dir(&skills_path, &db);

            // Status is "linked" if the agent is recorded in the database
            let status = if db.linked_agents.contains(&agent_name) {
                "✓ linked"
            } else {
                "○ not linked"
            };

            // Format skills column: show count or "-" if not linked
            let skills = if db.linked_agents.contains(&agent_name) {
                if total > 0 {
                    format!("{} ({} managed, {} other)", total, managed, external)
                } else {
                    "0".to_string()
                }
            } else {
                "-".to_string()
            };

            AgentRow {
                name: agent_name,
                status,
                skills,
                path: display_path_with_tilde(&skills_path),
            }
        })
        .collect();

    let table = Table::new(rows)
        .with(Style::rounded())
        .with(Padding::new(1, 1, 0, 1))
        .to_string();

    println!("{}", table);
    println!();
    println!(
        "{} Run {} to link skills to agents",
        "Tip:".cyan(),
        "skillshub link".bold()
    );

    Ok(())
}
