# canon

<div align="center">

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**The same entity has five names across three vendors. canon makes them one.**

```bash
brew install cmdrvl/tap/canon
```

</div>

---

The same loan appears as CUSIP `037833100` in one system, ISIN `US0378331005` in another, and ticker `AAPL` in a third. Three vendors, three identifiers, one entity. Your reconciliation pipeline needs them to be the same row. Right now, the mapping lives in a VLOOKUP chain, an unmaintained Python script, or someone's head.

**canon resolves identifiers against versioned registries — deterministic, traceable, reproducible.** Every resolution records which registry version was used, which rule produced the match, and what didn't match. Same input plus same registry version equals same output, every time. No fuzzy matching, no silent normalization, no guessing.

### What makes this different

- **Versioned registries** — every resolution is pinned to a registry version with semver. When the registry updates, `canon registry diff` tells you exactly what changed. Registries are plain JSON directories — inspectable in git, diffable, no database required.
- **Pipeline composable** — `canon --emit csv` appends a `<column>__canon` column to your CSV. Pipe the output directly into `rvl` or `shape`: `canon nov.csv --column cusip --emit csv | rvl - dec.canon.csv --key cusip__canon`.
- **Full traceability** — every mapping includes `rule_id`, `canonical_type`, and `confidence`. Every unresolved entry includes the reason. Every result is auditable.
- **Deduplication built in** — input values are deduplicated before lookup. 500 unique CUSIPs produce 500 mapping entries whether your file has 500 rows or 500,000.
- **Org identity resolution** — `canon org` resolves entities that appear under different names across documents via a deterministic multi-stage pipeline: block, score evidence, solve clusters, audit against evaluation suites, and promote into the registry.

---

## Quick Example

```bash
$ canon tape.csv --registry registries/cusip-isin/ --column cusip
```

```json
{
  "version": "canon.v0",
  "outcome": "PARTIAL",
  "registry": { "id": "cusip-isin", "version": "3.2.1", "source": "registries/cusip-isin/" },
  "summary": { "total": 3, "resolved": 2, "unresolved": 1 },
  "mappings": [
    { "input": "u8:037833100", "canonical_id": "u8:AAPL", "canonical_type": "ticker", "rule_id": "CUSIP_TO_TICKER", "confidence": "deterministic" },
    { "input": "u8:594918104", "canonical_id": "u8:MSFT", "canonical_type": "ticker", "rule_id": "CUSIP_TO_TICKER", "confidence": "deterministic" }
  ],
  "unresolved": [
    { "input": "u8:UNKNOWN99", "reason": "no matching rule" }
  ],
  "refusal": null
}
```

Two out of three resolved. One didn't match anything in the registry. Exit code 1 (PARTIAL).

```bash
# Pipeline mode — canonicalize and compare in one shot:
$ canon nov.csv --registry registries/cusip-isin/ --column cusip --emit csv > nov.canon.csv
$ canon dec.csv --registry registries/cusip-isin/ --column cusip --emit csv > dec.canon.csv
$ rvl nov.canon.csv dec.canon.csv --key cusip__canon

# What didn't resolve?
$ canon tape.csv --registry registries/cusip-isin/ --column cusip | jq '.unresolved[]'

# Exit code only (for scripts):
$ canon tape.csv --registry registries/cusip-isin/ --column cusip > /dev/null 2>&1
$ echo $?  # 0 = all resolved, 1 = partial/unresolved, 2 = refused
```

---

## The Four Outcomes

`canon` always produces exactly one of four outcomes. Every input value is classified as resolved or unresolved — no third bucket.

### 1. RESOLVED

Every input value mapped to a canonical ID.

```
summary: { total: 4183, resolved: 4183, unresolved: 0 }
```

Exit 0. The mapping is complete. Every resolution is traceable to a specific registry entry and rule ID.

### 2. PARTIAL

At least one input resolved AND at least one didn't.

```
summary: { total: 4183, resolved: 4150, unresolved: 33 }
```

