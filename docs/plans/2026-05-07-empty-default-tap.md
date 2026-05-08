# Reset the Default `EYH0602/skillshub` Tap to a Single `using-skillshub` Skill

**Date**: 2026-05-07
**Status**: Draft — awaiting review

## Problem

The repo currently bundles 13 skills under `skills/` (analyze-ci, docs-review,
docstring, fuzzing, github-actions-templates, paper-polish, python-packaging,
read-repo-references, senior-data-scientist, temporal-python-testing,
testing-python, uv-package-manager, write-unit-tests). We want to drop these
in preparation for a different skill set, but we don't want the default tap
to be completely empty — a brand-new user installing skillshub and running
`skillshub install-all` should get at least one immediately useful skill.

The most useful default skill is one that teaches an AI agent how to use
`skillshub` itself. That gives every freshly-onboarded agent a working mental
model of the tool — taps vs. installs vs. links, common workflows, and where
things live — so the user can immediately ask their agent things like "add
this Gist as a skill and link it to Cursor" and have the agent do the right
thing.

## Scope

This plan covers:

1. Removing the 13 existing bundled skills.
2. Adding one new skill, `using-skillshub`, that teaches agents how to drive
   the CLI (built via `/skill-creator`).
3. Updating docs/examples that referenced the removed skills.

It does **not** touch:

- `DEFAULT_TAP_NAME` / `DEFAULT_TAP_URL` constants in `src/registry/db.rs`
- The `normalize_default_taps` invariant
- The auto-add-default-tap behavior on first run
- The `is_default` flag handling

The default-tap *concept* stays; only its *contents* change.

## What changes

### 1. Remove the 13 existing skill directories

```
skills/analyze-ci/                skills/paper-polish/
skills/docs-review/               skills/python-packaging/
skills/docstring/                 skills/read-repo-references/
skills/fuzzing/                   skills/senior-data-scientist/
skills/github-actions-templates/  skills/temporal-python-testing/
                                  skills/testing-python/
                                  skills/uv-package-manager/
                                  skills/write-unit-tests/
```

### 2. Add `skills/using-skillshub/`

New skill folder layout:

```
skills/using-skillshub/
├── SKILL.md                       # ~150 lines — workflows, mental model, gotchas
└── references/
    ├── cli-reference.md           # Copied from docs/cli-reference.md
    └── architecture.md            # Copied from docs/architecture.md
```

Why bundle the references inside the skill folder rather than pointing at the
repo's `docs/`: once a user installs this skill via `skillshub install
EYH0602/skillshub/using-skillshub`, the skill folder is copied to
`~/.skillshub/skills/EYH0602/skillshub/using-skillshub/` — far away from the
repo's `docs/` tree. Bundling keeps the skill self-contained after install.
The cost is that the bundled refs can drift from the canonical `docs/`; this
is acceptable because both files change rarely and a stale ref still
captures the load-bearing concepts (taps, installed skills, link, agents).

The SKILL.md itself focuses on:

- A decision table (user intent → command) so the agent picks the right
  command on the first try
- A directory-flow diagram (tap repo → clone → installed → linked) — the
  single concept that resolves most "why doesn't agent X see skill Y?"
  questions
- Common workflows (one-skill install, whole-tap subscribe, gist install,
  re-link, update, uninstall, full purge)
- A short SKILL.md authoring guide for when the user asks the agent to write
  or edit a skill
- Pointers to the bundled references for everything else

### 3. Documentation

| File | Lines | Change |
|---|---|---|
| `README.md` | 36–37, 78–79, 81–82, 84–86, 88–89 | Replace `EYH0602/skillshub/code-reviewer` examples with `EYH0602/skillshub/using-skillshub` (the skill that now actually exists in this tap). Update line 36's comment from "bundled skills" → "bundled skill" if singular reads better. |
| `CLAUDE.md` / `AGENTS.md` | 35 | Same: `code-reviewer` → `using-skillshub` in the `cargo run -- install …` testing snippet. |
| `src/cli.rs` | 20, 32, 53 | Doc-comment examples reference `EYH0602/skillshub/code-reviewer`. Switching them to `using-skillshub` makes the example actually installable from the default tap, which is more honest. Line 143 (`e.g., EYH0602/skillshub`) is fine as-is — it documents tap-name format. |
| `docs/cli-reference.md` | 30 | "The default tap is `EYH0602/skillshub` (bundled)." — keep as-is, still accurate. |
| `docs/architecture.md` | 28 | `# Bundled skills (default tap)` — still accurate (we'll have one). |

