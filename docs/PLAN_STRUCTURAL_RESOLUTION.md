# canon resolve — Cross-Tape Entity Resolution

> **Status**: Draft
> **Created**: 2026-03-17
> **Revised**: 2026-05-06
> **Context**: Different data sources describe the same entities with different
> identifiers. Canon needs a way to discover that tape A's `loan_id=223232` is
> the same loan as tape B's `deal=a, loan=2` — using structural similarity, not
> pre-built ID mappings.

> **Architecture status**: Scaffolded future resolution workbench. This plan
> describes a sibling to `canon org`, not current core lookup behavior. The CLI
> parse surface and shared contracts for `canon resolve` are scaffolded, but the
> matching pipeline is still implementation-pending; production lookup remains
> exact registry lookup as described in `PLAN_CANON.md` and
> `IDENTITY_ARCHITECTURE.md`.

---

## One-line thesis

Give canon two tapes from the same deal. It hydrates a graph, matches records
across tapes by structural similarity, emits a cross-reference registry, and
forgets the structural attributes that got it there.

---

## Why this exists

Canon v0 resolves entities by exact lookup against versioned registries. This
works when someone has already built the cross-reference table. But the
cross-reference table is exactly what doesn't exist yet.

In CMBS, the same loan pool appears in:

- trustee remittance reports (loan_id = 223232)
- servicer reports (deal = WFCM2019-C50, loan = 2)
- Bloomberg feeds (CUSIP = 12345ABC6)
- rating agency supplements (deal-level, different loan numbering)

These sources don't share a common key. A human analyst knows they describe the
same loans because the structural attributes line up: same property address, same
UPB (within rounding), same rate, same servicer, same maturity. The analyst builds
a cross-reference spreadsheet by hand.

Canon should do what the analyst does:

1. Load both tapes
2. Compare records by structural attributes
3. Find matches
4. Emit the cross-reference as a registry
5. Throw away the structural attributes — the registry is the asset

Once the cross-reference registry exists, every future tape resolves via fast
exact lookup. The structural matching only runs once per identifier pair.

---

## Design principles

1. **Canon stays a tool, not a loop.** Canon takes two tapes + a strategy, emits
   a matching report. It does not iterate, optimize, or learn. The tournament
   drives canon from the outside.

2. **The tapes are the graph data.** No persistent entity store, no growing
   database. Both tapes are loaded into an ephemeral petgraph, used for matching,
   and torn down. Nothing persists except the ID cross-references.

3. **No external infrastructure.** The graph is in-process (petgraph), hydrated
   on demand, torn down when done. Same pattern as benchmark embedding DuckDB.

4. **Strategies are promotable artifacts.** A matching strategy is a versioned
   YAML file that the tournament discovers and promotes. It defines which columns
   to compare across tapes and how.

5. **The registry is the only output.** Structural attributes are ephemeral.
   The cross-reference mapping is what persists. Once two IDs are linked, the
   structural fields that established the link can be forgotten.

6. **Deterministic given the same inputs.** Same tapes + same strategy = same
   matching, every time.

---

## Non-goals

This plan is NOT:

- A fuzzy matching engine (no Jaro-Winkler, no edit distance, no ML embeddings)
- A probabilistic record linker (Fellegi-Sunter is still deferred)
- A persistent entity database (no growing attribute store)
- An address parser or geocoder (libpostal integration is still deferred)
- A general-purpose graph database (petgraph is ephemeral and task-scoped)
- A replacement for data-fabric (the event store and lineage graph remain separate)

---

## How it works

### Input: two tapes

Each tape is a CSV or JSONL file. One tape is the **reference** (its IDs are
treated as canonical or at least authoritative). The other is the **target**
(its records need to be matched against the reference).

```bash
canon resolve trustee_tape.csv servicer_tape.csv \
  --strategy cmbs-loan-match.v1.yaml \
  --registry registries/cmbs-loan/ \
  [--gold gold/loan_matches.jsonl] \
  [--write-back] \
  [--emit json|summary]
```

Position matters: first argument is the reference tape, second is the target tape.
The strategy declares which columns exist in each tape and how to align them.

### The matching pipeline

