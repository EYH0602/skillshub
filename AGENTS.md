# AGENTS.md - Project Context for Coding Agents

## Project Purpose

Skillshub is a package manager for AI coding agent skills - like Homebrew for skills.
It provides a CLI to install, manage, and link reusable skills to various coding agents.

## Architecture

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

## Key Concepts

- **Taps**: Git repositories containing skills (like Homebrew taps). Skills are auto-discovered by scanning for `SKILL.md` files.
- **Skills**: Reusable instruction sets for AI coding agents, defined in `SKILL.md` files
- **Database**: `~/.skillshub/db.json` tracks installed skills, their versions, and external skills
- **Installation**: Skills are downloaded/copied to `~/.skillshub/skills/<tap>/<skill>/`
- **Linking**: Per-skill symlinks are created from agent skill directories
- **External Skills**: Skills installed through other means (marketplace, manual) are discovered and synced
- **Agents**: Coding assistants like Claude, Codex, OpenCode, Aider, Cursor, Continue

## Data Flow

```
GitHub Tap Repository          Local Database           Installed Skills
┌─────────────────────┐       ┌──────────────┐        ┌─────────────────────┐
│ any/path/           │──────▶│ db.json      │◀──────▶│ ~/.skillshub/       │
│   SKILL.md          │       │ - taps       │        │   skills/           │
│   (auto-discovered) │       │ - installed  │        │     tap/skill/      │
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

## Supported Agents

Each agent has its own skills subdirectory name:

| Agent    | Directory     | Skills Path          |
| -------- | ------------- | -------------------- |
| Claude   | `~/.claude`   | `~/.claude/skills`   |
| Codex    | `~/.codex`    | `~/.codex/skills`    |
| OpenCode | `~/.opencode` | `~/.opencode/skill`  |
| Aider    | `~/.aider`    | `~/.aider/skills`    |
| Cursor   | `~/.cursor`   | `~/.cursor/skills`   |
| Continue | `~/.continue` | `~/.continue/skills` |

## CLI Commands

### Adding Skills from URLs
```bash
skillshub add <github-url>                  # Add skill directly from GitHub URL
# Example: skillshub add https://github.com/user/repo/tree/commit/path/to/skill
```

### Skill Management
```bash
skillshub list                              # List all available skills
skillshub search <query>                    # Search skills across all taps
skillshub install <owner/repo/skill>[@commit]  # Install a skill
skillshub uninstall <owner/repo/skill>      # Remove installed skill
skillshub update [owner/repo/skill]         # Update skill(s) to latest
skillshub info <owner/repo/skill>           # Show skill details
skillshub install-all                       # Install all from default tap
```

### Tap Management

The default tap is `EYH0602/skillshub` (bundled). Add third-party taps like `anthropics/skills` or `vercel-labs/agent-skills`.

```bash
skillshub tap list                          # List configured taps
# Skills column shows installed/available counts (e.g., 2/15 or 1/?)
skillshub tap add <github-url>              # Add a third-party tap
skillshub tap remove <owner/repo>           # Remove a tap
skillshub tap update [owner/repo]           # Refresh tap registry
```

### Agent Management
```bash
skillshub link                              # Link skills to detected agents
skillshub agents                            # Show detected agents
```

### External Skills Management
```bash
skillshub external list                     # List discovered external skills
skillshub external scan                     # Scan for external skills
skillshub external forget <name>            # Stop tracking an external skill
```

External skills are skills found in agent directories that weren't installed via skillshub (e.g., from Claude marketplace or manual installation). They are automatically discovered from all agent directories during `skillshub link` and synced to all other agents.

### Migration
```bash
skillshub migrate                           # Migrate old-style installations
```

## Development Notes

- Rust 2021 edition
- Uses `clap` for CLI parsing
- Uses `tabled` for table output
- Uses `colored` for terminal colors
- Uses `serde` + `serde_json` for database
- Uses `serde_yaml` for SKILL.md frontmatter parsing
- Uses `reqwest` for HTTP requests (GitHub API)
- Uses `flate2` + `tar` for tarball extraction
- Uses `chrono` for timestamps

### Developing

- Before start working, refresh your knowledge from contents in `.agents` first.
- Always update `README.md` and `CLAUDE.md` when you introduce new features or libraries.
- Always write unit tests for integration testing and functional testing of new features.
- Always test your code after your implementation.
- Use `pre-commit install --install-hooks` (and optionally `--hook-type pre-push`) to enable local git hooks.

#### Scratch Space

Do not create ad-hoc files at repo root.
1. Use `.agents/sandbox/` for throwaway exploration that will not be committed.
2. Use `.agents/notes/` for longer-term notes that may be useful later.
Always write down your plans and reasoning for future reference when encountering major tasks,
like adding a feature.
3. Use `.agents/accomplished/` for recording completed tasks and the summary of what we did,
this may be useful for future reference.

### Building

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo run -- list        # Run directly
```

### Testing locally

```bash
cargo run -- tap list
cargo run -- list
cargo run -- install skillshub/code-reviewer
cargo run -- link
cargo run -- agents
cargo run -- external list
cargo run -- external scan
```

## Skill Format

Each skill has a `SKILL.md` with YAML frontmatter:

```yaml
---
name: skill-name
description: What this skill does
allowed-tools: Tool1, Tool2 # Optional, comma-separated or array
---
# Skill instructions in markdown...
```

Optional subdirectories:

- `scripts/` - Executable scripts the agent can run
- `references/` or `resources/` - Documentation loaded into context

## Tap Format

Any GitHub repository can be a tap. Skills are automatically discovered by scanning for folders containing a `SKILL.md` file anywhere in the repository. No `registry.json` or special configuration is required.

Example tap structure:
```
my-tap-repo/
├── skills/
│   ├── skill-a/
│   │   └── SKILL.md
│   └── skill-b/
│       └── SKILL.md
├── other/path/
│   └── another-skill/
│       └── SKILL.md
└── README.md
```

All three skills above would be discovered when adding this repo as a tap.

## Common Tasks

### Adding a new skill to the default tap

1. Create directory under `skills/<skill-name>/`
2. Add `SKILL.md` with frontmatter and instructions
3. Optionally add `scripts/` and `references/` subdirectories
4. Test with `cargo run -- info skillshub/<skill-name>`

### Adding a new agent

1. Update `KNOWN_AGENTS` in `src/agent.rs`
2. Specify the agent directory and skills subdirectory name

### Creating a third-party tap

1. Create a GitHub repository
2. Add skill folders with `SKILL.md` files anywhere in the repo
3. Users can add with: `skillshub tap add https://github.com/user/repo`
4. Skills are automatically discovered by scanning for `SKILL.md` files
