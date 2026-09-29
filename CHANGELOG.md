# Changelog

## 0.14.0 — 2026-09-28

- Cluster strategies can declare audit-gated policy acceptance for new identities.
  Qualification requires connected policy-supported evidence, preserves hard
  cannot-links and human overrides, records decision authority, and refuses run
  ceiling overruns. Human review remains the default.
- Policy audits execute the declared strategy against pinned inputs and complete
  identity labels, measuring pair and component precision. Promotion verifies
  the audit, strategy, manifest, and decision ledger before writing exact aliases.
- Attribute-conflict comparators support declared unit ambiguity, unknown zero
  rates, and sentinel dates. Ambiguous comparisons remain visible as soft
  negative evidence; they never create identity support. Genuine mismatches
  remain hard conflicts.
- The instrument profile excludes CUSIP `999999999` and opts into these comparator
  rules. Its content hash changes; downstream profile pins must be refreshed.

## 0.13.0 — 2026-09-28

- Entity profiles can declare exact-view, anchor, composite-key, n-gram, rare-token,
  and supplied alias-pair retrieval. Similarity operators can partition before
  selection and n-grams can use an explicit document-frequency limit. Strategies
  can override declared top-k limits; diagnostics retain the effective configuration.
- Retrieval keys produce candidates for evidence evaluation. They do not establish
  equality. `exact_identity` explicitly preserves the legacy equality-bucket contract.
- Built-in field mappings, canonical views, normalization, and blocking are data.
  Engine behavior no longer selects normalization or core views by profile name.
  Existing custom native profiles that relied on a built-in name for their prepare
  mapping must declare that mapping; `entity profile init` emits complete examples.
- Candidate generation stops at the first actual budget crossing and reports the
  operator and lower-bound counts. `entity run` writes an advisory sampled preflight
  unless the strategy sets `block.preflight: false`.
- N-gram retrieval reuses dense score storage and materializes only selected
  candidates, preserving saturated-score and tie ordering. `canon_entity_topk.v1`
  retains exact drop counts; its `dropped` list is empty rather than unbounded.
- Solver constraint evaluation groups edges once by component, preserving graph
  order and conflict decisions while avoiding a full edge scan per component.
- The instrument profile declares identifier/anchor retrieval, an issuer/maturity/rate
  composite key, and title retrieval partitioned by maturity and rate. Missing issuer
  LEIs do not silently fall back to issuer names. Large exact-key groups still consume
  pair budgets; compressed scored retrieval buckets are not part of this change.
- Fixed-decimal evidence comparisons accept leading-dot values such as `.625`
  without floating-point conversion. Malformed values and out-of-tolerance conflicts
  remain rejected or reported under the existing contracts.

Exact registry lookup and review-gated promotion remain unchanged. Profile bytes,
operator IDs, canonical-view provenance markers, and dependent artifact hashes change;
old and new workbench artifacts are not claimed byte-identical across that migration.