```
Step 1: Load both tapes into memory
  → parse reference tape: extract ID columns + structural columns
  → parse target tape: extract ID columns + structural columns
  → each record becomes a petgraph node with its attributes

Step 2: Candidate selection (narrowing)
  → for each target record, find reference records that share an
    anchor value (e.g., same servicer, or UPB within 20%)
  → this avoids O(n×m) full cross-product when tapes are large

Step 3: Score candidates
  → for each (target record, candidate reference record) pair,
    evaluate the strategy's assertions
  → each assertion compares one field from the target against
    one field from the reference using an operator (exact, tolerance, etc.)
  → weighted score computed

Step 4: Match decision
  → best candidate above match_threshold with sufficient ambiguity_gap
    → MATCHED: target ID ↔ reference ID
  → no candidate above threshold
    → UNMATCHED: target record reported with near-misses
  → ambiguous (top two candidates too close)
    → AMBIGUOUS: reported with both candidates for human review

Step 5: Emit cross-reference
  → matched pairs become registry entries mapping both IDs to
    a shared canonical ID
  → structural attributes are NOT included in the output
  → evidence (which assertions matched, scores) IS included
```

### What persists vs what's discarded

| Data | Persists? | Where |
|------|-----------|-------|
| Reference tape IDs → canonical ID | Yes | Registry mapping file |
| Target tape IDs → canonical ID | Yes | Registry mapping file |
| Structural attributes (UPB, rate, address) | **No** | Discarded after matching |
| Match evidence (scores, assertions) | Yes | Resolution report (JSON) |
| Strategy used | Yes | Content hash in report |
| The graph | **No** | Torn down after matching |

---

## Resolution strategies

A matching strategy is a YAML file that tells canon how to align and compare
columns across two tapes. Strategies are the mutable artifact that the tournament
iterates over.

### Strategy schema (v0)

```yaml
strategy_id: cmbs-loan-match.v1
strategy_version: "0.1.0"
entity_type: loan
description: "Match CMBS loans across trustee and servicer tapes"

# Which columns identify a record in each tape
identity:
  reference:
    id_columns: [loan_id]                # composite key if multiple columns
  target:
    id_columns: [deal, loan_number]      # composite key: "WFCM2019-C50|2"

# Narrow the search space before scoring (optional but recommended)
candidate_filter:
  - field_ref: servicer
    field_tgt: servicer_name
    op: canon_match                       # resolve both through canon registry first
  - field_ref: upb
    field_tgt: balance
    op: range
    range_pct: 0.20                       # only compare if within 20%

# How to score each candidate pair
assertions:
  - field_ref: property_address
    field_tgt: address
    op: exact
    weight: 0.30
    required: true
  - field_ref: upb
    field_tgt: balance
    op: tolerance_pct
    tolerance: 0.05
    weight: 0.25
    required: false
  - field_ref: rate
    field_tgt: coupon
    op: tolerance_abs
    tolerance: 0.001
    weight: 0.15
    required: false
  - field_ref: servicer
    field_tgt: servicer_name
    op: canon_match
    weight: 0.15
    required: false
  - field_ref: maturity_date
    field_tgt: maturity
    op: date_range
    days: 5
    weight: 0.10
    required: false
  - field_ref: origination_date
    field_tgt: orig_date
    op: exact
    weight: 0.05
    required: false

# Decision thresholds
match_threshold: 0.75
ambiguity_gap: 0.15          # minimum score gap between #1 and #2 candidate
max_candidates: 10           # refuse if more than N candidates survive filter
```

Key difference from the previous plan: the strategy now maps **column names
across tapes**. `field_ref` is a column in the reference tape. `field_tgt` is
the corresponding column in the target tape. The strategy is the Rosetta Stone
between two file formats.

### Assertion operators (v0)

| Operator | Behavior | Applicable types |
|----------|----------|------------------|
| `exact` | Byte-equal after ASCII-trim | All |
| `canon_match` | Resolve both sides through canon registry, then compare canonical IDs | Names (servicer, trustee, borrower) |
| `tolerance_pct` | `abs(a - b) / max(abs(a), abs(b)) <= tolerance` | Numeric (UPB, balance) |
| `tolerance_abs` | `abs(a - b) <= tolerance` | Numeric (rate, small values) |
| `range` | Value falls within `candidate ± range_pct%` | Numeric |
| `date_range` | Dates within N days of each other | Date fields |
| `prefix` | One value is a prefix of the other | String (partial IDs) |

Each operator is a pure function: two values in, `(bool, f64)` out.

### Strategy lifecycle

