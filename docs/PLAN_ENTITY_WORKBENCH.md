# canon entity — Profiled Entity Workbench

> **Status**: Draft direct-replacement plan
> **Created**: 2026-06-25
> **Direct replacement for**: `PLAN_ORG_IDENTITY_TOURNAMENT.md`
> **Scope**: Rename the generic `canon org` workbench to `canon entity`,
> add native Rust name/label matching primitives, and make anti-merge evidence
> a first-class part of registry authoring.

---

## One-line Thesis

`canon entity` is the deterministic, profile-driven registry-authoring
workbench inside `canon`: it turns messy observations into exact registry
knowledge through native Rust normalization, candidate generation, merge
evidence, anti-merge evidence, audit, review, and promotion.

The core lookup kernel remains unchanged:

```text
core canon lookup = exact registry lookup
canon entity      = audited workbench that manufactures registry knowledge
ontology layer    = cross-registry hierarchy/alignment graph
```

---

## Why Rename `org` To `entity`

The implemented `canon org` pipeline is already generic:

- observations have surfaces, anchors, context, and provenance
- strategies declare `entity_type`
- blocking generates candidate pairs
- edge generation emits typed evidence
- solving clusters or abstains
- audit/review/promote write exact registry aliases and sidecars

The name `org` is too narrow for the next natural use cases:

- CMBS tenant labels
- legal entities
- investment funds and vehicles
- brands
- people
- properties
- instruments or local securities aliases

The workbench should be named after its real contract: entity/label
canonicalization under an explicit profile.

---

## Design Doctrine

1. **Profiles define identity semantics.**
   `entity` is not one universal matcher. Every strategy must declare what
   "same" means for that profile.

2. **Models and fuzzy machinery never enter the lookup kernel.**
   Native Rust similarity and probabilistic scoring can propose audited
   registry knowledge inside `entity`. Ordinary `canon <INPUT>` still resolves
   by exact lookup only.

3. **Anti-merge evidence is structurally privileged.**
   A high merge score does not override a hard distinctness signal.

4. **Related is not same.**
   `entity` may emit relation hints upward, but the ontology/hierarchy layer
   decides cross-domain relationships. `canon` should connect scoped canonical
   IDs through explicit edges, not collapse them prematurely.

5. **Everything is local, deterministic, and inspectable.**
   No frontier model calls, no network dependencies, no hidden randomness.

6. **Performance is a product feature.**
   Candidate generation must be bounded, streaming-friendly where possible, and
   fast enough for large tape iteration.

7. **Operator ergonomics matter.**
   The workbench should feel smooth for analysts: clear summaries, explainable
   evidence, review queues, alias patch files, and practical next commands.

---

## Identity Semantics

`canon entity` must make the target identity explicit:

```yaml
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
```

Other profiles can use different semantics:

```yaml
entity_type: organization
identity_semantics: same_legal_entity
canonical_type: org
```

```yaml
entity_type: fund_vehicle
identity_semantics: same_legal_vehicle
canonical_type: fund_vehicle
```

This distinction prevents one profile's safe merge from becoming another
profile's over-merge.

Example:

```text
cmbs_tenant_label:TNT-SEARS
investor_name:INVESTOR-SEARS
brand:BRAND-SEARS
legal_entity:ORG-SEARS-HOLDINGS
```

These scoped IDs do not become identical because their labels look similar.
They become connected later by the ontology layer:

```text
TNT-SEARS      --denotes_brand--> BRAND-SEARS
INVESTOR-SEARS --refers_to-->     ORG-SEARS-HOLDINGS
BRAND-SEARS    --associated_with-> ORG-SEARS-HOLDINGS
```

---

## First Validated Use Case: CMBS Tenant Label Backfill

The first new profile is `cmbs_tenant_label`, driven by a historical CMBS book
backfill: roughly thousands of deals and hundreds of thousands of tenant rows.
The workbench must solve tenant-label canonicalization at book scale before it
claims broader legal-entity identity value.

The core use case is not "run fuzzy matching on one deal." It is:

```text
3,000 historical CMBS deals
  -> extracted tenant observations
  -> unique tenant surfaces
  -> global candidate/index pass
  -> reviewed/promoted tenant-label registry
  -> exact replay over every deal
```

The first profile's identity semantics are intentionally scoped:

```yaml
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
```

This is not the same as legal obligor identity, brand hierarchy, ownership
history, or investor identity. Those belong to later profiles or the ontology
layer.

Target:

```text
SEARS LLC              -> TNT-SEARS
Sears                  -> TNT-SEARS
Sears Roebuck & Co.    -> TNT-SEARS
Sears #1234            -> TNT-SEARS
```

Harder cases must not silently collapse:

```text
Sears Auto Center      -> related/distinct unless profile policy permits
Kmart                  -> related/distinct, not same tenant label
Transform SR LLC       -> related/distinct unless explicitly patched
Sears Holdings         -> related/distinct unless explicitly patched
```

This profile is intentionally easier than legal-entity identity. It creates a
high-value path to clean CMBS tenant strings while forcing the workbench to get
anti-merge and operator review right.

The public-data benchmark contract for this use case lives in
[`CMBS_TENANT_BENCHMARKS.md`](CMBS_TENANT_BENCHMARKS.md). It is derived from a
real CMBS tenant sample with `10,143` tenant observations and `431` unique raw
tenant names. The benchmark suite defines fixed extraction counts, must-link
tenant-label clusters, hard negatives, review/escrow cases, exact-bucket
compactness checks, replay assertions, and performance tiers. Implementation
Beads for the CMBS profile and backfill workflow should treat that file and
`tests/fixtures/entity/cmbs/tenant_sample_benchmark_manifest.json` as the
use-case #1 benchmark contract.

Shared eval scoring, structural performance gates, wall-clock targets,
telemetry fields, and run commands live in
[`ENTITY_EVALS_AND_PERFORMANCE.md`](ENTITY_EVALS_AND_PERFORMANCE.md) and
`tests/fixtures/entity/evals/entity_eval_performance_targets.json`. The shared
guardrails also cover registry mutation safety, explainability completeness,
human review goldens, metamorphic invariants, holdout discipline, no-network /
no-model runtime behavior, and peak memory.

---

## CMBS Tenant Backfill Pipeline

`canon entity` should handle the canonicalization workbench end to end, assuming
upstream extraction has already produced tenant observations. It should not
perform OCR/PDF extraction, ontology hierarchy discovery, or business analytics.

The required pipeline is:

```text
1. ingest extracted tenant observations
2. prepare normalized surfaces and provenance
3. dedupe to unique tenant surfaces
4. resolve already-known surfaces through exact registry lookup
5. build global indexes over unresolved unique surfaces
6. generate bounded candidate surface pairs
7. score merge and anti-merge evidence
8. solve surface-level clusters
9. export grouped review queues
10. import review decisions and patch files
11. promote accepted aliases into exact registry entries
12. replay all deal rows through ordinary exact canon lookup
```

The scale boundary is important:

```text
row count       = all tenant rows across all deals
surface count   = unique raw/normalized tenant strings
cluster count   = canonical tenant labels
```

Candidate generation, edge scoring, and solving must operate on unique surfaces
first. Rows are expanded only after a surface has resolved to a canonical tenant
label.

Every prepared unique surface must receive a deterministic `surface_id` derived
from profile, normalized view, and a collision-safe hash of the raw surface set.
`surface_id` is the stable join key across block, edge, solve, review, promote,
and apply artifacts. Raw `source_row_id` values remain provenance, not the
primary unit of candidate generation.

Expected shape:

```text
500,000 tenant rows
60,000 raw unique tenant strings
25,000 normalized unique surfaces
8,000 canonical tenant labels
```

These numbers are examples, not promises. The implementation must report the
actual counts at every stage.

### Backfill Commands

`run` should orchestrate the whole workflow, but the internal stages must remain
available as explicit commands for debugging, benchmarking, and review.

```bash
canon entity prepare tenants/*.jsonl \
  --profile cmbs_tenant_label \
  --registry registries/cmbs-tenants \
  --work-dir work/cmbs-tenants

canon entity index build work/cmbs-tenants/prepare \
  --strategy strategies/cmbs_tenant_label.yaml \
  --out work/cmbs-tenants/index

canon entity block work/cmbs-tenants/prepare \
  --index work/cmbs-tenants/index \
  --strategy strategies/cmbs_tenant_label.yaml \
  --out work/cmbs-tenants/blocks.jsonl

canon entity edge work/cmbs-tenants/prepare \
  --candidates work/cmbs-tenants/blocks.jsonl \
  --strategy strategies/cmbs_tenant_label.yaml \
  --registry registries/cmbs-tenants \
  --out work/cmbs-tenants/edges.jsonl

canon entity solve work/cmbs-tenants/prepare \
  --edges work/cmbs-tenants/edges.jsonl \
  --strategy strategies/cmbs_tenant_label.yaml \
  --registry registries/cmbs-tenants \
  --out work/cmbs-tenants/solve.json

canon entity promote work/cmbs-tenants/solve.json \
  --audit work/cmbs-tenants/audit.json \
  --registry registries/cmbs-tenants \
  --next-version 2026.06.25

canon entity apply tenants/*.jsonl \
  --registry registries/cmbs-tenants \
  --column raw_tenant_name \
  --out work/cmbs-tenants/tenants.canon.jsonl
```

