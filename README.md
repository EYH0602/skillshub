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
skillshub tap add https://github.com/anthropics/skills
skillshub install anthropics/skills/frontend-design

skillshub tap add https://github.com/vercel-labs/agent-skills
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

# Add with a specific commit (permalink)
skillshub add https://github.com/user/repo/tree/abc1234/skills/my-skill
```

The skill will be organized under the repository identifier (e.g., `owner/repo/my-skill`).

### Skill Management

```bash
# List all available and installed skills
skillshub list

# Search for skills
skillshub search python

# Install a skill from a tap (format: owner/repo/skill)
skillshub install EYH0602/skillshub/code-reviewer

# Install a specific version (by commit)
skillshub install EYH0602/skillshub/code-reviewer@abc1234

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
skillshub tap add https://github.com/anthropics/skills
skillshub tap add https://github.com/vercel-labs/agent-skills

# Update tap registries (re-discover skills)
skillshub tap update                        # Update all taps
skillshub tap update anthropics/skills      # Update specific tap

# Remove a tap
skillshub tap remove vercel-labs/agent-skills
```

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

## Supported Agents

Skillshub automatically detects and links to these coding agents:

| Agent    | Directory     | Skills Path          |
| -------- | ------------- | -------------------- |
| Claude   | `~/.claude`   | `~/.claude/skills`   |
| Codex    | `~/.codex`    | `~/.codex/skills`    |
| OpenCode | `~/.opencode` | `~/.opencode/skill`  |
| Aider    | `~/.aider`    | `~/.aider/skills`    |
| Cursor   | `~/.cursor`   | `~/.cursor/skills`   |
| Continue | `~/.continue` | `~/.continue/skills` |

## GitHub API Rate Limiting

Skillshub uses the GitHub API to discover skills in repositories. Unauthenticated requests are limited to 60 per hour, which may cause errors when adding taps or listing skills.

To avoid rate limiting, set a GitHub personal access token:

```bash
export GITHUB_TOKEN=your_token_here
skillshub tap add https://github.com/anthropics/skills
```

You can generate a token at https://github.com/settings/tokens (no special scopes needed for public repos).

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
name: my-skill
description: What this skill does and when to use it
---

# My Skill

Instructions for the AI agent...
```

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
skillshub tap add https://github.com/user/my-skills-repo
skillshub install my-skills-repo/python-testing
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
