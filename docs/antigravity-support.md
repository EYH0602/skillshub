# Google Antigravity Agent Support

**Date**: 2026-09-15
**Status**: Implemented
**Issue**: [#79](https://github.com/EYH0602/skillshub/issues/79)

## Overview

Google Antigravity (https://antigravity.google/) is an agentic AI development platform and IDE/CLI (`agy`) ecosystem. It natively supports the Agent Skills open standard (`SKILL.md` files with YAML frontmatter).

Skillshub supports Antigravity as a first-class coding agent by detecting `~/.antigravity` and linking skills to `~/.antigravity/skills`.

## Behavior

- **Detection**: Skillshub checks `$HOME/.antigravity` during agent discovery (`discover_agents`).
- **Linking**: When `skillshub link` runs, skills are symlinked into `~/.antigravity/skills`.
- **External Skills**: Skills manually placed or installed in `~/.antigravity/skills` are recognized by `skillshub external scan` / `skillshub external list`.
- **Agent Status**: Reported in `skillshub agents` with its linked status and skill count.
- **Cleanup**: `skillshub clean` and `skillshub clean all` remove managed symlinks from `~/.antigravity/skills`.

## Directory Mapping

| Agent | Directory | Skills Path |
| --- | --- | --- |
| Antigravity | `~/.antigravity` | `~/.antigravity/skills` |

## Key Components

- `src/agent.rs`: Added `(".antigravity", "skills")` to `KNOWN_AGENTS`.
- `tests/agent_linking_test.rs`: Added `(".antigravity", "skills")` to integration tests.
- `README.md` & `docs/architecture.md`: Updated Supported Agents tables.
