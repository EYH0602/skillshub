# Tolerate Non-Strict YAML Frontmatter in SKILL.md

**Date**: 2026-10-03
**Status**: Proposed

## Problem

`skillshub tap add QingYunA/answer-me-with-html` finds no skills, even though the
repo has a well-placed `skills/answer-me-with-html/SKILL.md`.

The frontmatter `description` is an unquoted plain scalar that contains `": "`:

```yaml
description: ... a one-page visual HTML explainer: the model writes ...
```

This is invalid YAML (`mapping values are not allowed here`), so `serde_yaml`
rejects the whole block. Skill authors write frontmatter like this often, and some
agents accept it, so skillshub should too.

The current failure handling makes the problem hard to see:

| Site | On parse failure |
|------|------------------|
| `registry/tap.rs` `discover_skills_from_local` (git taps) | Warns "invalid frontmatter (missing name field)", which is wrong: `name` exists |
| `registry/github.rs` `discover_skills_from_repo` (API path) | Drops the skill with no message |
| `registry/github.rs` `discover_skills_from_gist` | Drops the file with no message |
| `skill.rs` `parse_skill_metadata` (list / link / migration / installed-skill info) | Returns `Err`; callers skip or fall back |

## Proposed Solution

Add one shared frontmatter parser in `src/skill.rs` with two passes:

1. **Strict pass**: `serde_yaml::from_str` (current behavior). Valid files behave
   exactly as before.
2. **Lenient pass**, only if the strict pass fails: a line-based reader for the
   top-level keys skillshub uses (`name`, `description`, `license`):
   - A top-level key is a line with no leading whitespace that matches `key: rest`.
     The text after the **first** `": "` is the value, so later colons stay in the value.
   - Indented lines after a key are continuation lines, joined with a space
     (folded plain scalar). Block indicators `|` / `>` keep their usual meaning.
   - Remove one pair of matching surrounding quotes, if present.
   - Ignore other keys (`allowed-tools`, `metadata`, ...); they keep their defaults.
   - If no `name` is found, the parse still fails.

The parser returns the metadata and a flag that says whether the lenient pass was
used. `parse_skill_md_content` and `parse_skill_metadata` both call it, so all
sites above get the fallback.

### Warning policy

Warn only where the user acts on a source, so routine commands are not noisy:

- **Tap add / tap update** (`discover_skills_from_local`, `discover_skills_from_repo`)
  and **gist install**: print once per file
  `! <path>: frontmatter is not valid YAML (<serde error>); parsed leniently`.
- **list / link / migration / installed-skill info**: use the fallback silently,
  because the user already saw the warning when the tap was added.

Also fix the remaining messages:
- `discover_skills_from_local`: when both passes fail, print the real YAML error
  instead of "missing name field".
- `discover_skills_from_repo`: when both passes fail, warn instead of dropping the
  skill silently.

## Implementation Steps

1. `src/skill.rs`: add `parse_frontmatter(content) -> Result<ParsedFrontmatter>`
   (`metadata`, `lenient: bool`, `strict_error: Option<String>`) and the lenient
   line parser. Make `parse_skill_metadata` use it.
2. `src/registry/github.rs`: make `parse_skill_md_content` delegate to
   `parse_frontmatter`, and expose the lenient flag and error to discovery callers.
   Add warnings in `discover_skills_from_repo` and `discover_skills_from_gist`.
3. `src/registry/tap.rs`: add the lenient warning and the real-error message in
   `discover_skills_from_local`.
4. Unit tests:
   - Unquoted description with `": "`: parses, lenient flag set, full value kept.
   - Multi-line continuation and quoted values.
   - Valid YAML: strict path, lenient flag not set, `allowed-tools` / `metadata` still parsed.
   - Missing `name`: still fails in both passes.
   - `discover_skills_from_local` on a fixture with the bad frontmatter finds the skill.
5. Verify with `cargo test`, `cargo clippy`, and
   `cargo run -- tap add QingYunA/answer-me-with-html`.
6. Update `README.md` / `CLAUDE.md` if needed, then turn this plan into
   `docs/lenient-frontmatter.md` and delete it from `plans/`.

## Out of Scope

- Lenient parsing of nested keys (`metadata.author`, list-form `allowed-tools`).
- Rewriting or "fixing" upstream SKILL.md files on disk.
