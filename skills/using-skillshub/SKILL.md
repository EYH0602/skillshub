---
name: using-skillshub
description: How to install, link, and manage AI-agent skills with the `skillshub` CLI. Use this skill whenever the user wants to install a skill, add or remove a tap, share skills across multiple coding agents, sync `~/.skillshub/`, troubleshoot a missing or broken skill, write a `SKILL.md`, or do anything that touches the `skillshub` command — even when they describe the goal without naming the tool (e.g. "I want this skill in Cursor too", "why doesn't Codex see my skill", "package this folder as a skill").
license: MIT
metadata:
  author: EYH0602
  version: "1.0"
---

# Using Skillshub

Skillshub is a package manager for AI coding-agent skills. Think Homebrew, but
for the `SKILL.md` files that agents like Claude Code, Codex, Cursor, OpenCode,
Continue, etc. read for instructions. One install, many agents — `skillshub
link` wires installed skills into every detected agent.

This skill teaches you (the agent) how to drive the CLI on the user's behalf.

## When to reach for which command

Match the user's actual intent before picking a command. The most common
mismatch is reaching for `tap add` when the user just wants one skill from a
repo — `skillshub add <url>` is usually the faster path.

| User wants to… | Command |
|---|---|
| Install one specific skill from a GitHub URL | `skillshub add <github-url-to-skill-folder>` |
| Install a skill from a Gist | `skillshub add <gist-url>` |
| Subscribe to a whole repo of skills | `skillshub tap add <owner/repo>` then `skillshub install <owner/repo/skill>` (or `--install` to grab everything) |
| Make every detected agent see the installed skills | `skillshub link` |
| See which agents skillshub knows about | `skillshub agents` |
| Find a skill across all taps | `skillshub search <query>` |
| Update everything | `skillshub update` (skills) and `skillshub tap update` (registries) |
| Diagnose why something is off | `skillshub doctor` |
| Fully uninstall | `skillshub clean all` |

The full command surface lives in `references/cli-reference.md` — load it when
the user asks about something not in the table above, or when you need exact
flag names.

## The mental model

Three locations matter. Confusing them is the source of most "why doesn't my
agent see this skill?" questions:

```
GitHub tap repo  ──git clone──▶  ~/.skillshub/taps/owner/repo/   (cache)
                                          │
                                          │ install copies the skill
                                          ▼
                                ~/.skillshub/skills/owner/repo/skill/   (canonical)
                                          │
                                          │ link symlinks per-skill into…
                                          ▼
                                ~/.claude/skills/, ~/.codex/skills/, …  (agent dirs)
```

- **Taps** are git repos with `SKILL.md` files anywhere inside. Skillshub
  walks the clone to discover them — no manifest required.
- **Installed skills** live in `~/.skillshub/skills/<owner>/<repo>/<skill>/`.
  This is the canonical copy.
- **Linked skills** are per-skill symlinks from each agent's skills directory
  back to the canonical copy. This is why one install fans out to every agent.
- **External skills** are `SKILL.md` folders an agent already had (e.g. from a
  marketplace). `skillshub link` discovers them and replicates them to the
  other agents so everyone stays in sync.

If the user reports "agent X doesn't see skill Y", check in this order:
1. Is the skill installed? `skillshub list` — look for it.
2. Is agent X detected? `skillshub agents`.
3. Has `skillshub link` been run since installing? Re-run it; symlinks are
   not created automatically on install.
4. Run `skillshub doctor` for git/clone/orphan diagnostics.

## Common workflows

### Install one skill from a URL
The user pasted a GitHub URL pointing at a skill folder.

```bash
skillshub add https://github.com/owner/repo/tree/main/path/to/skill
skillshub link
```

Use `add` (not `tap add`) when they only want this one skill — it bypasses
tap management entirely and creates a synthetic `owner/repo` namespace
behind the scenes.

### Subscribe to a tap and install everything
The user wants a whole collection.

```bash
skillshub tap add anthropics/skills --install
skillshub link
```

