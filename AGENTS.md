# AGENTS.md — canon

> Guidelines for AI coding agents working in this Rust codebase.

---

## canon — What This Project Does

`canon` resolves messy identifiers to canonical IDs using versioned registries. It is the **canonicalization tool** in the spine pipeline:

```
canon → rvl    (canonicalize IDs, then compare)
canon → shape  (canonicalize IDs, then check structure)
```

### Quick Reference

```bash
# JSON mapping output (default)
canon tape.csv --registry registries/cusip-isin/ --column cusip

# CSV pipeline stage
canon tape.csv --registry registries/cusip-isin/ --column cusip --emit csv > tape.canon.csv

# Full audit pipeline
canon nov.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/nov.map.json > nov.canon.csv

# Quality gate
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

### Source of Truth

- **Spec:** [`docs/PLAN_CANON.md`](./docs/PLAN_CANON.md) — all behavior must follow this document
- Do not invent behavior not present in the plan

### Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry + exit code mapping |
| `src/lib.rs` | Orchestration flow |
| `src/cli/` | Argument parsing (clap derive) |
| `src/input/` | CSV and JSONL parsing, column extraction |
| `src/registry/` | Registry loading, validation, SQLite index |
| `src/resolve/` | Lookup logic, dedup, match precedence |
| `src/output/` | JSON and CSV output formatting |
| `src/refusal/` | Refusal envelope, codes, details |
| `src/witness/` | Witness append behavior (BLAKE3 hashing) |
| `operator.json` | Machine-readable operator contract |
| `docs/PLAN_CANON.md` | Full specification |

---

## Output Contract (Critical)

`canon` has two output modes, both structured:

- **`--emit json` (default):** single JSON object to stdout — the mapping artifact
- **`--emit csv`:** canonicalized CSV to stdout, optional JSON sidecar via `--map-out`

Domain outcomes are represented by exit code and JSON envelope:

| Exit | Meaning |
|------|---------|
| `0` | RESOLVED — all inputs mapped to canonical IDs |
| `1` | PARTIAL or UNRESOLVED — some or all inputs unresolved |
| `2` | REFUSAL — cannot operate |

On refusal, emit the refusal JSON envelope with `code`/`message`/`detail`/`next_command`. In CSV mode, refusals go to stderr (no CSV on stdout). Refusals are operator handoffs, not dead ends.

---

## Core Invariants (Do Not Break)

### 1. Deterministic resolution

Same input + same registry version = byte-identical output. No randomness, no heuristics, no timestamp-dependent behavior in the mapping. `registry.source` reflects the CLI argument verbatim (path-dependent), but all resolution logic is path-independent.

### 2. Exact match only (v0)

Matching is exact byte match after ASCII-trim. No case normalization, no punctuation stripping, no stemming, no fuzzy logic. The registry is the complete source of truth. If matching behavior changes, it is a **breaking change**.

### 3. Summary invariant

`summary.total == summary.resolved + summary.unresolved`. Every unique input value is classified as exactly one of resolved or unresolved. No third bucket. This is a hard invariant — if it breaks, the tool is broken.

### 4. Dedup semantics

`mappings[]` and `unresolved[]` contain one entry per **unique** input value, not one per row. `summary.total` counts unique values. Special reasons (`empty_value`, `null_value`, `missing_field`, `non_scalar_value`) produce at most one unresolved entry each with `input: null`.

### 5. Registry version tracking

Every output includes `registry.id` and `registry.version` from `registry.json`. Resolution without a versioned registry is not permitted. If `registry.json` is missing or malformed, refuse with `E_BAD_REGISTRY`.

### 6. Match precedence

Mapping files are evaluated in filename-sorted (lexicographic) order. First match wins. Within a file, entry order determines precedence. This ordering must be deterministic and documented.

### 7. CSV mode preserves rows

`--emit csv` preserves every input row exactly — same delimiter, same quoting, same field values. Unresolved rows get an empty canonical column (not dropped). Blank records are passed through unchanged.

### 8. Witness parity

Ambient witness semantics must match spine conventions (`shape`/`rvl`/`lock` parity):
- Append by default
- `--no-witness` opt-out
- Witness failures do not mutate domain outcome semantics

---

## Toolchain

- **Language:** Rust, Cargo only
- **Edition:** 2024 (or `rust-toolchain.toml` when present)
- **Unsafe code:** forbidden (`#![forbid(unsafe_code)]`)
- **Dependencies:** explicit versions, small and pinned

