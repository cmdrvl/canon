# CLAUDE.md - canon

Claude Code-specific notes for this repo. Read [AGENTS.md](./AGENTS.md)
first; it is the shared source of truth for scope, safety, Beads, Agent Mail,
quality gates, and session completion.

## Commands

Use non-interactive commands from the repository root:

```bash
br ready --json
br list --status open --json
br dep cycles --json
br sync --flush-only

cargo run -- --describe
cargo run -- doctor health --json
cargo run -- doctor capabilities --json
cargo run -- doctor --robot-triage

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

For registry maintenance work, prefer the implemented `registry next-id`,
`registry mint`, and `registry add-entry` commands over hand editing mapping
JSON. Use the self-contained examples in [README.md](./README.md) when you need
a copy-pasteable local smoke test.

Never run bare `bv`; use robot flags such as `bv --robot-next`.

## Permissions

Allowed repo operations: read files, edit exact task files, run Cargo gates,
run `br`/robot-safe `bv`, and inspect git state. Use Agent Mail reservations
when the tools are exposed.

Do not delete files, run destructive git commands, force push, or overwrite
other agents' changes unless the user explicitly directs it. Network access is
not needed for the normal quality gate; OpenFIGI behavior is tested with local
fixtures/mocks.

## Hooks

No repo-local `.claude/` hook or slash-command configuration is checked in.
Do not assume hooks will format, test, sync Beads, or update the Homebrew tap.
The checked-in automated gates are [.github/workflows/ci.yml](./.github/workflows/ci.yml)
and [.github/workflows/release.yml](./.github/workflows/release.yml).

## Environment

`canon` is a Rust 2024 Cargo project. There is no `rust-toolchain.toml`; CI
uses stable Rust. Release builds use `cargo build --release --locked` in the
release workflow, so keep `Cargo.lock` synchronized when dependency or version
metadata changes.

## Session Caveats

Core lookup remains exact byte match after ASCII-trim. Workbench commands such
as `canon org` and `canon resolve` can create registry knowledge, but ordinary
lookup does not become fuzzy or multi-column. If behavior and docs disagree,
start from [docs/PLAN_CANON.md](./docs/PLAN_CANON.md).

Last reviewed: 2026-05-27.
