# Lenient SKILL.md Frontmatter Parsing

**Date**: 2026-10-03
**Status**: Implemented

## Why

`skillshub tap add QingYunA/answer-me-with-html` found no skills, even though the
repo had a well-placed `skills/answer-me-with-html/SKILL.md`. Its frontmatter
`description` was an unquoted plain scalar containing `": "`:

```yaml
description: ... a one-page visual HTML explainer: the model writes ...
```

This is invalid YAML (`mapping values are not allowed in this context`), so
`serde_yaml` rejected the whole block and the skill was dropped. Skill authors
write frontmatter like this often, and some agents accept it.

The failure was also hard to see:

| Site | Old behavior on parse failure |
|------|-------------------------------|
| `discover_skills_from_local` (git taps) | Warned "invalid frontmatter (missing name field)", which was wrong |
| `discover_skills_from_repo` (API path) | Dropped the skill with no message |
| `discover_skills_from_gist` | Dropped the file with no message |
| `parse_skill_metadata` (list / link / migration / info) | Returned `Err`; callers skipped or fell back |

## Behavior

`crate::skill::parse_frontmatter(content)` is the single frontmatter parser. It
returns `ParsedFrontmatter { metadata, lenient_reason }`.

1. **Strict pass**: `serde_yaml`. Valid files behave as before. The leading newline
   after `---` is kept, so YAML error line numbers match the file.
2. **Lenient pass**, only if the strict pass fails: a line-based reader.
   - A top-level key is an unindented line `key: rest` where `key` is
     `[A-Za-z0-9_-]+`. The value is everything after the **first** colon, so later
     `": "` sequences stay in the value.
   - Indented lines continue the previous key and are joined with a space.
     Block indicators `|` (keep newlines) and `>` (fold to spaces) are honored,
     including `-` / `+` chomping suffixes.
   - One pair of matching surrounding quotes is removed.
   - Only `name`, `description` and `license` are extracted; `allowed-tools`
     and `metadata` keep their defaults.
   - No `name` means the parse fails, with the strict YAML error in the message.

When the lenient pass is used, `lenient_reason` holds the strict YAML error.

### Warnings

Warnings are printed only where the user acts on a source, so routine commands
stay quiet:

- **Tap add / tap update** (`discover_skills_from_local`, `discover_skills_from_repo`)
  and **gist discovery** print:
  ```
  ! skills/foo/SKILL.md: frontmatter is not valid YAML (mapping values are not allowed in this context at line 3 column 87); parsed leniently
  ```
- If both passes fail, discovery prints `! Skipping <path>: <error>` with the real
  YAML error. The API path now warns instead of dropping the skill silently.
- `parse_skill_metadata` (list / link / migration / info) uses the fallback
  silently; the user already saw the warning when the tap was added.

## Tests

- Reproduction of the original bug at each layer: frontmatter parsing
  (`github.rs`), git-tap discovery (`tap.rs`), and installed-skill metadata
  (`skill.rs`).
- `skill.rs`: strict path for valid YAML (including `allowed-tools`), lenient
  reason with file-accurate line numbers, continuation lines and ignored nested
  keys, quote stripping, `|` / `>-` block scalars, and missing `name` still failing.

## Out of Scope

- Lenient parsing of nested keys (`metadata.author`, list-form `allowed-tools`).
- Rewriting upstream SKILL.md files on disk.
