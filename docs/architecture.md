# Architecture

```
skillshub/
├── src/
│   ├── main.rs                 # CLI entry point
│   ├── cli.rs                  # CLI command definitions (clap)
│   ├── agent.rs                # Agent detection
│   ├── skill.rs                # Skill discovery and parsing
│   ├── paths.rs                # Path utilities
│   ├── util.rs                 # General utilities
│   ├── commands/               # Command implementations
│   │   ├── mod.rs
│   │   ├── agents.rs           # Show detected agents
│   │   ├── clean.rs            # Clean cache and links
│   │   ├── external.rs         # External skills management
│   │   └── link.rs             # Link skills to agents
│   └── registry/               # Tap-based registry system
│       ├── mod.rs
│       ├── models.rs           # Data structures
│       ├── db.rs               # Database operations (~/.skillshub/db.json)
│       ├── github.rs           # GitHub API integration
│       ├── tap.rs              # Tap management
│       ├── skill.rs            # Skill install/uninstall/update
│       └── migration.rs        # Old installation migration
├── skills/                     # Bundled skills (default tap)
│   └── <skill-name>/
│       ├── SKILL.md            # Skill metadata and instructions
│       ├── scripts/            # Optional executable scripts
│       └── references/         # Optional documentation
├── Cargo.toml                  # Rust package config
└── README.md                   # User documentation
```

## Data Flow

```
GitHub Tap Repository          Local Database           Installed Skills
┌─────────────────────┐       ┌──────────────┐        ┌─────────────────────┐
│ any/path/           │──────▶│ db.json      │◀──────▶│ ~/.skillshub/       │
│   SKILL.md          │       │ - taps       │        │   skills/           │
│   (auto-discovered) │       │ - installed  │        │     owner/repo/skill│
└─────────────────────┘       │ - external   │        └─────────────────────┘
                              └──────────────┘
                                     ▲
                                     │ discovers
                              ┌──────┴──────┐
                              │ Agent dirs  │
                              │ (external   │
                              │  skills)    │
                              └─────────────┘
```

## Key Concepts

- **Taps**: Git repositories containing skills (like Homebrew taps). Skills are auto-discovered by scanning for `SKILL.md` files.
- **Skills**: Reusable instruction sets for AI coding agents, defined in `SKILL.md` files
- **Database**: `~/.skillshub/db.json` tracks installed skills, their versions, and external skills
- **Installation**: Skills are downloaded/copied to `~/.skillshub/skills/<owner>/<repo>/<skill>/`
- **Linking**: Per-skill symlinks are created from agent skill directories
- **External Skills**: Skills installed through other means (marketplace, manual) are discovered and synced

## Supported Agents

| Agent    | Directory      | Skills Path           |
| -------- | -------------- | --------------------- |
| Claude   | `~/.claude`    | `~/.claude/skills`    |
| Codex    | `~/.codex`     | `~/.codex/skills`     |
| OpenCode | `~/.opencode`  | `~/.opencode/skill`   |
| Aider    | `~/.aider`     | `~/.aider/skills`     |
| Cursor   | `~/.cursor`    | `~/.cursor/skills`    |
| Continue | `~/.continue`  | `~/.continue/skills`  |
| Trae     | `~/.trae`      | `~/.trae/skills`      |
| Kimi     | `~/.kimi`      | `~/.kimi/skills`      |
| OpenClaw | `~/.openclaw`  | `~/.openclaw/skills`  |
| ZeroClaw | `~/.zeroclaw`  | `~/.zeroclaw/skills`  |

To add a new agent, update `KNOWN_AGENTS` in `src/agent.rs`.

## Skill Format

Each skill has a `SKILL.md` with YAML frontmatter:

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
# Skill instructions in markdown...
```

Required: `name`. Optional: `description`, `allowed-tools`, `license`, `metadata.author`, `metadata.version`.

Optional subdirectories: `scripts/` (executables), `references/` or `resources/` (documentation).

## Tap Format

Any GitHub repository can be a tap. Skills are automatically discovered by scanning for folders containing a `SKILL.md` file anywhere in the repository. No special configuration required.
