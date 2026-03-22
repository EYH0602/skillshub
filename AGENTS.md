# AGENTS.md - Project Context for Coding Agents

## Project Purpose

Skillshub is a package manager for AI coding agent skills - like Homebrew for skills.
It provides a CLI to install, manage, and link reusable skills to various coding agents.

For architecture, CLI reference, supported agents, and skill/tap formats, see `docs/`.

## Development

- Rust 2021 edition
- Always update `README.md` and `CLAUDE.md` when you introduce new features or libraries.
- Always write unit tests for new features.
- Always test your code after implementation.
- Use `pre-commit install --install-hooks` to enable local git hooks.
- Do not commit or create pull requests — let the human do them. Suggest commit messages.

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
cargo run -- install EYH0602/skillshub/code-reviewer
cargo run -- link
cargo run -- agents
cargo run -- external list
cargo run -- external scan
```

### Planning

Use `plans/` for planning out your work.

- When adding a new feature, ALWAYS first create a plan in `plans/` and ask for review from the human developer before implementation.
- Include the problem background, proposed solution, and implementation steps in your plan.
- Commit the plan to the repo and ask for review before implementation.
- After the plan is fully implemented, rewrite it as a design doc in `docs/`, and remove it from `plans/`.

### Scratch Space

Do not create ad-hoc files at repo root.
- Use `.agents/sandbox/` for throwaway exploration that will not be committed.
- Use `.agents/accomplished/` for recording completed tasks and summaries.