Exit 1. Resolved mappings are still valid — partial is not a failure, it's an honest report. Unresolved entries include the reason (no matching rule, empty value, etc.).

### 3. UNRESOLVED

Zero inputs could be mapped.

```
summary: { total: 4183, resolved: 0, unresolved: 4183 }
```

Exit 1. Distinct from REFUSAL — the tool operated correctly, it just found no matches. Check the registry or input values.

### 4. REFUSAL

Cannot operate (bad input, bad registry, missing column, etc.).

```json
{
  "outcome": "REFUSAL",
  "refusal": {
    "code": "E_COLUMN_NOT_FOUND",
    "message": "Column 'cusip' not found in input file",
    "detail": { "column": "cusip", "available_columns": ["security_id", "isin", "name"] },
    "next_command": "canon positions.csv --registry registries/cusip-isin/ --column security_id"
  }
}
```

Exit 2. Every refusal includes a recovery path — either a `next_command` or escalation guidance.

---

## How It Works

### Registries

A registry is a versioned directory of JSON mapping files:

```
registries/cusip-isin/
├── registry.json            # Metadata: id, version, description, updated
├── cusip-to-isin.json       # Mapping file
├── cusip-to-ticker.json     # Mapping file
└── _build.json              # Optional build provenance; ignored during resolution
```

Each mapping file is an array of entries:

```json
{"input": "037833100", "canonical_id": "AAPL", "canonical_type": "ticker", "rule_id": "CUSIP_TO_TICKER"}
{"input": "Wells Fargo", "canonical_id": "C-00012", "canonical_type": "counterparty_id", "rule_id": "COUNTERPARTY_ALIAS"}
{"input": "WFB", "canonical_id": "C-00012", "canonical_type": "counterparty_id", "rule_id": "COUNTERPARTY_ALIAS"}
```

Registries are versioned with semver, inspectable in git, and diffable. A SQLite derived index is built automatically for fast lookups against large registries. `_build.json` is reserved for materializer provenance and is ignored during normal resolution.

### Matching

v0 matching is **exact byte match after ASCII-trim**. No uppercasing, no punctuation stripping, no stemming. The registry is the complete source of truth — if you need case-insensitive matching, include all case variants as registry entries.

Mapping files are evaluated in filename-sorted order. First match wins.

### Deduplication

Input values are deduplicated before lookup. Output arrays contain one entry per **unique** input value, not one per row. `summary.total` counts unique values, keeping output proportional to cardinality — 500 unique CUSIPs produce 500 mapping entries whether the file has 500 or 500,000 rows.

---

## Output Modes

### JSON (default: `--emit json`)

Single JSON object to stdout. The mapping artifact for audit, `pack`, or inspection.

```bash
canon tape.csv --registry registries/cusip-isin/ --column cusip
```

### CSV (`--emit csv`)

Original CSV with a canonical column appended. Makes `canon` a pipeline stage.

```bash
$ canon tape.csv --registry registries/cusip-isin/ --column cusip --emit csv
cusip,balance,rate,cusip__canon
037833100,1000000,3.5,AAPL
594918104,500000,4.2,MSFT
UNKNOWN99,250000,2.8,
```

Unresolved rows get an empty canonical column. The exit code tells you whether to trust it blindly (exit 0) or inspect (exit 1).

Use `--map-out <PATH>` to write the JSON mapping artifact as a sidecar:

```bash
canon tape.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/tape.map.json > tape.canon.csv
```

---

## How canon Compares

| Capability | canon | VLOOKUP / INDEX-MATCH | Custom Python script | MDM platform |
|------------|-------|----------------------|---------------------|--------------|
| Versioned mappings | Registry version in every output | Untracked | Ad-hoc | Yes |
| Deterministic | Same input + version = same output | Depends on sheet state | Depends on code | Usually |
| Traceable | Rule ID + registry version per mapping | Manual | You build it | Varies |
| Pipeline-composable | `--emit csv \| rvl` | No | Possible | Heavy |
| Refusal on ambiguity | Refuses, never guesses | Silent errors | Crashes | Varies |
| Setup time | One command | N/A | Hours | Months |

