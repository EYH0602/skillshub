use anyhow::Result;
use colored::Colorize;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::Print;
use crossterm::{cursor, queue, terminal};
use inquire::error::InquireError;
use inquire::{Confirm, MultiSelect, Select};
use std::io::{IsTerminal, Write};

use crate::paths::get_skills_install_dir;
use crate::registry::db::{self, init_db, save_db};
use crate::registry::models::Database;
use crate::registry::skill::remove_installed_skills_batch;
use crate::registry::tap::{get_tap_registry, list_tap_summaries, remove_tap, TapSummary};
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

        // Operational errors are reported and the session stays alive; only the
        // genuinely fatal ones (init_db, the picker itself) propagate.
        let action_result = match tap_picker(&summaries)? {
            PickerOutcome::Drill(name) => tap_view(&name),
            PickerOutcome::Delete(name) => delete_tap_flow(&name).map(|_| ()),
            PickerOutcome::UpdateAll => update_skill(UpdateSelection::All, false),
            PickerOutcome::Quit => break,
            PickerOutcome::Interrupted => unreachable!("tap_picker exits on Interrupted"),
        };
        if let Err(e) = action_result {
            eprintln!("{} {:#}", "✗".red(), e);
        }
    }

    Ok(())
}

/// One row in the top-level tap picker.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PickerRow {
    Tap(String),
    UpdateEverything,
    Quit,
}

/// What the tap picker resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PickerOutcome {
    /// Drill into a tap's view (skill management / tap deletion).
    Drill(String),
    /// Delete the focused tap outright (after confirmation).
    Delete(String),
    UpdateAll,
    Quit,
    /// Ctrl-C pressed: restore the terminal first, then exit like SIGINT (130).
    Interrupted,
}

fn picker_rows(summaries: &[TapSummary]) -> Vec<PickerRow> {
    let mut rows: Vec<PickerRow> = summaries.iter().map(|s| PickerRow::Tap(s.name.clone())).collect();
    rows.push(PickerRow::UpdateEverything);
    rows.push(PickerRow::Quit);
    rows
}

/// If the focused row is a tap, resolve to deleting it; otherwise no-op.
fn delete_focused(rows: &[PickerRow], cursor: usize) -> Option<PickerOutcome> {
    match &rows[cursor] {
        PickerRow::Tap(name) => Some(PickerOutcome::Delete(name.clone())),
        _ => None,
    }
}

/// Pure key handling for the tap picker. `Some` ends the picker with an
/// outcome; `None` continues (possibly after moving the cursor).
fn handle_picker_key(rows: &[PickerRow], cursor: &mut usize, key: KeyEvent) -> Option<PickerOutcome> {
    // In raw mode Ctrl combos arrive as Char with CONTROL set (Ctrl-C is not
    // SIGINT). Plain-character bindings must not fire on them: Ctrl-D is a
    // habitual "get me out" gesture and must not trigger tap deletion.
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Some(PickerOutcome::Interrupted),
        KeyCode::Up => {
            *cursor = cursor.saturating_sub(1);
            None
        }
        KeyCode::Char('k') if !ctrl => {
            *cursor = cursor.saturating_sub(1);
            None
        }
        KeyCode::Down => {
            if *cursor + 1 < rows.len() {
                *cursor += 1;
            }
            None
        }
        KeyCode::Char('j') if !ctrl => {
            if *cursor + 1 < rows.len() {
                *cursor += 1;
            }
            None
        }
        KeyCode::Enter => Some(match &rows[*cursor] {
            PickerRow::Tap(name) => PickerOutcome::Drill(name.clone()),
            PickerRow::UpdateEverything => PickerOutcome::UpdateAll,
            PickerRow::Quit => PickerOutcome::Quit,
        }),
        KeyCode::Delete | KeyCode::Backspace => delete_focused(rows, *cursor),
        KeyCode::Char('d') if !ctrl => delete_focused(rows, *cursor),
        KeyCode::Esc => Some(PickerOutcome::Quit),
        KeyCode::Char('q') if !ctrl => Some(PickerOutcome::Quit),
        _ => None,
    }
}

const HEADER: &str = "Select a tap (↑/↓ or j/k move, Enter view, d/Del/⌫ delete, q quit)";

/// Truncate to at most `max` chars so a row never wraps to a second physical
/// line — the redraw accounting counts one line per printed row.
fn truncate_to_width(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars()
            .take(max.saturating_sub(1))
            .chain(std::iter::once('…'))
            .collect()
    }
}

