# Entity Evals And Performance Targets

> Purpose: define the common eval scorecard, performance gates, wall-clock
> targets, telemetry, and run commands for `canon entity`.
>
> Machine-readable contract:
> `tests/fixtures/entity/evals/entity_eval_performance_targets.json`
>
> Discovery-quality report schema:
> `schemas/canon.entity.quality.v1.schema.json`

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
cargo test --test entity_evidence_ablation -- --nocapture
cargo test sec10d_regab -- --nocapture
jq '.' tests/fixtures/canon_v1/quality/ablation_cases.json
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

# Final guardrail suites.
cargo test entity_registry_mutation_safety -- --nocapture
cargo test entity_explainability_completeness -- --nocapture
cargo test entity_review_golden_artifacts -- --nocapture
cargo test entity_metamorphic_eval -- --nocapture
cargo test entity_runtime_guard -- --nocapture
cargo test --ignored entity_peak_memory -- --nocapture
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

## Discovery-Quality Contract (`canon.entity.quality.v1`)

Exact replay is a floor, not a discovery score. `canon entity` should report
how much of an eval corpus was already solvable from the registry snapshot, but
that replay coverage cannot satisfy non-exact discovery gates. Repeating
physical exact rows may change replay row counters only; it must not change
non-exact discovery numerators, denominators, ranks, confidence intervals, or
gate outcomes.

The machine-readable report shape is
`schemas/canon.entity.quality.v1.schema.json`. It is domain-neutral: no CMBS,
Reg AB, tenant, obligor, or hierarchy-specific vocabulary belongs in the core
contract.

### Strata

Every labeled logical case must live in exactly one stratum:

| Stratum ID | Meaning | Counts toward non-exact discovery scores |
|------------|---------|------------------------------------------|
| `exact_known_replay` | Already resolvable from the input registry snapshot before discovery work begins. | No |
| `withheld_alias_incumbent` | A withheld alias should map onto an incumbent canonical entity. | Yes |
| `novel_multi_observation` | Multiple observations belong to the same novel entity and discovery must create that equivalence. | Yes |
| `directional_cross_source` | The same entity must be linked across source boundaries or directional contexts. | Yes |
| `related_or_hierarchy_distinct` | The records are related, hierarchical, or confusable, but must not auto-merge as the same entity. | Yes |
| `genuinely_unresolved` | Evidence is intentionally insufficient for auto-linking and should end in review or explicit refusal. | Yes |

The first four strata test whether the engine can discover new same-entity
knowledge safely. `related_or_hierarchy_distinct` measures anti-merge behavior.
`genuinely_unresolved` measures whether the engine can abstain honestly without
being scored as a discovery failure. A name-only ambiguity that reaches review
or explicit refusal in `genuinely_unresolved` is a correct outcome, not a
measured miss.

### Outcome Accounting

Every labeled logical case must be classified as exactly one of:

- `correct`
- `review`
- `explicit_refusal`
- `measured_miss`

Measured misses must also name one stage:

| `miss_stage` | Meaning |
|--------------|---------|
| `candidate_generation` | The true pair never surfaced in the bounded candidate set or compact exact-bucket assertion. |
| `evidence_scoring` | The true pair surfaced, but evidence/scoring failed to preserve it as a viable same-entity decision. |
| `solver` | The candidate and evidence existed, but the final deterministic decision abstained, split, or otherwise missed the labeled same-entity case. |

Over-abstain on a labeled same-entity case is therefore a `solver` miss, not a
candidate-generation miss. `exact_known_replay` is reported separately as
`correct_exact_replay`; it never enters the non-exact discovery score
denominators.

### Metric Contract

Every rate metric records `sample_count`, `numerator`, `denominator`, `value`,
and a 95% Wilson confidence interval. Rank and resource metrics still record
`sample_count`, but may set `confidence_interval_95` to `null` when no rate
interpretation exists. When a denominator is zero, the metric must emit:

```text
value = null
confidence_interval_95 = null
gate_status = not_applicable
```

Required metrics:

| Metric ID | Denominator | Notes |
|-----------|-------------|-------|
| `candidate_recall_at_50` | Labeled same-entity discovery cases in the three non-exact must-link strata | Success means the true pair surfaced within top-50 or a compact exact-bucket assertion. Excludes `exact_known_replay`. |
| `true_pair_rank` | Same denominator, restricted to surfaced same-entity cases | Report deterministic rank summaries such as p50/p95/worst. |
| `auto_link_precision` | All non-exact auto-link decisions | Penalized by every false merge, including hard negatives. |
| `auto_link_recall` | All labeled same-entity discovery cases | This is the labeled must-link recall gate. |
| `pairwise_precision` / `pairwise_recall` / `pairwise_f1` | Non-exact labeled discovery clusters only | `exact_known_replay` is excluded. |
| `b_cubed_precision` / `b_cubed_recall` / `b_cubed_f1` | Non-exact labeled discovery clusters only | `exact_known_replay` is excluded. |
| `hard_negative_false_merges` | Non-exact labeled distinct or hierarchy cases | Report the total count in `metrics.hard_negative_false_merges.value`, with the critical/high/medium/low breakout in `severity_counts`. |
| `abstention_precision` | Cases that reached review or refusal | Measures whether abstention was used on truly ambiguous cases rather than obvious misses. |
| `review_coverage` | Discovery cases that require human escalation | Measures whether the queue covers the right unresolved cases. |
| `review_yield` | Reviewed cases | Measures how often review produces durable alias/distinct/refusal knowledge. |
| `exact_replay_coverage` | `exact_known_replay` only | Reported, but never mixed into non-exact discovery gates. |
| `accounted_case_rate` | All non-exact labeled cases | Must be `1.0`: every case is correct, review, refusal, or a measured miss. |
| `candidate_pairs_per_surface_p95` / `candidate_pairs_per_surface_p99` / `wall_clock_seconds` / `peak_memory_bytes` | Resource samples | Structural/resource metrics that exact replay rows cannot improve. |

### Severity Classes

False merges must report a severity class:

| Severity | Meaning |
|----------|---------|
| `critical` | Unsafe same-entity merge across a labeled distinct / hierarchy boundary. Stop-ship in the core contract. |
| `high` | Strongly harmful incorrect merge with broad repair cost. |
| `medium` | Wrong merge with bounded local repair cost. |
| `low` | Wrong merge with narrow or reversible impact. |

Domain packages may add stricter severity rules, but they may not silently
weaken `critical`.

### Initial Gates

The initial cross-domain release gates are:

- `candidate_recall_at_50 >= 0.995`
- `auto_link_precision >= 0.995`
- `auto_link_recall >= 0.98`
- `hard_negative_false_merges.critical == 0`
- `accounted_case_rate == 1.0`

`exact_replay_coverage` is always reported, but it is informational. It cannot
turn a failing discovery gate into a passing one.

The hard-negative metric family and its release gate are intentionally split:
`metrics.hard_negative_false_merges` reports the total counted false merges for
the distinct/hierarchy denominator, while `critical_false_merges_max` reads the
critical severity bucket at `severity_counts.critical`. The critical gate does
not infer its observed value from the total metric count.

### Waivers

Threshold-lowering after a holdout reveal is not freeform. The contract allows
only explicit waivers with all of:

- `waiver_bead_id`
- `holdout_id`
- `metric_id`
- `gate_id`
- `old_threshold`
- `new_threshold`
- `reason`
- `approved_by`
- `approved_at`
- `scope`
- `replacement_holdout_id` or `expires_at`

Rules:

- Lowering a threshold requires a new versioned holdout or an explicit waiver bead.
- Exact replay rows cannot justify lowering a non-exact discovery threshold.
- A waiver must name the exact metric/gate/scope being changed.
- The core contract does not waive `hard_negative_false_merges.critical == 0`.

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

### `ER-REGISTRY-001`: Registry Mutation Safety

Registry-writing commands are more dangerous than scoring commands. Any command
that can write a registry, sidecar, patch ledger, or promotion proof must run
through this eval.

Expected:

- refusal paths do not mutate registry files, sidecars, ledgers, or indexes;
- promotion is atomic: a failed write leaves the previous snapshot byte-identical;
- replaying the same approved promotion is idempotent;
- pre-write and post-write registry tree hashes are recorded;
- stale registry snapshot, stale audit, stale review import, and failed audit
  paths refuse before any write;
- partial temp files are either cleaned or recorded as failed temp artifacts
  outside the registry path.

### `ER-EXPLAIN-001`: Explainability Completeness

Every merge, non-merge, review, refusal, and promotion should be reconstructable
without rerunning candidate generation.

Required sections:

```text
input_surface
profile_and_strategy
normalized_views
blocking_candidates
support_evidence
anti_merge_evidence
relation_hints
solver_decision
review_or_patch_decision
registry_snapshot
promotion_provenance
next_action
```

Expected:

- every auto-merge has at least one support evidence lane and no active
  cannot-link veto;
- every non-merge/review item has anti-merge evidence or an uncertainty reason;
- every exact replay points to registry id, version, rule id, and snapshot hash;
- explain artifacts are deterministic after canonical sorting.

### `ER-REVIEW-GOLDEN-001`: Human Review Golden Artifacts

Review quality needs stable human-readable and machine-readable goldens, not
only aggregate counts.

Expected artifacts:

```text
review.csv
review.jsonl
review.summary.md
review.expected_actions.json
```

Gates:

- CSV headers are stable and ordered;
- JSONL rows contain surface ids, representative raw labels, counts, positive
  evidence, anti-merge/relation evidence, proposed action, and reason codes;
- markdown summaries are generated from the same JSONL, not hand-maintained;
- importing `review.expected_actions.json` produces the expected ledger events;
- duplicate review groups are zero on small-CI suites.

### `ER-META-001`: Metamorphic Eval

Metamorphic checks catch regressions that are not covered by one fixed golden.
The initial relation set is:

| Relation | Expected invariant |
|----------|--------------------|
| `MR-ROW-SHUFFLE` | row order does not change prepared surfaces, candidates, edges, solve groups, or apply output after canonical sort |
| `MR-BATCH-SIZE` | physical chunk size does not change artifacts after canonical sort |
| `MR-DUPLICATE-ROWS` | duplicate raw rows change provenance/counts, not canonical surface identity or merge decisions |
| `MR-CACHE-STATE` | cold and warm cache runs produce identical semantic artifacts |
| `MR-HARMLESS-NOISE` | identity-preserving spelling/punctuation/case mutations keep candidate recall above target |
| `MR-PROFILE-FIREWALL` | same surface text under incompatible profiles refuses cross-profile reuse/import |
| `MR-APPLY-REPLAY-IDEMPOTENCE` | applying the same approved registry snapshot twice is byte-identical |

Every relation must name its source fixture, transformation seed or algorithm,
expected invariant, allowed differences, and strength score. Relations with a
strength score below `2.0` are too weak to count as release gates.

### `ER-HOLDOUT-001`: Corpus Holdout Protocol

CMBS public sample v1 and the current sec10d Reg AB baseline are not enough.
They are seed corpora. Future public holdouts must be versioned as immutable
benchmark series rather than silently replacing fixtures.

Expected:

- holdout ids use monotonic names such as `cmbs-public-v1`, `cmbs-public-v2`,
  `regab-baseline-v1`, and `regab-baseline-v2`;
- every holdout manifest records source hash, fixture selector, profile,
  benchmark ids, expected row/surface counts, and artifact hash policy;
- training/tuning corpora are separated from holdout corpora;
- lowering a threshold requires an explicit waiver field and Bead reference;
- adding a new holdout must not rewrite older expected results except for
  explicit schema migrations.

### `ER-RUNTIME-001`: No-Network / No-Model Runtime Eval

The entity workbench should remain native, local, reproducible Rust. The eval
harness must prove this at runtime, not only in prose.

Expected:

- no frontier model calls;
- no network access;
- no runtime model downloads;
- no Python or general ML framework runtime;
- no dense embedding service dependency for large corpora;
- model/data resources, if any, are pinned local artifacts with content hashes;
- eval logs include the runtime guard verdict.

### `ER-MEM-001`: Peak Memory Eval

