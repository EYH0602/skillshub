use anyhow::Result;
use colored::Colorize;
use tabled::{
    settings::{Padding, Style},
    Table,
};

use crate::agent::{discover_agents, known_agent_names, AgentRow};
use crate::paths::display_path_with_tilde;

/// Show discovered coding agents
pub fn show_agents() -> Result<()> {
    let agents = discover_agents();

    if agents.is_empty() {
        println!("No coding agents found.");
        println!();
        println!("Looked for: {}", known_agent_names());
        return Ok(());
    }

    let rows: Vec<AgentRow> = agents
        .iter()
        .map(|agent| {
            let agent_name = agent
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let link_path = agent.path.join(agent.skills_subdir);

            let status = if link_path.exists() {
                if link_path.is_symlink() {
                    "✓ linked"
                } else {
                    "! not symlink"
                }
            } else {
                "○ not linked"
            };

            AgentRow {
                name: agent_name,
                status,
                path: display_path_with_tilde(&link_path),
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
