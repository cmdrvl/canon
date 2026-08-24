# E4 Adjudication — bd-1g4x, first offline pass

Date: 2026-08-24. Harness: `tests/geo_adjudication.rs`
(`cargo test --test geo_adjudication -- --nocapture`). Fixtures:
`e4_gate_v2_population.json` + `e4_gate_v2_evidence_enrichment.json`
(companion, join contract pinned by
`evidence_enrichment_fixture_stays_joined_to_the_population_fixture`).

> **Not a release claim.** Every precision-flavored number below is limited by
> the Gate V2 truth instrument (bd-179b rebuild open). This pass measures what
> admitted-sound evidence does to residuals; it does not certify precision.

## Method

For each of the 15 frozen non-condo multi-BBL cases:

1. **Base solve** — universe = candidate parcels, soft PIP preferences only,
   no hard constraints (the shipped bd-2kjx.3 kernel).
2. **PAD-span channel** (§16.3 row 3, sound → hard): parse integer tokens
   from every asserted address string; a candidate satisfies if its PAD
   `min/max_house_number_int` span contains any parsed number; compile as
   `AnyOf`. Integer comparisons only — no string similarity, no floats.
3. **Asserted-SQFT bands** (rows 6/7, empirical → **diagnostic only**):
   ±25% band against MapPLUTO `bldg_area`, counted never constraining
   (band width declared per §2.1's honest half-width).
4. **Geocode discs** (row 1): unavailable offline — parcel coordinates are
   not landed. Recorded as a gap, not approximated.
5. Truth-survival after every channel checked via
   `model_satisfies_request` (exact membership). Soundness claims are gated
   on **truth representability**: truth inside the candidate universe AND
   carrying attribute rows.

## Predeclared verdict ladder (fixed before the run)

ResolvedByPadSpan / CollapsedHonestAmbiguity / UnchangedNonvacuousChannel /
ThinEvidenceUnchangedVacuousChannel / RefutationFinding / BaseConflict /
ChannelBudgetFallback / TruthUnrepresentableReachLimit.

## Results

Denominator: 15 cases. **Truth representable: 7/15.**

| case | cand | repr | pad set | base count | after count | verdict |
|---|---:|---|---:|---:|---:|---|
| 096dd0bb45f6388c | 49 | no | 0/49 infeasible | 5.63e14 | — | reach limit |
| 0b6d06c62cc8a31e | 41 | no | 8/41 | 2.199e12 | 2.190e12 | reach limit |
| 338e7d4c248865da | 19 | no | 9/19 | 524 287 | 523 264 | reach limit |
| 3cf11e9a58e3b710 | 92 | yes | 8/92 | ≥u64 (sat) | ≥u64 (sat) | unchanged-nonvacuous |
| 41858dad7a1286af | 92 | yes | 9/92 | ≥u64 (sat) | ≥u64 (sat) | unchanged-nonvacuous |
| 4493d4ab4cb35283 | 11 | no | 0/11 infeasible | 2 047 | — | reach limit |
| 5ef8cda2d818cdb0 | 28 | yes | 3/28 | 268 435 455 | 234 881 024 | collapsed honest |
| 873c99f8121c08eb | 43 | yes | 5/43 | 8.796e12 | 8.521e12 | collapsed honest |
| 9922164563c925c5 | 27 | no | 0/27 infeasible | 1.34e8 | — | reach limit |
| a001bb36b4af9488 | 7 | yes | 3/7 | 127 | 112 | collapsed honest |
| ced7ad9f0d74abf7 | 40 | no | 3/40 | 1.100e12 | 9.621e11 | reach limit |
| d37aed509107bbbf | 58 | yes | 3/58 | 2.882e17 | 2.522e17 | collapsed honest |
| da11f90bd6d69f44 | 19 | yes | 4/19 | 524 287 | 491 520 | collapsed honest |
| e43aa7e8cfe9cc00 | 31 | no | 0/31 infeasible | 2.147e9 | — | reach limit |
| eccfde711e69d8fa | 61 | no | 18/61 | 2.306e18 | 2.306e18 | reach limit |

Verdict totals: **5 collapsed-honest · 2 unchanged-under-saturation ·
8 reach-limit · 0 refutation findings · 0 base conflicts ·
0 budget fallbacks (budget raised to 2^21) · 0 ρ violations.**

## Readings

1. **ρ held everywhere it was decidable.** No representable truth was ever
   pruned by the sound channel. The tripwire caught one methodology error
   during bring-up — conflating unrepresentable truth with violation — which
   is now structurally separated (`truth_representable` gate).
2. **The PAD-span channel has real but modest VoI**: collapse fractions of
   3–13% of residual space on the five measurable cases (e.g. 127→112,
   524287→491520). Residuals stay astronomically wide because free-parcel
   inclusion dominates; composition needs the *joint* channels (geocode
   discs once coordinates land, footprint exclusivity) before singletons
   appear. This quantifies §9's value-of-information row-by-row instead of
   asserting it.
3. **Saturation honesty worked**: the two 92-candidate cases cannot report
   exact counts; their rows are lower bounds and their collapse is recorded
   as unmeasurable rather than faked.
4. **Reach is now the binding measurement gap**: 8/15 truths are outside the
   adjudicable universe (missing candidate membership or attribute rows).
   This sharpens L.3's 72/79 into a per-case ledger and tells acquisition
   exactly which parcels to land next.
5. **SQFT-band calibration is thin**, consistent with L.2: truth parcels hit
   the ±25% band in 1 of 15 cases (4/4 hits there), candidates hit sparsely.
   Row 6 remains sparse-here, not dead.


## Joint pass — geocode-disc channel (containment-corrected, 2026-08-24)

Pinned fixture: `e4_gate_v2_geodisc.json`. **The landed derivation
supersedes an earlier centroid-distance draft from the same day**: two
provisional rooftop "geocode-defect proofs" were overturned by polygon
containment — every asserted point has lots at `nearest_mm = 0`
(`ST_DWITHIN(GEOM_GEOG, pt, 8)`). The centroid proxy pruned unsoundly on
large lots; the canonical disc is geometry containment per plan §3, exactly
as Appendix D argued for the footprint predicate. Process receipt: the
harness tripwire plus one verification query killed the unsound admission
before it reached any conclusion.

Channel semantics (sound direction): **every asserted property must be
covered by some selected parcel whose geometry lies within its tier radius**
(rooftop 8 m; nearest_rooftop_match / range_interpolation 150 m) — one hard
`AnyOf` per property, intersected with case candidates.

Joint-run results (base vs base + PAD-span + all property discs):

| Outcome | Count | Notes |
|---|---:|---|
| rho violations (representable truth pruned) | **0** | across every applied channel |
| Measurable honest collapses | 3 | a001 127->112; da11f 524287->442368 (-15.6%); plus reach-limited shrink |
| Largest measured collapse | 4.6x | 0b6d 2.199e12 -> 4.79e11 on a reach-limited case |
| ChannelBudgetFallback | 7 | merged PAD+disc components exceed 2^21 width — counts withheld, never guessed |
| UnchangedNonvacuous (u64-saturated) | 2 | both 92-candidate cases |
| Reach-limit rows | 8 | adjudication withheld per L.3 gate |

Readings:

1. **Containment discs prune strictly harder than centroid proxies while
   staying sound** — overlap pulls adjacent frontage lots into the
   existential set, buying correctness and pruning power together.
2. **The binding constraint on E4 completion is solver component width**,
   not evidence availability: seven cases cannot report exact joint
   residuals inside 2^21 assignments. The named follow-up is factorized
   decomposition inside merged components (Regin-style SCC /
   tree-decomposition per the section 6 ladder).
3. Reach stays 7/15 — eight rows still need their truth parcels landed
   before adjudication can speak.

## Next (still open on bd-1g4x)

- Land parcel coordinates/centroids to admit geocode-disc channels exactly
  (integer frame per §4) and re-measure joint collapse toward singletons.
- Extend population to H.4's 79 non-condo multi-BBL loans (needs one
  structured derivation against the recorded gate SQL).
- Consolidate per-case verdicts into the §16.1 claim-class report once
  bd-179b's truth-gate rebuild lands; until then nothing here promotes.
