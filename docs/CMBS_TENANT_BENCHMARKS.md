# CMBS Tenant Benchmark Suite

> Source sample: `/Users/zacharyruiz/Downloads/tenant_sample_cmbs.csv`
> SHA-256: `34f9ce6be4e941ea04899299c99aa409ac6e833da3208495e4ab4322e57e0e00`
> Purpose: benchmark `canon entity` use case #1, `cmbs_tenant_label`, against
> real public CMBS tenant strings without test theatre.
> Shared eval/performance contract: `docs/ENTITY_EVALS_AND_PERFORMANCE.md`

This benchmark suite is designed for the first validated `canon entity` use
case: canonical tenant-label authoring from CMBS property tenant fields. It
should prove that the workbench can extract, normalize, block, score, review,
promote, and replay tenant labels without over-merging subtly different
entities or hiding scale problems.

The full raw CSV does not need to be committed by default. The committed fixture
contract should include:

- a small hand-labeled manifest, currently
  `tests/fixtures/entity/cmbs/tenant_sample_benchmark_manifest.json`;
- deterministic fixture generators/selectors that can derive subsets from the
  public sample;
- small golden outputs for CI;
- ignored/operator benchmark commands for the full sample and generated stress
  cases.

Do not turn this into artifact-exists testing. Every benchmark below has a
behavioral assertion.

---

## Source Profile

The public sample has:

| Metric | Value |
|--------|-------|
| CSV data rows | 6,000 |
| CSV columns | 74 |
| Tenant slots | 18,000 |
| Non-empty tenant observations | 10,143 |
| Blank tenant slots | 7,857 |
| Unique raw tenant names | 431 |
| Distinct property keys observed | 249 |
| Repeated property keys | 228 |

Tenant slots are:

| Rank | Tenant field | Square feet field | Lease expiration field |
|------|--------------|-------------------|------------------------|
| 1 | `LARGESTTENANT` | `SQUAREFEETLARGESTTENANTNUMBER` | `LEASEEXPIRATIONLARGESTTENANTDATE` |
| 2 | `SECONDLARGESTTENANT` | `SQUAREFEETSECONDLARGESTTENANTNUMBER` | `LEASEEXPIRATIONSECONDLARGESTTENANTDATE` |
| 3 | `THIRDLARGESTTENANT` | `SQUAREFEETTHIRDLARGESTTENANTNUMBER` | `LEASEEXPIRATIONTHIRDLARGESTTENANTDATE` |

Required observation shape:

```text
source_row_id
source_file_sha256
csv_line_number
filing_id
form_submission_id
filing_date
company_name
asset_number
property_name
property_address
property_city
property_state
property_zip
tenant_rank
tenant_name_raw
tenant_square_feet
tenant_lease_expiration
```

`source_row_id` must be provenance only. Candidate generation operates on
prepared unique surfaces, not raw rows.

Recommended `source_row_id`:

```text
tenant_sample_cmbs:<csv_line_number>:<tenant_rank>:<filing_id>:<asset_number>
```

---

## Benchmark Tiers

### Tier 0: Small Golden CI

Small, committed, hand-labeled fixtures selected from this file. These should
run in normal CI and must be deterministic.

Required checks:

- input contract and extraction counts for the selected rows;
- namekit normalization goldens;
- must-link candidate recall;
- hard-negative anti-merge;
- review grouping;
- exact replay over promoted aliases;
- raw-field preservation.

### Tier 1: Full Public Sample

Run against the full 6,000-row public sample when the file is available locally
or in a public-data fixture cache.

Required checks:

- 10,143 tenant observations extracted;
- 431 unique raw tenant names before normalization;
- `0` placeholder handling;
- row-order determinism;
- bounded candidate counts;
- exact-bucket compactness;
- review group counts and top unresolved/anti-merge summaries.

### Tier 2: Generated Stress

Synthetic generators extend the real sample shape:

- 500k tenant rows with repeated CMBS-style names;
- 500k unique tenant surfaces;
- one pathological exact bucket;
- common-token/posting-list cap cases;
- typo/noise expansion around the real goldens.

Stress tests are ignored/operator tier until baselines exist. Commit generators
and small goldens, not giant generated CSVs.

---

## Benchmarks

### `CMBS-SRC-001`: Source Fingerprint

Input: the full public CSV.

Expected:

- SHA-256 equals
  `34f9ce6be4e941ea04899299c99aa409ac6e833da3208495e4ab4322e57e0e00`;
- 6,000 data rows;
- 74 columns;
- tenant slot fields are present exactly as listed above.

Failure meaning:

- The benchmark is not running against the expected sample.

### `CMBS-EXTRACT-001`: Tenant Slot Extraction

Input: the full public CSV.

