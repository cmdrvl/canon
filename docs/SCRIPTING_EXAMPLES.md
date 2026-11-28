# canon scripting examples

Worked shell examples for lookup, registry maintenance and the older entity
commands. Moved here from the README.

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

For a Parquet seed or key crosswalk, add `--parquet` and use a `.parquet` output path. CSV remains the default; `--parquet` is refused for `search-index` and other export formats.

```bash
canon registry export \
  --format dbt-seed \
  --registry registries/funds/ \
  --namespace funds \
  --out seeds/canon_funds.parquet \
  --parquet

canon registry export \
  --format key-crosswalk \
  --registry registries/funds/ \
  --out artifacts/funds.keys.parquet \
  --parquet
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

canon registry mint \
  --registry "$example_dir/registries/properties" \
  --canonical-type cmbs_property \
  --prefix PROP \
  --scope deal=CIK1690255 \
  --with-alias 'properties.json=41-001:absee_property_alias'

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
