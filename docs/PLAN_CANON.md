# canon — Canonical Entity Resolution

## One-line promise
**Resolve messy identifiers to canonical IDs using versioned registries. Know what matched, what didn't, and why.**

If it can't resolve, say so clearly. If the match is probabilistic, mark it as a suggestion — never auto-accept.

---

## Problem (clearly understood)
Structured data in finance is full of identifier chaos:
- the same entity has 5 names across 3 vendors
- CUSIPs map to ISINs map to tickers — but which mapping version?
- counterparty names drift: "Wells Fargo" vs "Wells Fargo Bank, N.A." vs "WFB"
- property addresses have dozens of variants for the same building

Today this means:
- ad-hoc VLOOKUP chains
- unmaintained Python scripts with hardcoded aliases
- "just ask Dave, he knows the mappings"
- zero reproducibility — the same input resolves differently next month

`canon` replaces that with **one deterministic command** backed by versioned, inspectable registries.

---

## Non-goals (explicit)
`canon` is NOT:
- a fuzzy matcher (suggestions may be probabilistic, but accepted mappings are deterministic)
- a master data management system
- a record linker (it resolves IDs, not entity clusters)
- an address parser or geocoder
- a data cleansing tool
- a replacement for a proper MDM platform at scale

It does not build or maintain registries.
It resolves input values against them and records everything.

---

## CLI (v0)
```bash
canon <INPUT> --registry <REGISTRY> --column <COLUMN> [--emit json|csv] [--canon-column <NAME>] [--map-out <PATH>] [--max-rows <N>] [--max-bytes <N>]
```

Arguments:
- `<INPUT>`: CSV or JSONL file with IDs to canonicalize. Format is detected by file extension: `.csv` and `.tsv` are parsed as CSV; `.jsonl` and `.ndjson` are parsed as JSONL. Files with unrecognized or missing extensions are REFUSAL (`E_PARSE`). Use `-` for stdin (JSONL only; CSV requires seekable input for delimiter detection).

Options:
- `--registry <PATH>`: Registry directory (versioned). Required.
- `--column <COLUMN>`: Column containing IDs to resolve (CSV column name or JSONL field name). Required. Uses the same identifier encoding as `rvl` (`u8:<...>` or `hex:<...>` for ambiguous names).
- `--emit <json|csv>`: What goes to stdout. Default: `json` (mapping artifact). `csv` emits the original file with a canonical ID column appended — making `canon` a pipeline stage, not just an artifact producer. CSV input only; `--emit csv` with JSONL input is a REFUSAL (`E_EMIT_FORMAT`).
- `--canon-column <NAME>`: Name of the appended canonical ID column. Default: `<COLUMN>__canon` (e.g., `cusip__canon`). Only meaningful with `--emit csv`; ignored otherwise.
- `--map-out <PATH>`: Write the JSON mapping artifact to this file. Only meaningful with `--emit csv` — provides the mapping sidecar for `pack` or audit. Without it, `--emit csv` produces no JSON artifact (the witness ledger still records the run). Ignored with a stderr warning in `--emit json` mode (the mapping already IS stdout).
- `--max-rows <N>`: Refuse if input exceeds N data rows (raw row count including duplicates, excluding header). This is an I/O budget, not a cardinality limit — contrast with `summary.total` which counts unique values.
- `--max-bytes <N>`: Refuse if input file exceeds N bytes. For regular files, checked via file size before reading. For stdin (`-`), bytes are counted during streaming; refusal triggers as soon as the limit is exceeded (partial output may have been buffered — in JSON mode this is safe since output is emitted at end; stdin is JSONL-only so CSV mode is not affected).
- `--version`: Print `canon <semver>` and exit 0.
- `--describe`: Emit `operator.json` as JSON to stdout and exit 0. This is the spine's standard tool identity record (tool name, version, accepted inputs, output schema, refusal codes) — used by orchestrators and `pack` to introspect tools without running them.
- `--schema`: Print JSON Schema for the mapping artifact (`canon.v0` object) to stdout and exit 0. This is the schema for `--emit json` output and `--map-out` sidecar, not a description of CSV output format.
- `--no-witness`: Suppress witness ledger append.

### Output modes

