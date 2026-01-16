use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

use crate::paths::{get_embedded_skills_dir, get_skills_install_dir};
use crate::skill::discover_skills;

/// Show detailed information about a skill
pub fn show_skill_info(name: &str) -> Result<()> {
    let source_dir = get_embedded_skills_dir()?;
    let install_dir = get_skills_install_dir()?;

    // Check installed location first, then fall back to source
    let installed_skills = discover_skills(&install_dir)?;
    let source_skills = discover_skills(&source_dir)?;

    let installed_skill = installed_skills.iter().find(|s| s.name == name);
    let source_skill = source_skills.iter().find(|s| s.name == name);

    // Prefer installed skill for display, but need source to know if it's available
    let skill = installed_skill
        .or(source_skill)
        .with_context(|| format!("Skill '{}' not found", name))?;

    let is_installed = installed_skill.is_some();

    println!("{}", skill.name.bold().underline());
    println!();
    println!("  {}: {}", "Description".cyan(), skill.description);
    println!(
        "  {}: {}",
        "Status".cyan(),
        if is_installed {
            "Installed".green()
        } else {
            "Not installed".yellow()
        }
    );
    println!("  {}: {}", "Location".cyan(), skill.path.display());

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