/// Render one picker frame and return how many physical lines it occupies.
///
/// The returned count must equal the number of `\r\n` emitted: the next frame
/// rewinds with `MoveUp(prev_lines)` + `Clear(FromCursorDown)`, so a mismatch
/// would erase the user's scrollback above the picker. Rows are truncated to
/// the terminal width (no wrapping) and windowed to a viewport around the
/// cursor that never exceeds the terminal height.
fn render_picker(
    out: &mut impl Write,
    summaries: &[TapSummary],
    rows: &[PickerRow],
    cursor: usize,
    prev_lines: u16,
    term_size: (u16, u16),
) -> Result<u16> {
    let (cols, term_rows) = term_size;
    let width = (cols as usize).max(1);
    // Reserve one line for the header and one for the trailing newline: every
    // row ends with \r\n, so a frame of exactly term_rows lines scrolls the
    // terminal, pushing the header off and desyncing the MoveUp rewind.
    let max_rows = (term_rows as usize).saturating_sub(2).max(1);

    // Scroll the window only when the cursor moves past its bottom edge.
    let start = if cursor >= max_rows { cursor + 1 - max_rows } else { 0 };
    let end = (start + max_rows).min(rows.len());

    if prev_lines > 0 {
        queue!(out, cursor::MoveUp(prev_lines))?;
    }
    queue!(out, terminal::Clear(terminal::ClearType::FromCursorDown))?;
    queue!(out, Print(format!("{}\r\n", truncate_to_width(HEADER, width))))?;
    let mut lines: u16 = 1;
    for (i, row) in rows.iter().enumerate().take(end).skip(start) {
        let label = match row {
            PickerRow::Tap(name) => summaries
                .iter()
                .find(|s| &s.name == name)
                .map(|s| s.picker_label())
                .unwrap_or_else(|| name.clone()),
            PickerRow::UpdateEverything => UPDATE_EVERYTHING.to_string(),
            PickerRow::Quit => QUIT.to_string(),
        };
        let marker = if i == cursor { ">" } else { " " };
        queue!(
            out,
            Print(format!(
                "{}\r\n",
                truncate_to_width(&format!("{} {}", marker, label), width)
            ))
        )?;
        lines += 1;
    }
    out.flush()?;
    Ok(lines)
}

/// RAII guard restoring the terminal to cooked mode on drop, so a panic
/// inside the picker can't leave the user's shell in raw mode.
struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// Top-level tap picker with row-level key actions (inquire cannot bind
/// arbitrary keys to list rows, so this level uses crossterm directly).
fn tap_picker(summaries: &[TapSummary]) -> Result<PickerOutcome> {
    let rows = picker_rows(summaries);
    let mut cursor = 0usize;
    let mut stdout = std::io::stdout();

    let guard = RawModeGuard::new()?;
    let result = picker_event_loop(&mut stdout, summaries, &rows, &mut cursor);
    // Restore the terminal before any exit path.
    drop(guard);

    if matches!(result, Ok(PickerOutcome::Interrupted)) {
        std::process::exit(130);
    }

    result
}

fn picker_event_loop(
    out: &mut impl Write,
    summaries: &[TapSummary],
    rows: &[PickerRow],
    cursor: &mut usize,
) -> Result<PickerOutcome> {
    let term_size = || terminal::size().unwrap_or((80, 24));
    let mut prev_lines = render_picker(out, summaries, rows, *cursor, 0, term_size())?;
    loop {
        match event::read()? {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if let Some(outcome) = handle_picker_key(rows, cursor, key) {
                    queue!(
                        out,
                        cursor::MoveUp(prev_lines),
                        terminal::Clear(terminal::ClearType::FromCursorDown)
                    )?;
                    out.flush()?;
                    return Ok(outcome);
                }
                prev_lines = render_picker(out, summaries, rows, *cursor, prev_lines, term_size())?;
            }
            // Redraw on resize so the frame matches the new dimensions.
            Event::Resize(_, _) => {
                prev_lines = render_picker(out, summaries, rows, *cursor, prev_lines, term_size())?;
            }
            _ => {}
        }
    }
}

/// Classify a prompt error inside a flow: Esc cancels the current prompt
/// (back to the parent view), Ctrl-C exits.
fn handle_prompt_error(e: InquireError) -> Result<()> {
    match e {
        InquireError::OperationCanceled => Ok(()),
        InquireError::OperationInterrupted => std::process::exit(130),
        other => Err(other.into()),
    }
}

