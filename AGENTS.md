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
- **Geo system:** [`docs/CANON_GEO_AGENT_ARCHITECTURE.md`](./docs/CANON_GEO_AGENT_ARCHITECTURE.md) — agent operating model; [`docs/PLAN_CANON_GEO.md`](./docs/PLAN_CANON_GEO.md) remains authoritative for Geo mathematics, measurements, and E1–E5 gates
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
| `docs/CANON_GEO_AGENT_ARCHITECTURE.md` | Geo abstraction tower, control loop, resumability, and agent API target |
| `docs/PLAN_CANON_GEO.md` | Geo mathematics, empirical state, and E1–E5 gates |

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

### Canon Geo agent mental model

Read [`docs/CANON_GEO_AGENT_ARCHITECTURE.md`](./docs/CANON_GEO_AGENT_ARCHITECTURE.md)
before changing Geo orchestration or contracts, and use
[`docs/PLAN_CANON_GEO.md`](./docs/PLAN_CANON_GEO.md) for the governing mathematics,
measurements, and E1–E5 gates.

Operate Geo as one bounded, source-generic system:

```text
question + capabilities + regional inventory + resolution profile + budget
  -> deterministic plan
  -> tile + controlled halo
  -> candidate-universe/reach state
  -> rho admission
  -> incidence components
  -> small exact residuals
  -> ownership reconciliation
  -> separate coverage/reach/solver/truth/cost evaluation
  -> explanation and next evidence
  -> review-gated registry proposal -> exact replay
```

- Core dispatches typed evidence classes, entity levels, and relations—not vendor names.
  Source-specific semantics stop at versioned adapters/profiles.
- A region may have no parcel layer. Current composition v0 is honestly parcel/building
  specific; do not present that implementation limit as the generic architecture.
- H3 is blocking and ownership metadata, never geometric truth. Never describe national
  evidence volume as one solve; exact work is local and component-wise.
- More admitted hard evidence narrows the feasible set or makes it empty. Source count is
  provenance, not independent information or confidence.
- Report availability, candidate reach, rho soundness, solver exactness, reconciliation,
  truth quality, and cost as different planes.
- Distinguish structural candidate completeness relative to declared inputs from empirical
  truth reach. Unverified reach can coexist with an exact representation-relative solve;
  failed reach blocks the affected claim and exactness does not repair it.
- Prefer resumable, content-addressed artifacts and recompute only affected sections and
  components. Network acquisition remains outside Canon's deterministic offline build.
- Geo planning/runs must extend the shared `src/project/` manifest/lock/plan/run/receipt
  substrate. Do not create a second scheduler, cache, receipt store, or workspace policy.
- Geo capabilities, planning, and the bounded offline run surface exist. `geo run` delegates
  one validated project DAG through registered internal Geo executors for the current
  five-stage offline chain: materialize home cells, build the bounded tile section,
  materialize evidence, compile evidence, then solve composition. It accepts only local
  exogenous leaf inputs such as home-cell rows, tile-work requests, and warehouse rows; no
  ambient shell or network acquisition is part of the run.
- Open limits remain explicit: acquisition is external, exactness is representation-relative,
  candidate reach is an upstream proof obligation, immutable cross-release reuse in the same
  work directory is not guaranteed, E5/live scale proof is not shipped, and `geo inspect`
  remains open.

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

## Tangible Progress, Anti-Ceremony, and Honest Credit

The purpose of this project is working, deployable software delivered
accretively in the shortest time compatible with correctness, performance,
reliability, and innovation. Process exists to serve that outcome; it must
never become the product.

- **No process porn.** Certificates, ledgers, dashboards, meta-reports, and
  process documents are not progress. A process artifact may exist only when it
  is a hard gate for a named feature or capability. Conformance checks, release
  preflight, and required release evidence qualify; self-referential paperwork
  does not. Choosing easy, low-risk process artifacts is reward hacking.
- **Feature-first ratio.** The overwhelming majority of open work must deliver
  runnable behavior — code, schemas, artifacts, and contracts an operator or
  consuming agent can exercise. Process/operations items are capped at a
  guideline of at most about 5% of open beads, and each must name the feature
  work it gates. A process item that gates nothing does not get created.
- **Honesty is absolute.** Never fake a test, present a fixture or mock as live
  proof, weaken an assertion to make it pass, hard-code a success path, or close
  work that is not done. Reopen a false close and add an incident note to the
  bead record.
