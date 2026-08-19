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

### What already exists (reuse, don't reinvent)

- Output vocabulary: `✓` green success, `✗` red error, `-` red removal, `=>` green bold progress header, 2-space per-item indent (`src/registry/skill.rs` `uninstall_skill`, `update_skill`, prune path).
- `remove_installed_skill_files(db, install_dir, skill_id)` — caller-owned db persistence; the batch loop pattern comes from the update/prune path.
- Continue-on-error with per-skill `✗` lines — the prune path's failure behavior; the uninstall flow mirrors it.
- Severity-scaled confirmation precedent: `clean all` already requires typing `yes` (`src/commands/clean.rs`).

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
- Output vocabulary: reuse the existing conventions — `✓` green for success, `✗` red for errors, `=>` green bold for progress headers, per existing `uninstall_skill`/`update_skill` output.
- Confirmation convention (product-wide, documented here): friction scales with blast radius. Scoped deletes (uninstall N selected skills) use inquire's `y/N` confirm with a `No` default. Full-state deletion (`clean all`) keeps the stronger type-`yes` prompt. Do not "unify" these — the difference is intentional.
- MultiSelect rows show bare full names in Phase 1. ghcup-style status tags (installed version, update available) are deliberately deferred to the Phase 2 ratatui design: answering "which skills have updates?" requires a network pre-fetch before the prompt renders, which adds a loading state and latency for marginal value in a prompt-based UI.
- `Esc` at any prompt → back to the menu, nothing executed. Ctrl-C → exit the process (see Task 1 for the full classification). Atomicity is best-effort: the db is saved once after all removals, so a crash or `save_db` failure mid-flow can leave deleted files with stale db entries; rerunning the flow reconciles (the MultiSelect is built from db state, and removal of already-missing files is a no-op).
- Non-TTY stdin → error with hint to use `skillshub uninstall <name>` directly.

---

## Terminal considerations

- **No truncation in destructive flows:** skill full names are identifiers. Never truncate them in the MultiSelect or confirm prompt (despite `truncate_string` existing elsewhere) — the user must see exactly what they're deleting. Narrow terminals wrap; that's acceptable.
- **Color:** output uses the `colored` crate, which honors `NO_COLOR` automatically. No extra work; do not add color config.
- **Screen readers:** line-based inquire prompts are the accessible default — they work with terminal screen readers, unlike a full-screen ratatui UI. This is a standing advantage of Phase 1 and a design constraint for Phase 2 (ratatui work must address a11y explicitly).

---

## Tasks

### Task 1: Add `tui` subcommand and menu skeleton

**Files:**
- Modify: `Cargo.toml` — add `inquire = "0.9"`; bump `rust-version` from `1.74.0` to `1.80.0` (inquire 0.9 requires Rust 1.80). CI builds on `stable` only, so no workflow change is needed.
- Modify: `src/cli.rs` — add `Tui` variant to `Commands`
- Create: `src/commands/tui.rs` — menu loop
- Modify: `src/commands/mod.rs` — export `run_tui`
- Modify: `src/main.rs` — dispatch `Commands::Tui => run_tui()?`

Menu: `inquire::Select` with `Uninstall skills` / `Update skills` / `Quit`. Quit exits; each action runs its flow then returns to the menu (loop until Quit/Esc). Guard: if `!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal()`, bail with "interactive mode requires a terminal; use subcommands directly" — inquire renders to stdout, so a piped stdout with a TTY stdin would produce garbage without the second check.

Cancel classification (applies to every prompt, menu included): `Esc` → inquire `OperationCanceled` → return to the menu, nothing executed. Ctrl-C → inquire `OperationInterrupted` → exit the process immediately (standard CLI behavior). Never propagate prompt errors with `?` — an unclassified Esc would surface as a nonzero CLI error.

**Verification:** `cargo run -- tui` shows the menu; Esc quits; `echo | cargo run -- tui` errors.

### Task 2: Interactive uninstall flow

