# Step 3 kernel replay on DEV-1 and H1-P (2026-09-29)

Script `step3_generic_replay.py` (adapted from the Pryor replay), summaries `step3_generic_replay_DEV-1.json` and `step3_generic_replay_H1-P.json`. Pinned canon 0.14.0, frozen snapshots from `step2_snapshot_manifests/`.
Rules fixed before any label-based scoring: universe = Overture buildings >= 1,000 m2 within 1,300 m of any anchor with a coordinate (all competing anchors kept); partition = nearer its nearest anchor than to the
nearest Foursquare place more than 30 m from every anchor; module (>= 3 footprints within 5% area, each >= 20,000 m2) and Microsoft cross-layer (within 30 m and 25% area) as in the Pryor rules. Diagnostic; one rule set, one filter value, no accuracy claim.

## Kernel output
| Case | Anchors used | Candidates | Set sizes (partition / module / cross-layer) | Solve |
|---|---|---|---|---|
| DEV-1 | 3 | 69 | 3 / 0 / 58 | ambiguous; residual count saturated; no ranked models |
| H1-P | 7 | 35 | 5 / 5 / 13 | ambiguous; 34,359,738,367 residual models; no ranked models |

As in the larger Pryor universes, no candidate is excluded and nothing is ranked. The kernel output is the whole universe in both cases.

## Reach and precision against the sealed labels (evaluator side; numbers only, report sealed, sha256 `017e8e1186e839e8...`)
The evaluator counted the labeled site's entities from label-derived cues, as in the representability check, so site boundaries are an inference.

| | DEV-1 | H1-P |
|---|---|---|
| Site footprint records (Overture / FEMA / Microsoft) | 62 (13 / 28 / 21) | 15 (5 / 0 / 10) |
| Distinct physical entities in the site | 33 | 5 |
| Site entities inside the >= 1,000 m2 universe | 3 of 33 | 5 of 5 |
| Site built area, and share inside the universe | 38,124 m2, 0.83 | 115,029 m2, 1.00 |
| Site share of the universe (by count / by area) | 3 of 69 (0.04) / 0.07 | 5 of 35 (0.14) / 0.41 |
| Site fully contained in the returned answer | no (30 of 33 entities never reach the universe) | yes |

Reading:
- **DEV-1: the universe filter is the reach problem.** The two shell-sized buildings that hold most of the built area are in the universe, but 30 of the site's 33 entities are smaller than 1,000 m2 and are dropped before the solver.
  By area 0.83 is reached, by entity count 0.09. Whether the small objects belong to the "site" depends on the label grain (address grain here), so this depends on how the association is defined.
- **H1-P: reach is complete and precision is not.** All 5 site entities are in the universe, and the returned answer is the whole universe: 86% of candidates by count (59% by area) are outside the labeled site.
- **Both cases:** the kernel returns the union of everything asserted and cannot subtract, so containment is trivially satisfied when reach is (H1-P) and precision is low. This matches the Pryor finding; the objective, not retrieval, determines the second failure.
- **Discrepancy to resolve:** the earlier representability check called H1-P "partial" (the larger labeled parcel most likely outside the 1,600 m neighborhood). This reach count finds all 5 site entities inside the universe. The two checks used different cues and possibly a
  different definition of the site (the earlier one covered a second parcel). It needs the same adjudicator to reconcile them; until then H1-P reach is stated as 5 of 5 under this evaluator's site definition and unverified beyond that.

## Not yet done
Step 4 (smallest patch) is not started. Because the finding is in the objective (translation, then formulation) and not in retrieval, any patch changes canon geo code and needs a decision on approach and scope before it begins.
