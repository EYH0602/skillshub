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
    /// Install all skills from the default tap
    InstallAll,

    /// Install a skill (format: tap/skill[@commit])
    Install {
        /// Full skill name (e.g., skillshub/skill-creator)
        name: String,
    },

    /// Add a skill directly from a GitHub URL
    Add {
        /// GitHub folder URL (e.g., https://github.com/user/repo/tree/commit/path/to/skill)
        url: String,
    },

    /// Uninstall a skill (format: tap/skill)
    Uninstall {
        /// Full skill name (e.g., skillshub/skill-creator)
        name: String,
    },

    /// Update installed skill(s) to latest version
    Update {
        /// Full skill name to update, or omit to update all
        name: Option<String>,
    },

    /// List all available skills
    List,

    /// Search for skills across all taps
    Search {
        /// Search query
        query: String,
    },

    /// Show detailed information about a skill
    Info {
        /// Full skill name (e.g., skillshub/skill-creator)
        name: String,
    },

    /// Link installed skills to discovered coding agents
    Link,

    /// Show which coding agents are detected on this system
    Agents,

    /// Manage skill taps (repositories)
    #[command(subcommand)]
    Tap(TapCommands),

    /// Migrate old-style installations to the new registry format
    Migrate,
}

#[derive(Subcommand)]
pub enum TapCommands {
    /// Add a new tap from a GitHub URL
    Add {
        /// GitHub repository URL (e.g., https://github.com/user/skillshub-tap)
        url: String,
    },

    /// Remove a tap
    Remove {
        /// Name of the tap to remove
        name: String,
    },

    /// List configured taps
    List,

    /// Update tap registry (fetch latest from remote)
    Update {
        /// Name of the tap to update, or omit to update all
        name: Option<String>,
    },
}
