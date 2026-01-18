mod agent;
mod cli;
mod commands;
mod paths;
mod registry;
mod skill;
mod util;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands, ExternalCommands, TapCommands};
use commands::{external_forget, external_list, external_scan, link_to_agents, show_agents};
use registry::{
    add_skill_from_url, add_tap, install_all, install_all_from_tap, install_skill, list_skills, list_taps,
    migrate_old_installations, needs_migration, remove_tap, search_skills, show_skill_info, uninstall_skill,
    update_skill, update_tap,
};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Auto-migrate old installations on first run (except for migrate command itself)
    if !matches!(cli.command, Commands::Migrate) && needs_migration()? {
        migrate_old_installations()?;
    }

    match cli.command {
        Commands::InstallAll => install_all()?,
        Commands::Install { name } => install_skill(&name)?,
        Commands::Add { url } => add_skill_from_url(&url)?,
        Commands::Uninstall { name } => uninstall_skill(&name)?,
        Commands::Update { name } => update_skill(name.as_deref())?,
        Commands::List => list_skills()?,
        Commands::Search { query } => search_skills(&query)?,
        Commands::Info { name } => show_skill_info(&name)?,
        Commands::Link => link_to_agents()?,
        Commands::Agents => show_agents()?,
        Commands::Tap(tap_cmd) => match tap_cmd {
            TapCommands::Add { url } => add_tap(&url)?,
            TapCommands::Remove { name } => remove_tap(&name)?,
            TapCommands::List => list_taps()?,
            TapCommands::Update { name } => update_tap(name.as_deref())?,
            TapCommands::InstallAll { name } => install_all_from_tap(&name)?,
        },
        Commands::External(ext_cmd) => match ext_cmd {
            ExternalCommands::List => external_list()?,
            ExternalCommands::Scan => external_scan()?,
            ExternalCommands::Forget { name } => external_forget(&name)?,
        },
        Commands::Migrate => migrate_old_installations()?,
    }

    Ok(())
}
