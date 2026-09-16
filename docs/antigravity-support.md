# Google Antigravity Agent Support

**Date**: 2026-09-15
**Status**: Implemented
**Issue**: [#79](https://github.com/EYH0602/skillshub/issues/79)

## Overview

Google Antigravity (https://antigravity.google/) is an agentic AI development platform and IDE/CLI (`agy`) ecosystem. It natively supports the Agent Skills open standard (`SKILL.md` files with YAML frontmatter).

Skillshub supports Antigravity as a first-class coding agent by detecting `~/.gemini/config` and linking skills to `~/.gemini/config/skills`.

## Behavior

- **Detection**: Skillshub checks `$HOME/.gemini/config` during agent discovery (`discover_agents`).
- **Linking**: When `skillshub link` runs, skills are symlinked into `~/.gemini/config/skills`.
- **External Skills**: Skills manually placed or installed in `~/.gemini/config/skills` are recognized by `skillshub external scan` / `skillshub external list`.
- **Agent Status**: Reported in `skillshub agents` with its linked status and skill count.
- **Cleanup**: `skillshub clean` and `skillshub clean all` remove managed symlinks from `~/.gemini/config/skills`.

## Directory Mapping

| Agent | Directory | Skills Path |
| --- | --- | --- |
| Antigravity | `~/.gemini/config` | `~/.gemini/config/skills` |

## Key Components

- `src/agent.rs`: Added `(".antigravity", ".gemini/config", "skills")` to `KNOWN_AGENTS`.
- `tests/agent_linking_test.rs`: Added `(".gemini/config", "skills")` to integration tests.
- `README.md` & `docs/architecture.md`: Updated Supported Agents tables.
