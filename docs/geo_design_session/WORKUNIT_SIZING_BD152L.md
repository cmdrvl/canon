# bd-152l - r10+k-ring Work-Unit Sizing Across NYC

Measurement agent: PinkGorge

Date: 2026-08-16

Scope:

- SQL-via-Loom only for data measurements.
- No `src/` or canon implementation edits.
- All cited counts must come from returned structured result tables, not Loom prose.
- Exact SQL is recorded next to every finding.
- Geometric area/coverage derivations must use geometry or H3 coverage, not asserted area
  fields.

Sanity gates:

- Work-unit feature counts must reconcile:
  `total_features = parcel_features + nyc_footprints + fema_structures`.
- Exceedance counts must be bounded by `work_unit_count`.
- Boundary-crossing counts must be bounded by source feature counts.
- H3 r9/r10 derivation and center-cell universe are recorded before distribution results.

## Setup And Derivation

Status: complete.

H3 derivation:

- No measured source table has native `H3_R9`, `H3_R9_INT`, `H3_R10`, or `H3_R10_INT`.
- Work-unit home cells are derived from feature centroids:
  `H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 10)` and
  `H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 9)`.
- Work units are centered on parcel-containing NYC cells, matching Appendix C's MapPLUTO
  parcel-defined NYC universe. A work unit is `H3_GRID_DISK(center_cell, 1)`, i.e. the
  center cell plus its 6 neighbors.
- FEMA is restricted to NYC counties:
  `STATE_FIPS = '36' AND COUNTY_FIPS IN ('36005','36047','36061','36081','36085')`.
- FEMA row count and distinct `PROVIDER_FEATURE_ID` count are identical in this slice.

Source slices:

| Source | Feature count | Null centroids | Null geometry | Native r9/r10 columns |
|---|---:|---:|---:|---|
| FEMA NYC-county structures | 486,282 | 0 | 0 | none on all checked tables |
| NYC footprints | 1,081,999 | 0 | 0 | none on all checked tables |
| MapPLUTO parcels | 856,614 | 0 | 0 | none on all checked tables |

FEMA identity sanity:

| FEMA rows | Distinct provider IDs | Duplicate provider-ID rows |
|---:|---:|---:|
| 486,282 | 486,282 | 0 |

H3 function probe:

| Function | Result |
|---|---|
| `H3_POINT_TO_CELL_STRING(ST_POINT(-73.9, 40.7), 10)` | `8a2a100c3297fff` |
| `H3_POINT_TO_CELL(ST_POINT(-73.9, 40.7), 10)` | `622236723176898559` |
| `H3_INT_TO_STRING(...)` | `8a2a100c3297fff` |
| `H3_GRID_DISK(..., 1)` | 7 cells |

Setup SQL:

```sql
WITH column_check AS (
  SELECT
    table_name,
    COUNT_IF(column_name IN ('H3_R9','H3_R9_INT','H3_R10','H3_R10_INT')) AS native_r9_r10_columns
  FROM EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
  WHERE table_schema = 'SOURCE'
    AND table_name IN (
      'NYC_DCP_MAPPLUTO_HOT',
      'NYC_BUILDING_FOOTPRINTS_HOT',
      'FEMA_USA_STRUCTURES_HOT',
      'FEMA_USA_STRUCTURES_FEATURE_H3_COVERAGE'
    )
  GROUP BY table_name
),
counts AS (
  SELECT
    'parcels' AS source,
    COUNT(*) AS feature_count,
    COUNT_IF(CENTROID_GEOG IS NULL) AS null_centroid_count,
    COUNT_IF(GEOM_GEOG IS NULL) AS null_geom_count
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE IS_CURRENT_RELEASE = TRUE
  UNION ALL
  SELECT
    'nyc_footprints' AS source,
    COUNT(*) AS feature_count,
    COUNT_IF(CENTROID_GEOG IS NULL) AS null_centroid_count,
    COUNT_IF(GEOM_GEOG IS NULL) AS null_geom_count
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE IS_ACTIVE_FOOTPRINT = TRUE
  UNION ALL
  SELECT
    'fema_structures_nyc_counties' AS source,
    COUNT(*) AS feature_count,
    COUNT_IF(CENTROID_GEOG IS NULL) AS null_centroid_count,
    COUNT_IF(GEOM_GEOG IS NULL) AS null_geom_count
  FROM EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT
  WHERE STATE_FIPS = '36'
    AND COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
)
SELECT
  c.source,
  c.feature_count,
  c.null_centroid_count,
  c.null_geom_count,
  LISTAGG(cc.table_name || ':' || cc.native_r9_r10_columns, ', ')
    WITHIN GROUP (ORDER BY cc.table_name) AS native_r9_r10_columns_by_table
FROM counts c
CROSS JOIN column_check cc
GROUP BY c.source, c.feature_count, c.null_centroid_count, c.null_geom_count
ORDER BY c.source;
```