```
Draft → Tested → Promoted → Versioned artifact

Draft:      Author proposes a strategy (manually or via tournament mutation)
Tested:     Strategy is evaluated against gold cross-reference set
Promoted:   Strategy passes gold validation, enters production use
Versioned:  Strategy is immutable, content hash recorded in every match report
```

---

## The embedded graph

### Why petgraph

Same rationale as before: in-process, no infrastructure, microsecond hydration,
deterministic iteration, same binary.

But in the two-tape model, the graph is even simpler:

| Aspect | Two-tape model |
|--------|----------------|
| Node count | Records from tape A + records from tape B |
| Edge count | Zero at hydration time (edges are discovered, not loaded) |
| Lifetime | One `canon resolve` call |
| Peak size | Both tapes in memory (one deal = typically 50-500 records per tape) |

### Graph schema

```
Node types:
  RefRecord(id: CompositeKey, source: "reference", attrs: BTreeMap<String, Value>)
  TgtRecord(id: CompositeKey, source: "target", attrs: BTreeMap<String, Value>)

Edge types (discovered during matching):
  MATCHED(score: f64, assertions: Vec<AssertionResult>)
```

Nodes are inserted in sorted order (by composite ID key) for deterministic
iteration.

### When the graph earns its keep (v1)

For v0, the graph is essentially two flat lists with pairwise scoring. Petgraph
is useful but not strictly necessary.

For v1, graph-structural signals become valuable:

- "Tape A has 3 loans collateralized by the same property. Tape B also has 3
  loans on that address. The structural neighborhood shape matches." This is a
  signal that pairwise comparison can't see.
- "If we already matched loan X across tapes, and loan Y shares the same deal
  in both tapes, loan Y's match confidence increases." Propagation.
- Bipartite matching constraints: "each reference record should match at most one
  target record." Global optimization, not greedy per-record.

These are graph algorithms (connected components, constraint propagation,
Hungarian algorithm for bipartite matching). Petgraph supports all of them.
But they're v1 — v0 ships with greedy pairwise matching.

### Memory budget

For a CMBS deal:
- Reference tape: 50-500 records × ~20 fields = 1-10K attribute values
- Target tape: same scale
- Total graph: 100-1000 nodes, < 1MB
- Even for a 10,000-loan deal: < 10MB, trivial

---

## CLI

### `canon resolve`

```bash
canon resolve <REFERENCE_TAPE> <TARGET_TAPE> \
  --strategy <STRATEGY.yaml> \
  --registry <REGISTRY_DIR> \
  [--gold <GOLD_SET.jsonl>] \
  [--write-back] \
  [--emit json|summary] \
  [--max-candidates <N>] \
  [--no-witness]
```

Arguments:
- `<REFERENCE_TAPE>`: CSV or JSONL file (the authoritative side)
- `<TARGET_TAPE>`: CSV or JSONL file (the side to be matched)
- `--strategy`: Matching strategy YAML
- `--registry`: Registry directory (used for `canon_match` operator lookups
  and write-back destination)
- `--gold`: Known-correct cross-references for scoring (optional)
- `--write-back`: Write matched ID pairs to the registry
- `--emit`: Output mode
- `--no-witness`: Suppress witness ledger

### Exit codes

- `0`: All target records matched
- `1`: Partial (some unmatched or ambiguous)
- `2`: Refusal (bad input, bad strategy, incompatible tapes)

---

## Output contract (`canon_resolve.v0`)

