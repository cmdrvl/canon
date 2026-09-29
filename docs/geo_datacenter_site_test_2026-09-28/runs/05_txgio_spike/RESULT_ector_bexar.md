# Run 05b: TxGIO Ector (Odessa) and Bexar (San Antonio), and what three counties say about a Texas landing

Run 2026-09-28 by `spot_scrapers/txgio_land_parcels` (`snapshot --county ector`, `--county bexar`, shapefile).
Ector 18,640,964 bytes; Bexar 179,607,999 bytes; both Last-Modified 2026-05-15, both 1/1 ok. Raw and extracted
files are in `cmdrvl-curves/local_data/spot_scrapers/txgio_land_parcels/` (gitignored). Read-only analysis after landing.

## Schema across three counties: mostly one schema, with per-county traps
| | Taylor | Ector | Bexar |
|---|---|---|---|
| polygons | 70,602 | 75,947 | 709,541 |
| CRS | 4326 | 4326 | 4326 |
| null / invalid geometry | 4 / 5 | 56 / 12 | 0 / 29 |
| fields | 37 | 36 | 35 (only ancillary GIS fields differ: OBJECTID, OBJECTID_1, Shape_*) |
| `DATE_ACQ` | 20250201 | 20250801 | 20250701 (per-county freshness) |
| **real parcel key** | `Prop_ID` (67,771 of 70,602) | **`GEO_ID`** (75,464 of 75,947). `Prop_ID` is a float-formatted subdivision-type number: **3,791 distinct**, 2,974 rows `0.00000000` | `Prop_ID` (703,258 of 709,541) |
| `GIS_AREA` (acres) | 100% filled, matches geometry | **all zeros** | 100% filled, matches geometry |
| `LEGAL_AREA` (acres) | 36% filled | **97.5% filled, matches geometry** | 13% filled |
| `STAT_LAND_` null | 24% | 0.6% | 12% |
| `STAT_LAND_` style | codes, doubled ("A1,A1") | **code + description ("F2-Industrial")** | codes, doubled |
| `SITUS_CITY` null | 90% | **100%** | 6% |
Conclusions for any landing: (1) the parcel key field and the reliable acreage field are county-specific, so a
loader must pick them per county, or derive keys and acres from geometry; (2) polygon-derived acres (equal-area CRS)
are the only size that is reliable in all three counties; (3) use-code text differs, so cross-county land-use logic
needs a normalizer; (4) per-county completeness varies widely, so every county needs its own null-rate report.

## Size band alone does not find a site (news sizes)
- Ector, "Texas Critical Data Center" (200 MW, sold or financed, **493 acres**): the narrow band 0.75x-1.25x
  (370-616 acres) leaves **204 parcels** in the county, nearly all native pasture or rural land; the wide band
  0.5x-2.6x leaves 786. With no location anchor beyond "Odessa", a size band cannot pick the site.
- Bexar, "Bexar County" (operational, **158 acres**): narrow band (118-198 acres) leaves **340 parcels**, wide 912.
  The 158-acre figure is probably a campus total across several parcels, which a per-parcel band cannot express.
- Consistent with run 04: size helps only when an anchor already limits the candidates to about 1 km.

## What does work: owner name + parcel + situs, from the county's own record
Bexar records (owners and situs are the appraisal district's, independent of Epoch, Foursquare and any geocoder):
| Epoch site (address as Epoch lists it) | County parcel found | Match |
|---|---|---|
| Microsoft SAT14, 3545 Wiseman Blvd | MICROSOFT CORPORATION, **3545 WISEMAN BLVD**, 33.7 ac, market value $69,185,000, centroid 29.47569,-98.68144 | exact address and owner |
| Microsoft SAT40, 15000 Lambda Drive | MICROSOFT CORPORATION, 15434 LAMBDA DR, 94.5 ac, $306,700,000, centroid 29.41402,-98.80293; also 14785 OMICRON DR 22.4 ac | same street and owner, house number differs |
| Vantage TX1, 14720 Omicron Dr | VANTAGE DATA CENTERS TX1 LLC, OMICRON DR (no number), 44.7 ac, $56,686,260, centroid 29.41563,-98.78938 | same street and operator |
Other data-center-scale owners in the same file that Epoch does not list: Vantage Data Centers TX2 LLC (31.0 ac, Rogers
Rd), QTS San Antonio III LLC (20.1 and 19.5 ac, Omicron Dr), Amazon Data Services (12807 Donop Rd 48.9 ac; 2200 State Hwy
211 37.6 ac), C1 San Antonio V TRP LLC (14719 Omicron Dr Bldg 2, 22.0 ac, $130,000,000), and 13 parcels owned by
Microsoft in two clusters. Owner names are not proof of a data center; the F1 (commercial) state code and
market values in the $50M-$300M range are consistent with one, but I have not verified use.

## A false anchor we avoided (run 01)
In run 01, Foursquare returned the same POI, "Microsoft Data Center, 5150 Rogers Rd", as a name-only match for BOTH
Microsoft SAT40 and Microsoft SAT14. The county file shows 5150 Rogers Rd is a different Microsoft parcel (36.3 ac,
centroid 29.48005,-98.69261), about 13 km from SAT40's Lambda Dr parcel. The run did not use name-only matches as
anchors, so this never became a placement, but a naive name-based anchor would have put SAT40 in the wrong place.

## What this changes for the canon geo test
- Parcels plus owner/situs resolve hyperscale campuses that the Census geocoder and Foursquare cannot (SAT14, SAT40 and
  TX1 were city-level or unresolved in run 01). This is the strongest evidence so far that landed parcels add value.
- Campuses are parcel sets (Microsoft has 13 parcels in Bexar; Lancium 12 in Taylor). A single-parcel assumption fails; a
  set-sum size band is the right shape and is where canon's `integer_sum_band` applies.
- These county records can serve as OFFICIAL LABELS for scoring: address + owner + acreage from the appraisal district.
  Still needed to score canon composition: an anchor that does NOT use the parcel record (Epoch coordinates or a
  permit point) so the test is not circular.

## Caveats
- Three counties, one vintage. Bexar and Ector were inspected once; no claim about the other 251.
- The owner-keyword search used a list I chose (operator and energy names); it finds what it is told to look for.
- "Texas Critical Data Center" and the news "Bexar County" site were not resolved to parcels.
