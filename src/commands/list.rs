use anyhow::Result;
use colored::Colorize;
use std::collections::HashSet;
use tabled::{
    settings::{Padding, Style},
    Table,
};

use crate::paths::{get_embedded_skills_dir, get_skills_install_dir};
use crate::skill::{discover_skills, SkillRow};
use crate::util::truncate_string;

/// List all available skills
pub fn list_skills() -> Result<()> {
    let install_dir = get_skills_install_dir()?;

    // Try to get embedded skills (only works in dev or with bundled skills)
    let source_skills = get_embedded_skills_dir()
        .ok()
        .and_then(|dir| discover_skills(&dir).ok())
        .unwrap_or_default();

    let installed_skills = discover_skills(&install_dir)?;

    // Merge both sources: installed skills + any source-only skills
    let installed_names: HashSet<_> = installed_skills.iter().map(|s| &s.name).collect();

    // Build combined list: all installed + source-only skills
    let mut all_skills = installed_skills.clone();
    for skill in &source_skills {
        if !installed_names.contains(&skill.name) {
            all_skills.push(skill.clone());
        }
    }

    // Sort by name for consistent display
    all_skills.sort_by(|a, b| a.name.cmp(&b.name));

    if all_skills.is_empty() {
        println!("No skills found. Install skills with 'skillshub install-all' first.");
        return Ok(());
    }

    let rows: Vec<SkillRow> = all_skills
        .iter()
        .map(|skill| {
            let status = if installed_names.contains(&skill.name) {
                "✓"
            } else {
                "○"
            };

            let extras = format!(
                "{}{}",
                if skill.has_scripts { "scripts" } else { "" },
                if skill.has_references {
                    if skill.has_scripts {
                        ", refs"
                    } else {
                        "refs"
                    }
                } else {
                    ""
                }
            );

            SkillRow {
                status,
                name: skill.name.clone(),
                description: truncate_string(&skill.description, 60),
                extras,
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
        "{} installed, {} total",
        installed_skills.len().to_string().green(),
        all_skills.len()
    );

    Ok(())
}
