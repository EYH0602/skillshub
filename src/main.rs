use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Known coding agents that skillshub can manage
const KNOWN_AGENTS: &[&str] = &[
    ".claude",
    ".codex",
    ".opencode",
    ".aider",
    ".cursor",
    ".continue",
];

/// Skillshub - A package manager for AI coding agent skills
#[derive(Parser)]
#[command(name = "skillshub")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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

/// Skill metadata parsed from SKILL.md frontmatter
#[derive(Debug, Deserialize)]
struct SkillMetadata {
    name: String,
    description: Option<String>,
    #[serde(rename = "allowed-tools")]
    #[serde(default)]
    #[allow(dead_code)]
    allowed_tools: AllowedTools,
}

/// Flexible deserializer for allowed-tools (can be string or array)
#[derive(Debug, Default)]
#[allow(dead_code)]
struct AllowedTools(Vec<String>);

impl<'de> Deserialize<'de> for AllowedTools {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct AllowedToolsVisitor;

        impl<'de> Visitor<'de> for AllowedToolsVisitor {
            type Value = AllowedTools;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or array of strings")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(AllowedTools(
                    value.split(',').map(|s| s.trim().to_string()).collect(),
                ))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut tools = Vec::new();
                while let Some(value) = seq.next_element::<String>()? {
                    tools.push(value);
                }
                Ok(AllowedTools(tools))
            }
        }

        deserializer.deserialize_any(AllowedToolsVisitor)
    }
}

/// Represents a discovered skill
#[derive(Debug)]
struct Skill {
    name: String,
    description: String,
    path: PathBuf,
    has_scripts: bool,
    has_references: bool,
}

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

/// Get the skillshub home directory (~/.skillshub)
fn get_skillshub_home() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".skillshub"))
}

/// Get the skills installation directory (~/.skillshub/skills)
fn get_skills_install_dir() -> Result<PathBuf> {
    Ok(get_skillshub_home()?.join("skills"))
}

/// Get the embedded skills directory (relative to the binary or from cargo package)
fn get_embedded_skills_dir() -> Result<PathBuf> {
    // First, try to find skills relative to the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check if we're running from the development directory
            let dev_skills = exe_dir.join("../../skills");
            if dev_skills.exists() {
                return Ok(dev_skills.canonicalize()?);
            }

            // Check for skills in the same directory as the binary
            let local_skills = exe_dir.join("skills");
            if local_skills.exists() {
                return Ok(local_skills);
            }
        }
    }

    // Try current working directory
    let cwd_skills = std::env::current_dir()?.join("skills");
    if cwd_skills.exists() {
        return Ok(cwd_skills);
    }

    // Fallback: check if running from cargo run in the project directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cargo_skills = PathBuf::from(manifest_dir).join("skills");
    if cargo_skills.exists() {
        return Ok(cargo_skills);
    }

    anyhow::bail!("Could not find skills directory. Make sure you're running from the skillshub directory or have installed skills.")
}

/// Parse skill metadata from SKILL.md file
fn parse_skill_metadata(skill_md_path: &PathBuf) -> Result<SkillMetadata> {
    let content = fs::read_to_string(skill_md_path)
        .with_context(|| format!("Failed to read {}", skill_md_path.display()))?;

    // Extract YAML frontmatter between --- markers
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        anyhow::bail!(
            "Invalid SKILL.md format: missing YAML frontmatter in {}",
            skill_md_path.display()
        );
    }

    let yaml_content = parts[1].trim();
    let metadata: SkillMetadata = serde_yaml::from_str(yaml_content).with_context(|| {
        format!(
            "Failed to parse YAML frontmatter in {}",
            skill_md_path.display()
        )
    })?;

    Ok(metadata)
}

