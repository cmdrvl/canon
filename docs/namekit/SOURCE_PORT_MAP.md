# Namekit Source Port Map

Status: ENT-P02.7 research gate, created 2026-06-25.

Purpose: namekit must be a deterministic Rust layer for entity-name
normalization, blocking features, similarity evidence, and review/explain
payloads. This note records which upstream semantics are accepted, rejected, or
deferred before any implementation starts. It is a design contract, not a
runtime dependency list.

Hard constraints:

- Do not add runtime Python, ICU, Java, Spark, scikit-learn, model downloads,
  network calls, or frontier model calls.
- Do not copy upstream implementation code. Port observable semantics into
  small Rust modules with canon-owned fixtures.
- Core `canon` lookup remains exact registry lookup after ASCII trim. Namekit
  only feeds entity workbench surfaces.
- Every lossy transformation must emit stable reason codes and source
  provenance sufficient for `canon entity explain`.

## Target Rust Surface

These module names are the expected landing zones for downstream ENT-P02 beads:

| Target module | Role | Downstream artifacts |
|---|---|---|
| `src/namekit/mod.rs` | Public API boundary, profile-aware views, stable structs | `canon_entity_prepare.v0` |
| `src/namekit/normalize.rs` | Unicode, case, punctuation, control, whitespace semantics | `surfaces[].namekit.normalization_steps[]` |
| `src/namekit/legal.rs` | Legal suffix/form lexicon, profile preserve/drop rules | `surfaces[].namekit.legal_forms[]` |
| `src/namekit/fingerprint.rs` | Token-sort/dedupe and ngram fingerprint views | `surfaces[].namekit.fingerprints[]`, `canon_entity_block.v0` |
| `src/namekit/tokens.rs` | Token spans, protected tokens, surface ranges | `surfaces[].namekit.tokens[]` |
| `src/namekit/tfidf.rs` | Deterministic sparse tf-idf/idf and common-token weighting | `canon_entity_block.v0`, `canon_entity_edge.v0` |
| `src/namekit/topk.rs` | Bounded top-k sparse candidate selection | `candidate_budget`, `topk_trace[]` |
| `src/namekit/metric.rs` | Audited native string metrics and cutoffs | `edge.evidence[].metric` |
| `src/namekit/review.rs` | Same/not-same/undecided patch mapping | review CSV/JSON, decision ledger |
| `src/namekit/explain.rs` | Reason-code ordering and explain reconstruction | `canon_entity_explain.v0` |

Reserved reason code families for ENT-P02.16 to finalize:

- `NK_UNICODE_FOLDED`
- `NK_CASE_FOLDED`
- `NK_DIACRITIC_DROPPED`
- `NK_PUNCT_DROPPED`
- `NK_CONTROL_DROPPED`
- `NK_WHITESPACE_COLLAPSED`
- `NK_LEGAL_FORM_STRIPPED`
- `NK_LEGAL_FORM_PRESERVED`
- `NK_LEGAL_FORM_JURISDICTION_HINT`
- `NK_TOKEN_SORTED`
- `NK_TOKEN_DEDUPED`
- `NK_NGRAM_FINGERPRINTED`
- `NK_COMMON_TOKEN_DOWNWEIGHT`
- `NK_RARE_TOKEN_SUPPORT`
- `NK_TOPK_THRESHOLD_APPLIED`
- `NK_METRIC_CUTOFF_APPLIED`
- `NK_METRIC_HINT_USED`
- `NK_PROTECTED_TOKEN_CONFLICT`
- `NK_SUPPORT_FEATURE`
- `NK_CONTRADICTION_FEATURE`
- `NK_REVIEW_SAME_REUSED`
- `NK_REVIEW_NOT_SAME_REUSED`
- `NK_REVIEW_UNDECIDED_REUSED`

Reason-code ordering must be deterministic:

1. Input-position order for transformations on the same source span.
2. Lexicographic reason-code order for independent transformations at the same
   source span.
3. Support evidence before contradiction evidence.
4. Stable source id as the final tie-breaker.

## Source Inventory

Pinned versions were checked from upstream tags or package metadata on
2026-06-25. Commit hashes are tag targets from `git ls-remote`; annotated tags
are recorded by their peeled commit.