Without `--install` you get the registry but no installed skills; you'd then
pick with `skillshub install anthropics/skills/<name>` or grab them all with
`skillshub tap install-all anthropics/skills`.

### Add a skill from a Gist
Gists are flat (no folders), so each file with valid `SKILL.md` frontmatter
becomes its own skill under `owner/gists/<skill-name>`.

```bash
skillshub add https://gist.github.com/user/<gist_id>
skillshub link
```

Set `GITHUB_TOKEN` first to avoid rate limiting — gists hit the GitHub API,
unlike regular taps which use `git clone`.

### Sync after editing or installing anything
Anytime installed skills change — first install, update, uninstall, or after
the user manually drops a skill into one agent's directory — re-run:

```bash
skillshub link
```

It's idempotent and cheap. Suggest it whenever the user reports
sync-feels-off symptoms.

### Update
```bash
skillshub tap update             # refresh all tap registries
skillshub update                 # update every installed skill
skillshub update owner/repo/skill   # one specific skill
```

Tap update and skill update are separate steps because the registry (what's
*available*) and the install (what's *on disk*) are tracked independently.

### Uninstall a tap but keep its skills
Useful when the user wants to stop tracking a registry without losing what's
already linked into their agents.

```bash
skillshub tap remove owner/repo --keep-skills
```

### Full purge
```bash
skillshub clean all              # interactive confirm
skillshub clean all --confirm    # for scripts/CI
```

This removes every skillshub-managed symlink from every agent dir and
deletes `~/.skillshub/` entirely.

## Writing or editing a SKILL.md

When the user asks you to author a new skill or fix one, follow this format.
Only `name` is required; everything else is optional but worth filling in.

```yaml
---
name: skill-name
description: One-sentence trigger — what the skill does AND when to use it.
allowed-tools: Read, Edit, Bash       # optional, comma-separated or YAML array
license: MIT                          # optional, SPDX identifier
metadata:                             # optional nested block
  author: my-org
  version: "1.0"
---

# Skill Name

Imperative instructions for the agent...
```

A few rules worth internalising:

- **The description is the trigger.** Skillshub doesn't drive triggering, but
  Claude does — it picks skills based on the description. Make it specific
  about *when* to fire, not just *what* it does.
- **One folder per skill.** The folder name should match `name`.
- **Optional sibling dirs:** `scripts/` for executables the agent can run,
  `references/` for deeper docs the agent can load on demand.
- **Auto-discovery.** Anywhere in a tap, any folder containing `SKILL.md` is
  a skill. No manifest, no registration. This means the user can drop skills
  under `skills/`, `experimental/`, `agents/`, wherever — skillshub will find
  them.

## Supported agents

Skillshub auto-detects these and links per-skill into each:

Claude (`~/.claude`), Codex (`~/.codex`), OpenCode (`~/.opencode`), Aider
(`~/.aider`), Cursor (`~/.cursor`), Continue (`~/.continue`), Trae, Kimi,
OpenClaw, ZeroClaw, Kiro (uses `~/.kiro/steering`), Gemini, Copilot, Junie,
Augment, Warp, Cline.

Run `skillshub agents` to see which are present on the user's machine.

## Things to watch out for

- **`@commit` no longer works for non-gist taps.** Shallow clones can't check
  out arbitrary commits. If the user pastes `owner/repo/skill@<sha>`, expect
  a hard error and suggest installing latest instead.
- **Private repos** need git credential helpers or SSH keys configured. A
  `GITHUB_TOKEN` is *not* enough for tap operations — only for gists and
  star-list imports, which still hit the GitHub API.
- **`git` is a hard runtime dependency.** If `skillshub doctor` reports a git
  problem, fix that first; nothing tap-related works without it.
- **External skills are real.** When the user manually drops a skill into one
  agent's directory, `skillshub link` will replicate it to every other agent.
  This is intentional sync behaviour — flag it if it might surprise them.

## Where to dig deeper

- `references/cli-reference.md` — every command and flag
- `references/architecture.md` — directory layout, data flow, key crates
- The `skillshub` repo itself: https://github.com/EYH0602/skillshub
