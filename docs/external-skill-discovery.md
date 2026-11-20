# External skill discovery

`skillshub external scan` finds skills installed outside Skillshub. It checks `~/.agents/skills`, the [shared store used by Vercel Skills](https://github.com/vercel-labs/skills/blob/50894c1d98bc45ca1ab127213e59d6525065b5e7/src/installer.ts#L116-L119), and the skill directories of detected agents. The scan labels a skill by its source directory. It does not infer which tool put it there.

## Global discovery and linking

`external scan` and `link` use the same scanner. The shared store is a source, never a destination. The scanner reads real skill directories with a `SKILL.md` file, even if no agents are detected. It ignores shared-store symlinks, including when the store itself redirects to another directory. Existing tracked sources keep their names; for a new name, the shared store takes precedence over an agent-local copy. The scanner does not need Node.js, a lockfile, or network access.

`link` reuses the shared copy. It skips agents that [already read the global shared store](https://github.com/vercel-labs/skills/blob/50894c1d98bc45ca1ab127213e59d6525065b5e7/src/agents.ts#L941-L954), leaves existing destination entries intact, and does not link a shared skill whose source disappeared. `npx skills` still handles updates and removal for its own installs. `skillshub clean links` does not remove links into the shared store because a shared target does not identify which tool owns a link. Removing a source can leave a dangling link.

## Project discovery

`skillshub external scan --project <DIR>` reports skills under the selected project's `.agents/skills` and supported agent skill directories. It lists paths relative to the project, reports a shared source only once, and skips directories outside the project. It does not open or migrate the global database, edit the project, or create links. Use `--project .` to scan the current directory.