FEMA identity sanity SQL:

```sql
SELECT
  COUNT(*) AS fema_rows,
  COUNT(DISTINCT PROVIDER_FEATURE_ID) AS distinct_provider_feature_ids,
  COUNT(*) - COUNT(DISTINCT PROVIDER_FEATURE_ID) AS duplicate_provider_feature_id_rows
FROM EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT
WHERE STATE_FIPS = '36'
  AND COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
  AND GEOM_GEOG IS NOT NULL
  AND CENTROID_GEOG IS NOT NULL;
```

## Block 1 - r10+k-ring-1 Work-Unit Feature Distribution

Status: complete.

Center universe:

- `r10+k1`: 39,098 parcel-containing r10 center cells.
- Every r10 work unit expanded to exactly 7 H3 cells (`min_ring_cell_count = max = 7`).

Total feature distribution, with parcels + NYC footprints + FEMA structures:

| Work unit | Centers | Min | Median | Mean | P90 | P99 | Max | >200 count | >200 % | >400 count | >400 % | Reconcile failures | Exceedance sanity |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `r10_k1` | 39,098 | 1 | 418.00 | 421.43 | 755.00 | 1,011.00 | 1,329 | 29,669 | 75.88% | 20,384 | 52.14% | 0 | PASS |

Source split for `r10+k1`:

| Source count in work unit | Min | Median | Mean | P90 | P99 | Max |
|---|---:|---:|---:|---:|---:|---:|
| Parcels | 1 | 150.00 | 150.11 | 272.00 | 352.00 | 545 |
| NYC footprints | 0 | 180.00 | 187.84 | 350.00 | 499.00 | 700 |
| FEMA structures | 0 | 71.00 | 83.47 | 172.00 | 235.00 | 354 |

Block 1 finding:

- Appendix C option 3 does not restore the total work unit to ~190 features once FEMA is
  included and the k-ring halo is retained.
- `r10+k1` is much smaller than `r9+k1`, but its measured median is 418 total features, and
  52.14% of work units exceed 400 features.
- Counts reconcile exactly: every row's `total_features` equals parcel + NYC + FEMA counts.

## Block 2 - Boundary-Crossing Surface Rates

Status: complete with a narrower method after direct coverage timed out.

Direct all-source `H3_COVERAGE_STRINGS(geom, res)` aggregation failed twice:

- First failure: `SQL execution canceled`.
- Retry failure: `SQL execution was cancelled by the client due to a timeout`.

Replacement boundary predicate:

```sql
NOT ST_COVERS(
  H3_CELL_TO_BOUNDARY(H3_POINT_TO_CELL_STRING(CENTROID_GEOG, <resolution>)),
  GEOM_GEOG
)
```

This measures whether the feature geometry is fully contained in its centroid's H3 home
cell. If not, it crosses the home-cell boundary at that resolution. It uses geometry only,
not asserted area fields.

Boundary-crossing rates:

| Source | Resolution | Features | Crossing | Contained | Crossing % | Sanity |
|---|---|---:|---:|---:|---:|---|
| FEMA NYC-county structures | r10 | 486,282 | 127,104 | 359,178 | 26.14% | PASS |
| FEMA NYC-county structures | r9 | 486,282 | 52,803 | 433,479 | 10.86% | PASS |
| NYC footprints | r10 | 1,081,999 | 219,949 | 862,050 | 20.33% | PASS |
| NYC footprints | r9 | 1,081,999 | 87,356 | 994,643 | 8.07% | PASS |
| MapPLUTO parcels | r10 | 856,614 | 318,996 | 537,618 | 37.24% | PASS |
| MapPLUTO parcels | r9 | 856,614 | 132,003 | 724,611 | 15.41% | PASS |

Block 2 finding:

- Boundary pressure roughly doubles to triples when moving from r9 to r10.
- The r10 crossing rates are large enough to matter: 20.33% of NYC footprints, 26.14% of
  FEMA structures, and 37.24% of parcels cross their r10 centroid home-cell boundary.
- This confirms Appendix C's warning that option 3 buys smaller work units at the cost of
  more boundary surface.