For normal operation, the wrapper should be:

```bash
canon entity run tenants/*.jsonl \
  --profile cmbs_tenant_label \
  --strategy strategies/cmbs_tenant_label.yaml \
  --registry registries/cmbs-tenants \
  --work-dir work/cmbs-tenants-2026-06 \
  --emit summary
```

### Batching Model

The historical book should be ingested in physical batches but solved against a
global surface/index view.

Good:

```text
ingest 100 deals at a time
append normalized surfaces to a global prepared corpus
rebuild or incrementally update the global index
solve unresolved unique surfaces against the global registry
promote reviewed aliases
replay affected deals exactly
```

Bad:

```text
solve deal 1 in isolation -> mint TNT-SEARS
solve deal 2 in isolation -> mint TNT-SEARS-2
solve deal 3 in isolation -> mint TNT-SEARS-3
later reconcile accidental duplicates
```

The workbench should optimize for one global registry and one global tenant
surface memory, even when execution is chunked for IO and memory.

---

## Second Validated Use Case: sec10d Reg AB Firm Identity

The second validated profile is `regab_firm_identity`, driven by the `sec10d`
Reg AB 10-K/10-D enrichment pipeline. This use case is organization-like, but
it must not reuse tenant-label semantics.

Current `sec10d` state:

- `sec10d` parser emits raw Reg AB JSONL records.
- A downstream helper extracts firm-bearing fields into `org_mentions.csv`.
- The helper currently runs exact core `canon` lookup against a reviewed firms
  registry and appends `*_org_*` enrichment fields.
- The remaining planned hardening is to use the formal workbench path for
  unresolved/reviewable firm surfaces instead of maintaining alias sheets by
  hand.

The target profile is:

```yaml
profile: regab_firm_identity
entity_type: organization
identity_semantics: same_firm_or_reviewed_alias
canonical_type: org
```

This profile differs from `cmbs_tenant_label`:

```text
cmbs_tenant_label  = canonical display label for tenant strings
regab_firm_identity = reviewed firm identity / firm alias canonicalization
```

### Reg AB Observation Shape

`sec10d` already emits nearly the right entity-workbench observation shape:

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

`canon entity prepare` should accept this shape directly, with the profile
declaring which fields are primary surfaces, aliases, mentions, anchors, and
context.

### Reg AB Firm Semantics

The Reg AB firm profile should resolve reviewed firm aliases while keeping
division, agent, platform/category, parent/subsidiary, and role-conflict cases
reviewable.

Examples:

```text
Wells Fargo Bank, N.A.          -> reviewed firm canonical ID
Wells Fargo Bank, National Association -> same reviewed firm if policy says so
PNC Bank, National Association  -> distinct from Midland unless reviewed
Midland Loan Services           -> distinct from PNC parent/affiliate by default
platform_name                   -> not automatically an organization
certifying_party_name           -> excluded until a people/person path exists
```

Hard anti-merge/review triggers:

```text
parent vs subsidiary
bank vs loan-services division
servicer vs subservicer/agent capacity conflict
platform/category label used as if it were a firm
auditor firm vs subject party role conflict
same family/parent but different regulated entity
```

### sec10d Target Pipeline

The downstream `sec10d` enrichment helper should migrate from bespoke exact
lookup orchestration to proper `canon entity` stages:

```bash
canon entity prepare org_mentions.csv \
  --profile regab_firm_identity \
  --registry registries/firms \
  --work-dir work/sec10d-regab-firms

canon entity run org_mentions.csv \
  --profile regab_firm_identity \
  --strategy strategies/regab_firm_identity.yaml \
  --registry registries/firms \
  --work-dir work/sec10d-regab-firms \
  --emit summary

canon entity review export work/sec10d-regab-firms/solve.json \
  --include escrow \
  --emit csv \
  > work/sec10d-regab-firms/review.csv

canon entity promote work/sec10d-regab-firms/solve.json \
  --audit work/sec10d-regab-firms/audit.json \
  --registry registries/firms \
  --next-version 2026.06.25

canon entity apply org_mentions.csv \
  --registry registries/firms \
  --column org_name \
  --out work/sec10d-regab-firms/org_mentions.canon.csv
```

The enriched `sec10d` JSONL should continue to preserve raw parser fields and
append canonical fields only:

```text
*_org_canon_id
*_org_canonical_name
*_org_resolution_status
*_org_registry_id
*_org_registry_version
*_org_rule_id
```

`sec10d` remains parser-first. It should not own entity solving, registry
promotion, hierarchy discovery, or parent/subsidiary modeling. It supplies
observations and consumes exact canonical enrichment.

### Compatibility With Existing sec10d Baseline

The current exact baseline is a valid Phase 0:

```text
extract org mentions
run exact core canon lookup
write unresolved review queue
append enrichment fields
```

The entity workbench subsumes it by adding prepare/index/block/edge/solve/audit
around unresolved or reviewable surfaces, while preserving exact replay as the
production enrichment path.

The baseline benchmark contract for this use case lives in
`docs/SEC10D_REGAB_BENCHMARKS.md`, with machine-readable expectations in
`tests/fixtures/entity/regab/sec10d_regab_benchmark_manifest.json`. It is based
on `sec10d_regab_org_canon_baseline_20260623T204557Z.zip`, whose baseline has
127,991 firm mentions, 46 unique raw firm surfaces, 31 canonical ids, 0
unresolved mentions, and registry `firms` version `1.0.12`.

Implementation Beads for ENT-P11/ENT-P13 should treat that doc and manifest as
the use-case #2 benchmark contract. Passing the benchmark means the migrated
`canon entity` path preserves the current exact baseline, accepts the existing
`org_mentions.csv` shape, keeps parser evidence append-only, and enforces the
Reg AB anti-collapse boundary for cases such as PNC vs Midland and Wells Fargo
Bank vs Wells Fargo Commercial Mortgage Servicing.

The shared entity eval/performance contract applies here too. In particular,
Reg AB migration must satisfy `ER-DIFF-001`, `ER-ADV-001`, `ER-DET-001`,
`ER-REGISTRY-001`, `ER-EXPLAIN-001`, `ER-RUNTIME-001`, `ER-MEM-001`,
`PERF-REGAB-FULL`, `PERF-REGAB-PREPARE`, and `PERF-REGAB-APPLY` from
[`ENTITY_EVALS_AND_PERFORMANCE.md`](ENTITY_EVALS_AND_PERFORMANCE.md).

---

## CLI Migration

Target CLI:

```bash
canon entity run <ROWS> --strategy <STRATEGY.yaml> --registry <REGISTRY_DIR>
canon entity prepare <ROWS> --profile <PROFILE> --registry <REGISTRY_DIR> --work-dir <DIR>
canon entity index build <PREPARE_DIR> --strategy <STRATEGY.yaml> --out <INDEX_DIR>
canon entity block <ROWS> --strategy <STRATEGY.yaml> --registry <REGISTRY_DIR>
canon entity edge <ROWS> --strategy <STRATEGY.yaml> --candidates <BLOCK.jsonl> --registry <REGISTRY_DIR>
canon entity solve <ROWS> --strategy <STRATEGY.yaml> --edges <EDGES.jsonl> --registry <REGISTRY_DIR>
canon entity audit <RESULT.json> --suite <SUITE_DIR>
canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <REGISTRY_DIR> --next-version <VERSION>
canon entity review export <RESULT.json> --emit json|csv --include resolved|escrow|contradictions|all
canon entity review import <REVIEW.json|csv> --registry <REGISTRY_DIR> --next-version <VERSION>
canon entity explain <RESULT.json> --row <SOURCE_ROW_ID>|--canon-id <CANON_ID>|--escrow-id <ESCROW_ID>
canon entity apply <ROWS> --registry <REGISTRY_DIR> --column <COLUMN> --out <ROWS.canon.jsonl>
```

Migration policy:

- `canon org` has no compatibility obligation yet. Replace it outright with
  `canon entity`.
- Do not carry a long-lived `canon org` alias, dual command surface, or
  backward-compatible artifact shim.
- Rename artifact versions cleanly to the `canon_entity_*` family.
- Update tests, docs, operator metadata, and examples in the same migration so
  `org` does not remain as the public workbench concept.

Target artifact names:

```text
canon_entity_projection.v0
canon_entity_prepare.v0
canon_entity_index.v0
canon_entity_block.v0
canon_entity_edge.v0
canon_entity_solve.v0
canon_entity_run.v0
canon_entity_audit.v0
canon_entity_promote.v0
canon_entity_explain.v0
canon_entity_apply.v0
```

---

## Entity Workbench Invariants

These invariants are mandatory. Implementation packets and tests should refer to
them by ID.

