# Interactive TUI (`skillshub tui`)

**Date**: 2026-08-19
**Status**: Implemented

## Why

Uninstalling or updating skills required knowing and typing full
`owner/repo/skill` names. Issue #66 asked for an interactive entry point. The
shipped design is **tap-centric**: the top-level object is the tap, the user
drills into a tap to manage its skills, and `d` on the tap list deletes the
focused tap. One navigation model covers today's view/uninstall/update and
leaves a seam for Phase 2 install.

## Navigation model

```
Tap list (crossterm picker)        Tap view (inquire Select)
─────────────────────────          ─────────────────────────
> EYH0602/skillshub (default) — 1/3    View/manage skills
  anthropics/skills — 2/45             Delete this tap…
  Update everything                    Back
  Quit

  Skill list (inquire MultiSelect)
  ────────────────────────────────
  [x] anthropics/skills/pdf (installed)   → Cancel / Update selected /
  [ ] anthropics/skills/pptx                Uninstall selected
```

- **Tap list** — custom `crossterm` raw-mode picker (`src/commands/tui.rs`).
  inquire's `Select` cannot bind arbitrary keys to list rows, so the top level
  uses crossterm directly: `↑/↓`/`j`/`k` move, `Enter` drills in,
  `d`/`Delete`/`Backspace` deletes the focused tap, `q`/`Esc` quit,
  `Ctrl-C` exits (130). Trailing rows: `Update everything`, `Quit`.
- **Everything below the tap list uses inquire** — tap view (`Select`), skill
  list (`MultiSelect`, installed skills pre-checked), action prompt, confirms.
- **Esc backs up exactly one level** (skill list → tap view → tap list → exit).
  The tap view loops so a cancelled child prompt redisplays it. Inside inquire
  prompts `q` is a filter keystroke, not a quit key.
- **Non-TTY guard**: `tui` bails when stdin/stdout is not a terminal; the
  subcommands are the scripting interface.

## Design decisions

### Confirmation friction scales with blast radius

Scoped deletes (skill uninstall, tap deletion) use a `y/N` confirm defaulting
to **No**, and the prompt names exactly what will be removed
(`Uninstall 2 skill(s): tap/a, tap/b?`, `Delete tap 'X' and uninstall its N
skill(s)?`). Full-state deletion (`clean all`) keeps the stronger type-`yes`
prompt. The difference is intentional — do not "unify" them.

In the skill MultiSelect, `Cancel` is the first (default-highlighted) action:
installed skills are pre-checked for viewing, so the accept-the-defaults path
must not be the destructive one.

### Raw-mode safety in the tap picker

The picker enables raw mode, so `Ctrl-C` is *not* SIGINT — it arrives as a key
event. It resolves to `PickerOutcome::Interrupted`; raw mode is restored before
the process exits with code 130. Raw mode is held by an RAII guard
(`RawModeGuard`), so a panic inside the picker cannot leave the user's shell in
raw mode either.

Plain-character bindings (`d`, `q`, `j`, `k`) require no CONTROL modifier —
in raw mode `Ctrl-D` arrives as `Char('d')` and must not trigger tap deletion.

### Redraw accounting

The picker redraws in place with `MoveUp(prev_lines)` +
`Clear(FromCursorDown)`, so the returned line count must equal the number of
`\r\n` emitted. To keep that invariant: rows are truncated to the terminal
width (never wrap), and the rendered rows are windowed to a viewport around
the cursor that never exceeds the terminal height. `Event::Resize` triggers a
redraw against the new dimensions.

### Batch uninstall semantics

`remove_installed_skills_batch` (`src/registry/skill.rs`) never aborts
mid-batch: per-skill results are collected, only successes lose their
`db.installed` entry, and the caller owns `save_db`. Within one removal, the
files are deleted **before** the agent symlinks — a failed `remove_dir_all`
must not strip links from a still-installed skill.

Agent link names are not tap-qualified (`tapA/pdf` and `tapB/pdf` share the
link name `pdf`), so symlink removal matches the **exact** expected target
(`install_dir/<tap>/<skill>`), not an install-dir prefix — uninstalling one
tap's skill never deletes another tap's link. Removal failures warn (with a
`skillshub clean links` hint) instead of being silently swallowed.

### Link replacement is gated

`create_or_refresh_symlink` (`src/commands/link.rs`) replaces an existing
symlink only when it is dangling or already points inside the skillshub skills
dir (i.e. skillshub-managed). A symlink pointing anywhere else is user-owned
and left alone. Per-skill link failures are reported and counted, not
propagated — one bad link cannot leave the remaining agents unlinked.

### `UpdateSelection` over empty-slice sentinel

`update_skill` takes an explicit `UpdateSelection::{All, Selected}` enum: in
the TUI an empty selection means "update nothing", and an empty slice silently
meaning "update everything" is a latent bug. `resolve_update_names` is the
pure, tested seam; tap pulls are deduplicated per run via a `HashSet`, marked
pulled only on success.

### Error handling at the loop boundary

Operational errors inside a flow (tap removal, update failures) are printed at
the `run_tui` loop boundary (`✗ {:#}`) and the session stays alive; only the
genuinely fatal errors (`init_db`, the picker itself) propagate. `save_db`
runs before per-skill success lines are printed, so a failed save is reported
as "removed from disk but the database could not be updated — re-run to
reconcile" instead of tearing down the TUI after announcing success.

## Key components

- `tap_picker` / `handle_picker_key` / `render_picker` (`src/commands/tui.rs`)
  — raw-mode tap picker; key handling and rendering are pure seams with unit
  tests.
- `tap_view` / `manage_skills` / `delete_tap_flow` (`src/commands/tui.rs`) —
  inquire flows below the tap list.
- `TapSummary` / `list_tap_summaries` (`src/registry/tap.rs`) — tap list data
  layer (default tap first, then alphabetical).
- `remove_installed_skills_batch` / `remove_installed_skill_files` /
  `remove_agent_symlinks_in` (`src/registry/skill.rs`) — batch uninstall with
  exact-target agent symlink cleanup.
- `UpdateSelection` / `resolve_update_names` (`src/registry/skill.rs`) —
  explicit update targeting.

## Phase 2 (not shipped)

- Full-screen `ratatui` two-pane UI with single-key row actions (`u`/`U`/`i`)
  inside the skill list. The `tui` command name is the stable seam.
- **Accessibility constraint**: line-based inquire prompts work with terminal
  screen readers; a ratatui redesign must address a11y explicitly (e.g. keep
  the inquire flows as a `--prompt` fallback).
- Interactive tap *add* and installing not-yet-installed skills from the TUI.
- `--keep-skills` in TUI tap deletion stays CLI-only.