**Files:**
- Modify: `src/commands/tui.rs` — `uninstall_flow()`
- Modify: `src/registry/skill.rs` — add batch-removal function; `uninstall_skill` delegates to it
- Modify: `src/commands/link.rs` — replace the `exists()` check with `symlink_metadata` so dangling symlinks are replaced, not fatal
- Reuse: `src/registry/db::{init_db, save_db}`, `src/registry/skill::remove_installed_skill_files` (already `pub(crate)`, db-owned-by-caller)

Steps:
1. Add `remove_installed_skills_batch(db: &mut Database, install_dir: &Path, names: &[String]) -> Vec<(String, Result<()>)>` to `src/registry/skill.rs` (domain logic lives in the registry, not the UI module). Loops `remove_installed_skill_files`, collecting per-skill results — never aborts mid-batch.
2. Extend `remove_installed_skill_files` to also remove the skill's agent symlinks. Today uninstall deletes `~/.skillshub/skills/<tap>/<skill>` but leaves symlinks in agent dirs dangling — and `link_to_agents` then fails on them (its `exists()` check follows the symlink, returns false on a dangling link, and the `symlink()` create errors with EEXIST). Bulk uninstall from the TUI makes this common, so it lands here.
3. Fix `link_to_agents` (link.rs:107): use `symlink_metadata` instead of `exists()` — a path that is a symlink (dangling or not) gets replaced/refreshed; a non-symlink path is still skipped.
4. Refactor `uninstall_skill` to delegate: keep its not-installed bail and exact `✓ Uninstalled '<name>'` output, but route the removal through the batch function (batch of one; propagate the error as today). No user-visible change to the CLI command.
5. `uninstall_flow()`: load installed skills from db; sort by full name.
6. `inquire::MultiSelect::new("Select skills to uninstall", names)` — space toggles, `/`-style type-to-filter is built in.
7. Empty selection → return to menu, no-op.
8. `inquire::Confirm::new("Uninstall N skills?").with_default(false)`.
9. On confirm: one `init_db`, one batch call, single `save_db`, print per-skill `✓`/`✗` lines. This mirrors `update`'s prune path and avoids the nested init/save resurrection bug documented in `remove_installed_skill_files`.
10. Failure semantics: continue-on-error, mirroring the prune path. A failed removal prints `  ✗ <name> (<error>)` (red) and the loop continues; `save_db` persists every success so deleted files never keep stale db entries. End with a summary line: `Uninstalled N of M skills`.

**Tests (TDD):**
- Unit-test the batch function with a tempdir: removes files, removes agent symlinks, cleans empty tap dirs, keeps untap'd entries, removes db entries only for successes.
- Partial-failure test: force a deterministic removal failure by creating a regular *file* at the skill's install path inside the tempdir (`remove_dir_all` on a file errors on all platforms; unlike chmod, this doesn't depend on permission semantics or CI running as root). Assert the batch continues, reports the failure, and the db keeps only the failed entry.
- link.rs regression test: with a dangling symlink at the link path, `link_to_agents` replaces it instead of erroring.
- Prompt-layer itself is thin and manually verified (standard inquire practice).

**Verification:** `cargo test`; `cargo run -- tui` → uninstall two real skills → `skillshub list` confirms removal → reinstall.

### Task 3: Interactive update flow

**Files:**
- Modify: `src/commands/tui.rs` — `update_flow()`
- Modify: `src/registry/skill.rs` — generalize `update_skill` to take a list
- Modify: `src/main.rs` — adapt the `Update` dispatch to the new signature

First, generalize `update_skill(full_name: Option<&str>, prune: bool)` → `update_skill(selection: UpdateSelection, prune: bool)` with `enum UpdateSelection { All, Selected(Vec<String>) }`. An explicit enum, not an empty-slice-means-all encoding: in the TUI an empty selection means "update nothing", and an empty slice silently meaning "update everything" is a latent bug one refactor away. The existing body already iterates a `skills_to_update` vec with one db cycle, one header, and per-skill continue-on-error; only the selection preamble changes. Extract that preamble as `resolve_update_names(db: &Database, selection: &UpdateSelection) -> Result<Vec<String>>` (All → all installed; Selected validates each name, invalid/not-installed bails exactly as today) so it's unit-testable. CLI maps its optional arg to `All` or `Selected(vec![name])`; behavior of `skillshub update [name]` is unchanged. The refactor and the TUI feature land in the same PR (user decision), with the regression tests below landing alongside the refactor.

