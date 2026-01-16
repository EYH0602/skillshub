use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

use crate::paths::{get_embedded_skills_dir, get_skills_install_dir};
use crate::skill::discover_skills;
use crate::util::copy_dir_recursive;

/// Install all skills to ~/.skillshub
pub fn install_all() -> Result<()> {
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
pub fn install_skill(name: &str) -> Result<()> {
    let install_dir = get_skills_install_dir()?;
    let dest = install_dir.join(name);

    // Check if already installed
    if dest.exists() {
        println!(
            "{} Skill '{}' is already installed at {}",
            "Info:".cyan(),
            name,
            dest.display()
        );
        return Ok(());
    }

    // Try to find the skill in embedded/source directory
    let source_dir = get_embedded_skills_dir().with_context(|| {
        format!(
            "Skill '{}' is not installed and no source directory found.\n\
             Run 'skillshub install' from the skillshub repository directory,\n\
             or use 'skillshub install-all' to install all available skills.",
            name
        )
    })?;

    let skills = discover_skills(&source_dir)?;
    let skill = skills
        .iter()
        .find(|s| s.name == name)
        .with_context(|| format!("Skill '{}' not found in {}", name, source_dir.display()))?;

    fs::create_dir_all(&install_dir)?;

    copy_dir_recursive(&skill.path, &dest)?;

    println!(
        "{} Installed '{}' to {}",
        "✓".green(),
        skill.name,
        dest.display()
    );

    Ok(())
}

/// Uninstall a specific skill
pub fn uninstall_skill(name: &str) -> Result<()> {
    let install_dir = get_skills_install_dir()?;
    let skill_path = install_dir.join(name);

    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' is not installed", name);
    }

    fs::remove_dir_all(&skill_path)?;

    println!("{} Uninstalled '{}'", "✓".green(), name);

    Ok(())
}