/// Actions available on a tap. Loops so that Esc inside a child flow
/// (skill manager, delete confirm) redisplays this view — Esc backs up
/// exactly one level.
fn tap_view(tap_name: &str) -> Result<()> {
    loop {
        let options = vec![VIEW_SKILLS, DELETE_TAP, BACK];

        match Select::new(&format!("Tap: {}", tap_name), options).prompt() {
            Ok(VIEW_SKILLS) => manage_skills(tap_name)?,
            Ok(DELETE_TAP) => {
                if delete_tap_flow(tap_name)? {
                    // The tap is gone; back out to the tap list.
                    return Ok(());
                }
            }
            Ok(_) => return Ok(()),
            Err(e) => return handle_prompt_error(e),
        }
    }
}

/// Interactive flow to delete a whole tap (uninstalling its skills).
/// Returns `true` when the tap was actually deleted.
fn delete_tap_flow(tap_name: &str) -> Result<bool> {
    let db = init_db()?;

    if db::get_tap(&db, tap_name).map(|t| t.is_default).unwrap_or(false) {
        println!("Cannot delete the default tap '{}'.", tap_name);
        return Ok(false);
    }

    let prompt = delete_confirm_prompt(tap_name, db::get_skills_from_tap(&db, tap_name).len());

    let confirmed = match Confirm::new(&prompt).with_default(false).prompt() {
        Ok(c) => c,
        Err(e) => {
            handle_prompt_error(e)?;
            return Ok(false);
        }
    };

    if !confirmed {
        return Ok(false);
    }

    remove_tap(tap_name, false)?;
    Ok(true)
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
        vec![CANCEL, UPDATE_SELECTED, UNINSTALL_SELECTED],
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
            let confirmed = match Confirm::new(&format!("Uninstall {} skill(s): {}?", count, plan.join(", ")))
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

            // Persist before announcing: if the save fails, the success lines
            // must not have been printed yet, and the session stays alive with
            // an actionable message.
            if let Err(e) = save_db(&db) {
                eprintln!(
                    "{} Skills were removed from disk but the database could not be updated: {}\n  Re-run the uninstall once the error is fixed to reconcile.",
                    "✗".red(),
                    e
                );
                return Ok(());
            }

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

            println!("Uninstalled {} of {} skills", succeeded, count);
            if succeeded < count {
                println!("  {} Re-run to retry the failed skill(s).", "!".yellow());
            }
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

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_rows() -> Vec<PickerRow> {
        vec![
            PickerRow::Tap("a/one".to_string()),
            PickerRow::Tap("b/two".to_string()),
            PickerRow::UpdateEverything,
            PickerRow::Quit,
        ]
    }

    #[test]
    fn delete_key_on_tap_row_deletes_that_tap() {
        let rows = test_rows();
        let mut cursor = 1;

        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Delete)),
            Some(PickerOutcome::Delete("b/two".to_string()))
        );

        let mut cursor = 0;
        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Char('d'))),
            Some(PickerOutcome::Delete("a/one".to_string()))
        );

        let mut cursor = 1;
        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Backspace)),
            Some(PickerOutcome::Delete("b/two".to_string()))
        );
    }

    #[test]
    fn delete_key_on_non_tap_rows_is_noop() {
        let rows = test_rows();

        let mut cursor = 2;
        assert_eq!(handle_picker_key(&rows, &mut cursor, key(KeyCode::Delete)), None);
        assert_eq!(handle_picker_key(&rows, &mut cursor, key(KeyCode::Backspace)), None);
        assert_eq!(handle_picker_key(&rows, &mut cursor, key(KeyCode::Char('d'))), None);

        let mut cursor = 3;
        assert_eq!(handle_picker_key(&rows, &mut cursor, key(KeyCode::Delete)), None);
    }

    #[test]
    fn enter_resolves_row_outcomes() {
        let rows = test_rows();

        let mut cursor = 0;
        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Enter)),
            Some(PickerOutcome::Drill("a/one".to_string()))
        );

        let mut cursor = 2;
        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Enter)),
            Some(PickerOutcome::UpdateAll)
        );

        let mut cursor = 3;
        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Enter)),
            Some(PickerOutcome::Quit)
        );
    }

    #[test]
    fn movement_clamps_to_row_bounds() {
        let rows = test_rows();
        let mut cursor = 0;

        assert_eq!(handle_picker_key(&rows, &mut cursor, key(KeyCode::Up)), None);
        assert_eq!(cursor, 0);

        for _ in 0..10 {
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Char('j')));
        }
        assert_eq!(cursor, rows.len() - 1);

        handle_picker_key(&rows, &mut cursor, key(KeyCode::Char('k')));
        assert_eq!(cursor, rows.len() - 2);
    }

    #[test]
    fn esc_and_q_quit() {
        let rows = test_rows();
        let mut cursor = 0;

        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Esc)),
            Some(PickerOutcome::Quit)
        );
        assert_eq!(
            handle_picker_key(&rows, &mut cursor, key(KeyCode::Char('q'))),
            Some(PickerOutcome::Quit)
        );
    }

    #[test]
    fn ctrl_c_returns_interrupted_instead_of_exiting() {
        let rows = test_rows();
        let mut cursor = 0;

        assert_eq!(
            handle_picker_key(
                &rows,
                &mut cursor,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            Some(PickerOutcome::Interrupted)
        );
    }

    #[test]
    fn ctrl_modified_chars_do_not_fire_plain_char_bindings() {
        let rows = test_rows();

        for ch in ['d', 'q', 'j', 'k'] {
            let mut cursor = 0;
            assert_eq!(
                handle_picker_key(
                    &rows,
                    &mut cursor,
                    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
                ),
                None,
                "Ctrl-{} must not trigger the plain '{}' binding",
                ch,
                ch
            );
            assert_eq!(cursor, 0, "Ctrl-{} must not move the cursor", ch);
        }
    }

    fn summary(name: &str) -> TapSummary {
        TapSummary {
            name: name.to_string(),
            is_default: false,
            installed_count: 0,
            available_count: None,
        }
    }

    #[test]
    fn render_line_count_matches_emitted_lines() {
        let summaries = vec![summary("a/one"), summary("b/two")];
        let rows = test_rows();
        let mut out = Vec::new();

        let lines = render_picker(&mut out, &summaries, &rows, 1, 0, (80, 24)).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert_eq!(lines as usize, rows.len() + 1);
        assert_eq!(
            text.matches("\r\n").count(),
            lines as usize,
            "returned count must equal the number of emitted lines"
        );
        assert!(text.contains("> b/two"), "focused row must carry the '>' marker");
    }

    #[test]
    fn render_windows_rows_to_terminal_height() {
        let summaries: Vec<TapSummary> = (0..10).map(|i| summary(&format!("t{}/tap", i))).collect();
        let rows: Vec<PickerRow> = summaries.iter().map(|s| PickerRow::Tap(s.name.clone())).collect();
        let mut out = Vec::new();

        // Terminal height 5 => header + a 3-row window; one line stays free so
        // the trailing \r\n never scrolls the terminal.
        let lines = render_picker(&mut out, &summaries, &rows, 7, 0, (80, 5)).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert_eq!(lines, 4);
        assert!(text.contains("> t7/tap"), "focused row must be in the viewport");
        assert!(text.contains("t5/tap"));
        assert!(!text.contains("t4/tap"), "rows above the window must not render");
    }

    #[test]
    fn render_never_emits_a_full_screen_of_lines() {
        // A frame of exactly term_rows lines scrolls on the trailing \r\n and
        // desyncs the MoveUp rewind, so the invariant is lines <= term_rows - 1.
        let summaries: Vec<TapSummary> = (0..30).map(|i| summary(&format!("t{}/tap", i))).collect();
        let rows: Vec<PickerRow> = summaries.iter().map(|s| PickerRow::Tap(s.name.clone())).collect();

        for term_rows in [3u16, 5, 24, 50] {
            let mut out = Vec::new();
            let lines = render_picker(&mut out, &summaries, &rows, 25, 0, (80, term_rows)).unwrap();
            assert!(
                lines < term_rows,
                "frame of {} lines fills a {}-row terminal and scrolls",
                lines,
                term_rows
            );
        }
    }

    #[test]
    fn render_truncates_rows_to_terminal_width() {
        let summaries = vec![summary("a-very-long-owner-name/a-very-long-tap-name")];
        let rows = vec![PickerRow::Tap(summaries[0].name.clone())];
        let mut out = Vec::new();

        let lines = render_picker(&mut out, &summaries, &rows, 0, 0, (20, 24)).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert_eq!(lines, 2, "truncated rows still occupy exactly one line each");
        // First segment also carries the Clear escape sequence; check the row line.
        let row = text.split("\r\n").nth(1).unwrap();
        assert!(row.chars().count() <= 20, "row must fit the terminal width: {:?}", row);
        assert!(row.ends_with('…'));
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
