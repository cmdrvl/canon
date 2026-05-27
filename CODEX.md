# CODEX.md - canon

Codex-specific notes for this repo. Read [AGENTS.md](./AGENTS.md) first; this
file records only Codex harness caveats and repo-local command shortcuts.

## Commands

Use structured, non-interactive commands:

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

For focused Rust checks, start with the relevant integration test, then run the
full gate before closing a bead. Useful examples:

```bash
cargo test --test registry_next_id
cargo test --test registry_add_entry
cargo test --test registry_mint
cargo test --test registry_id_scheme
```

## Permissions

Use `apply_patch` for manual edits. Read with `rg`, `sed`, `git diff`, and
other narrow shell commands. Reserve only exact files in Agent Mail when the
tools are available.

Do not use destructive git commands, delete files, stash/revert other agents'
work, or make broad scripted rewrites. Do not rely on network access for normal
tests; provider behavior has local test coverage.

## Hooks

No repo-local Codex hook configuration is checked in. The harness will not
automatically format, run Cargo, sync Beads, commit, push, or release for you.
Run the gate yourself and include `.beads/issues.jsonl` after `br sync
--flush-only`.

## Environment

Expected working directory: repository root. The crate targets Rust edition
2024 and uses Cargo only. CI uses stable Rust and runs fmt, clippy, and tests
on `main`; release packaging and Homebrew tap updates are in
[.github/workflows/release.yml](./.github/workflows/release.yml).

## Session Caveats

Keep `operator.json`, `canon --describe`, README examples, and
[docs/PLAN_CANON.md](./docs/PLAN_CANON.md) aligned when CLI behavior changes.
Registry maintenance commands author flat exact-match entries; do not document
or implement fuzzy lookup in the core path.

Last reviewed: 2026-05-27.