**When to use canon:**
- Normalizing identifiers before reconciliation (`canon --emit csv | rvl`)
- Resolving counterparty aliases across vendor datasets
- Building audit trails for regulatory mappings (every resolution traceable)

**When canon might not be ideal:**
- Fuzzy entity matching (address variants, phonetic matching)
- Master data management at enterprise scale
- Probabilistic record linkage requiring ML models

---

## Installation

### Homebrew (Recommended)

```bash
brew install cmdrvl/tap/canon
```

### Shell Script

```bash
curl -fsSL https://raw.githubusercontent.com/cmdrvl/canon/main/scripts/install.sh | bash
```

### From Source

```bash
cargo build --release
./target/release/canon --help
```

---

## CLI Reference

```bash
canon <INPUT> --registry <REGISTRY> --column <COLUMN> [OPTIONS]
canon registry build --source <SOURCE> --seed <SEED> --seed-column <COLUMN> --output <DIR> --version <VER> [OPTIONS]
canon registry diff --old <OLD_REGISTRY> --new <NEW_REGISTRY> [--emit json|summary]
canon registry audit <SEED> --registry <REGISTRY> --column <COLUMN> [--emit json|summary]
canon registry lint <REGISTRY> [--profile standard|org|strategy|auto] [--emit json|summary]
canon strategy profile <INPUT> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]
canon strategy audit --schema <PROFILE.json> --script <SCRIPT> --suite <DIR> [--emit json|summary]
canon strategy resolve --registry <DIR> --schema <SCHEMA.json> --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]
canon strategy register --registry <DIR> --schema <SCHEMA.json> --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json> --next-version <VER> [--emit json|summary]
canon strategy diff --old <OLD_DIR> --new <NEW_DIR> [--emit json|summary]
canon org run <ROWS> --strategy <YAML> --registry <DIR> [--suite <DIR>] [--emit json|summary]
canon org block|edge|solve|audit|promote|explain [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<INPUT>` | CSV or JSONL file. Format detected by extension (`.csv`, `.tsv`, `.jsonl`, `.ndjson`). Use `-` for stdin (JSONL only). |

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--registry <PATH>` | string | *(required)* | Registry directory (versioned). |
| `--column <COLUMN>` | string | *(required)* | Column containing IDs to resolve. |
| `--emit <json\|csv>` | string | `json` | Output mode. `csv` requires CSV input. |
| `--canon-column <NAME>` | string | `<COLUMN>__canon` | Name of the appended canonical column. Only with `--emit csv`. |
| `--map-out <PATH>` | string | *(none)* | Write JSON mapping artifact to file. Only with `--emit csv`. |
| `--max-rows <N>` | integer | *(none)* | Refuse if input exceeds N data rows. |
| `--max-bytes <N>` | integer | *(none)* | Refuse if input exceeds N bytes. |
| `--no-witness` | flag | `false` | Suppress witness ledger append. |
| `--version` | flag | | Print version and exit. |
| `--describe` | flag | | Emit `operator.json` to stdout and exit. |
| `--schema` | flag | | Print JSON Schema for the mapping artifact and exit. |

### Subcommands

| Subcommand | Description |
|------------|-------------|
| `registry build --source <NAME> --seed <PATH> --seed-column <COLUMN> --output <DIR> --version <VER>` | Materialize a standard canon registry directory from a provider-backed seed corpus, with optional repeatable `--provider-config key=value` overrides. |
| `registry diff --old <PATH> --new <PATH> [--emit json\|summary]` | Compare two versions of the same registry ID and report added, removed, changed, and unchanged effective mappings. |
| `registry audit <SEED> --registry <PATH> --column <COLUMN> [--emit json\|summary]` | Audit a seed corpus against a registry and emit resolved/unresolved entries plus aggregate canonical-target and rule-hit counts. |
| `registry lint <DIR> [--profile standard\|org\|strategy\|auto] [--emit json\|summary]` | Preflight standard mapping, org, or strategy registry health with severity-tagged findings. |
| `strategy profile <INPUT> [--emit json\|summary] [--max-rows <N>] [--max-bytes <N>]` | Derive a deterministic schema/profile artifact from CSV, TSV, JSONL, or NDJSON for `strategy resolve` and `strategy register`. |
| `strategy audit --schema <JSON> --script <PATH> --suite <DIR> [--emit json\|summary]` | Run a frozen script against deterministic fixture expectations and emit a `canon_strategy_audit.v0` proof artifact. |
| `strategy resolve --registry <DIR> --schema <JSON> --skill <PATH>\|--skill-hash <HASH>` | Resolve a schema shape plus skill hash to a frozen champion script. EXACT and COMPATIBLE exit `0`; PARTIAL and UNRESOLVED exit `1`. |
| `strategy register --registry <DIR> --schema <JSON> --skill <PATH>\|--skill-hash <HASH> --script <PATH> --script-id <ID> --language <LANG> --verify <JSON> --assess <JSON> --airlock <JSON> --next-version <VER>` | Register a frozen script after verify, assess, and airlock proof artifacts pass. |
| `strategy diff --old <DIR> --new <DIR> [--emit json\|summary]` | Compare frozen-script strategy registry versions by effective `(schema_fingerprint, skill_hash)` entries. |
| `org run <ROWS> --strategy <YAML> --registry <DIR> [--suite <DIR>] [--emit json\|summary]` | Run the full deterministic org-identity pipeline (block → edge → solve, optional audit + promote). |
| `org block <ROWS> --strategy <YAML> --registry <DIR> [--emit jsonl\|summary]` | Generate candidate neighborhoods via blocking operators. |
| `org edge <ROWS> --strategy <YAML> --candidates <JSONL> --registry <DIR> [--emit jsonl\|summary]` | Score typed evidence edges for blocked candidate pairs. |
| `org solve <ROWS> --strategy <YAML> --edges <JSONL> --registry <DIR> [--emit json\|summary]` | Solve deterministic identity assignments from evidence edges. |
| `org audit <RESULT> --suite <DIR> [--emit json\|summary]` | Validate a solve/run artifact against a frozen evaluation suite. |
| `org promote <RESULT> --audit <JSON> --registry <DIR> --next-version <VER> [--emit json\|summary]` | Write audited results into registry aliases and escrow sidecars. |
| `org explain <RESULT> --row <ID>\|--canon-id <ID>\|--escrow-id <ID> [--emit json\|summary]` | Proof trace for one row, canonical entity, or escrow entity. |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | RESOLVED (all inputs mapped) |
| `1` | PARTIAL or UNRESOLVED (some or all inputs unresolved) |
| `2` | REFUSAL or CLI error |

`canon registry diff`, `canon registry audit`, and `canon registry lint` exit `0` when the report succeeds and `2` on refusal. Lint findings are represented inside `canon_registry_lint.v0` rather than via exit status. `canon registry build` exits `0` when materialization succeeds and `2` on refusal; provider failures are preserved in the JSON report and warned on stderr.

`canon strategy profile`, `canon strategy register`, and `canon strategy diff` exit `0` when their reports or writes succeed and `2` on refusal. `canon strategy audit` exits `0` when all fixtures pass, `1` when deterministic fixture checks fail, and `2` on refusal. `canon strategy resolve` exits `0` for an EXACT or COMPATIBLE frozen-script match, `1` for PARTIAL or UNRESOLVED, and `2` on refusal.

### Output Routing

| `--emit` | stdout | Mapping artifact | Use case |
|----------|--------|------------------|----------|
| `json` (default) | JSON mapping object | IS stdout | Audit, pack, inspection |
| `csv` | Canonicalized CSV | `--map-out` sidecar | Pipeline stage |

---

## Scripting Examples

Canonicalize and compare (the core workflow):

```bash
canon nov.csv --registry registries/cusip-isin/ --column cusip --emit csv > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip --emit csv > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon
```

Audit-grade pipeline with evidence:

```bash
canon nov.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/nov.map.json > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/dec.map.json > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon --json > evidence/rvl.json
pack seal evidence/ --note "Nov->Dec recon with canonical CUSIPs"
```

Inspect unresolved entries:

```bash
canon tape.csv --registry registries/cusip-isin/ --column cusip | jq '.unresolved[]'
```

Review what changed before rolling a registry version:

```bash
canon registry diff \
  --old registries/openfigi-cusip-v2026.02/ \
  --new registries/openfigi-cusip-v2026.03/

