# Plan: `--prune` for removing orphaned skills on update

## Problem background

When an upstream tap renames or deprecates a skill, the local installed copy is
left stale. Verified in the current code:

- `skillshub tap update` (`update_single_tap`, `src/registry/tap.rs:329`) re-fetches
  the tap, diffs old vs. new, and computes `removed_installed` — installed skills
  that no longer exist upstream (`tap.rs:380-388`). But it only **prints** a hint
  (`tap.rs:294-303`) telling the user to run `skillshub uninstall` manually. It
  never removes anything.
- `skillshub update` (`update_skill`, `src/registry/skill.rs:468`) hits the
  `None` arm for a missing skill (`skill.rs:567-573`), prints `(not in registry)`,
  and skips. The skill stays in `db.installed`, on disk, and symlinked.

A rename is the worst case: the old skill stays installed + linked, and the new
name is never auto-installed. The user silently keeps a deprecated skill and
lacks its replacement.

Note: there is no tap manifest declaring renames. Skills are discovered by
walking `SKILL.md` files (`discover_skills_from_local`, `tap.rs:540`), so a
rename is indistinguishable from delete-old + add-new. This plan therefore
treats prune as "remove skills that no longer exist upstream", not "follow a
rename".

## Proposed solution

Add an opt-in `--prune` flag to `skillshub tap update` that automatically
uninstalls the skills in `removed_installed` instead of only printing a hint.
Default behavior (no flag) is unchanged: detect + warn only. Prune is never the
default, so the change is non-destructive unless the user asks for it.

Scope decision (for review): start with `tap update --prune` only, since that
command already computes `removed_installed` and is where a rename is detected.
`skillshub update --prune` can be added later if wanted; noted as optional below.

### Behavior

`skillshub tap update --prune`:
- For each tap, after computing `removed_installed`, uninstall each such skill
  (remove install dir, remove the `db.installed` entry, clean up empty tap dir).
- Print what was pruned, e.g. `pruned: tap/foo` instead of the "run uninstall
  manually" hint.
- Without `--prune`, keep the current advisory output verbatim.

## Key implementation concern: DB consistency

`uninstall_skill` (`skill.rs:435`) opens its **own** fresh DB via `db::init_db()`
and calls `db::save_db` at the end. `update_tap` holds its own in-memory `db`
and saves once at the end (`tap.rs:311`). Calling `uninstall_skill` from inside
the update loop would: (a) re-read the DB, and (b) let the final `save_db(&db)`
in `update_tap` overwrite it with the in-memory copy that **still contains** the
pruned entry — resurrecting the skill.

Fix: extract the file/entry removal into an in-memory helper that mutates the
caller's `db` without init/save, and call that from both places.

## Implementation steps

1. **Extract in-memory uninstall helper** in `src/registry/skill.rs`:
   ```rust
   /// Remove a skill's files and its db.installed entry, mutating `db` in place.
   /// Does NOT init or save the DB — the caller owns persistence.
   pub(crate) fn remove_installed_skill_files(
       db: &mut Database,
       install_dir: &Path,
       skill_id: &SkillId,
   ) -> Result<()>
   ```
   Move the dir-removal + empty-tap-dir cleanup + `db::remove_installed_skill`
   logic (`skill.rs:447-459`) into it. Rewrite `uninstall_skill` to be a thin
   wrapper: `init_db` → check installed → call helper → `save_db` → print.
   (Pure refactor; existing `uninstall` behavior unchanged.)

2. **Add the CLI flag** in `src/cli.rs` on `TapCommands::Update` (around line 136):
   ```rust
   Update {
       name: Option<String>,
       /// Uninstall installed skills that no longer exist in the tap
       #[arg(long)]
       prune: bool,
   },
   ```

3. **Thread the flag** in `src/main.rs:46`:
   `TapCommands::Update { name, prune } => update_tap(name.as_deref(), prune)?`

4. **Update `update_tap`** (`src/registry/tap.rs:251`) to take `prune: bool`.
   When `prune` is true and `result.removed_installed` is non-empty, loop and
   call `remove_installed_skill_files(&mut db, &install_dir, &skill_id)` for each,
   then print `pruned: tap/skill`. When false, keep the current advisory
   (`tap.rs:294-303`). The single `db::save_db(&db)` at `tap.rs:311` persists both
   the refreshed cache and the pruned entries in one consistent write.
   - `install_dir` from `get_skills_install_dir()` (already used in skill.rs).
   - Build `SkillId` from `format!("{}/{}", tap_name, skill)`.

## Symlink cleanup (decision point for review)

`uninstall_skill` today removes the install dir but does **not** remove the agent
symlinks pointing at it, so they become dangling (and `link` won't clean them —
it is additive only). Two options for prune:

- **A (recommended, v1):** Match existing `uninstall` semantics exactly — remove
  files + db entry, leave symlink cleanup to the user / `skillshub clean links`.
  Smallest, most consistent change.
- **B:** Also remove the specific agent symlinks for pruned skills (targeted
  version of `remove_managed_symlinks` in `clean.rs:40`). Cleaner result, but
  larger scope and diverges from `uninstall`. Could instead be a separate
  follow-up that fixes dangling links for `uninstall` and `prune` together.

Recommendation: ship A now; track B as a follow-up so `uninstall` and `prune`
stay consistent.

## Tests

In `src/registry/tap.rs` tests (alongside `test_tap_update_detects_removed_installed_skills`, `tap.rs:783`):
- `test_tap_update_prune_removes_installed_orphans`: install a skill, simulate the
  tap dropping it, run update with `prune = true`, assert the `db.installed` entry
  is gone and the install dir is removed.
- `test_tap_update_no_prune_keeps_orphans`: same setup with `prune = false`,
  assert the entry and dir remain (current behavior preserved).
- Refactor guard in `skill.rs` tests: existing `uninstall_skill` tests still pass
  after the helper extraction.

## Verification

- `cargo build`
- `cargo test` (new + existing)
- Manual smoke: `cargo run -- tap update --prune` and `cargo run -- tap update`
  against a tap where a skill was removed; confirm prune uninstalls and no-flag
  only warns.

## Docs

- Update `README.md` and `CLAUDE.md` command references for the new
  `tap update --prune` flag.
- After implementation, rewrite this plan as a design doc under `docs/` and
  remove it from `plans/` (per repo workflow).

## Open questions for review

1. Scope: `tap update --prune` only (recommended), or also `skillshub update --prune`?
2. Symlink cleanup: option A (match uninstall) or B (also clean links)?
3. Flag name: `--prune` (recommended) vs. `--remove-orphans` / `--clean`?
