use anyhow::Result;
use colored::Colorize;
use inquire::error::InquireError;
use inquire::{Confirm, MultiSelect, Select};
use std::io::IsTerminal;

use crate::paths::get_skills_install_dir;
use crate::registry::db::{self, init_db, save_db};
use crate::registry::models::Database;
use crate::registry::skill::remove_installed_skills_batch;
use crate::registry::tap::{get_tap_registry, list_tap_summaries, remove_tap};
use crate::registry::{update_skill, UpdateSelection};

const UPDATE_EVERYTHING: &str = "Update everything";
const QUIT: &str = "Quit";
const VIEW_SKILLS: &str = "View/manage skills";
const DELETE_TAP: &str = "Delete this tap…";
const BACK: &str = "Back";
const UNINSTALL_SELECTED: &str = "Uninstall selected";
const UPDATE_SELECTED: &str = "Update selected";
const CANCEL: &str = "Cancel";

/// Run the interactive TUI for managing taps and skills
pub fn run_tui() -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!("interactive mode requires a terminal; use subcommands directly");
    }

    loop {
        let db = init_db()?;
        let summaries = list_tap_summaries(&db);

        if summaries.is_empty() {
            println!("No taps configured. Add one with `skillshub tap add <owner/repo>`.");
            return Ok(());
        }

        let mut options: Vec<String> = summaries.iter().map(|s| s.picker_label()).collect();
        options.push(UPDATE_EVERYTHING.to_string());
        options.push(QUIT.to_string());

        match Select::new("Select a tap (Enter to view its skills)", options).prompt() {
            Ok(choice) if choice == UPDATE_EVERYTHING => update_skill(UpdateSelection::All, false)?,
            Ok(choice) if choice == QUIT => break,
            Ok(choice) => {
                if let Some(summary) = summaries.iter().find(|s| s.picker_label() == choice) {
                    tap_view(&summary.name)?;
                }
            }
            Err(InquireError::OperationCanceled) => break,
            Err(InquireError::OperationInterrupted) => std::process::exit(130),
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

/// Classify a prompt error inside a flow: Esc returns up one level, Ctrl-C exits.
fn handle_prompt_error(e: InquireError) -> Result<()> {
    match e {
        InquireError::OperationCanceled => Ok(()),
        InquireError::OperationInterrupted => std::process::exit(130),
        other => Err(other.into()),
    }
}

/// Actions available on a tap.
fn tap_view(tap_name: &str) -> Result<()> {
    let options = vec![VIEW_SKILLS, DELETE_TAP, BACK];

    match Select::new(&format!("Tap: {}", tap_name), options).prompt() {
        Ok(VIEW_SKILLS) => manage_skills(tap_name),
        Ok(DELETE_TAP) => delete_tap_flow(tap_name),
        Ok(_) => Ok(()),
        Err(e) => handle_prompt_error(e),
    }
}

/// Interactive flow to delete a whole tap (uninstalling its skills).
fn delete_tap_flow(tap_name: &str) -> Result<()> {
    let db = init_db()?;

    if db::get_tap(&db, tap_name).map(|t| t.is_default).unwrap_or(false) {
        println!("Cannot delete the default tap '{}'.", tap_name);
        return Ok(());
    }

    let prompt = delete_confirm_prompt(tap_name, db::get_skills_from_tap(&db, tap_name).len());

    let confirmed = match Confirm::new(&prompt).with_default(false).prompt() {
        Ok(c) => c,
        Err(e) => return handle_prompt_error(e),
    };

    if !confirmed {
        return Ok(());
    }

    remove_tap(tap_name, false)
}

/// Blast-radius-aware confirm message for tap deletion.
fn delete_confirm_prompt(tap_name: &str, installed_count: usize) -> String {
    if installed_count > 0 {
        format!(
            "Delete tap '{}' and uninstall its {} skill(s)?",
            tap_name, installed_count
        )
    } else {
        format!("Delete tap '{}'?", tap_name)
    }
}

/// One row in a tap's skill picker.
pub(crate) struct SkillRow {
    /// Full skill name (`owner/repo/skill`) — the db key and CLI identifier.
    pub full_name: String,
    pub installed: bool,
}

impl SkillRow {
    fn label(&self) -> String {
        if self.installed {
            format!("{} (installed)", self.full_name)
        } else {
            self.full_name.clone()
        }
    }
}

/// Rows for a tap's skill picker: union of the cached registry and the
/// installed set (so orphaned installs still appear), sorted by full name.
pub(crate) fn build_skill_rows(db: &Database, tap_name: &str) -> Vec<SkillRow> {
    let mut names: Vec<String> = get_tap_registry(db, tap_name)
        .ok()
        .and_then(|opt| opt)
        .map(|registry| {
            registry
                .skills
                .keys()
                .map(|skill| format!("{}/{}", tap_name, skill))
                .collect()
        })
        .unwrap_or_default();

    for (full_name, _) in db::get_skills_from_tap(db, tap_name) {
        if !names.contains(full_name) {
            names.push(full_name.clone());
        }
    }

    names.sort();

    names
        .into_iter()
        .map(|full_name| SkillRow {
            installed: db.installed.contains_key(&full_name),
            full_name,
        })
        .collect()
}

/// What to do with a skill selection: only installed skills are actionable.
pub(crate) enum SkillAction {
    Uninstall(Vec<String>),
    Update(Vec<String>),
}

/// Resolve a MultiSelect result + action choice into an executable plan.
/// `None` means nothing to do (empty selection or nothing installed).
pub(crate) fn resolve_skill_action(rows: &[SkillRow], selected: &[String], uninstall: bool) -> Option<SkillAction> {
    let names: Vec<String> = rows
        .iter()
        .filter(|r| r.installed && selected.contains(&r.label()))
        .map(|r| r.full_name.clone())
        .collect();

    if names.is_empty() {
        return None;
    }

    Some(if uninstall {
        SkillAction::Uninstall(names)
    } else {
        SkillAction::Update(names)
    })
}

/// Interactive flow to uninstall/update skills within one tap.
fn manage_skills(tap_name: &str) -> Result<()> {
    let db = init_db()?;
    let rows = build_skill_rows(&db, tap_name);

    if rows.is_empty() {
        println!(
            "No skills found in tap '{}'. Try `skillshub tap update {}`.",
            tap_name, tap_name
        );
        return Ok(());
    }

    let labels: Vec<String> = rows.iter().map(|r| r.label()).collect();
    let installed_indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.installed)
        .map(|(i, _)| i)
        .collect();

    let selected = match MultiSelect::new("Select skills (space to toggle, type to filter)", labels)
        .with_default(&installed_indices)
        .prompt()
    {
        Ok(s) => s,
        Err(e) => return handle_prompt_error(e),
    };

    if selected.is_empty() {
        return Ok(());
    }

    let action = match Select::new(
        "What do you want to do with the selected skills?",
        vec![UNINSTALL_SELECTED, UPDATE_SELECTED, CANCEL],
    )
    .prompt()
    {
        Ok(a) => a,
        Err(e) => return handle_prompt_error(e),
    };

    match action {
        UNINSTALL_SELECTED => {
            let plan = match resolve_skill_action(&rows, &selected, true) {
                Some(SkillAction::Uninstall(names)) => names,
                _ => {
                    println!("No installed skills selected.");
                    return Ok(());
                }
            };

            let count = plan.len();
            let confirmed = match Confirm::new(&format!("Uninstall {} skills?", count))
                .with_default(false)
                .prompt()
            {
                Ok(c) => c,
                Err(e) => return handle_prompt_error(e),
            };

            if !confirmed {
                return Ok(());
            }

            let mut db = init_db()?;
            let install_dir = get_skills_install_dir()?;
            let results = remove_installed_skills_batch(&mut db, &install_dir, &plan);

            let mut succeeded = 0usize;
            for (name, result) in &results {
                match result {
                    Ok(()) => {
                        println!("{} Uninstalled '{}'", "✓".green(), name);
                        succeeded += 1;
                    }
                    Err(e) => println!("  {} {} ({})", "✗".red(), name, e),
                }
            }

            save_db(&db)?;

            println!("Uninstalled {} of {} skills", succeeded, count);
            Ok(())
        }
        UPDATE_SELECTED => {
            let plan = match resolve_skill_action(&rows, &selected, false) {
                Some(SkillAction::Update(names)) => names,
                _ => {
                    println!("No installed skills selected.");
                    return Ok(());
                }
            };
            update_skill(UpdateSelection::Selected(plan), false)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{Database, InstalledSkill, SkillEntry, TapInfo, TapRegistry};
    use chrono::Utc;

    fn insert_tap_with_registry(db: &mut Database, tap: &str, skills: &[&str]) {
        let mut skill_map = std::collections::HashMap::new();
        for &s in skills {
            skill_map.insert(
                s.to_string(),
                SkillEntry {
                    path: format!("skills/{}", s),
                    description: None,
                    homepage: None,
                },
            );
        }
        db.taps.insert(
            tap.to_string(),
            TapInfo {
                url: format!("https://github.com/{}", tap),
                skills_path: "skills".to_string(),
                updated_at: None,
                is_default: false,
                cached_registry: Some(TapRegistry {
                    name: tap.to_string(),
                    description: None,
                    skills: skill_map,
                }),
                branch: None,
            },
        );
    }

    fn insert_installed(db: &mut Database, tap: &str, skill: &str) {
        db.installed.insert(
            format!("{}/{}", tap, skill),
            InstalledSkill {
                tap: tap.to_string(),
                skill: skill.to_string(),
                commit: None,
                installed_at: Utc::now(),
                source_url: None,
                source_path: None,
                gist_updated_at: None,
            },
        );
    }

    #[test]
    fn delete_prompt_names_blast_radius() {
        assert_eq!(
            delete_confirm_prompt("owner/repo", 3),
            "Delete tap 'owner/repo' and uninstall its 3 skill(s)?"
        );
        assert_eq!(delete_confirm_prompt("owner/repo", 0), "Delete tap 'owner/repo'?");
    }

    #[test]
    fn skill_rows_union_registry_and_installed() {
        let mut db = Database::default();
        insert_tap_with_registry(&mut db, "owner/repo", &["alpha", "beta"]);
        // beta installed and in the registry; gamma installed but orphaned (not in registry)
        insert_installed(&mut db, "owner/repo", "beta");
        insert_installed(&mut db, "owner/repo", "gamma");

        let rows = build_skill_rows(&db, "owner/repo");
        let names: Vec<&str> = rows.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, vec!["owner/repo/alpha", "owner/repo/beta", "owner/repo/gamma"]);

        assert!(!rows[0].installed);
        assert!(rows[1].installed);
        assert!(rows[2].installed);
    }

    #[test]
    fn skill_rows_empty_without_registry() {
        let db = Database::default();
        assert!(build_skill_rows(&db, "missing/tap").is_empty());
    }

    #[test]
    fn resolve_action_filters_to_installed() {
        let mut db = Database::default();
        insert_tap_with_registry(&mut db, "owner/repo", &["alpha", "beta"]);
        insert_installed(&mut db, "owner/repo", "beta");

        let rows = build_skill_rows(&db, "owner/repo");
        let selected: Vec<String> = rows.iter().map(|r| r.label()).collect();

        match resolve_skill_action(&rows, &selected, true) {
            Some(SkillAction::Uninstall(names)) => assert_eq!(names, vec!["owner/repo/beta"]),
            _ => panic!("expected uninstall of installed skill only"),
        }

        match resolve_skill_action(&rows, &selected, false) {
            Some(SkillAction::Update(names)) => assert_eq!(names, vec!["owner/repo/beta"]),
            _ => panic!("expected update of installed skill only"),
        }
    }

    #[test]
    fn resolve_action_none_when_nothing_installed_selected() {
        let mut db = Database::default();
        insert_tap_with_registry(&mut db, "owner/repo", &["alpha"]);

        let rows = build_skill_rows(&db, "owner/repo");
        let selected: Vec<String> = rows.iter().map(|r| r.label()).collect();

        assert!(resolve_skill_action(&rows, &selected, true).is_none());
        assert!(resolve_skill_action(&rows, &[], true).is_none());
    }
}
