# Entity Evals And Performance Targets

> Purpose: define the common eval scorecard, performance gates, wall-clock
> targets, telemetry, and run commands for `canon entity`.
>
> Machine-readable contract:
> `tests/fixtures/entity/evals/entity_eval_performance_targets.json`

This document is the working target for implementation agents. The CMBS tenant
and sec10d Reg AB benchmark docs define domain-specific fixtures; this document
defines the shared eval harness and performance bar those fixtures must feed.

Do not treat this as a loose checklist. Every release claim should identify the
eval ids it passed, the fixture or corpus used, the artifact hashes, and the
telemetry file for the run.

---

## How To Run

These commands work now and guard the eval/performance contract itself:

```bash
cargo test --test entity_eval_performance_contract -- --nocapture
cargo test sec10d_regab -- --nocapture
jq '.' tests/fixtures/entity/evals/entity_eval_performance_targets.json
jq '.' tests/fixtures/entity/cmbs/tenant_sample_benchmark_manifest.json
jq '.' tests/fixtures/entity/regab/sec10d_regab_benchmark_manifest.json
```

These are target commands for future implementation Beads:

```bash
# Small normal-CI evals.
cargo test entity_eval_ci_small -- --nocapture
cargo test entity_perf_ci_small -- --nocapture

# Full public samples when local public-data files are available.
canon entity eval run \
  --suite tests/fixtures/entity/evals/entity_eval_performance_targets.json \
  --profile cmbs_tenant_label \
  --input /Users/zacharyruiz/Downloads/tenant_sample_cmbs.csv \
  --work-dir target/entity-evals/cmbs-public \
  --emit json

canon entity eval run \
  --suite tests/fixtures/entity/evals/entity_eval_performance_targets.json \
  --profile regab_firm_identity \
  --input /Users/zacharyruiz/Downloads/sec10d_regab_org_canon_baseline_20260623T204557Z.zip \
  --work-dir target/entity-evals/sec10d-regab \
  --emit json

# Operator stress tiers.
cargo test --ignored cmbs_500k_stress -- --nocapture
cargo test --ignored entity_500k_unique_stress -- --nocapture
```

Every future eval run should write:

```text
target/entity-evals/<suite>/<run_id>/eval.json
target/entity-evals/<suite>/<run_id>/telemetry.json
target/entity-evals/<suite>/<run_id>/summary.md
target/entity-evals/<suite>/<run_id>/artifacts/
```

The JSON result is the source of truth. The markdown summary is for operators.

---

## Eval Tiers

| Tier | Purpose | CI status | Wall-clock enforcement |
|------|---------|-----------|------------------------|
| `contract` | Parse manifests, verify fixture coverage, verify score/target schema. | Normal CI | Yes, should be sub-second but not measured as product performance. |
| `small-ci` | Hand-labeled goldens, adversarial pairs, determinism, exact replay. | Normal CI | Structural only; no brittle timing gates. |
| `public-sample` | Full public CMBS tenant sample and full sec10d baseline. | Local/nightly/operator | Initial wall-clock targets apply after baseline calibration. |
| `stress` | 500k-row and 500k-unique generated workloads. | Ignored/operator | Hardware-scoped target budgets and deterministic refusal gates. |

Normal CI must never require the 500k stress workload or the full sec10d zip.
It must still prove that the benchmark contract is real and that fixtures are
parseable, representative, and anti-theatrical.

---

## Correctness Scorecard

### `ER-SCORE-001`: Cluster And Pairwise Metrics

Every labeled eval suite should report:

```text
pairwise_precision = true_positive_pairs / predicted_positive_pairs
pairwise_recall    = true_positive_pairs / gold_positive_pairs
pairwise_f1        = harmonic_mean(pairwise_precision, pairwise_recall)
cluster_precision  = exact_or_subset cluster precision over labeled surfaces
cluster_recall     = exact_or_subset cluster recall over labeled surfaces
b_cubed_precision  = mean per-item precision
b_cubed_recall     = mean per-item recall
b_cubed_f1         = harmonic_mean(b_cubed_precision, b_cubed_recall)
```

Gates:

