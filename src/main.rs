mod agent;
mod cli;
mod commands;
mod paths;
mod skill;
mod util;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};
use commands::{
    install_all, install_skill, link_to_agents, list_skills, show_agents, show_skill_info,
    uninstall_skill,
};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::InstallAll => install_all()?,
        Commands::Install { name } => install_skill(&name)?,
        Commands::List => list_skills()?,
        Commands::Info { name } => show_skill_info(&name)?,
        Commands::Link => link_to_agents()?,
        Commands::Agents => show_agents()?,
        Commands::Uninstall { name } => uninstall_skill(&name)?,
    }

    Ok(())
}