| ID | Invariant |
|----|-----------|
| I01 | Core `canon <INPUT>` lookup remains exact registry lookup after ASCII-trim; no fuzzy or multi-field logic enters the lookup kernel. |
| I02 | `canon entity` may generate fuzzy/similarity evidence only before promotion. Promoted output is still exact registry aliases and sidecars. |
| I03 | Every entity run is local and deterministic: same inputs, strategy, registry snapshot, profile version, and patch files produce byte-stable artifacts. |
| I04 | Every workbench artifact records strategy reference, registry snapshot, profile id/version, input content hash, and artifact content hash when persisted. |
| I05 | Every prepared surface has a deterministic `surface_id`; raw row IDs are provenance, not candidate-generation identity. |
| I06 | Candidate generation, edge scoring, and solving operate on unique surfaces first; raw rows are expanded only during `apply`/replay. |
| I07 | Exact normalized buckets are represented as compact assertions or direct surface groups; they never expand into all pairwise edges. |
| I08 | A hard `cannot_link` or trusted-anchor conflict vetoes auto-merge regardless of support score. |
| I09 | Relation hints never add positive merge score by default. They are exported for ontology consumers and review context. |
| I10 | Profiles define identity semantics. No strategy may run without explicit `profile`, `entity_type`, `identity_semantics`, and `canonical_type`. |
| I11 | Tenant-label canonicalization never claims legal obligor identity. Reg AB firm identity never reuses tenant-label merge semantics. |
| I12 | All lossy normalization steps emit reason codes available to explanation/review artifacts. |
| I13 | Candidate caps are enforced before artifact explosion. Exceeding configured caps produces bounded abstention or structured refusal, not uncontrolled output. |
| I14 | Review exports are grouped by surface cluster or ambiguity pattern, not by raw row. |
| I15 | Review import cannot silently override hard cannot-link facts; overrides require explicit operator decision provenance. |
| I16 | Patch files are versioned inputs and participate in strategy/profile hashes. |
| I17 | Alias, distinctness, and relation patches are checked for internal contradiction before use. |
| I18 | Promotion requires a matching audit artifact for the exact result artifact and registry snapshot being promoted. |
| I19 | Promotion never mutates parser/source evidence rows. It writes registry aliases, sidecars, escrow/cannot-link facts, and proof artifacts only. |
| I20 | `apply`/replay appends canonical fields and preserves raw input fields byte-for-byte where the input format permits. |
| I21 | Cache hits are allowed only when input hash, strategy hash, profile version, registry snapshot hash, patch hash, and namekit version all match. |
| I22 | Cache misses must be explicit in summaries so operators can tell why a large run rebuilt. |
| I23 | All top-k and tie-breaking behavior is stable and deterministic. |
| I24 | Every auto-merge, abstention, contradiction, review item, and promotion has machine-readable evidence and a human-readable summary. |
| I25 | Cross-profile alignment belongs outside `canon entity`; `canon entity` may emit candidate relation hints but does not collapse scoped IDs across profiles. |

---

## Error And Refusal Taxonomy

New entity commands should reuse the normal `canon` refusal envelope and exit
semantics. Entity-specific failure modes need stable reason codes so operators
can recover without reading source.

| Code | Applies To | Meaning | Recovery |
|------|------------|---------|----------|
| E_ENTITY_PROFILE | all entity commands | Unknown, missing, or semantically invalid profile. | Run `canon entity profile list` or fix the strategy profile block. |
| E_ENTITY_STRATEGY | prepare/index/block/edge/solve/run | Strategy YAML is malformed or references unsupported operators/views. | Run strategy lint/doctor and fix the strategy. |
| E_ENTITY_INPUT_CONTRACT | prepare/apply | Input rows are missing required profile fields or contain malformed side fields. | Fix extraction or provide a profile mapping. |
| E_ENTITY_SURFACE_ID_COLLISION | prepare | Two distinct prepared surfaces produced the same `surface_id`. | Report collision details; adjust hash/surface-id derivation. |
| E_ENTITY_PATCH_CONFLICT | prepare/edge/review import | Alias/distinct/relation patches contradict each other or the registry. | Resolve the patch conflict before running. |
| E_ENTITY_REGISTRY_SNAPSHOT | all mutating stages | Registry snapshot does not match the artifact being consumed. | Re-run from prepare or use the matching registry snapshot. |
| E_ENTITY_CACHE_MISMATCH | prepare/index/block | A cache artifact exists but hashes do not match the current run. | Rebuild cache or pass the intended work directory. |
| E_ENTITY_INDEX_LIMIT | index/block | Posting list, bucket, or top-k limits are exceeded. | Tighten strategy, increase explicit caps, or review large buckets. |
| E_ENTITY_CANDIDATE_BUDGET | block | Candidate budget exceeded before bounded candidate emission. | Adjust blocking operators or run grouped review on large buckets. |
| E_ENTITY_ARTIFACT_CONTRACT | edge/solve/audit/promote/apply | Input artifact has wrong version, profile, strategy, registry, or hash. | Use the correct upstream artifact or re-run the stage. |
| E_ENTITY_CANNOT_LINK_OVERRIDE | solve/review import/promote | A requested merge conflicts with a hard cannot-link fact. | Create explicit operator override evidence or keep in review. |
| E_ENTITY_REVIEW_IMPORT | review import | Review CSV/JSON is malformed, references unknown items, or mixes runs. | Export a fresh queue or repair the review file. |
| E_ENTITY_AUDIT_GATE | promote | Audit artifact is missing, stale, or failed required gates. | Re-run audit and fix failures before promotion. |
| E_ENTITY_APPLY_UNRESOLVED | apply | Apply was configured to require full resolution but unresolved surfaces remain. | Promote more aliases or run with partial output allowed. |
| E_ENTITY_IO_BUDGET | all large stages | Max rows, bytes, artifacts, memory budget, or work-dir budget exceeded. | Increase explicit limits or process in physical batches. |

Refusal is not the same as abstention. Abstention is a successful domain
outcome inside solve/review artifacts; refusal means the command could not
evaluate its contract safely.

---

## Artifact Contracts

The exact schemas should be formalized later as JSON Schema, but implementation
should start from these minimal shapes. Every artifact must include version,
profile, strategy, registry snapshot, input hashes, and deterministic summary.

### `canon_entity_prepare.v0`

```json
{
  "version": "canon_entity_prepare.v0",
  "profile": {"id": "cmbs_tenant_label", "version": "0.1.0"},
  "strategy": {"id": "cmbs_tenant_label.v1", "version": "0.1.0", "content_hash": "blake3:..."},
  "registry_snapshot": {"id": "cmbs-tenants", "version": "2026.06.25", "lookup_snapshot_hash": "blake3:..."},
  "input": {"row_count": 500000, "content_hash": "blake3:..."},
  "summary": {"raw_unique_surfaces": 61423, "prepared_surfaces": 27118, "exact_resolved_surfaces": 18044, "unresolved_surfaces": 9074},
  "surfaces_path": "prepare/surfaces.jsonl"
}
```

Surface JSONL:

```json
{
  "surface_id": "surf:cmbs_tenant_label:blake3:...",
  "primary_surface": "SEARS LLC",
  "normalized_views": {"tenant_core": "sears", "tenant_tokens": ["sears"]},
  "row_count": 183,
  "deal_count": 42,
  "provenance_samples": [{"source_row_id": "deal-1#loan-2#tenant-0", "deal_id": "deal-1"}],
  "exact_lookup": {"status": "unresolved", "canonical_id": null}
}
```

### `canon_entity_index.v0`

```json
{
  "version": "canon_entity_index.v0",
  "prepare_hash": "blake3:...",
  "strategy_hash": "blake3:...",
  "summary": {
    "surface_count": 27118,
    "token_count": 11804,
    "ngram_count": 39210,
    "large_bucket_count": 17,
    "cache_status": "rebuilt"
  },
  "postings_path": "index/postings.bin",
  "diagnostics_path": "index/diagnostics.jsonl"
}
```

### `canon_entity_block.v0`

```json
{
  "version": "canon_entity_block.v0",
  "left_surface_id": "surf:...",
  "right_surface_id": "surf:...",
  "block_hits": [{"operator_id": "ngram_topk:tenant_core", "rank": 3}],
  "candidate_score_hint": 0.91
}
```

Exact bucket assertion:

```json
{
  "version": "canon_entity_block_bucket.v0",
  "bucket_id": "bucket:tenant_core:sears",
  "operator_id": "exact_view:tenant_core",
  "surface_ids": ["surf:...", "surf:..."],
  "row_count": 183,
  "pair_expansion": "forbidden"
}
```

### `canon_entity_edge.v0`

```json
{
  "version": "canon_entity_edge.v0",
  "left_surface_id": "surf:...",
  "right_surface_id": "surf:...",
  "hits": [
    {"kind": "support", "namespace": "name", "operator_id": "string_similarity:jaro_winkler", "score": 24, "explanation": "tenant_core similarity 0.97"},
    {"kind": "cannot_link", "namespace": "tenant_role", "operator_id": "related_distinct_phrase:auto center", "score": 0, "explanation": "related distinct phrase"}
  ],
  "pair_score_total": 24,
  "has_hard_cannot_link": true,
  "relation_hints": [{"relation": "related_brand_family", "confidence": "rule"}]
}
```

### `canon_entity_solve.v0`

```json
{
  "version": "canon_entity_solve.v0",
  "summary": {"resolved_existing": 18044, "promotable_new": 7831, "escrow": 912, "contradictions": 17},
  "entities": [
    {
      "state": "PROMOTABLE_NEW",
      "canonical_id": "TNT-SEARS",
      "surface_ids": ["surf:..."],
      "aliases": ["Sears", "SEARS LLC"],
      "merge_witnesses": [],
      "anti_merge_warnings": []
    }
  ],
  "review_groups": [],
  "decision_ledger_path": "solve/decisions.jsonl"
}
```

