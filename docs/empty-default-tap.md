# Default Tap: A Single `using-skillshub` Skill

**Date**: 2026-05-07
**Status**: Implemented

## Why

The default `EYH0602/skillshub` tap previously bundled 13 skills (analyze-ci,
docs-review, docstring, fuzzing, github-actions-templates, paper-polish,
python-packaging, read-repo-references, senior-data-scientist,
temporal-python-testing, testing-python, uv-package-manager,
write-unit-tests). Curating an opinionated multi-skill default in this repo
duplicates work that belongs in user-managed taps, and ties the binary's
release cadence to skill content.

We want the default tap to ship one immediately useful skill — enough that a
fresh `skillshub install-all` is non-empty, but small enough that broader
curation can move out of this repo.

The most useful single default is one that teaches an AI agent how to drive
`skillshub` itself. Every freshly-onboarded agent then has a working mental
model of taps vs. installs vs. links, common workflows, and where things
live, so the user can immediately ask "add this Gist as a skill and link it
to Cursor" and get the right behaviour.

## Scope

In:

1. Removing the 13 previously bundled skills.
2. Adding `skills/using-skillshub/` — a SKILL.md plus bundled
   `references/cli-reference.md` and `references/architecture.md`.
3. Updating examples in `README.md`, `CLAUDE.md`, `AGENTS.md`, and
   `src/cli.rs` doc comments to reference `using-skillshub` (so copy-pasted
   examples actually install something from the default tap).

Out of scope:

- `DEFAULT_TAP_NAME` / `DEFAULT_TAP_URL` constants (unchanged).
- `normalize_default_taps` invariant (unchanged).
- Auto-add-default-tap on first run (unchanged).
- The `is_default` flag (unchanged).

The default-tap *concept* stays; only its *contents* change.

## Key decisions

**Bundle reference docs inside the skill folder** rather than pointing at the
repo's `docs/`. After install, the skill ends up at
`~/.skillshub/skills/EYH0602/skillshub/using-skillshub/`, far from this
repo's `docs/`. Self-contained references survive the install boundary. The
cost is that the bundled refs can drift from canonical `docs/`; both files
change rarely and a stale ref still captures the load-bearing concepts.

**Don't rename test fixtures.** `code-reviewer` appears in test code only as
a fixture name string, never as a path that reads from `skills/` on disk.
Renaming would add diff noise without changing behaviour.

**One skill, not zero.** A truly empty default tap would be simpler, but
breaks the new-user "install something useful with one command" path.

## What's bundled

```
skills/using-skillshub/
├── SKILL.md
└── references/
    ├── cli-reference.md       # copy of docs/cli-reference.md
    └── architecture.md        # copy of docs/architecture.md
```

SKILL.md covers:

- Decision table mapping user intent → command (right command on first try)
- Directory-flow diagram (tap repo → clone → installed → linked) — the
  single concept that resolves most "why doesn't agent X see skill Y?"
  questions
- Common workflows (one-skill install, whole-tap subscribe, gist install,
  re-link, update, uninstall, full purge)
- Short SKILL.md authoring guide for when the user asks the agent to write
  or edit a skill
- Pointers to bundled references for everything else

## Follow-ups

- Skill-creator eval loop (with-skill vs. without-skill subagents) before
  declaring `using-skillshub` v1.0 done — track as an issue if not done now.
