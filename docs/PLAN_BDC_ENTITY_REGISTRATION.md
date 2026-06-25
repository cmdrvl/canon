# canon register — BDC Entity Registration

> **Status**: Draft
> **Created**: 2026-03-18
> **Revised**: 2026-05-06
> **Context**: The BDC tournament extracts Schedule of Investments rows with ~20
> fields, but there is no entity resolution layer. The Identity Model
> (`cmdrvl-context/docs/09-plans/tournament/IDENTITY_MODEL.md`) defines three layers:
> `source_row_id` (exists), `issuer_canon_id` (this plan), `position_match_id`
> (future). This plan builds the second layer — canonical company identity.

> **Architecture status**: Legacy bootstrap domain plan for the organization
> workbench now generalized under `canon entity`. This older `canon register`
> framing should be read as BDC issuer-profile background, not a separate
> current CLI surface or compatibility alias.

---

## One-line thesis

Give canon one tape of extracted portfolio company names. It normalizes, clusters,
mints deterministic `IC-*` canonical IDs, and emits a registry — so the same company
always resolves to the same ID regardless of how the name was typed in the filing.

---

## Why this exists

BDC filings contain portfolio company names with predictable variation:

- `"Acme Corp."` vs `"ACME Corporation"` vs `"Acme Corp. (1)"`
- Legal suffixes: Inc., LLC, Corp., L.P., Ltd.
- Footnote markers: `(1)`, `(2)(3)`
- Parenthetical notes: `(Acquired Jan 2024)`
- Case drift, whitespace drift, trailing punctuation

Today these are distinct strings. Without entity resolution, the same company
appears as 3-5 rows in downstream analytics. Cross-quarter investment continuity
(Layer 3) is impossible without first knowing which names refer to the same company.

80%+ of BDC investments are private. There is no CUSIP, no FIGI, no Bloomberg
ticker. The portfolio company name is the only identifier. Entity resolution must
work from names alone.

---

## Relationship to CMBS cross-tape resolution

This is fundamentally different from `canon resolve`
(`PLAN_STRUCTURAL_RESOLUTION.md`):

| | BDC Entity Registration | CMBS Cross-Tape |
|---|---|---|
| Input | One tape (parser extraction output) | Two tapes (different sources) |
| Problem | Name dedup + ID minting | Structural cross-reference |
| IDs | Minted (`IC-*`) when new | Reference tape ID is canonical |
| External IDs | 80%+ private, no FIGI/CUSIP | Sometimes available |
| Graph | Not needed for v0 | petgraph for structural matching |
| Matching | Deterministic normalization | Weighted assertion scoring |

The user explicitly said to keep them separate: "lets keep it separate for now and
we will consider them together later."

---

## Design principles

1. **Canon stays a tool, not a loop.** Canon takes one tape + a strategy, emits
   a registration report. It does not iterate, optimize, or learn. The tournament
   drives canon from the outside.

2. **Deterministic given the same inputs.** Same tape + same strategy = same
   clusters, same IDs, every time. The canonical ID is a hash of the normalized
   key — process the same corpus twice, get identical output.

3. **Normalization is the matching strategy.** Unlike CMBS cross-tape (which uses
   weighted assertion scoring), BDC entity registration uses a pipeline of pure
   string transforms. The pipeline is the mutable artifact the tournament iterates.

4. **The registry is the only output.** Name variants are transient. The mapping
   from raw name to canonical ID is what persists.

5. **Strategies are promotable artifacts.** A normalization strategy is a versioned
   YAML file that the tournament discovers and promotes.

---

## Non-goals

This plan is NOT:

- A fuzzy matcher (no Jaro-Winkler, no edit distance, no ML embeddings)
- A record linker (it clusters names, not entity attribute bags)
- An NLP pipeline (no entity extraction, no coreference resolution)
- A master data management system
- Cross-quarter position matching (that's Layer 3, `position_match_id`)
- A replacement for the existing `canon` lookup fast path

---

## How it works

### CLI: `canon register`

```bash
canon register <INPUT> \
  --column <COLUMN> \
  --strategy <STRATEGY.yaml> \
  --registry <REGISTRY_DIR> \
  [--gold <GOLD_SET.jsonl>] \
  [--write-back] \
  [--emit json|summary] \
  [--no-witness]
```

Arguments:
- `<INPUT>`: CSV or JSONL file with extracted portfolio company names
- `--column`: Column containing names to register (e.g., `portfolio_company`)
- `--strategy`: Registration strategy YAML (normalization pipeline)
- `--registry`: Registry directory (existing entries checked first, write-back
  destination)
- `--gold`: Known-correct cluster assignments for scoring (optional)
- `--write-back`: Write new cluster entries to the registry
- `--emit`: Output mode (default: `json`)
- `--no-witness`: Suppress witness ledger

### The registration pipeline

```
extraction output (normalized_rows.csv with portfolio_company column)
  |
  v
1. Load portfolio_company values from input
2. Exact-lookup each name against existing registry (fast path)
3. Normalize unresolved names (strategy-defined pipeline)
4. Cluster by normalized key
5. Mint IC-* canonical ID per cluster: IC- + blake3(normalized_key)[0..12]
6. Optional: attempt OpenFIGI enrichment for syndicated instruments
7. Score against gold set if provided
8. Write-back new entries to registry if --write-back + zero regressions
```

### Three-tier matching

1. **Tier 1: Exact registry lookup** — existing canon mechanism, fast path. Names
   already in the registry resolve immediately. This is how subsequent runs avoid
   re-clustering names that were already registered.

2. **Tier 2: Normalized key clustering** — deterministic, no fuzzy matching. Names
   that normalize to the same key are clustered together and assigned the same
   canonical ID.

3. **Tier 3: Context-assisted graph scoring** — v1, uses petgraph. Deferred.
   Would use investment-type and deal context to break ties when normalization
   produces near-misses.

For v0, Tiers 1+2 are sufficient.

---

## Name normalization pipeline

Each step is a pure function `String → String`, applied in the order declared
in the strategy YAML:

```
"Acme Corp. (1)"
  → lowercase        → "acme corp. (1)"
  → strip_footnotes  → "acme corp."
  → strip_trailing   → "acme corp"
  → normalize_ws     → "acme corp"
  → strip_legal      → "acme"
  → strip_parens     → "acme"

"ACME Corporation"
  → lowercase        → "acme corporation"
  → strip_footnotes  → "acme corporation"
  → strip_trailing   → "acme corporation"
  → normalize_ws     → "acme corporation"
  → strip_legal      → "acme"
  → strip_parens     → "acme"
```

Both produce `"acme"` → same cluster → same `IC-*` ID.

### Normalization steps (v0)

| Step | Function | What it does |
|------|----------|-------------|
| `lowercase` | `s.to_lowercase()` | Case-fold |
| `strip_footnote_markers` | Remove trailing `(1)`, `(2)(3)` | Pattern: `\s*\(\d+\)\s*$` |
| `strip_trailing_punctuation` | Remove trailing `.`, `,` | Trim `.` and `,` from end |
| `normalize_whitespace` | Collapse multiple spaces | `\s+` → single space, trim |
| `strip_legal_suffixes` | Remove Inc., LLC, Corp., etc. | Pattern from strategy YAML |
| `strip_parenthetical_notes` | Remove `(Acquired Jan 2024)` | Pattern: `\s*\([^)]*\)\s*$` |

Each step is a separate, testable pure function. The strategy declares which
steps to apply and in what order. The tournament mutates this pipeline.

---

## Canonical ID minting

```
issuer_canon_id = "IC-" + blake3(normalized_key)[0..12]
```

- Deterministic: same normalized key always produces the same ID
- Content-addressed: the ID is derived from the key, not from insertion order
- Prefix `IC-` distinguishes issuer canon IDs from other ID types
- 12 hex characters from blake3 = 48 bits = collision space of ~281 trillion

---

## Strategy YAML schema

```yaml
strategy_id: bdc-issuer-match.v1
strategy_version: "0.1.0"
entity_type: issuer
description: "Cluster and register BDC portfolio company names"

identity:
  name_column: portfolio_company
  id_prefix: "IC"

normalization:
  - lowercase
  - strip_footnote_markers
  - strip_trailing_punctuation
  - normalize_whitespace
  - strip_legal_suffixes
  - strip_parenthetical_notes

legal_suffix_patterns:
  - "\\b(inc|incorporated|corp|corporation|co|company|llc|lp|l\\.p\\.|ltd|limited|plc)\\b\\.?"

footnote_marker_pattern: "\\s*\\(\\d+\\)\\s*$"
parenthetical_note_pattern: "\\s*\\([^)]*\\)\\s*$"

figi_lookup:
  enabled: true
  eligible_investment_types:
    - "Senior Secured First Lien Term Loan"
    - "Senior Secured Bond"
    - "Senior Unsecured Bond"
```

### What the tournament mutates

1. **Normalization pipeline**: add, remove, or reorder steps
2. **Legal suffix patterns**: expand or tighten the regex
3. **Footnote/parenthetical patterns**: adjust for new filing quirks
4. **FIGI eligibility rules**: expand or restrict investment types

### What the tournament holds frozen

- The input tape (same names for all strategy variants in one round)
- The gold cluster assignments
- The registry (existing entries)
- Canon's registration code

---

## Gold set format and scoring

Gold set uses **pairwise cluster agreement**, not ID matching. Two names that
should resolve to the same company share a gold ID:

```jsonl
{"raw_name": "Acme Corp", "expected_issuer_id": "GOLD-ACME-001"}
{"raw_name": "ACME Corporation", "expected_issuer_id": "GOLD-ACME-001"}
{"raw_name": "Acme Corp. (1)", "expected_issuer_id": "GOLD-ACME-001"}
{"raw_name": "Beta Holdings LLC", "expected_issuer_id": "GOLD-BETA-001"}
{"raw_name": "Beta Holdings", "expected_issuer_id": "GOLD-BETA-001"}
```

### Metrics

- **Precision**: no over-merge (two names that shouldn't cluster together don't)
- **Recall**: no under-merge (two names that should cluster together do)
- **F1**: harmonic mean
- **Gate**: `regressions.len() == 0` — no previously-correct clusters broken

### How gold sets are built

A human analyst reviews 50-100 portfolio company names from 2-3 BDC filings and
manually groups them. This is a one-time effort per filing family. The gold set is
frozen; the strategy mutates.

---

## Output contract (`canon_register.v0`)

```json
{
  "version": "canon_register.v0",
  "strategy": {
    "id": "bdc-issuer-match.v1",
    "version": "0.1.0",
    "content_hash": "blake3:9f2a..."
  },
  "summary": {
    "total_names": 450,
    "resolved_existing": 380,
    "clustered_new": 65,
    "ambiguous": 5,
    "clusters_created": 42
  },
  "clusters": [
    {
      "canonical_id": "IC-7f3a2b9c1d4e",
      "normalized_key": "acme",
      "members": ["Acme Corp", "ACME Corporation", "Acme Corp. (1)"],
      "tier": "normalized_cluster"
    }
  ],
  "ambiguous": [
    {
      "raw_name": "ABC Holdings",
      "candidates": ["IC-aaa111", "IC-bbb222"],
      "reason": "normalized_key_collision"
    }
  ],
  "gold_score": {
    "precision": 0.99,
    "recall": 0.985,
    "f1": 0.987,
    "regressions": []
  }
}
```

---

## Registry write-back

### What gets written

When `--write-back` is specified and gold validation passes (zero regressions):

For each cluster, write one registry entry per raw name variant:

```json
{"input": "Acme Corp", "canonical_id": "IC-7f3a2b9c1d4e", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
{"input": "ACME Corporation", "canonical_id": "IC-7f3a2b9c1d4e", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
{"input": "Acme Corp. (1)", "canonical_id": "IC-7f3a2b9c1d4e", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
```

Written to timestamped mapping file: `register-YYYYMMDD.json`. The registry's
`_index.sqlite` is rebuilt to include them.

### What does NOT get written

- No investment attributes (fair value, coupon, maturity)
- No positional data
- No filing metadata
- No growing database

The registry stays flat: `input string → canonical ID`. Same format canon has
always used.

### Write-back safety

- Write-back only fires with explicit `--write-back` flag
- Gated on gold validation: any regression → no entries written
- Creates new mapping files (never modifies existing ones)
- Registry version NOT bumped (manual/CI step after review)
- Each entry carries `rule_id` for auditability

### Registry scale after 100 filings

- ~3,000-5,000 raw name variants → ~1,500-2,500 unique issuers
- ~500KB mapping files, ~300KB SQLite index
- Sublinear growth: most new filings add mostly existing issuers

---

## OpenFIGI integration

Already implemented in `canon/src/registry/provider.rs`. The provider itself is
a general corpus-scoped CUSIP/ISIN/SEDOL registry materializer; in this BDC flow
it is used as **secondary enrichment**, not primary resolution:

- Only attempted for syndicated instrument types (strategy-configured)
- FIGI result stored as enrichment metadata, not as the canonical ID
- 80%+ of BDC investments will miss — this is expected and fine

---

## Exit codes

- `0`: All names resolved or clustered
- `1`: Partial (some ambiguous names)
- `2`: Refusal (bad input, bad strategy)

---

## Implementation

### Files to modify

| File | Change |
|------|--------|
| `canon/src/cli.rs` | Add `Register` variant to `CanonCommand`, `RegisterCli` args struct |
| `canon/src/lib.rs` | Add `pub mod register;`, `run_register()` orchestration, `CanonRegisterOutput` types |
| `canon/src/refusal.rs` | Add `EBadStrategy` refusal code |
| `canon/Cargo.toml` | Add `serde_yaml = "0.9"` |

### New files

```
canon/src/register/
  mod.rs          # orchestration: load input → lookup existing → normalize → cluster → emit
  strategy.rs     # strategy YAML parsing and validation
  normalize.rs    # name normalization pipeline (pure functions)
  cluster.rs      # deterministic clustering by normalized key, IC-* ID minting
  gold.rs         # pairwise cluster agreement scoring
  writeback.rs    # registry write-back of entity mappings
```

### Reused existing modules

- `input.rs` — CSV/JSONL parsing (load extraction output)
- `lookup.rs` — exact-match fast path (resolve names already in registry)
- `registry.rs` — load registry, write mapping files, rebuild index
- `registry/provider.rs` — OpenFIGI enrichment
- `witness.rs` — witness ledger
- `refusal.rs` — refusal envelope
- `output/` — JSON emitters

### New dependency

```toml
serde_yaml = "0.9"   # shared with CMBS plan if it lands first
```

No petgraph for v0. Enters with Tier 3 context-assisted scoring.

### New refusal codes

| Code | Trigger |
|------|---------|
| `E_BAD_STRATEGY` | Strategy YAML is malformed, unknown normalization steps, missing fields |

---

## Build order

| # | Component | ~LOC | What it does | Test strategy |
|---|-----------|------|-------------|---------------|
| 1 | Strategy parser (`register/strategy.rs`) | ~150 | Parse + validate strategy YAML. Verify normalization steps, patterns, column name. | Fixture YAMLs, malformed input, missing fields |
| 2 | Name normalizer (`register/normalize.rs`) | ~200 | Pure functions: `String → String`. One function per normalization step. Pipeline composed from strategy. | Property tests (idempotence, stability), edge cases |
| 3 | Cluster builder (`register/cluster.rs`) | ~150 | Group names by normalized key, mint `IC-*` IDs via blake3 hash. | Known input/output pairs, deterministic IDs |
| 4 | Existing registry integration (in `register/mod.rs`) | ~100 | Fast-path: lookup names against existing registry before normalizing. | Partial resolution, all-resolved, none-resolved |
| 5 | Register orchestration (`register/mod.rs`) | ~200 | Wire together: load → lookup → normalize → cluster → emit. | Integration tests with fixtures |
| 6 | Gold scoring (`register/gold.rs`) | ~150 | Pairwise cluster agreement: precision, recall, F1, regressions. | Planted correct/incorrect clusters |
| 7 | Registry write-back (`register/writeback.rs`) | ~100 | Write cluster entries to registry mapping file, rebuild index. | Round-trip: write-back → lookup resolves |
| 8 | CLI integration (`cli.rs` + `lib.rs`) | ~150 | `canon register` subcommand, argument parsing, output formatting, exit codes. | Smoke tests, exit codes |
| 9 | OpenFIGI enrichment (in orchestration) | ~100 | Attempt FIGI lookup for eligible investment types, store as metadata. | Mock provider, miss handling |
| 10 | Witness integration | ~50 | Record register runs in the witness ledger. | Verify witness append |
| **Total** | | **~1350** | | |

---

## Tournament integration

```bash
# Phase 1: parser extracts normalized_rows.csv (existing)
# Phase 2: register entities
canon register normalized_rows.csv \
  --column portfolio_company \
  --strategy bdc-issuer-match.v1.yaml \
  --registry registries/bdc-issuers/ \
  --gold gold/issuer_matches.jsonl \
  --write-back --emit json > registration_report.json

# Phase 3: resolve entities for downstream use (existing canon)
canon normalized_rows.csv \
  --registry registries/bdc-issuers/ \
  --column portfolio_company \
  --emit csv > resolved_rows.csv
```

### Tournament loop

```
1. Propose strategy mutation
   (e.g., reorder normalization steps, add strip_dba step,
   expand legal suffix patterns, change footnote regex)

2. Run: canon register normalized_rows.csv \
          --column portfolio_company \
          --strategy proposed.yaml \
          --registry registries/bdc-issuers/ \
          --gold gold/issuer_matches.jsonl

3. Score:
   - Primary: gold_score.f1 (higher is better)
   - Gate: gold_score.regressions must be empty
   - Secondary: summary.ambiguous (lower is better)
   - Tertiary: summary.clusters_created (fewer is better, if F1 holds)

4. Decide:
   - If f1 > champion AND regressions == 0: PROMOTE
   - If f1 == champion AND ambiguous < champion: PROMOTE
   - Else: REVERT

5. Repeat until convergence or max rounds
```

### What the tournament discovers

- "strip_legal_suffixes before strip_parenthetical_notes produces fewer clusters"
- "Adding `\\bd/b/a\\b` to legal suffixes catches DBA variants"
- "The footnote pattern needs `\\(\\d+\\)` in the middle of the string, not just end"
- "Investment type `Unitranche` should be eligible for FIGI lookup"
- "Some BDC filers use semicolons instead of commas — normalize_punctuation step needed"

---

## Determinism contract

1. **Input loading order.** Names loaded in file order, then sorted for clustering.

2. **Normalization order.** Steps applied in YAML-declaration order.

3. **Cluster ordering.** Clusters sorted by normalized key for deterministic output.

4. **ID minting.** `blake3(normalized_key)` — no randomness, no counters, no
   insertion order dependency.

5. **Content-addressed strategy.** Strategy file hashed (blake3), hash recorded
   in every report.

---

## Testing philosophy

### Must-pass

- 20 portfolio company names from 3 BDC filings → correct clusters
- Golden file test: byte-identical output for same input + strategy
- Names already in registry → resolve via fast path, not re-clustered
- Write-back entries resolve via standard `canon` lookup on next run
- Same tape + same strategy → byte-identical output (determinism)
- Malformed strategy → REFUSAL E_BAD_STRATEGY
- Empty input → zero clusters, zero errors
- All names identical after normalization → one cluster
- All names distinct after normalization → N clusters (one per name)
- Gold set with planted regression → regression detected, write-back blocked

### Property-based tests (normalize module)

- Each normalization step is idempotent: `f(f(x)) == f(x)`
- Each step preserves non-empty input: `f(x).len() > 0` if `x.len() > 0`
  (except: a name that IS only a legal suffix reduces to empty — handle as edge case)
- Normalization pipeline is deterministic: same input → same output
- blake3 hash is deterministic: same normalized key → same IC-* ID

### Golden file tests

- Fixture: 20 portfolio company names with known cluster assignments
- Golden output file checked byte-for-byte
- Strategy content hash recorded and verified

---

## Example: registering portfolio companies from a BDC filing

### Input

Extracted from a Schedule of Investments:

```csv
portfolio_company,investment_type,fair_value
"Acme Corp.",Senior Secured First Lien Term Loan,15000000
"ACME Corporation",Senior Secured First Lien Revolver,2000000
"Acme Corp. (1)",Senior Secured First Lien Term Loan,15000000
"Beta Holdings LLC",Senior Secured Second Lien Term Loan,8000000
"Beta Holdings",Senior Unsecured Bond,3000000
"Gamma Inc.",Equity,500000
```

### Registration

```bash
canon register investments.csv \
  --column portfolio_company \
  --strategy bdc-issuer-match.v1.yaml \
  --registry registries/bdc-issuers/ \
  --write-back --emit json
```

### Normalization trace

| Raw name | After pipeline | Cluster |
|----------|---------------|---------|
| `Acme Corp.` | `acme` | IC-7f3a2b9c1d4e |
| `ACME Corporation` | `acme` | IC-7f3a2b9c1d4e |
| `Acme Corp. (1)` | `acme` | IC-7f3a2b9c1d4e |
| `Beta Holdings LLC` | `beta holdings` | IC-a2c4e6f8b1d3 |
| `Beta Holdings` | `beta holdings` | IC-a2c4e6f8b1d3 |
| `Gamma Inc.` | `gamma` | IC-d5e7f9a1b3c5 |

### Output summary

```
resolved_existing: 0    (fresh registry)
clustered_new: 6        (6 raw names)
clusters_created: 3     (3 unique issuers)
ambiguous: 0
```

### Write-back

```json
{"input": "Acme Corp.", "canonical_id": "IC-7f3a2b9c1d4e", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
{"input": "ACME Corporation", "canonical_id": "IC-7f3a2b9c1d4e", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
{"input": "Acme Corp. (1)", "canonical_id": "IC-7f3a2b9c1d4e", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
{"input": "Beta Holdings LLC", "canonical_id": "IC-a2c4e6f8b1d3", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
{"input": "Beta Holdings", "canonical_id": "IC-a2c4e6f8b1d3", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
{"input": "Gamma Inc.", "canonical_id": "IC-d5e7f9a1b3c5", "canonical_type": "issuer_canon_id", "rule_id": "NORMALIZED_CLUSTER:bdc-issuer-match.v1"}
```

### Next filing

A second BDC filing arrives with `"Acme Corp"` and `"Delta Systems Inc."`:

- `"Acme Corp"` → registry lookup finds `IC-7f3a2b9c1d4e` → resolved (Tier 1)
- `"Delta Systems Inc."` → not in registry → normalize → `"delta systems"` → mint
  new `IC-*` → write-back

The registry grows monotonically. Each new filing mostly resolves via fast lookup,
with a shrinking tail of new names.

---

## Sequencing

### Phase 1: Strategy parser + name normalizer (~350 LOC)

Pure functions, no I/O dependencies. Can be tested in complete isolation.

**Exit criteria:** All normalization steps pass property-based tests (idempotence,
stability). Malformed strategies refused. Strategy content hashing works. Pipeline
composition from YAML order verified.

### Phase 2: Cluster builder + registry integration (~250 LOC)

Deterministic clustering by normalized key. IC-* ID minting. Fast-path lookup
against existing registry.

**Exit criteria:** Known inputs produce expected clusters. blake3 IDs are
deterministic. Existing registry entries resolve via Tier 1. New names cluster
via Tier 2.

### Phase 3: Orchestration + CLI (~350 LOC)

Wire together the full pipeline. Add `canon register` subcommand.

**Exit criteria:** End-to-end registration works. Exit codes correct. Output
contract matches spec. Witness records runs.

### Phase 4: Gold scoring + write-back (~250 LOC)

Tournament integration. Write cluster entries to registry.

**Exit criteria:** Gold F1 computed. Regressions detected. Write-back gated on
zero regressions. Feedback loop verified: write-back entries resolve via standard
`canon` lookup.

### Phase 5: Tournament validation

Run the full loop externally on real BDC filings:
1. Extract 2-3 BDC Schedules of Investments
2. Have an analyst group 50-100 portfolio company names into gold clusters
3. Author initial strategy from domain knowledge
4. Run tournament: mutate strategy → canon register → score → promote/revert
5. Run promoted strategy on a new filing to validate generalization

**Exit criteria:** Tournament improves F1 without regressing gold. Promoted
strategy generalizes to at least one additional filing.

---

## Open questions

1. **DBA variants.** Some filings use "doing business as" (`d/b/a`) names alongside
   legal names. A `strip_dba` normalization step would help. Deferred to tournament
   discovery — if it matters, the tournament will find it.

2. **Subsidiary disambiguation.** "Acme Holdings" and "Acme Operating" may be the
   same issuer or different. For v0, they normalize to different keys (correct
   conservative behavior). Tier 3 context-assisted scoring would use investment-type
   and deal structure to disambiguate.

3. **Cross-BDC-filer consistency.** Different BDC filers may spell the same company
   differently. For v0, the registry grows across filings, so the first variant
   mints the ID and subsequent variants accumulate. Cross-filer gold sets would
   validate this.

4. **Enrichment beyond FIGI.** SEC EDGAR entity search, LEI lookups, and LinkedIn
   company pages could enrich the registry. Deferred — the architecture supports
   additional providers via `registry/provider.rs`.

5. **When normalization over-merges.** If `strip_legal_suffixes` collapses two
   genuinely different companies to the same key (e.g., "Delta" the airline and
   "Delta" the faucet company), the gold set catches it as a regression. The
   tournament can discover that a more selective suffix pattern avoids the collision.

---

## Success criteria

- Canon registers 500+ portfolio company names per second (no network calls
  in the critical path, normalization is pure string transforms)
- The tournament discovers normalization improvements a human didn't specify
- Registry grows with each new filing: resolution rate on subsequent filings
  improves as more name variants accumulate
- Evidence chain is complete: every cluster traces to strategy version, normalization
  steps, and raw input names
- A human can read the report and understand why name X was clustered with name Y
- Zero regressions on gold cluster assignments across strategy promotions
- A promoted strategy generalizes across at least two BDC filings from different
  filers

---

## Final rule

If you can't explain why two names ended up in the same cluster by showing the
normalization trace, it doesn't ship. Entity registration is deterministic string
normalization with full evidence, not a black box that outputs ID pairs.
