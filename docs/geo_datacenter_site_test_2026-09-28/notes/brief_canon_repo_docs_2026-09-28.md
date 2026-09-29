# Brief: canon repo geo docs (design, measured results, addressless experiment, evaluate, workflow)

Source: subagent read-only review, 2026-09-28. **Model output; I spot-checked only the "15/15" claim
(see bottom).** Citations are the subagent's. R=README.md, A=AGENTS.md, P=docs/PLAN_CANON_GEO.md,
ARCH=docs/CANON_GEO_AGENT_ARCHITECTURE.md, ADDR=docs/PLAN_CANON_GEO_FIRST_ADDRESSLESS_EXPERIMENT.md,
FX=scripts/geo_measurements/fixtures. Nothing was run against data (only `--help`).

## 1. What canon geo is (and is not)
- "An agent tool for evidence-backed geographic association": which entity does the evidence identify, at
  what grain, as of when, and why trust it (A:135-145; ARCH:44-56).
- **Output is not a coordinate.** It is the exact set of parcel/building compositions still consistent with
  the admitted evidence (the residual), the forced backbone, soft rankings, conflicts, and next evidence
  needed (P:264-265). "A singleton is a decision, a doubleton is an honest doubleton."
- Geocodes are admitted only as weak bounds (rooftop r=8 m, interpolated r=150 m; P:330-331). An empty model
  set is proof a source violated its own error model (P:345-355).
- Measured on the historical NYC CMBS truth set: geometry point-in-polygon was ~66% precise at lot grain,
  mostly failing by geocode error or representation, not ranking (P:3229-3236, 3821-3837).
- Shipped scope: parcel/building composition. Source-neutral address membership is NYC/PAD-only and open.

## 2. Evidence admission
- Every attribute enters through a declared, versioned relaxation contract (rho). Classes: LOGICAL,
  EMPIRICAL (calibrated on a named population with a falsification rule), DIAGNOSTIC, REJECTED (ARCH:471-477).
- Hard narrows/empties the feasible set; soft changes cost only; diagnostic changes neither.
- Source count is provenance, not independent information (lineage ids carried).
- Address evidence is existential only. Owner match "permits, never forbids".
- Calibrated hard (D1, NYC): size band on MapPLUTO BLDGAREA 0.5x-2.6x asserted sqft (13/16 truth in band);
  tax-roll owner exclusion (194/224). Weak: footprint-count floor. Soft/uncalibrated: geocode point-in-parcel
  (50/70), descriptive name/address/unit/year/type, **Foursquare/Overture place support (uncalibrated
  prefer_member weight 1; experimental adapter, not a shipped profile)**.
- Imagery/observers (NAIP, 3DEP, ortho) are PROPOSED only.

## 3. Measured results and open gates (all scoped, none release claims)
- ACRIS truth-set precision: ledger 154/233 = 66.09%, entity 186/233 = 79.83%, 186/193 = 96.37% with gross
  class abstained (P:3931-3937). "Not a release claim." Some "gross" class is likely truth contamination.
- E3 re-ranking: 0/7 true-lot wins; 72/79 failures unreachable by construction. Premise CUT.
- E4 solver: 17-case harness "reaches only 9/17 truths" (P:1389-1391).
- Population evaluate: 70/70 exact residual artifacts, reach 38 full / 15 partial / 17 none, **69 ambiguous
  abstentions and 0 backbone true positives: "positive solver operation, not evidence-driven resolution"**
  (P:3599-3610).
- G1 restack of the 15-case E4 population (fixture class, not live): PAD-only baseline reachable 7/15,
  resolved 0; stacked evidence reachable 7/15, resolved 4, deed-exact 3, false merges 0, residual <=16: 8.
  **Truth fully inside the universe stays 7/15: "the binding constraint" (P:2019).**
- D1 fourth pass (roll universe, exact-owner hard, GSF band): 16 resolved (6 deed-exact), 44 ambiguous,
  4 conflict, 15 truth exclusions. Franklin County OH reach: 147/151 subjects (P:1333-1348).