canon registry diff \
  --old registries/openfigi-cusip-v2026.02/ \
  --new registries/openfigi-cusip-v2026.03/ \
  --emit summary
```

Audit a seed corpus while maintaining a registry:

```bash
canon registry audit seeds.csv \
  --registry registries/cusip-isin/ \
  --column cusip

canon registry audit seeds.csv \
  --registry registries/cusip-isin/ \
  --column cusip \
  --emit summary
```

Preflight a registry before production use:

```bash
canon registry lint registries/org/ --profile auto --emit summary
```

Materialize a registry from a provider-backed seed corpus:

```bash
OPENFIGI_API_KEY=xxx \
canon registry build \
  --source openfigi \
  --seed seeds.csv \
  --seed-column cusip \
  --output registries/openfigi-cusip/ \
  --version 2026.03.13
```

Resolve a frozen strategy script for a repeated schema shape:

```bash
canon strategy profile rows.csv --emit json > profile.json
```

The profile artifact includes sorted columns, primitive type labels, exact distinct counts, null/empty/missing/non-scalar counts, the raw input BLAKE3 hash, and a profile content hash. Its top-level `columns` array can be used directly as `--schema` for strategy lookup or registration.

Audit a frozen script against a deterministic fixture suite:

```bash
canon strategy audit \
  --schema profile.json \
  --script scripts/procurement_total.py \
  --suite suites/procurement_total.v1/ \
  --emit json > evidence/audit.json
