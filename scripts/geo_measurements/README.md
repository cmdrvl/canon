# Canon Geo pinned B/C/D measurements

These queries are the executable source for Appendices B, C, and D of
`docs/PLAN_CANON_GEO.md`. They are a release gate, not retained evidence for the
solver: fixture results must never be presented as a fresh warehouse run.

## Execution contract

1. Call the cmdrvl-data MCP table list and describe operations first. Use the
   fully qualified names returned by discovery; bare names may describe
   successfully but fail in SQL.
2. Run one SQL file at a time through the guarded read-only query operation.
3. Require `ok=true`, a nonzero declared denominator, and all sanity columns to
   pass. Record the SQL returned by the MCP, not a prose summary.
4. Recover the Snowflake query ID from
   `INFORMATION_SCHEMA.QUERY_HISTORY` when the MCP success envelope omits it.
5. A changed release is `snapshot_moved`, not a failed reproduction. Update a
   pin only as a new measurement with its own receipt.

Pinned source snapshots for the 2026-08-28 run:

| Source | Pin | Declared feature grain |
|---|---|---|
| MapPLUTO | `RELEASE='26v1'`, `RELEASE_DT='2026-05-01'` | distinct nonempty `BBL` |
| NYC building footprints | `RELEASE_DT='2026-08-09'`, active only | distinct `OBJECTID` |
| FEMA USA Structures | `RELEASE_DT='2025-06-06'` | distinct `PROVIDER_FEATURE_ID` |

## Fresh receipts

All successful queries below ran through cmdrvl-data MCP on 2026-08-28. Query
IDs were read back from Snowflake query history after execution.

| Measurement | Query ID | Snowflake elapsed | Result |
|---|---|---:|---|
| release-string positive control | `01c6b1b6-0821-83a1-006c-c7030888b86e` | 509 ms | 2,343 parcels / 2,343 distinct BBLs |
| `00_discovery.sql` | `01c6b1c1-0821-83a1-006c-c7030888b8f2` | 1,192 ms | all 9 source-cell counts nonzero |
| Appendix B observations | `01c6b1c1-0821-784b-006c-c7030888c3ce` | 221 ms | 100 parcels + 93 footprints |
| Appendix C density + reach control | `01c6b1d2-0821-784b-006c-c7030888c4da` | 2,222 ms | 1,192 cells / 856,614 parcels; 824 footprints outside parcel-home cells; 0 null H3 |
| Appendix D same-cell file | `01c6b1c0-0821-83a1-006c-c7030888b8de` | 3,527 ms | BX 287/4/0; BK 2,332/22/0 |
| Appendix D complete-reach file | `01c6b1c0-0821-784b-006c-c7030888c3c6` | 2,332 ms | BX 290/1/0; BK 2,352/2/0 |

The two historical D claim classes are deliberately separate. `same-cell`
reproduces the legacy H3-home-cell candidate restriction. `complete bbox reach`
scans the pinned parcel snapshot behind a complete bounding-box prefilter before
applying the warehouse spatial predicate. The difference is candidate reach,
upstream of predicate or solver truth. Snowflake GEOGRAPHY arithmetic is an
empirical reference here, not Canon exact-local-integer truth.

The original appendix labeled `882a100d8bfffff` as Manhattan. A fresh
borough/coordinate control on 2026-08-29 proved it is Brooklyn: 2,343/2,343
MapPLUTO rows have `BOROUGH='BK'`, 2,354/2,354 footprint BBLs have borough
prefix 3, and centroid bounds are longitude -73.9361..-73.9236 / latitude
40.6811..40.6897. The executable labels now say `BK_DENSE`; the numeric
predicate and reach denominators are unchanged in those historical queries.

## 2026-08-29 home-cell bridge receipt

Fresh list/describe-first controls used the deployed
`EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_GEOM_V3_EXT` and
`EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT` contracts.

- MapPLUTO v3 completeness query
  `01c6b6dc-0821-9a4f-006c-c703088b300e`: 26v1 has 856,614 rows and
  26v2 has 856,687; both have zero null centroids/WGS84 WKT and all rows have
  null `H3_R7_SOURCE`/`H3_R8_SOURCE`. H3 therefore remains a derived WGS84
  sibling, not source-plane geometry.
- Footprint parity sample `01c6b6e6-0821-9a4f-006c-c703088b3012`: ten
  release-pinned active footprint centroids from `882a100d8bfffff` replayed
  through h3o at r8 with 10 matches, zero mismatches, and zero 1-microdegree
  boundary-sensitive rows. This is a bounded positive sample, not global
  parity proof.
- Geography correction query `01c6b6e9-0821-9a4f-006c-c703088b3016` proves
  the `MN_DENSE` label was false and is superseded as described above.
- MapPLUTO v3 export query `01c6b6f4-0821-9a4f-006c-c703088b306a`: five
  26v2 derived-centroid rows materialized into two r8 cells, with zero
  boundary-sensitive rows. Two fresh processes emitted identical artifact
  bytes, SHA-256
  `6c95edd213259bef185021b14b5b77321fe809a7766ca84a00119b94612c17bf`.
- Controlled-halo reach query `01c6b6f9-0821-83a1-006c-c703088a39aa`
  (10,819 ms): Canon supplied the explicit h3o r8+k1 cells and Snowflake
  compared same-cell and k1 blocking with a complete bbox reference over the
  release-pinned geom-v3 parcel plane. Brooklyn produced same-cell 2,333/21,
  k1 2,353/1, and complete-reference 2,353/1/0; Bronx produced 287/4,
  290/1, and 290/1/0. The complete reference found zero majority edges outside
  k1 in either cell, and k1 repaired 20 Brooklyn plus 3 Bronx same-cell misses.
  All denominators reconciled.

The v3 rerun supersedes the HOT-only operational counts by one Brooklyn row;
the earlier query IDs remain historical receipts rather than being rewritten.
These receipts validate an executable source-to-blocking bridge and bounded
k1 reach for exactly two cells. They do not prove citywide candidate recall,
other resolutions, or solver correctness.

Appendix B's query returns the frozen observation set. Build the bipartite graph
between parcel and footprint centroids using haversine distance with mean Earth
radius `6,371,008.8 m`; for each declared radius, connected components include
isolated observations. The 2026-08-28 output exactly reproduced every retained
row:

| radius m | components | mean | max | p50 | p90 | size 6–20 | isolated |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 10 | 64 | 3.02 | 17 | 2 | 7 | 8 | 16 |
| 15 | 51 | 3.78 | 25 | 2 | 8 | 7 | 7 |
| 20 | 33 | 5.85 | 34 | 2 | 15 | 7 | 4 |
| 25 | 24 | 8.04 | 37 | 4 | 16 | 9 | 1 |
| 30 | 12 | 16.08 | 49 | 8 | 37 | 5 | 1 |
| 35 | 7 | 27.57 | 59 | 31 | 59 | 1 | 1 |
| 40 | 6 | 32.17 | 59 | 31 | 59 | 1 | 1 |
| 50 | 3 | 64.33 | 77 | 59 | 77 | 0 | 0 |
| 60 | 1 | 193.00 | 193 | 193 | 193 | 0 | 0 |
| 150 | 1 | 193.00 | 193 | 193 | 193 | 0 | 0 |