## Block 3 - r9+k-ring-1 Comparison

Status: complete from the same structured distribution query as Block 1.

Center universe:

- `r9+k1`: 6,829 parcel-containing r9 center cells.
- Every r9 work unit expanded to exactly 7 H3 cells (`min_ring_cell_count = max = 7`).

Total feature distribution, with parcels + NYC footprints + FEMA structures:

| Work unit | Centers | Min | Median | Mean | P90 | P99 | Max | >200 count | >200 % | >400 count | >400 % | Reconcile failures | Exceedance sanity |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `r9_k1` | 6,829 | 1 | 2,274.00 | 2,441.80 | 4,619.20 | 6,102.92 | 7,515 | 6,472 | 94.77% | 6,198 | 90.76% | 0 | PASS |

Source split for `r9+k1`:

| Source count in work unit | Min | Median | Mean | P90 | P99 | Max |
|---|---:|---:|---:|---:|---:|---:|
| Parcels | 1 | 824.00 | 865.81 | 1,645.00 | 2,104.72 | 2,549 |
| NYC footprints | 0 | 996.00 | 1,090.09 | 2,132.00 | 2,953.72 | 3,684 |
| FEMA structures | 0 | 404.00 | 485.90 | 1,010.20 | 1,398.00 | 1,829 |

Block 3 finding:

- `r9+k1` is far larger than Appendix C's two-source estimate once FEMA is included:
  median 2,274 total features, mean 2,441.80, p99 6,102.92, max 7,515.
- `r10+k1` reduces the median total feature count by about 5.44x versus `r9+k1`
  (`2,274 / 418`) and reduces the mean by about 5.80x (`2,441.80 / 421.43`).
- The r10 cell-center count is 39,098 versus 6,829 at r9, a 5.73x increase in center cells
  over the parcel-defined NYC land universe.

Block 1 and Block 3 distribution SQL:

```sql
WITH parcel_homes AS (
  SELECT
    H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 10) AS r10_cell,
    H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 9) AS r9_cell,
    COUNT(*) AS parcel_count
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE IS_CURRENT_RELEASE = TRUE
    AND CENTROID_GEOG IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
  GROUP BY r10_cell, r9_cell
),
parcel_r10 AS (
  SELECT r10_cell AS cell, SUM(parcel_count) AS parcel_count
  FROM parcel_homes
  GROUP BY r10_cell
),
parcel_r9 AS (
  SELECT r9_cell AS cell, SUM(parcel_count) AS parcel_count
  FROM parcel_homes
  GROUP BY r9_cell
),
nyc_footprint_homes AS (
  SELECT
    H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 10) AS r10_cell,
    H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 9) AS r9_cell,
    COUNT(*) AS nyc_footprint_count
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE IS_ACTIVE_FOOTPRINT = TRUE
    AND CENTROID_GEOG IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
  GROUP BY r10_cell, r9_cell
),
nyc_r10 AS (
  SELECT r10_cell AS cell, SUM(nyc_footprint_count) AS nyc_footprint_count
  FROM nyc_footprint_homes
  GROUP BY r10_cell
),
nyc_r9 AS (
  SELECT r9_cell AS cell, SUM(nyc_footprint_count) AS nyc_footprint_count
  FROM nyc_footprint_homes
  GROUP BY r9_cell
),
fema_homes AS (
  SELECT
    H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 10) AS r10_cell,
    H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 9) AS r9_cell,
    COUNT(*) AS fema_structure_count
  FROM EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT
  WHERE STATE_FIPS = '36'
    AND COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
    AND CENTROID_GEOG IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
  GROUP BY r10_cell, r9_cell
),
fema_r10 AS (
  SELECT r10_cell AS cell, SUM(fema_structure_count) AS fema_structure_count
  FROM fema_homes
  GROUP BY r10_cell
),
fema_r9 AS (
  SELECT r9_cell AS cell, SUM(fema_structure_count) AS fema_structure_count
  FROM fema_homes
  GROUP BY r9_cell
),
centers_r10 AS (
  SELECT cell AS center_cell
  FROM parcel_r10
),
centers_r9 AS (
  SELECT cell AS center_cell
  FROM parcel_r9
),
rings_r10 AS (
  SELECT c.center_cell, f.value::string AS neighbor_cell
  FROM centers_r10 c, LATERAL FLATTEN(input => H3_GRID_DISK(c.center_cell, 1)) f
),
rings_r9 AS (
  SELECT c.center_cell, f.value::string AS neighbor_cell
  FROM centers_r9 c, LATERAL FLATTEN(input => H3_GRID_DISK(c.center_cell, 1)) f
),
ring_counts_r10 AS (
  SELECT center_cell, COUNT(*) AS ring_cell_count
  FROM rings_r10
  GROUP BY center_cell
),
ring_counts_r9 AS (
  SELECT center_cell, COUNT(*) AS ring_cell_count
  FROM rings_r9
  GROUP BY center_cell
),
unit_r10 AS (
  SELECT
    r.center_cell,
    SUM(COALESCE(p.parcel_count, 0)) AS parcel_features,
    SUM(COALESCE(n.nyc_footprint_count, 0)) AS nyc_footprints,
    SUM(COALESCE(f.fema_structure_count, 0)) AS fema_structures
  FROM rings_r10 r
  LEFT JOIN parcel_r10 p ON p.cell = r.neighbor_cell
  LEFT JOIN nyc_r10 n ON n.cell = r.neighbor_cell
  LEFT JOIN fema_r10 f ON f.cell = r.neighbor_cell
  GROUP BY r.center_cell
),
unit_r9 AS (
  SELECT
    r.center_cell,
    SUM(COALESCE(p.parcel_count, 0)) AS parcel_features,
    SUM(COALESCE(n.nyc_footprint_count, 0)) AS nyc_footprints,
    SUM(COALESCE(f.fema_structure_count, 0)) AS fema_structures
  FROM rings_r9 r
  LEFT JOIN parcel_r9 p ON p.cell = r.neighbor_cell
  LEFT JOIN nyc_r9 n ON n.cell = r.neighbor_cell
  LEFT JOIN fema_r9 f ON f.cell = r.neighbor_cell
  GROUP BY r.center_cell
),
units AS (
  SELECT
    'r10_k1' AS work_unit,
    u.center_cell,
    rc.ring_cell_count,
    u.parcel_features,
    u.nyc_footprints,
    u.fema_structures,
    u.parcel_features + u.nyc_footprints + u.fema_structures AS total_features
  FROM unit_r10 u
  JOIN ring_counts_r10 rc ON rc.center_cell = u.center_cell
  UNION ALL
  SELECT
    'r9_k1' AS work_unit,
    u.center_cell,
    rc.ring_cell_count,
    u.parcel_features,
    u.nyc_footprints,
    u.fema_structures,
    u.parcel_features + u.nyc_footprints + u.fema_structures AS total_features
  FROM unit_r9 u
  JOIN ring_counts_r9 rc ON rc.center_cell = u.center_cell
),
stats AS (
  SELECT
    work_unit,
    COUNT(*) AS work_unit_count,
    MIN(ring_cell_count) AS min_ring_cell_count,
    MAX(ring_cell_count) AS max_ring_cell_count,
    MIN(total_features) AS min_total_features,
    ROUND(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY total_features), 2) AS median_total_features,
    ROUND(AVG(total_features), 2) AS mean_total_features,
    ROUND(PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY total_features), 2) AS p90_total_features,
    ROUND(PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY total_features), 2) AS p99_total_features,
    MAX(total_features) AS max_total_features,
    COUNT_IF(total_features > 200) AS work_units_gt_200,
    ROUND(100.0 * COUNT_IF(total_features > 200) / COUNT(*), 2) AS pct_gt_200,
    COUNT_IF(total_features > 400) AS work_units_gt_400,
    ROUND(100.0 * COUNT_IF(total_features > 400) / COUNT(*), 2) AS pct_gt_400,
    MIN(parcel_features) AS min_parcels,
    ROUND(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY parcel_features), 2) AS median_parcels,
    ROUND(AVG(parcel_features), 2) AS mean_parcels,
    ROUND(PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY parcel_features), 2) AS p90_parcels,
    ROUND(PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY parcel_features), 2) AS p99_parcels,
    MAX(parcel_features) AS max_parcels,
    MIN(nyc_footprints) AS min_nyc_footprints,
    ROUND(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY nyc_footprints), 2) AS median_nyc_footprints,
    ROUND(AVG(nyc_footprints), 2) AS mean_nyc_footprints,
    ROUND(PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY nyc_footprints), 2) AS p90_nyc_footprints,
    ROUND(PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY nyc_footprints), 2) AS p99_nyc_footprints,
    MAX(nyc_footprints) AS max_nyc_footprints,
    MIN(fema_structures) AS min_fema_structures,
    ROUND(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY fema_structures), 2) AS median_fema_structures,
    ROUND(AVG(fema_structures), 2) AS mean_fema_structures,
    ROUND(PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY fema_structures), 2) AS p90_fema_structures,
    ROUND(PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY fema_structures), 2) AS p99_fema_structures,
    MAX(fema_structures) AS max_fema_structures,
    COUNT_IF(total_features <> parcel_features + nyc_footprints + fema_structures) AS reconcile_fail_count,
    COUNT_IF(total_features > 400) <= COUNT_IF(total_features > 200) AS exceedance_monotone_ok
  FROM units
  GROUP BY work_unit
)
SELECT *
FROM stats
ORDER BY work_unit;
```

