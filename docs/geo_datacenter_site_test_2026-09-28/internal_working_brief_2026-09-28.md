# Where canon geo adds value on data center sites

Test run 2026-09-28. Bead `bd-dzdy1`. Everything below is backed by a run folder under `runs/` and a saved artifact.
Small samples throughout; nothing here is an accuracy claim, and canon composition output is workbench data, not a
release claim (`bd-179b` gates those).

## Bottom line
1. **Canon geo did not locate a data center better than a good ladder (geocoder + Foursquare + footprints).** On the one
   site where it could be scored (Google Pryor) it returned the whole candidate universe as its best answer.
2. **The limit is evidence reach, not the solver.** Half our sites never got an anchor; the biggest campuses had no POI at all;
   canon's own docs say the same ("reach is the binding constraint").
3. **What canon adds is real but different from locating:** byte-identical reproducibility, exact enumeration of what is
   still possible, typed conflict and ambiguity instead of a false answer, and hashed provenance.
4. **What moved outcomes in our tests was not composition.** It was landed parcels with owner names, an independent
   anchor, and stated sizes.

## What we tested
Sites: Epoch AI's 69 addressed US data centers, our 129 news sites (GDELT, now in Snowflake), and official records
(county parcel classes, state air-permit coordinates). Layers: Foursquare, Microsoft, FEMA and Overture footprints, plus
county parcels we landed or scraped (Franklin OH, Taylor/Ector/Bexar TX). Arms: A geocoder, B + Foursquare, C + footprints,
D canon composition over buildings, E canon composition over parcels with a size band.

## What we found
| # | Finding | Evidence |
|---|---|---|
| 1 | Canon is deterministic across a version upgrade. | Site G re-solved on 0.13.0: canonical hash identical to the stored 0.12.1 result, 0.05 s (run 00). |
| 2 | Half the sites get an anchor from the ladder. | 34 of 69 (Census exact 31, Foursquare +3); 29 no match; 27 city-level, 8 unresolved (run 01). |
| 3 | An anchor is a hint, not a site. | Geocoder vs Foursquare points differ by a median 552 m (n=9); the search radius is 1.5 km (run 01). |
| 4 | The geocoder can be confidently wrong. | "Co Rd 42" matched "42 County Ct" as "exact". My first guard was wrong on 2 of 3 demotions; fixed (run 01). |
| 5 | Composition with soft evidence returns the whole universe. | Pryor: unique cost-0 rank 1 = all 10 buildings; the building nearest the official point ranks 1,019 of 1,023 alone (run 02). Same pattern in the three earlier canon runs. |
| 6 | Big campuses have no anchor to compose from. | Of 18 Franklin parcels the county calls "data center", 8 have any POI within 600 m and none of the large ones do (run 03). |
| 7 | A stated size would prune hard, if we had an anchor and it were right. | Upper bound: ~840 candidate parcels within 1 km fall to a median 14 (wide band) or 6 (narrow); unique for the 55-480 acre campuses (run 04). |
| 8 | News carries a size for 29% of sites. | 38 of 129 have acres or sq ft; 31 acres, 9 sq ft (run 04). |
| 9 | Parcels plus owner names place campuses the geocoder cannot. | Bexar county records: Microsoft SAT14 exact address match; SAT40 and Vantage TX1 same street and owner; run 01 had them at city level (run 05b). |
| 10 | Texas parcels are not "Franklin again". | One schema, per-county traps: Ector's `Prop_ID` is not the key and its `GIS_AREA` is all zeros; keys repeat; codes vs text; per-county freshness. Derive keys and acres from geometry (run 05, 05b). |
| 11 | Canon can audit a size claim exactly. | Bexar "158-acre Microsoft Texas Research Park": Microsoft-owned parcels near Lambda/Omicron total 116.9 ac, so bands needing >= 118.5 ac give a typed CONFLICT (0 sets); with neighbors allowed, 17-28 exactly tied best explanations (run 06). |

## Where canon geo adds value, and where it does not
| It does add | It does not add (yet) |
|---|---|
| Reproducible, hash-checked results | A better location than the ladder |
| Exact enumeration of what remains possible | Discrimination between buildings under soft evidence |
| Typed conflict: "no composition matches this claim" | Any way to tell adjacent parcels from distant ones (no adjacency in the request) |
| Provenance for every source that touched a decision | Ownership relationships (is this LLC an affiliate?) |
| Honest abstention instead of a false answer | Anything for sites with no anchor or no parcels |

## What would change the answer (in the order the evidence supports)
1. **An independent anchor.** State air-permit notices print facility address and often exact coordinates (ODEQ Pryor,
   NMED Los Lunas). Los Lunas is a site no arm could place. Cheap, official, and not circular.
2. **Landed parcels with owner names.** Bexar shows what they buy. Needs a per-county loader keyed on geometry; Franklin's
   3,500-line scraper is not a template for Texas.
3. **A calibrated size.** Canon only lets a band act as a hard constraint after calibration on a named population.
   We have not measured how accurate news-stated size is. That needs pairs of (news size, official size).
4. **Adjacency and ownership relations** in the evidence, so composition can prefer a contiguous campus.

## What I would do next
1. Pilot a **permit-coordinate harvest** for the Epoch sites (OK, NM, IN, TX, OH first). Highest value per effort.
2. **Fix extraction gaps** found here: the Bexar row kept the size but dropped operator and park name.
3. **Build the calibration set** for news sizes (this decides whether size can ever be a hard constraint).
4. **Scope a per-county parcel loader as a bead** rather than copying Franklin. Land counties where we have sites.
5. **Hold further composition experiments** until 1-2 give anchors; composition is downstream of reach.
6. Position canon geo as an **auditor of claims** (conflicts, exact alternatives), which fits the evidence-not-corroboration
   doctrine, not as a locator.

## What we do not know
- Accuracy. Two verified official point labels, 18 county-classed parcels, county records for a few Bexar campuses. No
  scored population.
- Whether news-stated sizes are reliable (one case, "several acres" at Hilliard, looks loose).
- Whether other counties behave like the three Texas ones.
- Labels reported by research agents (Ohio, New Carlisle, Abilene abatement accounts) are unverified except two Taylor
  account numbers now confirmed in the county file.
- The KSAT quote came through a fetch summarizer; our extraction's character spans are consistent with it.

## Where things are
`README.md` (charter, log) · `runs/00_bridge_site_g` · `01_epoch_arms_abc` · `02_pryor_arm_d` · `03_franklin_parcel_arm` ·
`04_size_band_pruning` · `05_txgio_taylor_spike` · `06_bexar_arm_e` · `notes/` (doc briefs, truth labels). Code:
`cmdrvl-curves/spot_scrapers/txgio_land_parcels/` (committed). Bead: `bd-dzdy1` (closed `bd-399so`, `bd-ncw6p`).
