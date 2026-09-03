# canon

![canon: versioned identifier resolution. A painterly dashboard showing 32 email hashes resolving against a Canonical People Registry v3.2. Two of the 32 resolve to canonical person records; thirty are unresolved (structural, not error). The email-hash normalization rule appears as four checked steps: lowercase, trim, drop +suffix, sha256. The footnote reads: canon is operational, not a social-graph mirror.](docs/images/canon.webp)

> *Identity is a registry, not a guess. Zero matches is a finding, not a failure.*

<div align="center">

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**The same entity has five names across three vendors. canon makes them one.**

```bash
brew install cmdrvl/tap/canon
```

</div>

---

The same real-world thing appears as a CUSIP in one file, an ISIN in another,
a vendor label in a third, and a manually reviewed alias in a fourth. Your
pipeline needs those rows to replay as one canonical ID, but the evidence that
gets you there may be incomplete, contradictory, or worth human review.

**canon is an identity compiler: messy evidence -> reviewed versioned registry
-> exact replay.** The runtime lookup command is deliberately simple: it takes
one input value and resolves it by exact match against a local versioned
registry. The uncertain work happens before that, in workbenches that prepare
observations, build candidates, score evidence, abstain when needed, export
review queues/inboxes, and promote only accepted knowledge back into registry
files.

That boundary is the product. `canon` can help create registries, package them,
export them to dbt/search consumers, compare temporal snapshots, and run
project workflows. It does not ship industry ontology, provider knowledge, or a
probabilistic runtime lookup engine. Domain expertise lives in registries,
profiles, strategies, packages, or out-of-tree extensions that operators choose
and audit. See [`docs/IDENTITY_ARCHITECTURE.md`](docs/IDENTITY_ARCHITECTURE.md)
for the boundary.

### What makes this different

- **Exact replay from versioned registries** — every runtime resolution is pinned to a registry version. When the registry updates, `canon registry diff` tells you exactly what changed. Registries are plain JSON directories with derived indexes and provenance, inspectable in git and reproducible in CI.
- **Pipeline composable** — `canon --emit csv` appends a `<column>__canon` column to your CSV. Pipe the output directly into `rvl` or `shape`: `canon nov.csv --column cusip --emit csv | rvl - dec.canon.csv --key cusip__canon`.
- **Full traceability** — every mapping includes `rule_id`, `canonical_type`, and `confidence`. Every unresolved entry includes the reason. Every result is auditable.
- **Deduplication built in** — input values are deduplicated before lookup. 500 unique CUSIPs produce 500 mapping entries whether your file has 500 rows or 500,000.
- **Self-authored registries** — use `canon registry default-id-scheme`, `next-id`, `add-entry`, and `mint` to maintain local alias registries without hand-editing mapping JSON.
- **Evidence workbenches** — `canon entity` compiles profiled observations into registry proposals. Cluster mode finds same-entity groups within one corpus; link mode aligns a reference corpus to a target corpus through the same artifact path. Both keep relationship evidence separate from equivalence merge evidence.
- **Cross-source structural linkage** — `canon entity link` aligns two local row sets under an explicit YAML strategy and emits hash-bound `canon_entity_link.v1` decisions plus observation/surface bindings. Accepted knowledge enters registries through review, audit, promotion, and exact apply; direct `--write-back` currently refuses before mutation.
- **Distribution surfaces** — `canon registry export` preserves dbt seed and SQLite search-index consumers; package, project, and temporal workflows move the same registry knowledge through reproducible deployment and snapshot checks.

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

## Identity Compiler Workflow Map

Start with exact lookup. Add build-time evidence only when the registry does not
yet know enough.

| Workflow | Purpose | Boundary |
|----------|---------|----------|
| `canon <INPUT> --registry <DIR> --column <COLUMN>` | Replay accepted registry knowledge into JSON or CSV outputs. | Exact runtime lookup only. |
| `canon registry mint/add-entry/default-id-scheme/next-id` | Maintain self-authored aliases and ID conventions. | Operator-accepted facts become flat mapping entries. |
| `canon registry build/providers/provider-schema` | Materialize provider-backed seed mappings into local registry files. | Provider calls happen during maintenance, never during runtime lookup. |
| `canon entity run` | Cluster profiled observations inside one corpus and propose registry knowledge. | Deterministic artifacts, audit, review inbox, promotion. |
| `canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json\|summary] [--cache-mode enabled\|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]` | Link two row sets through the same typed artifact path used by project mode. | Cross-source linkage; relation evidence is not an equivalence shortcut. Profile and work-dir are required for execution; omissions write nothing. Cache mode defaults to enabled. |
| `canon entity alias-withholding --manifest <EXECUTION_ENVELOPE.json>` | Compile artifact-backed withheld-alias trials into a JSON or summary report. | Strict execution envelope only; outcomes are derived from referenced artifacts, not self-declared. |
| `canon entity generalization --manifest <STRICT_ENVELOPE.json>` | Compile artifact-backed entity-disjoint and time-forward trials into a JSON or summary report. | Strict execution envelope only; one public/private command, redacted identifiers and paths, outcomes and leakage checks derived from referenced artifacts rather than self-attested fields. Strict solve derivation binds typed edge artifacts, edge records, prepared surfaces, and `canon.evaluation.generalization.solve_policy.v0`. |
| `canon entity calibrate sweep <RESULT> --gold <GOLD.jsonl> --strategy <STRATEGY.yaml>` | Sweep integer threshold tuples against gold labels and emit a truth-space recommendation report. | Read-only workbench report; no registry writes, no strategy mutation, frozen `canon.entity.quality.v1` gates. |
| `canon registry export --format dbt-seed|search-index` | Project exact registry knowledge into transform or serving artifacts. | Deterministic downstream snapshots; no new matching semantics. |
| Package, project, and temporal workflows | Move registries and strategies through reproducible deployment, project locks, and snapshot comparison. | They package and check knowledge; they do not change exact lookup. |
| Extensions and adapters | Add profiles, source mappings, provider materializers, and domain policies out of tree. | Domain expertise stays outside Canon core defaults unless explicitly packaged and audited. |

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

The focused README examples in this section are covered by bd-np37's docs
parity checks where they use committed fixtures or temporary outputs. Exhaustive
execution of the generated command corpus is owned by bd-ndfh.

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

These workflows only create or update registry files. Normal `canon <INPUT> --registry ...` still resolves by exact byte match after ASCII-trim.

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
- Resolving reviewed aliases across vendor datasets
- Running deterministic entity resolution when the domain has modeled observations, anchors, context fields, audit suites, and a versioned registry (`canon entity`)
- Building cross-reference registries from two row sets that describe the same records with different IDs (`canon entity link`)
- Building audit trails for regulatory mappings (every resolution traceable)

**When canon might not be ideal:**
- Unbounded fuzzy entity matching with no strategy, audit, or review gate
- Master data management at enterprise scale
- Probabilistic record linkage requiring ML models

---

## Installation

### Homebrew (Recommended)

```bash
brew install cmdrvl/tap/canon
```

### From Source

```bash
cargo build --release
./target/release/canon --help
```

---

## CLI Reference