Wall-clock numbers without memory numbers are incomplete for a 500k-row tool.
Peak RSS or platform-equivalent measurements must be captured for every
operator performance tier.

Initial targets:

| Workload | Peak memory target |
|----------|--------------------|
| Small CI fixture eval | `<= 256 MiB` |
| CMBS public sample | `<= 512 MiB` |
| sec10d full baseline | `<= 512 MiB` |
| CMBS 500k rows, normal repetition | `<= 2 GiB` |
| CMBS 500k unique names | bounded completion or deterministic refusal before `4 GiB` |

Memory results must include the measurement method. On macOS this can be
`rusage`/peak RSS or a documented wrapper; Linux runs may use `/usr/bin/time -v`
or an equivalent telemetry source.

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

## Evidence Ablation Program

The ablation program is diagnostic. It exists to prove which generic evidence
families move same-entity decisions, where false merges enter, and which stage
lost each labeled pair. It is not a license to bake corpus strings, domain
dictionaries, or benchmark thresholds into Canon defaults.

The planted fixture for this program is
`tests/fixtures/canon_v1/quality/ablation_cases.json`. It is intentionally
split into runtime inputs and hidden labels. Candidate generation, retrieval,
evidence extraction, scoring, clustering, and linking may read only the
runtime-input portion. Labels and expected ablation outcomes are evaluation-only
material.

This slice defines the design and fixture contract only. A future public
command must wire the compiled eval loop before any release claim says evidence
ablation is implemented end to end.

### Ablation Families

Every ablation run should report the baseline with one family disabled, the
family-only diagnostic run when meaningful, and the full-system union.

| Family ID | What It Tests | Shortcut Guardrail |
|-----------|---------------|--------------------|
| `exact_alias` | Exact alias or exact retained surface evidence. | Must not become fuzzy matching. |
| `normalized_name` | Generic normalization-preserving name equivalence. | No domain dictionary, suffix whitelist, or corpus-specific rewrite by default. |
| `char_token_similarity` | Character and token similarity features. | Similarity alone cannot override strong contradiction. |
| `sparse_retrieval` | TF-IDF or equivalent sparse candidate retrieval. | Retrieval is candidate admission, not a merge decision. |
| `trusted_anchors` | Trusted identifiers or reviewed external anchors. | Anchor namespace trust must be explicit and profile/configured. |
| `address_web_domain_anchors` | Address, URL, host, or web-domain anchors. | Domain co-ownership and shared locations are not identity by default. |
| `contextual_cooccurrence` | Source-local context and repeated co-occurrence. | Co-occurrence is support only when the profile exposes reusable context fields. |
| `source_priors` | Source reliability and source-specific prior evidence. | Priors cannot force identity without entity evidence. |
| `relationship_evidence` | Parent, subsidiary, servicer, agent, hierarchy, or other relationships. | Relationship evidence is non-equivalence unless separately supported. |
| `full_system_union` | The combined deterministic candidate and scoring system. | Must expose marginal contribution and cannot hide family failures. |

No family owns a default threshold in this document or in the planted fixture.
Thresholds, weights, and domain policies belong in explicit profile or strategy
configuration and must be reported in the run artifact hash inputs.

### Stage-Local Reason Codes

Each false negative, false positive, and avoidable abstention should record one
stage-local reason code. The `miss` form explains where a labeled true pair was
lost. The `admission` form explains where a labeled false pair entered or
survived too far.

| Stage | Miss Code | Admission Code |
|-------|-----------|----------------|
| `normalization` | `normalization.miss` | `normalization.admission` |
| `retrieval` | `retrieval.miss` | `retrieval.admission` |
| `evidence_extraction` | `evidence_extraction.miss` | `evidence_extraction.admission` |
| `scoring` | `scoring.miss` | `scoring.admission` |
| `constraint` | `constraint.miss` | `constraint.admission` |
| `cluster` | `cluster.miss` | `cluster.admission` |
| `link` | `link.miss` | `link.admission` |
| `policy` | `policy.miss` | `policy.admission` |

Reason codes are local to the first stage where the case became unrecoverable
or unsafe. A later stage may report secondary diagnostics, but the primary code
must stay deterministic for the same inputs.

