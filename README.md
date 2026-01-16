# Skillshub

A package manager for AI coding agent skills - like Homebrew for skills.

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

## Usage

### Install Skills

```bash
# Install all available skills to ~/.skillshub
skillshub install-all

# Install a specific skill
skillshub install code-reviewer
```

### Link to Coding Agents

```bash
# Link installed skills to all detected coding agents
skillshub link

# Show detected agents and their link status
skillshub agents
```

### Manage Skills

```bash
# List all available skills
skillshub list

# Show detailed info about a skill
skillshub info code-reviewer

# Uninstall a skill
skillshub uninstall code-reviewer
```

## Supported Agents

Skillshub automatically detects and links to these coding agents:

- `.claude` (Claude Code)
- `.codex` (OpenAI Codex)
- `.opencode` (OpenCode)
- `.aider` (Aider)
- `.cursor` (Cursor)
- `.continue` (Continue)

## How It Works

1. Skills are installed to `~/.skillshub/skills/`
2. Running `skillshub link` creates symlinks from each agent's `.skills` directory to the installed skills
3. Each skill contains a `SKILL.md` with metadata and instructions, plus optional scripts and references

## Adding New Skills

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