```json
{
  "version": "canon_resolve.v0",
  "strategy": {
    "id": "cmbs-loan-match.v1",
    "version": "0.1.0",
    "content_hash": "blake3:9f2a..."
  },
  "registry": {
    "id": "cmbs-loan",
    "version": "2.1.0"
  },
  "reference_tape": {
    "path": "trustee_tape.csv",
    "record_count": 127
  },
  "target_tape": {
    "path": "servicer_tape.csv",
    "record_count": 131
  },
  "summary": {
    "target_records": 131,
    "matched": 119,
    "unmatched": 8,
    "ambiguous": 4,
    "match_rate": 0.908
  },
  "matches": [
    {
      "reference_id": "223232",
      "target_id": "WFCM2019-C50|2",
      "canonical_id": "223232",
      "score": 0.92,
      "assertions": [
        {"field_ref": "property_address", "field_tgt": "address", "op": "exact", "passed": true, "weight": 0.30},
        {"field_ref": "upb", "field_tgt": "balance", "op": "tolerance_pct", "passed": true, "weight": 0.25, "detail": {"ref_val": 2440000, "tgt_val": 2450000, "diff_pct": 0.004}},
        {"field_ref": "rate", "field_tgt": "coupon", "op": "tolerance_abs", "passed": true, "weight": 0.15},
        {"field_ref": "servicer", "field_tgt": "servicer_name", "op": "canon_match", "passed": true, "weight": 0.15},
        {"field_ref": "maturity_date", "field_tgt": "maturity", "op": "date_range", "passed": true, "weight": 0.10},
        {"field_ref": "origination_date", "field_tgt": "orig_date", "op": "exact", "passed": false, "weight": 0.05}
      ],
      "runner_up": {
        "reference_id": "223233",
        "score": 0.45,
        "gap": 0.47
      }
    }
  ],
  "unmatched": [
    {
      "target_id": "WFCM2019-C50|135",
      "reason": "no_candidates_above_threshold",
      "best_candidate": {"reference_id": "223500", "score": 0.42}
    }
  ],
  "ambiguous": [
    {
      "target_id": "WFCM2019-C50|87",
      "candidates": [
        {"reference_id": "223401", "score": 0.81},
        {"reference_id": "223402", "score": 0.78}
      ],
      "gap": 0.03,
      "reason": "ambiguity_gap_insufficient"
    }
  ],
  "gold_score": {
    "total": 95,
    "correct": 93,
    "incorrect": 1,
    "unmatched_in_gold": 1,
    "accuracy": 0.979,
    "regressions": ["WFCM2019-C50|44"]
  }
}
```

### Canonical ID assignment

For v0, the reference tape's ID IS the canonical ID. Simple rule:

- `canonical_id = reference_id`
- Target ID maps to the reference ID
- If the registry already has a canonical ID for the reference ID, use that instead

This avoids inventing synthetic canonical IDs. The reference tape is the authority.

---

## Registry write-back

### What gets written

When `--write-back` is specified and gold validation passes (zero regressions):

For each matched pair, write **two** registry entries:

```json
{"input": "223232", "canonical_id": "223232", "canonical_type": "loan_id", "rule_id": "IDENTITY:reference"}
{"input": "WFCM2019-C50|2", "canonical_id": "223232", "canonical_type": "loan_id", "rule_id": "STRUCTURAL_MATCH:cmbs-loan-match.v1"}
```

The first entry is the reference ID mapping to itself (idempotent if it already
exists). The second entry maps the target ID to the reference ID.

Both go into a timestamped mapping file: `resolve-matches-YYYYMMDD.json`.
The registry's `_index.sqlite` is rebuilt to include them.

### What does NOT get written

- No structural attributes (UPB, rate, address, servicer, maturity)
- No relationship data
- No entity attribute store
- No growing database

The registry stays flat: `input string → canonical ID`. Same format canon has
always used. The only thing that grows is the number of ID cross-references.

### Write-back safety

- Write-back only fires with explicit `--write-back` flag
- Gated on gold validation: any regression → no entries written
- Creates new mapping files (never modifies existing ones)
- Registry version NOT bumped (manual/CI step after review)
- Each entry carries `rule_id` for auditability

### The scale question

A million entities × 2-3 variant IDs each = 2-3 million registry entries. At
~100 bytes per entry, that's ~300MB of mapping files and a ~200MB SQLite index.
Comfortable for SQLite, comfortable for disk, no attribute bloat.

The key insight: **each entity contributes a few short strings to the registry,
not a bag of structural attributes.** The registry doesn't grow with the
complexity of the entities — only with the number of distinct identifier variants.

---

## Gold scoring

### Gold set format

A gold set is a JSONL file of known-correct cross-references:

```jsonl
{"target_id": "WFCM2019-C50|2", "expected_reference_id": "223232"}
{"target_id": "WFCM2019-C50|3", "expected_reference_id": "223233"}
{"target_id": "WFCM2019-C50|44", "expected_reference_id": "223401"}
```

### Gold metrics

- `accuracy`: fraction of matched pairs that agree with gold
- `regressions`: specific target IDs where canon matched the wrong reference ID
- `unmatched_in_gold`: gold entries where canon couldn't find a match at all

