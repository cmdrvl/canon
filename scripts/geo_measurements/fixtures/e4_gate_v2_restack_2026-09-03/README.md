# E4 gate_v2 population re-stacked through the four G1 channels (2026-09-03, orchestrator run)

Inputs: tests/fixtures/geo/e4_gate_v2_population_request.json (15 cases), the D1 receipts under
../d1_residuals/mcp_stack_2026-09-03 (assessment roll FY2026P3, PAD BBL, footprints, ACRIS parties, geocodes),
and e4_case_bindings.json (case -> D1 subject by deed truth-set match -> loan -> document -> borrower).
Baseline: PAD observations only on the MapPLUTO universe (e4_overlay_base.json -> e4_eval_base.json).
Stacked: universe widened to assessment-roll lots on the case blocks; PAD + roll exact-owner exclusion (hard)
+ affiliate preference + roll gross-sqft band 0.7x-1.6x (hard) + geocode preference (e4_overlay_roll.json.gz,
e4_pop_roll.json.gz -> e4_eval_roll.json). Built by e4_build.py from the scratch receipts.
Proof class: fixture replay of an observed warehouse snapshot, not live; not a gate pass. The frozen gate in
tests/geo_adjudication.rs is untouched. Summary in e4_summary.json.
