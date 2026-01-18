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
    /// Install all skills from default taps
    InstallAll,

    /// Install a skill (format: owner/repo/skill[@commit])
    Install {
        /// Full skill name (e.g., EYH0602/skillshub/code-reviewer)
        name: String,
    },

    /// Add a skill directly from a GitHub URL
    Add {
        /// GitHub folder URL (e.g., https://github.com/user/repo/tree/commit/path/to/skill)
        url: String,
    },

    /// Uninstall a skill (format: owner/repo/skill)
    Uninstall {
        /// Full skill name (e.g., EYH0602/skillshub/code-reviewer)
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
        /// Full skill name (e.g., EYH0602/skillshub/code-reviewer)
        name: String,
    },

    /// Link installed skills to discovered coding agents
    Link,

    /// Show which coding agents are detected on this system
    Agents,

    /// Manage skill taps (repositories)
    #[command(subcommand)]
    Tap(TapCommands),

    /// Manage external skills (discovered from other sources)
    #[command(subcommand)]
    External(ExternalCommands),

    /// Migrate old-style installations to the new registry format
    Migrate,
}

#[derive(Subcommand)]
pub enum TapCommands {
    /// Add a new tap from a GitHub URL
    Add {
        /// GitHub repository URL (e.g., https://github.com/user/skillshub-tap)
        url: String,

        /// Install all skills from the tap after adding
        #[arg(short, long)]
        install: bool,
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

    /// Install all skills from a specific tap
    InstallAll {
        /// Name of the tap to install from (e.g., EYH0602/skillshub)
        name: String,
    },
}

#[derive(Subcommand)]
pub enum ExternalCommands {
    /// List all discovered external skills
    List,

    /// Scan agent directories for external skills
    Scan,

    /// Stop tracking an external skill (does not delete the skill)
    Forget {
        /// Name of the external skill to forget
        name: String,
    },
}