### 4. Tests

Audit confirms no test reads the real `skills/` directory:

- `tests/database_test.rs`, `tests/local_skill_test.rs`,
  `tests/agent_linking_test.rs` build their own fixtures via helpers in
  `tests/common/`.
- `code-reviewer` appears only as a fixture *name string*, not a path that
  reads from disk. Renaming these to `using-skillshub` is cosmetic and adds
  diff noise — leave them. Fixture identifiers don't have to mirror real
  skills.
- The other 12 removed skill names appear nowhere in `src/` or `tests/`.

**No test changes required.** The plan still requires running the full suite
to confirm.

### 5. CHANGELOG

New `[Unreleased]` section:

```markdown
## [Unreleased]

### Added
- Bundled `using-skillshub` skill in the default `EYH0602/skillshub` tap.
  Teaches AI agents how to drive the `skillshub` CLI — install/link/update
  workflows, the tap → install → link mental model, and SKILL.md authoring.

### Removed
- Previously bundled skills (analyze-ci, docs-review, docstring, fuzzing,
  github-actions-templates, paper-polish, python-packaging,
  read-repo-references, senior-data-scientist, temporal-python-testing,
  testing-python, uv-package-manager, write-unit-tests). Default tap now
  ships a single `using-skillshub` skill; broader skill curation is
  intentionally moving out of this repo.
```

Don't bump the version in this commit — that happens when the new skill set
beyond `using-skillshub` lands (or as part of a separate release commit).

## Implementation steps

1. **Delete the 13 skill directories.**
   ```bash
   rm -rf skills/analyze-ci skills/docs-review skills/docstring skills/fuzzing \
          skills/github-actions-templates skills/paper-polish skills/python-packaging \
          skills/read-repo-references skills/senior-data-scientist \
          skills/temporal-python-testing skills/testing-python \
          skills/uv-package-manager skills/write-unit-tests
   ```
2. **Create `skills/using-skillshub/SKILL.md`** with the content drafted via
   `/skill-creator` (already written in this branch).
3. **Bundle reference docs** by copying them into the skill folder:
   ```bash
   cp docs/cli-reference.md skills/using-skillshub/references/cli-reference.md
   cp docs/architecture.md skills/using-skillshub/references/architecture.md
   ```
4. **Update `README.md`, `CLAUDE.md`, `AGENTS.md`, `src/cli.rs`** with the
   `code-reviewer` → `using-skillshub` substitutions.
5. **Update `CHANGELOG.md`** with the `[Unreleased]` section above.
6. **Verify locally:**
   ```bash
   cargo build
   cargo test
   cargo run -- tap list      # EYH0602/skillshub still listed as default
   cargo run -- list          # 1 skill: EYH0602/skillshub/using-skillshub
   cargo run -- info EYH0602/skillshub/using-skillshub
   ```
7. **Suggested commits** (two atomic commits keep the diff legible):
   ```
   chore: remove previously bundled skills from default tap

   Drops the 13 skills under skills/ in preparation for a focused default
   tap. Documentation examples (README, CLAUDE.md, AGENTS.md, src/cli.rs
   doc comments) are updated in the follow-up commit that adds the
   replacement skill.
   ```
   then
   ```
   feat: add using-skillshub as the bundled default skill

   Introduces a single bundled skill that teaches AI coding agents how
   to drive the skillshub CLI: tap vs. install vs. link mental model,
   command decision table, common workflows, and SKILL.md authoring.
   References docs/cli-reference.md and docs/architecture.md are copied
   into the skill folder so it remains self-contained after install.

   Updates README, CLAUDE.md, AGENTS.md, and src/cli.rs doc comments to
   use the new skill in examples. CHANGELOG records both the additions
   and removals.
   ```

## Optional follow-up: skill-creator eval loop

The skill-creator workflow recommends running test prompts against the new
skill (with-skill vs. without-skill subagents) and reviewing outputs. Worth
doing once before declaring `using-skillshub` v1.0 done. Skipping it for the
initial drop is fine; flag it as a follow-up issue if we don't do it now.
