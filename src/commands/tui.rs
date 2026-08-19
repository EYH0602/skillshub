use anyhow::Result;
use colored::Colorize;
use inquire::error::InquireError;
use inquire::{Confirm, MultiSelect, Select};
use std::io::IsTerminal;

use crate::paths::get_skills_install_dir;
use crate::registry::db::{init_db, save_db};
use crate::registry::skill::remove_installed_skills_batch;
use crate::registry::{update_skill, UpdateSelection};

const ALL_SKILLS: &str = "All skills";

/// Run the interactive TUI for managing skills
pub fn run_tui() -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!("interactive mode requires a terminal; use subcommands directly");
    }

    loop {
        match Select::new(
            "What do you want to do?",
            vec!["Uninstall skills", "Update skills", "Quit"],
        )
        .prompt()
        {
            Ok("Uninstall skills") => uninstall_flow()?,
            Ok("Update skills") => update_flow()?,
            Ok(_) => break,
            Err(InquireError::OperationCanceled) => break,
            Err(InquireError::OperationInterrupted) => std::process::exit(130),
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

/// Classify a prompt error inside a flow: Esc returns to the menu, Ctrl-C exits.
fn handle_prompt_error(e: InquireError) -> Result<()> {
    match e {
        InquireError::OperationCanceled => Ok(()),
        InquireError::OperationInterrupted => std::process::exit(130),
        other => Err(other.into()),
    }
}

/// Interactive flow to uninstall selected skills
fn uninstall_flow() -> Result<()> {
    let db = init_db()?;
    let mut names: Vec<String> = db.installed.keys().cloned().collect();
    names.sort();

    if names.is_empty() {
        println!("No skills installed.");
        return Ok(());
    }

    let selected = match MultiSelect::new("Select skills to uninstall", names).prompt() {
        Ok(s) => s,
        Err(e) => return handle_prompt_error(e),
    };

    if selected.is_empty() {
        return Ok(());
    }

    let count = selected.len();
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
    let results = remove_installed_skills_batch(&mut db, &install_dir, &selected);

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

/// Interactive flow to update selected skills
fn update_flow() -> Result<()> {
    let db = init_db()?;
    let mut names: Vec<String> = db.installed.keys().cloned().collect();
    names.sort();

    if names.is_empty() {
        println!("No skills installed.");
        return Ok(());
    }

    let mut options = vec![ALL_SKILLS.to_string()];
    options.extend(names);

    let selected = match MultiSelect::new("Select skills to update", options).prompt() {
        Ok(s) => s,
        Err(e) => return handle_prompt_error(e),
    };

    match resolve_update_selection(&selected) {
        Some(selection) => update_skill(selection, false),
        None => Ok(()),
    }
}

/// Resolve a MultiSelect result into an `UpdateSelection`.
///
/// `None` means an empty selection (no-op). `Some(All)` wins if the
/// "All skills" entry is present; otherwise `Some(Selected(names))`.
pub(crate) fn resolve_update_selection(selected: &[String]) -> Option<UpdateSelection> {
    if selected.is_empty() {
        return None;
    }
    if selected.iter().any(|s| s == ALL_SKILLS) {
        return Some(UpdateSelection::All);
    }
    Some(UpdateSelection::Selected(selected.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_selection_is_noop() {
        assert_eq!(resolve_update_selection(&[]), None);
    }

    #[test]
    fn all_skills_wins_over_individuals() {
        let selected = vec![ALL_SKILLS.to_string(), "owner/repo/skill".to_string()];
        assert_eq!(resolve_update_selection(&selected), Some(UpdateSelection::All));
    }

    #[test]
    fn individuals_only_become_selected() {
        let selected = vec!["owner/repo/a".to_string(), "owner/repo/b".to_string()];
        assert_eq!(
            resolve_update_selection(&selected),
            Some(UpdateSelection::Selected(vec![
                "owner/repo/a".to_string(),
                "owner/repo/b".to_string()
            ]))
        );
    }
}