```bash
canon <INPUT> --registry <REGISTRY> --column <COLUMN> [--emit json|csv] [--canon-column <NAME>] [--map-out <PATH>] [--max-rows <N>] [--max-bytes <N>]
canon doctor health [--json]
canon doctor capabilities [--json]
canon doctor robot-docs
canon doctor --robot-triage
canon package pack --root <DIR> --package <package.json> --out <ARCHIVE>
canon package inspect <ARCHIVE> [--emit json|summary]
canon package verify <ARCHIVE> [--emit json|summary]
canon package unpack <ARCHIVE> --target <EMPTY_DIR> [--emit json|summary]
canon package push --archive <ARCHIVE> --registry <OCI_BASE_URL> --repository <REPOSITORY> [--tag <TAG>] [--emit json|summary]
canon package pull --registry <OCI_BASE_URL> --repository <REPOSITORY> --cache <DIR> (--digest <sha256:...>|--tag <TAG>) [--emit json|summary]
canon project init <DIR> [--project-id <ID>] [--mapping-profile <REF>] [--emit json|summary]
canon project validate <DIR> [--manifest <PATH>] [--emit json|summary]
canon project describe <DIR> [--manifest <PATH>] [--emit json|summary]
canon project lock refresh --manifest <MANIFEST> --out <LOCK> [--emit json|summary]
canon project plan --manifest <MANIFEST> --lock <LOCK> [--out <PLAN>] [--cache-hit <NODE>...] [--emit json|summary]
canon project run [--plan <PLAN>] [--manifest <MANIFEST>] [--lock <LOCK>] [--node <NODE>...] [--workspace <DIR>] [--work-dir <DIR>] [--max-parallelism <N>] [--allow-network] [--allow-mutation-gates] [--emit json|summary]
# Geo — primary surface (the commands used in the course of business)
canon geo capabilities [--emit json]
canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>
canon geo run --plan <PLAN.json> --work-dir <DIR> [--input <NODE_ID:BINDING_ID=PATH>...] [--satisfy <REQUEST_ID=RECEIPT.json>...]
canon geo replan-from-acquisition --base-plan <PLAN.json> --base-inventory <INVENTORY.json> --question <QUESTION.json> --capabilities <CAPABILITIES.json> --profile <PROFILE.json> --budget <BUDGET.json> --satisfy <REQUEST_ID=RECEIPT.json> --local-artifact <LOCAL_ARTIFACT_ID=PATH>... [--result <DIGEST_ID=PATH>...] --advancement-out <ADVANCEMENT.json>
canon geo evaluate --population <POPULATION.json> [--artifact-dir <DIR>]
canon geo inspect                                    # planned, not implemented
canon geo ledger build|validate|exposure|collision|card   # planned, not implemented

# Geo — stage leaves (driven by `geo run` and Demo 0; independently callable, hidden from top-level help)
canon geo link-sources --request <REQUEST.json> --rows-out <ROWS.csv>
canon geo materialize-home-cells --rows <ROWS.json>
canon geo tile-work --request <REQUEST.json>
canon geo reconcile-tiles --request <REQUEST.json>
canon geo materialize-geometry --request <REQUEST.json>
canon geo materialize-warehouse-geometry --rows <ROWS.json>
canon geo materialize-evidence --rows <ROWS.json>
canon geo materialize-address-evidence --request <REQUEST.json>
canon geo compile-evidence --request <REQUEST.json>
canon geo stack-evidence --population <POPULATION.json> --overlay <OVERLAY.json>
canon geo solve --request <REQUEST.json>

# Geo — measurement adapters (bounded profile adapters; moving under scripts/geo_measurements/)
canon geo materialize-h7-population --rows <ROWS.json>
canon geo materialize-h7-staging-batch --batch <BATCH.json>
canon geo materialize-h7-pip-block-batch --batch <BATCH.json>

canon inbox list --inbox <INBOX.json> [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json|summary]
canon inbox show --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json|summary]
canon inbox explain --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json|summary]
canon inbox stats --inbox <INBOX.json> [--policy <POLICY.json>] [--emit json|summary]
canon inbox export-review --inbox <INBOX.json> [--out <REVIEW.json>] [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json|summary]
canon inbox apply-review --inbox <INBOX.json> --review <REVIEW.json> --expected-inbox-hash <HASH> --out <GROUPS.json> [--emit json|summary]
canon inbox plan-entity --inbox <INBOX.json> --expected-inbox-hash <HASH> --out <REQUEST.json> [--policy <POLICY.json>] [--event-key <KEY>...] [--limit <N>] [--mode cluster|link] [--emit json|summary]
canon registry build --source <SOURCE> --seed <SEED> --seed-column <COLUMN> --output <DIR> --version <VER> [--incremental] [--max-rows <N>] [--max-bytes <N>] [--batch-size <N>] [--rate-limit-ms <MS>] [--provider-config <KEY=VALUE>]
canon registry export --format dbt-seed|search-index --registry <REGISTRY> --out <PATH> [--namespace <CONTEXT>] [--source-file <FILE>...] [--canonical-type <TYPE>...] [--rule-id-prefix <PREFIX>...] [--canonical-iri-prefix <PREFIX>] [--schema-out <schema.yml>] [--anti-collapse-test-out <test.sql>] [--emit json|summary]
canon registry providers [--emit json|summary]
canon registry provider-schema <PROVIDER> [--emit json|summary]
canon registry next-id [PREFIX] --registry <DIR> [--zero-pad <N>] [--emit plain|json]
canon registry add-entry --registry <DIR> --alias-file <FILE> --canonical-id <ID> --input <INPUT> --rule-id <RULE> [--canonical-type <TYPE>] [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain]
canon registry mint --registry <DIR> [--canonical-id <ID> | --prefix <PREFIX>] --canonical-type <TYPE> --with-alias <FILE=INPUT:RULE_ID>... [--bump patch|minor|major | --next-version <VER>] [--no-lint] [--emit json|plain]
canon registry default-id-scheme --registry <DIR> --prefix <PREFIX> [--zero-pad <N>] [--strict] [--bump patch|minor|major | --next-version <VER>] [--emit json|plain]
canon registry diff --old <OLD_REGISTRY> --new <NEW_REGISTRY> [--emit json|summary]
canon registry audit <SEED> --registry <REGISTRY> --column <COLUMN> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]
canon registry lint <REGISTRY> [--profile standard|org|strategy|auto] [--emit json|summary]
canon strategy profile <INPUT> [--emit json|summary] [--max-rows <N>] [--max-bytes <N>]
canon strategy audit --schema <PROFILE.json> --script <SCRIPT> --suite <DIR> [--emit json|summary]
canon strategy resolve --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]
canon strategy register --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --grade operator-attested|proof-attested --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--rule-id <RULE>] [--emit json|summary] [--no-witness]
canon strategy update --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --script <SCRIPT> --script-id <ID> --language <LANG> --next-version <VER> [--operator <ID> --reason <TEXT> --attested-at <RFC3339>] [--verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json>] [--emit json|summary] [--no-witness]
canon strategy deprecate --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --operator <ID> --reason <TEXT> --next-version <VER> [--attested-at <RFC3339>] [--emit json|summary] [--no-witness]
canon strategy promote --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> --verify <VERIFY.json> --assess <ASSESS.json> --airlock <AIRLOCK.json> --next-version <VER> [--emit json|summary] [--no-witness]
canon strategy list --registry <DIR> [--key-type schema|task] [--grade operator-attested|proof-attested] [--status active|deprecated] [--emit json|summary]
canon strategy explain --registry <DIR> (--schema <SCHEMA.json>|--task <TASK>) --skill <SKILL.md>|--skill-hash <HASH> [--emit json|summary]
canon strategy diff --old <OLD_DIR> --new <NEW_DIR> [--emit json|summary]
canon entity run <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--cache-mode enabled|disabled] [--suite <DIR>] [--emit json|summary] [--no-witness]
canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json|summary] [--cache-mode enabled|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]
canon entity alias-withholding --manifest <EXECUTION_ENVELOPE.json> [--emit json|summary]
canon entity generalization --manifest <STRICT_ENVELOPE.json> [--emit json|summary]
canon entity calibrate sweep <RESULT|EVIDENCE> --gold <GOLD.jsonl> --strategy <STRATEGY.yaml> [--emit json|summary]
canon entity prepare <ROWS> --profile <PROFILE> --registry <DIR> --work-dir <DIR>
canon entity index build <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit json|summary]
canon entity block <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit jsonl|summary]
canon entity block preflight <ROWS> --profile <PROFILE> --strategy <YAML> [--sample-pct <N>] [--work-dir <DIR>] [--emit json|summary]
canon entity candidate-recall --manifest <MANIFEST.json> --candidates <CANDIDATES.jsonl> --diagnostics <DIAGNOSTICS.json> --exact-bucket-count <N> [--emit json|summary]
canon entity evidence <ROWS> [--profile <PROFILE>] --strategy <YAML> --candidates <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit jsonl|summary]
canon entity solve <ROWS> [--profile <PROFILE>] --strategy <YAML> --evidence <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit json|summary]
canon entity audit <RESULT.json> --suite <DIR> [--emit json|summary]
canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <DIR> --next-version <VER> [--emit json|summary]
canon entity apply <RESULT.json> --rows <ROWS> --registry <DIR> [--column <COL>] [--output <PATH>] [--work-dir <DIR>] [--require-full-resolution|--allow-partial-output] [--emit json|summary]
canon entity review export <RESULT.json> [--artifact queue|native-review] [--group-by signature] [--emit json|csv|html] [--include resolved|escrow|contradictions|all]
canon entity review import <REVIEW.json|csv> --registry <DIR> --next-version <VER> [--audit <AUDIT.json>] [--source-review <NATIVE_REVIEW.json>] [--emit json|summary]
canon entity explain <RESULT.json> --row <ROW_ID>|--surface-id <SURFACE_ID>|--canon-id <CANON_ID>|--escrow-id <ESCROW_ID> [--emit json|summary]
canon entity profile list [--emit json|summary]
canon entity profile init <PROFILE> --output <PATH>
```

#### Geo command surface

Geo commands are declared in three tiers, and `canon --describe` carries the tier on
every row:

| Tier | Commands | Who runs it |
|------|----------|-------------|
| **primary** | `geo capabilities`, `geo plan`, `geo run`, `geo replan-from-acquisition`, `geo evaluate`, plus `geo inspect` and `geo ledger` once they ship | Agents and operators in the course of business. Listed first in `--help`, here, and in `canon --describe`. |
| **leaf** | `link-sources`, `materialize-home-cells`, `tile-work`, `reconcile-tiles`, `materialize-geometry`, `materialize-warehouse-geometry`, `materialize-evidence`, `materialize-address-evidence`, `compile-evidence`, `stack-evidence`, `solve` | The stages `geo run` executes. Kept independently callable for Demo 0, single-stage debugging, and tests; hidden from top-level `--help` and still machine-described. |
| **measurement** | `materialize-h7-population`, `materialize-h7-staging-batch`, `materialize-h7-pip-block-batch` | Bounded profile adapters for the measurement harness. They move under `scripts/geo_measurements/` and the `canon_geo_measurements` binary; they are not a regional engine. |

`geo inspect` and `geo ledger` are on the primary surface in
[`docs/PLAN_CANON_GEO.md`](docs/PLAN_CANON_GEO.md) §19.3 but are **not implemented yet** —
they carry `planned_not_implemented` status and are documented here so the intended surface
is legible, not because they are callable. No shipped command was removed or renamed to
reach this shape: the tiers change ordering and visibility, not availability.

##### Primary

`canon geo plan --question --capabilities --inventory --profile --budget` is a
deterministic offline planner. It emits `canon_geo_plan.v0`, a Geo semantic overlay over one
validated `canon.project.plan.v1` DAG. It plans the current parcel/building composition
profile only: omitted/default `parcel` still requires a parcel universe, explicit
`building` can produce a parcel-free building plan, and unsupported grains stay separately
typed instead of poisoning supported grains. Missing local source inputs become typed
discovery/acquisition requests when the required release or as-of selector is present;
otherwise they remain explicit discovery gaps. The command does not execute work, acquire
data, or prove candidate reach without an independent reference.

