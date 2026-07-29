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

An opt-in `--prune` flag is available on **both** update commands. Default
behavior (no flag) is unchanged — detect and warn only — so the change is
non-destructive unless the user asks for it.

- `skillshub tap update --prune` — for each tap, after refreshing the registry,
  uninstall every installed skill that is absent from the fresh registry, then
  print what was pruned. Without `--prune`, the same orphans are listed with the
  manual `skillshub uninstall` hint.
- `skillshub update --prune` — while updating installed skills, uninstall any
  skill whose source (tap clone or gist) no longer contains it. Without
  `--prune`, it prints a "no longer in tap" notice and skips.

Pruning removes the install directory and the `db.installed` entry, and cleans up
a now-empty tap directory. Matching existing `uninstall` semantics, it does **not**
remove dangling agent symlinks — that is left to the user / `skillshub clean
links` (symlink option A; a unified fix for `uninstall` and `prune` is a possible
follow-up).

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

`get_tap_registry` is cache-only. Deciding to prune from that cache is unsafe: a
stale cache could miss a real upstream removal (leading to a cryptic "skill path
not found in local clone" copy error) or prune a skill that still exists upstream.

For clone-backed (non-default, non-gist) taps, `update_skill` now pulls the clone
first and re-discovers its skills with `discover_skills_from_local`, then decides
membership against that authoritative state — pruning when the skill is gone and
using the fresh path otherwise. The default (bundled) tap and gist taps carry an
authoritative registry at that point (gists are re-fetched fresh), so their prune
decision is made directly from it. The shared `prune_or_report_missing` helper
gives both paths identical prune-vs-report behavior.

## Key components

- `remove_installed_skill_files()` (`src/registry/skill.rs`) — in-place file +
  db-entry removal; the shared uninstall/prune primitive.
- `prune_or_report_missing()` (`src/registry/skill.rs`) — prune (when the flag is
  set) or print a "no longer in tap" notice; shared by the cache-miss and
  fresh-clone-miss paths.
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
