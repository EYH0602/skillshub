# Dedicated Prune Command

**Date**: 2026-11-19
**Status**: Implemented

## Why

Skills removed upstream stay installed and linked by default. Before this
command, removing them in bulk required `update --prune` (which also updates
unrelated installs) or `tap update --prune` (which refreshes registries and
skips gist installs). Neither offered a preview limited to the skills that
would be removed.

```bash
skillshub prune                          # Check all installed skills and remove confirmed orphans
skillshub prune owner/repo               # Check only installs from this tap
skillshub prune owner/repo --dry-run     # Preview without uninstalling anything
```

## Behavior

`prune` only removes; it never updates, installs, or refreshes any other
installed skill. It groups installed skills by the source they came from and
checks only sources that still have installed skills:

- **Git taps** (including the default tap): the configured branch is refreshed
  once per tap — cloned if there is no local copy yet, pulled otherwise — and
  scanned for skills. Membership is decided by name in the fresh scan OR by
  the installed record's `source_path` still holding a `SKILL.md` in the
  clone (see [Presence is path-or-name](#presence-is-path-or-name)).
- **Gist installs** (`gist_updated_at` set): the gist is fetched once per gist
  id — even when its recorded timestamp is unchanged, because the timestamp
  never proves a skill was removed — and its discovered skill names decide
  membership.
- **Bundled registry**: not consulted. The bundled copy is an install source,
  not an upstream authority; the default tap is verified against its upstream
  clone like any other tap.

A complete scan with zero skills is a valid empty source: every installed
skill from that tap is an orphan. Anything less — a failed pull or clone, an
interrupted scan, a gist whose files come back without content — keeps that
source's skills and makes the run exit nonzero, after every other source was
still checked.

Removal reuses the same code as `uninstall`: installed files, the database
entry, matching agent symlinks, and an emptied tap directory. Candidates are
printed by full skill name in sorted order before anything is removed; a dry
run stops there and leaves files, links, and the database untouched.

Skills whose tap record is missing — the ones `tap remove --keep-skills`
intentionally retains — are never candidates: a missing tap record is not
proof of upstream deletion. They are reported and kept, without failing the
run. Unknown tap names in `prune <owner/repo>` are rejected.

Existing `update --prune` and `tap update --prune` keep their behavior; use
`prune` when cleanup is the only goal. Prune does not replace `clean links`,
and it never installs a renamed skill's replacement: a rename of the skill
name reads as removal of the old name.

## Design

### Presence is path-or-name

The scan keys skills by frontmatter `name`. Two install shapes do not match
that key, and treating the mismatch as deletion would delete skills that
still exist upstream:

1. `skillshub add <github-url>` names the install after the URL's folder
   (`GitHubUrl::skill_name`), which can differ from the frontmatter name.
2. A tap can install from directories the scan skips (`examples/`,
   `tests/`, and similar).
3. A broken upstream `SKILL.md` (invalid frontmatter, unsafe name) is
   skipped with a warning, hiding the name from the scan until upstream
   fixes it.

So a git-tap skill counts as present when the fresh scan lists its name or
its recorded `source_path` still contains a `SKILL.md` inside the clone.
`source_path` is repo-relative, guarded against absolute paths and `..`
components before joining. Gists have no paths (files, not directories), so
their check stays name-based — and files returned without content make the
gist unverifiable rather than "skill removed".

### Unverifiable sources fail the run

A stale cache or failed check is not proof that a skill was deleted, so the
command refuses to decide: the source's skills stay installed, the reason is
printed, other sources are still processed, and the process exits nonzero so
scripts can tell an incomplete run from a clean one. Removal failures count
the same way.

### No database writes beyond removals

`prune` never refreshes cached registries or timestamps — `tap update` owns
cache freshness. The database is saved only when a skill was actually
removed, which also makes "dry run leaves the database byte-identical"
trivially true.

## Key components

- `prune_skills()` (`src/registry/prune.rs`): grouping, source checks,
  candidate printing, removal, exit-status accounting.
- `walk_local_skills()` (`src/registry/tap.rs`): the clone scan shared with
  `discover_skills_from_local()`. `Ok(empty)` is a confirmed empty source;
  `Err` (unreadable directory or SKILL.md mid-walk) is an incomplete scan.
  `discover_skills_from_local()` keeps bailing on empty scans, so the update
  flows' behavior is unchanged.
- `remove_installed_skill_files()` (`src/registry/skill.rs`): the uninstall
  removal path, shared by `uninstall`, both update commands, and `prune`.

## Tests

`src/registry/prune.rs`:

- Orphan removal keeps siblings, their links, other taps, and non-skillshub
  links in the same agent directory.
- Dry run keeps files, links, and the database byte-identical.
- A confirmed empty tap prunes its last skill and cleans the tap directory.
- A failed source check (unclonable URL) keeps its skills and fails the run.
- Skills from removed taps survive; scoping checks only the named tap and
  rejects unknown names.
- Path-presence regressions: broken upstream frontmatter and folder-named
  URL installs survive; gist files without content are unverifiable.
- The scan distinguishes complete-empty from interrupted (unreadable
  SKILL.md) walks.