`canon geo run --plan --work-dir` is a bounded offline run over the Geo plan's single
validated project DAG. It delegates to the shared project runner and registered internal
Geo executor for the current five-stage chain: materialize home cells, build the bounded
tile section, materialize evidence, compile evidence, then solve composition. `--input` is
optional at invocation because a blocked plan or missing local artifacts can still produce
a typed `WAITING_FOR_INPUT` run with exact next actions. When supplied, public inputs are
local exogenous leaf artifacts only: home-cell rows, tile-work requests, and warehouse
rows, each bound by artifact id, media/contract, byte count, and canonical BLAKE3 digest.
Compile-evidence and solve are fed by declared dependency outputs, not by external
overrides. `--satisfy` validates consistency between acquisition receipts and explicit
input bytes only; it does not mutate the plan, clear acquisition blockers, or replan.

##### Stage leaves

`canon geo link-sources --request` makes the existing N-source materializer reachable
without changing the two-tape `canon entity link` contract. The request declares three or
more named CSV sources, exactly one client-book target, at least one bounded reference,
optional peers, a complete-by-default comparison graph, and explicit pair budgets.
`canonical_reference` is refused on this Geo surface: no parcel or footprint vendor wins
globally by role. The command atomically publishes the merged rows at `--rows-out` and
emits `canon_entity_multisource_link.v1` on stdout with per-source hashes, the merged-row
hash, pair-count diagnostics, anchor-conflict abstentions, and a path-independent semantic
artifact hash suitable for an upstream artifact reference. Roles and source count are
provenance; they do not become evidence weights or independent constraints.

`canon geo materialize-home-cells --rows` derives deterministic H3 blocking/ownership cells
offline from release-bound, fixed-decimal EPSG:4326 representative points using h3o. Each
row binds the point to a source snapshot, record, geometry SHA-256, representative-point
method, and optional transform execution/definition pair. The artifact reports claimed
warehouse-cell matches and mismatches rather than treating a claim as authority, and
retains the cells reached by nine deterministic corner, axis, and center probes of a
declared coordinate perturbation envelope. That sampled sensitivity set and its minimum
covering halo expose blocking-boundary fragility; they do not exhaustively cover the
continuous envelope and are not additional identity evidence. The digest is a typed
binding, not a recomputation: geometry bytes are intentionally absent from this artifact.
One source name cannot mix snapshots, point methods, or transform executions, preventing
temporal evidence from becoming a timeless blocking claim. H3 remains an index, never a
substitute for exact geometric predicates.

`canon geo tile-work --request` turns explicit H3 home-cell assignments, including the
`tile_work_features` projection emitted by `materialize-home-cells`, into a
budgeted center-plus-`k`-ring work unit. The artifact records the complete ordered work
cell set and classifies every supplied feature as center or halo. Mixed resolutions,
duplicate source features, features outside the declared work cells, and cell/feature
budget overruns refuse. Geometry contracts remain authoritative for spatial predicates,
and even verified home-cell assignment cannot prove candidate recall: reach still requires
comparison against a complete bounded reference. Declared budgets are additionally capped
by fixed schema/kernel ceilings. A work unit is an envelope for incidence factorization and
small exact residuals; it is not an instruction to solve every feature in the tile
monolithically.

`canon geo reconcile-tiles --request` consumes independently solved decision batches,
including the exact canonical work unit supplied to each solver, and emits one decision
per canonical member set plus a BLAKE3 receipt for every input work unit. A proposal member
must occur in that work unit with the same home cell. The owner is the numerically smallest
member home cell, independent of source names, input order, or worker completion order. A
missing owner work unit, a proposal seen only from halo work units, an unavailable member,
or different payload digests for the same members is a typed refusal; conflicts are never
silently merged. The reconciler checks payload digests, not the payload semantics, which
remain the local solver's responsibility.

`canon geo solve --request` accepts either a bare
`canon_geo_composition_request.v0` request or the complete
`canon_geo_evidence_compilation.v0` artifact emitted by `compile-evidence`. Solving the
compilation artifact preserves its content digest in the composition output.

`canon geo materialize-geometry --request` converts fixed-decimal source coordinates
through a versioned per-tile integer affine frame into canonical millimetre geometry.
It emits point, polygon, or multipolygon feature values with explicit CRS/frame metadata,
bbox, vertex count, exact snap-loss accounting, and a separate projection-error envelope.
Canonical normalization makes ring direction/start and hole/polygon input ordering
byte-invariant. Invalid topology, non-finite or over-precision coordinates, mixed CRS,
antimeridian crossing, overflow, and declared vertex/byte-budget excess are typed refusals;
decision geometry is never simplified or truncated to fit.

`canon geo materialize-warehouse-geometry --rows` is the release-pinned source-plane
bridge. It recomputes each base64 ISO-WKB SHA-256, refuses mixed source releases,
archives, CRS/SRID values, geometry contracts, or transform executions, and decodes only
2D point, polygon, and multipolygon values. For planar sources it uses the supplied stable,
versioned source origin and exact source-unit-to-millimetre ratio to construct a bounded
local frame. Reusing that origin is mandatory: deriving it from the current row bounds
would move earlier canonical coordinates when new evidence accretes. The artifact reports
WKB-float to fixed-decimal admission loss separately from fixed-decimal to integer-
millimetre snapping; the exact source-plane affine translation declares zero projection
error. Transform ids retain linkage to the WGS84 interoperability sibling, but that
operation is not reapplied to decision geometry.

`canon geo materialize-evidence --rows` is the offline bridge from exported relational
rows to `canon_geo_evidence_request.v0`. Its input contract is
`canon_geo_warehouse_rows.v0`: parcel rows declare the candidate grain,
building/parcel rows declare candidate incidence (a null parcel is an explicit
no-containment marker), and evidence rows group immutable source records under one typed
rho observation. The command rejects duplicate grains and conflicting rows, sorts keyed
collections, validates the result through the same compiler used by `compile-evidence`,
and writes only to stdout. It performs no warehouse acquisition, and multiple source
records remain provenance for one observation rather than independent constraint weight.

`canon geo stack-evidence --population --overlay` accretes contracts and observations onto
named cases in an already bounded `canon_geo_population_request.v0`. The overlay contract
cannot carry truth, candidate members, solver profile, or solver budgets. It may optionally
bind the exact pre-stack evidence digest for optimistic concurrency; the emitted
`canon_geo_population_evidence_stack.v0` retains both inputs, the resulting population,
per-case before/after digests, and hard/soft/diagnostic admission counts so the whole step
can be replay-validated. Exact observation reuse is idempotent. Contract drift, observation
ID drift, and the same semantic observation renamed under another ID are refusals, which
prevents source or preference-count inflation. A validated stack artifact can be passed
directly to another `stack-evidence` invocation or to `canon geo evaluate`. More hard
evidence narrows the feasible set and can make it empty; soft evidence changes objective
cost only; diagnostic evidence changes neither feasibility nor cost. Source-record count is
reported solely as provenance volume.

`canon geo materialize-address-evidence --request` is the single-call offline bridge
from an address parse request plus a PAD address set to an address/PAD parcel
observation bundle. The request contract is
`canon_geo_address_parcel_evidence_request.v0`, which carries the parse request,
PAD set, and `canon_geo_address_parcel_bridge_request.v0`; the emitted
`canon_geo_address_parcel_evidence_bundle.v0` preserves the parse forest, PAD
membership, and `canon_geo_address_parcel_bridge.v0`. Supported ambiguous readings
union their parcel candidates. Readings without PAD source-member support stay
diagnostic, so the command may abstain instead of fabricating an empty hard
constraint. When the bridge emits `bridge.observation`, that observation still must
be wrapped by a separately authored `canon_geo_evidence_request.v0` with an explicit
candidate universe and matching rho `contracts` before `compile-evidence` can admit
it. The bundle itself is not a compiler-ready evidence request. Each source-record
association carries a Canon-recomputed BLAKE3 of the normalized PAD member payload,
so an id-only binding cannot silently change the lot/address content. That integrity
link does not authenticate upstream PAD bytes or replace an acquisition receipt. The
command performs no live acquisition and makes no empirical truth claim.

##### Measurement adapters

`canon geo materialize-h7-staging-batch --batch` is the bounded NYC/H.7 profile adapter
for the staging-derived source-record payload contract. It validates batch guards,
denominators, release pins, source-record bytes, and profile truth planes before delegating
to the generic H.7 population materializer. It is not the generic regional engine and does
not turn derived payload rows or fixtures into live source proof.

`canon geo materialize-h7-pip-block-batch --batch` is the bounded offline adapter for the
warehouse H.7 candidate-block export. It validates the complete observed snapshot,
release-row and source-plane denominators, exact MapPLUTO candidate locators, truth-plane
separation, and a BLAKE3 digest over the typed rows before emitting the generic H.7
population. Observed rows remain diagnostic evidence unless a separately cited live
acquisition satisfies the stricter query-receipt contract.

Open Geo limits remain: acquisition stays outside Canon's deterministic offline run,
exactness is representation-relative to admitted candidates and contracts, candidate reach
is an upstream proof obligation, immutable cross-release reuse in the same work directory is
not guaranteed, E5/live scale proof is not shipped, and the primary-surface `geo inspect` and `geo ledger` verbs remain unimplemented.

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
| `--explicit` | flag | `false` | Show input and canonical_id values verbatim in JSON output. By default they are masked as `[REDACTED]` for zero-retention safety and the envelope reports `"redacted": true`. |
| `--plain-json-values` | flag | `false` | With `--explicit`, emit UTF-8 JSON `input`/`canonical_id` values without the `u8:` prefix and add `*_encoding` metadata. Non-text/control-byte values remain hex encoded. |
| `--version` | flag | | Print version and exit. |
| `--describe` | flag | | Emit `operator.json` to stdout and exit. |
| `--schema` | flag | | Print JSON Schema for the mapping artifact and exit. |

### Config Footprint