- Open: bd-179b truth rebuild (79-case denominator "five genuine nonduplicate cases short"); E4 acceptance;
  E5 non-NYC tier curve not run; live-scale proof not shipped.

## 4. Addressless experiment (most relevant to us)
- Hypothesis: bounded geography + descriptive attributes can collapse the candidate universe with no address
  (ADDR:30-35). Framing: "Do not ask Canon Geo to guess the address" (ADDR:320).
- Results (FX/addressless_*): Summit at Sabal Park was a blind FAILURE (889 -> residual 135, but a
  zero-tolerance unit-count band excluded the true parcel). Courtney Cove: hints ranked truth first among 889
  but hard residual stayed 889; ABSTAINED. Orlando: 2 of 3 uniquely soft-ranked. Cornerstone with
  Foursquare/Overture: 2,983 tied -> 1 preferred parcel, 347 buildings -> 2-building tie; corroboration
  diagnostic, not a fresh blind result.
- Supports discovery and corroboration at parcel grain, not a proven resolver. Cohort: 5 properties, 2
  Florida counties.
- **Data centers: no experiment has been run.** Shipped profile does not fit: 5 channels (name, address,
  type, unit_count, year_built), no acreage/owner/planning-id channel, and it is single-member ("one parcel or
  one building"; data-center sites are likely parcel assemblages). ADDR:275-277 defers PowerMW and says the
  result may be an assemblage.

## 5. `canon geo evaluate`
- E4/E5 gate instrument: "labeled composition cases without exposing labels to composition logic".
- Input `canon_geo_population_request.v0`: cases[] each {id, evidence (universe, contracts, observations,
  budgets), truth_plane, truth:{parcels[], buildings[]}}. truth_plane values include gate_v2_historical,
  address_derived_control, deed_grain_instrument, human_adjudication.
- Output `canon_geo_population_evaluation.v0`: resolved/ambiguous/conflict/abstention, false_merge_cases,
  full_truth_recall_cases, candidate_reach full/partial/none, truth_reach_by_grain, solver_truth_exclusion.
- Usable for our sites in principle, but needs a hand-built population request with truth parcel/building
  IDs per site, and evidence over an already-bounded universe.

## 6. Offline workflow for one site (nothing acquires data; all inputs local and pinned)
1. `canon geo capabilities`. 2. Author question / inventory / profile / budget JSON, then `canon geo plan`.
3. `canon geo run --plan --work-dir --input NODE:BINDING=PATH ...` (nine stages: home cells, section,
   materialize-evidence, compile-evidence, propagate, solve, explain, separate, next-evidence).
4. `canon geo inspect --run ... --recommend-next`. Stage leaves can be called singly.
- Parcels are effectively required (default profile needs a parcel universe; building-only is possible).
- Working examples: scripts/geo_measurements/addressless_asset.py:58-190; GD/canon/build_warehouse_rows.py.

## 7. Limits
- Reach is an upstream obligation: a truth excluded from the universe cannot be recovered.
- Hard evidence can exclude truth (Summit). Uncalibrated stays soft/diagnostic.
- A soft winner is not an identity; costs are not probabilities; runs report ABSTAINED.
- A parcel answer does not imply a building or complete extent.
- Summit took 502.5 s in a debug build.
- Value beyond a geocoder needs: parcel/building geometry for the region, attribute rows, ideally calibration
  data for hard admission, and independent truth for scoring. Imagery not required.

## Stale/contradictory docs
ADDR:114-121 (hard constraints from unit/year/type) vs fixture READMEs (uncalibrated = soft); ADDR:3-7
"next-action" vs later fixtures; R:1699-1712 Cornerstone before/after was not a fresh blind test.

## My spot-check (2026-09-28)
The brief could not find "15/15 exact residuals". Verified: P:1391 says "reaches only 9/17 truths"; P:2019 is
the 15-case G1 restack (7/15 reachable, 4 resolved, 3 deed-exact). My memory note's "15/15 exact ambiguous
residuals" (2026-08-23) means solver operation on all 15 cases, not truth found. Memory note clarified.