Primary tournament metric: `accuracy`. Gate: `regressions.len() == 0`.

### How gold sets are built

For v0 (BDC): a human analyst manually cross-references 50-100 loans from one
deal. This is the kind of spreadsheet that already exists in the wild — analysts
do this today. The gold set is just that spreadsheet in JSONL format.

For tournament iteration: the gold set is frozen. The strategy mutates. Canon
scores each strategy variant against the same gold set.

---

## Tournament integration

### What the tournament mutates

The matching strategy YAML is the mutable artifact:

1. **Field selection**: which columns appear in assertions
2. **Cross-tape column mapping**: which target column maps to which reference column
3. **Weights**: how much each assertion contributes
4. **Thresholds**: `match_threshold`, `ambiguity_gap`
5. **Operator parameters**: tolerance values, range percentages, date windows
6. **Candidate filter**: which fields narrow the search space and how aggressively

### What the tournament holds frozen

- The two tapes (same records for all strategy variants in one round)
- The gold cross-reference set
- The registry (for `canon_match` lookups)
- Canon's matching code

### Tournament loop

```
1. Propose strategy mutation
   (e.g., add maturity_date assertion, widen UPB tolerance from 5% to 8%,
   change address weight from 0.30 to 0.40)

2. Run: canon resolve trustee.csv servicer.csv \
          --strategy proposed.yaml \
          --registry registries/cmbs-loan/ \
          --gold gold/deal_matches.jsonl

3. Score:
   - Primary: gold_score.accuracy (higher is better)
   - Gate: gold_score.regressions must be empty
   - Secondary: summary.match_rate (higher is better)
   - Tertiary: summary.ambiguous (lower is better)

4. Decide:
   - If accuracy > champion AND regressions == 0: PROMOTE
   - If accuracy == champion AND match_rate > champion: PROMOTE
   - Else: REVERT

5. Repeat until convergence or max rounds
```

### What the tournament discovers

- "UPB tolerance can be 8% for Wells Fargo because WF rounds to thousands"
- "Adding rate as an assertion reduces ambiguity for same-property loans"
- "Property address matching is unreliable for this trustee — drop weight to 0.10,
  increase UPB weight to 0.35"
- "The optimal match_threshold for this deal family is 0.72, not 0.80"
- "Maturity date is the best disambiguator when two loans share a property"

### Validation across deals

Once a strategy works on one deal, run it on a second deal:

```bash
# Same strategy, different deal
canon resolve deal2_trustee.csv deal2_servicer.csv \
  --strategy cmbs-loan-match.v1.yaml \
  --registry registries/cmbs-loan/ \
  --gold gold/deal2_matches.jsonl
```

If the match rate and accuracy hold, the strategy generalizes. If they drop, the
tournament can discover a deal-family-specific variant. Different deals may need
different strategies — that's what family routing is for.

---

## Relationship to data-fabric

| Concern | Canon (petgraph) | Data-fabric (Neo4j + MSSQL) |
|---------|-------------------|------------------------------|
| Purpose | Fast, ephemeral cross-tape matching | Durable event store + lineage graph |
| Lifetime | One `canon resolve` call | Permanent |
| Scope | Two tapes from one deal | Entire corpus |
| Persistence | None (registry entries only) | Full audit trail |
| Who calls it | Tournament runner → canon | Everything (cold path) |

Canon's confirmed matches flow INTO data-fabric as lineage events after the fact:

```json
{
  "event": "entity_resolution.v0",
  "entity_type": "loan",
  "reference_id": "223232",
  "target_id": "WFCM2019-C50|2",
  "canonical_id": "223232",
  "method": "structural",
  "strategy_id": "cmbs-loan-match.v1",
  "score": 0.92
}
```

Canon doesn't know or care about data-fabric. The tournament runner or decoding
routes events downstream.

---

## Implementation

### New dependencies

```toml
# canon/Cargo.toml additions
petgraph = "0.7"
serde_yaml = "0.9"               # strategy file parsing
```

No other new deps. Existing `rusqlite`, `serde`, `serde_json`, `blake3`, `csv`,
`clap` are sufficient.

### Module structure

