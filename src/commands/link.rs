use anyhow::Result;
use colored::Colorize;

use crate::agent::{discover_agents, known_agent_names};
use crate::paths::get_skills_install_dir;

/// Link installed skills to all discovered coding agents
pub fn link_to_agents() -> Result<()> {
    let skills_dir = get_skills_install_dir()?;

    if !skills_dir.exists() {
        anyhow::bail!("No skills installed. Run 'skillshub install-all' first.");
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

    for agent in &agents {
        let agent_name = agent.path.file_name().unwrap().to_string_lossy();
        let link_path = agent.path.join(agent.skills_subdir);

        if link_path.exists() {
            if link_path.is_symlink() {
                println!("  {} {} (link exists)", "○".yellow(), agent_name);
            } else {
                println!(
                    "  {} {} ({} exists but is not a symlink)",
                    "!".red(),
                    agent_name,
                    agent.skills_subdir
                );
            }
            continue;
        }

        // Create symlink to the skills directory
        #[cfg(unix)]
        std::os::unix::fs::symlink(&skills_dir, &link_path)?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&skills_dir, &link_path)?;

        println!("  {} {} (linked)", "✓".green(), agent_name);
    }

    println!("\n{} Skills linked successfully!", "Done!".green().bold());

    Ok(())
}