Expected:

- 18,000 candidate slots;
- 10,143 non-empty tenant observations;
- 7,857 blank slots skipped with deterministic counts;
- 431 unique raw tenant names;
- each emitted observation carries rank, square feet, lease expiration, filing,
  property, and asset provenance where present.

Failure meaning:

- The profile is losing tenant slots, treating blanks as tenants, or failing to
  preserve context needed by review and replay.

### `CMBS-EXTRACT-002`: Placeholder Zero Handling

Real sample fact:

```text
raw tenant name "0" appears 785 times
all occurrences are rank 1
square feet is blank in sampled contexts
```

Expected:

- `0` is not promoted into a canonical tenant label;
- it is classified as placeholder/noise/unresolved with a stable reason code;
- it may appear in summary diagnostics;
- it must not create a large exact bucket that becomes a tenant entity.

Failure meaning:

- The system is manufacturing entities from placeholders.

### `CMBS-PREP-001`: Dedupe-First Prepared Surfaces

Input: full sample and a shuffled copy of the same rows.

Expected:

- prepared surface IDs are byte-identical across row order;
- summary counts are identical;
- provenance sample ordering is deterministic and bounded;
- raw row IDs do not participate in candidate identity.

Failure meaning:

- The workbench is row-order-sensitive or using row IDs as identity.

### `CMBS-NK-001`: Must-Link Normalization Goldens

These clusters should be candidates and should resolve to the same tenant label
after review/promotion policy permits. Safe display-label merges may auto-merge
when no hard anti-merge fires.

| Cluster | Expected label | Total obs | Raw variants |
|---------|----------------|-----------|--------------|
| `TNT-238-SAND-ISLAND-PROPERTY` | 238 Sand Island Property | 1,072 | `238 Sand Island Prop`; `238 SAND ISLAND PROPERTY LLC`; `238 Sand Island Property LLC` |
| `TNT-2020-AUTO-BODY` | 2020 Auto Body | 336 | `2020 Auto Body  LLC`; `2020 Auto Body, LLC` |
| `TNT-24-HOUR-FITNESS` | 24 Hour Fitness | 681 | `24 Hour Fitness`; `24 HOUR FITNESS`; `24 HOUR FITNESS USA INC`; `24 HOUR FITNESS USA, INC.`; `24 HR FITNESS`; `24 HR Fitness` |
| `TNT-FOOT-LOCKER` | Foot Locker | 147 | `FOOT LOCKER RETAIL`; `-STORE 2:Foot Locker`; `Foot Locker` |
| `TNT-TJ-MAXX` | TJ Maxx | 42 | `T J Maxx`; `T.J. MAXX`; `T.J. Maxx`; `TJ Maxx #0108` |
| `TNT-10X-GENOMICS` | 10x Genomics | 77 | `10x Genomics Inc.`; `10x Genomics  Inc.`; `10X GENOMICS, INC.`; `10x Genomics, Inc.` |
| `TNT-23ANDME` | 23andMe | 104 | `23 AND ME INC`; `23andMe Inc`; `23andME  Inc` |
| `TNT-1-LIFE-HEALTHCARE` | 1 Life Healthcare | 95 | `1 Life Healthcare Inc`; `1 Life Healthcare, Inc.`; `1Life Healthcare`; `1Life Healthcare, Inc. LIFE` |
| `TNT-TAVERN-BOWL` | Tavern & Bowl | 37 | `Tavern & Bowl`; `TAVERN & BOWL`; `Tavern & Bowl (Costa Mesa55 Tavern & Bowl. LLC)` |
| `TNT-PANGAEA-OUTPOST` | Pangaea Outpost | 70 | `Pangaea Outpost`; `Pangaea Outpost  LLC`; `PANGAEA OUTPOST` |
| `TNT-TWO-TAILS` | Two Tails | 73 | `TWO TAILS`; `Two Tails LLC`; `Two Tails, LLC` |
| `TNT-ETHOS-LENDING` | Ethos Lending | 54 | `Ethos Lending,`; `Ethos Lending`; `Ethos Lending LLC`; `Ethos Lending  LLC.` |
| `TNT-MGA-ENTERTAINMENT` | MGA Entertainment | 132 | `150 - MGA Entertainment Inc., a California corporation`; `MGA Entertainment Inc., a California corporation` |

Expected evidence:

- lossy normalization reason codes for legal suffixes, punctuation, repeated
  spaces, all-caps fold, store-number stripping, and leading record number
  stripping where applicable;
- support from exact normalized views and token/ngram similarity;
- no dependency on frontier models or network calls.

### `CMBS-CAND-001`: Candidate Recall For Goldens

For every raw variant pair inside `CMBS-NK-001`:

- the pair appears in candidates, or both variants are already joined by a
  compact exact-bucket assertion;
- candidate evidence explains which operator recovered it;
- no must-link pair is dropped because of common-token caps.

Failure meaning:

- Blocking is too brittle; the solver never has the chance to make the correct
  decision.

### `CMBS-HARDNEG-001`: Numeric And Address-Like False Positives

Pairs that must not auto-merge:

| Left | Right | Required outcome |
|------|-------|------------------|
| `2020 Auto Body, LLC` | `2020 Broadway Ave` | cannot-link or no edge; never same tenant |
| `100 Riverside Parking LLC` | `220 Riverside Parking LLC` | cannot-link/review distinct |
| `100 Riverside Parking LLC` | `100 Forsyth Restaurant LLC` | cannot-link/review distinct |
| `1OAK` | `1 Life Healthcare Inc` | cannot-link/no edge despite shared numeric token |
| `13 Rattles` | `137 VENTURES MANAGEMENT  LLC` | cannot-link/no edge despite prefix overlap |
| `24 Hour Fitness` | `24 Hour Club` | review/distinct unless explicit alias patch says otherwise |
| `Triangle Cinemas` | `TIME NIGHT CLUB` | same-property context must not imply same tenant |
| `1460 Broadway Tenant` | `FOOT LOCKER RETAIL` | same-property context must not imply same tenant |
| `MGA Entertainment Inc., a California corporation` | `San Fernando Valley Mental Health Center, Inc.` | same-property context must not imply same tenant |

Expected:

- high string/token similarity or same-property context is not enough to merge;
- anti-merge evidence includes address-like-token conflict, numeric-anchor
  conflict, different-brand token, same-property-distinct-rank, or profile
  policy reason;
- hard negatives either never reach auto-merge or land in review/escrow.

### `CMBS-REVIEW-001`: d/b/a And Slash Review Cases

These are not failures. They are exactly the ambiguous cases the review queue
should surface once, with enough context to decide:

| Case | Raw variants | Expected |
|------|--------------|----------|
| China King Buffet | `Chen and Lin 88, LLC dba China King Buffet`; `Chen and Lin 88, LLC dba China IGng Buffet` | candidate + typo support + d/b/a reason; review or safe merge after policy |
| Randall's / Tom Thumb | `(GrL) Randall's Food and Drugs, L.P. / Tom Thumb (Store #1972-00)`; `(GrL) Randall's Food and Drugs, L.P. /Tom Thumb (Store #1972-00)` | grouped review; slash relation preserved |
| WeWork tenant shells | `1460 Broadway Tenant - WEWORK`; `1460 Broadway Tenant LLC - WeWork`; `222 Kearny Street Tenant LLC (WeWork)`; `12655 Jefferson Blvd Tenant LLC/WeWork`; `1201 3rd Ave Tenant LLC (VVeWork)` | grouped as tenant-shell/brand review; no cross-property shell collapse without policy |
| Tepper/Southern States | `Southern States (Tepper Technologies)`; `TEPPER TECHNOLOGIES, INC. (SOUTHERN STATES)` | relation-aware review or merge only with explicit policy |
| 100 Forsyth / Wayla | `100 Forsyth Restaurant LLC`; `100 Forsyth Restaurant LLC\n(Little Wayla)`; `100 Forsyth Restaurant LLC\nWayla)` | candidate + parenthetical d/b/a reason; review grouping |

Expected:

- review groups are by ambiguity pattern/surface cluster, not raw row;
- each group shows row count, deal/property count, representative rows, support,
  anti-merge/relation evidence, and suggested action;
- review import can create alias, distinct, or relation patches.

### `CMBS-BLOCK-001`: Exact Bucket Compactness

Use high-repeat names from the sample:

| Surface family | Raw observations |
|----------------|------------------|
| `238 Sand Island Prop` | 934 |
| `0` placeholder | 785 |
| `24 Hour Fitness` | 551 |

Expected:

- exact normalized buckets emit compact bucket assertions, not pairwise rows;
- `0` placeholder bucket does not become an entity bucket;
- record count grows O(N), not O(N^2);
- edge/solve can consume bucket assertions while preserving cannot-link vetoes.

Failure meaning:

- Large repeated CMBS tenants will explode the workbench.

### `CMBS-SOLVE-001`: Signed Graph And Cannot-Link Veto

Input: combine must-link clusters with hard negatives and review cases.

Expected:

- must-link/support edges can form promotable clusters;
- hard cannot-link splits, abstains, or emits contradiction;
- relation hints never add positive merge score by default;
- exact incumbent overlap inherits existing IDs only when no conflict exists.

Failure meaning:

- The solver is still threshold-based fuzzy matching rather than constrained
  entity resolution.

### `CMBS-REPLAY-001`: Registry Promotion And Exact Replay

Given accepted review decisions for the `CMBS-NK-001` clusters:

Expected:

- aliases promote into a normal exact registry;
- `canon entity apply` appends canonical fields without mutating raw fields;
- ordinary core `canon` lookup resolves promoted aliases exactly;
- unresolved/reviewable values remain explicit, not silently guessed.

Failure meaning:

- The workbench is not producing durable registry knowledge.

### `CMBS-PERF-001`: Full Sample Structural Performance

Input: full public sample.

Expected:

- prepare/index/block/edge/solve complete under configured local limits;
- candidate generation operates on unique surfaces, not 10,143 raw rows;
- exact buckets are compact;
- exact-bucket pair expansion is 0;
- candidate pairs per unique surface satisfy the shared p95/p99 targets unless
  an explicit benchmark waiver exists;
- summaries include row count, raw unique count, prepared surface count,
  candidate count, edge count, exact bucket count, suppressed candidate count,
  review group count, artifact sizes, cache status, and timing metadata;
- output is deterministic across repeated runs.

Initial wall-clock target from the shared contract:

```text
CMBS public sample, 10,143 observations / 431 raw names: < 2s end-to-end
```

This is an operator/release target after telemetry-backed baseline calibration,
not a brittle normal-CI timing assertion.

### `CMBS-PERF-002`: Shuffled Replay Determinism

Input: full public sample and a deterministically shuffled copy.

Expected:

- same prepared surfaces;
- same candidate set ordering;
- same edge artifact after canonical sort;
- same solve/review groups;
- same apply output after sorting by `source_row_id`.

Failure meaning:

- Hidden row-order dependence has entered the workbench.

### Shared Guardrail Evals

The CMBS tenant suite must also satisfy the shared eval contract:

- `ER-REGISTRY-001`: refused review imports, failed promotions, and stale
  registry snapshots do not mutate tenant registries.
- `ER-EXPLAIN-001`: every tenant merge/review/non-merge can be reconstructed
  from normalized views, candidates, support evidence, anti-merge evidence,
  review decisions, and registry provenance.
- `ER-REVIEW-GOLDEN-001`: review CSV/JSONL/markdown/expected-action artifacts
  are stable enough for operators and agents to compare.
- `ER-META-001`: row shuffle, batch size, duplicate-row, cache-state, harmless
  noise, profile-firewall, and apply-idempotence relations hold.
- `ER-HOLDOUT-001`: future public CMBS samples become `cmbs-public-v2`,
  `cmbs-public-v3`, and so on, without rewriting older holdouts.
- `ER-RUNTIME-001`: the suite runs without network access, frontier model calls,
  runtime model downloads, Python ML runtime, or a general ML framework.
- `ER-MEM-001`: peak memory is reported for public-sample and 500k stress runs,
  with 500k unique names refusing deterministically before memory explosion.

---

## Commands To Eventually Support

These are target commands for the implementation Beads, not commands that work
before `canon entity` exists:

```bash
canon entity prepare /Users/zacharyruiz/Downloads/tenant_sample_cmbs.csv \
  --profile cmbs_tenant_label \
  --registry tests/fixtures/entity/cmbs/registries/tenants \
  --work-dir target/entity-bench/cmbs-sample

canon entity run /Users/zacharyruiz/Downloads/tenant_sample_cmbs.csv \
  --profile cmbs_tenant_label \
  --strategy tests/fixtures/entity/cmbs/cmbs_tenant_label.yaml \
  --registry tests/fixtures/entity/cmbs/registries/tenants \
  --work-dir target/entity-bench/cmbs-sample \
  --emit summary

canon entity review export target/entity-bench/cmbs-sample/solve.json \
  --include all \
  --emit csv > target/entity-bench/cmbs-sample/review.csv

canon entity apply /Users/zacharyruiz/Downloads/tenant_sample_cmbs.csv \
  --registry tests/fixtures/entity/cmbs/registries/tenants \
  --column LARGESTTENANT \
  --out target/entity-bench/cmbs-sample/largest.canon.csv
```

For CI, tests should use committed selected fixtures and the manifest instead
of requiring the local Downloads path.

---

## Non-Negotiables

- Do not count file creation as success.
- Do not treat `0` as a tenant entity.
- Do not let same-property context merge distinct ranked tenants.
- Do not let numeric/address tokens dominate matching.
- Do not expand exact buckets into all-pairs edges.
- Do not make a frontier model, network call, Python ML runtime, or runtime
  model download part of these benchmarks.
- Do not mutate raw parser/source fields during replay.
- Do not pass a benchmark without machine-readable evidence and human-readable
  explanation.
