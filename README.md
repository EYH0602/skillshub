[![CI](https://github.com/EYH0602/skillshub/actions/workflows/ci.yml/badge.svg)](https://github.com/EYH0602/skillshub/actions/workflows/ci.yml)

# Skillshub

Skillshub is a package manager for AI coding agent skills - like Homebrew for skills.
Install skills once and link them to every detected agent so all of your agents stay in sync.

## Why Skillshub

- **Direct URL install**: Add skills directly from GitHub URLs - no registry needed
- **Tap-based registry**: Optionally organize skills into taps (like Homebrew)
- **One install, many agents**: A single skills registry in `~/.skillshub/skills`
- **One command to sync**: `skillshub link` wires skills into all detected agents
- **Version tracking**: Track which commit each skill was installed from
- **Clear skill format**: Each skill lives in its own folder with `SKILL.md` metadata

## Installation

### From Cargo (recommended)

```bash
cargo install skillshub
```

### From Source

```bash
git clone https://github.com/EYH0602/skillshub
cd skillshub
cargo install --path .
```

## Quick Start

```bash
# Install from the default tap (bundled skills)
skillshub install EYH0602/skillshub/code-reviewer

# Or add third-party taps and install from them
skillshub tap add anthropics/skills
skillshub install anthropics/skills/frontend-design

skillshub tap add vercel-labs/agent-skills
skillshub install vercel-labs/agent-skills/vercel-deploy

# Link installed skills to every detected agent
skillshub link

# See which agents were detected
skillshub agents
```

## Commands

### Adding Skills from URLs

The easiest way to add skills is directly from GitHub URLs:

```bash
# Add a skill from any GitHub repository
skillshub add https://github.com/user/repo/tree/main/skills/my-skill

# Add a skill from a GitHub Gist
skillshub add https://gist.github.com/user/gist_id
```

Skills from repositories are organized as `owner/repo/skill-name`. Gist skills are organized as `owner/gists/skill-name`.

### Skill Management

```bash
# List all available and installed skills
skillshub list

# Search for skills
skillshub search python

# Install a skill from a tap (format: owner/repo/skill)
skillshub install EYH0602/skillshub/code-reviewer

# Show detailed info about a skill
skillshub info EYH0602/skillshub/code-reviewer

# Update installed skills to latest version
skillshub update                                    # Update all
skillshub update EYH0602/skillshub/code-reviewer    # Update one

# Uninstall a skill
skillshub uninstall EYH0602/skillshub/code-reviewer

# Install all skills from the default taps
skillshub install-all
```

### Tap Management (Optional)

Taps are Git repositories containing skills. Skills are automatically discovered by scanning for folders with `SKILL.md` files - no special configuration required.

```bash
# List configured taps
skillshub tap list
# Skills column shows installed/available counts (e.g., 2/15 or 1/?)

# Add third-party taps (any GitHub repo with SKILL.md files)
skillshub tap add anthropics/skills
skillshub tap add vercel-labs/agent-skills

# Full URLs also work
skillshub tap add https://github.com/some-org/some-skills

# Add a tap and install all its skills in one command
skillshub tap add anthropics/skills --install

# Add a tap from a specific branch
skillshub tap add user/repo --branch dev

# Update tap registries (re-discover skills)
skillshub tap update                        # Update all taps
skillshub tap update anthropics/skills      # Update specific tap

# Install all skills from a specific tap
skillshub tap install-all anthropics/skills

# Remove a tap (also uninstalls all its skills)
skillshub tap remove vercel-labs/agent-skills

# Remove a tap but keep its installed skills
skillshub tap remove vercel-labs/agent-skills --keep-skills
```

### Import from GitHub Star Lists

If you curate skills taps in a GitHub star list, you can import all of them at once:

```bash
# Add all repos from a star list as taps
skillshub star-list https://github.com/stars/username/lists/skills

# Add taps and install all skills from each
skillshub star-list https://github.com/stars/username/lists/skills --install
```

This requires a `GITHUB_TOKEN` (the GraphQL API requires authentication).

### Agent Linking

```bash
# Link installed skills to all detected agents
skillshub link

# Show which agents are detected
skillshub agents
```

### External Skills Management

Skillshub can discover and sync skills installed through other means (e.g., Claude marketplace, manual installation):

```bash
# List discovered external skills
skillshub external list

# Scan agent directories for external skills
skillshub external scan

# Stop tracking an external skill (doesn't delete it)
skillshub external forget my-skill
```

When you run `skillshub link`, external skills are automatically discovered from all agent directories and synced to all other agents. If the same skill name exists in multiple agents, the first one found is used as the source.

### Cleanup

```bash
# Clear cached tap registry data (forces re-fetch on next update)
skillshub clean cache

# Remove all skillshub-managed symlinks from agent directories
skillshub clean links

# Remove symlinks AND delete all installed skills
skillshub clean links --remove-skills

# Completely remove all skillshub state (full uninstall/purge)
skillshub clean all

# Skip the interactive confirmation prompt (useful for scripts/CI)
skillshub clean all --confirm
```

The `clean all` command is a full uninstall/purge: it removes all skillshub-managed symlinks from every detected agent directory, then deletes the entire `~/.skillshub/` directory (including all installed skills and the database). Without `--confirm`, it prints a summary of what will be removed and prompts you to type `yes` to proceed.

## Supported Agents

Skillshub automatically detects and links to these coding agents:

| Agent    | Directory      | Skills Path           |
| -------- | -------------- | --------------------- |
| Claude   | `~/.claude`    | `~/.claude/skills`    |
| Codex    | `~/.codex`     | `~/.codex/skills`     |
| OpenCode | `~/.opencode`  | `~/.opencode/skills`  |
| Aider    | `~/.aider`     | `~/.aider/skills`     |
| Cursor   | `~/.cursor`    | `~/.cursor/skills`    |
| Continue | `~/.continue`  | `~/.continue/skills`  |
| Trae     | `~/.trae`      | `~/.trae/skills`      |
| Kimi     | `~/.kimi`      | `~/.kimi/skills`      |
| OpenClaw | `~/.openclaw`  | `~/.openclaw/skills`  |
| ZeroClaw | `~/.zeroclaw`  | `~/.zeroclaw/skills`  |
| Kiro     | `~/.kiro`      | `~/.kiro/steering`    |
| Gemini   | `~/.gemini`    | `~/.gemini/skills`    |
| Copilot  | `~/.copilot`   | `~/.copilot/skills`   |
| Junie    | `~/.junie`     | `~/.junie/skills`     |
| Augment  | `~/.augment`   | `~/.augment/skills`   |
| Warp     | `~/.warp`      | `~/.warp/skills`      |
| Cline    | `~/.cline`     | `~/.cline/skills`     |

## GitHub API & Authentication

Tap operations (`tap add`, `tap update`, `install`, `update`) use **local git clone/pull** — no GitHub API calls and no rate limits.

The GitHub API is only used for:
- **Gist skills** (`skillshub add https://gist.github.com/...`)
- **Star list imports** (`skillshub star-list ...`)

For these operations, set a `GITHUB_TOKEN` to avoid rate limiting:

```bash
export GITHUB_TOKEN=your_token_here
```

For **private repositories**, configure git credential helpers or SSH keys — skillshub uses `git clone` directly.

## Shell Completions

Generate tab-completion scripts for your shell:

```bash
# Bash
skillshub completions bash > ~/.local/share/bash-completion/completions/skillshub

# Zsh (ensure ~/.zfunc is in your fpath)
skillshub completions zsh > ~/.zfunc/_skillshub

# Fish
skillshub completions fish > ~/.config/fish/completions/skillshub.fish
```

Completions are generated from the CLI definition, so they are always in sync with the installed version.

## Diagnostics

```bash
# Check git, tap clones, installed skills, and orphan clones
skillshub doctor
```

## How It Works

1. Skills are organized by source: `~/.skillshub/skills/<owner>/<repo>/<skill>/`
2. A database at `~/.skillshub/db.json` tracks installed skills and their versions
3. Running `skillshub link` creates per-skill symlinks in each agent's skills directory
4. External skills (from other sources) are discovered and synced to all agents
5. Re-run `skillshub link` any time to keep all agents synchronized

## Skill Format

Each skill folder must contain a `SKILL.md` file with YAML frontmatter:

```yaml
---
name: skill-name
description: What this skill does
allowed-tools: Tool1, Tool2 # Optional, comma-separated or array
license: MIT                # Optional, SPDX identifier
metadata:                   # Optional nested block
  author: my-org
  version: "1.0"
---

# My Skill

Instructions for the AI agent...
```

Required fields:
- `name` - The skill identifier

Optional fields:
- `description` - What this skill does and when to use it
- `allowed-tools` - Comma-separated string or YAML array of allowed tool names
- `license` - SPDX license identifier (e.g. `MIT`, `Apache-2.0`)
- `metadata.author` - Author or organization name
- `metadata.version` - Semantic version string (e.g. `"1.0"`)

The `license`, `metadata.author`, and `metadata.version` fields are displayed by `skillshub info` when present.

Optional subdirectories:
- `scripts/` - Executable scripts the agent can run
- `references/` - Documentation to be loaded into context

## Creating a Tap (Optional)

Any GitHub repository can be a tap. Just add folders with `SKILL.md` files anywhere in your repo:

```
my-skills-repo/
├── skills/
│   ├── python-testing/
│   │   └── SKILL.md
│   └── code-review/
│       └── SKILL.md
├── advanced/
│   └── refactoring/
│       └── SKILL.md
└── README.md
```

All skills are automatically discovered when users add your repo:

```bash
skillshub tap add user/my-skills-repo
skillshub install user/my-skills-repo/python-testing
```

## Migration

If you have an existing installation from before the tap system was introduced, skillshub will automatically migrate your skills on the first run. You can also run migration manually:

```bash
skillshub migrate
```

## Development

```bash
# Install pre-commit (one option)
python -m pip install --user pre-commit

# Install git hooks (requires `pre-commit`)
pre-commit install --install-hooks
pre-commit install --hook-type pre-push --install-hooks

# Run all checks locally
pre-commit run --all-files
```