| `--emit` | stdout | Mapping artifact | Use case |
|----------|--------|------------------|----------|
| `json` (default) | JSON mapping object | IS stdout | Audit, pack, inspection |
| `csv` | Canonicalized CSV with `<col>__canon` appended | Written to `--map-out` if specified | Pipeline stage — feed directly to `rvl`, `verify`, etc. |

In `json` mode, `canon` is an artifact tool (always structured JSON on stdout). In `csv` mode, `canon` becomes a pipeline stage (file in, file out) with the mapping artifact as an optional sidecar.

Exit codes (resolution-like)
- `0`: RESOLVED (all inputs mapped to canonical IDs)
- `1`: PARTIAL or UNRESOLVED (some or all inputs unresolved)
- `2`: REFUSAL / error

Exit codes are the same in both emit modes. In `csv` mode, a PARTIAL result (exit 1) still writes the CSV — unresolved rows have an empty canonical column. The exit code tells you whether to trust it blindly or inspect.

Streams
- `--emit json`: single JSON object to stdout (including refusals — the `"outcome": "REFUSAL"` object IS the stdout output, same pattern as `rvl --json`).
- `--emit csv`: canonicalized CSV to stdout. On refusal (exit 2), no CSV is written to stdout; the refusal JSON object goes to stderr instead.
- stderr is always reserved for process-level warnings, witness failures, and (in csv mode only) refusals.

---

## Outcomes (exactly one)
1) RESOLVED
- every input value mapped to a canonical ID
- all mappings are deterministic with registry version recorded
- `summary.resolved > 0 && summary.unresolved == 0`

2) PARTIAL
- at least one input resolved AND at least one unresolved
- unresolved entries listed with reason
- resolved mappings are still valid — partial is not a failure, it's an honest report
- `summary.resolved > 0 && summary.unresolved > 0`

3) UNRESOLVED
- zero inputs could be mapped
- `summary.resolved == 0 && summary.unresolved > 0`
- this is distinct from REFUSAL — the tool operated correctly, it just found no matches in the registry

4) REFUSAL
- cannot operate (bad input, bad registry, missing column, etc.)

No other outcomes.

---

## Definitions (v0)
- **Input value**: the raw cell content in the specified column, after ASCII-trim
- **Canonical ID**: the resolved identifier from the registry
- **Canonical type**: the type/namespace of the canonical ID (e.g., `ticker`, `isin`, `counterparty_id`, `property_id`)
- **Rule ID**: the specific mapping rule that produced the match (e.g., `CUSIP_TO_TICKER`)
- **Registry**: a versioned directory of lookup data (see Registry Format below)
- **Deterministic match**: exact lookup in a versioned registry — same input + same registry version = same output, every time
- **Suggested match**: probabilistic or fuzzy match — flagged as `"confidence": "suggested"` and **not accepted** until explicitly persisted to the registry by a human
- **Unresolved entry**: an input value that could not be mapped to a single canonical ID

---

## Input Contract

### CSV input
- Same byte-oriented CSV rules as `rvl`: header required, UTF-8 BOM stripped, delimiter auto-detected (same algorithm)
- `--column` specifies the column containing IDs to resolve
- Column must exist in the input (else REFUSAL `E_COLUMN_NOT_FOUND`)
- All rows are processed; blank records (all fields empty after ASCII-trim) are skipped
- Input values are ASCII-trimmed before lookup. If a value is empty after trim (but the row is not a blank record), it is classified as unresolved with reason `"empty_value"` without performing a registry lookup — an empty string is never a valid identifier. This is distinct from `"no matching rule"` which means a non-empty value had no registry entry.

