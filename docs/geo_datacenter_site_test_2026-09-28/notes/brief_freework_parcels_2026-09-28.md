# Brief: freework canon-geo skills, OC-0048, parcel availability, prior GDELT canon runs

Source: subagent read-only review, 2026-09-28. **Model output, not verified by me.** Line citations are the
subagent's; spot-check before relying on any of them. Abbreviations: FW=freework, CTX=cmdrvl-context/docs,
GD=outcomes/gdelt-cre-signals.

## 1. Freework canon-geo skills: no live gate exists
- `canon-geo` (FW/skills/canon-geo/SKILL.md:15-27) is a tool-harness wrapper over canon_geo_capabilities /
  solve / plan / run. Typed outcomes (ambiguity, conflict, budget_fallback, waiting_for_input, blocked) are
  valid results. Must not invent addresses, parcels, counts or extent (:29-30). Output is experiment output;
  Canon's truth rebuild (bd-179b) gates release claims (:32-36).
- `canon-geo-known-address` (:12-22, 28-36) must end with `report_blocked` / `upstream_missing`, naming gaps
  canon.geo.inquiry_facade, known_address_contracts, partial_acquisition_replan, truth_rebuild.
- Neither live-gate file exists; no frozen suite under suites/canon-geo*/ (policies :3-8). Matrix status:
  "Scripted/fixture proof only" (FW/docs/skill-matrix.md:27-28). Blocking beads bd-2nk.8, bd-2nk.12.
  Plan.md:878: mode B (region + attributes) "not implemented at all"; mode A exists only as an NYC/PAD bundle.
- **Consequence for us: no pass criteria exist for "a claim about a site". A labeled set from this exercise
  would be new.**

## 2. OC-0048 Location Master (CTX/03-products/outcomes/OC-0048-location-master.md)
- Status proposed. "Are these two addresses the same location, and what buildings and parcels does it
  consist of?" Tile-based (H3 res-9) deterministic cascade; entities parcel/building/property; integer scores
  out of 10,000; abstention first-class. Proving ground NYC/PLUTO, not data centers.
- Kill criterion (:710): if the cascade cannot beat address-string matching, tile work stops. That baseline had
  not been measured as of 2026-08-14 (:712-717). No stated link to data-center sites (subagent's inference only).

## 3. Parcel availability (2026-09-01 probe of 45 counties; GD/results/county_parcel_sources.md, 2026-09-17)
"Confirmed" = endpoint queryable, not landed. Only Franklin OH is landed.

| County | Verdict | Basis |
|---|---|---|
| Franklin OH | LANDED | table :89, :138-149 |
| Licking OH | landable | county layer, 83,688 rows, first in landing order |
| Dona Ana NM | landable | county REST, 96,348 rows |
| Laramie WY | landable | county REST, 45,920 rows |
| Mecklenburg NC | landable | 396,941 rows; NC OneMap 442,287; public domain |
| Shelby TN | conflict | county HARD/no-redistribution; TN statewide excludes Shelby |
| Fayette GA, Contra Costa CA | unknown | not surveyed; GA statewide "none free"; CA HARD, owner names removed (AB 1785) |
| Racine WI, Ozaukee WI, St. Joseph IN, Shackelford TX, Taylor TX | likely landable | INFERENCE from statewide coverage (WI V12 72/72, IGIO 92/92, TxGIO 253/254); no doc names them |
| Mayes OK, Richland Parish LA | unknown | OK and LA are among 14 unresolved states |

Internal discrepancy: county_parcel_sources.md says "7 of 8" have parcels (:3) but the table lists 10 rows.

## 4. Prior real canon runs on news-derived sites (GD/canon/, canon 0.12.1)
- No README/spec in that folder. Builder header: every source `uncalibrated`, `soft_with_weight`; candidate
  universe is the only hard boundary.
- **All three solves returned status "ambiguous"**, parcel_candidates=0, hard_forced empty.
  - Site B (Aligned "Project Phoenix", Mansfield PA): 14 FEMA candidates, 16,383 compositions; rank 1 = all 14
    (~1.0M SF). FEMA retail/wholesale structures stay in because soft evidence cannot subtract.
  - Site C (Oppidan / McDonald Steel, 100 Ohio Ave OH): 13 candidates, 8,191 compositions; rank 1 = all 13.
  - **Site G (Google New Albany OH): 11 Overture candidates, 2,047 compositions; rank 1 = all 11 (~2.16M SF);
    support 13 for four ~287k SF hall modules, support 5 for a 57k SF building 1.39 km away. No FEMA match
    (new construction).** Epoch also lists Google New Albany: a direct cross-check target.
  - Sites D (Dona Ana) and E (Laramie/Cheyenne) not located: no address, nothing in Overture/FEMA yet.
- What was resolved: a ranked candidate set with support attribution and hashed provenance. What was not: exact
  site membership. Radius universe of 34 with soft-only evidence = 2^34-1 compositions, so the universe was
  bounded to the union of positive assertions (Demo 0 pattern). Fixes named: calibrate a source or land parcels
  (site_dossiers.md:506-528).
- **Contradiction:** dossier summary (site_dossiers.md:479-505) says brownfield sites "resolve fully"; canon's own
  status was "ambiguous" with all candidates included. "Located" there means the anchor tract, not the footprint.

## 5. Grading (designed, not measured)
- PLAN_GDELT_NEWS_15MIN_LANDING.md:341-343: "exact, relaxed, ranked_candidates, city_level, unresolved". G-4
  acceptance: "Pittsburg resolves exact again from landed evidence alone; unresolved sites name the parcels they need".
- canon-geo-site-workup SKILL.md:20-26 defines the ladder. **No "exact confirmed" vs "exact candidate" split is
  documented in canon material** (that split comes from the AIBuildout posting lane).
- No numeric calibration or held-out set for site grades.

## Implications for the test design (mine)
1. We would be building the first labeled evaluation set for data-center sites. That is itself a deliverable.
2. Add a parcels arm where parcels exist: Ohio (Franklin landed; Licking landable) holds several Epoch sites
   (Google Columbus, Google New Albany, Meta Prometheus, AWS New Albany, others).
3. Re-run Google New Albany as a bridge: canon output already exists (site G), Epoch has an address for it.
4. Expect ranked_candidates, not exact membership, wherever parcels are missing.
