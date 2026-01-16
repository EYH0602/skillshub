use clap::{Parser, Subcommand};

/// Skillshub - A package manager for AI coding agent skills
#[derive(Parser)]
#[command(name = "skillshub")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install all available skills to ~/.skillshub
    InstallAll,
    /// Install a specific skill by name
    Install {
        /// Name of the skill to install
        name: String,
    },
    /// List all available skills
    List,
    /// Show detailed information about a skill
    Info {
        /// Name of the skill
        name: String,
    },
    /// Link installed skills to discovered coding agents
    Link,
    /// Show which coding agents are detected on this system
    Agents,
    /// Uninstall a specific skill
    Uninstall {
        /// Name of the skill to uninstall
        name: String,
    },
}
