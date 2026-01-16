# [WIP] Skillshub

Skillshub is a package manager for AI coding agent skills. 
Install skills once and link them to every detected agent so all of your agents stay in sync.

NOTE: work in progress and currently missing feature. VERY buggy.

## Why Skillshub

- One install, many agents: a single skills registry in `~/.skillshub/skills`
- One command to sync: `skillshub link` wires skills into all detected agents
- Clear skill format: each skill lives in its own folder with `SKILL.md` metadata

## Installation

### From Cargo (recommended)

```bash
cargo install skillshub
```

### From Source

```bash
git clone <repo-url> skillshub
cd skillshub
cargo install --path .
```

## Quick Start

```bash
# Install all available skills
skillshub install-all

# Link installed skills to every detected agent
skillshub link

# See which agents were detected
skillshub agents
```

## Common Commands

```bash
# List all available skills
skillshub list

# Install a specific skill
skillshub install code-reviewer

# Show detailed info about a skill
skillshub info code-reviewer

# Uninstall a skill
skillshub uninstall code-reviewer
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

1. Skills are installed to `~/.skillshub/skills/`
2. Running `skillshub link` creates symlinks from each agent's skills directory to the installed skills
3. Re-run `skillshub link` any time to keep all agents synchronized

## Skill Format

Create a new directory under `skills/` with a `SKILL.md` file:

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
