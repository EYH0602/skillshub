# Tap-Centric TUI Redesign Plan

**Goal:** Restructure the interactive UI around **taps as the top-level object** instead of action-first menus. The user sees their taps, presses Enter to drill into a tap's skills, or `d` to delete the focused tap. The same navigation model applies to every interactive surface — today's view/uninstall, and future install — so the UX contract is learned once.

**Supersedes:** the action-first hub in `plans/2026-08-19-interactive-tui-plan.md` (that plan's output vocabulary, cancel classification, and confirmation conventions carry over unchanged).

**User decisions (2026-08-19):**
- Redesign Phase 1 too — the inquire flows become tap-drill-down now; key-driven row actions (`d` on the focused row) arrive with Phase 2 ratatui.
- Inside a tap's skill list, row actions are **single-key, ghcup-style** (`u` uninstall, `U` update) in Phase 2; Phase 1 inquire approximates this with an action prompt per skill selection.

---

## Design: one navigation model, two renderers

```
Top level (tap list)                Tap view (skill list)
─────────────────────               ─────────────────────
> EYH0602/skillshub (default)       Tap: anthropics/skills
  anthropics/skills                 > [x] pdf              installed
  vercel-labs/agent-skills            [ ] pptx             not installed
                                      [x] docx             installed
Enter → tap view                    Phase 1: space toggles, Enter → action prompt
d     → delete tap (confirm)        Phase 2: u uninstall / U update / i install
Esc/q → back / quit                 Esc → back to tap list
```

- **Tap list rows** show tap name, default marker, and `installed/available` skill counts (data already computed by `list_taps`, tap.rs:207-213).
- **Deleting a tap** = existing `remove_tap(name, keep_skills: false)` (tap.rs:126). Default tap is not deletable — `remove_tap` already bails; the UI greys it out / explains instead of failing after the fact.
- **Confirmation convention unchanged:** friction scales with blast radius. Skill uninstall → `y/N` (default No). Tap deletion uninstalls every skill from that tap, so it gets the same `y/N` confirm but with the consequence spelled out: `Delete tap 'X' and uninstall its N skills?`
- **Esc/Ctrl-C classification unchanged:** Esc → up one level (skill list → tap list → exit); Ctrl-C → exit process (130).
- **Non-TTY guard unchanged.**

## Phase 1 (inquire) — ships now

inquire cannot bind arbitrary keys to list rows, so Phase 1 renders the same *navigation model* with prompts:

1. **Tap picker** — `Select` over configured taps (default first, sorted), annotated `name (N installed / M available)`. Trailing entries: `Update everything`, `Quit`.
2. **Skill view** — after choosing a tap, `Select`: `View/manage skills`, `Delete this tap…`, `Back`.
   - *View/manage skills* → `MultiSelect` of the tap's skills annotated installed/not-installed, pre-checking installed ones → action prompt: `Uninstall selected`, `Update selected`, `Cancel`. Uninstall confirms `y/N`, then reuses `remove_installed_skills_batch` + per-skill `✓`/`✗` output. Update reuses `UpdateSelection::Selected` (one `update_skill` call — no per-skill db cycles).
   - *Delete this tap…* → `y/N` confirm naming the blast radius → `remove_tap(name, false)`. On bail (default tap) print the error and return to the tap view.
3. Empty tap list → print "No taps configured. Add one with `skillshub tap add`." and exit.

The current flat "Uninstall skills / Update skills" menu entries are removed; updating everything lives at the tap-list level as `Update everything` (→ `UpdateSelection::All`).

## Phase 2 (ratatui) — same model, key-driven

- Two-pane states: `TapList` ↔ `SkillList(tap)`.
- TapList keys: `Enter`/`→` drill in, `d` delete tap (confirm modal), `u` update tap registry, `q` quit, `?` help.
- SkillList keys: `space` select, `u` uninstall selected, `U` update selected, `i` install selected (future), `Esc`/`←` back.
- Accessibility: per the Phase 1 plan's standing constraint, the ratatui design must address screen-reader support explicitly (e.g. keep the inquire flows as an `--prompt` fallback).

## Tasks

### Task 1: Tap-picker data layer
- Add `TapSummary { name, is_default, installed_count, available_count }` and `list_tap_summaries(db) -> Vec<TapSummary>` in `src/registry/tap.rs`, extracting the counting logic out of `list_taps` (tap.rs:197-249) so both share it. Default tap first, then alphabetical.
- Unit test: ordering, counts, empty db.

### Task 2: Tap-centric inquire flows
- Rewrite `src/commands/tui.rs`: tap picker loop, tap view (`View/manage skills` / `Delete this tap…` / `Back`), skill MultiSelect + action prompt, `Update everything` entry, empty-taps message.
- Uninstall path reuses `remove_installed_skills_batch`; update path reuses `UpdateSelection::Selected`/`All`; delete path calls `remove_tap(name, false)`.
- Keep: TTY guard, Esc/Ctrl-C classification, output vocabulary, untruncated full names.
- Unit tests (pure helpers): tap-picker option construction (default first, annotations), skill-selection → action resolution, delete-confirm blast-radius message.

### Task 3: Docs
- `README.md`, `CLAUDE.md`, `docs/cli-reference.md`: describe tap-centric navigation; remove the old menu description.

## Out of scope

- ratatui renderer itself (separate Phase 2 plan, after #19).
- Interactive tap *add* and interactive install of not-yet-installed skills (the SkillList already annotates them so Phase 2's `i` has a seam, but no install flow ships now).
- `--keep-skills` in the TUI tap deletion — TUI always uninstalls the tap's skills; power users keep the CLI flag.

## Test plan

- `cargo test` — summaries, option construction, selection resolution.
- Manual matrix: 0 taps / 1 tap / many taps; default-tap delete attempt; Esc at every level; non-TTY error; delete a tap with installed skills → `skillshub list` confirms removal; update-everything parity with CLI.
