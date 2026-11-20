# Dedicated prune command

**Date:** 2026-11-19

## TL;DR

Add `skillshub prune [owner/repo]` to uninstall skills whose sources no longer list them, without updating any other installed skills. Add `--dry-run` to show what would be removed. Keep the existing update flags for users who want to update and prune in one command.

## Problem

Skills removed upstream stay installed and linked by default. Today, removing them in bulk requires `update --prune` or `tap update --prune`. The first can update unrelated installs; the second refreshes the registry and skips gist taps. Neither offers a preview limited to the skills that would be uninstalled.

## Constraints

A stale cache or failed source check is not proof that a skill was deleted. The command must keep skills from sources it cannot verify and report those sources. It must also keep skills intentionally retained by `tap remove --keep-skills`; a missing tap record is not proof of upstream deletion.

## Proposed approach

```bash
skillshub prune                          # Check all installed skills and remove confirmed orphans
skillshub prune owner/repo               # Check only installs from this tap
skillshub prune owner/repo --dry-run     # Preview without uninstalling anything
```

Check only sources with installed skills. For Git taps, refresh the configured branch once per tap and scan for skill names. For gist installs, fetch each gist once and check its skill names even if its timestamp is unchanged. For installs from the bundled copy of the default tap, check the bundled registry. Treat a successful, complete scan with no skills as a valid empty source. An incomplete scan must not authorize deletion.

Print candidates by full skill name in sorted order. Dry runs may fetch source data, but they must leave installed files, agent links, and the database unchanged. A normal run uses the same removal code as `uninstall`. Report source-check and removal failures with a nonzero exit status so scripts can distinguish an incomplete run from a clean result. Continue checking other sources when one fails.

## Alternatives considered

`tap prune` would fit Git taps, but the top-level `prune` belongs alongside `install` and `uninstall` and can cover gist installs too. Pruning from the cached tap registry would avoid fetching sources, but an out-of-date registry can produce an incorrect deletion.

## Tradeoffs

Preview and removal both need current source data, so an offline run cannot prune remote installs. Treat a rename as removal of the old name; the command never installs the replacement. It also does not replace `clean links`, which removes managed symlinks across agents.

## Migration

Existing `update --prune` and `tap update --prune` calls keep their current behavior. Use `prune` when cleanup is the only goal.

## Next Steps

1. Add `Prune` to `src/cli.rs` and dispatch it from `src/main.rs`.
2. Add the prune flow under `src/registry/`. Reuse Git and gist fetching and the uninstall helper. Adapt tap discovery to distinguish a confirmed empty source from an incomplete scan, while preserving the existing update commands' behavior.
3. Add focused tests using local fixtures for tap scoping, dry-run safety, removal of the last skill, and preservation after a failed or incomplete scan. Verify that retained installs from removed taps and unrelated agent links survive. Run a CLI smoke test against a local Git tap.
4. Document the command in `README.md`, `CLAUDE.md`, and the CLI reference. Replace this plan with a design note after implementation.
