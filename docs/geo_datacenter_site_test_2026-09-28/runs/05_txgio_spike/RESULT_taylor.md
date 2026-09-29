# Run 05: TxGIO parcel spike, Taylor County TX (Abilene)

Run 2026-09-28. New spot scraper `cmdrvl-curves/spot_scrapers/txgio_land_parcels/` (lint clean, 7 tests pass).
Command: `snapshot --county taylor --format shp`. Summary: `extraction_summary_taylor_shp.json` (this folder).
Raw + extracted files: `cmdrvl-curves/local_data/spot_scrapers/txgio_land_parcels/20260928T235510Z/` (gitignored).
Question (Zac): is Texas easy to land under our conventions, and would it look exactly like Franklin?

## What was fetched
- Source: public TNRIS warehouse bucket, `.../stratmap-2025-land-parcels/items/shp/...taylor_48441_shp.zip`.
- 22,120,579 bytes, sha256 `45ada43eec80cd8f...`, Last-Modified 2026-05-15, ETag `618d7f58...`. Took about 10 s.
- The 2025 collection is per-county zips (FGDB and SHP) plus a 2.8 GB statewide zip. Sizes (fgdb): Taylor 15.7 MB,
  Ector 15.0, Bexar 147, Harris 224. My first guess that the statewide file was the only asset was wrong (I had printed
  only base names from the bucket listing).

## What the file is
| | Taylor (TxGIO 2025) | Franklin (landed) |
|---|---|---|
| Polygons | 70,602 (4 null geometry, 5 invalid) | 494,704 |
| CRS | **EPSG:4326 already** | EPSG:3735 -> 4326 with transform receipts and evidence plane |
| Parcel key | `Prop_ID` (67,771 distinct: **786 IDs repeat across 3,613 polygons, one ID on 1,791**), `GEO_ID` (3.7% null) | `PARCELID` (repeats for condos, flagged) |
| Acreage | `GIS_AREA` 100% populated, matches polygon area (median ratio 0.9999); `LEGAL_AREA` populated on 36% (95% of those within 25% of geometry) | `ACRES`, `STATEDAREA` |
| Owner | `OWNER_NAME` (3.7% null) | `OWNERNME1..3` |
| Land-use | `STAT_LAND_` state code, **24% null**, doubled like "A1,A1" (F1 commercial and F2 industrial exist; no data-center code) | `CLASSDSCRP` text, includes "data center" |
| Building area | none | `BLDGAREA` |
| Sale price/date | none | `SALEDATE`, `SALEPRICE` |
| Values | `LAND_VALUE`, `IMP_VALUE`, `MKT_VALUE` | `LNDVALUEBA`, `BLDVALUEBA`, `TOTVALUEBA` |
| Situs | `SITUS_ADDR` (0% null but city/state/zip columns 74-90% null) | `SITEADDRES` |
| Release identity | `TAX_YEAR` 2025, `DATE_ACQ` 20250201 (per county), file Last-Modified 2026-05-15 | hub export Last-Modified, release tag |
| Source stamp | `SOURCE` = "TAYLOR APPRAISAL DISTRICT" | Auditor |

## So: does Texas look exactly like Franklin? No.
- **Easier:** already WGS84 (no State Plane transform or CRS admission evidence), tiny per-county files, one schema
  across counties, area in acres already computed.
- **Harder or different:** keys repeat (stacked polygons) so a parcel is not one row; use codes are state codes, not
  text; no building area, no sale data; freshness is per county (each appraisal district's own acquisition date);
  completeness varies by county (this county: 24% null land-use, 74-90% null situs parts). About 254 files and 14M
  polygons for the whole state, versus one county. The 3,500-line Franklin scraper (geometry admission, bitemporal
  release columns, evidence plane, catalog) would need a Texas-shaped rewrite, not a copy.
- **Not tested:** the other 253 counties. Their schemas share the TxGIO standard, but null rates and stacking will
  differ. Bexar (147 MB) and Harris (224 MB) are 10-15x this file.
- **MCP/warehouse:** nothing here touches Snowflake. A landing would need HOT-table columns, H3 keys and catalog
  registration; that is separate work I have not scoped.

## What the file adds to the canon geo test right away
1. **Acreage is usable everywhere**, so a size band (run 04) can be tested on Texas.
2. **Cross-check of an agent-reported label.** The research agent reported ten Taylor CAD account numbers from the Texas
   Comptroller Ch. 312 abatement record for Lancium Abilene LLC (1093719, 1093718, 1093720, 1096944, 1096945, 1096946,
   1096947, 1093721, 1089582, 1061063). In this file **two are present** (Prop_ID 1089582, 16.0 ac, 5614 Spinks Rd;
   1061063, 15.1 ac, Spinks Rd), both owned by LANCIUM ABILENE LLC. The other eight IDs are not in this Feb-2025 file
   (they may be newer accounts; not established).
3. **Assemblage, confirmed by data.** Twelve parcels in the file are owned by Lancium Abilene LLC, 345.2 acres total
   (14 to 85 acres each). Epoch lists "OpenAI Stargate Abilene" at 5502 Spinks Rd; two of these parcels are on Spinks
   Rd. A single-parcel assumption cannot describe this site; a set-sum size band would be needed.
4. **Owner names are searchable** (3.7% null), which gives an operator-to-parcel bridge that Franklin also has.

## Caveats
- The Ch. 312 account list is still agent-reported; only the two overlapping IDs are now corroborated, and only for
  owner and address, not for the abatement.
- "Lancium Abilene LLC" owning parcels on Spinks Rd does not by itself say those parcels are the Stargate campus.
- Total polygon area in the file is 618,880 acres against Taylor County's roughly 588,000 acres, so stacked
  polygons overlap. Sum only after de-stacking.
- One county, one vintage, one run.
