# GEMINI.md - canon

Gemini-specific notes for this repo. Read [AGENTS.md](./AGENTS.md) first for
the shared project contract, safety rules, Beads workflow, and quality gate.

## Commands

Prefer commands that return bounded text or JSON:

```bash
br ready --json
br list --status open --json
br dep cycles --json
br sync --flush-only

cargo run -- --describe
cargo run -- doctor health --json
cargo run -- doctor robot-docs
cargo run -- doctor --robot-triage

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Do not run bare `bv`; use robot modes such as `bv --robot-next`,
`bv --robot-plan`, or `bv --robot-insights`.

## Permissions

Assume this is a shared worktree. Read freely, edit only task-scoped files,
and use exact Agent Mail reservations when available. If the Gemini harness
asks for filesystem or network approval, use the approval path instead of
working around it.

Do not discard uncommitted changes you did not make, delete files, run
destructive git commands, or force push without an explicit user instruction.

## Hooks

No repo-local Gemini hook or config file is checked in. CI is the authoritative
automation surface: [.github/workflows/ci.yml](./.github/workflows/ci.yml)
runs fmt, clippy, and tests; [.github/workflows/release.yml](./.github/workflows/release.yml)
builds release artifacts and updates the Homebrew tap.

## Environment

`canon` is a Rust 2024 CLI/library with no checked-in `rust-toolchain.toml`.
Use direct Cargo commands. Normal tests should not require provider credentials
or live network calls.

## Session Caveats

Ground behavior claims in current files. `docs/PLAN_CANON.md` is the source of
truth for the core lookup contract, and `docs/IDENTITY_ARCHITECTURE.md`
defines the boundary between exact lookup and offline workbenches.

Last reviewed: 2026-05-27.