```
canon/src/
├── cli.rs                    # existing + new resolve subcommand
├── input.rs                  # existing (CSV/JSONL parsing — reused for tape loading)
├── lib.rs                    # existing + resolve orchestration
├── lookup.rs                 # existing (SQLite exact match)
├── output/                   # existing (json, csv emitters)
├── refusal.rs                # existing + new refusal codes
├── registry.rs               # existing (load, build, diff, audit)
├── resolve/                  # NEW: cross-tape resolution module
│   ├── mod.rs                # resolve orchestration: load tapes → filter → score → emit
│   ├── strategy.rs           # strategy YAML parsing and validation
│   ├── graph.rs              # petgraph hydration from two tapes
│   ├── assertions.rs         # assertion operators (exact, tolerance, canon_match, etc.)
│   ├── scoring.rs            # candidate scoring and threshold/ambiguity evaluation
│   ├── writeback.rs          # registry write-back of matched ID pairs
│   └── gold.rs               # gold set scoring for tournament integration
└── witness.rs                # existing
```

### Build order

| Order | Component | LOC est. | What it does | Test strategy |
|-------|-----------|----------|-------------|---------------|
| 1 | **Strategy parser** | ~200 | Parse + validate strategy YAML. Verify column names, operator types, threshold ranges. | Fixture strategies, malformed input, missing fields |
| 2 | **Assertion operators** | ~250 | Pure functions: two values → (bool, f64). One function per operator. | Property-based tests (symmetry, tolerance boundaries, edge cases) |
| 3 | **Tape loader** | ~150 | Load CSV/JSONL tape into a `Vec<Record>` where Record has id (composite key) + attributes (BTreeMap). Reuse existing input.rs where possible. | Fixture tapes, missing columns, empty tapes, composite keys |
| 4 | **Graph hydration** | ~200 | Build petgraph from two tape record sets. Apply candidate filter to narrow pairs. | Synthetic tapes, verify node counts, filter effectiveness |
| 5 | **Candidate scoring** | ~200 | For each target record, score against filtered reference candidates. Apply threshold + ambiguity gap. | Known-score fixtures, threshold edge cases, tie-breaking |
| 6 | **Resolve orchestration** | ~200 | Wire together: load → hydrate → filter → score → decide → emit. | Integration tests with fixture tapes + strategies |
| 7 | **Gold scoring** | ~100 | Compare matches against gold cross-references. Compute accuracy, detect regressions. | Planted correct/incorrect matches |
| 8 | **Registry write-back** | ~100 | Write matched ID pairs to registry mapping file, rebuild index. | Verify new entries appear in subsequent lookups |
| 9 | **CLI integration** | ~150 | `canon resolve` subcommand, argument parsing, output formatting, exit codes. | CLI smoke tests |
| 10 | **Witness integration** | ~50 | Record resolve runs in the witness ledger. | Verify witness append |
| **Total** | | **~1600** | | |

### New refusal codes

| Code | Trigger |
|------|---------|
| `E_BAD_STRATEGY` | Strategy YAML is malformed, unknown operators, invalid thresholds |
| `E_COLUMN_NOT_FOUND` | Strategy references a column that doesn't exist in the tape |
| `E_TOO_MANY_CANDIDATES` | More candidates than `max_candidates` survived filter |
| `E_EMPTY_TAPE` | One or both tapes have zero parseable records |
| `E_INCOMPATIBLE_TAPES` | Tapes have no overlapping structural columns for comparison |

### Determinism contract

1. **Tape loading order.** Records are loaded in file order, then sorted by
   composite ID for deterministic graph insertion.

2. **Candidate filter order.** Filters are applied in strategy-declaration order.

3. **Assertion evaluation order.** Assertions evaluated in YAML array order.

4. **Tie-breaking.** When two reference candidates score identically against a
   target record, the candidate with the lexicographically smaller composite ID
   wins.

5. **Float comparison.** Tolerance operators use the exact formula in the
   strategy. Scores compared with `total_bits` ordering.

6. **Content-addressed strategy.** Strategy file hashed (blake3), hash recorded
   in every report.

---

## Testing philosophy

### Must-pass

- Two tapes with known matches → correct pairings produced
- All assertions pass → high score, match confirmed
- Optional fields missing → matching proceeds on available fields
- No candidates above threshold → UNMATCHED with near-misses
- Two candidates within ambiguity gap → AMBIGUOUS
- Tie-breaking → lexicographically smaller ID wins
- Gold set all correct → accuracy 1.0, empty regression list
- Gold set with planted regression → regression detected, listed
- Write-back with regressions → no entries written
- Write-back without regressions → entries appear in registry
- Write-back entries resolve via canon lookup on next run
- Same tapes + same strategy → byte-identical output
- Malformed strategy → REFUSAL E_BAD_STRATEGY
- Missing column → REFUSAL E_COLUMN_NOT_FOUND
- Empty tape → REFUSAL E_EMPTY_TAPE
- Large tapes (5K records each) complete without OOM
- Graph torn down after each run (no memory leak)

