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
| Appendix D same-cell file | `01c6b1c0-0821-83a1-006c-c7030888b8de` | 3,527 ms | BX 287/4/0; MN 2,332/22/0 |
| Appendix D complete-reach file | `01c6b1c0-0821-784b-006c-c7030888c3c6` | 2,332 ms | BX 290/1/0; MN 2,352/2/0 |

The two D claim classes are deliberately separate. `same-cell` reproduces the
legacy H3-home-cell candidate restriction. `complete bbox reach` scans the
pinned parcel snapshot behind a complete bounding-box prefilter before applying
the exact spatial predicate. The difference is candidate reach, upstream of
predicate or solver truth.

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