```

The suite manifest is `manifest.json` with `suite_id`, optional `version`, optional `repeatability_runs`, and fixture entries containing `id`, `input`, `expected_stdout`, and optional `expected_exit_code`. Fixture input bytes are sent to the script on stdin. A passing audit artifact includes `passed: true`, `decision: "PROCEED"`, and `sealed: true`, so it can be used directly as the `--verify`, `--assess`, and `--airlock` proof artifact for `strategy register`.

```bash
canon strategy resolve \
  --registry registries/procurement-strategies/ \
  --schema profile.json \
  --skill skills/procurement/SKILL.md
```

EXACT means the registered schema columns, types, and cardinalities match. COMPATIBLE means the columns and types match but cardinalities differ. PARTIAL means the schema overlaps but is missing or changing fields, so an LLM rewrite should be escalated and registered only after verify, assess, and airlock pass.

Register a passing frozen script:

```bash
canon strategy register \
  --registry registries/procurement-strategies/ \
  --schema profile.json \
  --skill skills/procurement/SKILL.md \
  --script scripts/procurement_total.py \
  --script-id procurement_total.v1 \
  --language python \
  --verify evidence/verify.json \
  --assess evidence/assess.json \
  --airlock evidence/airlock.json \
  --next-version 2026.05.06
```

Review frozen-script registry changes before adoption:

```bash
canon strategy diff \
  --old registries/procurement-strategies-v2026.05.01/ \
  --new registries/procurement-strategies-v2026.05.06/ \
  --emit summary
```

Resolve counterparty aliases:

```bash
canon counterparties.csv --registry registries/counterparty-cmbs/ --column servicer_name \
  | jq '.summary'
