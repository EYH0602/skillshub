# Changelog & versioning policy

**Date:** 2026-05-07
**Status:** Awaiting human review

## Background

Skillshub already follows [Keep a Changelog](https://keepachangelog.com/) and
[Semantic Versioning](https://semver.org/), but the rules are unwritten. The
result: `Cargo.toml` is at `1.0.3` while `CHANGELOG.md` only documents up to
`1.0.0`. Three released versions (`1.0.1`, `1.0.2`, `1.0.3`) have no entries.

`/home/yfhe/vibelift/core/` documents an explicit policy in `AGENTS.md` and
ties every PR to a patch bump + CHANGELOG entry. We want the same discipline
here, adapted for skillshub's "every PR squash-merges to `main`" flow (no
long-lived `release/*` branch).

## Goals

1. Make the bump-and-changelog rule a documented, non-optional step on every PR.
2. Close the gap between `Cargo.toml` and `CHANGELOG.md`.
3. Keep the policy simple — no `[Unreleased]` buffer, no release-branch flow.

## Non-goals

- No CI enforcement of the rule (lint check) in this change. PR reviewer
  catches misses for now; can be added later if it becomes a problem.
- No PR template (`.github/PULL_REQUEST_TEMPLATE`) in this change.

## Policy

The project follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html)
(`MAJOR.MINOR.PATCH`). The version is stored in `Cargo.toml` under
`[package] version`.

- **Patch** (`1.0.x` → `1.0.x+1`): bump for every PR. Each PR is squash-merged
  to `main`.
- **Minor** (`1.x` → `1.x+1`): bump for new user-facing features grouped into
  a release.
- **Major**: reserved for breaking changes to the CLI surface, on-disk config
  layout, or `SKILL.md` schema.

### Rules

- Every PR **must** bump the version in `Cargo.toml` before merging.
- Every PR **must** add an entry to `CHANGELOG.md` under a new
  `## [X.Y.Z] - YYYY-MM-DD` heading that matches the bumped version, inserted
  at the top of the version list (newest-first order).
- Every PR **must** add the corresponding compare link at the bottom of
  `CHANGELOG.md`:
  `[X.Y.Z]: https://github.com/EYH0602/skillshub/compare/vX.Y.(Z-1)...vX.Y.Z`

### Respecting the existing CHANGELOG

The CHANGELOG already follows a consistent style. Match it exactly:

- Keep the existing header text and the [Keep a Changelog 1.1.0] /
  [SemVer 2.0.0] links — do **not** swap them for the unversioned URLs.
- Use a blank line between a group heading and its first bullet
  (`### Added\n\n- entry`).
- Write entries as descriptive prose ending in a period. A trailing
  `(#PR)` reference is **optional** — current entries don't use them, so
  add one only when the PR number adds meaningful traceability.
- Wrap long bullets at ~80 columns with continuation lines indented two
  spaces.
- Do **not** introduce an `## [Unreleased]` section. Every PR ships
  immediately under its own version heading.
- Never edit or reorder previously released entries.

### CHANGELOG format

Group entries under: `Added`, `Changed`, `Fixed`, `Removed`, `Performance`.

```markdown
## [1.0.4] - 2026-05-08

### Added

- Short description of what was added.

### Fixed

- Short description of what was fixed.
```

## Concrete changes

### 1. `CLAUDE.md` and `AGENTS.md`

Both files are kept in sync (identical content, separate files). Add a new
`## Versioning` section after the existing `## Development` block, containing
the policy text above.

### 2. `CHANGELOG.md` — backfill missing versions

Insert three new version sections **above** the existing `## [1.0.0]` heading
(newest-first order: 1.0.3 → 1.0.2 → 1.0.1 → 1.0.0), and add three compare
links at the bottom of the file. Use the existing prose style — no `(#PR)`
suffix.

Source data:

| Version | Date       | Source commit | PR  | Group | Summary                                        |
| ------- | ---------- | ------------- | --- | ----- | ---------------------------------------------- |
| 1.0.3   | 2026-04-22 | `8f2a76f`     | #72 | Added | Kiro CLI and 6 trending agent support          |
| 1.0.2   | 2026-04-22 | `3562b7f`     | #71 | Fixed | Correct OpenCode skills path                   |
| 1.0.1   | 2026-03-24 | `7d240a8`     | #65 | Fixed | Panic on multi-byte UTF-8 in `truncate_string` |

Dates are commit-author dates from `git log`. Read each commit before writing
the entry so the prose accurately reflects what shipped (titles alone may be
too terse).

New compare links to append at the bottom (above the existing `[1.0.0]` link):

```
[1.0.3]: https://github.com/EYH0602/skillshub/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/EYH0602/skillshub/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/EYH0602/skillshub/compare/v1.0.0...v1.0.1
```

Do **not** modify the existing `[1.0.0]` and earlier compare links.

## Out of scope (YAGNI)

- `[Unreleased]` section — not needed because every PR ships immediately.
- `release/*` branch flow — skillshub does not batch.
- CI lint that fails PRs missing a version bump or CHANGELOG entry.
- PR template that prompts for the bump.

## Open questions

### Missing release tags for `v1.0.2` and `v1.0.3`

`git ls-remote --tags origin 'v1.0*'` returns only `v1.0.0` and `v1.0.1`.
Cargo.toml is at `1.0.3` and PRs #71, #72 have been merged, but the
corresponding tags were never pushed. Because of this, the compare links
proposed in the backfill section won't resolve.

Two ways to close the gap, pick one before implementing the backfill:

1. **Create the missing tags** (recommended). Tag `v1.0.2` at `3562b7f` and
   `v1.0.3` at `8f2a76f`, then push. Compare links resolve as written.
   This also locks in "every PR creates a matching tag" as part of the
   versioning policy going forward.
2. **Use commit SHAs in the backfill compare links**. Leaves the tag gap
   open, but keeps the spec self-contained:
   ```
   [1.0.3]: https://github.com/EYH0602/skillshub/compare/3562b7f...8f2a76f
   [1.0.2]: https://github.com/EYH0602/skillshub/compare/v1.0.1...3562b7f
   [1.0.1]: https://github.com/EYH0602/skillshub/compare/v1.0.0...v1.0.1
   ```

Either choice resolves the immediate issue. Per the project rule
(`Do not commit or create pull requests — let the human do them`), tag
creation is the human's call regardless.

### Should the policy require a release tag per PR?

Vibelift's policy doesn't mandate tags. Skillshub already pushes tags for
some versions but not others. If we choose option 1 above, it makes sense
to also extend the policy with: "Every PR's version bump **must** be
accompanied by a `vX.Y.Z` git tag pushed to `origin` after merge." Decide
together with the question above.

## Implementation order

1. Edit `CLAUDE.md` — add `## Versioning` section.
2. Edit `AGENTS.md` — mirror the same section.
3. Edit `CHANGELOG.md` — backfill `1.0.1`, `1.0.2`, `1.0.3` entries and
   compare links, dates pulled from `git log`.
4. Suggest a commit message to the human (per `CLAUDE.md`: "Do not commit
   or create pull requests — let the human do them").
