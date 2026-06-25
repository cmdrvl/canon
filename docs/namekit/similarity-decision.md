# Namekit Similarity Dependency Decision

Status: ENT-P02.12 decision record, created 2026-06-25.

## Decision

Namekit adopts `rapidfuzz = "=0.5.0"` behind the canon-owned
`src/namekit/similarity.rs` adapter.

The dependency audit passed the ENT-P02.12 gates:

- license: MIT, compatible with the repository license posture;
- dependency surface: no transitive runtime dependencies in the crate manifest;
- runtime posture: pure Rust library, no Python, ICU, Java, model download, or
  native extension runtime;
- unsafe posture: crate root declares `#![forbid(unsafe_code)]`, matching canon;
- deterministic API fit: callers provide explicit byte or char iterators;
- ASCII path: canon uses byte iterators only when both normalized inputs are
  ASCII;
- Unicode path: canon uses char iterators when either input is non-ASCII;
- cutoff/hint semantics: `score_cutoff` may suppress a below-cutoff result, and
  `score_hint` is tested to preserve the answer;
- batch reuse: batch comparators are allowed only when their output matches the
  pairwise adapter for the same metric/options.

## Canon Boundary

RapidFuzz may use floating-point math internally, but canon does not expose
floating scores across module or artifact boundaries. The adapter converts every
accepted metric output to integer score units on the shared
`NAMEKIT_SCORE_SCALE = 10_000` scale. Rounding is deterministic:

```text
units = floor(clamp(score, 0.0, 1.0) * 10_000 + 0.5)
```

`NaN` is treated as `0`. Values below a configured `score_cutoff` are emitted as
`score = null` with `passed_cutoff = false`.

## Guardrails

Similarity metrics are support evidence only. They cannot bypass cannot-link
evidence, protected-token conflicts, stale artifact checks, profile firewalls,
budget refusals, or human review requirements.

Fixture coverage lives in
`tests/fixtures/namekit/source_parity/rapidfuzz_metrics.jsonl` and is exercised
by `tests/namekit_similarity.rs`.
