# Namekit Implementation Decisions

Status: ENT-P02.21 decision record, created 2026-06-25.

This document locks the implementation choices that downstream namekit,
index/block, edge, and benchmark beads must consume. It refines
`docs/namekit/SOURCE_PORT_MAP.md` and the implementation-quality defaults in
`docs/PLAN_ENTITY_WORKBENCH.md`; it does not add a runtime dependency by
itself.

## Decisions

### Metric Strategy

ENT-P02.12 should audit pinned RapidFuzz Rust first for local string metrics.
The accepted path is a pinned native Rust dependency only if the audit confirms
license compatibility, no runtime Python/ICU/Java/native extension, no unsafe
policy conflict, deterministic byte and Unicode behavior, cutoff/hint parity,
and bounded dependency surface.

If the RapidFuzz audit fails any of those gates, namekit implements the required
metrics internally instead of falling back to a weaker fuzzy-only matcher. The
fallback set is Levenshtein/edit distance, Jaro-Winkler-style prefix-sensitive
similarity if needed for parity, Dice/Sorensen over tokens or n-grams, and token
set/sort support evidence. Metrics are evidence only; they cannot override
protected-token conflicts, cannot-link lanes, stale artifact checks, or profile
firewalls.

### Sparse Data Model

Dense vectors are forbidden for large corpora. ENT-P02.11 and ENT-P04.5 must
use sparse TF-IDF data structures:

- corpus-local integer dictionaries for word tokens and char n-grams;
- deterministic ID assignment by normalized byte key, then source kind;
- sorted posting lists keyed by term ID with surface IDs in ascending order;
- CSR-like `term_offsets` plus posting arrays where compact reload matters;
- per-surface sparse rows storing `(term_id, tf_units, idf_units, weight_units)`;
- top-k candidate retrieval from posting-list accumulators, not dense
  all-pairs multiplication.

The data model may borrow sparse_dot_topn's bounded top-N semantics, but canon
does not bind to its C++/Python implementation.

### TF/IDF Formula Family

Namekit uses sparse, integer-weighted TF/IDF support evidence rather than
floating artifact scores. The default formula family is:

- `tf_units`: capped integer term frequency, with raw counts above the cap
  treated as the cap for evidence so repeated boilerplate cannot dominate;
- `idf_units`: deterministic document-frequency weighting where corpus-common
  values receive lower units and rare-but-not-singleton values receive higher
  units;
- `weight_units = tf_units * idf_units`, computed with checked integer math;
- common-token downweight and rare-token support are emitted as explainable
  reason codes, not hidden model behavior.

ENT-P02.11 must pin the exact cap, IDF integer formula, overflow bounds, and
golden values in tests. It must not emit floats in artifacts or rely on
platform-dependent floating comparison for top-k ordering.

### Score Units

Every score crossing a module or artifact boundary uses deterministic integer
score units. The shared scale is `NAMEKIT_SCORE_SCALE = 10_000`, where `0`
means no support and `10_000` means exact support for that evidence component.

Internal calculations may use wider integers or local numeric helpers, but
artifact fields, thresholds, review rows, top-k traces, edge evidence, and solve
inputs must carry integer score units. Tie ordering is always:

1. higher integer score;
2. stronger evidence class before weaker diagnostic evidence;
3. normalized key bytes;
4. canonical surface ID or row-stable surface ordinal.

### Sorted-Neighborhood

Sorted-neighborhood is supplemental recall machinery, not authoritative merge
evidence. It may be implemented only with:

- explicit key material;
- deterministic window size and cap behavior;
- diagnostics naming key, window, emitted pair count, and capped pair count;
- cannot-link/protected-token preservation;
- top-k and budget enforcement after dedupe.

If those diagnostics are not present, sorted-neighborhood remains deferred.

## Source-Parity Fixture List

Downstream beads must either implement or explicitly defer these fixture
families:

- `tests/fixtures/namekit/normalization/unicode_normality.jsonl`
- `tests/fixtures/namekit/legal_suffix/examples.jsonl`
- `tests/fixtures/namekit/legal_suffix/provenance.jsonl`
- `tests/fixtures/namekit/source_parity/normality_unicode.jsonl`
- `tests/fixtures/namekit/source_parity/openrefine_fingerprint.jsonl`
- `tests/fixtures/namekit/source_parity/cleanco_suffixes.jsonl`
- `tests/fixtures/namekit/source_parity/legal_form_jurisdictions.jsonl`
- `tests/fixtures/namekit/source_parity/emm_indexers.jsonl`
- `tests/fixtures/namekit/source_parity/sorted_neighborhood.jsonl`
- `tests/fixtures/namekit/source_parity/sparse_topn.jsonl`
- `tests/fixtures/namekit/source_parity/sparse_topn_chunk_zip.jsonl`
- `tests/fixtures/namekit/source_parity/splink_tf_adjustments.jsonl`
- `tests/fixtures/namekit/source_parity/rapidfuzz_metrics.jsonl`
- `tests/fixtures/namekit/source_parity/logic_v2_features.jsonl`

Fixture tests must assert normalized views, protected tokens, reason-code order,
score units, tie order, caps, and negative/non-equivalence cases. Smoke tests
that only prove code runs are not sufficient.

## Downstream Contract

ENT-P02.11 implements sparse TF-IDF, bounded top-k, and any accepted
sorted-neighborhood helper against this record. ENT-P02.12 audits and adopts or
rejects RapidFuzz Rust. ENT-P02.17 defines compact symbol tables and integer ID
layout. ENT-P04.5 aligns index reload layout with the same sparse model.
ENT-P06.6 and `bd-39z.6` use the same integer score units and tie ordering for
edge scoring.