| Source | Pin | License | Decision | Source |
|---|---:|---|---|---|
| OpenSanctions `rigour` / `rigour.names` | `v2.1.2` @ `315fa3331b287ab459acb59e4c71784138daacb9` | MIT | Port selected data-model semantics | <https://github.com/opensanctions/rigour>, <https://rigour.followthemoney.tech/names/> |
| `normality` | `3.1.0` @ `29333c9b1de4bc40489edeaa124112b0cfaaa316` | MIT | Port selected observable text semantics; reject `pyicu` dependency | <https://github.com/pudo/normality> |
| `fingerprints` | `1.3.1` @ `4e0126d8380a21b59736700b3c2b87e6f7a7ece2` | MIT | Defer to rigour successor, port legacy fixture expectations only | <https://github.com/opensanctions/fingerprints> |
| `cleanco` | `2.3` @ `6567188f77e979edc2ea627b04d339144c9c02ae` | MIT | Port legal-suffix concepts, not whole term table blindly | <https://github.com/psolin/cleanco> |
| GLEIF ISO 20275 ELF list | February 2026 current list | GLEIF data terms, not bundled yet | Use as authoritative legal-form provenance input | <https://www.gleif.org/en/lei-data/code-lists/iso-20275-entity-legal-forms-code-list> |
| ING `EntityMatchingModel` | `v2.1.11` @ `5b4f9158b049be233fc0ec8b27c88be3fb8ee75b` | MIT plus third-party NOTICE | Port candidate-selection ideas; reject supervised/Spark runtime | <https://github.com/ing-bank/EntityMatchingModel> |
| ING `sparse_dot_topn` | `v1.2.0` @ `c61e8cc0ff9e459a645860ebb795a2ef4a576983` | Apache-2.0 | Port bounded top-k semantics in pure Rust | <https://github.com/ing-bank/sparse_dot_topn> |
| Splink TF adjustments | `v4.0.16` @ `775a50f6061050b1628725bf717bdf53ef9fa72a` | MIT | Port common/rare weighting diagnostic, not probabilistic EM | <https://github.com/moj-analytical-services/splink>, <https://moj-analytical-services.github.io/splink/topic_guides/comparisons/term-frequency.html> |
| RapidFuzz Rust | `v0.5.0` @ `8b55d54919d5458c368fe53f18e1ae8f4f7cee1a` | MIT OR Apache-2.0 | Candidate dependency or parity target for native metrics | <https://github.com/rapidfuzz/rapidfuzz-rs> |
| OpenSanctions matcher logic-v2 | Public matcher docs 2026-06-25 | API/docs terms; implementation not vendored | Port support/contradiction evidence shape | <https://www.opensanctions.org/matcher/> |
| `nomenklatura` Resolver | `4.10.0` @ `8b4756cbac227e0ce17ef21c14e5a74274909f53` | MIT | Port judgement graph semantics into patches/review | <https://github.com/opensanctions/nomenklatura> |
| Dedupe | `v3.0.3` @ `c738749049f4c0e79339afbd6eee4c5ac09eac5c` | MIT | Defer ML training; port active-review workflow ideas only | <https://github.com/dedupeio/dedupe>, <https://docs.dedupe.io/> |
| OpenRefine clustering | `3.9.5` @ `5c89cd82eb91e693954a194050bde7dfb647db8f` | BSD-style | Port fingerprint/ngram fixture semantics; reject Java runtime | <https://github.com/OpenRefine/OpenRefine>, <https://openrefine.org/docs/technical-reference/clustering-in-depth> |

## Port Decisions

### OpenSanctions rigour and rigour.names

Decision: port selected semantics.

Accepted:

- Typed name model: original form, normalized form, name type tag, parts,
  spans, symbols.
- `NameTypeTag` profile distinction: organization/person/object/entity/unknown
  informs profile-specific handling, but canon public contracts use generic
  entity naming.
- `NamePartTag` style part labels: legal, suffix, stop, numeric, title,
  family/given only where person profiles later require them.
- Alignment evidence shape: matched symbol, residue cluster, or unmatched extra
  becomes `namekit.alignment[]` evidence in edge/explain artifacts.
- Organization type replacement/removal is profile-controlled and never global.

Rejected/deferred:

- Python object model and mutable score/weight policy passes are not ported.
- Cultural/cross-script symbol databases are deferred until provenance and
  fixture ownership are explicit.
- Person-name specific honorific/family/given rules are deferred for CMBS and
  Reg AB because the first profiles are organization/tenant focused.

Target modules:

- `src/namekit/tokens.rs`
- `src/namekit/legal.rs`
- `src/namekit/metric.rs`
- `src/namekit/explain.rs`

Expected reason codes:

- `NK_LEGAL_FORM_STRIPPED`
- `NK_LEGAL_FORM_PRESERVED`
- `NK_SUPPORT_FEATURE`
- `NK_CONTRADICTION_FEATURE`

Fixture families:

- `tests/fixtures/namekit/source_parity/rigour_names_org_parts.jsonl`
- `tests/fixtures/namekit/source_parity/rigour_alignment_evidence.jsonl`

Downstream fields:

- `surfaces[].namekit.parts[]`
- `edges[].evidence[].alignment`
- `explain.namekit_trace[]`

### normality

Decision: port observable text-normalization semantics, reject runtime
dependency.

Accepted:

- UTF-8/unicode input handling.
- Case folding.
- Diacritic removal where deterministic under Rust's pinned unicode crate.
- Punctuation/control removal.
- Whitespace collapse.
- ASCII-friendly slug/fingerprint preparation when explicitly requested by a
  profile.

Rejected/deferred:

- The `pyicu` dependency introduced in normality 3.x is rejected. Canon must not
  depend on ICU dynamically or through Python.
- Locale-sensitive collation is rejected for v0 namekit. Any locale-sensitive
  behavior must be an explicit future profile contract.

Target modules:

- `src/namekit/normalize.rs`
- `src/namekit/explain.rs`

Expected reason codes:

- `NK_UNICODE_FOLDED`
- `NK_CASE_FOLDED`
- `NK_DIACRITIC_DROPPED`
- `NK_PUNCT_DROPPED`
- `NK_CONTROL_DROPPED`
- `NK_WHITESPACE_COLLAPSED`

Fixture families:

- `tests/fixtures/namekit/source_parity/normality_unicode.jsonl`
- `tests/fixtures/namekit/source_parity/openrefine_fingerprint.jsonl`

Downstream fields:

- `surfaces[].namekit.normalized`
- `surfaces[].namekit.normalization_steps[]`

### fingerprints

Decision: defer implementation source to rigour successor; preserve legacy
expectations as parity cases.

Accepted:

- Entity fingerprint definition as a simplified identifier for cross-dataset
  matching.
- Token-order normalization and duplicate-token removal for key-collision style
  blocking.
- Legacy examples such as person-name reorder and organization legal-form
  simplification become regression fixtures.

Rejected/deferred:

- The standalone library is unmaintained as of the upstream 2025 note, so no
  code or table is imported from it.
- Address/entity fingerprinting beyond names is deferred to profile-specific
  source maps.

Target modules:

- `src/namekit/fingerprint.rs`
- `src/namekit/tokens.rs`

Expected reason codes:

- `NK_TOKEN_SORTED`
- `NK_TOKEN_DEDUPED`

Fixture families:

- `tests/fixtures/namekit/source_parity/fingerprints_legacy.jsonl`

Downstream fields:

- `surfaces[].namekit.fingerprints[]`
- `block_assertions[].key`

### cleanco

Decision: port legal-suffix concepts and selected fixtures.

Accepted:

- Legal suffix stripping as an explainable, profile-controlled view.
- Repeated suffix stripping caveat: profiles may run bounded repeated stripping
  only when every stripped suffix is recorded.
- Business type and possible jurisdiction hints feed evidence, not automatic
  merge authority.
- Custom term lists are profile artifacts, not hardcoded global behavior.

Rejected/deferred:

- Do not blindly embed the full cleanco term table. Legal suffix data must be
  pinned by provenance, license, and profile.
- Do not treat inferred country/type as identity. Use it as weak evidence or
  anti-merge context only.

Target modules:

- `src/namekit/legal.rs`
- `src/namekit/explain.rs`

Expected reason codes:

- `NK_LEGAL_FORM_STRIPPED`
- `NK_LEGAL_FORM_PRESERVED`
- `NK_LEGAL_FORM_JURISDICTION_HINT`

Fixture families:

- `tests/fixtures/namekit/source_parity/cleanco_suffixes.jsonl`
- `tests/fixtures/namekit/source_parity/legal_form_jurisdictions.jsonl`

Downstream fields:

- `surfaces[].namekit.legal_forms[]`
- `edges[].evidence[].legal_form`

### GLEIF ISO 20275 ELF list

Decision: use as primary provenance for legal-form data, but do not bundle until
ENT-P02.8 records license/data handling.

