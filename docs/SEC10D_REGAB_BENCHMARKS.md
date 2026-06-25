# sec10d Reg AB Firm Benchmark Suite

> Baseline artifact:
> `/Users/zacharyruiz/Downloads/sec10d_regab_org_canon_baseline_20260623T204557Z.zip`
> SHA-256:
> `5766b83bb2e1bad3736b1d78fa7ea1433d929d1f3d936762fdfbdba7cc9bdf3b`
> Purpose: benchmark `canon entity` use case #2, `regab_firm_identity`,
> against the current `sec10d` Reg AB organization enrichment baseline.

This benchmark suite is designed for the second validated `canon entity` use
case: Reg AB firm identity and reviewed alias canonicalization for the
`sec10d` pipeline. It should prove that the future entity workbench can accept
the existing `org_mentions.csv` shape, preserve parser evidence, replay the
reviewed firm registry exactly, and avoid silently collapsing hierarchy-like
relationships such as bank parent vs loan-services division.

The baseline data is public, but the full zip is not necessary in the repo. The
committed fixture contract is:

- `tests/fixtures/entity/regab/sec10d_regab_benchmark_manifest.json`;
- `tests/fixtures/entity/regab/sec10d_baseline_public/`, a compact public-data
  slice with one real row per observed firm surface, exact expected lookup
  output, small enriched JSONL samples, and the registry snapshot needed for
  normal CI.

The rest of the full benchmark remains an operator tier that runs when the zip
is available locally or in a public-data fixture cache.

Do not turn this into artifact-exists testing. Every benchmark below has a
behavioral assertion.

---

## Baseline Profile

The baseline zip contains:

| Artifact | Expected value |
|----------|----------------|
| Registry id | `firms` |
| Registry version | `1.0.12` |
| Mention rows | 127,991 |
| Unique source row ids | 127,991 |
| Duplicate source row ids | 0 |
| Unique raw firm surfaces | 46 |
| Unique canonical ids observed | 31 |
| Resolved mentions | 127,991 |
| Unresolved mentions | 0 |
| Accessions | 19 |
| Deal keys | 7,030 |
| Source exhibits | 256 |

Mention rows by dataset and field:

| Dataset field | Mentions |
|---------------|----------|
| `regab_attestations.attesting_firm_name` | 172 |
| `regab_attestations.subject_party_name` | 172 |
| `regab_platform_rosters.reporting_party_name` | 32,055 |
| `regab_servicer_schedules.reporting_party_name` | 47,796 |
| `regab_servicer_schedules.servicer_name` | 47,796 |

Enriched output record counts:

| Enriched dataset | Records |
|------------------|---------|
| `regab_attestations.jsonl` | 172 |
| `regab_platform_rosters.jsonl` | 32,055 |
| `regab_servicer_schedules.jsonl` | 47,796 |

The profile identity semantics are:

```yaml
profile: regab_firm_identity
entity_type: organization
identity_semantics: same_firm_or_reviewed_alias
canonical_type: org
```

This is not tenant-label identity. It must not inherit CMBS tenant display-label
merge behavior.

---

## Input Contract

`canon entity prepare` should accept the baseline `org_mentions.csv` shape
directly:

```text
source_row_id
record_id
dataset
record_version
field_name
org_name
doc_id
as_of_date
filing_cik
accession
filing_form
filed_date
period
source_exhibit_document_name
source_exhibit_type
source_item
role_context
capacity
capacity_normalized
reporting_party_capacity
platform_capacity
platform_capacity_normalized
subject_role
deal_key
transaction_name
alias_surfaces_json
mention_surfaces_json
```

The canonical CSV output appends exactly:

```text
org_canon_id
```

The enriched JSONL outputs append only approved Snowflake-facing canonical
fields for each firm-bearing source field:

```text
*_org_canon_id
*_org_canonical_name
*_org_resolution_status
*_org_registry_id
*_org_registry_version
*_org_rule_id
```

Raw parser fields remain byte-preserved.

---

## Benchmark Tiers

### Tier 0: Small Golden CI

Small, committed fixtures selected from this baseline. These should run in
normal CI and must be inspectable. The primary fixture root is
`tests/fixtures/entity/regab/sec10d_baseline_public/`.

Required checks:

- accepted `org_mentions` shape;
- all 46 observed surfaces are represented by a real selected row;
- raw parser field preservation;
- PNC vs Midland anti-collapse;
- Wells Fargo bank vs Wells Fargo Commercial Mortgage Servicing anti-collapse;
- reviewed punctuation, case, prefix, predecessor, and subsidiaries aliases;
- append-only enriched JSONL fields;
- exact replay with stable registry id/version/rule metadata.

### Tier 1: Full Baseline Zip

Run against the full baseline zip when available.

Required checks:

- baseline zip and inner file hashes match the manifest;
- 127,991 mentions and 46 unique surfaces are read;
- all 46 surfaces resolve to the exact canonical id and rule id listed in the
  manifest;
- 31 canonical ids have the expected mention-count distribution;
- review queue is header-only because the baseline is fully resolved;
- enriched datasets preserve record counts and append only the approved
  canonical fields;
- row-order replay is deterministic.

### Tier 2: Regression And Drift

Operator or ignored tier for future refreshed `sec10d` baselines.

Required checks:

- any new unresolved surfaces become grouped review work, not silent aliases;
- any reviewed alias changes require a manifest update and operator decision;
- parent/division/subsidiary wording remains review-sensitive unless an explicit
  reviewed policy rule maps it;
- exact apply remains the production replay path after promotion.

---

## Benchmarks

### `REGAB-SRC-001`: Baseline Fingerprint

Input: the full baseline zip.

Expected:

- zip SHA-256 equals
  `5766b83bb2e1bad3736b1d78fa7ea1433d929d1f3d936762fdfbdba7cc9bdf3b`;
- inner artifact hashes match the manifest;
- required files are present under `org_canon_baseline/`.

Failure meaning:

- The benchmark is not running against the expected baseline.

### `REGAB-OBS-001`: Mention Shape And Counts

Input: `org_mentions.csv`.

Expected:

- 127,991 mention rows;
- 27 input columns, exactly as listed above;
- 127,991 unique `source_row_id` values;
- 46 unique `org_name` surfaces;
- field and dataset counts match the manifest;
- `alias_surfaces_json` and `mention_surfaces_json` parse as JSON arrays.

Failure meaning:

- The sec10d input contract changed or `canon entity prepare` is losing
  provenance/context.

### `REGAB-OBS-002`: Committed Public Fixture Slice

Input: `tests/fixtures/entity/regab/sec10d_baseline_public/`.

Expected:

- `org_mentions_selected.csv` has 46 rows and 46 unique `org_name` surfaces;
- `org_mentions_selected.canon.csv` has the same rows plus `org_canon_id`;
- selected rows cover all 31 canonical ids observed in the full baseline;
- `org_lookup_expected.map.json` has the 46 expected mappings;
- the selected fixture root includes the small `firms` registry snapshot;
- enriched sample JSONL files cover all three enriched dataset contracts.

Failure meaning:

- The committed CI fixture is no longer representative of the full public
  baseline.

### `REGAB-LOOKUP-001`: Exact Mapping Parity

Input: `org_mentions.canon.csv` and `org_lookup.map.json`.

Expected:

- `canon_exit` is 0;
- all 46 unique surfaces resolve;
- no unresolved mappings;
- every surface maps to the canonical id, rule id, and mention count listed in
  the manifest;
- canonical id mention distribution matches the manifest.

Failure meaning:

- Registry lookup parity broke, or a reviewed alias changed without an explicit
  migration decision.

### `REGAB-APPLY-001`: CSV Append-Only Replay

Input: `org_mentions.csv` and `org_mentions.canon.csv`.

Expected:

- canonical CSV row count equals input row count;
- input columns are byte-preserved and in the same order;
- output appends exactly one column, `org_canon_id`;
- no row reordering;
- no blank `org_canon_id` values.

Failure meaning:

- Exact replay is mutating parser evidence or changing downstream CSV shape.

### `REGAB-ENRICH-001`: JSONL Append-Only Enrichment

Input: `enriched_datasets/*.jsonl`.

Expected:

- enriched record counts match the manifest;
- raw parser fields remain present and unchanged;
- only approved `*_org_*` canonical fields are appended;
- registry id/version and rule id are present for each resolved firm-bearing
  field.

Failure meaning:

- The sec10d Snowflake-facing contract has drifted.

### `REGAB-HIER-001`: PNC And Midland Anti-Collapse

Real baseline facts:

```text
PNC Bank, National Association -> ORG-034
Midland -> ORG-035
Midland Loan Services, a division of PNC Bank, National Association -> ORG-035
Servicer compliance statement, Midland Loan Services, a division of PNC Bank, National Association -> ORG-035
```

Expected:

- PNC and Midland surfaces remain distinct canonical ids;
- division wording may create relation/review context, but it must not merge
  Midland into PNC;
- any future ML/evidence path must represent this as anti-collapse or reviewed
  alias policy, not generic string similarity.

Failure meaning:

- The workbench is confusing hierarchy or affiliation with same-firm identity.

### `REGAB-HIER-002`: Wells Fargo Bank And Servicing Division Anti-Collapse

Real baseline facts:

```text
Wells Fargo -> ORG-012
Wells Fargo Bank, National Association -> ORG-012
Wells Fargo Commercial Mortgage Servicing, a division of Wells Fargo Bank, National Association -> ORG-053
```

Expected:

- Wells Fargo bank surfaces stay distinct from the commercial mortgage
  servicing division surface;
- division text does not auto-collapse to the parent bank;
- relation hints are allowed, merge evidence is not.

Failure meaning:

- Parent/division semantics are leaking into same-entity resolution.

### `REGAB-ALIAS-001`: Reviewed Alias Rule Coverage

Expected rule counts:

| Rule id | Mapping count |
|---------|---------------|
| `REGAB_EXACT_ALIAS` | 26 |
| `CMBS_COUNTERPARTY` | 5 |
| `REGAB_PUNCTUATION_VARIANT_ALIAS` | 4 |
| `REGAB_PREFIXED_COMPLIANCE_STATEMENT_ALIAS` | 4 |
| `REGAB_DIVISION_ALIAS` | 3 |
| `FIRM_NAME_TO_FIRM` | 1 |
| `REGAB_CASE_VARIANT_ALIAS` | 1 |
| `REGAB_REVIEWED_PREDECESSOR_ALIAS` | 1 |
| `REGAB_SUBSIDIARIES_ALIAS` | 1 |

Failure meaning:

- A reviewed policy lane changed, disappeared, or was replaced by generic fuzzy
  behavior.

### `REGAB-REVIEW-001`: Review Queue Semantics

Input: `org_review_queue.csv`.

Expected:

- header-only review queue for this baseline because unresolved mentions are 0;
- if future refreshed baselines contain unresolved or hierarchy-sensitive
  surfaces, they must group into review items with counts, fields, examples, and
  proposed action;
- review work must not be hidden by automatic parent/subsidiary collapse.

Failure meaning:

- Unresolved handling is either noisy or unsafe.

### `REGAB-PREP-001`: Dedupe-First Surface Preparation

Input: full baseline and a shuffled copy.

Expected:

- 46 prepared unique surfaces before exact resolution;
- prepared surface ids and summaries are row-order deterministic;
- raw row ids remain provenance only;
- candidate generation operates on unique surfaces, not 127,991 raw rows.

Failure meaning:

- The workbench will not scale or will produce row-order-sensitive results.

### `REGAB-PERF-001`: Structural Performance Guard

Input: full baseline.

Expected:

- no all-pairs generation over 127,991 mention rows;
- exact-resolved surfaces skip unnecessary fuzzy work;
- unresolved/reviewable work is bounded by unique surfaces and profile caps;
- telemetry records rows, unique surfaces, resolved surfaces, artifact sizes,
  cache status, and timings.

Failure meaning:

- The migration is operationally weaker than the current exact helper.

### `REGAB-FIREWALL-001`: Cross-Profile Semantic Firewall

Expected:

- `regab_firm_identity` artifacts cannot be reviewed/promoted/applied into
  `cmbs_tenant_label` registries and vice versa;
- tenant-label display merges do not affect Reg AB firm identity;
- hierarchy output, if any, is relation context for the ontology layer, not
  same-as inside canon.

Failure meaning:

- Canon is mixing profile semantics and creating unsafe cross-domain identity.

---

## Non-Negotiables

- No frontier model calls.
- No network access.
- No Python or general ML framework runtime.
- No runtime model downloads.
- No silent parent/subsidiary/division collapse.
- Raw sec10d parser evidence stays unchanged.
- Exact core lookup remains the production replay path after promotion.
- Reviewed alias policy changes require explicit benchmark/manifest updates.
