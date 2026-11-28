# Lookup and registries in detail

The workflow map, the four outcomes, the registry patterns and the output modes,
in more detail than the [README](../README.md) gives. Moved here from the README.

## Identity Compiler Workflow Map

Start with exact lookup. Add build-time evidence only when the registry does not
yet know enough.

| Workflow | Purpose | Boundary |
|----------|---------|----------|
| `canon <INPUT> --registry <DIR> --column <COLUMN> [--scope DIMENSION=VALUE]...` | Replay accepted registry knowledge into JSON or CSV outputs. | Exact runtime lookup only. |
| `canon registry mint/add-entry/add-entries/default-id-scheme/next-id` | Maintain self-authored aliases and ID conventions. | Operator-accepted facts become flat mapping entries. |
| `canon registry build/providers/provider-schema` | Materialize provider-backed seed mappings into local registry files. | Provider calls happen during maintenance, never during runtime lookup. |
| `canon entity run` | Cluster profiled observations inside one corpus and propose registry knowledge. | Deterministic artifacts, audit, review inbox, promotion. |
| `canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json\|summary] [--cache-mode enabled\|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]` | Link two row sets through the same typed artifact path used by project mode. | Cross-source linkage; relation evidence is not an equivalence shortcut. Profile and work-dir are required for execution; omissions write nothing. Cache mode defaults to enabled. |
| `canon entity calibrate sweep <RESULT> --gold <GOLD.jsonl> --strategy <STRATEGY.yaml>` | Sweep integer threshold tuples against gold labels and emit a truth-space recommendation report. | Read-only workbench report; no registry writes, no strategy mutation, frozen `canon.entity.quality.v1` gates. |
| `canon entity calibrate em <EVIDENCE.jsonl> --strategy <STRATEGY.yaml>` | Estimate support-evidence weights from aggregated agreement patterns and emit proposed support-score YAML. | Read-only calibration assist; fixed-point EM over existing evidence only, no registry writes, no strategy mutation, no automatic authority. |
| `canon registry export --format dbt-seed|search-index|key-crosswalk|entity-card` | Project exact registry knowledge into transform or serving artifacts, the key-to-key crosswalk, or one bounded JSON context card per canonical ID. `dbt-seed` and `key-crosswalk` also support `--parquet`; CSV remains the default. | Deterministic downstream snapshots; no new matching semantics. |
| `canon temporal compile --request <REQUEST.json> [--emit json\|summary] [--no-witness]` | Compile the native `TemporalCompileRequest` at explicit `valid_at` and `known_as_of` cutoffs into `canon.temporal.compile.v1`. | Offline snapshot artifact with mapping proofs and conflict omissions; no registry writes or promotion. Exit 0 means compilation succeeded, including an empty or conflict-omitting snapshot. Malformed or invalid requests refuse with exit 2. |
| `canon temporal relation finalize --facts <FACTS.json> [--emit json\|summary] [--max-bytes <N>] [--no-witness]` | Finalize a JSON array of native `canon.identity.relation.v1` facts using Canon's relation interval, provenance, deduplication, and graph-constraint APIs. | Deterministic relation facts only; no registry writes, alias creation, or identity acceptance. |
| Package and project workflows | Move registries and strategies through reproducible deployment and project locks. | They package and check knowledge; they do not change exact lookup. |
| Extensions and adapters | Add profiles, source mappings, provider materializers, and domain policies out of tree. | Domain expertise stays outside Canon core defaults unless explicitly packaged and audited. |

`canon temporal relation finalize` applies Canon's native `finalize_relations`
implementation to the supplied facts. The input declares relation type, evidence
locator, validity interval, review disposition, and graph constraints; finalization
validates these fields and recomputes `relation_id` and `edge_key`, but does not
independently verify the cited source or accept identity truth. Relation edges never
create aliases or exact lookup mappings, even when review disposition is `same`;
equality requires a separate identity fact. The command writes no registry.

Relation sidecars supplied to `canon temporal compile` remain opaque, hash-bound
references and projection-only omissions. Compile does not load or validate
sidecar contents. Callers may retain finalized relation facts as a separate sidecar
and bind its path and digest; relation finalization does not attach it to a temporal
alias package.

