# Interactive TUI Plan (`skillshub tui`)

**Goal:** Add a `skillshub tui` command as the interactive entry point. Phase 1 delivers a hub menu with two workflows — **interactive uninstall** (the immediate pain point) and **interactive update** — using prompt-based interaction. The command name `tui` is stable; the underlying renderer can upgrade to a full-screen TUI later without changing the UX contract.

**Issue:** #66 (interactive tui)

---

## Research summary

**Framework landscape (verified 2026-08-19):**

| Option | Version | Verdict |
|---|---|---|
| `inquire` | 0.9.4 | Prompt-based (`Select`, `MultiSelect` with fuzzy filter + space toggle, `Confirm`). ~50 lines per flow, tiny dep tree. **Phase 1 choice.** |
| `ratatui` + `crossterm` | 0.30.2 / 0.29.0 | De-facto standard full-screen TUI (gitui uses it in production). ~300–500 lines + owned event loop, 30–40 transitive crates. **Phase 2 upgrade path.** |
| `dialoguer` | 0.12.0 | Lighter than inquire but no built-in fuzzy MultiSelect parity. Skip. |
| `cursive` | 0.21.1 | Sparse releases, wrong fit for dense package lists. Skip. |
| `iocraft` | 0.8.4 | Attractive React-like API, single-maintainer 0.x risk. Skip. |

**ghcup tui (the model):** separate lib behind a build flag, hub screen with list navigation, single-key row actions (`i` install / `u` uninstall / `s` set), status tags in list rows, confirm for destructive ops, `?` help, `q` quit. No multi-select — ghcup queues long installs one at a time.

**Precedent:** npm/pnpm use one-shot interactive prompts (the inquire analog); no cargo/pipx-style tool ships a full TUI; ghcup keeps its TUI strictly optional. Prompts-first matches the ecosystem and ships now.

**Decision:** Phase 1 = `inquire` hub-and-spoke prompts behind `skillshub tui`. Phase 2 (separate plan, after #19 project-level skills settles the data model) = ratatui full-screen UI reusing the same workflow functions.

---

## Phase 1 UX flow

```
$ skillshub tui
? What do you want to do?
> Uninstall skills
  Update skills
  Quit

# → Uninstall skills
? Select skills to uninstall (space to toggle, type to filter)
> [x] EYH0602/skillshub/using-skillshub
  [ ] superpowers/brainstorming
  [x] gstack/ship

# → Confirm
? Uninstall 2 skills? (y/N)

✓ Uninstalled 'EYH0602/skillshub/using-skillshub'
✓ Uninstalled 'gstack/ship'
```

- Empty installed list → print "No skills installed." and return to menu.
- `Esc`/Ctrl-C at any prompt → back out cleanly, no partial state (db saved once after all removals).
- Non-TTY stdin → error with hint to use `skillshub uninstall <name>` directly.

---

## Tasks

### Task 1: Add `tui` subcommand and menu skeleton

**Files:**
- Modify: `Cargo.toml` — add `inquire = "0.9"`
- Modify: `src/cli.rs` — add `Tui` variant to `Commands`
- Create: `src/commands/tui.rs` — menu loop
- Modify: `src/commands/mod.rs` — export `run_tui`
- Modify: `src/main.rs` — dispatch `Commands::Tui => run_tui()?`

Menu: `inquire::Select` with `Uninstall skills` / `Update skills` / `Quit`. Quit exits; each action runs its flow then returns to the menu (loop until Quit/Esc). Guard: if `!std::io::stdin().is_terminal()`, bail with "interactive mode requires a terminal; use subcommands directly".

**Verification:** `cargo run -- tui` shows the menu; Esc quits; `echo | cargo run -- tui` errors.

### Task 2: Interactive uninstall flow

**Files:**
- Modify: `src/commands/tui.rs` — `uninstall_flow()`
- Reuse: `src/registry/db::{init_db, save_db}`, `src/registry/skill::remove_installed_skill_files` (already `pub(crate)`, db-owned-by-caller)

Steps:
1. Load installed skills from db; sort by full name.
2. `inquire::MultiSelect::new("Select skills to uninstall", names)` — space toggles, `/`-style type-to-filter is built in.
3. Empty selection → return to menu, no-op.
4. `inquire::Confirm::new("Uninstall N skills?").with_default(false)`.
5. On confirm: one `init_db`, loop `remove_installed_skill_files` per skill, single `save_db`, print per-skill `✓` lines. This mirrors `update`'s prune path and avoids the nested init/save resurrection bug documented in `remove_installed_skill_files`.

**Tests (TDD):**
- Extract the pure parts for testability: a function that, given `&mut Database`, `&Path` install dir, and a list of names, removes them and reports per-skill results. Unit-test with a tempdir: removes files, cleans empty tap dirs, keeps untap'd entries.
- Prompt-layer itself is thin and manually verified (standard inquire practice).

**Verification:** `cargo test`; `cargo run -- tui` → uninstall two real skills → `skillshub list` confirms removal → reinstall.

### Task 3: Interactive update flow

**Files:**
- Modify: `src/commands/tui.rs` — `update_flow()`

Same shape: `MultiSelect` over installed skills (plus an "All skills" first entry) → run existing `update_skill(Some(name), false)` per selection, or `update_skill(None, false)` for all. No confirm needed (non-destructive). Keep `--prune` out of the TUI for now.

**Verification:** `cargo run -- tui` → update one skill → confirm output matches `skillshub update <name>`.

### Task 4: Docs and polish

**Files:**
- Modify: `README.md` — add `skillshub tui` to usage
- Modify: `CLAUDE.md` — note the command
- Modify: `docs/cli-reference.md` — document `tui`
- `completions` need no change (clap-derived).

**Verification:** `cargo build`, `cargo test`, `pre-commit run --all-files`, manual run of all menu paths.

---

## Out of scope (Phase 2, separate plan)

- Full-screen ratatui UI (browse taps, install from TUI, live progress, details pane, `?` help overlay) — design after #19 (project-level skills) settles.
- Interactive install/search, tap management in TUI.
- Cargo `tui` feature flag — defer until ratatui lands; inquire's dep weight doesn't justify it.

## Test plan

- `cargo test` — unit tests for batch-removal logic (tempdir-based).
- Manual matrix: empty install db / single skill / many skills; Esc at each prompt; non-TTY error; uninstall-then-reinstall round trip; update flow parity with CLI.
