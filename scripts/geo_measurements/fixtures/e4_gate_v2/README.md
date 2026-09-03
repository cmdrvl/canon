# E4 Gate V2 Restack Fixture

Proof class: fixture replay of a retained warehouse snapshot, not live proof and
not a gate pass.

This directory records the bd-2ezy replay of the frozen 15-case E4 population in
`tests/fixtures/geo/e4_gate_v2_population_request.json` through the new G1
evidence producers. The PAD-only baseline is evaluated on the original
MapPLUTO universe. The stacked replay widens only the frozen candidate-universe
blocks against `assessment_roll_fy2026p3_lots.json.gz`; it does not widen from
truth blocks.

`tests/geo_e4_restack.rs` drives the stage library entry points because there is
not yet one `geo run` plan that performs the cross-case joins from retained D1
receipts into E4 case identifiers. The shell harness in
`scripts/geo_demo/e2e_e4_gate.sh` then replays the committed population and
overlay requests through `canon geo stack-evidence` and
`canon geo evaluate --artifact-dir`.

The committed measurement caps each case at `max_assignments = 2097152`, matching
the frozen E4 fixture's declared per-case budget. Solver
fallbacks are therefore typed abstentions, not guessed residuals. The original
frozen fixture budget is not raised for this replay; the widened assessment-roll
blocks include up to 599 parcels and the hard GSF band couples whole block
universes.

`footprints.json` is a summarized active-BIN count by BBL. The test helper
expands those counts into deterministic fixture BIN rows before calling the
footprint-roll stage. The condo bridge stage is replayed and recorded; this
15-case fixture has zero unit-lot truth cases, so there is no condo truth-grain
reach adjustment in `summary.json`.

Retained D1 observations are attached by the first matching D1 subject with the
same deed truth-parcel set. Retained hard PAD membership observations are
admitted only when every member in the original retained observation is present
in the active universe. The replay does not subset-trim hard membership
observations into new claims.

## G1 Numbers

| harness | cases | no observations | reachable | resolved | ambiguous | conflict | component fallback | deed-exact | false merges | truth exclusions | residual <=16 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| PAD-only baseline | 15 | 2 | 7 | 0 | 15 | 0 | 0 | 0 | 0 | 0 | 0 |
| stacked G1 evidence | 15 | 0 | 7 | 4 | 9 | 0 | 2 | 3 | 0 | 1 | 8 |

## Files

- `base_population_request.json`: original 15-case E4 population request with
  the explicit measurement assignment cap.
- `widened_population_request.json`: assessment-roll block-widened population
  request used by the stacked harness.
- `pad_only_overlay_request.json`: retained PAD-only baseline translated from
  the D1 stack by loan binding.
- `stacked_overlay_request.json`: retained PAD/geocode plus assessment-roll
  owner, roll GSF band, and footprint floor observations.
- `cases.json`: per-case status, residual count, truth reach, forced set,
  deed-exact flag, stage observation counts, and conflict explanation when
  present.
- `summary.json`: source fixture digests, stage summaries, and the G1 table.