### JSONL input
- One JSON object per line
- `--column` specifies the field name containing IDs to resolve
- Field must exist in each object (missing field on a line => that line is unresolved with reason `"missing_field"`)
- Input values are string-coerced and ASCII-trimmed before lookup. JSON `null` is treated as a missing value (unresolved with reason `"null_value"`), not coerced to the string `"null"`. Objects and arrays are unresolved with reason `"non_scalar_value"` (IDs are scalars; structured values aren't coercible to a lookup key). Numbers and booleans are coerced to their JSON string representation (e.g., `42` → `"42"`, `true` → `"true"`).

### Identifier encoding
- Input values and canonical IDs in JSON output use the same encoding as `rvl`:
  - `u8:<utf8-string>` if valid UTF-8 with no ASCII control bytes
  - `hex:<lowercase-hex-bytes>` otherwise

---

## Registry Format

A registry is a versioned directory of JSON mapping files.

```
registries/cusip-isin/
+-- registry.json            # Metadata: id, version, description, updated
+-- cusip-to-isin.json       # Mapping file: array of { input, canonical_id, canonical_type, rule_id }
+-- cusip-to-ticker.json     # Mapping file
```

### `registry.json` schema
```json
{
  "id": "cusip-isin",
  "version": "3.2.1",
  "description": "CUSIP to ISIN and ticker mappings",
  "updated": "2026-01-15",
  "entry_count": 48291
}
```

### Mapping file discovery
- All `*.json` files in the registry directory except `registry.json` are treated as mapping files
- Subdirectories are ignored (flat structure only in v0)
- Non-JSON files (e.g., `.md`, `.txt`) are ignored
- If a discovered `.json` file is not a valid mapping file (wrong schema, malformed JSON) → REFUSAL `E_BAD_REGISTRY`
- Files are evaluated in filename-sorted (lexicographic) order for match precedence

### Mapping file schema (each entry)
```json
{
  "input": "037833100",
  "canonical_id": "AAPL",
  "canonical_type": "ticker",
  "rule_id": "CUSIP_TO_TICKER"
}
```

### Registry types

Registries vary in complexity. `canon` treats all registries uniformly — input values in, canonical IDs out, unresolved entries flagged — but internal structure differs by domain:

| Registry type | Matching | v0? | Example |
|---------------|----------|-----|---------|
| **ID mapping** | Exact lookup (input ID -> canonical ID) | Yes | CUSIP->ISIN, ticker normalization |
| **Alias resolution** | Exact lookup with pre-populated variants | Yes | "Wells Fargo" / "WFB" -> counterparty C-00012 (each variant is a separate registry entry) |
| **Entity resolution** | Multi-column matching (address + name + coordinates -> canonical ID) | **v1** | Property address variants -> canonical property P-00456 |

ID mapping and alias resolution both use the same v0 mechanism: exact byte match after ASCII-trim. The difference is how the registry is authored (one entry per ID vs many entries per entity). Entity resolution requires multi-column matching and is deferred to v1.

### Registry governance

- A small set of standard registries (CUSIP->ISIN, ticker normalization, LEI) are distributed alongside the tool as files in a `registries/` directory — the user still references them via `--registry <PATH>` like any other registry. No special "built-in" resolution; they're just pre-authored registries that ship with the release.
- CMD+RVL publishes and sells official, industry-relevant registries (sector classifications, ABS deal mappings, servicer ID normalization) as a commercial layer on top of the open-source tool

### Versioning

- Registries are versioned at the directory level (`registry.json` carries the version)
- Follows semver: any entry addition, modification, or removal bumps the version
- The `registry.json` version is recorded in `canon` output for reproducibility
- Directories are inspectable, diffable, and versionable in git
- `entry_count` in `registry.json` is advisory (for display/logging) — `canon` does not refuse on mismatch with actual entry count, but logs a warning to stderr if they differ

---

## Lookup Behavior

### Resolution order
1. Load registry from `--registry` path
2. Validate registry format (refuse on malformed registry)
3. Parse input file, extract `--column` values
4. For each unique input value (after ASCII-trim and special-case classification):
   - If value is empty string → unresolved with reason `"empty_value"` (no lookup)
   - If value originated from JSONL `null` → unresolved with reason `"null_value"` (no lookup)
   - If value originated from a missing JSONL field → unresolved with reason `"missing_field"` (no lookup)
   - If value originated from a JSONL object or array → unresolved with reason `"non_scalar_value"` (no lookup)
   - Otherwise: look up in registry via exact byte match against `input` fields
   - If match: record mapping (input, canonical_id, canonical_type, rule_id, confidence)
   - If no match: record as unresolved with reason `"no matching rule"`

### Match precedence
- Exact match (byte-for-byte after ASCII-trim) takes priority
- Within a registry, mapping files are evaluated in filename-sorted order
- First match wins (no ambiguity — if two rules could match, the first one by file order + entry order is used)

### Normalization and alias matching (v0)
Alias resolution in v0 is **not fuzzy** — it is exact lookup against pre-normalized registry entries. The registry author is responsible for including all known variants as separate `input` entries:

```json
{"input": "Wells Fargo", "canonical_id": "C-00012", ...}
{"input": "Wells Fargo Bank, N.A.", "canonical_id": "C-00012", ...}
{"input": "WFB", "canonical_id": "C-00012", ...}
```

`canon` does not normalize input values beyond ASCII-trim. No uppercasing, no punctuation stripping, no stemming. This keeps the matching fully deterministic and transparent — the registry is the complete source of truth for what matches.

If a registry needs case-insensitive matching, it must include all case variants as entries (or a future `canon` version may support a per-registry `match_mode` field in `registry.json` — see v1 ideas).

> **Rationale:** Implicit normalization rules are a common source of subtle bugs in entity resolution systems. By keeping v0 matching purely exact (post-ASCII-trim), every resolution is directly traceable to a specific registry entry. The registry is auditable; the matching is trivial.

### Suggestions vs accepted mappings
- Suggestions may be probabilistic (e.g., fuzzy name matching), but **accepted mappings are deterministic, persisted, and versioned**
- v0 only supports deterministic matching (exact lookup + alias resolution)
- Probabilistic suggestions (via `canon suggest` mode) are deferred to v1

### Duplicate input values
- Input values are deduplicated before lookup — each unique value is resolved once
- `mappings[]` and `unresolved[]` contain one entry per **unique input value**, not one per row
- `summary.total` counts unique input values (after ASCII-trim), not row count
- Row count is not tracked in output (the tool maps values, not rows)
- This keeps output size proportional to cardinality, not file length (500 unique CUSIPs = 500 mapping entries, regardless of whether the file has 500 or 500k rows)

### Unresolved entries and hypotheses
- When `canon` cannot resolve an input to a single canonical ID, it emits the entry as `unresolved` with the reason
- Downstream systems (e.g., the data factory's decode policy) may hold unresolved entries as provisional hypotheses rather than treating them as terminal failures

---

## Output (JSON: `--emit json`, default)

Single JSON object on stdout. This is the default output mode and the format used for `--map-out` in CSV mode.

### Schema (`canon.v0`)
```json
{
  "version": "canon.v0",
  "outcome": "PARTIAL",
  "registry": {
    "id": "cusip-isin",
    "version": "3.2.1",
    "source": "registries/cusip-isin/"
  },
  "summary": {
    "total": 4183,
    "resolved": 4150,
    "unresolved": 33
  },
  "mappings": [
    {
      "input": "u8:037833100",
      "canonical_id": "u8:AAPL",
      "canonical_type": "ticker",
      "rule_id": "CUSIP_TO_TICKER",
      "confidence": "deterministic"
    }
  ],
  "unresolved": [
    {
      "input": "u8:UNKNOWN123",
      "reason": "no matching rule"
    }
  ],
  "refusal": null
}
```

### Field definitions

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Always `"canon.v0"` |
| `outcome` | string | `"RESOLVED"`, `"PARTIAL"`, `"UNRESOLVED"`, or `"REFUSAL"` |
| `registry.id` | string | Registry identifier |
| `registry.version` | string | Registry semver |
| `registry.source` | string | Path to registry directory (as provided via `--registry`; may be relative or absolute — consumers should not assume filesystem semantics) |
| `summary.total` | integer | Count of unique entries processed: unique normal input values (after ASCII-trim, excluding blank records) plus one per distinct special reason that fired (see Dedup rules below) |
| `summary.resolved` | integer | Count of successfully mapped entries |
| `summary.unresolved` | integer | Count of entries that could not be mapped |
| `mappings[]` | array | One entry per resolved input |
| `mappings[].input` | string | Original input value (identifier-encoded) |
| `mappings[].canonical_id` | string | Resolved canonical ID (identifier-encoded) |
| `mappings[].canonical_type` | string | Type/namespace of the canonical ID |
| `mappings[].rule_id` | string | Which mapping rule produced this match |
| `mappings[].confidence` | string | `"deterministic"` or `"suggested"` (v0: always deterministic) |
| `unresolved[]` | array | One entry per unresolved input |
| `unresolved[].input` | string\|null | Original input value (identifier-encoded), or `null` for special reasons (`"empty_value"`, `"null_value"`, `"missing_field"`, `"non_scalar_value"`) |
| `unresolved[].reason` | string | Why resolution failed (see reason values below) |
| `refusal` | object/null | Refusal envelope (null unless REFUSAL) |

**Invariant:** `summary.total == summary.resolved + summary.unresolved`. Every unique input value is classified as exactly one of resolved or unresolved — there is no third bucket.

### Confidence values
- `"deterministic"` — exact match in versioned registry, fully reproducible
- `"suggested"` — probabilistic match, **not accepted** until explicitly persisted to registry (v1)

### Unresolved reason values (v0)

| Reason | Trigger |
|--------|---------|
| `"no matching rule"` | Non-empty input value had no exact match in the registry |
| `"empty_value"` | Input value was empty after ASCII-trim (CSV: non-blank row with blank column; JSONL: empty string field) |
| `"missing_field"` | JSONL object did not contain the `--column` field |
| `"null_value"` | JSONL field value was JSON `null` |
| `"non_scalar_value"` | JSONL field value was an object or array (not coercible to a string ID) |

**Dedup rules for special reasons:** `"empty_value"`, `"null_value"`, `"missing_field"`, and `"non_scalar_value"` each produce at most one unresolved entry regardless of how many input rows triggered them (same dedup principle as regular values). For these entries, `unresolved[].input` is `null` (JSON null, not the string `"null"`) since there is no meaningful input value to report. Each distinct reason that fires contributes 1 to `summary.total` and 1 to `summary.unresolved`.

### Note on output size
The `mappings` and `unresolved` arrays contain one entry per **unique** input value (see Duplicate Input Values above). For registries with high cardinality (e.g., 50k unique CUSIPs), the output JSON can still be large. `canon` emits the complete mapping for pack/audit integrity — agents processing the output should use streaming JSON parsers. The `summary` object provides aggregate counts without reading the full arrays.

---

## Output (CSV: `--emit csv`)

When `--emit csv` is specified, stdout is the original CSV with one column appended: the resolved canonical ID.

### Behavior
- Every row from the input CSV is preserved exactly (same delimiter, same quoting, same field values)
- A new column is appended at the end of each row (header + data)
- Header gets the canonical column name (`<COLUMN>__canon` or `--canon-column` value)
- Resolved rows: the raw `canonical_id` value from the registry match — no identifier encoding prefix (i.e., `AAPL`, not `u8:AAPL`). Identifier encoding (`u8:`/`hex:`) is a JSON output concern only. `canonical_type` and `rule_id` are in the JSON mapping artifact only.
- Unresolved rows: empty string in the new column
- Blank records: passed through unchanged (canonical column is empty). Note: blank rows appear in CSV output but are excluded from the JSON mapping artifact's `summary.total`, `mappings[]`, and `unresolved[]` — the CSV preserves row structure while the JSON counts unique processable values.
- Delimiter matches the input file's detected delimiter
- Quoting follows the input file's detected escape mode

### Example
```
$ cat tape.csv
cusip,balance,rate
037833100,1000000,3.5
594918104,500000,4.2
UNKNOWN99,250000,2.8

$ canon tape.csv --registry registries/cusip-isin/ --column cusip --emit csv
cusip,balance,rate,cusip__canon
037833100,1000000,3.5,AAPL
594918104,500000,4.2,MSFT
UNKNOWN99,250000,2.8,

$ echo $?
1  # PARTIAL — one unresolved row
```

### Pipeline: canon -> rvl (the real workflow)
```bash
# Canonicalize, then compare by canonical ID — no manual join
canon nov.csv --registry registries/cusip-isin/ --column cusip --emit csv > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip --emit csv > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon

# With mapping artifacts for audit
canon nov.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/nov.map.json > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/dec.map.json > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon --json > evidence/rvl.json
pack seal evidence/ --note "Q4 recon with canonical IDs"
```

### Refusals in CSV mode
- If `canon` refuses (exit 2), no CSV is written to stdout
- The refusal JSON object is written to stderr (same envelope as JSON mode)
- `--map-out` file is not created on refusal

### Constraints
- `--emit csv` requires CSV input (not JSONL) — REFUSAL `E_EMIT_FORMAT` otherwise
- If the canonical column name (whether default `<COLUMN>__canon` or explicit `--canon-column`) already exists in the input header — REFUSAL `E_COLUMN_EXISTS`
- The canonical column is always the last column (no column reordering)

---

## Refusal Codes (v0)

| Code | Trigger | Next step |
|------|---------|-----------|
| `E_IO` | Can't read input or registry | Check paths |
| `E_ENCODING` | Unsupported text encoding | Convert/re-export as UTF-8 |
| `E_CSV_PARSE` | CSV parse failure | Re-export as standard CSV |
| `E_BAD_REGISTRY` | Registry format invalid (missing `registry.json`, malformed entries) | Fix registry |
| `E_COLUMN_NOT_FOUND` | `--column` doesn't exist in input | Check column name |
| `E_PARSE` | Can't parse JSONL input or unrecognized/missing file extension | Check format; use `.csv`, `.tsv`, `.jsonl`, or `.ndjson` extension |
| `E_EMPTY_INPUT` | Input has no processable data (header only, empty JSONL, or all rows are blank records) | Check input file |
| `E_TOO_LARGE` | Input exceeds `--max-rows` or `--max-bytes` | Increase limits or reduce input |
| `E_EMIT_FORMAT` | `--emit csv` used with JSONL input | Use `--emit json` or provide CSV input |
| `E_COLUMN_EXISTS` | `--emit csv` and canonical column name already exists in input header | Choose a different `--canon-column` name |

### Refusal output contract
Every REFUSAL prints a single JSON object with the shared refusal envelope:
```json
{
  "version": "canon.v0",
  "outcome": "REFUSAL",
  "registry": null,
  "summary": null,
  "mappings": [],
  "unresolved": [],
  "refusal": {
    "code": "E_COLUMN_NOT_FOUND",
    "message": "Column 'cusip' not found in input file",
    "detail": {
      "column": "cusip",
      "available_columns": ["security_id", "isin", "name"]
    },
    "next_command": "canon positions.csv --registry registries/cusip-isin/ --column security_id"
  }
}
```

Refusals are operator handoffs, not dead ends. Every refusal includes either a `next_command` (mechanical recovery) or explicit escalation guidance.

---

## Pipeline Composition

### The core workflow: canonicalize then compare

`--emit csv` is how `canon` plugs into the spine. Canonicalize both sides, then run `rvl` on the canonical files:

```bash
# Monthly loan tape reconciliation with canonical IDs
canon nov.csv --registry registries/cusip-isin/ --column cusip --emit csv > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip --emit csv > dec.canon.csv
rvl nov.canon.csv dec.canon.csv --key cusip__canon
```

Three commands. No VLOOKUP. No manual join. The canonical column is right there in the file.

### Audit-grade pipeline (with evidence)

```bash
# Canonicalize with mapping artifacts preserved
canon nov.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/nov.map.json > nov.canon.csv
canon dec.csv --registry registries/cusip-isin/ --column cusip \
  --emit csv --map-out evidence/dec.map.json > dec.canon.csv

# Compare on canonical IDs
rvl nov.canon.csv dec.canon.csv --key cusip__canon --json > evidence/rvl.json

# Seal everything as evidence
pack seal evidence/ --note "Nov->Dec recon with canonical CUSIPs"
```

### Inspection and debugging (JSON mode)

```bash
# What resolved and what didn't?
canon tape.csv --registry registries/cusip-isin/ --column security_id \
  | jq '.unresolved[]'

# Resolve entity aliases (inspect the mapping)
canon counterparties.csv --registry registries/counterparty-cmbs/ --column servicer_name \
  | jq '.summary'

# Canonicalize a JSONL file (stdin via -)
cat events.jsonl | canon - --registry registries/entity/ --column entity_id
```

---

## Rust Implementation Sketch

### Core crates
- `clap` for CLI (derive)
- `csv` for CSV parsing (streaming, same engine as `rvl`)
- `serde` + `serde_json` for JSON I/O
- `rusqlite` (bundled) for derived registry index
- `blake3` for hashing (witness protocol)

### Registry loading
- Source of truth: directory of JSON mapping files (git-friendly, inspectable, diffable)
- Derived index: SQLite database built automatically on first use, stored inside the registry directory as `_index.sqlite` (`.gitignore`d, rebuilt when stale). Requires write permission to the registry directory on first use or when stale.
- Staleness check: compare `registry.json` version + file modification times against SQLite metadata
- Rebuild is automatic and logged to stderr (no separate subcommand needed for v0)
- This gives git-friendly source files AND sub-millisecond queries against 389K+ entries

### Lookup implementation
- v0: exact-match query (ASCII-trimmed input against registry `input` field) via SQLite
- Alias resolution uses the same exact-match path — the registry author pre-populates all known variants as separate entries
- Entity resolution registries with multi-column matching: deferred to v1

### Core data types
```
Mapping { input, canonical_id, canonical_type, rule_id, confidence }
Unresolved { input, reason }
Registry { id, version, source, entries }
```

### Processing

**JSON mode (`--emit json`, default):**
- Parse input CSV/JSONL in streaming fashion, collecting unique input values into a `HashSet`
- Resolve each unique value once against the SQLite index
- Build `mappings` and `unresolved` lists in memory (bounded by unique value count, not row count)
- Emit single JSON object at end

**CSV mode (`--emit csv`):**
- Pass 1: stream input, collect unique column values into a `HashMap<input_value, canonical_id|None>`
- Resolve all unique values against the SQLite index (single batch query)
- Pass 2: stream input again, appending the canonical column to each row using the lookup map
- Memory: bounded by unique value count (the lookup map), not row count
- The CSV writer preserves the input's delimiter and quoting
- If `--map-out` is specified, the JSON mapping artifact is written after pass 2 completes

### Witness protocol
- Same pattern as `rvl`: hash inputs, hash output, append witness record
- `output_hash`: in JSON mode, hash the JSON mapping blob. In CSV mode, hash the CSV bytes as they stream through stdout (incremental BLAKE3 — the hasher sees the same bytes the pipe does).
- ~100-150 LOC in a `witness` module
- Never block on witness failure

---

## Testing Philosophy

Must-pass (v0)
- exact match resolves correctly with registry version recorded
- missing column in input => REFUSAL (`E_COLUMN_NOT_FOUND`)
- malformed registry => REFUSAL (`E_BAD_REGISTRY`)
- empty registry (valid format, zero entries) => all inputs unresolved, outcome UNRESOLVED, exit 1
- all inputs resolve => outcome RESOLVED, exit 0
- some inputs unresolved => outcome PARTIAL, exit 1
- zero inputs unresolved, all resolved => outcome RESOLVED, exit 0
- zero inputs resolved, all unresolved => outcome UNRESOLVED (not PARTIAL), exit 1
- input with blank records => blanks skipped, counts correct
- JSONL input with missing field => unresolved with `"missing_field"` reason
- JSONL input with `null` field value => unresolved with `"null_value"` reason (not string-coerced)
- CSV input with empty column value (non-blank row) => unresolved with `"empty_value"` reason
- JSONL input with object/array field value => unresolved with `"non_scalar_value"` reason
- file with no extension or unrecognized extension => REFUSAL (`E_PARSE`)
- registry version is recorded in output (reproducibility)
- same input + same registry version + same `--registry` path = byte-identical output (determinism). Note: `registry.source` in JSON output reflects the CLI argument verbatim, so different paths to the same registry produce different `registry.source` values — all other fields are path-independent.
- large input (100k+ rows) completes without OOM
- `--max-rows` / `--max-bytes` enforcement => REFUSAL (`E_TOO_LARGE`)
- identifier encoding: non-UTF-8 bytes in input values => `hex:` rendering
- alias resolution: variant names resolve to same canonical ID
- Unicode edge cases in input values handled without panic
- `--emit csv`: output has original columns + canonical column appended
- `--emit csv`: resolved rows have canonical ID, unresolved rows have empty canonical column
- `--emit csv`: delimiter and quoting match input file
- `--emit csv`: default canonical column name is `<column>__canon`
- `--emit csv`: `--canon-column` overrides the name
- `--emit csv` + JSONL input => REFUSAL (`E_EMIT_FORMAT`)
- `--emit csv` + canonical column name already in header => REFUSAL (`E_COLUMN_EXISTS`)
- `--emit csv` + `--map-out`: JSON mapping artifact matches what `--emit json` would produce
- `--emit csv`: PARTIAL exit code 1 but CSV is still fully written (unresolved rows are visible, not dropped)
- `--emit csv` + `--emit json` consistency: for the same input + registry, the CSV canonical column values correspond to the `mappings[].canonical_id` values in JSON output after stripping the identifier encoding prefix (CSV has `AAPL`, JSON has `u8:AAPL` — same value, different representation)

Never allow
- silent resolution failures (every unresolved entry must be reported)
- auto-accepting probabilistic matches as deterministic
- resolving against an unversioned registry
- different output for the same input + registry version

---

## Success Criteria (Real World)
- `canon --emit csv | rvl` replaces a VLOOKUP-then-eyeball workflow in under 60 seconds
- an analyst opens the `.canon.csv` in Excel and sees the canonical column right there — no join required
- registry diffs show exactly what changed between versions
- the mapping output is inspectable: every resolution traceable to a registry entry + rule ID
- "who is Wells Fargo in this dataset?" has one answer, with a rule ID
- someone deletes a VLOOKUP spreadsheet because `canon --emit csv` made it unnecessary

If any feature makes the mapping less inspectable or the pipeline less composable, cut it.

---

## v1 Ideas (Only If v0 Is Loved)

### `canon suggest` — probabilistic matching
- LLM-assisted or fuzzy-matching mode for unresolved entries
- Emits suggestions with `"confidence": "suggested"` — never auto-accepted
- Human reviews suggestions, accepts into registry, re-runs for deterministic resolution
- This is the onramp for new registries: run `canon suggest`, curate results, freeze into a registry

### Registry `match_mode` (normalized matching)
- Per-registry `match_mode` field in `registry.json` (e.g., `"exact"`, `"case_insensitive"`, `"normalized"`)
- Eliminates the need for registries to enumerate all case variants
- Normalization rules defined per mode, documented and deterministic
- `strsim` crate (Jaro-Winkler, Sorensen-Dice) for fuzzy candidate scoring in suggest mode

### Entity resolution registries
- Multi-column matching (address + name + coordinates -> canonical ID)
- Geospatial matching via `geo` + `rstar` (Haversine + R-tree)
- Phonetic blocking via `rphonetic` (Metaphone, Soundex)
- H3 hex blocking via `h3o` for property matching at scale (389K+ entries)
- Address normalization (US abbreviations: ST->STREET, AVE->AVENUE, etc.)

### ID validation
- CUSIP format validation + check digit computation via `cusip` crate
- ISIN format validation via `isin` crate
- Validate input IDs before lookup (refuse malformed IDs with a new `E_INVALID_ID` code)

### Fellegi-Sunter probabilistic matching (v1+)
- Comparison vectors -> log-likelihood ratios -> threshold
- Weights trained offline, applied deterministically at runtime
- ~500 LOC of Rust

### `libpostal` integration (v1+)
- Statistical address parser for complex address variants
- FFI binding (~2 GB data files)
- Only for entity resolution registries with address matching

### Registry push/pull
- `canon push` / `canon pull` for data-fabric integration
- Share registries across teams with version tracking

### Decision notes
**Entity registry format:** The v0 registry format (flat JSON mapping files) handles ID mapping and alias resolution cleanly. Entity resolution registries (e.g., `canon.property-cmbs` with 389K properties, address variants, lat/lng coordinates) are structurally richer. The hybrid approach (JSONL source + SQLite derived index) handles both — source files remain git-friendly, derived index provides query performance. Multi-column matching logic lives in the lookup implementation, not the file format.

---

Final rule: If you can't explain the mapping to someone staring at a spreadsheet, it doesn't ship.