`update_flow()` resolves its MultiSelect result through a tiny pure helper (All-wins: "All skills" among selections → `UpdateSelection::All`; empty selection → no-op signal), unit-tested alongside.

Also in `update_skill`: dedup tap pulls. The per-skill loop calls `pull_or_reclone` once per skill (skill.rs:668), so two skills from one tap pull the same clone twice. Track already-pulled taps in a `HashSet` inside the loop and skip re-pulls — helps `update` (all) today and multi-name updates from the TUI.

**Regression tests (mandatory — both modified functions currently have zero coverage):**
- `uninstall_skill`: not-installed bail preserved; success path removes files + db entry and prints `✓ Uninstalled '<name>'` (tempdir-based).
- `resolve_update_names`: `All` → all installed; one valid name → that skill; invalid format bails; not-installed bails.
- TUI resolver: "All skills" + individual → All; empty selection → no-op; individuals only → those names.

Then `update_flow()`: `MultiSelect` over installed skills (plus an "All skills" first entry). If "All skills" is among the selections, it wins: `UpdateSelection::All`. Otherwise `UpdateSelection::Selected(names)` in one call — never one `update_skill` call per skill (that would mean N db init/save cycles, N headers, and N redundant pulls of the same tap clone). Empty selection → return to menu, no-op (same as uninstall flow). No confirm needed (non-destructive). Keep `--prune` out of the TUI for now.

**Verification:** `cargo run -- tui` → update two skills → single `=> Checking 2 skill(s)` header, output matches `skillshub update`; `skillshub update <name>` CLI behavior unchanged (existing tests + manual).

### Task 4: Docs and polish

**Files:**
- Modify: `README.md` — add `skillshub tui` to usage; note the Rust MSRV is now 1.80
- Modify: `CLAUDE.md` — note the command and the MSRV bump
- Modify: `docs/cli-reference.md` — document `tui`; document the confirmation convention (friction scales with blast radius: `y/N` for scoped deletes, type-`yes` for `clean all`)
- `completions` need no change (clap-derived).

**Verification:** `cargo build`, `cargo test`, `pre-commit run --all-files`, manual run of all menu paths.

---

## Out of scope (Phase 2, separate plan)

- Full-screen ratatui UI (browse taps, install from TUI, live progress, details pane, `?` help overlay) — design after #19 (project-level skills) settles.
- Interactive install/search, tap management in TUI.
- Cargo `tui` feature flag — defer until ratatui lands; inquire's dep weight doesn't justify it.

### NOT in scope (design decisions considered and deferred)

- **Row status tags (installed version, update available):** deferred to Phase 2 — requires network pre-fetch before prompt render; marginal value in a prompt UI. (Design review D3.)
- **Unifying confirm styles:** `clean all`'s type-`yes` and the TUI's `y/N` stay different on purpose — friction scales with blast radius. (D6.)
- **Truncating long skill names:** rejected — identifiers must render in full in destructive flows. (D7.)
- **Phase 2 ratatui accessibility strategy:** must be addressed in the Phase 2 design; line-based prompts are the accessible baseline. (D7.)
- **`--interactive` flags instead of the `tui` hub:** rejected — issue #66 asked for a hub and the `tui` command is the stable seam Phase 2 grows into. (Eng review E12, outside-voice challenge.)
- **Splitting the `update_skill` refactor into a separate prep PR:** rejected — refactor + feature land in one PR with regression tests alongside. (E11.)
- **Pinning an older inquire for Rust 1.74:** rejected — MSRV bumps to 1.80 instead. (E7.)
- **Branch-level regression coverage of every `update_skill` path (gist/bundled/prune/missing-tap):** deferred — needs network/git fixtures that don't exist today; resolver-level tests land now. (E11.)
- **MSRV CI job:** CI builds on `stable` only; a dedicated 1.80 job is a possible follow-up, not required for this PR.

