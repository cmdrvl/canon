# Namekit Source-Parity Fixtures

This directory is reserved by ENT-P02.7 for the fixture corpus that ENT-P02.10
must materialize. It intentionally contains only this README until the
implementation beads add JSONL fixtures.

Required fixture families:

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

Each row should name its upstream source, profile, input string(s), expected
namekit views, expected reason codes, protected-token status, and any accepted
loss. The authoritative source map is
`docs/namekit/SOURCE_PORT_MAP.md`.
