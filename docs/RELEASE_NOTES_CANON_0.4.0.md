# canon 0.4.0 Release Notes

## Headline

`canon resolve` is implemented as a deterministic cross-tape structural
resolution workbench. Core `canon` lookup remains exact registry lookup.

## Added

- `canon resolve <REFERENCE_TAPE> <TARGET_TAPE>` end-to-end orchestration.
- `canon_resolve.v0` JSON artifact and summary output.
- YAML strategies with composite IDs, candidate filters, weighted assertions,
  thresholds, ambiguity gaps, and BLAKE3 strategy hashes.
- Assertion/filter operators: `exact`, `canon_match`, `tolerance_pct`,
  `tolerance_abs`, `range`, `date_range`, and `prefix`.
- Gold-set scoring via JSONL `target_id` / `expected_reference_id` records.
- Explicit `--write-back` of matched ID pairs into flat registry entries.
- Witness append by default, with `--no-witness` opt-out.
- Integration coverage for end-to-end fixtures, golden output, determinism,
  refusal envelopes, write-back feedback, and 5K-by-5K bounded-candidate scale.

## Operational Notes

- Exit `0`: every target record matched.
- Exit `1`: at least one target record is unmatched or ambiguous.
- Exit `2`: refusal.
- Write-back creates a new `resolve-matches-YYYYMMDD.json` mapping file and does
  not persist structural attributes.
- If `--gold` is supplied, any regression suppresses write-back. Without
  `--gold`, operators must treat the emitted evidence artifact as the review
  gate before accepting the registry change.
- Registry versions are not bumped automatically after write-back; update
  `registry.json` and review diffs as part of the release/registry workflow.

## Migration

- Existing `canon <INPUT> --registry <DIR> --column <COLUMN>` behavior is
  unchanged.
- Existing `canon.v0` JSON and CSV output contracts are unchanged.
- Existing registries remain valid. To use `canon_match` in resolve strategies,
  make sure the registry contains every exact alias variant needed by both
  tapes.

## Homebrew Tap

- Publish the crate/tag normally after the quality gate passes.
- Ensure the tap formula points at the release tag that includes `canon 0.4.0`
  and the `canon resolve` operator metadata.
- Smoke after tap update:

```bash
brew install cmdrvl/tap/canon
canon --version
canon --describe | jq '.commands[] | select(.name == "resolve")'
```
