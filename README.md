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
# Add a skill directly from a GitHub URL (easiest way)
skillshub add https://github.com/vercel-labs/agent-skills/tree/main/skills/react-best-practices

# Or install from the default tap
skillshub install skillshub/code-reviewer

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

The skill will be organized under the repository name (e.g., `repo/my-skill`).

### Skill Management

```bash
# List all available and installed skills
skillshub list

# Search for skills
skillshub search python

# Install a skill from a tap (format: tap/skill)
skillshub install skillshub/code-reviewer

# Install a specific version (by commit)
skillshub install skillshub/code-reviewer@abc1234

# Show detailed info about a skill
skillshub info skillshub/code-reviewer

# Update installed skills to latest version
skillshub update                           # Update all
skillshub update skillshub/code-reviewer   # Update one

# Uninstall a skill
skillshub uninstall skillshub/code-reviewer

# Install all skills from the default tap
skillshub install-all
```

### Tap Management (Optional)

Taps are repositories that contain skills with a registry. The default `skillshub` tap is included.

```bash
# List configured taps
skillshub tap list
# Skills column shows installed/available counts (e.g., 2/15 or 1/?)

# Add a third-party tap (requires registry.json)
skillshub tap add https://github.com/user/my-skills-tap

# Update tap registries
skillshub tap update            # Update all taps
skillshub tap update my-tap     # Update specific tap

# Remove a tap
skillshub tap remove my-tap
```

### Agent Linking

```bash
# Link installed skills to all detected agents
skillshub link

# Show which agents are detected
skillshub agents
```

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

## How It Works

1. Skills are organized by source: `~/.skillshub/skills/<repo-or-tap>/<skill>/`
2. A database at `~/.skillshub/db.json` tracks installed skills and their versions
3. Running `skillshub link` creates per-skill symlinks in each agent's skills directory
4. Re-run `skillshub link` any time to keep all agents synchronized

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

For organizing many skills, you can create a tap with a `registry.json`:

```json
{
  "name": "my-tap",
  "description": "My custom skills collection",
  "skills": {
    "my-skill": {
      "path": "skills/my-skill",
      "description": "What this skill does"
    }
  }
}
```

Users can then add your tap and install skills from it:

```bash
skillshub tap add https://github.com/user/my-tap
skillshub install my-tap/my-skill
```

## Migration

If you have an existing installation from before the tap system was introduced, skillshub will automatically migrate your skills on the first run. You can also run migration manually:

```bash
skillshub migrate
```