### `canon_entity_apply.v0`

```json
{
  "version": "canon_entity_apply.v0",
  "registry": {"id": "cmbs-tenants", "version": "2026.06.25"},
  "summary": {"rows": 500000, "resolved": 483201, "unresolved": 16799},
  "output_path": "tenants.canon.jsonl"
}
```

---

## Native Rust Namekit

Add an internal Rust module/crate boundary:

```text
src/namekit/
  mod.rs
  normalize.rs
  legal_suffix.rs
  tenant.rs
  tokenize.rs
  ngram.rs
  similarity.rs
  tfidf.rs
  patches.rs
  explain.rs
```

`namekit` owns fast deterministic text/entity-name primitives. It should be
usable by `entity` profiles without importing Python runtimes or broad ML
frameworks.

### Port Targets

Port ideas, contracts, and dictionaries where useful. Do not embed whole Python
frameworks.

| Source | What to port | Why |
|--------|--------------|-----|
| OpenSanctions `rigour` / `normality` | Unicode/text normalization, name fingerprints, identifier cleanup ideas | Battle-tested entity-data cleanup primitives |
| `cleanco` | Legal suffix tables and stripping behavior | Essential for `SEARS LLC` vs `Sears` |
| ING `EntityMatchingModel` | Word/char TF-IDF, sparse cosine, sorted-neighborhood candidate generation | Strong fit for company/tenant names without neural models |
| Splink | Fellegi-Sunter-style comparison levels, term-frequency adjustment, diagnostics | Transparent scoring and rare-token weighting |
| RapidFuzz / string metrics | Levenshtein, Jaro-Winkler, token set/sort, Dice/Sorensen | Fast local similarity primitives |
| OpenSanctions `datapatch` | Versioned operator override files | Alias and distinctness patches for messy real data |
| nomenklatura resolver pattern | Explicit same/different/undecided judgement graph | Proven identity-review ergonomics |

### Namekit Invariants

- Pure Rust.
- `#![forbid(unsafe_code)]`.
- Deterministic outputs.
- No network calls.
- No runtime model downloads.
- No hidden locale dependence.
- Inputs and outputs are byte-stable under tests.
- Every lossy transformation can emit a reason code.

---

## Strategy Extensions

Normalization operators:

```yaml
normalize:
  views:
    tenant_core:
      - unicode_fold
      - lowercase
      - strip_tenant_noise
      - strip_legal_suffixes
      - normalize_whitespace
    tenant_brand:
      - tenant_brand_fingerprint
    tenant_tokens:
      - unicode_fold
      - lowercase
      - tokenize
      - drop_tenant_stopwords
```

Blocking operators:

```yaml
blocking:
  - op: exact_view
    view: tenant_core
  - op: ngram_topk
    view: tenant_core
    k: 25
  - op: rare_token_overlap
    left_view: tenant_tokens
    right_view: tenant_tokens
    min_tokens: 1
    min_idf: 1.0
  - op: alias_patch_match
```

Support evidence:

```yaml
evidence:
  support:
    - op: exact_view
      view: tenant_core
      score: 40
    - op: string_similarity
      view: tenant_core
      metric: jaro_winkler
      min_score: 0.94
      score: 24
    - op: tfidf_cosine
      view: tenant_tokens
      min_score: 0.85
      score: 18
    - op: alias_patch_match
      score: 50
```

Anti-merge evidence:

```yaml
evidence:
  cannot_link:
    - op: alias_patch_distinct
    - op: protected_token_conflict
      view: tenant_brand
    - op: related_distinct_phrase
      phrases: [auto center, holdings, properties, management, capital]
    - op: conflicting_anchor
      anchor: tenant_tax_id
```

Relation hints for the ontology layer should be separate from merge decisions:

```yaml
relations:
  hints:
    - op: related_brand_family
    - op: possible_successor_predecessor
    - op: same_parent_or_sponsor
```

If relation hints are added to artifacts, they must not contribute positive
merge score by default.

---

## Evidence Model

The solver should reason over separate evidence lanes:

```text
must_link      = trusted exact identity evidence
support        = positive merge support
cannot_link    = hard or soft anti-merge evidence
relation_hint  = related-but-not-same signal for ontology consumers
```

Initial implementation can encode `relation_hint` as `cannot_link` plus a
namespace/reason if avoiding a schema bump is preferable. The target model
should eventually make relation hints explicit to avoid overloading
`cannot_link`.

Merge policy:

```text
auto-merge only if:
  merge evidence is strong
  anti-merge evidence is absent
  no trusted anchor conflict exists
  graph consistency checks pass
  audit suite passes

escrow/review if:
  merge evidence and anti-merge evidence are both high
  score margin is narrow
  relation hints explain similarity

never auto-merge if:
  a hard cannot-link fires
```

For tenant labels, anti-merge is not a failure. It is useful signal:

```text
Sears vs Sears Auto Center -> related/distinct or review
Sears vs Kmart             -> related/distinct
Sears vs Transform SR LLC  -> related/distinct
```

---

## Advanced Resolution Mechanics

The workbench should be more than fuzzy string matching with thresholds. The
long-term quality bar is a constrained, explainable identity compiler.

### Signed Evidence Graph

The solver should treat support and cannot-link facts as a signed graph:

```text
positive edges = support/must-link evidence
negative edges = hard cannot-link and soft anti-merge evidence
relation edges = related-but-not-same hints
```

Auto-clustering must satisfy hard negative constraints. If a candidate component
contains any hard cannot-link pair, the solver must split, abstain, or emit a
contradiction. It must not "outscore" the contradiction.

Signed-graph requirements:

- hard cannot-link edges are constraints, not negative weights
- soft anti-merge edges lower confidence and increase review priority
- relation hints explain similarity but do not authorize merges
- component-level decisions report the strongest positive and negative cuts
- exact buckets are compact hyperedges, not pairwise explosions

### Blocking Quality Metrics

Blocking must be measurable. Every audit suite should report:

```text
candidate_recall
pairs_per_surface_p50/p95/p99
candidate_pairs_emitted
candidate_pairs_suppressed_by_cap
large_buckets_suppressed
gold_matches_blocked_out
top_blocking_operators_by_yield
top_blocking_operators_by_false_positive_review_load
```

The goal is not just fast blocking. It is blocking that preserves recall while
making review and scoring tractable.

### Active Review Selection

Review queues should rank decisions by expected risk reduction, not just raw
score or arbitrary order. A review item is more valuable when it has high:

- row count blast radius
- deal count blast radius
- evidence conflict
- uncertainty margin
- likelihood of becoming a reusable alias or distinctness patch
- downstream profile/Snowflake impact

Review summaries should expose why an item was selected:

```json
{
  "review_priority": 0.97,
  "priority_reasons": ["high_row_count", "support_and_cannot_link", "known_brand_family"],
  "affected_rows": 183,
  "affected_deals": 42
}
```

### Decision Ledger

Every human/operator/system decision should be appended to an immutable
decision ledger before it is compiled into registry aliases or patches.

Decision events:

```text
merge_confirmed
distinct_confirmed
relation_confirmed
alias_patch_added
cannot_link_added
operator_override_requested
operator_override_approved
promotion_applied
promotion_reverted
```

Each event records:

```text
decision_id
timestamp
operator
profile
strategy_hash
registry_snapshot_hash
left/right surface ids or entity ids
decision
reason_code
freeform note
source review artifact hash
```

The ledger gives the system rollback, provenance, future training data, and a
way to explain why two surfaces are known distinct even after future strategies
change.

### Profile-Aware Negative Knowledge

Negative knowledge is profile-scoped. `Sears` vs `Sears Auto Center` may be a
tenant-label distinction; that does not automatically imply anything about
legal-entity identity, brand identity, or ownership hierarchy. Cannot-link facts
must therefore include profile and identity semantics.

---

## Contracts And Threats

### Behavioral Contracts

| ID | Contract |
|----|----------|
| C01 | `prepare` produces deterministic surface IDs and stable normalized views. |
| C02 | `index build` never changes semantic decisions; it only materializes lookup structures. |
| C03 | `block` emits bounded candidates and compact bucket assertions. |
| C04 | `edge` emits evidence only for declared operators and valid candidate surfaces. |
| C05 | `solve` never merges across hard cannot-link constraints. |
| C06 | `review export` groups by ambiguity pattern/surface cluster and preserves enough provenance to decide. |
| C07 | `review import` records decisions in the decision ledger before deriving aliases or patches. |
| C08 | `audit` verifies profile semantics, benchmark gates, hard negatives, and artifact hash continuity. |
| C09 | `promote` writes only registry aliases/sidecars/proofs and never mutates input observations. |
| C10 | `apply` is exact replay against the promoted registry and preserves raw input fields. |
| C11 | `explain` can reconstruct positive evidence, anti-merge evidence, review decisions, and promotion provenance for one row/surface/entity. |
| C12 | `doctor` detects stale caches, patch contradictions, profile mismatch, unsupported operators, and sidecar drift. |

### Threats And Required Mitigations

