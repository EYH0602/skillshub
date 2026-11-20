use anyhow::{ensure, Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use crate::agent::{discover_agents, AgentInfo, KNOWN_AGENTS};
use crate::paths::get_home_dir;
use crate::registry::db::{get_all_external_skills, init_db, is_external_skill, remove_external_skill, save_db};
use crate::registry::models::{Database, ExternalSkill};

const SHARED_SKILLS_DIR: &str = ".agents/skills";
pub(super) const SHARED_SKILLS_SOURCE: &str = "~/.agents/skills";

#[derive(Tabled)]
struct ExternalSkillRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Source")]
    source_agent: String,
    #[tabled(rename = "Source Path")]
    source_path: String,
    #[tabled(rename = "Discovered")]
    discovered: String,
}

/// List all discovered external skills
pub fn external_list() -> Result<()> {
    let db = init_db()?;
    let external_skills = get_all_external_skills(&db);

    if external_skills.is_empty() {
        println!("{} No external skills discovered yet.", "Info:".cyan());
        println!("Run 'skillshub link' or 'skillshub external scan' to discover external skills.");
        return Ok(());
    }

    println!(
        "{} External Skills (managed elsewhere, synced by skillshub):\n",
        "=>".green().bold()
    );

    let mut rows: Vec<ExternalSkillRow> = external_skills
        .iter()
        .map(|(_, skill)| ExternalSkillRow {
            name: skill.name.clone(),
            source_agent: skill.source_agent.clone(),
            source_path: skill.source_path.display().to_string(),
            discovered: skill.discovered_at.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect();

    rows.sort_by(|a, b| a.name.cmp(&b.name));

    let table = Table::new(rows)
        .with(Style::rounded())
        .with(Padding::new(1, 1, 0, 1))
        .to_string();
    println!("{}", table);

    Ok(())
}

/// Scan the global shared store and agent directories for external skills.
pub fn external_scan() -> Result<()> {
    let mut db = init_db()?;

    let agents = discover_agents();

    let shared_dir = global_shared_skills_dir();
    println!(
        "{} Scanning the shared store and {} agent(s) for external skills...",
        "=>".green().bold(),
        agents.len()
    );

    let new_external = discover_external_skills(&agents, &mut db, shared_dir.as_deref())?;

    if new_external.is_empty() {
        println!(
            "{} No new external skills discovered. Total tracked: {}",
            "Info:".cyan(),
            db.external.len()
        );
    } else {
        println!(
            "{} Discovered {} new external skill(s):",
            "=>".green().bold(),
            new_external.len()
        );
        for name in &new_external {
            if let Some(ext) = db.external.get(name) {
                println!("  {} {} (from {})", "+".green(), name, ext.source_agent);
            }
        }
        save_db(&db)?;
        println!(
            "\n{} Total external skills tracked: {}",
            "Done!".green().bold(),
            db.external.len()
        );
    }

    Ok(())
}

/// Stop tracking an external skill
pub fn external_forget(name: &str) -> Result<()> {
    let mut db = init_db()?;

    if !is_external_skill(&db, name) {
        anyhow::bail!("External skill '{}' not found", name);
    }

    let removed = remove_external_skill(&mut db, name);
    save_db(&db)?;

    if let Some(skill) = removed {
        println!(
            "{} Stopped tracking external skill '{}' (was from {})",
            "Done!".green().bold(),
            name,
            skill.source_agent
        );
        println!(
            "{} The skill itself was not deleted. Symlinks in other agents will remain until removed.",
            "Note:".cyan()
        );
    }

    Ok(())
}

/// Do not adopt a shared store redirected to another scope by a symlink.
pub(super) fn global_shared_skills_dir() -> Option<PathBuf> {
    let expected = get_home_dir()?.canonicalize().ok()?.join(SHARED_SKILLS_DIR);
    let resolved = expected.canonicalize().ok()?;
    (resolved == expected && resolved.is_dir()).then_some(resolved)
}

fn has_skill_file(dir: &Path) -> bool {
    fs::symlink_metadata(dir.join("SKILL.md")).is_ok_and(|metadata| metadata.is_file())
}

pub(super) fn is_current_shared_skill(path: &Path, shared_dir: Option<&Path>) -> bool {
    shared_dir.is_some_and(|dir| path.parent() == Some(dir))
        && fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
        && has_skill_file(path)
}

fn visit_directories(root: &Path, mut visit: impl FnMut(fs::DirEntry)) -> Result<()> {
    if root.is_dir() {
        for entry in fs::read_dir(root).with_context(|| format!("Scanning {}", root.display()))? {
            let entry = entry?;
            // DirEntry::file_type does not follow symlinks.
            if entry.file_type()?.is_dir() {
                visit(entry);
            }
        }
    }
    Ok(())
}

pub(super) fn discover_external_skills(
    agents: &[AgentInfo],
    db: &mut Database,
    shared_dir: Option<&Path>,
) -> Result<Vec<String>> {
    let mut new_external = Vec::new();
    let managed_names: HashSet<&str> = db.installed.values().map(|skill| skill.skill.as_str()).collect();
    let mut scan = |root: &Path, source: &str| {
        visit_directories(root, |entry| {
            let path = entry.path();
            if source == SHARED_SKILLS_SOURCE && !has_skill_file(&path) {
                return;
            }
            let source_path = path.canonicalize().unwrap_or(path);
            // Real directories reached through an alias of their parent have
            // the same canonical name, so the name check also deduplicates them.
            let Some(name) = source_path.file_name() else {
                return;
            };
            let name = name.to_string_lossy();
            if managed_names.contains(name.as_ref()) || db.external.contains_key(name.as_ref()) {
                return;
            }
            let name = name.into_owned();
            db.external.insert(
                name.clone(),
                ExternalSkill {
                    name: name.clone(),
                    source_agent: source.to_owned(),
                    source_path,
                    discovered_at: Utc::now(),
                },
            );
            new_external.push(name);
        })
    };

    if let Some(dir) = shared_dir {
        scan(dir, SHARED_SKILLS_SOURCE)?;
    }
    for agent in agents {
        scan(&agent.path.join(agent.skills_subdir), agent.name)?;
    }
    Ok(new_external)
}

/// List project skills without reading or updating global state.
pub fn external_scan_project(project: &Path) -> Result<()> {
    let project = project
        .canonicalize()
        .with_context(|| format!("Resolving project directory {}", project.display()))?;
    ensure!(project.is_dir(), "Not a project directory: {}", project.display());
    let mut skills = BTreeSet::new();
    let mut scan = |root: PathBuf| -> Result<()> {
        let Ok(root) = root.canonicalize() else {
            return Ok(());
        };
        if root.starts_with(&project) {
            visit_directories(&root, |entry| {
                let path = entry.path();
                if has_skill_file(&path) {
                    skills.insert(path);
                }
            })?;
        }
        Ok(())
    };

    scan(project.join(SHARED_SKILLS_DIR))?;
    for &(_, agent_dir, subdir) in KNOWN_AGENTS {
        scan(project.join(agent_dir).join(subdir))?;
    }
    // Vercel uses these project paths for OpenClaw and Kiro CLI.
    scan(project.join("skills"))?;
    scan(project.join(".kiro/skills"))?;

    if skills.is_empty() {
        println!("No external project skills found in {}.", project.display());
    } else {
        println!("External project skills in {}:", project.display());
        for path in skills {
            println!("  {}", path.strip_prefix(&project)?.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_skill_dir(path: &std::path::Path) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            "---\nname: test\ndescription: Test\n---\n# Test\n",
        )
        .unwrap();
    }

    #[test]
    fn test_external_skill_row_creation() {
        let row = ExternalSkillRow {
            name: "test-skill".to_string(),
            source_agent: ".claude".to_string(),
            source_path: "/home/user/.claude/skills/test-skill".to_string(),
            discovered: "2024-01-17 10:00".to_string(),
        };

        assert_eq!(row.name, "test-skill");
        assert_eq!(row.source_agent, ".claude");
    }

    #[test]
    fn test_discover_external_skills_empty() {
        let temp = TempDir::new().unwrap();
        let mut db = Database::default();
        let new_external = discover_external_skills(&[], &mut db, Some(temp.path())).unwrap();

        assert!(new_external.is_empty());
        assert!(db.external.is_empty());
    }

    #[test]
    fn test_discover_external_skills_finds_real_dirs() {
        let temp = TempDir::new().unwrap();

        // Create a mock agent directory with an external skill
        let agent_path = temp.path().join(".claude");
        let skills_path = agent_path.join("skills");
        let external_skill_path = skills_path.join("my-external-skill");
        create_skill_dir(&external_skill_path);

        let agents = vec![AgentInfo {
            name: ".claude",
            path: agent_path,
            skills_subdir: "skills",
        }];

        let mut db = Database::default();
        let new_external = discover_external_skills(&agents, &mut db, None).unwrap();
        assert_eq!(new_external, ["my-external-skill"]);
        assert_eq!(
            db.external["my-external-skill"].source_path,
            external_skill_path.canonicalize().unwrap()
        );
    }

    #[test]
    fn shared_source_precedes_agent_copy_but_preserves_tracked_names() {
        let temp = TempDir::new().unwrap();
        let shared = temp.path().join(".agents/skills");
        let agent = AgentInfo {
            name: ".claude",
            path: temp.path().join(".claude"),
            skills_subdir: "skills",
        };
        let agent_skills = agent.path.join(agent.skills_subdir);
        create_skill_dir(&agent_skills.join("tracked"));
        create_skill_dir(&shared.join("tracked"));
        let mut db = Database::default();
        discover_external_skills(std::slice::from_ref(&agent), &mut db, None).unwrap();

        create_skill_dir(&agent_skills.join("new-skill"));
        create_skill_dir(&shared.join("new-skill"));
        assert_eq!(
            discover_external_skills(&[agent], &mut db, Some(&shared)).unwrap(),
            ["new-skill"]
        );
        assert_eq!(
            db.external["tracked"].source_path,
            agent_skills.join("tracked").canonicalize().unwrap()
        );
        assert_eq!(
            db.external["new-skill"].source_path,
            shared.join("new-skill").canonicalize().unwrap()
        );
    }
}
