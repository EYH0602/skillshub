# AGENTS.md - Project Context for Coding Agents

## Project Purpose

Skillshub is a package manager for AI coding agent skills - like Homebrew for skills.
It provides a CLI to install, manage, and link reusable skills to various coding agents.

## Architecture

```
skillshub/
├── src/
│   └── main.rs             # CLI implementation (clap-based)
├── skills/                 # Available skills (source registry)
│   └── <skill-name>/
│       ├── SKILL.md        # Skill metadata and instructions
│       ├── scripts/        # Optional executable scripts
│       └── references/     # Optional documentation
├── Cargo.toml              # Rust package config
└── README.md               # User documentation
```

## Key Concepts

- **Skills**: Reusable instruction sets for AI coding agents, defined in `SKILL.md` files
- **Installation**: Skills are copied from `skills/` to `~/.skillshub/skills/`
- **Linking**: Symlinks are created from agent directories to installed skills
- **Agents**: Coding assistants like Claude, Codex, OpenCode, Aider, Cursor, Continue

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

```bash
skillshub install-all       # Install all skills to ~/.skillshub
skillshub install <name>    # Install specific skill
skillshub uninstall <name>  # Remove installed skill
skillshub list              # List available skills
skillshub info <name>       # Show skill details
skillshub link              # Link skills to detected agents
skillshub agents            # Show detected agents
```

## Development Notes

- Rust 2021 edition
- Uses `clap` for CLI parsing
- Uses `tabled` for table output
- Uses `colored` for terminal colors
- Uses `serde` + `serde_yaml` for SKILL.md frontmatter parsing

### Developing

- Before start working, refresh your knowledge from contents in `.agents` first.
- Always update `README.md` and `AGENTS.md` when you introduce new features or libraries.
- Always write unit tests for integration testing and functional testing of new features.

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
cargo run -- install-all
cargo run -- link
cargo run -- agents
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

## Common Tasks

### Adding a new skill

1. Create directory under `skills/<skill-name>/`
2. Add `SKILL.md` with frontmatter and instructions
3. Optionally add `scripts/` and `references/` subdirectories
4. Test with `cargo run -- info <skill-name>`

### Adding a new agent

1. Update `KNOWN_AGENTS` in `src/main.rs`
2. Specify the agent directory and skills subdirectory name
