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

- **Versioned registries** — every resolution is pinned to a registry version with semver. When the registry updates, you know exactly what changed. Registries are plain JSON directories — inspectable in git, diffable, no database required.
- **Pipeline composable** — `canon --emit csv` appends a `<column>__canon` column to your CSV. Pipe the output directly into `rvl` or `shape`: `canon nov.csv --column cusip --emit csv | rvl - dec.canon.csv --key cusip__canon`.
- **Full traceability** — every mapping includes `rule_id`, `canonical_type`, and `confidence`. Every unresolved entry includes the reason. Every result is auditable.
- **Deduplication built in** — input values are deduplicated before lookup. 500 unique CUSIPs produce 500 mapping entries whether your file has 500 rows or 500,000.

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
└── cusip-to-ticker.json     # Mapping file
```

Each mapping file is an array of entries:

```json
{"input": "037833100", "canonical_id": "AAPL", "canonical_type": "ticker", "rule_id": "CUSIP_TO_TICKER"}
{"input": "Wells Fargo", "canonical_id": "C-00012", "canonical_type": "counterparty_id", "rule_id": "COUNTERPARTY_ALIAS"}
{"input": "WFB", "canonical_id": "C-00012", "canonical_type": "counterparty_id", "rule_id": "COUNTERPARTY_ALIAS"}
```

Registries are versioned with semver, inspectable in git, and diffable. A SQLite derived index is built automatically for fast lookups against large registries.

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
- Fuzzy entity matching (address variants, phonetic matching) — deferred to v1
- Master data management at enterprise scale
- Record linkage / entity clustering

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

```
canon <INPUT> --registry <REGISTRY> --column <COLUMN> [OPTIONS]
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

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | RESOLVED (all inputs mapped) |
| `1` | PARTIAL or UNRESOLVED (some or all inputs unresolved) |
| `2` | REFUSAL or CLI error |

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

---

## Troubleshooting

### "E_COLUMN_NOT_FOUND" but the column exists

Column names are matched exactly (byte-for-byte after ASCII-trim). Check for invisible characters, BOM artifacts, or case mismatches. The refusal message lists available columns.

### "E_BAD_REGISTRY" on a registry that looks fine

All `.json` files in the registry directory (except `registry.json`) must be valid mapping files. Check for stray JSON files, malformed entries, or missing required fields (`input`, `canonical_id`, `canonical_type`, `rule_id`).

### Unresolved entries that should match

v0 matching is exact byte match after ASCII-trim only. No case normalization, no punctuation stripping. Check that the registry contains the exact variant present in your input. Use `jq` to inspect unresolved entries:

```bash
canon tape.csv --registry registries/cusip-isin/ --column cusip \
  | jq '.unresolved[] | .input'
```

### Large registries are slow on first use

`canon` builds a SQLite derived index (`_index.sqlite`) on first use. Subsequent runs use the cached index. The build is logged to stderr.

---

## Limitations

| Limitation | Detail |
|------------|--------|
| **Exact match only** | No fuzzy, phonetic, or normalized matching in v0. Registry must contain all variants. |
| **Flat registries** | No subdirectories in v0. All mapping files must be at the registry root. |
| **No multi-column matching** | Entity resolution (address + name + coordinates) is deferred to v1. |
| **No suggestions** | Probabilistic/fuzzy suggestions (`canon suggest`) are deferred to v1. |
| **CSV-only for `--emit csv`** | JSONL input cannot use `--emit csv` mode. |

---

## FAQ

### Why "canon"?

Short for *canonical*. The tool produces canonical identifiers — one true ID for each entity, traceable to a versioned registry.

### Is this entity resolution?

Not in v0. `canon` v0 resolves identifiers and aliases via exact lookup. Multi-column entity resolution (property addresses, counterparty matching with fuzzy logic) is planned for v1.

### How does canon relate to rvl?

[`rvl`](https://github.com/cmdrvl/rvl) explains numeric changes between CSV files. `canon` normalizes identifiers so `rvl` can align rows that use different ID schemes. The pipeline: `canon --emit csv | rvl`.

### How does canon relate to shape?

[`shape`](https://github.com/cmdrvl/shape) checks structural compatibility between files. `canon` resolves identifiers within a single file. Use `shape` to verify structure, `canon` to normalize IDs, then `rvl` to explain changes.

### What about registries — do I have to build them?

A small set of standard registries ship with the tool. CMD+RVL also publishes official, industry-relevant registries (sector classifications, ABS deal mappings, servicer ID normalization) as a commercial layer.

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

## Spec

The full specification is `docs/PLAN_CANON.md`. This README covers everything needed to use the tool; the spec adds implementation details, edge-case definitions, and testing requirements.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