Accepted:

- ISO 20275/GLEIF legal entity form code, jurisdiction, native-language form,
  and abbreviated labels are the preferred provenance for future legal-form
  tables.
- Reserved-code semantics must remain visible when used in fixtures.

Rejected/deferred:

- No automatic download at runtime.
- No unreviewed bulk table import in ENT-P02.7. ENT-P02.8 must pin the exact
  downloaded file, checksum, license/data terms, and generated Rust table
  policy.

Target modules:

- `src/namekit/legal.rs`

Expected reason codes:

- `NK_LEGAL_FORM_JURISDICTION_HINT`
- `NK_LEGAL_FORM_PRESERVED`

Fixture families:

- `tests/fixtures/namekit/source_parity/iso20275_legal_forms.jsonl`

Downstream fields:

- `surfaces[].namekit.legal_forms[].source`

### ING EntityMatchingModel

Decision: port candidate-selection architecture, reject runtime stack.

Accepted:

- Complementary indexers: word tf-idf cosine, character tf-idf cosine, and
  sorted-neighborhood blocking.
- Candidate selection is separate from scoring and solving.
- Legal-entity features are evidence components, not automatic merges.
- Rank/runner-up features are useful for review prioritization.

Rejected/deferred:

- Pandas, Spark, sklearn, supervised classifier training, and probability
  calibration are rejected for v0 namekit.
- Group-to-ground-truth supervised matching is deferred; canon solver already
  owns cluster decisions.

Target modules:

- `src/namekit/tfidf.rs`
- `src/namekit/topk.rs`
- `src/namekit/legal.rs`

Expected reason codes:

- `NK_COMMON_TOKEN_DOWNWEIGHT`
- `NK_RARE_TOKEN_SUPPORT`
- `NK_TOPK_THRESHOLD_APPLIED`
- `NK_LEGAL_FORM_JURISDICTION_HINT`

Fixture families:

- `tests/fixtures/namekit/source_parity/emm_indexers.jsonl`
- `tests/fixtures/namekit/source_parity/sorted_neighborhood.jsonl`

Downstream fields:

- `block_artifact.indexer`
- `block_artifact.candidate_sources[]`
- `edge.evidence[].rank`

### sparse_dot_topn

Decision: port semantics in pure Rust; do not bind to C++ extension.

Accepted:

- Sparse matrix multiplication followed by per-row top-N selection.
- `threshold` lower-bound semantics.
- Optional sorted top-N output.
- Density/budget configuration as memory preallocation guidance, translated to
  canon budget refusals.
- Chunk/zipped matrix composition as an implementation pattern for very large
  runs.
- Max-heap collection semantics because it bounds memory by `top_n`.

Rejected/deferred:

- C++ extension, OpenMP, NumPy/SciPy, and Python wheels are rejected.
- Distributed cluster execution is deferred. Canon should chunk deterministically
  in-process first.

Target modules:

- `src/namekit/tfidf.rs`
- `src/namekit/topk.rs`
- `src/entity/block_artifact.rs`
- `src/entity/budget.rs`

Expected reason codes:

- `NK_TOPK_THRESHOLD_APPLIED`
- `NK_COMMON_TOKEN_DOWNWEIGHT`
- `NK_RARE_TOKEN_SUPPORT`

Fixture families:

- `tests/fixtures/namekit/source_parity/sparse_topn.jsonl`
- `tests/fixtures/namekit/source_parity/sparse_topn_chunk_zip.jsonl`

Downstream fields:

- `candidate_budget.observed_pairs_per_surface`
- `block_artifact.topk_trace[]`
- `edge.evidence[].tfidf`

### Splink TF adjustments

Decision: port diagnostic weighting concept, reject probabilistic linkage model.

Accepted:

- Common values should contribute less evidence than rare values.
- Rare-value support must be capped so spelling errors or singleton noise do not
  dominate.
- TF adjustments are explicit evidence independent of downstream merge
  thresholds.

Rejected/deferred:

- Fellegi-Sunter EM, SQL backend generation, comparison-level DSL, and model
  training are rejected for v0 namekit.
- Splink's probabilistic m/u vocabulary should not leak into canon public
  artifacts unless a future strategy profile explicitly asks for it.

Target modules:

- `src/namekit/tfidf.rs`
- `src/namekit/explain.rs`

Expected reason codes:

- `NK_COMMON_TOKEN_DOWNWEIGHT`
- `NK_RARE_TOKEN_SUPPORT`

Fixture families:

- `tests/fixtures/namekit/source_parity/splink_tf_adjustments.jsonl`

Downstream fields:

- `block_artifact.idf_stats[]`
- `edge.evidence[].token_weight`

### RapidFuzz Rust

Decision: acceptable audited dependency candidate and parity target.

Accepted:

- Native Rust metrics with no runtime Python.
- Byte fast path for ASCII-only strings.
- `score_cutoff` and `score_hint` style arguments as explicit cutoff/hint
  evidence.
- Batch comparator idea for one-to-many repeated comparisons.

Rejected/deferred:

- Do not add a dependency until ENT-P02.12 audits crate maintenance, API
  coverage, and benchmark parity.
- Metrics are evidence only. A high fuzzy score cannot override profile
  cannot-link rules or protected-token conflicts.

Target modules:

- `src/namekit/metric.rs`
- `src/namekit/explain.rs`

Expected reason codes:

- `NK_METRIC_CUTOFF_APPLIED`
- `NK_METRIC_HINT_USED`
- `NK_SUPPORT_FEATURE`

Fixture families:

- `tests/fixtures/namekit/source_parity/rapidfuzz_metrics.jsonl`

Downstream fields:

- `edge.evidence[].metric`
- `explain.namekit_trace[].metric`

### OpenSanctions matcher logic-v2

Decision: port evidence shape, reject sanctions-specific weights.

Accepted:

- Supporting and contradicting features are separate evidence classes.
- Identifier, legal-form, protected-token, and profile features can reduce a
  name score or force review.
- Cross-script/cultural matching is useful only when backed by pinned data and
  profile consent.
- Explanations must name the feature that moved a score.

Rejected/deferred:

- OpenSanctions API scoring weights are not canon defaults.
- Sanctions-screening-specific features are out of scope for CMBS tenant and Reg
  AB firm identity profiles.
- Cross-script/cultural data is deferred until data provenance is pinned.

Target modules:

- `src/namekit/metric.rs`
- `src/namekit/review.rs`
- `src/namekit/explain.rs`

Expected reason codes:

- `NK_SUPPORT_FEATURE`
- `NK_CONTRADICTION_FEATURE`
- `NK_PROTECTED_TOKEN_CONFLICT`

Fixture families:

- `tests/fixtures/namekit/source_parity/logic_v2_features.jsonl`

Downstream fields:

- `edge.evidence[].kind`
- `review.reason_codes[]`
- `solve.cannot_link_reasons[]`

### nomenklatura Resolver

Decision: port judgement graph semantics into review/patch workflow.

Accepted:

- Same, not-same, and undecided judgements.
- Transitive review reuse: if A is not B and B is same C, then A/C inherits a
  cannot-link review fact.
- Resolver-style canonical ID selection informs canon review patches, but canon
  promotion remains registry-controlled.
- Persistent decision ledger semantics map to existing entity decision artifacts.

Rejected/deferred:

- SQLite resolver database and TUI are not namekit runtime dependencies.
- FollowTheMoney entity model is not imported into canon.

Target modules:

- `src/namekit/review.rs`
- `src/entity/runtime/review.rs` or existing review runtime module
- `src/entity/runtime/solve.rs` or existing solve runtime module

Expected reason codes:

- `NK_REVIEW_SAME_REUSED`
- `NK_REVIEW_NOT_SAME_REUSED`
- `NK_REVIEW_UNDECIDED_REUSED`
- `NK_PROTECTED_TOKEN_CONFLICT`

Fixture families:

- `tests/fixtures/namekit/source_parity/resolver_judgements.jsonl`

Downstream fields:

- `decision_ledger.events[].judgement`
- `review.rows[].prior_decision`
- `solve.cannot_link_reasons[]`

### Dedupe

Decision: defer ML model; port active-review workflow ideas.

Accepted:

- Active review loop as an operator workflow: uncertain pairs get labels of
  duplicate/not duplicate/unsure.
- Custom blocking/comparator idea maps to strategy profile declarations.
- Persist labelled examples as explicit patches, not opaque model state.

Rejected/deferred:

- Machine-learning classifier training is rejected for v0 namekit.
- Runtime Python and sklearn-style model dependencies are rejected.
- Learned weights are deferred until a future audited strategy registry can
  prove deterministic replay.

Target modules:

- `src/namekit/review.rs`
- `src/entity/runtime/review.rs` or existing review runtime module

Expected reason codes:

- `NK_REVIEW_SAME_REUSED`
- `NK_REVIEW_NOT_SAME_REUSED`
- `NK_REVIEW_UNDECIDED_REUSED`

Fixture families:

- `tests/fixtures/namekit/source_parity/dedupe_active_review.jsonl`

Downstream fields:

- `review.rows[].suggested_label`
- `decision_ledger.events[].operator_label`

### OpenRefine

Decision: port fingerprint/ngram fixture semantics and strict-to-lax workflow.

Accepted:

- Key collision fingerprint order: trim, lowercase, remove punctuation/control,
  ASCII fold, split tokens, sort, dedupe, join.
- N-gram fingerprinting as a blocking view.
- Strict-to-lax clustering progression is a review workflow pattern.
- Nearest-neighbor methods require blocking first and explicit radius/cutoff.

Rejected/deferred:

- Java runtime and UI are rejected.
- OpenRefine cluster merge UI is not canon's review contract.
- Phonetic fingerprinting is deferred until locale/provenance is pinned.

Target modules:

- `src/namekit/fingerprint.rs`
- `src/namekit/tokens.rs`
- `src/namekit/review.rs`

Expected reason codes:

- `NK_TOKEN_SORTED`
- `NK_TOKEN_DEDUPED`
- `NK_NGRAM_FINGERPRINTED`
- `NK_METRIC_CUTOFF_APPLIED`

Fixture families:

- `tests/fixtures/namekit/source_parity/openrefine_fingerprint.jsonl`
- `tests/fixtures/namekit/source_parity/openrefine_ngram.jsonl`

Downstream fields:

- `block_artifact.block_keys[]`
- `review.grouping_method`

## Fixture Contract For ENT-P02.10

ENT-P02.10 must materialize these fixture files. Each JSONL row should include:

- `fixture_id`
- `source`
- `profile`
- `input`
- `expected_views`
- `expected_reason_codes`
- `accepted_loss`
- `protected_tokens`
- `notes`

Required fixture files:

- `normality_unicode.jsonl`
- `openrefine_fingerprint.jsonl`
- `openrefine_ngram.jsonl`
- `fingerprints_legacy.jsonl`
- `rigour_names_org_parts.jsonl`
- `rigour_alignment_evidence.jsonl`
- `cleanco_suffixes.jsonl`
- `legal_form_jurisdictions.jsonl`
- `iso20275_legal_forms.jsonl`
- `emm_indexers.jsonl`
- `sorted_neighborhood.jsonl`
- `sparse_topn.jsonl`
- `sparse_topn_chunk_zip.jsonl`
- `splink_tf_adjustments.jsonl`
- `rapidfuzz_metrics.jsonl`
- `logic_v2_features.jsonl`
- `resolver_judgements.jsonl`
- `dedupe_active_review.jsonl`

Required implementation tests:

- `cargo test namekit_source_parity -- --nocapture`
- `cargo test namekit_unicode_normality -- --nocapture`
- `cargo test namekit_legal_forms -- --nocapture`
- `cargo test namekit_fingerprints -- --nocapture`
- `cargo test namekit_tfidf_topk -- --nocapture`
- `cargo test namekit_metric_parity -- --nocapture`
- `cargo test namekit_review_judgements -- --nocapture`
- `cargo test entity_explain_namekit_trace -- --nocapture`

## Data And License Notes

- MIT, Apache-2.0, and BSD-style source licenses are compatible as references,
  but implementation must be original Rust unless a future bead explicitly adds
  a crate dependency.
- GLEIF legal-form data is data, not source code. ENT-P02.8 must pin exact file,
  checksum, redistribution terms, and generated-table policy before any table is
  bundled.
- OpenSanctions public data pages note non-commercial data licensing for their
  hosted datasets. This note references software/docs only; do not import
  OpenSanctions dataset contents into fixtures without a separate data-license
  review.
- Any optional dependency must be a Cargo dependency with an explicit version
  and license review. No Python subprocess, dynamic model download, or network
  lookup may be introduced.

## Downstream Handoff Checklist

Each downstream namekit bead must state:

- Which source row in this map it ports or rejects.
- Which fixture family it consumes.
- Which reason codes it emits.
- Which artifact fields it writes.
- Which profile gates prevent overmerge.
- Which behavior is deliberately not implemented.