| ID | Threat | Required Mitigation |
|----|--------|---------------------|
| T01 | Exact buckets explode into pairwise edges. | Compact bucket assertions; test with 8,000-row bucket. |
| T02 | Common tokens dominate candidate generation. | Term-frequency adjustment and stopword/large-posting caps. |
| T03 | Similar brand family over-merges distinct entities. | Hard-negative suites and profile-scoped cannot-link evidence. |
| T04 | Parent/subsidiary/agent relations collapse into same entity. | Relation hints and cannot-link/review guards. |
| T05 | Review queue floods operators with duplicates. | Group by ambiguity pattern and rank by expected risk reduction. |
| T06 | Cache reuse returns stale evidence. | Cache key includes input, profile, strategy, registry, patch, and namekit hashes. |
| T07 | Review import applies decisions to the wrong run. | Artifact hash and registry snapshot checks on import. |
| T08 | Operator override erases hard negative knowledge. | Require explicit override decision event and audit visibility. |
| T09 | `sec10d` parser evidence is mutated during enrichment. | `apply` appends fields only; parser raw fields are preserved. |
| T10 | Cross-profile IDs are collapsed too early. | Scoped IDs plus ontology-layer relation hints only. |
| T11 | New fuzzy operators enter core lookup by accident. | Module/API boundary and tests asserting core lookup remains exact. |
| T12 | Large runs produce non-deterministic top-k order. | Stable tie-breaking by normalized key and surface ID. |

---

## Alias And Distinctness Patches

Operators need a simple way to fix real data without changing code.

Patch files should be versioned and reviewable:

```yaml
profile: cmbs_tenant_label
version: 2026.06.25

aliases:
  - canonical_hint: TNT-SEARS
    inputs:
      - Sears
      - SEARS LLC
      - Sears Roebuck
      - Sears, Roebuck and Co.

distinct:
  - left: Sears
    right: Kmart
    reason: related_brand_family_not_same_tenant_label
  - left: Sears
    right: Transform SR LLC
    reason: successor_or_operator_not_display_label

relations:
  - left: Sears
    right: Kmart
    relation: related_brand_family
```

Patch files should feed both evidence and operator review:

- alias patch -> strong support or must-link depending on profile policy
- distinct patch -> cannot-link
- relation patch -> relation hint

---

## Performance Plan

The current `rare_token_overlap` implementation is simple and may become
quadratic. The entity workbench needs bounded candidate generation designed for
the CMBS tenant backfill shape: hundreds of thousands of rows, tens of
thousands of unique surfaces, and repeated replay as the registry improves.

Required performance features:

1. **Dedupe-first execution.**
   Normalize and collapse rows into unique tenant surfaces before candidate
   generation. Every prepared surface must retain row-count and provenance
   samples so the solver can later expand decisions back to rows.

2. **No pair expansion for exact buckets.**
   Exact normalized matches should become compact bucket assertions or direct
   surface clusters, not all pairwise row or surface edges. A bucket with 8,000
   rows must not emit 31,996,000 pair records.

3. **Inverted indexes for exact views and tokens.**
   Avoid all-pairs loops when token indexes can produce candidate sets.

4. **Char n-gram postings with top-k pruning.**
   Generate only the best candidate neighborhood per unique surface. Candidate
   top-k applies after dedupe, not per raw row.

5. **Term-frequency adjustment.**
   Downweight common terms such as `llc`, `inc`, `store`, `retail`, `tenant`,
   `properties`, `holdings`.

6. **Hard candidate caps and diagnostics.**
   Strategies should declare max candidates per unique surface, max candidate
   pairs per block operator, max exact-bucket expansion, and max total emitted
   edges. If caps are exceeded, emit a clear refusal or bounded abstention
   depending on the command.

7. **Stable deterministic ordering.**
   Top-k ties must break by stable row ID ordering.

8. **Memory-aware representations.**
   Use integer token IDs and compact postings internally; keep artifact output
   readable.

9. **Disk-backed prepared/index artifacts.**
   Large backfills should not require holding every raw row, token posting, and
   candidate edge in memory at once. Prepared surfaces, postings, and candidate
   summaries should be spillable or chunk-readable.

10. **Hash-keyed caches.**
   Cache normalization, token dictionaries, IDF tables, n-gram postings, and
   candidate indexes by input hash, strategy hash, registry snapshot hash, and
   profile version. Re-running after a small review import should not rebuild
   the full world.

11. **Scaling tests.**
   Add fixtures and synthetic stress tests that assert bounded candidate counts,
   deterministic output, and acceptable runtime.

The first benchmark target should be practical rather than theatrical:

```text
3,000-deal CMBS historical backfill
500k tenant rows
dedupe-first unique-surface solving
bounded candidate generation
no row-level O(n^2) pair explosion
no exact-bucket pair explosion
deterministic JSON/JSONL artifacts
```

Add one harsher synthetic stress target:

```text
500k unique tenant surfaces
bounded top-k candidates per surface
controlled memory footprint
deterministic abort/refusal if configured limits are exceeded
```

---

## Quality Gates

These gates are stop-ship criteria for the entity workbench. A release or bead
packet that claims entity readiness must name which gates it satisfies.

| Gate | Required Proof |
|------|----------------|
| G01 Core lookup unchanged | Existing core `canon` golden tests pass before and after the entity migration. |
| G02 Entity namespace replacement | No public CLI, docs, operator metadata, or artifact constants expose `canon org` as the current workbench. |
| G03 Prepare determinism | Same input/profile/registry/patches produce byte-identical prepare artifacts across three runs. |
| G04 Surface ID stability | Reordered input rows produce identical `surface_id` values and surface summaries. |
| G05 Exact-bucket compactness | A synthetic 8,000-row exact bucket emits one compact bucket assertion and zero pairwise expansion. |
| G06 Cannot-link veto | A high-support pair with hard cannot-link is never auto-merged. |
| G07 Relation hint non-merge | Relation hints appear in evidence/review artifacts but do not increase merge score. |
| G08 Review grouping | Repeated ambiguity across many deals appears as one grouped review item with row/deal counts. |
| G09 Decision ledger continuity | Review import and promotion record immutable decision events with artifact hashes. |
| G10 Cache correctness | A rerun with unchanged hashes reuses caches; a changed strategy/profile/patch invalidates the right caches. |
| G11 CMBS 500k-row scale | CMBS-shaped 500k-row fixture completes with bounded candidates and deterministic output. |
| G12 500k-unique stress | Synthetic 500k-unique-surface run stays within configured candidate caps or refuses deterministically. |
| G13 Reg AB boundary | `sec10d` raw parser fields remain unchanged after entity apply/enrichment. |
| G14 Promotion gate | Promotion refuses stale/missing/failed audit artifacts. |
| G15 Explainability | `explain` reconstructs normalized views, candidates, support, anti-merge evidence, review decisions, and promotion provenance. |

Initial benchmark budgets should be conservative and revised with real
measurements:

```text
candidate pairs per unique surface p95 <= 25
candidate pairs per unique surface p99 <= 100
exact-bucket pair expansion = 0
review items per 500k-row tenant backfill <= 2,000 unless explicitly waived
cache-hit rerun avoids rebuilding normalization and postings
```

Do not encode aspirational wall-clock claims until a baseline benchmark exists.
Record real hardware, build profile, input size, unique-surface count, and cache
state with every performance result.

The concrete eval scorecard and initial wall-clock targets are centralized in
[`ENTITY_EVALS_AND_PERFORMANCE.md`](ENTITY_EVALS_AND_PERFORMANCE.md). That
contract sets scorecards for false-merge weighted loss, adversarial anti-merge,
review queue quality, perturbation robustness, determinism, differential
baseline parity, registry mutation safety, explainability, review goldens,
metamorphic invariants, holdout protocol, runtime guards, and peak memory.

Initial timing targets:

```text
small CI fixture target < 1s
CMBS public sample target < 2s
sec10d full baseline target < 10s
CMBS 500k warm-cache target < 2min
CMBS 500k cold-cache target < 5min
CMBS 500k exact apply target < 15s
500k-unique stress = bounded completion or deterministic refusal
```

Normal CI asserts structure, determinism, and fixture coverage. Wall-clock
targets become release/operator gates only after telemetry-backed baselines are
recorded.

---

## Test Matrix

The implementation should create packet-level tests from this matrix. Test IDs
are stable handles for beads and CI output.

### Namekit Unit Tests

| ID | Input | Expected |
|----|-------|----------|
| NK-U001 | `SEARS LLC` | `tenant_core = sears`, legal suffix reason emitted |
| NK-U002 | `Sears, Roebuck and Co.` | punctuation folded, suffix stripped, stable tokens |
| NK-U003 | `PNC Bank, National Association` | Reg AB firm normalizer preserves bank/legal tokens required by profile |
| NK-U004 | non-ASCII accented name | deterministic Unicode fold with reason code |
| NK-U005 | reordered whitespace/punctuation variants | same normalized view, same reason-code ordering |

### Prepare And Surface Tests

| ID | Fixture | Expected |
|----|---------|----------|
| EN-P001 | duplicate tenant rows across deals | one prepared surface with correct row/deal counts |
| EN-P002 | same rows in different order | byte-identical surface IDs and sorted output |
| EN-P003 | malformed `alias_surfaces_json` | `E_ENTITY_INPUT_CONTRACT` refusal |
| EN-P004 | synthetic surface hash collision fixture | `E_ENTITY_SURFACE_ID_COLLISION` refusal |
| EN-P005 | existing registry alias | prepared surface marked exact-resolved before blocking |

