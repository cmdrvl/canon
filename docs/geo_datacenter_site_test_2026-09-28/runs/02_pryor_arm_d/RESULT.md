# Run 02: arm D (canon composition) on Google Pryor OK

Run 2026-09-28. Script: `run_arm_d_pryor.py` (rules fixed before looking at the truth label). Canon 0.13.0.
Pipeline: spec -> `build_warehouse_rows.py` (gdelt-cre-signals/canon) -> `canon geo materialize-evidence` ->
`compile-evidence` -> `solve`. Artifacts here: `pryor.spec.json`, `.warehouse_rows.json`, `.evidence_request.json`,
`.evidence_compilation.json`, `.solve.json`. Read-only against Snowflake (queries done before the build; receipts
`sha256:f8d00bb7...` Overture, `5b665b04...` Microsoft, `5ce01633...` Foursquare).

## Setup
- Universe (hard boundary): Overture buildings >= 10,000 m2 with centroid within 1,300 m of the Foursquare "Google"
  POI at 4581 Webb St: **10 buildings**.
- Evidence (all `uncalibrated`, `soft_with_weight`, per-member `prefer_member`, as the skill and builder require):
  - partition (w5): nearer the Google POI than to any named industrial Foursquare neighbor -> 5 of 10
  - repeated module (w3): >= 3 footprints within +/-5% area, each >= 20,000 m2 -> 3 (29.3k, 30.3k, 31.1k m2)
  - cross-layer (w3): Microsoft 2026-07 footprint within 30 m and +/-25% area -> 9 of 10
- **The official permit point (ODEQ, 36.24250, -95.33020) was never evidence.** It is used only for scoring below.

## What canon returned
- status **ambiguous**; 10 building candidates; 1,023 structurally feasible compositions (2^10 - 1); `hard_forced` empty.
- **Rank 1 (cost 0) is the whole universe of 10 buildings, and it is the only model at cost 0.**
- Rank 2 onward (cost 3) drops one building whose only support is the Microsoft cross-layer source.
- The same pattern as canon sites B, C and G (`outcomes/gdelt-cre-signals/canon/`).

## Where truth-like compositions rank (out of 1,023)
| Composition | Rank | Cost |
|---|---|---|
| whole universe (10) | 1 | 0 |
| Foursquare-partition set (5) | 136 | 18 |
| three repeated 30k m2 halls | 629 | 33 |
| building nearest the official point + next nearest (2) | 1,009 | 53 |
| building nearest the official point alone (overture:56e84c8d, 149 m from truth) | **1,019** | 56 |
| largest building alone (1.39M sq ft, 558 m from truth) | 1,020 | 58 |

## Why (mechanism, not a bug)
Every soft observation only says "prefer this member" and charges a cost when that member is absent. Nothing charges
for including an extra member ("soft evidence cannot subtract", per the skill and the earlier runs). So the cheapest
composition is always the union of everything any source asserts, and small compositions are penalized. With this
evidence class the composition step cannot narrow toward a smaller footprint; it can only rank *which subset of
asserted members to drop first* by source weight.

## Scoring against the official point (after the solve)
Distance of each candidate to the ODEQ permit point: nearest 149 m (overture:56e84c8d, 125k sq ft, supported only by
the Foursquare partition source and NOT present in Microsoft's 2026-07 release), then 337 m (72ea9c27, which sits 15 m
from the Red Devil factory POI at 4175 Webb St, so it is probably that factory), 558 m (the 1.39M sq ft building),
then 600-920 m for the rest.
- Simple rule "nearest large building to the Foursquare pin" (arm C style) picks overture:56e84c8d, the building
  nearest the official point (149 m).
- Canon composition with this evidence picks all ten. It contains the nearest building (reach holds) but does not
  identify it, and it ranks that building alone second-to-last.
- N = 1 site. This is an observation, not an accuracy measurement.

## What this tells us about canon geo's value here
- **Adds:** an exact, reproducible enumeration (deterministic; byte-identical rerun in 0.05 s on site G), typed
  ambiguity instead of a false answer, and hashed provenance for every source that touched the decision.
- **Does not add (yet):** discrimination between buildings. With uncalibrated evidence it returns the universe.
- **What would change that** (from canon's own docs): parcels (hard membership by containment and ownership),
  calibrated sources (a named population plus a falsification rule) so evidence may act as a hard constraint, or a
  cardinality/size band derived from something independent (for example a permit's stated building area). None is
  available for Mayes County OK today (parcel status unknown).

## Caveats
- The universe rule and thresholds are mine (10,000 m2, 1,300 m, +/-5%, 30 m, +/-25%). Different choices change the
  universe, not the qualitative result.
- Official truth is a single facility point (grain: campus), so it cannot score building-level membership.
- overture:72ea9c27 being a neighboring factory shows the partition source works as intended on one building, but its
  weight is hand-chosen.