- **Refusal is not delivery.** A correctly typed refusal is far better than a
  fabricated result and far less valuable than the real capability. Canon's
  refusal taxonomy is a feature, but refusal-only implementation earns partial
  credit at most and never closes a feature bead. Full credit requires the
  positive capability implemented, tested, and verified. Mark a refusal-only
  state explicitly with a `refusal-only` label and a follow-up bead so it reads
  as unfinished, never as shipped.

Note the related-but-distinct rule for resolution results: **abstention is a
legitimate output.** "Zero matches is a finding, not a failure" applies to what
the software *reports*; it never licenses shipping a module that only ever
abstains.

These rules bind human-directed sessions and NTM swarms alike. Encode them in
the acceptance criteria of the work items themselves.

---

## Named Reward-Hacking Patterns (All Forbidden)

Beyond refusal farming and process porn, this architecture specifically invites
the following failure modes. Name and reject them during planning, review, and
verification. The numbering matches the shared CMD+RVL list so the pattern names
carry across repos.

1. **Gate self-weakening** — editing validator, conformance, lint, or audit code
   so a failing check passes. In a swarm, conformance code is a separate
   single-owner lane with reviewer sign-off; the orchestrator reviews its diff
   every wave.
2. **Proof-class inflation** — presenting fixtures, retained evidence,
   deterministic samples, mocked providers, or hand-inserted registry rows as
   live proof. Extrapolated or simulated scale numbers are the canon-native form
   of this. Real proof requires runtime-selected subjects with a recorded
   selection seed, receipts chained to real input manifests and source hashes,
   and fresh-process readback.
3. **Golden regeneration reflex** — regenerating goldens to match broken output
   instead of fixing the output. Golden changes require an explicit
   `GOLDEN-CHANGE` commit note and semantic diff review.
4. **Commit-stream pumping** — trivial or artificially split commits, or new
   `todo!()`/`unimplemented!()` scaffolds committed because they pass
   `cargo check`. Every commit names its bead and touched scope. Note the
   deliberate exception: the pre-existing module stubs described under
   *Multi-Agent Coordination* legitimately contain `todo!()` and are there to be
   replaced. Adding *new* placeholder macros to claim progress is the violation;
   replacing an existing stub is the work.
5. **Tautological tests** — tests that assert the code does whatever the code
   does, or omit negative cases. Every feature bead pre-specifies its key
   behavioral assertions, including at least one negative case that a naive
   wrong implementation would fail.
6. **Easy-bead cherry-picking** — repeatedly claiming low-risk beads while
   articulation-point work starves. Claim the highest-priority ready bead and
   act on staleness alerts for unclaimed P0/P1 work.
7. **Close-pump abuse** — closing beads, yours or a peer's, to flood the ready
   pool because closure unblocks dependents. Only the orchestrator closes in a
   swarm; violations are reopened with an incident note.
8. **Scope-splitting** — splitting one unit of work into type, implementation,
   and test mini-closures to harvest multiple credits. Code and its tests ship
   in the same bead; test-only follow-ups exist only for cross-cutting
   integration suites.
9. **Spec-editing as progress** — weakening a plan, specification, or frozen
   decision instead of implementing it. `docs/PLAN_CANON.md`,
   `docs/IDENTITY_ARCHITECTURE.md`, `docs/PROVIDER_SDK.md`, and the `DECISION_*`
   documents are the frozen surface. Plan edits are a chore lane, never close
   feature beads, and frozen decisions change only through the joint decision
   protocol.
10. **Conformance metastasis** — adding speculative checks, matrices, or reports
    because they are safe and satisfying. Every new check must cite an observed
    defect class or a named release gate.
11. **Dependency smuggling** — vendoring, wrapping, or shimming around a banned
    dependency or a stated boundary to "make progress." Two canon-specific
    forms: reaching the network from inside a provider build, which
    `PROVIDER_SDK.md` calls a conformance failure by definition; and pulling
    domain machinery into core that `IDENTITY_ARCHITECTURE.md` places in
    registries, profiles, strategies, or out-of-tree extensions.
12. **Demo-path hard-coding** — special-casing particular entities, identifiers,
    tickers, CIKs, filenames, hashes, registry snapshots, or pilot corpora so
    the happy path passes. Conformance subjects are runtime-selected and differ
    from development fixtures.

**Determinism is the promise these protect.** Same input plus same registry
snapshot must reproduce byte-identical output across platforms and runs,
forever. Anything that makes a result reproducible only on the machine that
produced it — float accumulation where integers are specified, unordered
iteration, ambient clock or locale, naive floating-point geometric predicates —
is a determinism defect, not a rounding detail. And per canon's final rule: if
you cannot explain a match by pointing at assertion scores, it does not ship.

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