- curated hard-negative false merges: `0`;
- Reg AB hierarchy anti-collapse false merges: `0`;
- must-link candidate recall on labeled goldens: `>= 0.995`;
- small-CI pairwise precision on labeled decisions: `>= 0.995`;
- small-CI pairwise recall on labeled must-links: `>= 0.98`;
- any miss reports the exact surface ids, raw labels, evidence, and stage.

### `ER-SCORE-002`: False-Merge Weighted Loss

False merges are more expensive than false splits. Every scored suite should
emit:

```text
weighted_loss =
  10.0 * false_merge_rate
+  2.0  * false_split_rate
+  1.0  * missed_candidate_rate
+  0.25 * review_over_budget_rate
```

Initial gate:

```text
weighted_loss <= 0.02 for small-ci labeled suites
weighted_loss <= 0.05 for public-sample suites after baseline calibration
```

Hard-negative false merges are stop-ship even if weighted loss is otherwise
low.

### `ER-ADV-001`: Adversarial Anti-Merge Suite

Must cover:

- parent vs subsidiary;
- bank parent vs loan-services division;
- same brand family but different operator;
- same numeric/address-like token but different entity;
- same root token but different legal entity;
- tenant shell vs real tenant;
- platform/category label used as if it were a firm;
- d/b/a, slash, and parenthetical relation cases.

Expected:

- hard-negative pairs never auto-merge;
- relation hints do not add merge score by default;
- accepted same-as for predecessor/subsidiary wording requires an explicit
  reviewed rule id or patch.

### `ER-REVIEW-001`: Review Queue Quality

Review quality is scored independently from merge quality.

Required metrics:

```text
review_group_count
review_rows_covered
review_deals_covered
review_groups_per_1k_raw_rows
duplicate_review_group_rate
review_groups_with_positive_evidence_rate
review_groups_with_antimerge_evidence_rate
review_groups_with_next_action_rate
```

Gates:

- repeated ambiguity groups once by surface cluster or pattern;
- every review row has positive evidence, anti-merge evidence, or an explicit
  uncertainty reason;
- every review row proposes one of `alias`, `distinct`, `relation`, `escrow`,
  or `needs_policy`;
- duplicate review group rate is `0` on small-CI suites.

### `ER-PERTURB-001`: Mutation And Noise Robustness

Generated variants should include:

- case and punctuation changes;
- legal suffix variants;
- spacing and duplicate whitespace;
- OCR-like single-character errors;
- abbreviations such as `National Association` / `N.A.`;
- store/suite/unit/number noise for tenant labels.

Expected:

- candidate recall remains above target for identity-preserving mutations;
- identity-critical protected tokens still block unsafe merges;
- mutation generators are deterministic by seed and record provenance.

### `ER-DET-001`: Determinism And Batch Equivalence

Each suite should run at least:

```text
same input twice
row-shuffled input
different physical batch sizes
cold cache
warm cache
```

Expected:

- same prepared surface ids;
- same candidate sets after canonical sort;
- same edge artifacts after canonical sort;
- same solve/review groups;
- same promotion decisions for the same reviewed patches;
- same apply output after sorting by `source_row_id`.

### `ER-DIFF-001`: Legacy And Baseline Differential Eval

For Reg AB, current exact lookup is the floor.

Expected:

- the migrated entity path reproduces the 46-surface sec10d baseline mappings;
- raw parser fields remain append-only;
- any difference is classified as `intentional_improvement`, `review_required`,
  or `refusal`, never silent drift.

For CMBS, the public tenant sample is the first benchmark. Future public
holdouts should be added as `cmbs-public-v2`, `cmbs-public-v3`, and so on rather
than tuning only to one sample.

---

## Structural Performance Gates

These are non-negotiable. They apply before wall-clock targets:

| Gate | Required value |
|------|----------------|
| Row-level all-pairs | forbidden |
| Surface-level all-pairs | forbidden outside tiny explicit fixtures |
| Exact-bucket pair expansion | `0` |
| Exact-bucket representation | compact hyperedge/assertion |
| Candidate cap | enforced per unique surface and per operator |
| Candidate p95 | `<= 25` per unique surface, unless suite-specific waiver |
| Candidate p99 | `<= 100` per unique surface, unless suite-specific waiver |
| Review groups for 500k CMBS backfill | `<= 2,000` unless waived |
| Cache hit rerun | does not rebuild normalization/postings |
| Apply/replay | streaming over raw rows |
| 500k unique stress | bounded completion or deterministic refusal |

If a structural gate fails, the run fails even if wall-clock time looks good.

---

## Wall-Clock Targets

Wall-clock targets are release/operator targets, not normal-CI gates until
measured baselines exist. Each timing result must include hardware, OS, Rust
profile, target triple, canon git sha, cache state, corpus size, unique surface
count, candidate count, and artifact sizes.

Initial targets:

| Workload | Target |
|----------|--------|
| Small CI fixture eval | `< 1s` |
| Committed sec10d 46-surface fixture | `< 1s` |
| CMBS public sample, 10,143 tenant observations / 431 raw names | `< 2s` end-to-end after implementation is complete |
| sec10d full baseline, 127,991 mentions / 46 surfaces | `< 10s` end-to-end |
| sec10d exact replay/apply over full baseline | `< 5s` |
| sec10d prepare/dedupe full baseline | `< 5s` |
| CMBS 500k rows, normal repetition, warm cache | `< 2min` end-to-end |
| CMBS 500k rows, normal repetition, cold cache | `< 5min` end-to-end |
| CMBS exact apply/replay over 500k rows | `< 15s` |
| CMBS 500k unique names | bounded completion or deterministic refusal before memory/candidate explosion |

Targets may be tightened or relaxed only by updating
`entity_eval_performance_targets.json`, recording baseline artifacts, and
explaining the change in the relevant Bead.

---

## Telemetry Contract

Every performance-capable command should emit:

```text
run_id
suite_id
profile
canon_version
git_sha
rust_profile
target_triple
os
cpu_model
logical_cores
memory_bytes
cache_state
input_hash
profile_hash
strategy_hash
registry_snapshot_hash
patch_hash
raw_row_count
raw_observation_count
raw_unique_surface_count
prepared_surface_count
exact_resolved_surface_count
candidate_pair_count
candidate_pairs_per_surface_p50
candidate_pairs_per_surface_p95
candidate_pairs_per_surface_p99
suppressed_candidate_count
exact_bucket_count
exact_bucket_pair_expansion_count
largest_exact_bucket_size
largest_component_size
edge_count
review_group_count
artifact_bytes_by_stage
timings_ms_by_stage
peak_memory_bytes
refusal_code
next_command
```

Telemetry must not include raw source rows, private operator notes, or sensitive
unredacted payloads.

---

## Eval Result Contract

The future result artifact should use this shape:

```json
{
  "schema_version": "canon.entity.eval_result.v0",
  "suite_id": "cmbs-public-v1",
  "run_id": "2026-06-25T180000Z-local",
  "profile": "cmbs_tenant_label",
  "verdict": "pass",
  "scores": {
    "pairwise_precision": 1.0,
    "pairwise_recall": 0.99,
    "b_cubed_f1": 0.995,
    "weighted_loss": 0.0
  },
  "structural_gates": {
    "exact_bucket_pair_expansion_count": 0,
    "candidate_pairs_per_surface_p95": 12,
    "candidate_pairs_per_surface_p99": 38
  },
  "wall_clock": {
    "target_seconds": 120,
    "actual_seconds": 84.2,
    "enforcement": "operator_release_target"
  },
  "artifacts": {
    "telemetry": "telemetry.json",
    "summary": "summary.md"
  }
}
```

---

## No-Theatre Rules

- An eval cannot pass only because a file exists.
- A benchmark cannot pass only because the command exited 0.
- Every merge decision must be explainable by support evidence.
- Every non-merge/review decision must expose anti-merge evidence or uncertainty.
- Any wall-clock claim without telemetry is ignored.
- Any 500k result without candidate counts is ignored.
- Any exact bucket expansion above 0 is a stop-ship regression.
- Any hard-negative false merge is a stop-ship regression.
