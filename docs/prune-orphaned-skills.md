# Pruning Orphaned Skills with `--prune`

**Date**: 2026-07-29
**Status**: Implemented

## Why

When an upstream tap renames or deprecates a skill, the local installed copy is
left stale. Before this feature:

- `skillshub tap update` diffed the old vs. new registry, detected installed
  skills that no longer exist upstream, and only **printed a hint** telling the
  user to run `skillshub uninstall` by hand. It never removed anything.
- `skillshub update` hit a "not in registry" branch for a missing skill, printed
  a notice, and skipped. The skill stayed in `db.installed`, on disk, and
  symlinked.

A rename is the worst case: the old skill stays installed and linked while the
new name is never auto-installed, so the user silently keeps a deprecated skill
and lacks its replacement.

There is no tap manifest declaring renames — skills are discovered by walking
`SKILL.md` files (`discover_skills_from_local`), so a rename is indistinguishable
from delete-old + add-new. This feature therefore treats prune as "remove skills
that no longer exist upstream", not "follow a rename".

## Behavior

Both update commands accept `--prune`. Without it, they report missing skills and leave the installed copies and agent links in place.

- `skillshub tap update [owner/repo] --prune` refreshes the tap registry and uninstalls installed skills that are absent from it. Without `--prune`, it suggests uninstall commands. You can run it with `--prune` after a plain refresh.
- `skillshub update [owner/repo/skill] --prune` updates installed skills and uninstalls ones it finds missing from their tap or gist. Without `--prune`, it reports the missing skill and skips it.

Pruning uses the same cleanup as `uninstall`: it removes the installed files and database entry, and deletes the tap's install directory if it is empty. It also tries to remove matching symlinks from detected agents. A link must point to that exact skill's install path; files and links to other targets are left alone. Link cleanup failures are reported as warnings.

A failed refresh does not trigger pruning. This includes a Git tap scan that finds no valid `SKILL.md` files: the scan returns an error, so even `--prune` leaves the installed copies alone. If the tap has intentionally removed its last skill, use `skillshub uninstall <owner/repo/skill>` to remove the copy.

`skillshub tap remove <owner/repo>` is different: it uninstalls all skills from the tap by default. Pass `--keep-skills` to keep the local copies.

## Design

### DB consistency

`uninstall_skill` opens its own fresh DB via `db::init_db()` and calls
`db::save_db` at the end. The update flows hold their own in-memory `db` and save
once at the end. Calling `uninstall_skill` from inside an update loop would
re-read the DB and then let the final `save_db` overwrite it with the in-memory
copy that still contains the pruned entry — resurrecting the skill.

Fix: `remove_installed_skill_files(db, install_dir, skill_id)` removes the files
and the `db.installed` entry in place, mutating the caller's `db` without any
init/save cycle. `uninstall_skill` is a thin wrapper around it (init → check →
helper → save → print), and both update flows call it directly so their single
end-of-run `save_db` persists the refreshed cache and the pruned entries in one
consistent write.

### Orphan detection is membership-based, not diff-based (`tap update`)

`update_single_tap` overwrites the tap's cached registry and the run persists it.
Deriving the prune set from a diff against the pre-update cache would therefore
break on the exact path the no-prune hint suggests: after a plain `tap update`
refreshes the cache, a follow-up `tap update --prune` would diff two identical
registries and find nothing to prune.

Instead, `update_single_tap` returns the fresh registry's skill names
(`current_skills`), and `update_tap` computes orphans via
`orphaned_installed_skills(db, tap, current_skills)` — the installed skills of the
tap that are absent from the current registry. This depends only on current
membership, so a `--prune` re-run after a non-prune update still detects and
removes the orphan.

### Prune decision uses the freshly pulled clone (`skillshub update`)

For Git taps other than the default, `get_tap_registry` reads the cache. A stale cache can still list a deleted skill or omit a skill that exists upstream. Check the fresh clone before deciding what to prune.

For clone-backed taps, `update_skill` pulls the clone, scans it, and checks the installed name against the fresh scan. It uses the fresh path if the skill is still present. For the default tap, it uses the cached registry when available or the bundled registry otherwise. Gist installs are checked separately: the command fetches the gist, then checks for the skill if the gist's update timestamp changed.

## Key components

- `remove_installed_skill_files()` (`src/registry/skill.rs`) removes the files, agent links, and database entry. Uninstall and prune share it.
- `prune_or_report_missing()` (`src/registry/skill.rs`) handles a skill missing from the bundled or Git tap by pruning it or printing a notice.
- `orphaned_installed_skills()` (`src/registry/tap.rs`) — membership-based orphan
  set for a tap.
- `--prune` flags on `Commands::Update` and `TapCommands::Update` (`src/cli.rs`),
  threaded through `main.rs` into `update_skill` / `update_tap`.

## Tests

- `src/registry/tap.rs`: `orphaned_installed_skills` detection, scoping to a tap,
  the all-present case, and the regression case that fires on a `--prune` re-run
  after the cache has already been refreshed; plus the prune helper removing an
  orphan while keeping siblings and cleaning an emptied tap dir.
- `src/registry/skill.rs`: `prune_or_report_missing` removes files + db entry
  when `prune = true` and leaves them when `prune = false`.
- `src/registry/skill.rs`: uninstall removes the matching agent link and leaves unrelated links intact, including links to a same-named skill from another tap.