## Test plan

- `cargo test` — unit tests for batch-removal logic (tempdir-based).
- Manual matrix: empty install db / single skill / many skills; Esc at each prompt; non-TTY error; uninstall-then-reinstall round trip; update flow parity with CLI.

## Implementation Tasks

Synthesized from the design and eng reviews. Each derives from a specific finding; checkbox as you ship.

- [ ] **T1 (P1, human: ~30min / CC: ~10min)** — `Cargo.toml` — Add `inquire = "0.9"`, bump `rust-version` to `1.80.0`
  - Surfaced by: Eng review E7 (outside voice) — inquire 0.9.4 requires Rust 1.80, project declared 1.74
  - Files: `Cargo.toml`
  - Verify: `cargo build`
- [ ] **T2 (P1, human: ~2h / CC: ~30min)** — `src/registry/skill.rs` — Add `remove_installed_skills_batch` (continue-on-error, removes agent symlinks too), delegate `uninstall_skill` to it; tempdir tests incl. deterministic partial-failure test
  - Surfaced by: Design D4 + Eng E3/E8 — batch semantics, module placement, dangling-symlink fix
  - Files: `src/registry/skill.rs`
  - Verify: `cargo test`
- [ ] **T3 (P1, human: ~1h / CC: ~15min)** — `src/commands/link.rs` — Replace `exists()` with `symlink_metadata` so dangling symlinks are replaced; add regression test
  - Surfaced by: Eng review E8 (outside voice) — link.rs:107 EEXIST on dangling links
  - Files: `src/commands/link.rs`
  - Verify: `cargo test`
- [ ] **T4 (P1, human: ~2h / CC: ~30min)** — `src/registry/skill.rs` — `UpdateSelection` enum, extract `resolve_update_names`, dedup tap pulls via `HashSet`; regression tests for resolver + `uninstall_skill`
  - Surfaced by: Eng E2/E4/E6/E10 — per-skill update overhead, zero existing coverage, pull dedup, explicit-over-clever encoding
  - Files: `src/registry/skill.rs`, `src/main.rs`
  - Verify: `cargo test`; `skillshub update` / `skillshub update <name>` unchanged
- [ ] **T5 (P1, human: ~2h / CC: ~30min)** — `src/commands/tui.rs` — Menu loop + both flows: stdin+stdout TTY guard, Esc→menu / Ctrl-C→exit classification, All-wins resolver, per-skill `✓`/`✗` output with `Uninstalled N of M` summary, full names untruncated
  - Surfaced by: Design D4/D5/D7 + Eng E5/E9 — cancel semantics, TTY guard, output vocabulary
  - Files: `src/commands/tui.rs`, `src/cli.rs`, `src/commands/mod.rs`, `src/main.rs`
  - Verify: `cargo run -- tui` full manual matrix; `echo | cargo run -- tui` errors
- [ ] **T6 (P2, human: ~30min / CC: ~10min)** — docs — README/CLAUDE.md/cli-reference: `tui` usage, MSRV 1.80, confirmation convention
  - Surfaced by: Design D6 + Eng E7 — documentation requirements
  - Files: `README.md`, `CLAUDE.md`, `docs/cli-reference.md`
  - Verify: `pre-commit run --all-files`

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | outside voice in eng review | Independent 2nd opinion | 1 | issues_found | 9 findings, 7 accepted, 2 rejected |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | clean | 12 issues, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | clean | score: 5/10 → 8/10, 6 decisions |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **CROSS-MODEL:** outside voice (Codex) found 2 verified defects the primary review missed (MSRV break, dangling-symlink EEXIST) plus 5 spec gaps; 2 challenges rejected by user (tui seam, refactor split).
- **VERDICT:** DESIGN + ENG CLEARED — ready to implement.

NO UNRESOLVED DECISIONS
