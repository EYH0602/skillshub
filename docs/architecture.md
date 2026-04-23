# Architecture

```
skillshub/
├── src/
│   ├── main.rs                 # CLI entry point
│   ├── cli.rs                  # CLI command definitions (clap)
│   ├── agent.rs                # Agent detection
│   ├── skill.rs                # Skill discovery and parsing
│   ├── paths.rs                # Path utilities
│   ├── util.rs                 # General utilities (copy_dir_contents, etc.)
│   ├── commands/               # Command implementations
│   │   ├── mod.rs
│   │   ├── agents.rs           # Show detected agents
│   │   ├── clean.rs            # Clean cache and links
│   │   ├── doctor.rs           # Diagnostic checks (skillshub doctor)
│   │   ├── external.rs         # External skills management
│   │   └── link.rs             # Link skills to agents
│   └── registry/               # Tap-based registry system
│       ├── mod.rs
│       ├── models.rs           # Data structures (TapInfo, InstalledSkill, etc.)
│       ├── db.rs               # Database operations (~/.skillshub/db.json)
│       ├── git.rs              # Git CLI operations (clone, pull, ensure_clone)
│       ├── github.rs           # GitHub API (gists, star lists, URL parsing)
│       ├── tap.rs              # Tap management + local skill discovery
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
GitHub Tap Repository          Local Clone                 Installed Skills
┌─────────────────────┐       ┌──────────────────┐        ┌─────────────────────┐
│ any/path/           │──git──▶ ~/.skillshub/     │──copy─▶│ ~/.skillshub/       │
│   SKILL.md          │ clone │   taps/           │        │   skills/           │
│   (auto-discovered) │       │     owner/repo/   │        │     owner/repo/skill│
└─────────────────────┘       └──────────────────┘        └─────────────────────┘
                                     │                            │
                                     ▼                            │
                              ┌──────────────┐                    │
                              │ db.json      │◀───────────────────┘
                              │ - taps       │
                              │ - installed  │
                              │ - external   │
                              └──────────────┘
                                     ▲
                                     │ discovers
                              ┌──────┴──────┐
                              │ Agent dirs  │
                              │ (external   │
                              │  skills)    │
                              └─────────────┘

Local directory layout:
~/.skillshub/
├── db.json                     # Database
├── taps/                       # Cloned tap repositories
│   └── owner/
│       └── repo/               # Shallow git clone
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

## Key Concepts

- **Taps**: Git repositories containing skills (like Homebrew taps). Skills are auto-discovered by scanning for `SKILL.md` files.
- **Skills**: Reusable instruction sets for AI coding agents, defined in `SKILL.md` files
- **Database**: `~/.skillshub/db.json` tracks installed skills, their versions, and external skills
- **Installation**: Skills are copied from local tap clones to `~/.skillshub/skills/<owner>/<repo>/<skill>/`
- **Linking**: Per-skill symlinks are created from agent skill directories
- **External Skills**: Skills installed through other means (marketplace, manual) are discovered and synced

## Supported Agents

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