```

Canonicalize JSONL from stdin:

```bash
cat events.jsonl | canon - --registry registries/entity/ --column entity_id
```

Handle refusals programmatically:

```bash
canon tape.csv --registry registries/cusip-isin/ --column cusip \
  | jq 'select(.outcome == "REFUSAL") | .refusal'
```

---

## Refusal Codes

Every refusal includes the error code, a concrete message, and a recovery path.

| Code | Meaning | Next Step |
|------|---------|-----------|
| `E_IO` | Can't read input or registry | Check paths and permissions |
| `E_ENCODING` | Unsupported text encoding | Convert/re-export as UTF-8 |
| `E_CSV_PARSE` | CSV parse failure | Re-export as standard CSV |
| `E_BAD_REGISTRY` | Registry format invalid | Fix `registry.json` or mapping files |
| `E_COLUMN_NOT_FOUND` | `--column` doesn't exist in input | Check column name |
| `E_PARSE` | Can't parse input or unrecognized extension | Use `.csv`, `.tsv`, `.jsonl`, or `.ndjson` |
| `E_EMPTY_INPUT` | No processable data | Check input file |
| `E_TOO_LARGE` | Exceeds `--max-rows` or `--max-bytes` | Increase limits or reduce input |
| `E_EMIT_FORMAT` | `--emit csv` with JSONL input | Use `--emit json` or provide CSV input |
| `E_COLUMN_EXISTS` | Canonical column name already in header | Choose a different `--canon-column` |
| `E_ORG_INPUT_CONTRACT` | Org input rows violate the strategy contract | Check required fields and side-field JSON |
| `E_ORG_BAD_STRATEGY` | Org strategy YAML is malformed or invalid | Fix the strategy file |
| `E_ORG_BAD_SUITE` | Evaluation suite missing or profile-mismatched | Check suite directory and strategy profile |
| `E_ORG_FIXTURE_INVALID` | Suite fixture references are inconsistent | Fix fixture row catalog or expected pairs |
| `E_ORG_VERSION_BUMP_REQUIRED` | Promotion requires an explicit next version | Pass `--next-version` |
| `E_ORG_STALE_REGISTRY` | Registry changed since the audited snapshot | Re-run org against the current registry |

---

## Troubleshooting

### "E_COLUMN_NOT_FOUND" but the column exists

Column names are matched exactly (byte-for-byte after ASCII-trim). Check for invisible characters, BOM artifacts, or case mismatches. The refusal message lists available columns.

### "E_BAD_REGISTRY" on a registry that looks fine

All `.json` files in the registry directory except `registry.json` and `_build.json` must be valid mapping files. Check for stray JSON files, malformed entries, or missing required fields (`input`, `canonical_id`, `canonical_type`, `rule_id`).

### Unresolved entries that should match

v0 matching is exact byte match after ASCII-trim only. No case normalization, no punctuation stripping. Check that the registry contains the exact variant present in your input. Use `jq` to inspect unresolved entries:

```bash
canon tape.csv --registry registries/cusip-isin/ --column cusip \
  | jq '.unresolved[] | .input'
```

### Large registries are slow on first use

`canon` builds a SQLite derived index (`_index.sqlite`) on first use. Subsequent runs use the cached index. The build is logged to stderr.

---

## Organization Identity Resolution (`canon org`)

The same entity appears as "Wells Fargo & Company" in one document, "Wells Fargo Bank, N.A." in another, and "WFB" in a third. Three names, one issuer. `canon org` resolves these via a deterministic multi-stage pipeline — no ML models, no probabilistic matching, no black boxes.

The pipeline is YAML-driven: a **strategy file** defines which fields to observe, how to normalize names, which blocking operators generate candidates, how to score evidence, and what thresholds the solver uses to merge or abstain. Same strategy + same input + same registry = same output, every time.

```bash
# Full pipeline in one command:
$ canon org run rows.csv \
    --strategy strategy.yaml \
    --registry registries/org/ \
    --suite eval/holdout/ \
    --emit summary

