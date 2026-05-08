# Empty the Default `EYH0602/skillshub` Tap

**Date**: 2026-05-07
**Status**: Draft — awaiting review

## Problem

The repo currently bundles 13 skills under `skills/`:

```
analyze-ci/                github-actions-templates/  senior-data-scientist/
docs-review/               paper-polish/              temporal-python-testing/
docstring/                 python-packaging/          testing-python/
fuzzing/                   read-repo-references/      uv-package-manager/
                                                       write-unit-tests/
```

We want to drop these so the tap is empty, then repopulate it with a different
set of skills later. The top-level `skills/` directory must stay so the tap
keeps a valid registry shape.

## Scope

This plan covers **only** the skill-content removal and follow-on doc/test
updates. It does **not** touch:

- `DEFAULT_TAP_NAME` / `DEFAULT_TAP_URL` constants in `src/registry/db.rs`
- The `normalize_default_taps` invariant
- The auto-add-default-tap behavior on first run
- The `is_default` flag handling

The default-tap *concept* stays; only its *contents* are emptied.

## What changes

### 1. Skill files

Delete every immediate subdirectory of `skills/`:

```
skills/analyze-ci/
skills/docs-review/
skills/docstring/
skills/fuzzing/                  (also removes references/ subdir)
skills/github-actions-templates/
skills/paper-polish/
skills/python-packaging/
skills/read-repo-references/
skills/senior-data-scientist/    (also removes references/ subdir if any)
skills/temporal-python-testing/  (also removes references/ subdir if any)
skills/testing-python/
skills/uv-package-manager/
skills/write-unit-tests/
```

Keep `skills/` itself. To make the empty directory survive `git`, drop a
`skills/.gitkeep` file (consistent with other empty-dir-tracking conventions
already used in many Rust projects). No `README.md` inside `skills/` — keeping
it minimal until new skills land.

### 2. Documentation

Files that show installable-skill examples currently reference
`EYH0602/skillshub/code-reviewer`, which (a) never existed in this tap and
(b) will definitely not exist once it's empty. Update each to use a clearly
synthetic placeholder so readers don't try a broken command:

| File | Lines | Change |
|---|---|---|
| `README.md` | 37, 79, 82, 86, 89 | `EYH0602/skillshub/code-reviewer` → `<owner>/<tap>/<skill>` style placeholder, with a one-line note above the first example that the default tap is currently empty pending new skills. |
| `CLAUDE.md` | 35 | Same placeholder substitution in the local-testing snippet. |
| `AGENTS.md` | 35 | Same as `CLAUDE.md` (these two files mirror each other). |
| `src/cli.rs` | 20, 32, 53, 143 | The `///` doc comments use `EYH0602/skillshub/code-reviewer` as an example of the `owner/tap/skill` format. Keep `code-reviewer` here — it's purely illustrative format documentation and rotating it adds churn without value. **No change.** |

The `src/cli.rs` line 143 reads `e.g., EYH0602/skillshub` (without a skill
suffix); leave it as-is — it's documenting the tap-name format and is still
accurate.

### 3. Tests

Audit confirms no test reads from the real `skills/` directory tree:

- `tests/database_test.rs`, `tests/local_skill_test.rs`,
  `tests/agent_linking_test.rs` create their own fixtures via
  `create_test_skill` / `skill_md` helpers in `tests/common/`.
- `code-reviewer` appears only as a fixture *name string*, not as a path that
  reads from disk.
- The other 12 skill names appear nowhere in `src/` or `tests/`.

**No test changes required.** The plan still requires running the full suite
to confirm.

### 4. CHANGELOG

Add an `### Removed` block under a new `[Unreleased]` section noting the
default tap was emptied. Don't bump the version yet — that happens when the
new skill set lands.

## Implementation steps

1. **Delete skill directories.**
   ```bash
   rm -rf skills/analyze-ci skills/docs-review skills/docstring skills/fuzzing \
          skills/github-actions-templates skills/paper-polish skills/python-packaging \
          skills/read-repo-references skills/senior-data-scientist \
          skills/temporal-python-testing skills/testing-python \
          skills/uv-package-manager skills/write-unit-tests
   ```
2. **Add `skills/.gitkeep`** so the empty directory is tracked.
3. **Update `README.md`** — five example lines (37, 79, 82, 86, 89) plus a
   note above the first install example.
4. **Update `CLAUDE.md` and `AGENTS.md`** — the `cargo run -- install …` line
   in each.
5. **Update `CHANGELOG.md`** — new `[Unreleased]` section with `### Removed`.
6. **Verify locally:**
   ```bash
   cargo build
   cargo test
   cargo run -- tap list      # should still show EYH0602/skillshub as default
   cargo run -- list          # should show 0 installed skills
   ```
7. **Suggested commit message** (single commit, since the changes are tightly
   coupled):
   ```
   chore: empty default EYH0602/skillshub tap

   Removes the 13 bundled skills under skills/ in preparation for a new
   skill set. Updates README/CLAUDE.md/AGENTS.md examples to use a
   placeholder owner/tap/skill format and notes the tap is currently
   empty. CLI doc strings retain `code-reviewer` as illustrative format
   documentation. No code or test behavior changes.
   ```

## Open questions

1. **`code-reviewer` in `src/cli.rs` doc comments** — leave or replace? My
   recommendation is leave (it's format documentation, not an install
   suggestion), but happy to swap if you'd rather have zero references.
2. **README placeholder style** — `<owner>/<tap>/<skill>` or something more
   concrete like `EYH0602/skillshub/<skill-name>`? The latter still grounds
   the reader in this specific tap.
3. **`.gitkeep` vs. a placeholder `skills/README.md`** — `.gitkeep` is silent;
   a stub README could explain "skills coming soon." Lean toward `.gitkeep`
   unless you'd like the visible note.