The accretion loop is intentionally repetitive: unresolved or ambiguous evidence
enters a workbench, review accepts only defensible knowledge, the registry gets
a new version, and future production runs replay that version exactly.

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
{"input": "Alpha Entity LLC", "canonical_id": "ENT-00012", "canonical_type": "entity", "rule_id": "REVIEWED_ALIAS"}
{"input": "Alpha Entity", "canonical_id": "ENT-00012", "canonical_type": "entity", "rule_id": "REVIEWED_ALIAS"}
```

Registries are versioned with semver, inspectable in git, and diffable. A SQLite derived index is built automatically for fast lookups against large registries. `_build.json` is reserved for materializer provenance and is ignored during normal resolution.

### Two registry-creation patterns

Provider-fetched registries snapshot an external or bundled provider into normal mapping files. OpenFIGI is a corpus-scoped securities identifier materializer for CUSIP, ISIN, or SEDOL seeds; it is not an organization identity source, not CMBS-specific, and never participates in normal lookup after the registry files are written. This local example uses the built-in `mock` provider; real provider-backed runs use sources such as `openfigi` and may require provider configuration:

Provider-specific `--provider-config` semantics are discoverable through canon itself, not only through this prose. `canon registry providers --emit json` lists the available providers, and `canon registry provider-schema <PROVIDER> --emit json` emits the full option contract for one provider — keys, value types, enum values, secret flags, environment fallbacks, defaults, mutual exclusions, the interval encoding rule, and worked examples. Both surfaces are deterministic and offline (no provider call), and the same provider catalog is exposed under `providers` in `canon --describe`. Agents and skills should read the schema rather than hard-coding option lists:

The focused README examples in this section are covered by docs parity checks
where they use committed fixtures or temporary outputs. The generated command
corpus has separate exhaustive verification.

```bash
canon registry providers --emit json
canon registry provider-schema openfigi --emit json
```

```bash
example_dir=$(mktemp -d)
printf 'cusip\n037833100\n' > "$example_dir/seeds.csv"

canon registry build \
  --source mock \
  --seed "$example_dir/seeds.csv" \
  --seed-column cusip \
  --output "$example_dir/registries/mock-cusip" \
  --version 2026.03.13

canon "$example_dir/seeds.csv" \
  --registry "$example_dir/registries/mock-cusip" \
  --column cusip
```

Provider changes should be proven against a local twin before any live OpenFIGI maintenance run. OpenFIGI mapping filters may be passed as repeatable `--provider-config` values for corpus-wide disambiguation, such as `exchCode=US` for U.S. listed instruments. With `twinning` 0.5.1+ available, the OpenFIGI response-stub fixture exercises `canon registry build` without contacting `api.openfigi.com`:

```bash
twinning rest \
  --spec ../twinning/tests/fixtures/rest/openfigi_v2_v3/response-stub-schema.yaml \
  --server-variable basePath=v3 \
  --auth-mode shape \
  --run 'canon registry build --source openfigi --seed seeds.csv --seed-column cusip --provider-config id_type=ID_CUSIP --provider-config exchCode=US --provider-config api_key=stub-key --provider-config base_url="$TWIN_BASE_URL/v3/mapping" --output registries/openfigi-cusip/ --version 2026.06.09'
```

Self-authored registries are local operator conventions expressed as exact aliases. The maintenance commands keep `registry.json`, version bumps, and entry counts synchronized:

```bash
example_dir=$(mktemp -d)
mkdir -p "$example_dir/registries/people"

cat > "$example_dir/registries/people/registry.json" <<'JSON'
{
  "id": "people",
  "version": "0.1.0",
  "description": "Local people aliases",
  "updated": "2026-05-27",
  "entry_count": 0
}
JSON

printf '[]\n' > "$example_dir/registries/people/aliases.json"
printf 'name\nJane Doe\n' > "$example_dir/names.csv"

canon registry default-id-scheme \
  --registry "$example_dir/registries/people" \
  --prefix PPL \
  --zero-pad 3

canon registry mint \
  --registry "$example_dir/registries/people" \
  --canonical-type person \
  --with-alias 'aliases.json=Jane Doe:MANUAL'

canon "$example_dir/names.csv" \
  --registry "$example_dir/registries/people" \
  --column name
```

These workflows only create or update registry files. Normal `canon <INPUT> --registry ...` still resolves by exact byte match after ASCII-trim; scoped aliases require a matching repeatable `--scope DIMENSION=VALUE` lookup scope.

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

`--json` is shorthand for `--emit json`, so the spelling the other spine tools use
works here too. It cannot be combined with `--emit` (not even `--emit json`): clap
refuses with `the argument '--json' cannot be used with '--emit <EMIT>'` and exit 2,
so `--json --emit csv` can never silently pick one of the two.

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