org_run: 847 rows → 312 canonical entities, 4 escrow (pending), 0 escrow (conflict)
audit: holdout 98/98 pass, perturbation stability 0.998
```

Or run stages individually for inspection:

```bash
$ canon org block rows.csv --strategy strategy.yaml --registry registries/org/ > blocks.jsonl
$ canon org edge rows.csv --strategy strategy.yaml --candidates blocks.jsonl --registry registries/org/ > edges.jsonl
$ canon org solve rows.csv --strategy strategy.yaml --edges edges.jsonl --registry registries/org/ > result.json
$ canon org audit result.json --suite eval/holdout/
$ canon org promote result.json --audit audit.json --registry registries/org/ --next-version 2.1.0
$ canon org explain result.json --canon-id IC-00042
```

---

## The Org Pipeline

### Strategy

A YAML file that configures the entire pipeline. Defines observation fields (`name_fields`, `anchor_fields`, `context_fields`), normalization views (lowercase, strip legal suffixes, extract initials), blocking operators, evidence rules, solver thresholds, reconciliation policy, and promotion gates.

### Block

Candidate neighborhood generation. Blocking operators reduce the O(n²) comparison space to plausible pairs:

| Operator | What it does |
|----------|-------------|
| `exact_view` | Blocks on exact match of a normalized name view |
| `rare_token_overlap` | Blocks on shared rare tokens weighted by IDF |
| `shared_anchor` | Blocks on shared anchor values (LEI, CIK, FIGI) |
| `registry_alias_match` | Blocks on existing registry alias matches |

### Edge

Typed evidence scoring. Each candidate pair receives evidence edges:

- **Must-link** — strong deterministic evidence (shared trusted anchor, registry alias match)
- **Support** — scored positive evidence (exact name view match, acronym-plus-token, categorical field equality)
- **Cannot-link** — negative evidence (conflicting anchor values in the same namespace)

### Solve

Staged deterministic solver:

1. **Seed** — build initial components from must-link edges using union-find
2. **Backbone** — merge clusters via reciprocal best scoring pairs (requires positive name evidence, respects max cluster diameter)
3. **Attachment** — attach singletons to backbone clusters (requires winner margin, attachments don't chain)

**Reconciliation** then classifies each cluster:

- Single incumbent overlap → inherit existing canonical ID
- Multiple incumbent overlap → abstain with conflict escrow
- No incumbent → mint new canonical ID
- Low evidence → abstain with pending escrow

### Audit

Validate results against frozen evaluation suites. Checks holdout fixture pass rates and perturbation stability (strategy-configurable threshold, e.g. ≥ 0.995). Promotion requires a passing audit.

### Promote

Write audited results back to the registry:

- Resolved entities get alias entries added to registry mapping files
- Escrow sidecars are written for entities that need human review
- Requires an explicit `--next-version` bump

### Explain

Proof traces for any row, entity, or escrow decision:

```bash
$ canon org explain result.json --row src-row-42
$ canon org explain result.json --canon-id IC-00042
$ canon org explain result.json --escrow-id ESC-00007
```

Returns the full evidence chain: which blocking operator surfaced the pair, which evidence edges were scored, which solver stage produced the merge or abstention, and why.

---

## Limitations

| Limitation | Detail |
|------------|--------|
| **Exact match only (core lookup)** | Core `canon` lookup uses exact byte match after ASCII-trim. `canon org` adds multi-field deterministic resolution but not fuzzy/phonetic matching. |
| **Flat registries** | No subdirectories in v0. All mapping files must be at the registry root. |
| **CSV-only for `--emit csv`** | JSONL input cannot use `--emit csv` mode. |

---

## FAQ

### Why "canon"?

Short for *canonical*. The tool produces canonical identifiers — one true ID for each entity, traceable to a versioned registry.

### Is this entity resolution?

Yes — as of v0.3.0, `canon org` performs deterministic multi-field org-identity resolution. It resolves entities that appear under different names across documents using a YAML-driven pipeline of blocking, evidence scoring, and cluster solving. Core `canon` (without `org`) still resolves identifiers via exact lookup against versioned registries.

### How does canon relate to rvl?

[`rvl`](https://github.com/cmdrvl/rvl) explains numeric changes between CSV files. `canon` normalizes identifiers so `rvl` can align rows that use different ID schemes. The pipeline: `canon --emit csv | rvl`.

### How does canon relate to shape?

[`shape`](https://github.com/cmdrvl/shape) checks structural compatibility between files. `canon` resolves identifiers within a single file. Use `shape` to verify structure, `canon` to normalize IDs, then `rvl` to explain changes.

### What about registries — do I have to build them?

You can author registries by hand, consume published registries, or materialize them with `canon registry build`. The build workflow snapshots provider-backed lookups into a normal versioned registry directory plus `_build.json` provenance, and normal `canon` resolution ignores that metadata sidecar.

### Can I use this in CI/CD?

Yes. Exit codes (0/1/2) and JSON output are designed for automation. Gate on exit code, or parse the JSON for richer assertions.

---

<details>
<summary><strong>JSON Output Reference</strong></summary>

A single JSON object on stdout. This is the default output and the format used for `--map-out` in CSV mode.

```jsonc
{
  "version": "canon.v0",
  "outcome": "PARTIAL",                   // "RESOLVED" | "PARTIAL" | "UNRESOLVED" | "REFUSAL"
  "registry": {
    "id": "cusip-isin",
    "version": "3.2.1",
    "source": "registries/cusip-isin/"     // path as provided via --registry
  },
  "summary": {
    "total": 4183,                         // unique input values processed
    "resolved": 4150,
    "unresolved": 33
  },
  "mappings": [                            // one per resolved unique input
    {
      "input": "u8:037833100",
      "canonical_id": "u8:AAPL",
      "canonical_type": "ticker",
      "rule_id": "CUSIP_TO_TICKER",
      "confidence": "deterministic"        // v0: always "deterministic"
    }
  ],
  "unresolved": [                          // one per unresolved unique input
    {
      "input": "u8:UNKNOWN123",            // null for special reasons (empty_value, null_value, etc.)
      "reason": "no matching rule"
    }
  ],
  "refusal": null                          // null unless REFUSAL
  // When REFUSAL:
  // "refusal": {
  //   "code": "E_COLUMN_NOT_FOUND",
  //   "message": "Column 'cusip' not found in input file",
  //   "detail": { "column": "cusip", "available_columns": [...] },
  //   "next_command": "canon ... --column security_id"
  // }
}
```

### Identifier Encoding (JSON)

Input values and canonical IDs in JSON use unambiguous encoding:
- `u8:<string>` — valid UTF-8 with no ASCII control bytes
- `hex:<hex-bytes>` — anything else

CSV output uses raw values (no encoding prefix).

### Invariant

`summary.total == summary.resolved + summary.unresolved`. Every unique input value is classified as exactly one of resolved or unresolved.

### Confidence Values

- `"deterministic"` — exact match in versioned registry, fully reproducible
- `"suggested"` — probabilistic match, not auto-accepted (v1)

### Unresolved Reasons

| Reason | Trigger |
|--------|---------|
| `"no matching rule"` | Non-empty value had no exact match |
| `"empty_value"` | Value was empty after ASCII-trim |
| `"missing_field"` | JSONL object missing the `--column` field |
| `"null_value"` | JSONL field was JSON `null` |
| `"non_scalar_value"` | JSONL field was an object or array |

Special reasons (`empty_value`, `null_value`, `missing_field`, `non_scalar_value`) produce at most one unresolved entry each, with `input: null`.

</details>

---

## Agent Integration

For the full toolchain guide, see the [Agent Operator Guide](https://github.com/cmdrvl/.github/blob/main/profile/AGENT_PROMPT.md). Run `canon --describe` for this tool's machine-readable contract.

---

## Spec

The full specification is `docs/PLAN_CANON.md`. This README covers everything needed to use the tool; the spec adds implementation details, edge-case definitions, and testing requirements.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