### Property-based tests (assertions module)

- `exact` is reflexive: `assert(x, x)` always passes
- `tolerance_pct` is symmetric: `assert(a, b) == assert(b, a)`
- `tolerance_pct` with tolerance=0.0 equivalent to `exact` for non-zero values
- `tolerance_abs` with tolerance=0.0 equivalent to `exact`
- `canon_match` with identical pre-resolved IDs always passes
- Score always in [0.0, 1.0]
- Failed required assertion → total score excludes that weight

### Golden file tests

- Fixture: reference tape (10 records), target tape (12 records)
- 8 match, 2 unmatched, 2 ambiguous
- Golden output file checked byte-for-byte
- Strategy content hash recorded and verified

---

## Example: matching loans across two CMBS tapes

### Setup

Deal WFCM2019-C50. Two tapes from different sources:

**Reference tape** (trustee_tape.csv):
```
loan_id,property_address,upb,rate,servicer,maturity_date,origination_date
223232,123 Main St Miami FL 33101,2440000,0.0425,Wells Fargo,2029-03-15,2019-03-15
223233,456 Oak Ave Tampa FL 33602,1200000,0.0375,Wells Fargo,2030-06-01,2020-06-01
223234,789 Pine Rd Orlando FL 32801,3100000,0.0500,US Bank,2028-11-01,2018-11-01
```

**Target tape** (servicer_tape.csv):
```
deal,loan_number,address,balance,coupon,servicer_name,maturity,orig_date
WFCM2019-C50,2,123 MAIN ST MIAMI FL 33101,2450000,0.0425,Wells Fargo Bank N.A.,2029-03-15,2019-03-15
WFCM2019-C50,3,456 OAK AVE TAMPA FL 33602,1200000,0.0375,Wells Fargo Bank N.A.,2030-06-01,2020-06-01
WFCM2019-C50,7,999 Elm Dr Jacksonville FL 32099,890000,0.0410,Wells Fargo Bank N.A.,2031-01-15,2021-01-15
```

### Matching

**Target record `WFCM2019-C50|2` vs reference candidates:**

Candidate filter narrows to records where `servicer` matches via canon_match
(both resolve to `C-Wells-Fargo`) and UPB within 20%. Candidates: 223232, 223233.

| Assertion | vs 223232 | vs 223233 |
|-----------|-----------|-----------|
| address/property_address (exact, 0.30, req) | PASS | FAIL |
| balance/upb (tolerance 5%, 0.25) | PASS (0.4%) | FAIL (104%) |
| coupon/rate (tolerance 0.001, 0.15) | PASS | FAIL |
| servicer_name/servicer (canon_match, 0.15) | PASS | PASS |
| maturity/maturity_date (date_range 5d, 0.10) | PASS | FAIL |
| orig_date/origination_date (exact, 0.05) | PASS | FAIL |
| **Total** | **1.00** | **0.15** |

Match: `WFCM2019-C50|2` → `223232`. Gap = 0.85. Clear.

**Target record `WFCM2019-C50|7`:**

No reference candidate has a matching property address. UPB filter produces no
candidates within 20% that also share a servicer. Result: UNMATCHED.

### Output

```
matched:   WFCM2019-C50|2 → 223232  (score: 1.00)
matched:   WFCM2019-C50|3 → 223233  (score: 0.95)
unmatched: WFCM2019-C50|7           (no candidate above 0.75)
```

### Write-back

New registry entries:

```json
{"input": "WFCM2019-C50|2", "canonical_id": "223232", "canonical_type": "loan_id", "rule_id": "STRUCTURAL_MATCH:cmbs-loan-match.v1"}
{"input": "WFCM2019-C50|3", "canonical_id": "223233", "canonical_type": "loan_id", "rule_id": "STRUCTURAL_MATCH:cmbs-loan-match.v1"}
```

Next time any tape contains `WFCM2019-C50|2`, it resolves via fast lookup. No
graph, no structural matching, no attributes needed.