### Planted Fixture Shape

Each ablation case should contain:

- `runtime_inputs`: observations, neutral pair ids, and evidence records the
  command is allowed to read.
- `hidden_labels`: planted same-entity and distinct labels, expected outcomes,
  and reason codes. This section must not be read by candidate generation or
  solving.
- one family under test, with planted true and false pairs for the roles
  `necessary`, `misleading`, `absent`, `duplicated`, and `contradictory`.

The roles mean:

| Role | Required Behavior |
|------|-------------------|
| `necessary` | Disabling the family causes a true-pair miss or lets a false pair through. |
| `misleading` | The family creates attractive evidence for a false pair that later evidence or policy must reject. |
| `absent` | The family contributes no evidence; the engine must not fabricate support. |
| `duplicated` | Duplicate family evidence must be deduped and must not inflate confidence. |
| `contradictory` | Support and contradiction must both survive into explanation and scoring. |

Relationship evidence has an extra rule: a relationship edge may support review,
explanation, blocking, or a cannot-link decision, but it is not same-entity
evidence unless an independent non-relationship family also supports the merge.
Parent-child, agent-principal, successor, manager, and hierarchy relations are
therefore false-merge risks, not aliases.

### Future Public Command Contract

The eventual public loop should produce one machine artifact per suite with:

- per-family disabled, family-only, and full-system results;
- candidate recall, auto-link precision/recall, review volume, calibration,
  wall-clock, and memory deltas by family;
- all false negatives, false positives, and avoidable abstentions grouped by the
  stage-local reason codes above;
- unchanged hidden labels across reruns, with sealed holdouts excluded until the
  declared gate;
- artifact hashes for inputs, profile/configuration, registry snapshots,
  ablation family set, and output.

The command must fail closed if it cannot prove that candidate generation and
solving did not read hidden labels.

---

## Wall-Clock Targets

Wall-clock targets are release/operator targets, not normal-CI gates until
measured baselines exist. Each timing result must include hardware, OS, Rust
profile, target triple, canon git sha, cache state, corpus size, unique surface
count, candidate count, and artifact sizes.

Initial targets:

| ID | Workload | Target |
|----|----------|--------|
| `PERF-SMALL-CI` | Small CI fixture eval | `< 1s` |
| `PERF-REGAB-COMMITTED` | Committed sec10d 46-surface fixture | `< 1s` |
| `PERF-CMBS-PUBLIC` | CMBS public sample, 10,143 tenant observations / 431 raw names | `< 2s` end-to-end after implementation is complete |
| `PERF-REGAB-FULL` | sec10d full baseline, 127,991 mentions / 46 surfaces | `< 10s` end-to-end |
| `PERF-REGAB-APPLY` | sec10d exact replay/apply over full baseline | `< 5s` |
| `PERF-REGAB-PREPARE` | sec10d prepare/dedupe full baseline | `< 5s` |
| `PERF-CMBS-500K-WARM` | CMBS 500k rows, normal repetition, warm cache | `< 2min` end-to-end |
| `PERF-CMBS-500K-COLD` | CMBS 500k rows, normal repetition, cold cache | `< 5min` end-to-end |
| `PERF-CMBS-500K-APPLY` | CMBS exact apply/replay over 500k rows | `< 15s` |
| `PERF-CMBS-500K-UNIQUE` | CMBS 500k unique names | bounded completion or deterministic refusal before memory/candidate explosion |

Targets may be tightened or relaxed only by updating
`entity_eval_performance_targets.json`, recording baseline artifacts, and
explaining the change in the relevant Bead.

## Peak Memory Targets

Peak-memory targets are defined by `ER-MEM-001` and enforced by the same
machine-readable contract as wall-clock targets. They are operator/release
targets until telemetry-backed baselines exist; normal CI should only assert
that the contract and small fixtures can report memory fields.

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
holdout_id
metamorphic_relation_id
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
peak_memory_method
registry_pre_mutation_hash
registry_post_mutation_hash
runtime_guard_status
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
    "summary": "summary.md",
    "explain": "artifacts/explain.jsonl",
    "review": "artifacts/review.jsonl"
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
