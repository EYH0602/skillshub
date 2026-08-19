# Tap-List Delete Key Plan

**Goal:** In `skillshub tui`, pressing `Delete` (or `d`) while a tap is focused in the **top-level tap list** deletes the whole tap and uninstalls its skills — without drilling into the tap first.

## Background

The tap-centric TUI (plans/2026-08-19-tap-centric-tui-plan.md, shipped in ced39ff) exposes tap deletion only one level down: tap list → `Tap: X` view → `Delete this tap…`. That plan deferred key-driven row actions to Phase 2 (ratatui) because inquire's `Select` cannot bind arbitrary keys to list rows — verified against inquire 0.9.4's API (`with_vim_mode`, scorer/sorter/formatter, no key hooks).

This plan adds the delete key to the top level **without waiting for full ratatui**: a small crossterm-based tap picker replaces inquire's `Select` at the tap-list level only. Everything below (tap view, skill MultiSelect, confirms) stays on inquire unchanged.

## Design

```
Tap list (custom crossterm picker)
──────────────────────────────────
> EYH0602/skillshub (default) 1 installed / 3 available
  anthropics/skills           2 installed / 45 available

↑/↓ or j/k  move        Enter  drill into tap (existing tap_view)
Delete / d  delete tap  Esc/q  quit            Ctrl-C  exit(130)
```

- Rows are `TapSummary` + `picker_label()` — unchanged data layer (tap.rs:197-218).
- `Update everything` and `Quit` stay as trailing rows; `Enter` activates them. `Delete`/`d` on a non-tap row is a no-op.
- Delete flow reuses the existing pieces verbatim:
  - default-tap guard and blast-radius confirm `Delete tap 'X' and uninstall its N skill(s)?` (`delete_confirm_prompt`, tui.rs:104) via inquire `Confirm` with default No,
  - `remove_tap(name, false)` (tap.rs:126), which already uninstalls the tap's skills.
  - The picker exits raw mode before running the inquire confirm, and re-enters on return.
- After a deletion (or drill-in), the loop re-reads the db and redraws — same refresh model as today.
- Esc/q/Ctrl-C classification unchanged (Esc → exit picker to shell, Ctrl-C → 130).
- Non-TTY guard unchanged.

## Tasks

1. **Dependency** — add `crossterm` (same major version inquire 0.9.4 already uses, so no duplicate crossterm in the tree) to Cargo.toml.
2. **Tap picker** — new `tap_picker` in `src/commands/tui.rs`: raw-mode alternate-screen-free list renderer (draw, on key: move/enter/delete/esc), returning `PickerOutcome::{Drill(tap), Delete(tap), UpdateAll, Quit}`. Pure, testable seam: `handle_key(state, KeyEvent) -> (state, Option<PickerOutcome>)`.
3. **Wire-up** — `run_tui` loops on the picker: `Drill` → existing `tap_view`, `Delete` → extract `delete_tap_flow` (already exists, tui.rs:81) and call it, `UpdateAll` → `update_skill(All)`, `Quit` → break.
4. **Tests** — key handling (Delete on tap row → Delete outcome; Delete on Update/Quit row → no-op; j/k/arrow movement; Enter outcomes), confirm-prompt blast radius (already covered).
5. **Docs** — README.md, CLAUDE.md, docs/cli-reference.md: document the tap-list keys (Enter drill, Delete/d delete, q quit) replacing the "Select a tap" description.

## Out of scope

- Key-driven actions inside the skill list (`u`/`U`/`i`) and ratatui two-pane UI — still Phase 2.
- Screen-reader fallback for the custom picker (e.g. `--prompt` flag keeping inquire) — flagged as a Phase-2 requirement already; noted here so it isn't lost.
- `--keep-skills` in TUI deletion — still CLI-only.

## Test plan

- `cargo test`, `cargo clippy`, `cargo fmt --check`.
- Manual: tap list renders; `d` on a tap with installed skills → blast-radius confirm → `skillshub list` shows skills gone and tap removed; `d` on default tap → refusal message, list intact; Esc/q quit; Ctrl-C exit code 130; non-TTY errors.

Suggested commit message: `feat: delete taps from the tap list with Delete/d key`