Core crates: `clap` (derive), `csv`, `serde` + `serde_json`, `rusqlite` (bundled), `blake3`

Release profile:

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

---

## Quality Gate

Run after any substantive change:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Test Coverage Areas

- Exact match resolves with registry version recorded
- Missing column → REFUSAL `E_COLUMN_NOT_FOUND`
- Malformed registry → REFUSAL `E_BAD_REGISTRY`
- Empty registry (valid format, zero entries) → UNRESOLVED, exit 1
- All resolved → RESOLVED, exit 0
- Some unresolved → PARTIAL, exit 1
- All unresolved → UNRESOLVED (not PARTIAL), exit 1
- Blank records skipped, counts correct
- JSONL: missing field → `"missing_field"`, null → `"null_value"`, object/array → `"non_scalar_value"`
- CSV: empty column on non-blank row → `"empty_value"`
- Unrecognized file extension → REFUSAL `E_PARSE`
- Determinism: same input + same registry = byte-identical output
- Large input (100k+ rows) without OOM
- `--max-rows` / `--max-bytes` enforcement
- Identifier encoding: non-UTF-8 → `hex:` rendering
- Alias resolution: variant names → same canonical ID
- `--emit csv`: columns appended, delimiter preserved, unresolved rows have empty column
- `--emit csv` + JSONL → REFUSAL `E_EMIT_FORMAT`
- `--emit csv` + column exists → REFUSAL `E_COLUMN_EXISTS`
- `--emit csv` + `--map-out` consistency with `--emit json`

---

## Git and Release

- **Primary branch:** `main`
- **`master`** exists for legacy URL compatibility — keep synced: `git push origin main:master`
- Bump `Cargo.toml` semver appropriately on release
- Sync `Cargo.lock` before release workflows that use `--locked`

---

## Editing Rules

- **No file deletion** without explicit written user permission
- **No destructive git commands** (`reset --hard`, `clean -fd`, `rm -rf`, force push) without explicit authorization
- **No scripted mass edits** — make intentional, reviewable changes
- **No file proliferation** — edit existing files; new files for genuinely new functionality only
- **No surprise behavior** — do not invent behavior not in `docs/PLAN_CANON.md`
- **No backwards-compatibility shims** — fix the code directly

---

## RULE 0

If the user gives a direct instruction, follow it even if it conflicts with defaults in this file.

---

## Beads (`br`) Workflow

Use Beads as source of truth for task state.

```bash
br ready              # Show unblocked ready work
br list --status=open # All open issues
br show <id>          # Full issue details
br update <id> --status=in_progress
br close <id> --reason "Completed"
br sync --flush-only  # Export to JSONL (no git ops)
```

Pick unblocked beads. Mark in-progress before coding. Close with evidence when done.

---

## Agent Mail (Multi-Agent Sessions)

When Agent Mail is available:

- Register identity in this project
- Reserve only specific files you are actively editing — never entire directories
- Send start/finish updates per bead
- Poll inbox at moderate cadence (2-5 minutes)
- Acknowledge `ack_required` messages promptly
- Release reservations when done

---

## Session Completion

Before ending a session:

1. Run quality gate (`fmt` + `clippy` + `test`)
2. Confirm docs/spec alignment for behavior changes
3. Commit with precise message
4. Push `main` and sync `master`
5. Summarize: what changed, what was validated, remaining risks