### Blocking And Edge Tests

| ID | Fixture | Expected |
|----|---------|----------|
| EN-B001 | 8,000 identical `Sears` rows | compact bucket assertion, zero pairwise exact edges |
| EN-B002 | large common-token bucket | posting cap diagnostic and bounded candidates |
| EN-B003 | `Sears` vs `SEARS LLC` | support edge generated |
| EN-B004 | `Sears` vs `Sears Auto Center` | support plus anti-merge/relation evidence |
| EN-B005 | candidate cap exceeded | `E_ENTITY_CANDIDATE_BUDGET` refusal or bounded abstention per command policy |

### Solver Tests

| ID | Fixture | Expected |
|----|---------|----------|
| EN-S001 | high support, no negatives | promotable cluster |
| EN-S002 | high support plus hard cannot-link | abstain/contradiction, no merge |
| EN-S003 | relation hint only | relation exported, no merge |
| EN-S004 | exact resolved incumbent overlap | inherit existing canonical ID when no conflicts |
| EN-S005 | multiple incumbent overlap | abstain conflict and emit cannot-link/proof |

### Review, Promotion, Apply Tests

| ID | Fixture | Expected |
|----|---------|----------|
| EN-R001 | repeated ambiguity across 100 rows/20 deals | one review item with row/deal counts |
| EN-R002 | review import with stale result hash | `E_ENTITY_REVIEW_IMPORT` refusal |
| EN-R003 | operator confirms distinct | decision ledger event plus cannot-link sidecar |
| EN-PR001 | promotion with stale audit | `E_ENTITY_AUDIT_GATE` refusal |
| EN-PR002 | accepted aliases | exact registry entries written, version bumped, lint clean |
| EN-A001 | apply after promotion | raw fields preserved and canonical fields appended |

### Profile Integration Tests

| ID | Profile | Fixture | Expected |
|----|---------|---------|----------|
| CMBS-I001 | `cmbs_tenant_label` | Sears aliases | `TNT-SEARS` promotable/resolved |
| CMBS-I002 | `cmbs_tenant_label` | Sears/Kmart/Transform hard negatives | no silent collapse |
| CMBS-I003 | `cmbs_tenant_label` | 500k-row synthetic book | gates G05/G08/G11 pass |
| REGAB-I001 | `regab_firm_identity` | current `sec10d` `org_mentions.csv` shape | prepare accepts fields directly |
| REGAB-I002 | `regab_firm_identity` | PNC vs Midland | distinct/review, no parent collapse |
| REGAB-I003 | `regab_firm_identity` | platform/category label | not auto-resolved as firm |
| REGAB-I004 | `regab_firm_identity` | apply/enrich | raw parser fields unchanged |

---

## Operator Ergonomics

The workbench should be fast, but also pleasant to operate.

Required surfaces:

1. **Profile templates**

```bash
canon entity profile list
canon entity profile init cmbs_tenant_label --output strategy.yaml
```

2. **Readable summaries**

```text
3,000 deals
500,000 tenant rows
61,423 raw unique names
27,118 normalized unique surfaces
18,044 already resolved by exact registry
7,831 newly promotable aliases
912 grouped review items
331 anti-merge protected surface groups
top unresolved tokens: ...
top anti-merge reasons: ...
```

3. **Explain one decision**

```bash
canon entity explain result.json --row row-123 --emit summary
```

Should show:

- normalized views
- candidate neighbors
- positive evidence
- anti-merge evidence
- final solver action
- registry writeback eligibility

4. **Review queues**

Operators need CSV and JSON review exports with enough context to decide:

- grouped ambiguity key
- affected row count
- affected deal count
- representative provenance examples
- left/right surfaces
- normalized names
- proposed canonical ID
- support evidence
- anti-merge evidence
- relation hints
- suggested review action

Review queues must be grouped by surface cluster or ambiguity pattern, not by
raw row. Operators should not review the same `Sears Auto Center` ambiguity
hundreds of times because it appears in many deals.

5. **Patch generation from review**

The review import should be able to update:

- exact alias registry entries
- alias patch files
- distinctness patch files
- cannot-link sidecars

6. **Doctor/lint integration**

`canon doctor` or registry lint should detect:

- duplicate patch entries
- alias and distinct patch conflicts
- profile/strategy mismatch
- unsupported operators
- overly broad token stripping
- sidecar snapshot drift

---

## Packet Decomposition

This plan is too large for one implementation bead. Convert it into packets in
this order so each packet has crisp contracts and can be reviewed independently.

| Packet | Builds | Blocks |
|--------|--------|--------|
| ENT-P00 | Finalize `canon entity` artifact constants, invariants, error codes, and profile schema. | Every other packet. |
| ENT-P01 | Direct `org` -> `entity` namespace migration: CLI, modules, tests, docs, operator metadata, artifact constants. | ENT-P02+ |
| ENT-P02 | `src/namekit` foundation: normalization, legal suffixes, tokenization, reason codes, golden tests. | ENT-P03, ENT-P04, profiles. |
| ENT-P03 | `prepare`: profile input mapping, unique surfaces, deterministic `surface_id`, exact lookup status, prepared artifact. | ENT-P04, ENT-P05. |
| ENT-P04 | `index build`: token/IDF/ngram indexes, cache keys, stale-cache diagnostics. | ENT-P05. |
| ENT-P05 | `block`: compact exact buckets, bounded top-k candidates, candidate diagnostics. | ENT-P06. |
| ENT-P06 | `edge`: string metrics, TF-IDF evidence, anti-merge operators, relation hints. | ENT-P07. |
| ENT-P07 | Signed-graph solver: hard cannot-link constraints, relation non-merge, review groups. | ENT-P08, ENT-P09. |
| ENT-P08 | Decision ledger and review import/export: grouped review queues, immutable decisions, patch derivation. | ENT-P09. |
| ENT-P09 | Audit/promote/apply: gates, registry writes, sidecars, exact replay, raw-field preservation. | Profiles and integrations. |
| ENT-P10 | `cmbs_tenant_label` profile: tenant strategy, hard negatives, 500k-row shaped benchmark. | ENT-P12. |
| ENT-P11 | `regab_firm_identity` profile: `sec10d` observation shape, firm guards, hard negatives. | ENT-P13. |
| ENT-P12 | CMBS tenant backfill end-to-end packet: prepare -> run -> review -> promote -> apply. | Release readiness. |
| ENT-P13 | `sec10d` migration packet: replace exact-only helper orchestration with `canon entity` stages. | Release readiness. |
| ENT-P14 | Performance/caching packet: benchmark harness, cache-hit tests, memory/candidate diagnostics. | Release readiness. |
| ENT-P15 | Doctor/lint/operator ergonomics packet: profile init/list, summaries, explain, robot diagnostics. | Release readiness. |

Each packet must carry:

- relevant invariant IDs
- relevant error codes
- artifact schemas touched
- concrete tests from the matrix
- exact verification commands
- rollback or migration notes

### Parallel Implementation Lane Map

The Entity Workbench implementation is expected to run with multiple agents in
one shared checkout. Agents should claim the narrowest ready Bead in their lane,
reserve only the exact file or files they will write, and avoid directory-wide
or broad-glob reservations. The default reservation shape is one implementation
file plus one test or fixture file. Parent packet Beads such as ENT-P00 through
ENT-P15, and the overall `bd-25k` feature Bead, are orchestration or acceptance
Beads unless they have been split into exact file-level tasks.

If a lane has no ready Bead, agents should unblock the highest-priority ready
P0/P1 contract Bead whose files are unreserved. Critical-path contract Beads
include `bd-3b4.*` for entity contracts, `bd-1fm.*` for the direct `org` to
`entity` namespace migration, and stage contracts `bd-18g.6`, `bd-486.6`,
`bd-486.7`, and `bd-39z.6`.

