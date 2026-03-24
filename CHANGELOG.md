# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-03-23

### Added

- Git-based tap management: taps are now cloned and updated via `git` instead of
  the GitHub API, eliminating rate-limit issues and improving performance.
- `skillshub doctor` command for environment diagnostics (checks `git`, config,
  cloned taps).
- `--branch` flag on `tap add` to track a non-default branch; the branch choice
  is persisted in `TapInfo`.
- Git resilience helpers (`check_git`, `ensure_clone`, `pull_or_reclone`) for
  robust clone recovery.
- Broader skill discovery via `walkdir` with duplicate-path warnings.

### Changed

- `tap add` clones the repository locally before discovering skills.
- `tap update` performs `git pull` and re-scans the local clone.
- `tap remove` cleans up the clone directory on disk.
- `install` prefers the local clone over an API download.
- `update` pulls from the local clone with API fallback removed.

### Removed

- GitHub API fallback for skill installation and updates; `git` is now a hard
  runtime dependency.
- Dead API helper functions in `github.rs`; `copy_dir_contents` moved to
  `util.rs`.

## [0.2.0] - 2026-03-22

### Added

- Skill installation from GitHub Gists (`install gist:<id>`).
- `clean all` command for full uninstall/purge of skillshub data.
- `star-list` command to import taps from a GitHub user's starred repositories.
- `install-all` installs every skill from all added taps.
- License, author, and version fields in `SKILL.md` frontmatter.
- Support for Kimi, OpenClaw, and ZeroClaw agents.
- Tap update diff: shows newly added and removed skills after `tap update`.
- Auto-uninstall of skills when removing a tap (with `--keep-skills` opt-out).

### Changed

- `tap remove` now auto-uninstalls associated skills by default; pass
  `--keep-skills` to retain them.
- Docs moved from `CLAUDE.md` to `docs/` directory; `CLAUDE.md` slimmed to
  process rules only.

### Fixed

- Normalize default-tap flags and guard impossible skill counts in `tap list`.
- Install default-tap skills from local bundled directory instead of network.
- Catch `reqwest`/`system-configuration` panics in `build_client`.
- Downgrade `wiremock` to 0.5 for stable Rust compatibility.
- Address clippy warnings in `github.rs`.
- Update docs with new `SKILL.md` fields and improve `info` display order.

### Removed

- Low-quality bundled skills pruned from the default tap.

## [0.1.10] - 2026-02-04

### Added

- Exponential back-off for GitHub API rate-limit retries.

### Fixed

- Use this repository (`EYH0602/skillshub`) as the default tap instead of the
  previous hardcoded value.

## [0.1.9] - 2026-01-30

### Added

- Trae IDE agent support.

### Fixed

- Discover skills when `SKILL.md` is at the repository root.

## [0.1.8] - 2026-01-19

### Fixed

- Detect repository's default branch instead of hardcoding `main`.

## [0.1.7] - 2026-01-18

### Added

- `clean` commands for cache and symlink cleanup.

## [0.1.6] - 2026-01-18

### Added

- `tap add` accepts `owner/repo` shorthand format.
- Integration test infrastructure and first test suite.
- `cache` and `install-all` commands for bulk tap operations.
- `scan` command to detect skills already present on disk.
- `GITHUB_TOKEN` authentication for API requests.
- Auto-discovery of skills from git repositories using `owner/repo` tap naming.

### Fixed

- Remove `skill-create` command that conflicted with the default Anthropic tap.

## [0.1.0] - 2026-01-15

### Added

- Initial CLI: `install`, `uninstall`, `list`, `link`, `info` commands.
- Tap registry with `tap add`, `tap remove`, `tap list`.
- Per-skill agent linking (creates config symlinks for each supported agent).
- Anthropic skills as the default tap.
- Basic table-formatted output for skill listings.

[Unreleased]: https://github.com/EYH0602/skillshub/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/EYH0602/skillshub/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/EYH0602/skillshub/compare/0.1.10...v0.2.0
[0.1.10]: https://github.com/EYH0602/skillshub/compare/d291d9e...0.1.10
[0.1.9]: https://github.com/EYH0602/skillshub/compare/0.1.8...d291d9e
[0.1.8]: https://github.com/EYH0602/skillshub/compare/0.1.7...0.1.8
[0.1.7]: https://github.com/EYH0602/skillshub/compare/0.1.6...0.1.7
[0.1.6]: https://github.com/EYH0602/skillshub/compare/d5bde06...0.1.6
[0.1.0]: https://github.com/EYH0602/skillshub/tree/d5bde06
