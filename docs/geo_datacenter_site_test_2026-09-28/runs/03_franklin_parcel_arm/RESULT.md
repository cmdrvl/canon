# Run 03: independent anchors for the 18 Franklin County "data center" parcels

> **CORRECTION (2026-09-28, added when this was committed to canon):** below, "all four Amazon/VADATA parcels" is wrong for the
> set of 18. **Three** Amazon/VADATA parcels are in the 18 (050-011455, 050-011444, 050-002806). The fourth VADATA parcel
> (275-000010, 66.8 ac) is classed "OTHER INDUSTRIAL", not "data center". The finding stands with "three".

Run 2026-09-28. Script `find_anchors.py`, output `anchors.json` (every query keeps its `vid`). Read-only.
Question: for the parcels the County Auditor itself classes "data center" (truth labels, see
`notes/truth_candidates_franklin_auditor.md`), does an independent anchor exist that a parcel-level canon test could
start from? Anchor = a Foursquare OS Places POI (release 2026-08-11) within 600 m whose name matches ONE fixed brand
list (general hyperscaler/colo/neocloud brands plus "data center", decided before looking, identical for every
parcel) or whose category says data center. The parcel's owner, class and building area were not used to pick terms.

## Result
Parcels with at least one brand/category POI within 600 m: **8 of 18**.

| Parcel | Auditor record | Nearest matching POI |
|---|---|---|
| 610-210593 | 555-585 Scherers Ct, 7.6 ac | "Huntington National Bank Data Center @ Cologix", 585 Scherers Ct, 16 m |
| 273-012619 | 5700 Innovation Dr, 4.4 ac | "Expedient Data Center", 5700 Innovation Dr, 44 m |
| 222-004355 | 7300 Souder Rd, 28.5 ac | "Motorists Data Center", 6560 New Albany Rd E, 92 m |
| 222-001940 / 222-004365 / 222-004644 / 222-004441 | four small New Albany parcels | "Aep Data Center" (no address) at 17-482 m; one POI, four candidate parcels |
| 610-207094 | 7500 Alta View Blvd, 7.3 ac | only "Oracle Elevator Company" (a name-match false positive), 276 m |
| **510-180711, 050-011455, 222-004984, 050-011444, 050-002806, 222-002127, 050-011895, 222-002056, 222-005361, 050-011984** | the largest and newest parcels, incl. all four Amazon/VADATA | **none** |

## Reading
- The parcels with a POI anchor are small colocation and enterprise data centers. A POI's own address matches the
  parcel's site address for two (Scherers Ct, Innovation Dr); the others are near-misses or one POI shared by
  several parcels.
- **The large hyperscale campuses, the ones that matter, have no POI at all.** That includes all four Amazon/VADATA
  parcels (55 to 143 acres, 695k to 880k sq ft each) and the 480-acre, 1.17M sq ft parcel. Foursquare does not know
  them, so a Foursquare-anchored ladder cannot even start.
- This is canon's own stated bottleneck seen from the data side: candidate reach / evidence availability, not
  ranking. The solver cannot help where no evidence reaches the site.
- Where anchors do exist (about 4 parcels), the sample is too small to score a parcel-level canon run credibly.
  Two of them (Cologix, Expedient) are trivially resolved by address.

## What would give anchors for the large campuses
- State air-permit records (facility address and often coordinates; see `notes/truth_labels_v0.md` D). For Ohio:
  Ohio EPA permits for backup generators at these sites. Not fetched here.
- Utility interconnection and tax-abatement / development-agreement records naming parcel numbers.
- News plus the county's own recorded deeds for the 2022-2025 land sales shown in the Auditor table.

## Not done
- No canon composition run on the Franklin parcels (too few independent anchors to make a fair test yet).
- No claim about parcel-level precision.
