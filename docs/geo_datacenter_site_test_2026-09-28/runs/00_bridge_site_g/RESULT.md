# Bridge: Google New Albany OH (canon site G) vs Epoch

Run 2026-09-28. Inputs copied from outcomes/gdelt-cre-signals/canon/ (site_g.*). Read-only.

## 1. Canon reproducibility
`canon geo solve --request site_g.evidence_compilation.json` with canon 0.13.0 versus the stored 0.12.1 result:
- Canonical JSON sha256 identical (bc1849c742c9cbdd); evidence_compilation blake3 identical (bc6d587393599f28).
- status ambiguous; 11 building candidates; 2,048 candidate assignments, 2,047 structurally feasible;
  hard_forced empty; every top-level field equal.
- Runtime 0.05 s. Result: **canon is deterministic across the 0.12.1 -> 0.13.0 upgrade on this case.**

## 2. Independent agreement on the anchor
- Epoch AI row "Google New Albany": address 1101 Beech Rd SW, New Albany, OH 43054; 333 MW current;
  Owner Google #confident.
- Canon site G source `fsq-google-point`: Foursquare "Google Data Center, 1101 Beech Rd SW"
  (lineage EDGAR_DB.SOURCE.FOURSQUARE_OS_PLACES_HOT), claim_role stable_identity_anchor, weight 5,
  "membership method not calibrated".
- Same street address from two independent inventories (Epoch's research; Foursquare's POI feed).

## 3. What each arm does with it
| Arm | Result |
|---|---|
| A. Census geocoder on "1101 Beech Rd SW, New Albany, OH 43054" | **no_match** (verifiable_id sha256:4e79d96b...) |
| B/C. Foursquare name + address, footprints | anchor found; 11 Overture buildings within 1,300 m |
| D. Canon composition | status ambiguous; rank 1 = all 11 buildings (~2.16M SF); support 13 on four ~287k SF halls, 5 on a 57k SF building 1.39 km away |

So the plain geocoder fails on this new-build address, Foursquare recovers a point, and canon composition turns
that into a ranked set with provenance, but does not identify which buildings are the site.

## 4. Caveats
- The stored composition used Overture only (no FEMA match: new construction). No Microsoft footprints, no parcels.
- Weights were hand-chosen; the membership method is uncalibrated (canon's own reason string).
- No independent truth label yet for which buildings are Google's, so reach is unscored.