/// Discover all skills in a directory
fn discover_skills(skills_dir: &PathBuf) -> Result<Vec<Skill>> {
    let mut skills = Vec::new();

    if !skills_dir.exists() {
        return Ok(skills);
    }

    for entry in fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        match parse_skill_metadata(&skill_md) {
            Ok(metadata) => {
                let has_scripts = path.join("scripts").exists();
                let has_references =
                    path.join("references").exists() || path.join("resources").exists();

                skills.push(Skill {
                    name: metadata.name,
                    description: metadata
                        .description
                        .unwrap_or_else(|| "No description".to_string()),
                    path,
                    has_scripts,
                    has_references,
                });
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to parse skill at {}: {}",
                    "Warning:".yellow(),
                    path.display(),
                    e
                );
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Install all skills to ~/.skillshub
fn install_all() -> Result<()> {
    let source_dir = get_embedded_skills_dir()?;
    let install_dir = get_skills_install_dir()?;

    println!(
        "{} Installing all skills from {}",
        "=>".green().bold(),
        source_dir.display()
    );

    // Create the installation directory
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("Failed to create {}", install_dir.display()))?;

    let skills = discover_skills(&source_dir)?;

    if skills.is_empty() {
        println!(
            "{} No skills found in {}",
            "Warning:".yellow(),
            source_dir.display()
        );
        return Ok(());
    }

    let mut installed_count = 0;

    for skill in &skills {
        let dest = install_dir.join(&skill.name);

        if dest.exists() {
            println!("  {} {} (already installed)", "○".yellow(), skill.name);
            continue;
        }

        // Copy the skill directory
        copy_dir_recursive(&skill.path, &dest)?;
        println!("  {} {}", "✓".green(), skill.name);
        installed_count += 1;
    }

    println!(
        "\n{} Installed {} skills to {}",
        "Done!".green().bold(),
        installed_count,
        install_dir.display()
    );

    // Prompt to link
    println!(
        "\n{} Run {} to link skills to your coding agents",
        "Tip:".cyan(),
        "skillshub link".bold()
    );

    Ok(())
}

/// Install a specific skill
fn install_skill(name: &str) -> Result<()> {
    let source_dir = get_embedded_skills_dir()?;
    let install_dir = get_skills_install_dir()?;

    let skills = discover_skills(&source_dir)?;
    let skill = skills
        .iter()
        .find(|s| s.name == name)
        .with_context(|| format!("Skill '{}' not found", name))?;

    fs::create_dir_all(&install_dir)?;

    let dest = install_dir.join(&skill.name);

    if dest.exists() {
        println!(
            "{} Skill '{}' is already installed at {}",
            "Info:".cyan(),
            name,
            dest.display()
        );
        return Ok(());
    }

    copy_dir_recursive(&skill.path, &dest)?;

    println!(
        "{} Installed '{}' to {}",
        "✓".green(),
        skill.name,
        dest.display()
    );

    Ok(())
}

/// List all available skills
fn list_skills() -> Result<()> {
    let source_dir = get_embedded_skills_dir()?;
    let install_dir = get_skills_install_dir()?;

    let available_skills = discover_skills(&source_dir)?;
    let installed_skills = discover_skills(&install_dir)?;

    let installed_names: std::collections::HashSet<_> =
        installed_skills.iter().map(|s| &s.name).collect();

    println!("{}", "Available Skills".bold().underline());
    println!();

    if available_skills.is_empty() {
        println!("  No skills found.");
        return Ok(());
    }

    for skill in &available_skills {
        let status = if installed_names.contains(&skill.name) {
            "✓".green()
        } else {
            "○".dimmed()
        };

        let extras = format!(
            "{}{}",
            if skill.has_scripts { " [scripts]" } else { "" },
            if skill.has_references { " [refs]" } else { "" }
        )
        .dimmed();

        println!(
            "  {} {:<25} {}{}",
            status,
            skill.name.bold(),
            skill.description.dimmed(),
            extras
        );
    }

    println!();
    println!(
        "  {} installed, {} available",
        installed_skills.len().to_string().green(),
        available_skills.len()
    );

    Ok(())
}

/// Show detailed information about a skill
fn show_skill_info(name: &str) -> Result<()> {
    let source_dir = get_embedded_skills_dir()?;
    let install_dir = get_skills_install_dir()?;

    // Check both source and installed locations
    let skills = discover_skills(&source_dir)?;
    let installed_skills = discover_skills(&install_dir)?;

    let skill = skills
        .iter()
        .chain(installed_skills.iter())
        .find(|s| s.name == name)
        .with_context(|| format!("Skill '{}' not found", name))?;

    let is_installed = install_dir.join(&skill.name).exists();

    println!("{}", skill.name.bold().underline());
    println!();
    println!("  {}: {}", "Description".cyan(), skill.description);
    println!("  {}: {}", "Location".cyan(), skill.path.display());
    println!(
        "  {}: {}",
        "Status".cyan(),
        if is_installed {
            "Installed".green()
        } else {
            "Not installed".yellow()
        }
    );

    if skill.has_scripts {
        println!("  {}: Yes", "Has scripts".cyan());
        let scripts_dir = skill.path.join("scripts");
        if scripts_dir.exists() {
            for entry in fs::read_dir(scripts_dir)? {
                let entry = entry?;
                println!("    - {}", entry.file_name().to_string_lossy().dimmed());
            }
        }
    }

    if skill.has_references {
        println!("  {}: Yes", "Has references".cyan());
        for dir_name in &["references", "resources"] {
            let refs_dir = skill.path.join(dir_name);
            if refs_dir.exists() {
                for entry in fs::read_dir(refs_dir)? {
                    let entry = entry?;
                    println!("    - {}", entry.file_name().to_string_lossy().dimmed());
                }
            }
        }
    }

    Ok(())
}

/// Discover coding agents on the system
fn discover_agents() -> Vec<PathBuf> {
    let mut agents = Vec::new();

    if let Some(home) = dirs::home_dir() {
        for agent in KNOWN_AGENTS {
            let agent_path = home.join(agent);
            if agent_path.exists() && agent_path.is_dir() {
                agents.push(agent_path);
            }
        }
    }

    agents
}

/// Link installed skills to all discovered coding agents
fn link_to_agents() -> Result<()> {
    let skills_dir = get_skills_install_dir()?;

    if !skills_dir.exists() {
        anyhow::bail!("No skills installed. Run 'skillshub install-all' first.");
    }

    let agents = discover_agents();

    if agents.is_empty() {
        println!(
            "{} No coding agents found. Looked for: {}",
            "Info:".cyan(),
            KNOWN_AGENTS.join(", ")
        );
        return Ok(());
    }

    println!(
        "{} Linking skills to {} discovered agent(s)",
        "=>".green().bold(),
        agents.len()
    );

    for agent_path in &agents {
        let agent_name = agent_path.file_name().unwrap().to_string_lossy();
        let link_path = agent_path.join(".skills");

        if link_path.exists() {
            if link_path.is_symlink() {
                println!("  {} {} (link exists)", "○".yellow(), agent_name);
            } else {
                println!(
                    "  {} {} (.skills exists but is not a symlink)",
                    "!".red(),
                    agent_name
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

/// Show discovered coding agents
fn show_agents() -> Result<()> {
    println!("{}", "Coding Agents".bold().underline());
    println!();

    let agents = discover_agents();

    if agents.is_empty() {
        println!("  No coding agents found.");
        println!();
        println!("  Looked for: {}", KNOWN_AGENTS.join(", "));
        return Ok(());
    }

    for agent_path in &agents {
        let agent_name = agent_path.file_name().unwrap().to_string_lossy();
        let link_path = agent_path.join(".skills");

        let status = if link_path.exists() {
            if link_path.is_symlink() {
                "linked".green()
            } else {
                "has .skills (not symlink)".yellow()
            }
        } else {
            "not linked".dimmed()
        };

        println!("  {} {:<15} [{}]", "●".cyan(), agent_name.bold(), status);
    }

    println!();
    println!(
        "  {} Run {} to link skills to agents",
        "Tip:".cyan(),
        "skillshub link".bold()
    );

    Ok(())
}

/// Uninstall a specific skill
fn uninstall_skill(name: &str) -> Result<()> {
    let install_dir = get_skills_install_dir()?;
    let skill_path = install_dir.join(name);

    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    fs::remove_dir_all(&skill_path)?;

    println!("{} Uninstalled '{}'", "✓".green(), name);

    Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in WalkDir::new(src).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(src)?;
        let dest_path = dst.join(relative);

        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dest_path)?;
        }
    }

    Ok(())
}