### Validation on second deal

Run the same strategy on deal WFCM2020-C56:

```bash
canon resolve deal2_trustee.csv deal2_servicer.csv \
  --strategy cmbs-loan-match.v1.yaml \
  --registry registries/cmbs-loan/ \
  --gold gold/deal2_matches.jsonl
```

If accuracy holds, the methodology validates. If it drops, the tournament finds
what's different about deal 2 and discovers an adapted strategy.

---

## Sequencing

### Phase 1: Strategy parser + assertion operators (~450 LOC)

Pure functions, no I/O dependencies. Can be tested in complete isolation.

**Exit criteria:** All operators pass property-based tests. Malformed strategies
refused. Strategy content hashing works. Cross-tape column mapping validates.

### Phase 2: Tape loader + graph hydration (~350 LOC)

Load two tapes into petgraph. Apply candidate filter. Reuse existing CSV/JSONL
parsing from `input.rs` where possible.

**Exit criteria:** Fixture tapes load correctly. Graph has expected node counts.
Candidate filter narrows pairs. Deterministic node ordering verified.

### Phase 3: Scoring + resolve orchestration + CLI (~400 LOC)

Wire together the full pipeline. Add `canon resolve` subcommand.

**Exit criteria:** End-to-end matching works. Exit codes correct. Output contract
matches spec. Witness records runs.

### Phase 4: Gold scoring + write-back (~200 LOC)

Tournament integration. Write matched ID pairs to registry.

**Exit criteria:** Gold accuracy computed. Regressions detected. Write-back gated
on zero regressions. Feedback loop verified: write-back entries resolve via lookup.

### Phase 5: Tournament validation

Run the full loop externally on real CMBS tapes:
1. Pick one deal with trustee + servicer tapes
2. Have an analyst build a 50-100 loan gold cross-reference
3. Author initial strategy from domain knowledge
4. Run tournament: mutate strategy → canon resolve → score → promote/revert
5. Run promoted strategy on a second deal to validate generalization

**Exit criteria:** Tournament improves match rate without regressing gold.
Promoted strategy generalizes to at least one additional deal.

---

## Open questions

1. **More than two tapes.** For v0, canon takes exactly two tapes. When a third
   source appears (Bloomberg, rating agency), the caller can run pairwise:
   resolve trustee vs servicer, then resolve trustee vs Bloomberg, etc. All
   matched IDs accumulate in the same registry. Future: native multi-tape support.

2. **Bipartite constraint.** V0 uses greedy matching (best match per target
   record, independently). This can produce conflicts where two target records
   match the same reference record. V1 could use global bipartite matching
   (Hungarian algorithm) to ensure 1:1 assignment. For now, flag conflicts in
   the output as warnings.

3. **Address normalization.** The `exact` operator on addresses is brittle
   ("123 Main St" ≠ "123 MAIN STREET"). A future `address_match` operator
   (case-insensitive, abbreviation expansion) would help. Or the tournament
   discovers that address weight should be low when exact-match is the only
   option. Deferred — strategy weight mutation is the first lever.

4. **Canonical ID assignment for multi-source.** When three tapes each have
   different IDs and none is the natural "canonical" source, who wins?
   For v0: first tape (reference) wins. For v1: configurable canonical ID
   strategy (use CUSIP if available, else trustee ID, else synthetic).

5. **Column type inference.** The strategy says `tolerance_pct` but both tapes
   store UPB as strings. Canon needs to parse numeric columns. For v0: strategy
   declares expected type, canon parses and refuses on failure. No automatic
   inference.

---

## Success criteria

- Canon resolves 100+ deals per hour during tournament iteration
  (no infrastructure, in-process graph, fast teardown)
- The tournament discovers strategy improvements a human didn't specify
- Registry grows with each new deal resolved: match rate on subsequent deals
  improves as more ID variants accumulate
- Evidence chain is complete: every match traces to strategy version, assertion
  scores, and tape records
- A human can read the report and understand why target X was matched to
  reference Y
- Zero regressions on gold cross-reference set across strategy promotions
- A promoted strategy generalizes across at least two deals from the same
  deal family

---

## Final rule

If you can't explain why this target record matched that reference record by
pointing at the assertion scores, it doesn't ship. Cross-tape matching is
deterministic pattern matching with full evidence, not a black box that outputs
ID pairs.
