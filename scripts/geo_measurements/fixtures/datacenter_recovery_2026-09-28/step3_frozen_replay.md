# Step 3 kernel replay on the frozen PRYOR snapshot (2026-09-29)

Script `step3_frozen_replay.py`, output `step3_frozen_replay.json`. Pinned canon 0.14.0 (revision `0d54287d9beb...`, from Step 0), frozen snapshot manifest sha256 `56eda4e61a5682de...`.
Rules are the pre-registered arm-D rules, unchanged and recomputed from the frozen bytes only (no warehouse call, no labels, no official point). Only the universe size filter varies.
Diagnostic only: no accuracy is measured and no patch is proposed.

| Universe (Overture, within 1,300 m of the pin) | Candidates | Partition / module / cross-layer set sizes | Soft preferences | Solve |
|---|---|---|---|---|
| >= 10,000 m2 (historical rule) | 10 | 5 / 3 / 9 | 17 | ambiguous; 1,023 feasible; rank 1 = all 10 buildings, the only model at that cost |
| >= 1,000 m2 | 27 | 17 / 3 / 23 | 43 | ambiguous; 134,217,727 residual models, not materialized |
| no size filter | 110 | 47 / 3 / 88 | 138 | ambiguous; residual count saturated at u64 max, not materialized |

## What this shows
1. **The historical universe reproduces from frozen bytes.** The 10 Overture buildings are exactly the historical ten (ids match) and the compilation has the same 17 soft preferences. The solve has the same
   superset signature (rank 1 = full universe, sole model at that cost). The solve bytes are not identical to the historical hash (`6978a1f66f2a296f...` vs `a8584ef52d222118...`); I have not diagnosed why
   (candidate record fields and set sizes may differ slightly from the hand-transcribed run-02 values, e.g. cross-layer 9 here). The Step 0 byte-identical claim stands for the retained run-02 input, not for this rebuild.
2. **Without the size filter the kernel cannot rank anything.** At 27 and 110 candidates the solve completes (status `ambiguous`) but returns no ranked models: the residual set is 1.3e8 and then saturated.
   The kernel factorizes into one independent variable per building (27 of 27 and 110 of 110 factors are single-building, `exhaustive_enumeration`, no constraints), and each factor reports exactly one positive
   assignment out of two. That is consistent with every building's optimum being "include", i.e. the superset behavior scales with the universe. I inferred the meaning of `positive_assignments` from the field names
   and the Step 3 ablation; I have not read its definition in the source.
3. **This is a translation and formulation finding, not a reach finding** for this snapshot: widening the universe (fixing the size-filter reach problem) does not help while the objective cannot subtract.
   Restoring small objects therefore makes the residual space larger without adding a way to choose among it.
4. Consistent with the blind reviewer's trace (`step3_pryor_blind_trace.md`): the snapshot supports retained ambiguity, and the 1,000 m2 filter changes which candidates exist.

## Limits
One site; Pryor has no independent label, so nothing here is a correctness measure. The three source rules are the original hand-chosen ones, not derived from the blind trace. The hall-module set is 3 in every variant
(the rule only ever counts buildings of at least 20,000 m2). The builder (`outcomes/gdelt-cre-signals/canon/build_warehouse_rows.py`) is outside this repo; it is unversioned here.
