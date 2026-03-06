# Gist Support for `skillshub add`

**Issue**: https://github.com/EYH0602/skillshub/issues/45
**Date**: 2026-03-06

## Problem

Users want to install skills from GitHub Gists. Gists are flat (files only, no directories), so they differ from regular repo-based skills.

Example: `skillshub add https://gist.github.com/garrytan/001f9074cab1a8f545ebecbc73a813df`

## Design

### URL Detection

Extend the existing `add` command to detect `gist.github.com/{owner}/{gist_id}` URLs and route to a gist-specific download flow. No new subcommand needed.

### Gist Fetch

Call `GET /gists/{gist_id}` via the GitHub API. Supports `GITHUB_TOKEN` for authentication. Returns file list with names, content, and metadata including `updated_at`.

### Skill Discovery (two levels)

1. **If any file is named `SKILL.md`**: Treat the gist as a single skill. Extract `name` from frontmatter.
2. **Otherwise**: Scan all files for valid SKILL.md frontmatter (requires `name` + `description` per the [agentskills.io spec](https://agentskills.io/specification)). Each valid file becomes its own skill.

### Installation

- **Namespace**: `owner/gists/skill-name` (e.g., `garrytan/gists/plan-exit-review`)
- For each discovered skill:
  - Create directory `~/.skillshub/skills/owner/gists/skill-name/`
  - Write the file content as `SKILL.md` inside that directory
- Create a synthetic tap `owner/gists` if it doesn't exist
- Record in `db.json` with `source_url` pointing to the gist

### Updates

- Track gist `updated_at` timestamp in the installed skill record
- On `skillshub update`, re-fetch gist, compare `updated_at`, re-download if changed
- Show "already up to date" if unchanged

### Linking

Same as regular skills — symlinks created in all detected agent directories.

## Constraints

- Gists are flat: no `scripts/`, `references/`, or `assets/` subdirectories. Gist skills are SKILL.md-only.
- Collision risk for `owner/gists/skill-name` is accepted as negligible.
