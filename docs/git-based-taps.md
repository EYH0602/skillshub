# Git-Based Tap Management

## Overview

Skillshub uses local git clones for all tap operations (add, update, install, update skills). This replaces the original GitHub API-based approach (tarball downloads, Tree API) with a simpler, faster, and rate-limit-free workflow.

**Gist taps and star list imports** still use the GitHub API.

## Architecture

```
tap add owner/repo
  → git clone --depth 1 → ~/.skillshub/taps/owner/repo/
  → walk filesystem for SKILL.md files (walkdir crate)
  → cache registry in db.json

tap update
  → git pull --ff-only (or delete + re-clone on failure)
  → re-discover skills from local clone

install owner/repo/skill
  → ensure_clone (verify clone health, re-clone if needed)
  → copy skill files from taps/ to skills/

update (skill)
  → pull_or_reclone tap clone
  → compare HEAD SHA
  → copy updated files
```

### Directory Layout

```
~/.skillshub/
├── db.json                     # Database (taps, installed skills, external skills)
├── taps/                       # Shallow git clones of tap repositories
│   └── owner/
│       └── repo/
│           ├── .git/
│           └── skills/
│               └── skill-name/
│                   └── SKILL.md
└── skills/                     # Installed skills (copied from taps/)
    └── owner/
        └── repo/
            └── skill-name/
                └── SKILL.md
```

## Key Components

### `src/registry/git.rs`

- **`check_git()`** — Pre-flight check that git is installed. Called before any git operation.
- **`git_clone()`** — Shallow clone (`--depth 1`) with optional branch. Uses `.status()` to stream git's progress output.
- **`git_pull()`** — Fast-forward only pull.
- **`ensure_clone()`** — Validates an existing clone (`.git` exists, `rev-parse HEAD` succeeds, remote URL matches, branch matches). Deletes and re-clones if any check fails.
- **`pull_or_reclone()`** — Attempts `git pull --ff-only`; on failure (force-push, diverged), deletes and re-clones. Clones are disposable caches.
- **`git_head_sha()`** — Returns the short HEAD commit SHA.

### `src/registry/tap.rs`

- **`discover_skills_from_local()`** — Walks a local clone using `walkdir` crate, finding all `SKILL.md` files. Skips `.git`, `node_modules`, `target`, `test`, `tests`, `examples`, `fixtures`, `vendor`, `benchmark`, and dot-prefixed directories. Warns on duplicate skill names and malformed frontmatter.
- **`add_tap()`** — Clones repo, discovers skills, caches registry.
- **`update_single_tap()`** — Pulls/re-clones, re-discovers, diffs old vs new registry.

### `src/registry/skill.rs`

- **`install_from_clone()`** — Ensures clone exists, validates path containment (prevents directory traversal), copies with cleanup on failure.
- **`add_skill_from_url()`** — Ensures clone, copies from clone (replaces API tarball download).

### `src/commands/doctor.rs`

Diagnostic checks: git health, clone integrity, installed skill files, orphan clone detection.

## Breaking Changes (from 0.2.x)

- **`@commit` specifier** produces a hard error for non-gist taps. Shallow clones cannot checkout arbitrary commits.
- **Private repos** require git credential helpers or SSH keys (previously used `GITHUB_TOKEN`).
- **`git` is a hard requirement.** `check_git()` runs before any git operation.

## What Still Uses the GitHub API

- Gist skill discovery and installation (`discover_skills_from_gist`, `fetch_gist`)
- Star list imports (`fetch_star_list_repos` via GraphQL)
- Gist tap discovery in `add_tap` and `update_single_tap` (`discover_skills_from_repo`)

## Dependencies

- **Added:** `walkdir` (recursive directory scanning)
- **Removed from production:** `flate2`, `tar` (tarball extraction)
- **Kept:** `reqwest` (gist API, star-list GraphQL)