By default, `canon` appends witness records to `~/.cmdrvl/state/witness/witness.jsonl`. `EPISTEMIC_WITNESS` remains an explicit operator override; override paths are used as provided and are not migrated.

On first default witness use, `canon` copy-migrates an existing legacy `~/.epistemic/witness.jsonl` or `.epistemic/witness.jsonl` ledger into the canonical path. It never deletes or moves the legacy file. Migration and deprecation notices are path-only JSONL records under `~/.cmdrvl/migrations/applied.jsonl` and `~/.cmdrvl/notices/deprecated-paths.jsonl`; file contents and secret values are not recorded.

### Subcommands

| Subcommand | Description |
|------------|-------------|
| `doctor health [--json]` | Report compiled manifest and read-only contract health. |
| `doctor capabilities [--json]` | Describe doctor commands, exit codes, side effects, and fixers. |
| `doctor robot-docs` | Emit concise machine-oriented usage notes. |
| `doctor --robot-triage` | Emit compact machine-readable triage JSON. |
| `package pack --root <DIR> --package <package.json> --out <ARCHIVE>` | Create a deterministic local package archive from canonical package bytes. |
| `package inspect <ARCHIVE> [--emit json\|summary]` | Inspect package inventory and metadata without writing files. |
| `package verify <ARCHIVE> [--emit json\|summary]` | Verify package archive digests and semantic package contracts. |
| `package unpack <ARCHIVE> --target <EMPTY_DIR> [--emit json\|summary]` | Unpack a verified archive into an existing empty target directory. |
| `package push --archive <ARCHIVE> --registry <OCI_BASE_URL> --repository <REPOSITORY> [--tag <TAG>] [--emit json\|summary]` | Publish a verified local package archive to an OCI registry by immutable digest. |
| `package pull --registry <OCI_BASE_URL> --repository <REPOSITORY> --cache <DIR> (--digest <sha256:...>\|--tag <TAG>) [--emit json\|summary]` | Pull and verify a package from an OCI registry into an external content cache. |
| `project init <DIR> [--project-id <ID>] [--mapping-profile <REF>] [--emit json\|summary]` | Create a minimal neutral project manifest in an explicit empty or missing directory. |
| `project validate <DIR> [--manifest <PATH>] [--emit json\|summary]` | Validate a project manifest and report deterministic diagnostics. |
| `project describe <DIR> [--manifest <PATH>] [--emit json\|summary]` | Describe project capabilities, state flags, side effects, and next commands. |
| `project lock refresh --manifest <MANIFEST> --out <LOCK> [--emit json\|summary]` | Hash actual declared local source bytes and atomically refresh a `canon.project.lock.v1` artifact. |
| `project plan --manifest <MANIFEST> --lock <LOCK> [--out <PLAN>] [--cache-hit <NODE>...] [--emit json\|summary]` | Emit the native validated `canon.project.plan.v1` artifact and executable `project run --node` next commands. |
| `project run [--plan <PLAN>] [--manifest <MANIFEST>] [--lock <LOCK>] [--node <NODE>...] [--workspace <DIR>] [--work-dir <DIR>] [--max-parallelism <N>] [--allow-network] [--allow-mutation-gates] [--emit json\|summary]` | Validate a project plan, reuse valid `canon.project.run.v2` receipts, and execute supported pending nodes through registered internal offline executors. Unknown or unsafe executor declarations still refuse before publication. |
| `geo capabilities [--emit json]` | *(primary)* Emit the compiled offline Geo capability contracts, including each command's surface tier. Deterministic and read-only. |
| `geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>` | *(primary)* Compile a deterministic offline `canon_geo_plan.v0` over one validated `canon.project.plan.v1` DAG. Missing local sources become typed discovery/acquisition requests or explicit discovery gaps; the command executes no work and acquires nothing. |
| `geo evaluate --population <POPULATION.json> [--artifact-dir <DIR>]` | *(primary)* Evaluate a bounded population request and report coverage, reach, rho, solver, truth, and cost as separate planes. This is the E4/E5 gate instrument. |
| `geo inspect` | *(primary, planned — not implemented)* Inspect a run's sections, components, and residuals. Tracked in `docs/PLAN_CANON_GEO.md` §19.3. |
| `geo ledger build\|validate\|exposure\|collision\|card` | *(primary, planned — not implemented)* Ledger construction, validation, exposure, collision, and card verbs. Tracked in `docs/PLAN_CANON_GEO.md` §19.3. |
| `geo run --plan <PLAN.json> --work-dir <DIR> [--input <NODE_ID:BINDING_ID=PATH>...] [--satisfy <REQUEST_ID=RECEIPT.json>...]` | *(primary)* Execute or preflight the current bounded offline Geo five-stage chain through the shared project runner and Geo executor, using only local exogenous leaf inputs when provided. It resumes validated completed outputs, refuses undeclared commands or compile/solve input overrides, and emits a `canon_geo_run.v0` projection over `canon.project.run.v2` receipts. `--satisfy` checks receipt/explicit-byte consistency only; it does not mutate the plan, clear acquisition blockers, or replan. |
| `geo replan-from-acquisition --base-plan <PLAN.json> --base-inventory <INVENTORY.json> --question <QUESTION.json> --capabilities <CAPABILITIES.json> --profile <PROFILE.json> --budget <BUDGET.json> --satisfy <REQUEST_ID=RECEIPT.json> --local-artifact <LOCAL_ARTIFACT_ID=PATH>... [--result <DIGEST_ID=PATH>...] --advancement-out <ADVANCEMENT.json>` | *(primary)* Validate one live, complete, positive, nontruncated, full-region acquisition receipt against exact local artifact bytes, atomically publish a separate `canon_geo_regional_inventory_advancement.v0` sidecar, and emit a new base-inventory-bound `canon_geo_plan.v0` on stdout. It never performs acquisition or mutates the old plan or inventory. |
| `geo <stage leaf>` and `geo materialize-h7-*` | Stage-leaf and measurement-tier commands. They stay independently callable for Demo 0, debugging, and tests but are not part of the primary surface — see [Geo command surface](#geo-command-surface) for the full tier table and per-command contracts. |
| `inbox list --inbox <INBOX.json> [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json\|summary]` | List ranked unresolved inbox items with deterministic pagination and typed filters. |
| `inbox show --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json\|summary]` | Show one unresolved inbox item and its next commands. |
| `inbox explain --inbox <INBOX.json> --event-key <KEY> [--policy <POLICY.json>] [--emit json\|summary]` | Explain one inbox item's priority score components and provenance. |
| `inbox stats --inbox <INBOX.json> [--policy <POLICY.json>] [--emit json\|summary]` | Summarize inbox counts and ranking coverage. |
| `inbox export-review --inbox <INBOX.json> [--out <REVIEW.json>] [--policy <POLICY.json>] [--limit <N>] [--cursor <CURSOR>] [--event-kind <KIND>...] [--reason-code <REASON>...] [--field-role <ROLE>...] [--partition <KEY>...] [--emit json\|summary]` | Export a stable review queue for selected unresolved inbox items. |
| `inbox apply-review --inbox <INBOX.json> --review <REVIEW.json> --expected-inbox-hash <HASH> --out <GROUPS.json> [--emit json\|summary]` | Apply explicit review decisions into a grouped unresolved artifact. |
| `inbox plan-entity --inbox <INBOX.json> --expected-inbox-hash <HASH> --out <REQUEST.json> [--policy <POLICY.json>] [--event-key <KEY>...] [--limit <N>] [--mode cluster\|link] [--emit json\|summary]` | Plan a bounded entity workbench request without deciding identity. |
| `registry build --source <NAME> --seed <PATH> --seed-column <COLUMN> --output <DIR> --version <VER>` | Materialize a standard canon registry directory from a provider-backed seed corpus, with optional repeatable `--provider-config key=value` overrides such as OpenFIGI `id_type`, `base_url`, `api_key`, or mapping filters like `exchCode=US`. |
| `registry export --format dbt-seed\|search-index --registry <DIR> --out <PATH>` | Export a versioned registry as a deterministic dbt seed CSV or a self-describing SQLite search index, with optional source-file, canonical-type, and rule-id-prefix filters. |
| `registry providers [--emit json\|summary]` | List the registry build providers available for materialization, with their seed-column support and a pointer to each provider's schema command. |
| `registry provider-schema <PROVIDER> [--emit json\|summary]` | Emit one provider's machine-readable `--provider-config` option contract — keys, types, enum values, secret flags, env fallbacks, defaults, mutual exclusions, interval encoding, and examples. Deterministic and offline; never contacts the provider. |
| `registry next-id [PREFIX] --registry <DIR> [--zero-pad <N>] [--emit plain\|json]` | Read the existing canonical IDs for a self-authored namespace and suggest the next deterministic ID. Uses `registry.json.default_id_scheme` when `PREFIX` is omitted. |
| `registry add-entry --registry <DIR> --alias-file <FILE> --canonical-id <ID> --input <INPUT> --rule-id <RULE> [--canonical-type <TYPE>]` | Append one exact alias entry to an existing root mapping file, bump the registry version, update `entry_count`, and run standard lint unless `--no-lint` is set. |
| `registry mint --registry <DIR> [--canonical-id <ID>\|--prefix <PREFIX>] --canonical-type <TYPE> --with-alias <FILE=INPUT:RULE_ID>...` | Mint one self-authored canonical ID and one or more starting aliases in a single versioned write. Without `--canonical-id`, allocates via `next-id`. |
| `registry default-id-scheme --registry <DIR> --prefix <PREFIX> [--zero-pad <N>] [--strict]` | Persist the registry's default self-authored ID convention in `registry.json` so `next-id` and `mint` can allocate without a prefix argument. |
| `registry diff --old <PATH> --new <PATH> [--emit json\|summary]` | Compare two versions of the same registry ID and report added, removed, changed, and unchanged effective mappings. |
| `registry audit <SEED> --registry <PATH> --column <COLUMN> [--emit json\|summary]` | Audit a seed corpus against a registry and emit resolved/unresolved entries plus aggregate canonical-target and rule-hit counts. |
| `registry lint <DIR> [--profile standard\|org\|strategy\|auto] [--emit json\|summary]` | Preflight standard mapping, entity-sidecar, or strategy registry health with severity-tagged findings. |
| `strategy profile <INPUT> [--emit json\|summary] [--max-rows <N>] [--max-bytes <N>]` | Derive a deterministic schema/profile artifact from CSV, TSV, JSONL, or NDJSON for `strategy resolve` and `strategy register`. |
| `strategy audit --schema <JSON> --script <PATH> --suite <DIR> [--emit json\|summary]` | Run a frozen script against deterministic fixture expectations and emit a `canon_strategy_audit.v0` proof artifact. |
| `strategy resolve --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH>` | Resolve a schema or exact task key plus skill hash to a frozen champion script. Schema keys can return EXACT/COMPATIBLE/PARTIAL/UNRESOLVED; task keys return EXACT or UNRESOLVED only. |
| `strategy register --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --script <PATH> --script-id <ID> --language <LANG> --grade operator-attested\|proof-attested --next-version <VER>` | Register a v1 typed strategy entry. Operator-attested entries need operator/reason and no proof artifacts; proof-attested entries require verify/assess/airlock. |
| `strategy update --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --script <PATH> --script-id <ID> --language <LANG> --next-version <VER>` | Update an active champion in place with a version bump. |
| `strategy deprecate --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --operator <ID> --reason <TEXT> --next-version <VER>` | Mark an active champion deprecated without deleting history. |
| `strategy promote --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> --verify <JSON> --assess <JSON> --airlock <JSON> --next-version <VER>` | Promote an operator-attested champion to proof-attested. |
| `strategy list --registry <DIR> [--key-type schema\|task] [--grade operator-attested\|proof-attested] [--status active\|deprecated] [--emit json\|summary]` | Inspect mixed schema/task strategy registries, provenance, grade, status, source file, and entry order. |
| `strategy explain --registry <DIR> (--schema <JSON>\|--task <TASK>) --skill <PATH>\|--skill-hash <HASH> [--emit json\|summary]` | Explain active and ignored entries for one strategy key. |
| `strategy diff --old <DIR> --new <DIR> [--emit json\|summary]` | Compare frozen-script strategy registry versions by typed key plus skill hash, including grade/status/attestation changes. |
| `entity run <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--cache-mode enabled\|disabled] [--suite <DIR>] [--emit json\|summary]` | Run the cluster-mode artifact pipeline (prepare -> index -> block -> evidence -> solve, optional audit). Cache mode defaults to enabled, and native cache receipts are recorded in the run artifact. |
| `entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json\|summary] [--cache-mode enabled\|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]` | Run link mode for aligning two row sets through the same typed request and artifact path as project mode, with optional suite/gold scoring. Profile and work-dir are required for successful execution even though generated syntax shows them bracketed; omissions write nothing. `--write-back` currently refuses before work-dir or registry mutation, so accepted knowledge flows through review, audit, promote, and apply. Cache mode defaults to enabled, and native cache receipts are inherited from the nested run. |
| `entity alias-withholding --manifest <EXECUTION_ENVELOPE.json> [--emit json\|summary]` | Compile a strict execution envelope into an alias-withholding report. The envelope references clean registry, candidate, link, run/solve, review, audit, leak-scan, assignment-firewall, and optional promotion/replay artifacts; Canon derives outcomes from those artifacts and refuses self-declared results. |
| `entity generalization --manifest <STRICT_ENVELOPE.json> [--emit json\|summary]` | Compile a strict artifact-backed entity-disjoint/time-forward envelope into a redacted report. The same command is used for public fixtures and operator-owned private corpora; identifiers, paths, and cutoffs are hashed at the CLI boundary, and outcomes/leakage checks are derived from referenced artifacts rather than self-attested fields. |
| `entity calibrate sweep <RESULT\|EVIDENCE> --gold <GOLD.jsonl> --strategy <STRATEGY.yaml> [--emit json\|summary]` | Sweep deterministic integer threshold tuples against gold labels and emit `canon.entity.calibrate_sweep.v0`. The report maximizes auto-accept subject to frozen `canon.entity.quality.v1` gates, emits a YAML threshold fragment for manual application, and never writes registry or strategy files. |
| `entity prepare <ROWS> --profile <PROFILE> --registry <DIR> --work-dir <DIR>` | Validate and project profile-mapped observations for artifact-backed entity preparation. |
| `entity index build <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit json\|summary]` | Build deterministic index artifacts for a work directory. |
| `entity block <ROWS> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--emit jsonl\|summary]` | Generate candidate neighborhoods via blocking operators. |
| `entity block preflight <ROWS> --profile <PROFILE> --strategy <YAML> [--sample-pct <N>] [--work-dir <DIR>] [--emit json\|summary]` | Estimate candidate cardinality and worst-block skew before running the block stage. |
| `entity candidate-recall --manifest <MANIFEST.json> --candidates <CANDIDATES.jsonl> --diagnostics <DIAGNOSTICS.json> --exact-bucket-count <N> [--emit json\|summary]` | Evaluate candidate retrieval recall against sealed public must-link labels. |
| `entity evidence <ROWS> [--profile <PROFILE>] --strategy <YAML> --candidates <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit jsonl\|summary]` | Score typed evidence for blocked candidate pairs. Relationship evidence remains a relation hint unless a separate equality fact or support lane justifies equivalence. |
| `entity solve <ROWS> [--profile <PROFILE>] --strategy <YAML> --evidence <JSONL> --registry <DIR> [--work-dir <DIR>] [--emit json\|summary]` | Solve deterministic identity assignments from evidence artifacts. |
| `entity audit <RESULT> --suite <DIR> [--emit json\|summary]` | Validate a solve/run artifact against a frozen evaluation suite. |
| `entity promote <RESULT> --audit <JSON> --registry <DIR> --next-version <VER> [--emit json\|summary]` | Write audited results into registry aliases and escrow sidecars. |
| `entity apply <RESULT> --rows <ROWS> --registry <DIR> [--column <COL>] [--output <PATH>] [--work-dir <DIR>] [--require-full-resolution\|--allow-partial-output] [--emit json\|summary]` | Replay accepted assignments from a solve or run artifact onto input rows without changing the registry. |
| `entity review export <RESULT> [--artifact queue\|native-review] [--group-by signature] [--emit json\|csv\|html] [--include resolved\|escrow\|contradictions\|all]` | By default, produce the unchanged review queue contract. With `--artifact native-review`, emit `canon_entity_native_review.v0` as JSON, CSV, or offline HTML; `--group-by signature` presents native items by deterministic evidence signature without changing decision authority. |
| `entity review import <REVIEW> --registry <DIR> --next-version <VER> [--audit <JSON>] [--source-review <NATIVE_REVIEW.json>] [--emit json\|summary]` | Import default queue decisions, or with `--source-review` import native decisions, including JSON group decisions expanded against the source review artifact, into `canon_entity_native_review_import.v0` while keeping registry/version compatibility arguments. |
| `entity explain <RESULT> --row <ID>\|--surface-id <ID>\|--canon-id <ID>\|--escrow-id <ID> [--emit json\|summary]` | Proof trace for one row, prepared surface, canonical entity, or escrow entity. |
| `entity profile list [--emit json\|summary]` | List built-in entity profile templates. |
| `entity profile init <PROFILE> --output <PATH>` | Write a built-in entity profile template to disk. |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success: RESOLVED, healthy report, or successful report/write |
| `1` | Domain outcome needs inspection: PARTIAL, UNRESOLVED, unhealthy contract health, or another command-specific failed gate |
| `2` | REFUSAL or CLI error |

`canon registry diff`, `canon registry audit`, `canon registry export`, and `canon registry lint` exit `0` when the report succeeds and `2` on refusal. Lint findings are represented inside `canon_registry_lint.v0` rather than via exit status. `canon registry build`, `registry next-id`, `registry add-entry`, `registry mint`, and `registry default-id-scheme` exit `0` when their report or write succeeds and `2` on refusal. Provider failures from `registry build` are preserved in the JSON report and warned on stderr. `add-entry` and `mint` restore the original files if their post-write lint gate finds errors.

`canon strategy profile`, `canon strategy register`, `canon strategy update`, `canon strategy deprecate`, `canon strategy promote`, `canon strategy list`, `canon strategy explain`, and `canon strategy diff` exit `0` when their reports or writes succeed and `2` on refusal. `canon strategy audit` exits `0` when all fixtures pass, `1` when deterministic fixture checks fail, and `2` on refusal. `canon strategy resolve` exits `0` for a schema EXACT/COMPATIBLE or task EXACT match, `1` for schema PARTIAL/UNRESOLVED or task UNRESOLVED, and `2` on refusal.

`canon entity link` exits `0` when every target record is matched, `1` when any target record is unmatched or ambiguous, and `2` on refusal. In `summary` mode, refusal JSON is written to stderr.

`canon entity alias-withholding` exits `0` when it emits a JSON or summary
report from a valid execution envelope and `2` when the envelope or any
referenced artifact refuses validation. It does not use exit `1` for benchmark
outcomes.

`canon entity generalization` exits `0` when it emits a structurally valid JSON
or summary report, including low-quality or critical-false-merge reports whose
`quality.release_claim_status` is `blocked`. It exits `2` only when the envelope
or referenced artifacts are malformed, missing, stale, tampered, or otherwise
refuse validation. It does not use exit `1` for benchmark outcomes.

`canon entity calibrate sweep` exits `0` when it emits a structurally valid JSON
or summary report, including `recommendation.status: blocked` when no threshold
tuple satisfies `canon.entity.quality.v1`. It exits `2` for malformed
result/gold/strategy inputs or gold pairs absent from the scored artifact.

`canon doctor health`, bare `canon doctor`, and `canon doctor --robot-triage`
exit `0` when compiled contract parity is healthy and `1` when the report is
emitted but unhealthy. `canon doctor capabilities` and `canon doctor robot-docs`
exit `0` when their read-only reports are emitted. All doctor forms exit `2` for
CLI usage errors such as unsupported `--fix`. The JSON schemas are
`canon.doctor.health.v1`, `canon.doctor.capabilities.v1`, and
`canon.doctor.triage.v1`.

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
canon registry lint tests/fixtures/registries/counterparty --profile auto --emit summary
```

Export a context-scoped dbt seed snapshot:

```bash
canon registry export \
  --format dbt-seed \
  --registry registries/funds/ \
  --namespace funds \
  --canonical-type fund \
  --out seeds/canon_funds.csv \
  --schema-out models/schema.yml \
  --anti-collapse-test-out tests/assert_canon_funds_no_collapse.sql
```

Export a generic SQLite search index for serving endpoints:

```bash
canon registry export \
  --format search-index \
  --registry registries/funds/ \
  --out artifacts/funds.search.sqlite
```

Both export formats preserve the registry boundary: they snapshot exact registry knowledge with version, content hash, source file, rule, alias kind, normalized key, and `canonical_iri` provenance. They do not add serving coverage facts or call providers.

Materialize a registry from a provider-backed seed corpus. `openfigi` supports `cusip`, `isin`, and `sedol` seed columns by inference, or an explicit `--provider-config id_type=ID_CUSIP|ID_ISIN|ID_SEDOL`; use `--provider-config base_url=...` for local twins and tests. Corpus-wide OpenFIGI mapping filters pass through to every mapping job when supplied as provider config, including `exchCode`, `micCode`, `currency`, `marketSecDes`, `securityType`, `securityType2`, `optionType`, `includeUnlistedEquities`, `strike`, `contractSize`, `coupon`, `expiration`, and `maturity`. These filters narrow provider materialization only; normal canon lookup still reads static registry files and never calls OpenFIGI. For identifier-heavy corpora, extract identifiers from the source tapes, normalize and dedupe them, split CUSIP/ISIN/SEDOL into separate seed files, run one build per id type, publish the resulting static registries, and use `--incremental` for follow-up corpus refreshes. The direct OpenFIGI command below is a live provider example, not an offline fixture:

```bash
OPENFIGI_API_KEY=xxx \
canon registry build \
  --source openfigi \
  --seed seeds.csv \
  --seed-column cusip \
  --provider-config exchCode=US \
  --output registries/openfigi-cusip/ \
  --version 2026.03.13
```

Maintain a self-authored alias registry:

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

canon registry default-id-scheme \
  --registry "$example_dir/registries/people" \
  --prefix PPL \
  --zero-pad 3

canon registry next-id --registry "$example_dir/registries/people"

canon registry mint \
  --registry "$example_dir/registries/people" \
  --canonical-type person \
  --with-alias 'aliases.json=Jane Doe:MANUAL'

canon registry add-entry \
  --registry "$example_dir/registries/people" \
  --alias-file aliases.json \
  --canonical-id PPL-001 \
  --input 'J. Doe' \
  --rule-id MANUAL

printf 'name\nJane Doe\nJ. Doe\n' > "$example_dir/names.csv"
canon "$example_dir/names.csv" \
  --registry "$example_dir/registries/people" \
  --column name \
  --emit csv
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

Save a task-keyed script that just worked, without proof artifacts:

```bash
canon strategy register \
  --registry registries/procurement-strategies/ \
  --task sql_lineage \
  --skill skills/sql-lineage/SKILL.md \
  --script scripts/sql_lineage.py \
  --script-id sql_lineage.v1 \
  --language python \
  --grade operator-attested \
  --operator "$USER" \
  --reason "worked on reviewed sample rows" \
  --next-version 2026.06.25

canon strategy resolve \
  --registry registries/procurement-strategies/ \
  --task sql_lineage \
  --skill skills/sql-lineage/SKILL.md

canon strategy list \
  --registry registries/procurement-strategies/ \
  --key-type task

canon strategy explain \
  --registry registries/procurement-strategies/ \
  --task sql_lineage \
  --skill skills/sql-lineage/SKILL.md
```

Task-keyed resolution is exact: an active `(task, skill_hash)` champion returns `EXACT`; otherwise it returns `UNRESOLVED`. `strategy update`, `strategy deprecate`, and `strategy promote` mutate the registry with explicit version bumps, before/after registry-hash receipts, and witness records unless `--no-witness` is passed. Deprecation removes a champion from active resolution without deleting registry history.

Review frozen-script registry changes before adoption:

```bash
canon strategy diff \
  --old registries/procurement-strategies-v2026.05.01/ \
  --new registries/procurement-strategies-v2026.05.06/ \
  --emit summary
```

Resolve reviewed aliases:

```bash
canon aliases.csv --registry registries/entities/ --column entity_label \
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

## Cross-Source Linkage (`canon entity link`)

`canon entity link` is for the moment before a cross-reference registry exists.
Give it reference rows, target rows, and a YAML strategy that says how fields
correspond. It runs under the entity workbench namespace, preserves
reference/target directionality, filters candidate pairs, scores deterministic
assertions, and emits `canon_entity_link.v1` with
`canon_entity_link_decisions.v1` and a hash-bound
`canon_entity_link_observation_surface_bindings.v1` sidecar.

This is still not the core lookup path. The normal
`canon <INPUT> --registry ...` command does exact lookup only. Link mode is a
workbench for manufacturing audited cross-reference entries that normal lookup
can use later. There is no public compatibility alias for the superseded
standalone resolve namespace.

`--profile` and `--work-dir` stay bracketed in generated-help syntax because
the parser emits a structured `E_ENTITY_INPUT_CONTRACT` refusal when either is
omitted. Successful link execution requires both flags, and omission performs
no writes.

Run the committed fixture corpus as JSON. The `--work-dir` path should be a
fresh temporary directory owned by the caller:

```bash
canon entity link \
  tests/fixtures/resolve/tapes/reference_loans.csv \
  tests/fixtures/resolve/tapes/target_loans.csv \
  --profile cmbs_tenant_label \
  --strategy tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml \
  --registry tests/fixtures/registries/resolve-servicers \
  --work-dir /tmp/canon-entity-link-work \
  --gold tests/fixtures/resolve/gold/loan_matches.jsonl \
  --no-witness
```

Summary mode is compact for operators:

```bash
canon entity link \
  tests/fixtures/resolve/tapes/reference_loans.csv \
  tests/fixtures/resolve/tapes/target_loans.csv \
  --profile cmbs_tenant_label \
  --strategy tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml \
  --registry tests/fixtures/registries/resolve-servicers \
  --work-dir /tmp/canon-entity-link-summary \
  --emit summary \
  --no-witness
```

`--write-back` is accepted by the parser as an explicit handoff request, but
the current public v1 path refuses before work-dir or registry mutation:

```bash
canon entity link <REFERENCE.csv> <TARGET.csv> \
  --profile entity_profile \
  --strategy <STRATEGY.yaml> \
  --registry <REGISTRY_DIR> \
  --work-dir /tmp/canon-entity-link-write-back \
  --suite <SUITE_DIR> \
  --gold <GOLD.jsonl> \
  --write-back
```

Use the emitted artifact with `canon entity review export`, review/import,
audit, promote, and apply to add accepted knowledge through the transactional,
versioned registry path. Link artifacts never authorize direct mutation by
themselves.

Current link-mode limits: exactly two row sets, deterministic local operators,
no network or model runtime, and no direct registry write-back. Registry
knowledge changes go through review, audit, promotion, and exact replay.

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
| `E_PACKAGE_NONCANONICAL` | Package JSON bytes are not canonical compact JSON | Rewrite package JSON as sorted-key compact UTF-8 bytes |
| `E_PACKAGE_CONTRACT` | Package fields, digests, paths, or archive constraints violate the package contract | Fix package metadata or package-root contents |
| `E_ENTITY_PROFILE` | Entity profile is unknown, missing, or semantically invalid | Fix the profile or pass a valid profile path |
| `E_ENTITY_STRATEGY` | Entity strategy YAML is malformed or references unsupported operators | Fix the strategy file |
| `E_ENTITY_INPUT_CONTRACT` | Entity input rows violate the active profile contract | Check required fields and side-field JSON |
| `E_ENTITY_SURFACE_ID_COLLISION` | Distinct prepared surfaces produced the same surface ID | Inspect the profile projection and source rows |
| `E_ENTITY_PATCH_CONFLICT` | Entity alias, distinctness, or relation patches contradict each other or the registry | Fix the conflicting patch input |
| `E_ENTITY_REGISTRY_SNAPSHOT` | Entity registry snapshot does not match the consumed artifact | Re-run against the current registry |
| `E_ENTITY_CACHE_MISMATCH` | Entity cache artifact hashes do not match the current run | Rebuild or bypass the cache |
| `E_ENTITY_INDEX_LIMIT` | Entity index posting, bucket, or top-k limits were exceeded | Tighten blocking or raise the relevant limit |
| `E_ENTITY_CANDIDATE_BUDGET` | Entity candidate budget was exceeded before bounded candidate emission | Tighten filters or raise the candidate budget |
| `E_ENTITY_ARTIFACT_CONTRACT` | Entity artifact has the wrong version, profile, strategy, registry, or hash | Rebuild the artifact chain |
| `E_GEO_COMMAND_UNAVAILABLE` | Geo primary command is planned but not implemented in this build | Use `canon geo capabilities --emit json` |
| `E_ENTITY_CANNOT_LINK_OVERRIDE` | Entity merge request conflicts with a hard cannot-link fact | Remove or justify the conflicting merge request |
| `E_ENTITY_REVIEW_IMPORT` | Entity review import is malformed, stale, or references unknown items | Regenerate the review artifact and decisions |
| `E_ENTITY_AUDIT_GATE` | Entity audit artifact is missing, stale, or failed required gates | Re-run audit and address failed gates |
| `E_ENTITY_APPLY_UNRESOLVED` | Entity apply requires full resolution but unresolved surfaces remain | Use a complete result or allow partial output |
| `E_ENTITY_IO_BUDGET` | Entity row, byte, artifact, memory, or work-dir budget was exceeded | Increase limits or reduce inputs |
| `E_BAD_STRATEGY` | Resolve strategy YAML is malformed or invalid | Fix the strategy file |
| `E_TOO_MANY_CANDIDATES` | Resolve candidate filters left too many candidates | Tighten filters or raise `--max-candidates` |
| `E_EMPTY_TAPE` | Resolve reference or target tape has no processable records | Provide non-empty tapes |
| `E_INCOMPATIBLE_TAPES` | Resolve strategy leaves no comparable fields | Fix strategy field mappings |

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

## Entity Workbench (`canon entity`)

The same entity appears as "Wells Fargo & Company" in one document, "Wells Fargo Bank, N.A." in another, and "WFB" in a third. Three names, one issuer. `canon entity` compiles profiled observations into reviewed registry knowledge through deterministic artifacts — no ML model dependency, no live provider lookup, no open-ended runtime guessing.

The pipeline is YAML-driven: a **strategy file** defines which fields to observe, how to normalize names, which blocking operators generate candidates, how to score evidence, and what thresholds the solver uses to merge, link, review, or abstain. Same strategy + same input + same registry = same artifacts, every time.

`canon entity` is a resolution workbench, not the core lookup path. It manufactures registry knowledge through evidence, audit, review, and promotion. After promotion, ordinary `canon` runs still resolve the resulting aliases through exact lookup.

There are two public entity modes:

- **Cluster mode:** `canon entity run` groups observations inside one profiled corpus and emits solved clusters, escrow, review, and promotion artifacts.
- **Link mode:** `canon entity link <REFERENCE> <TARGET> [--profile <PROFILE>] --strategy <YAML> --registry <DIR> [--work-dir <DIR>] [--suite <DIR>] [--gold <JSONL>] [--write-back] [--emit json|summary] [--cache-mode enabled|disabled] [--max-candidates <N>] [--max-rows <N>] [--max-bytes <N>] [--no-witness]` aligns two row sets through the same typed request and artifact path used by project mode. It is for cross-source linkage, not a hidden alias for edge scoring.

`canon entity alias-withholding --manifest <EXECUTION_ENVELOPE.json>` is an
evaluation compiler for artifact-backed withheld-alias trials. The manifest is
a strict execution envelope: it names a benchmark plus native execution
manifests whose paths are loaded relative to the envelope, including clean
registry, candidate-recall, link/run/solve, review, audit, leak-scan,
assignment-firewall, and optional promotion/exact-replay artifacts. The command
emits a JSON report by default or a compact `--emit summary` report; it refuses
invalid envelopes or stale/missing/tampered artifacts with exit 2. It does not
accept caller-declared decisions or outcomes, does not write registry files, and
does not change ordinary exact lookup semantics. The real clean registry must
exactly match the retained benchmark mappings; leakage and assignment checks
read nonempty concrete source files bound to the validated chain. Credited
attachments additionally require a rebuilt solve review queue, typed native
review-import receipt, exact one-entry sandbox registry diff, and ordinary exact
replay. CLI output hashes identifiers and paths so the public report can retain
counts, statuses, and evidence digests without disclosing source surfaces.

`canon entity generalization --manifest <STRICT_ENVELOPE.json>` is the
artifact-backed evaluation compiler for entity-disjoint and time-forward
discovery trials. Public fixtures and operator-owned private corpora use the
same command. The strict envelope binds the benchmark, native candidate-recall,
link, run, solve, observation/surface sidecar, and leakage-source artifacts by
path, version, and content hash. Strict solve derivation also requires
`solve_derivation.edge_artifact.path`, `solve_derivation.edge_records.path`,
`solve_derivation.prepared_surfaces.path`, and a hash-bound
`solve_derivation.solve_policy` artifact with version
`canon.evaluation.generalization.solve_policy.v0`; the policy file byte hash
must equal `policy_digest`, and the edge/prepared refs are path-bound to the
loaded run `work_dir`. Each trial's `registry_dir` must be manifest-relative
and resolve inside the envelope root; absolute paths, traversal segments, and
symlink registry roots are refused. `run.metadata.registry_snapshot.source` is
retained only as inert metadata continuity and is not opened for replay. The
loaded solve and run are rebuilt exactly from those inputs before scoring. The
report derives decisions, candidate ranks,
false-merge outcomes, and leakage status from those artifacts instead of
accepting caller-authored actual outcomes. The main report is
`canon.evaluation.generalization.v1` and carries a nested `quality` report with
`quality.version` set to
`canon.evaluation.generalization.quality_gate_report.v0`,
`quality.contract_version` set to `canon.entity.quality.v1`, fixed canonical
gate results for `candidate_recall_at_50_min`, `auto_link_precision_min`,
`auto_link_recall_min`, `critical_false_merges_max`, and
`accounted_case_rate_min`, and `quality.release_claim_status` set to `eligible`
or `blocked`. Under `canon.entity.quality.v1`, the fixed thresholds are
candidate recall at 50 `>= 0.995`, auto-link precision `>= 0.995`, auto-link
recall `>= 0.98`, critical false merges `== 0`, and accounted case rate
`== 1.0`; zero-denominator gates are `not_applicable`, which keeps the report
blocked because eligibility requires every gate to pass. The command accepts no
caller-adjustable thresholds or waivers. A structurally valid `blocked` report
is emitted with exit `0`; only malformed or tampered envelopes and artifacts
refuse with exit `2`. The command is read-only, emits JSON or summary, hashes
identifiers, paths, and cutoffs at the CLI boundary, and does not change
ordinary exact lookup semantics.

`canon entity calibrate sweep <RESULT|EVIDENCE> --gold <GOLD.jsonl> --strategy
<STRATEGY.yaml>` is a read-only threshold-selection report compiler for entity
workbench score artifacts. It builds a deterministic integer grid from observed
labeled score units, emits one truth-space row per threshold tuple with rates in
integer basis points, and recommends the tuple that maximizes auto-accept while
satisfying frozen `canon.entity.quality.v1` gates: precision `>= 9950` bps,
recall `>= 9800` bps, and critical false merges `== 0`. If no tuple passes,
`recommendation.status` is `blocked` and no fallback tuple is selected. The
report includes a proposed YAML threshold fragment for manual application only;
the command never writes registry files, work directories, witness ledgers, or
the strategy file.

The native entity workbench cache contract records `canon_entity_index_cache_receipt.v0`
stage artifacts. Public `canon entity run` and `canon entity link` expose
`--cache-mode enabled|disabled`, defaulting to `enabled`. A genuine warm cache
hit requires an enabled, reusable receipt and is reported as `cache_enabled` with
status `hit`; disabled cache mode bypasses reuse and records a non-reusable
`cache_disabled` receipt with status `bypassed`. The receipt bundle hash covers
the complete index bundle bytes: index artifact, cache key, postings, and
diagnostics. Run/link artifacts bind the cache receipt stage to the native index
stage, refuse stale or tampered bundles before reuse, and preserve semantic
outputs across enabled and disabled cache modes.

Link observation IDs and prepared `surface_id` values are separate namespaces.
The link artifact therefore carries a hash-bound observation/surface sidecar;
alias-withholding re-derives it from the materialized rows plus the supplied run
profile/strategy before joining candidate, solve, review, and promotion evidence.
Reports distinguish `evaluated_pair`, `prepared_surface_collapse`, and
`relation_policy_control`. A prepared-surface collapse binds two distinct link
observations to one derived surface, receives no candidate rank or recall
denominator credit, and is reported as collapse/accretion rather than retrieval.
Non-identity relation-policy controls are also excluded from candidate recall,
forbid promotion/replay, and surface any automatic attachment as an
`unsupported_guess` false merge.

`canon entity review export` defaults to the existing queue review contract:
`--artifact queue --emit json|csv` keeps the v1/legacy review queue behavior and
does not change the default review path. Explicit
`--artifact native-review --group-by signature --emit json|csv|html` emits
`canon_entity_native_review.v0` for native solve, run, or link artifacts. The
native artifact carries a self-hash, binds run/policy/registry context, and
validates exact mode-specific context. Signature grouping carries deterministic
group counts, first-N sample review IDs, and score stats as presentation data
only; decisions still expand to individually hash-bound review IDs. Candidate-free
unmatched directional link items carry `right_surface_id: null` and allow only
defer actions. The HTML projection is static and offline; the canonical decision
data remain a deterministic JSON/CSV envelope that other frontends can produce.

`canon entity review import` keeps the positional decisions file plus required
`--registry` and `--next-version` compatibility arguments. Without
`--source-review`, it imports the existing queue decisions. With
`--source-review <canon_entity_native_review.v0>`, it treats the positional file
as native JSON or CSV decisions, expands JSON `group_decisions` against that
source review artifact, verifies the source review self-hash and exact
mode-context binding for every expanded member, and emits a typed
`canon_entity_native_review_import.v0` patch receipt only. The native path does
not read or mutate the registry, and does not consume `--audit` or `--next-version` beyond Clap-required
compatibility. For a derivation-proven collapse, a singleton cluster Alias
decision is valid only with an explicit target canonical ID and exact
exported-surface equality.

```bash
# Full pipeline in one command:
$ canon entity run rows.csv \
    --profile entity_profile \
    --strategy strategy.yaml \
    --registry registries/entities/ \
    --work-dir work/entity-run \
    --suite eval/holdout/ \
    --emit summary

entity_run: 847 rows → 312 canonical entities, 4 escrow (pending), 0 escrow (conflict)
audit: holdout 98/98 pass, perturbation stability 0.998
```

Or run stages individually for inspection:

```bash
$ canon entity prepare rows.csv --profile entity_profile --registry registries/entities/ --work-dir work/entity
$ canon entity index build rows.csv --profile entity_profile --strategy strategy.yaml --registry registries/entities/ --work-dir work/entity
$ canon entity block preflight rows.csv --profile entity_profile --strategy strategy.yaml --sample-pct 100 --emit summary
$ canon entity block rows.csv --profile entity_profile --strategy strategy.yaml --registry registries/entities/ --work-dir work/entity > blocks.jsonl
$ canon entity evidence rows.csv --profile entity_profile --strategy strategy.yaml --candidates blocks.jsonl --registry registries/entities/ --work-dir work/entity > evidence.jsonl
$ canon entity solve rows.csv --profile entity_profile --strategy strategy.yaml --evidence evidence.jsonl --registry registries/entities/ --work-dir work/entity > result.json
$ canon entity audit result.json --suite eval/holdout/ > audit.json
$ canon entity review export result.json --include all --emit csv > review.csv
$ canon entity review export result.json --artifact native-review --group-by signature --include all --emit html > native-review.html
$ canon entity review import review.csv --audit audit.json --registry registries/entities/ --next-version 2.1.0
$ canon entity promote result.json --audit audit.json --registry registries/entities/ --next-version 2.1.0
$ canon entity explain result.json --canon-id IC-00042
```

---

## The Entity Pipeline

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

### Evidence

Typed evidence scoring. Each candidate pair receives lane-tagged evidence:

- **Support** — scored positive evidence (exact name view match, acronym-plus-token, categorical field equality)
- **Trusted anchors** — explicit configured identifiers that can support equality when the profile says they are identity anchors
- **Cannot-link** — negative evidence (conflicting anchor values in the same namespace)
- **Relation hints** — relationship, hierarchy, or contextual evidence that may explain why records co-occur but must not silently become equivalence merge evidence

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

### Review

Export resolved, escrowed, or contradictory clusters into JSON or CSV review artifacts. Each item carries a stable review ID, source row IDs, observed names, anchors, incumbent overlaps, evidence scores, contradiction reasons, and a proposed action. Importing a reviewed artifact refuses malformed or duplicate decisions, stale registry snapshots, anchor conflicts, and alias/anchor promotion decisions without a matching audit.

### Promote

Write audited results back to the registry:

- Resolved entities get alias entries added to registry mapping files
- Escrow sidecars are written for entities that need human review
- Requires an explicit `--next-version` bump

### Explain

Proof traces for any row, entity, or escrow decision:

```bash
$ canon entity explain result.json --row src-row-42
$ canon entity explain result.json --canon-id IC-00042
$ canon entity explain result.json --escrow-id ESC-00007
```

Returns the full evidence chain: which blocking operator surfaced the pair, which evidence edges were scored, which solver stage produced the merge or abstention, and why.

---

## Limitations

| Limitation | Detail |
|------------|--------|
| **Exact match only (core lookup)** | Core `canon` lookup uses exact byte match after ASCII-trim. `canon entity` cluster and link modes add deterministic workbenches, not fuzzy/phonetic matching in the lookup kernel. |
| **Flat registries** | No subdirectories in v0. All mapping files must be at the registry root. |
| **CSV-only for `--emit csv`** | JSONL input cannot use `--emit csv` mode. |

---

## FAQ

### Why "canon"?

Short for *canonical*. The tool produces canonical identifiers — one true ID for each entity, traceable to a versioned registry.

### Is this entity resolution?

Yes. `canon entity` performs deterministic profiled entity resolution. Cluster
mode groups observations inside one corpus, and link mode aligns reference and
target row sets for structural record linkage. Both use YAML-driven evidence
pipelines and emit audit artifacts. Core `canon` without a workbench subcommand
still resolves identifiers via exact lookup against versioned registries.

The important boundary is that entity resolution happens in workbench commands
under `canon entity`, then accepted knowledge is promoted into registries. The
default lookup command never performs open-ended fuzzy matching at resolution
time.

### How does canon relate to rvl?

[`rvl`](https://github.com/cmdrvl/rvl) explains numeric changes between CSV files. `canon` normalizes identifiers so `rvl` can align rows that use different ID schemes. The pipeline: `canon --emit csv | rvl`.

### How does canon relate to shape?

[`shape`](https://github.com/cmdrvl/shape) checks structural compatibility between files. `canon` resolves identifiers within a single file. Use `shape` to verify structure, `canon` to normalize IDs, then `rvl` to explain changes.

### What about registries — do I have to build them?

You can consume published registries, materialize provider-backed registries with `canon registry build`, or maintain local self-authored registries with `canon registry mint` and `canon registry add-entry`. Prefer the maintenance commands over hand-editing mapping JSON: they preserve exact lookup semantics while keeping versions, entry counts, and lint checks in sync. The build workflow snapshots provider-backed lookups into a normal versioned registry directory plus `_build.json` provenance, and normal `canon` resolution ignores that metadata sidecar. For OpenFIGI provider work, use a local `twinning rest` fixture to model success, no-match, error, and malformed-provider cases before running a live maintenance build.

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
  "redacted": true,                        // present on RESOLVED/PARTIAL/UNRESOLVED; true when values are masked. Re-run with --explicit to reveal. Absent on REFUSAL.
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

This lossless encoding remains the default for `--explicit` JSON audit
artifacts. For CLI and human workflows that want ordinary strings, pass
`--explicit --plain-json-values`; valid UTF-8 values are emitted without the
`u8:` prefix and each value gets `input_encoding` / `canonical_id_encoding`
metadata; unresolved non-null inputs use `input_encoding`. Values that cannot
be represented safely as text still use `hex:`.

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

For the full toolchain guide, see the [Agent Operator Guide](https://github.com/cmdrvl/.github/blob/main/profile/AGENT_PROMPT.md). Shared repo instructions live in [AGENTS.md](./AGENTS.md); harness-specific notes live in [CODEX.md](./CODEX.md), [CLAUDE.md](./CLAUDE.md), and [GEMINI.md](./GEMINI.md). Run `canon --describe` for this tool's machine-readable contract.

For Geo work, read the [agent operating architecture](./docs/CANON_GEO_AGENT_ARCHITECTURE.md)
and then the [mathematical and empirical plan](./docs/PLAN_CANON_GEO.md). The operating
order is question/profile/inventory -> bounded tile+halo -> candidate reach -> rho
admission -> incidence components -> exact residuals -> reconciliation -> separately
reported coverage/reach/solver/truth/cost planes. Source instances belong in adapters and
regional inventories; Canon core does not require a named vendor or, architecturally, a
parcel layer.

The Geo planner and target run view reuse Canon's shared project
manifest/lock/plan/run/receipt substrate; they are not a second orchestration engine.
`canon geo plan` now ships as an offline/read-only planner over one validated
`canon.project.plan.v1` DAG. `canon geo run` now ships as the bounded offline run
projection over that DAG: with optional explicit local home-cell rows, tile-work request,
and warehouse rows, it runs materialize-home-cells -> tile-work -> materialize-evidence ->
compile-evidence -> solve through the shared project runner and emits `canon_geo_run.v0`
over `canon.project.run.v2` receipts. Without required local inputs, it can still return a
typed `WAITING_FOR_INPUT` run. It resumes validated completed outputs, refuses undeclared
commands or compile/solve input overrides, and treats `--satisfy` as receipt/explicit-byte
consistency validation only, not plan mutation or replanning. To advance inventory from an
already acquired live receipt, `canon geo replan-from-acquisition` validates one receipt
against explicit local artifacts, atomically writes a separate
`canon_geo_regional_inventory_advancement.v0` sidecar, and emits a new
base-inventory-bound `canon_geo_plan.v0`.

`canon geo capabilities --emit json` ships as a deterministic, offline description of the
compiled Geo control contracts. Use `canon geo plan --question <QUESTION.json>
--capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json>
--budget <BUDGET.json>` to emit `canon_geo_plan.v0`. The current composition profile is
parcel/building only; candidate reach remains independently unverified unless the inputs
name a reference that proves it.

Drive Geo from the primary surface: `geo capabilities`, `geo plan`, `geo run`,
`geo replan-from-acquisition`, and `geo evaluate` today, plus `geo inspect` and
`geo ledger` once they ship. The eleven stage leaves and three measurement adapters remain
independently callable and machine-described via `canon --describe`, but they are stage and
harness surfaces — reach for them for Demo 0, single-stage debugging, and tests, not as the
normal way to ask Geo a question. See [Geo command surface](#geo-command-surface).

Geo run is not live acquisition or live proof: acquisition remains external, exactness is
representation-relative, candidate reach is upstream, immutable cross-release same-workdir
reuse is not guaranteed, E5/live scale proof is not shipped, and the primary-surface `geo inspect` and `geo ledger` verbs remain unimplemented.

---

## Spec

The full specification is `docs/PLAN_CANON.md`. This README covers everything needed to use the tool; the spec adds implementation details, edge-case definitions, and testing requirements.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

---

*`canon` is part of the open-source toolchain from the [CMD+RVL](https://cmdrvl.com) lineage and AI enablement practice. MIT-licensed. Contributions welcome from any practice or stack.*