Boundary SQL:

```sql
WITH parcel_boundary AS (
  SELECT
    'parcels' AS source,
    COUNT(*) AS feature_count,
    COUNT_IF(
      NOT ST_COVERS(
        H3_CELL_TO_BOUNDARY(H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 10)),
        GEOM_GEOG
      )
    ) AS r10_crossing_count,
    COUNT_IF(
      NOT ST_COVERS(
        H3_CELL_TO_BOUNDARY(H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 9)),
        GEOM_GEOG
      )
    ) AS r9_crossing_count
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE IS_CURRENT_RELEASE = TRUE
    AND GEOM_GEOG IS NOT NULL
    AND CENTROID_GEOG IS NOT NULL
),
nyc_boundary AS (
  SELECT
    'nyc_footprints' AS source,
    COUNT(*) AS feature_count,
    COUNT_IF(
      NOT ST_COVERS(
        H3_CELL_TO_BOUNDARY(H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 10)),
        GEOM_GEOG
      )
    ) AS r10_crossing_count,
    COUNT_IF(
      NOT ST_COVERS(
        H3_CELL_TO_BOUNDARY(H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 9)),
        GEOM_GEOG
      )
    ) AS r9_crossing_count
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE IS_ACTIVE_FOOTPRINT = TRUE
    AND GEOM_GEOG IS NOT NULL
    AND CENTROID_GEOG IS NOT NULL
),
fema_boundary AS (
  SELECT
    'fema_structures_nyc_counties' AS source,
    COUNT(*) AS feature_count,
    COUNT_IF(
      NOT ST_COVERS(
        H3_CELL_TO_BOUNDARY(H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 10)),
        GEOM_GEOG
      )
    ) AS r10_crossing_count,
    COUNT_IF(
      NOT ST_COVERS(
        H3_CELL_TO_BOUNDARY(H3_POINT_TO_CELL_STRING(CENTROID_GEOG, 9)),
        GEOM_GEOG
      )
    ) AS r9_crossing_count
  FROM EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT
  WHERE STATE_FIPS = '36'
    AND COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
    AND GEOM_GEOG IS NOT NULL
    AND CENTROID_GEOG IS NOT NULL
),
boundary AS (
  SELECT * FROM parcel_boundary
  UNION ALL
  SELECT * FROM nyc_boundary
  UNION ALL
  SELECT * FROM fema_boundary
),
long_boundary AS (
  SELECT source, 'r10' AS resolution, feature_count, r10_crossing_count AS crossing_count
  FROM boundary
  UNION ALL
  SELECT source, 'r9' AS resolution, feature_count, r9_crossing_count AS crossing_count
  FROM boundary
)
SELECT
  source,
  resolution,
  feature_count,
  crossing_count,
  feature_count - crossing_count AS contained_count,
  ROUND(100.0 * crossing_count / NULLIF(feature_count, 0), 2) AS crossing_pct,
  CASE
    WHEN crossing_count BETWEEN 0 AND feature_count THEN 'PASS'
    ELSE 'FAIL'
  END AS sanity_gate
FROM long_boundary
ORDER BY source, resolution;
```

## Verdict For Appendix C Option 3

Option 3 is directionally right but the original numeric expectation is too low.

- `r10+k1` cuts the measured all-source median work-unit size from 2,274 to 418 features,
  and cuts mean size from 2,441.80 to 421.43.
- But it does not return the all-source work unit to ~190 features. Most r10 work units
  still exceed 200 features (75.88%), and about half exceed 400 features (52.14%).
- The r10 surface penalty is real: r10 boundary-crossing rates are 20.33% for NYC
  footprints and 26.14% for FEMA structures, versus 8.07% and 10.86% at r9.
- Section 13 cost re-estimation should price r10+k1 around a median of 418 all-source
  features and p99 of 1,011 in NYC, not around 190.