| Lane | Owns | Default File Roots | Test And Fixture Roots | Critical Blockers |
|------|------|--------------------|------------------------|-------------------|
| contracts/CLI | ENT-P00, ENT-P01, artifact constants, profile/refusal contracts, public command surface, operator metadata. | `src/entity/contracts.rs`, `src/entity/mod.rs`, `src/cli.rs`, `operator.json`, targeted docs sections. | `tests/entity/contracts.rs`, CLI smoke tests, contract snapshots. | `bd-3b4.*`, `bd-1fm.*`. |
| namekit | ENT-P02 normalization, legal suffixes, tokens, n-grams, TF-IDF primitives, similarity score units, reason codes. | `src/namekit/*.rs`, with one module reserved per Bead. | `tests/namekit/*.rs`, `tests/fixtures/namekit/**`. | `bd-3k3.21`, `bd-3k3.12`, `bd-3k3.22`. |
| prepare/surface | ENT-P03 profile input mapping, projection, prepared surfaces, deterministic `surface_id`, exact lookup status. | `src/entity/prepare.rs`, `src/entity/projection.rs`, surface metadata helpers. | `tests/entity/prepare.rs`, `tests/fixtures/entity/prepare/**`. | `bd-18g.6` and prepare artifact contracts. |
| index/block | ENT-P04 and ENT-P05 index build, postings, exact-bucket assertions, candidate caps, blocking diagnostics. | `src/entity/index.rs`, `src/entity/block.rs`; use `src/namekit/tfidf.rs` only through namekit-owned contracts. | `tests/entity/index.rs`, `tests/entity/block.rs`, `tests/fixtures/entity/block/**`. | `bd-486.6`, `bd-486.7`, `bd-486.9`, `bd-3bu.5`. |
| edge/solve | ENT-P06 and ENT-P07 support/cannot-link/relation edges, integer scores, signed graph solve, contradiction handling. | `src/entity/edge.rs`, `src/entity/solve.rs`, shared score adapters only when their owning Bead is ready. | `tests/entity/edge.rs`, `tests/entity/solve.rs`, `tests/fixtures/entity/solve/**`. | `bd-39z.6`, ENT-P06 score-unit contracts, ENT-P07 solver contracts. |
| review/registry/apply | ENT-P08 and ENT-P09 decision ledger, review import/export, audit/promote/apply, registry writes, exact replay. | `src/entity/review.rs`, `src/entity/audit.rs`, `src/entity/promote.rs`, `src/entity/apply.rs`, registry writer helpers. | `tests/entity/review.rs`, `tests/entity/promote.rs`, `tests/entity/apply.rs`, review golden fixtures. | ENT-P08 ledger contracts, ENT-P09 mutation safety gates. |
| profiles/workflows | ENT-P10 through ENT-P13 CMBS tenant and Reg AB profiles, profile fixtures, migration workflow examples. | Profile-specific strategy/config files and narrowly scoped profile modules after contracts exist. | `tests/fixtures/entity/cmbs/**`, `tests/fixtures/entity/regab/**`, profile integration tests. | `bd-sbm.7`, CMBS/Reg AB benchmark manifests, profile firewall contracts. |
| evals/perf/ergonomics | ENT-P14 and ENT-P15 eval harness, benchmark telemetry, doctor/lint summaries, explain/operator ergonomics. | `tests/entity_eval_performance_contract.rs`, benchmark harness modules, doctor/lint output surfaces. | `tests/fixtures/entity/evals/**`, ignored stress fixtures, summary golden artifacts. | `bd-1pz.9`, `bd-2nw.6`, final acceptance sweep Beads. |

Cross-lane edits must be explicit in the Bead description. If a task needs a
shared type or module export, land the owning contract Bead first or split the
task so the owner edits the shared file. Runtime implementation Beads should
not also rewrite benchmark prose unless the acceptance criteria specifically
require a docs update.

---

## Adversarial Readiness Update

This section records the follow-up adversarial pass over the plan and Beads.
The goal is to remove implementation-time ambiguity before broad coding starts.
Where there is a tradeoff between a smaller implementation and better entity
resolution quality, choose the higher-quality entity-resolution path as long as
the exact lookup kernel, determinism, and local execution constraints remain
intact.

### Implementation-Quality Defaults

These decisions replace vague "decide/design/prove" language in the packet
graph:

1. **Recall-rich candidate generation wins.**
   Namekit and blocking should include word tokens, char n-grams, sparse TF-IDF
   support, rare-token weighting, deterministic top-k retrieval, and source
   parity fixtures. Do not simplify to plain edit distance or one fuzzy
   threshold.

2. **Sparse integer layouts are the default.**
   Use compact dictionaries, integer token/ngram IDs, sorted posting lists,
   CSR-like offset arrays where useful, and posting-list derived top-k
   retrieval. Dense per-surface vectors are forbidden for large corpora.

3. **Sorted-neighborhood is supplemental, not authoritative.**
   It may improve recall when it has a deterministic key/window contract and
   diagnostics, but it must not be the only candidate path and must not bypass
   caps, anti-merge evidence, or review.

4. **Similarity dependency default is source-parity first.**
   Prefer a pinned native Rust metric implementation or crate with strong
   parity tests, byte/Unicode paths, cutoff/hint correctness tests, and license
   review. A dependency is acceptable only if it respects the repository's
   `#![forbid(unsafe_code)]` posture or is explicitly isolated behind a
   reviewed boundary. If that audit fails, implement the required metrics
   internally.

5. **Scores are canonical integers at boundaries.**
   Internal metric math may use practical numeric representations, but every
   score used for thresholds, top-k ordering, artifact output, review, and
   solve must be quantized to deterministic integer score units. Floating point
   debug output is never part of an artifact contract.

6. **Budget policy is explicit and stage-specific.**
   Index/block hard limit breaches refuse before large artifact emission.
   Edge refuses stale or malformed candidate input before scoring. Solve may
   emit bounded abstentions only when configured and must record the affected
   component. Apply refuses full-resolution mode when unresolved surfaces
   remain. Every breach uses an `E_ENTITY_*` code plus `next_command`.

7. **Exact buckets are hyperedges.**
   Exact normalized buckets emit compact bucket assertions with O(N) membership
   and `pair_expansion = forbidden`. Edge and solve consume these as
   hyperedges/cluster assertions while preserving cannot-link veto checks.
   Exact-bucket all-pairs expansion is a stop-ship regression.

8. **Performance gates start from measured baselines.**
   Normal CI proves deterministic small fixtures and structural caps. Large
   500k-row and 500k-unique runs are ignored/operator tiers with telemetry.
   Do not encode aspirational wall-clock claims until a baseline run records
   hardware, build profile, cache state, row counts, unique-surface counts,
   candidate counts, artifact sizes, and timings.

9. **sec10d compatibility needs local frozen fixtures.**
   The Reg AB profile must carry canon-owned fixture snapshots for
   `org_mentions` input shape, parser-field preservation, Snowflake append-only
   output, and hard-negative examples. Do not rely on a moving external repo
   shape to define canon correctness.

10. **Final operator ergonomics is a release gate.**
    Operator journey, robot JSON, summaries, next commands, doctor/lint, and
    explain should be validated after the underlying packets exist. It is a
    fixture-driven acceptance sweep, not a catch-all implementation bead.

### Beads Flagged As Readiness Risks

The adversarial pass flagged these Beads for strengthening:

| Bead | Required tightening |
|------|---------------------|
| `bd-3k3.11` | Turn TF-IDF/sparse/top-N from open design into a locked Rust data-layout and scoring contract. |
| `bd-3k3.12` | Resolve metric dependency posture before importing a crate. |
| `bd-3bu.5` | Align index CSR/posting layout with the namekit sparse decision so two packets do not choose incompatible layouts. |
| `bd-39z.6` | Make integer score units and deterministic tie ordering a prerequisite to edge scoring. |
| `bd-486.6` | Add a stage-by-stage refusal vs bounded-abstention policy table. |
| `bd-486.7` | Specify the exact-bucket hyperedge API and record-count proof, not only a prose no-O(N^2) goal. |
| `bd-1pz.5` / `bd-1pz.6` / `bd-1pz.8` | Separate small CI checks from ignored/operator stress tiers and ground budgets in measured baselines. |
| `bd-2c6.6` | Require an executable mini e2e runbook fixture, not only prose commands. |
| `bd-sbm.6` | Freeze local sec10d contract fixtures inside canon. |
| `bd-2nw.6` | Treat as final acceptance sweep over completed packets, not implementation scope. |

### Readiness Gate Beads Added

The Beads graph now carries explicit gates for the adversarial findings:

| Bead | Purpose |
|------|---------|
| `bd-3b4.10` | Implementation readiness appendix, module skeleton, and packet-local contract/test coverage matrix. |
| `bd-3k3.21` | High-recall namekit source-parity implementation decisions: sparse layout, metrics, score quantization, sorted-neighborhood posture. |
| `bd-486.9` | Blocking budget table and exact-bucket hyperedge readiness proof. |
| `bd-1pz.9` | Measured performance baseline artifacts for stress-gate calibration. |
| `bd-sbm.7` | Local canon-owned sec10d contract fixture snapshots. |

### Escalations Requiring Owner Judgment

The implementation should proceed with the defaults above unless one of these
judgment points is hit:

1. **Unsafe or native-code metric dependency.**
   If the best similarity crate requires unsafe code, native extensions, or a
   large transitive dependency surface, pause for owner approval rather than
   weakening source-parity and quality silently.

2. **Repository size for stress fixtures.**
   Prefer deterministic fixture generators plus small golden outputs. Escalate
   before committing large generated 500k-row artifacts.

3. **Auto-merge aggressiveness.**
   Default to high precision and review/escrow for ambiguous tenant-family or
   parent/subsidiary cases. Escalate only if product goals require materially
   higher automatic recall at the cost of more over-merge risk.

---

## Reality Check Findings

Current implementation reality:

- `canon` still exposes the existing `org` namespace in code.
- there is no `canon entity` CLI yet.
- there is no `src/namekit` module yet.
- there are no `prepare`, `index build`, `block`, `edge`, `solve`,
  `review`, `promote`, `apply`, or `explain` entity subcommands yet.
- current Beads in the repo do not decompose this plan into implementation
  packets.

Plan readiness:

- This document is an architecture and packet plan, not a single
  implementation task.
- It is strong enough to begin packetization.
- It is not ready to hand to an agent as "build all of this" without first
  creating packet-level Beads with contracts, tests, and verification commands.

