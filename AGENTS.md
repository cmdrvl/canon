# AGENTS.md — canon

> Guidelines for AI coding agents working in this Rust codebase.

---

## canon — What This Project Does

`canon` compiles accepted identity knowledge into versioned registries and then
replays that knowledge through exact lookup. It is the **canonicalization tool**
in the spine pipeline:

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

# Self-authored registry maintenance
canon registry default-id-scheme --registry registries/people/ --prefix PPL --zero-pad 3
canon registry next-id --registry registries/people/
canon registry mint --registry registries/people/ \
  --canonical-type person --with-alias 'aliases.json=Jane Doe:MANUAL'
canon registry add-entry --registry registries/people/ \
  --alias-file aliases.json --canonical-id PPL-001 --input 'J. Doe' --rule-id MANUAL

# Quality gate
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

### Product Boundary

Use this mental model in code, tests, docs, and bead comments:

```text
messy evidence -> deterministic artifacts -> audit/review -> versioned registry -> exact replay
```

The default `canon <INPUT> --registry ... --column ...` path is exact runtime
lookup only. Uncertain evidence belongs in workbench artifacts such as
`canon entity`, `canon resolve`, strategy packages, project workflows, temporal
comparisons, or out-of-tree extensions. Do not imply that Canon core ships
industry ontology, provider knowledge, or probabilistic runtime lookup.

### Source of Truth

- **Spec:** [`docs/PLAN_CANON.md`](./docs/PLAN_CANON.md) — all behavior must follow this document
- **Boundary:** [`docs/IDENTITY_ARCHITECTURE.md`](./docs/IDENTITY_ARCHITECTURE.md) — exact runtime vs build-time evidence, entity cluster/link modes, and extension firewall
- **Harness notes:** [`CODEX.md`](./CODEX.md), [`CLAUDE.md`](./CLAUDE.md), and [`GEMINI.md`](./GEMINI.md) — runner-specific caveats only
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
| `src/entity/` | Build-time entity workbench artifacts, cluster/link flows, review, and promotion |
| `src/project/` | Project manifests, locks, and reproducible workflow planning |
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

Workbench evidence does not loosen this invariant. `canon entity`, `canon
resolve`, provider materialization, strategy selection, project workflows,
packages, temporal snapshots, and extensions may create or validate registry
knowledge before promotion. Once promoted, normal lookup still resolves only
through exact registry entries.

### 3. Summary invariant

`summary.total == summary.resolved + summary.unresolved`. Every unique input value is classified as exactly one of resolved or unresolved. No third bucket. This is a hard invariant — if it breaks, the tool is broken.

### 4. Dedup semantics

`mappings[]` and `unresolved[]` contain one entry per **unique** input value, not one per row. `summary.total` counts unique values. Special reasons (`empty_value`, `null_value`, `missing_field`, `non_scalar_value`) produce at most one unresolved entry each with `input: null`.

### 5. Registry version tracking

Every output includes `registry.id` and `registry.version` from `registry.json`. Resolution without a versioned registry is not permitted. If `registry.json` is missing or malformed, refuse with `E_BAD_REGISTRY`.

For self-authored registry updates, prefer `canon registry mint` or `canon registry add-entry` over hand-editing mapping JSON. The commands preserve the exact-match registry model while keeping version bumps, `entry_count`, and lint behavior aligned with the implementation.

### 6. Match precedence

Mapping files are evaluated in filename-sorted (lexicographic) order. First match wins. Within a file, entry order determines precedence. This ordering must be deterministic and documented.

### 7. CSV mode preserves rows

`--emit csv` preserves every input row exactly — same delimiter, same quoting, same field values. Unresolved rows get an empty canonical column (not dropped). Blank records are passed through unchanged.

### 8. Witness parity

Ambient witness semantics must match spine conventions (`shape`/`rvl`/`lock` parity):
- Append by default
- `--no-witness` opt-out
- Resolve implicit witness state to `~/.cmdrvl/state/witness/witness.jsonl`
- Copy any legacy `~/.epistemic/witness.jsonl` or `.epistemic/witness.jsonl` ledger to the canonical path on first default use; never delete the legacy ledger
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

## RULE 1: No File Deletion

**You are never allowed to delete a file without express permission.** Always ask and receive clear, written permission before deleting any file or folder.

---

## Beads (`br`) Workflow

Use Beads as source of truth for task state. Issues are stored in `.beads/` and tracked in git.

**Important:** `br` is non-invasive — it NEVER executes git commands. After `br sync --flush-only`, you must manually run `git add .beads/ && git commit`.

```bash
br ready              # Show unblocked ready work
br list --status=open # All open issues
br show <id>          # Full issue details
br update <id> --status=in_progress
br close <id> --reason "Completed"
br sync --flush-only  # Export to JSONL (no git ops)
```

Pick unblocked beads. Mark in-progress before coding. Close with evidence when done.

### Phase Labels

Beads are labeled `phase-0` through `phase-2` indicating when they can start:

| Phase | When | What |
|-------|------|------|
| `phase-0` | Immediately | Scaffold, fixtures, CI, release, docs |
| `phase-1` | After Phase 0 completes | All feature modules + orchestration (parallel) |
| `phase-2` | After Phase 1 completes | Integration test suites + witness protocol |

Use `br list --label phase-N` to see beads in each phase.

---

## Agent Mail (Multi-Agent Sessions)

When Agent Mail is available:

- Register identity in this project
- **Reserve only the specific file(s) you are editing — never entire directories or broad globs**
- Each bead's comments document the exact files to reserve (look for `RESERVATIONS:`)
- Send start/finish updates per bead using bead ID as `thread_id`
- Poll inbox at moderate cadence (2-5 minutes)
- Acknowledge `ack_required` messages promptly
- Release reservations when done

### File Reservation Rules

The scaffold (bd-1do) pre-creates all module stubs and shared types. This means:

1. **No bead except scaffold touches `lib.rs` shared types or `main.rs`** — your types and dispatch are already in place
2. **You only edit your own `.rs` file(s)** — the stubs have `todo!()` that you replace with real implementation
3. **Reserve only the files you are writing** — not the module directory, not other stubs
4. **src/output/ is split into json.rs and csv.rs** — bd-2p4 and bd-37k work in parallel without conflict

Example: if working on `registry` (bd-23d), reserve only `src/registry.rs`.

---

## Multi-Agent Coordination

When working alongside other agents:

- **Never stash, revert, or overwrite other agents' work**
- Treat unexpected changes in the working tree as if you made them
- If you see changes you didn't make in `git status`, those are from other agents working concurrently — commit them together with your changes
- This is normal and happens frequently in multi-agent environments

**Do NOT** stop working to ask about unexpected changes. **Do** continue working as normal and include those changes when you commit.

---

## Session Completion

Before ending a session:

1. Run quality gate (`fmt` + `clippy` + `test`)
2. Confirm docs/spec alignment for behavior changes
3. Update bead status (`br close <id>` or update progress)
4. Sync beads: `br sync --flush-only`
5. Commit with precise message:
   ```bash
   git add .beads/ <other files>
   git commit -m "..."
   git push
   ```
6. Verify: `git status` shows "up to date with origin"
7. Summarize: what changed, what was validated, remaining risks

**Work is NOT complete until `git push` succeeds.** Never stop before pushing.
