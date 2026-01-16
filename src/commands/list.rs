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
    let source_dir = get_embedded_skills_dir()?;
    let install_dir = get_skills_install_dir()?;

    let available_skills = discover_skills(&source_dir)?;
    let installed_skills = discover_skills(&install_dir)?;

    let installed_names: HashSet<_> = installed_skills.iter().map(|s| &s.name).collect();

    if available_skills.is_empty() {
        println!("No skills found.");
        return Ok(());
    }

    let rows: Vec<SkillRow> = available_skills
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
        "{} installed, {} available",
        installed_skills.len().to_string().green(),
        available_skills.len()
    );

    Ok(())
}