First implementation move:

```text
create Beads ENT-P00..ENT-P15 from Packet Decomposition
attach invariant/test/gate IDs to each bead
land ENT-P00 before touching broad implementation
```

The largest risks are not whether fuzzy matching can be added. The real risks
are over-merge, unbounded candidate generation, stale cache reuse, review queue
flooding, and ambiguous profile semantics. The plan therefore treats
anti-merge, signed graph constraints, artifact hashes, and quality gates as
first-class deliverables, not polish.

---

## Implementation Phases

### Phase 0: Plan And Compatibility Decision

- Add this plan.
- Adopt a direct breaking rename from `canon org` to `canon entity`.
- Define artifact version migration policy as a clean `canon_entity_*` schema
  family with no legacy `canon_org_*` compatibility layer.

### Phase 1: Entity Namespace

- Rename public docs and CLI target from `org` to `entity`.
- Rename generic internal types where useful:
  - `OrgStrategy` -> `EntityStrategy`
  - `OrgError` -> `EntityError`
  - artifact constants to `CANON_ENTITY_*`
- Remove `org` as the public command namespace.
- Do not keep an alias, dual parser path, or legacy artifact reader.
- Rename file/module paths from `org` to `entity` unless an internal name is
  deliberately domain-specific.

### Phase 2: Namekit Foundation

- Add `src/namekit`.
- Move existing suffix/stopword/tokenize logic out of `org/block.rs` and
  `org/edge.rs`.
- Add golden tests for normalization and fingerprints.

### Phase 3: CMBS Tenant Backfill Profile

- Add `cmbs_tenant_label` strategy fixture.
- Add tenant-specific normalization and noise stripping.
- Add alias/distinct patch fixture format.
- Define required input observation fields:
  - `deal_id`
  - `loan_id`
  - `property_id` when available
  - `source_doc` / source provenance
  - `raw_tenant_name`
  - optional suite/store/unit/context fields
- Add profile-level identity semantics for `canonical_display_label`.

### Phase 4: Prepare, Dedupe, And Global Index

- Add `canon entity prepare`.
- Add prepared-surface artifacts with raw row counts, normalized views,
  provenance samples, exact-registry lookup status, and unresolved-surface
  markers.
- Add deterministic `surface_id` generation and collision reporting.
- Add `canon entity index build`.
- Add hash-keyed caches for normalized surfaces, token dictionaries, IDF, and
  n-gram postings.
- Ensure physical ingestion can batch by deal while logical solving uses one
  global surface/index view.

### Phase 5: Native Similarity And Candidate Generation

- Add string metrics.
- Add n-gram candidate index.
- Add top-k pruning and deterministic tie-breaking.
- Add term-frequency weighting.
- Prevent exact-view bucket pair explosion by representing exact buckets as
  compact cluster assertions.
- Add hard caps for candidates per unique surface, operator, and run.
- Add blocking quality metrics: candidate recall, pairs per surface, suppressed
  candidates, and large bucket diagnostics.

### Phase 6: Anti-Merge Evidence

- Add protected-token conflict.
- Add alias patch distinctness.
- Add related-distinct phrase detection.
- Add solver policy tests showing cannot-link vetoes strong support.
- Add profile-scoped negative knowledge so cannot-link facts do not leak across
  identity semantics.

### Phase 6b: Signed-Graph Solver And Decision Ledger

- Represent support, cannot-link, and relation hints as signed evidence graph
  lanes.
- Enforce hard cannot-link constraints during component solving.
- Add component diagnostics showing strongest positive and negative cuts.
- Add immutable decision ledger events for review import, overrides, promotion,
  and reverts.
- Add active review priority scoring using row/deal blast radius, uncertainty,
  evidence conflict, and patch reuse value.

### Phase 7: Audit And Review Smoothness

- Add hard-negative suite for tenant labels:
  - `Sears` vs `Sears Auto Center`
  - `Sears` vs `Kmart`
  - `Sears` vs `Sears Holdings`
  - `Sears` vs `Transform SR LLC`
- Add summary output for anti-merge reasons.
- Add review export/import fields for merge and anti-merge evidence.
- Group review rows by surface cluster/ambiguity pattern with affected row and
  deal counts.
- Add review-import support for alias patches, distinctness patches,
  cannot-link sidecars, and exact alias registry entries.
- Add review priority reason codes and decision ledger linkage.

### Phase 8: Performance Hardening

- Add 500k-row CMBS-shaped scaling fixture/tests.
- Add 500k-unique-surface synthetic stress tests.
- Benchmark candidate generation.
- Optimize token IDs/postings if needed.
- Verify no accidental row-level or exact-bucket O(n^2) path remains for large
  tenant profiles.
- Verify reruns reuse hash-keyed prepared/index caches when inputs are unchanged.
- Record benchmark metadata: hardware, build profile, cache state, row count,
  unique-surface count, candidate count, and artifact sizes.

### Phase 9: Promotion And Exact Replay

- Promote accepted tenant aliases into a normal registry.
- Add `canon entity apply` or an equivalent replay command that applies the
  promoted exact registry to every original tenant row.
- Verify ordinary `canon` lookup remains exact:

```bash
canon tenants.csv --registry registries/cmbs-tenants --column tenant_name
```

### Phase 10: sec10d Reg AB Firm Profile

- Add `regab_firm_identity` strategy fixture.
- Accept the existing `sec10d` `org_mentions.csv` shape as a first-class
  profile input.
- Define primary surfaces, alias surfaces, mention surfaces, context fields,
  and anchors for Reg AB firm mentions.
- Add firm-specific anti-merge operators or configured guards for:
  - parent/subsidiary distinction
  - bank vs loan-services division
  - servicer/subservicer/agent role conflicts
  - platform/category labels
  - auditor vs subject-party role conflicts
- Add hard-negative suite examples from current `sec10d` policy:
  - PNC Bank vs Midland Loan Services
  - reporting party vs platform/category label
  - auditor firm vs subject party
  - parent bank vs regulated subsidiary/servicer entity

### Phase 11: sec10d Pipeline Migration

- Update the `sec10d` downstream enrichment helper to call `canon entity`
  subcommands rather than bespoke exact-only orchestration.
- Preserve the existing parser boundary: raw `sec10d` JSONL fields stay
  unchanged; canonical fields are appended downstream.
- Keep exact core `canon` lookup as the production replay/apply stage after
  registry promotion.
- Replace future `canon org` documentation and Beads language in `sec10d` with
  `canon entity`.
- Ensure Snowflake-facing enriched outputs preserve:
  - canonical org fields
  - resolution status
  - registry id/version
  - unresolved/reviewable status
  - raw parser evidence fields

---

## Definition Of Done

The first usable milestone is complete when:

- `canon entity` exists as the intended workbench namespace.
- no current public CLI, docs, operator metadata, or artifact constants still
  present `canon org` as the active workbench namespace.
- ENT-P00..ENT-P15 have been converted into Beads or equivalent packet records
  before implementation work is treated as tracked.
- packet Beads include invariant IDs, error codes, artifact contracts, tests,
  verification commands, and rollback/migration notes.
- `cmbs_tenant_label` is implemented as the first validated profile.
- the workbench can ingest a 3,000-deal historical CMBS tenant corpus in
  physical batches while solving against one global unique-surface view.
- candidate generation, edge scoring, and solving operate on unique tenant
  surfaces, not raw rows.
- exact normalized buckets do not emit pairwise edges.
- a 500k-row CMBS-shaped benchmark completes with bounded candidate counts.
- the 500k-unique-surface synthetic stress test either completes within
  configured candidate caps or refuses deterministically with a documented
  `E_ENTITY_*` error.
- all fuzzy/native scoring happens before promotion only.
- no frontier-model call, network dependency, Python ML runtime, or runtime
  model download is required by the `canon entity` workbench.
- anti-merge evidence can veto high string similarity.
- signed-graph solving enforces hard cannot-link constraints.
- relation hints are exported for ontology consumers but do not contribute
  positive merge score by default.
- review queues are grouped by ambiguity pattern with affected row/deal counts.
- active review ranking exposes reason codes and expected blast radius.
- review import writes immutable decision-ledger events before deriving aliases,
  distinctness patches, cannot-link sidecars, or registry changes.
- accepted aliases promote into an exact lookup registry.
- exact replay produces canonical tenant IDs for every historical row that now
  has a registry match.
- performance tests prove bounded candidate generation, exact-bucket
  compactness, deterministic ordering, and cache reuse.
- artifact hash continuity is enforced across prepare/index/block/edge/solve,
  review import, audit, promote, and apply.
- `canon entity explain` can reconstruct normalized views, candidates, support
  evidence, anti-merge evidence, review decisions, and promotion provenance for
  a row, surface, or entity.
- `regab_firm_identity` is implemented as the second validated profile.
- the existing `sec10d` Reg AB org enrichment path can be expressed through
  `canon entity prepare/run/review/promote/apply` without changing raw parser
  output semantics.
- `sec10d` enriched outputs append canonical fields while preserving raw parser
  evidence fields and Snowflake-facing contracts.
- quality gates G01..G15 have explicit pass/fail evidence for the release
  candidate.
- the test matrix has packet-level CI coverage or documented waived items.
- docs make clear that cross-profile alignment belongs to the ontology layer.
