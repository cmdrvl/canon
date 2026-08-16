# bd-3un6 — Stratified Predicate-C and FEMA Cross-Source Measurements

Measurement agent: PinkGorge

Date: 2026-08-16

Scope:

- SQL-via-Loom only.
- No `src/` or canon implementation edits.
- All cited counts must come from returned structured result tables, not Loom prose.
- Predicate C means `ST_INTERSECTS(parcel, footprint) AND footprint_area_inside_parcel / footprint_area > 0.5`.

Sanity gates:

- For predicate C, `multi_count` must be `0` because two disjoint parcels cannot each contain more than half of the same footprint area.
- Per source and cell, `exactly_one_count + zero_count + multi_count == footprint_count`.
- Component measurements report connected components in the compatibility graph induced by predicate-C edges.

## Prior Verified Baseline

Appendix D already verified:

| Cell | Stratum | Parcels | NYC Footprints | Exactly One | Zero | Multi |
|---|---:|---:|---:|---:|---:|---:|
| `882a100d8bfffff` | Manhattan dense | 2,343 | 2,354 | 1,988 (84%) | 366 (16%) | 0 |
| `882a100f4dfffff` | Bronx lower-density | 300 | 291 | 291 (100%) | 0 (0%) | 0 |

## T1 — Stratified NYC Predicate-C Measurements

Status: complete for four additional strata.

Important denominator finding:

Appendix D's published dense Manhattan count (`1,988` exactly-one, `366` zero) is not
reproduced by the literal geometric denominator `ST_AREA(footprint.GEOM_GEOG)`. It is
reproduced by the source `NYC_BUILDING_FOOTPRINTS_HOT.SHAPE_AREA` denominator:

| Cell | Footprints | `ST_AREA(intersection) / ST_AREA(footprint)` exactly-one/zero/multi | `ST_AREA(intersection) / SHAPE_AREA` exactly-one/zero/multi | `ST_AREA(intersection) * 10.7639 / SHAPE_AREA` exactly-one/zero/multi |
|---|---:|---:|---:|---:|
| `882a100d8bfffff` | 2,354 | 2,332 / 22 / 0 | 1,988 / 366 / 0 | 1,390 / 18 / 946 |

The T1 table below uses the Appendix-D-compatible `SHAPE_AREA` denominator for the main
predicate-C columns. The literal `ST_AREA(footprint)` result is carried in the `ST_*`
columns for comparison.

Selected cells came from whole-H3-cell parcel counts with the requested dominant borough:

| Target | Dominant borough | H3 cell | H3 int | Parcels | Dominant-borough parcels | NYC footprints |
|---|---|---|---:|---:|---:|---:|
| `MN_41` | MN | `882a1008c7fffff` | 613229523005079551 | 41 | 41 | 45 |
| `QN_1500` | QN | `882a103b6bfffff` | 613229536598818815 | 1,502 | 1,502 | 2,007 |
| `QN_700` | QN | `882a100e25fffff` | 613229524445822975 | 701 | 701 | 1,049 |
| `SI_LOW` | SI | `882a106019fffff` | 613229546444947455 | 101 | 101 | 256 |

Predicate-C result using Appendix-D-compatible `SHAPE_AREA` denominator:

| Cell | Borough | Parcels | Footprints | Exactly one | Zero | Multi | Components | Mean size | Max size | Size histogram | Literal-ST exactly/zero/multi | Sanity |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---|
| `882a1008c7fffff` | MN | 41 | 45 | 41 (91.11%) | 4 (8.89%) | 0 (0.00%) | 45 | 1.911 | 4 | `1:7, 2:36, 3:1, 4:1` | 42 / 3 / 0 | PASS |
| `882a103b6bfffff` | QN | 1,502 | 2,007 | 1,753 (87.34%) | 254 (12.66%) | 0 (0.00%) | 1,756 | 1.998 | 4 | `1:445, 2:875, 3:430, 4:6` | 1,992 / 15 / 0 | PASS |
| `882a100e25fffff` | QN | 701 | 1,049 | 927 (88.37%) | 122 (11.63%) | 0 (0.00%) | 823 | 2.126 | 5 | `1:201, 2:325, 3:290, 4:6, 5:1` | 1,036 / 13 / 0 | PASS |
| `882a106019fffff` | SI | 101 | 256 | 154 (60.16%) | 102 (39.84%) | 0 (0.00%) | 203 | 1.759 | 71 | `1:162, 2:37, 5:1, 6:1, 39:1, 71:1` | 204 / 52 / 0 | PASS |

T1 finding:

- The predicate still has zero multi-matches in all four new strata.
- Three cells decompose into tiny components: max size 4, 4, and 5.
- The Staten Island low-density cell is an exception to "tiny": it is still a forest, but has
  two large parcel-star components of size 39 and 71. Exact compilation survives as a forest
  but not as a universal 2-5-variable component claim.
- The literal `ST_AREA(footprint)` denominator is cleaner in every measured cell, but it does
  not reproduce Appendix D's 16% dense-Manhattan no-majority population.

Cell-selection SQL:

```sql
WITH targets AS (
  SELECT *
  FROM VALUES
    ('QN_1500','QN',1500),
    ('QN_700','QN',700),
    ('MN_41','MN',41),
    ('SI_LOW','SI',100)
    AS t(target_name, dominant_borough, target_parcels)
),
cell_totals AS (
  SELECT
    H3_R8 AS h3_r8_int,
    H3_INT_TO_STRING(H3_R8) AS h3_cell,
    COUNT(*) AS total_parcels
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE IS_CURRENT_RELEASE = TRUE
    AND H3_R8 IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
  GROUP BY H3_R8
),
borough_counts AS (
  SELECT
    H3_R8 AS h3_r8_int,
    BOROUGH,
    COUNT(*) AS borough_parcels,
    ROW_NUMBER() OVER (PARTITION BY H3_R8 ORDER BY COUNT(*) DESC, BOROUGH) AS rn
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE IS_CURRENT_RELEASE = TRUE
    AND H3_R8 IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
  GROUP BY H3_R8, BOROUGH
),
footprints AS (
  SELECT H3_R8 AS h3_cell, COUNT(*) AS footprints
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE IS_ACTIVE_FOOTPRINT = TRUE
    AND H3_R8 IS NOT NULL
    AND GEOM_GEOG IS NOT NULL
  GROUP BY H3_R8
),
ranked AS (
  SELECT
    t.target_name,
    t.dominant_borough,
    t.target_parcels,
    ct.h3_cell,
    ct.h3_r8_int,
    ct.total_parcels,
    bc.borough_parcels AS dominant_borough_parcels,
    COALESCE(f.footprints, 0) AS footprints,
    ABS(ct.total_parcels - t.target_parcels) AS parcel_delta,
    ROW_NUMBER() OVER (
      PARTITION BY t.target_name
      ORDER BY ABS(ct.total_parcels - t.target_parcels), COALESCE(f.footprints, 0) DESC, ct.h3_cell
    ) AS rn
  FROM targets t
  JOIN cell_totals ct ON TRUE
  JOIN borough_counts bc
    ON bc.h3_r8_int = ct.h3_r8_int
   AND bc.rn = 1
   AND bc.BOROUGH = t.dominant_borough
  LEFT JOIN footprints f ON f.h3_cell = ct.h3_cell
  WHERE COALESCE(f.footprints, 0) > 0
)
SELECT
  target_name,
  dominant_borough,
  target_parcels,
  h3_cell,
  h3_r8_int,
  total_parcels,
  dominant_borough_parcels,
  footprints,
  parcel_delta
FROM ranked
WHERE rn <= 5
ORDER BY target_name, rn;
```

Denominator-check SQL:

```sql
WITH parcels AS (
  SELECT
    COALESCE(NULLIF(BBL, ''), 'parcel_row:' || TO_VARCHAR(SOURCE_ROW_NUMBER)) AS parcel_id,
    GEOM_GEOG AS geom
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE IS_CURRENT_RELEASE = TRUE
    AND H3_R8 = H3_STRING_TO_INT('882a100d8bfffff')
    AND GEOM_GEOG IS NOT NULL
),
footprints AS (
  SELECT
    COALESCE(TO_VARCHAR(OBJECTID), 'footprint_row:' || TO_VARCHAR(SOURCE_ROW_NUMBER)) AS footprint_id,
    GEOM_GEOG AS geom,
    NULLIF(ST_AREA(GEOM_GEOG), 0) AS st_area_m2,
    NULLIF(SHAPE_AREA, 0) AS shape_area_source
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE IS_ACTIVE_FOOTPRINT = TRUE
    AND H3_R8 = '882a100d8bfffff'
    AND GEOM_GEOG IS NOT NULL
),
pair_fracs AS (
  SELECT
    f.footprint_id,
    p.parcel_id,
    ST_AREA(ST_INTERSECTION(f.geom, p.geom)) AS intersection_area_m2,
    ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.st_area_m2 AS frac_st_area,
    ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.shape_area_source AS frac_shape_raw,
    (ST_AREA(ST_INTERSECTION(f.geom, p.geom)) * 10.76391041671) / f.shape_area_source AS frac_shape_ft2
  FROM footprints f
  JOIN parcels p ON ST_INTERSECTS(f.geom, p.geom)
  WHERE f.st_area_m2 IS NOT NULL
    AND f.shape_area_source IS NOT NULL
),
counts AS (
  SELECT
    f.footprint_id,
    COUNT_IF(p.frac_st_area > 0.5) AS st_area_majority_count,
    COUNT_IF(p.frac_shape_raw > 0.5) AS shape_raw_majority_count,
    COUNT_IF(p.frac_shape_ft2 > 0.5) AS shape_ft2_majority_count
  FROM footprints f
  LEFT JOIN pair_fracs p ON p.footprint_id = f.footprint_id
  GROUP BY f.footprint_id
)
SELECT
  COUNT(*) AS footprint_count,
  COUNT_IF(st_area_majority_count = 1) AS st_area_exactly_one,
  COUNT_IF(st_area_majority_count = 0) AS st_area_zero,
  COUNT_IF(st_area_majority_count > 1) AS st_area_multi,
  COUNT_IF(shape_raw_majority_count = 1) AS shape_raw_exactly_one,
  COUNT_IF(shape_raw_majority_count = 0) AS shape_raw_zero,
  COUNT_IF(shape_raw_majority_count > 1) AS shape_raw_multi,
  COUNT_IF(shape_ft2_majority_count = 1) AS shape_ft2_exactly_one,
  COUNT_IF(shape_ft2_majority_count = 0) AS shape_ft2_zero,
  COUNT_IF(shape_ft2_majority_count > 1) AS shape_ft2_multi
FROM counts;
```

T1 measurement SQL:

```sql
WITH cells AS (
  SELECT *
  FROM VALUES
    ('QN_1500','QN','882a103b6bfffff',613229536598818815),
    ('QN_700','QN','882a100e25fffff',613229524445822975),
    ('MN_41','MN','882a1008c7fffff',613229523005079551),
    ('SI_LOW','SI','882a106019fffff',613229546444947455)
    AS c(cell_name, borough, h3_cell, h3_r8_int)
),
parcels AS (
  SELECT
    c.cell_name,
    c.borough,
    c.h3_cell,
    COALESCE(NULLIF(p.BBL, ''), 'parcel_row:' || TO_VARCHAR(p.SOURCE_ROW_NUMBER)) AS parcel_id,
    p.GEOM_GEOG AS geom
  FROM cells c
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p ON p.H3_R8 = c.h3_r8_int
  WHERE p.IS_CURRENT_RELEASE = TRUE
    AND p.GEOM_GEOG IS NOT NULL
),
footprints AS (
  SELECT
    c.cell_name,
    c.borough,
    c.h3_cell,
    COALESCE(TO_VARCHAR(f.OBJECTID), 'footprint_row:' || TO_VARCHAR(f.SOURCE_ROW_NUMBER)) AS footprint_id,
    f.GEOM_GEOG AS geom,
    NULLIF(f.SHAPE_AREA, 0) AS shape_area_source,
    NULLIF(ST_AREA(f.GEOM_GEOG), 0) AS st_area_geom
  FROM cells c
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f ON f.H3_R8 = c.h3_cell
  WHERE f.IS_ACTIVE_FOOTPRINT = TRUE
    AND f.GEOM_GEOG IS NOT NULL
),
pair_fracs AS (
  SELECT
    f.cell_name,
    f.footprint_id,
    p.parcel_id,
    ST_AREA(ST_INTERSECTION(f.geom, p.geom)) AS intersection_area,
    IFF(f.shape_area_source IS NOT NULL, ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.shape_area_source, NULL) AS frac_shape_raw,
    IFF(f.st_area_geom IS NOT NULL, ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.st_area_geom, NULL) AS frac_st_geom
  FROM footprints f
  JOIN parcels p
    ON p.cell_name = f.cell_name
   AND ST_INTERSECTS(f.geom, p.geom)
),
shape_edges AS (
  SELECT cell_name, footprint_id, parcel_id
  FROM pair_fracs
  WHERE frac_shape_raw > 0.5
),
st_edges AS (
  SELECT cell_name, footprint_id, parcel_id
  FROM pair_fracs
  WHERE frac_st_geom > 0.5
),
shape_footprint_counts AS (
  SELECT f.cell_name, f.footprint_id, COUNT(e.footprint_id) AS edge_count
  FROM footprints f
  LEFT JOIN shape_edges e
    ON e.cell_name = f.cell_name
   AND e.footprint_id = f.footprint_id
  GROUP BY f.cell_name, f.footprint_id
),
st_footprint_counts AS (
  SELECT f.cell_name, f.footprint_id, COUNT(e.footprint_id) AS edge_count
  FROM footprints f
  LEFT JOIN st_edges e
    ON e.cell_name = f.cell_name
   AND e.footprint_id = f.footprint_id
  GROUP BY f.cell_name, f.footprint_id
),
shape_split AS (
  SELECT
    cell_name,
    COUNT(*) AS footprint_count,
    COUNT_IF(edge_count = 1) AS exactly_one_count,
    COUNT_IF(edge_count = 0) AS zero_count,
    COUNT_IF(edge_count > 1) AS multi_count
  FROM shape_footprint_counts
  GROUP BY cell_name
),
st_split AS (
  SELECT
    cell_name,
    COUNT_IF(edge_count = 1) AS st_exactly_one_count,
    COUNT_IF(edge_count = 0) AS st_zero_count,
    COUNT_IF(edge_count > 1) AS st_multi_count
  FROM st_footprint_counts
  GROUP BY cell_name
),
parcel_counts AS (
  SELECT cell_name, ANY_VALUE(borough) AS borough, ANY_VALUE(h3_cell) AS h3_cell, COUNT(*) AS parcel_count
  FROM parcels
  GROUP BY cell_name
),
parcel_components AS (
  SELECT p.cell_name, 1 + COUNT(e.footprint_id) AS component_size
  FROM parcels p
  LEFT JOIN shape_edges e
    ON e.cell_name = p.cell_name
   AND e.parcel_id = p.parcel_id
  GROUP BY p.cell_name, p.parcel_id
),
zero_footprint_components AS (
  SELECT cell_name, 1 AS component_size
  FROM shape_footprint_counts
  WHERE edge_count = 0
),
components AS (
  SELECT * FROM parcel_components
  UNION ALL
  SELECT * FROM zero_footprint_components
),
component_summary AS (
  SELECT
    cell_name,
    COUNT(*) AS component_count,
    AVG(component_size) AS mean_component_size,
    MAX(component_size) AS max_component_size
  FROM components
  GROUP BY cell_name
),
component_hist AS (
  SELECT cell_name, component_size, COUNT(*) AS component_count
  FROM components
  GROUP BY cell_name, component_size
),
component_hist_string AS (
  SELECT
    cell_name,
    LISTAGG(TO_VARCHAR(component_size) || ':' || TO_VARCHAR(component_count), ', ')
      WITHIN GROUP (ORDER BY component_size) AS component_size_histogram
  FROM component_hist
  GROUP BY cell_name
)
SELECT
  pc.cell_name,
  pc.borough,
  pc.h3_cell,
  pc.parcel_count,
  ss.footprint_count,
  ss.exactly_one_count,
  ROUND(100.0 * ss.exactly_one_count / NULLIF(ss.footprint_count, 0), 2) AS exactly_one_pct,
  ss.zero_count,
  ROUND(100.0 * ss.zero_count / NULLIF(ss.footprint_count, 0), 2) AS zero_pct,
  ss.multi_count,
  ROUND(100.0 * ss.multi_count / NULLIF(ss.footprint_count, 0), 2) AS multi_pct,
  cs.component_count,
  ROUND(cs.mean_component_size, 3) AS mean_component_size,
  cs.max_component_size,
  ch.component_size_histogram,
  st.st_exactly_one_count,
  st.st_zero_count,
  st.st_multi_count,
  CASE
    WHEN ss.exactly_one_count + ss.zero_count + ss.multi_count = ss.footprint_count
     AND ss.multi_count = 0
    THEN 'PASS'
    ELSE 'FAIL'
  END AS sanity_gate
FROM parcel_counts pc
JOIN shape_split ss ON ss.cell_name = pc.cell_name
JOIN st_split st ON st.cell_name = pc.cell_name
JOIN component_summary cs ON cs.cell_name = pc.cell_name
JOIN component_hist_string ch ON ch.cell_name = pc.cell_name
ORDER BY pc.cell_name;
```

## T2 — FEMA Cross-Source and Merged Graph Measurements

Status: complete for the two Appendix-D cells plus the Queens 1,500-parcel stratum.

Main denominators:

- NYC footprint edge: `ST_AREA(ST_INTERSECTION(nyc.geom, parcel.geom)) / NYC.SHAPE_AREA > 0.5`
  to stay Appendix-D-compatible.
- FEMA structure edge: `ST_AREA(ST_INTERSECTION(fema.geom, parcel.geom)) / ST_AREA(FEMA.GEOM_GEOG) > 0.5`.
- FEMA `SQMETERS` is measured only as a comparison because it produced multi-matches in one
  cell and therefore is not the safe majority denominator.

FEMA candidate counts used the exploded coverage table
`FEMA_USA_STRUCTURES_FEATURE_H3_COVERAGE` at `H3_RESOLUTION = 8`.

| Cell | Parcels | NYC footprints | FEMA by coverage | FEMA by HOT `H3_R8` |
|---|---:|---:|---:|---:|
| `882a100f4dfffff` BX | 300 | 291 | 115 | 116 |
| `882a100d8bfffff` MN dense | 2,343 | 2,354 | 240 | 241 |
| `882a103b6bfffff` QN | 1,502 | 2,007 | 1,108 | 1,108 |

Three-layer result:

| Cell | Parcels | NYC exactly/zero/multi | FEMA exactly/zero/multi | FEMA `SQMETERS` exactly/zero/multi | Merged components | Merged mean | Merged max | Merged histogram | Sanity |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| `882a100f4dfffff` BX | 300 | 274 / 17 / 0 | 76 / 39 / 0 | 75 / 40 / 0 | 356 | 1.983 | 19 | `1:122, 2:174, 3:36, 4:14, 5:4, 6:3, 7:1, 8:1, 19:1` | PASS |
| `882a100d8bfffff` MN dense | 2,343 | 1,988 / 366 / 0 | 88 / 152 / 0 | 88 / 152 / 0 | 2,861 | 1.726 | 6 | `1:913, 2:1845, 3:83, 4:16, 5:3, 6:1` | PASS |
| `882a103b6bfffff` QN | 1,502 | 1,753 / 254 / 0 | 1,078 / 30 / 0 | 1,067 / 39 / 2 | 1,786 | 2.585 | 6 | `1:448, 2:299, 3:666, 4:293, 5:79, 6:1` | PASS |

T2 finding:

- Adding FEMA as a second footprint source does not re-percolate the graph under the geometric
  FEMA denominator. The merged graph remains a forest in all three cells.
- FEMA coverage is sparse in dense Manhattan: only 88 of 240 FEMA structures (36.67%) get a
  majority parcel, with 152 zero-majority. It is much stronger in the Queens cell: 1,078 of
  1,108 (97.29%) get a majority parcel.
- Bronx has a larger merged component (`max = 19`) than the dense Manhattan and Queens cells
  (`max = 6`), but this is still a parcel-star component, not tile percolation.
- `SQMETERS` is not safe as a drop-in denominator: in Queens it gives 2 FEMA multi-matches,
  while the geometric denominator gives 0. This is a source-field unit/semantics warning.

FEMA candidate count SQL:

```sql
WITH cells AS (
  SELECT 'MN_DENSE' AS cell_name, '882a100d8bfffff' AS h3_cell, H3_STRING_TO_INT('882a100d8bfffff') AS h3_r8_int
  UNION ALL
  SELECT 'BX_BASE', '882a100f4dfffff', H3_STRING_TO_INT('882a100f4dfffff')
  UNION ALL
  SELECT 'QN_1500', '882a103b6bfffff', H3_STRING_TO_INT('882a103b6bfffff')
),
parcels AS (
  SELECT c.cell_name, COUNT(*) AS parcel_count
  FROM cells c
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p ON p.H3_R8 = c.h3_r8_int
  WHERE p.IS_CURRENT_RELEASE = TRUE
    AND p.GEOM_GEOG IS NOT NULL
  GROUP BY c.cell_name
),
nyc AS (
  SELECT c.cell_name, COUNT(*) AS nyc_footprint_count
  FROM cells c
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f ON f.H3_R8 = c.h3_cell
  WHERE f.IS_ACTIVE_FOOTPRINT = TRUE
    AND f.GEOM_GEOG IS NOT NULL
  GROUP BY c.cell_name
),
fema_cov AS (
  SELECT c.cell_name, COUNT(DISTINCT cov.PROVIDER_FEATURE_ID) AS fema_coverage_feature_count
  FROM cells c
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_FEATURE_H3_COVERAGE cov
    ON cov.H3_RESOLUTION = 8
   AND cov.H3_CELL = c.h3_cell
  GROUP BY c.cell_name
),
fema_hot AS (
  SELECT c.cell_name, COUNT(DISTINCT f.PROVIDER_FEATURE_ID) AS fema_hot_h3_feature_count
  FROM cells c
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f ON f.H3_R8 = c.h3_cell
  WHERE f.GEOM_GEOG IS NOT NULL
  GROUP BY c.cell_name
)
SELECT
  c.cell_name,
  c.h3_cell,
  COALESCE(p.parcel_count,0) AS parcel_count,
  COALESCE(n.nyc_footprint_count,0) AS nyc_footprint_count,
  COALESCE(fc.fema_coverage_feature_count,0) AS fema_coverage_feature_count,
  COALESCE(fh.fema_hot_h3_feature_count,0) AS fema_hot_h3_feature_count
FROM cells c
LEFT JOIN parcels p ON p.cell_name = c.cell_name
LEFT JOIN nyc n ON n.cell_name = c.cell_name
LEFT JOIN fema_cov fc ON fc.cell_name = c.cell_name
LEFT JOIN fema_hot fh ON fh.cell_name = c.cell_name
ORDER BY c.cell_name;
```

T2 measurement SQL:

```sql
WITH cells AS (
  SELECT 'MN_DENSE' AS cell_name, '882a100d8bfffff' AS h3_cell, H3_STRING_TO_INT('882a100d8bfffff') AS h3_r8_int
  UNION ALL
  SELECT 'BX_BASE', '882a100f4dfffff', H3_STRING_TO_INT('882a100f4dfffff')
  UNION ALL
  SELECT 'QN_1500', '882a103b6bfffff', H3_STRING_TO_INT('882a103b6bfffff')
),
parcels AS (
  SELECT
    c.cell_name,
    c.h3_cell,
    COALESCE(NULLIF(p.BBL, ''), 'parcel_row:' || TO_VARCHAR(p.SOURCE_ROW_NUMBER)) AS parcel_id,
    p.GEOM_GEOG AS geom
  FROM cells c
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p ON p.H3_R8 = c.h3_r8_int
  WHERE p.IS_CURRENT_RELEASE = TRUE
    AND p.GEOM_GEOG IS NOT NULL
),
nyc_footprints AS (
  SELECT
    c.cell_name,
    COALESCE(TO_VARCHAR(f.OBJECTID), 'nyc_row:' || TO_VARCHAR(f.SOURCE_ROW_NUMBER)) AS nyc_id,
    f.GEOM_GEOG AS geom,
    NULLIF(f.SHAPE_AREA, 0) AS shape_area_source
  FROM cells c
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f ON f.H3_R8 = c.h3_cell
  WHERE f.IS_ACTIVE_FOOTPRINT = TRUE
    AND f.GEOM_GEOG IS NOT NULL
),
fema_structures AS (
  SELECT
    c.cell_name,
    f.PROVIDER_FEATURE_ID AS fema_id,
    f.GEOM_GEOG AS geom,
    NULLIF(ST_AREA(f.GEOM_GEOG), 0) AS st_area_geom,
    NULLIF(f.SQMETERS, 0) AS sqmeters
  FROM cells c
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_FEATURE_H3_COVERAGE cov
    ON cov.H3_RESOLUTION = 8
   AND cov.H3_CELL = c.h3_cell
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f ON f.PROVIDER_FEATURE_ID = cov.PROVIDER_FEATURE_ID
  WHERE f.GEOM_GEOG IS NOT NULL
),
nyc_pair_fracs AS (
  SELECT
    n.cell_name,
    n.nyc_id,
    p.parcel_id,
    ST_AREA(ST_INTERSECTION(n.geom, p.geom)) / n.shape_area_source AS frac_shape_raw
  FROM nyc_footprints n
  JOIN parcels p
    ON p.cell_name = n.cell_name
   AND ST_INTERSECTS(n.geom, p.geom)
  WHERE n.shape_area_source IS NOT NULL
),
nyc_edges AS (
  SELECT cell_name, nyc_id, parcel_id
  FROM nyc_pair_fracs
  WHERE frac_shape_raw > 0.5
),
fema_pair_fracs AS (
  SELECT
    f.cell_name,
    f.fema_id,
    p.parcel_id,
    IFF(f.st_area_geom IS NOT NULL, ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.st_area_geom, NULL) AS frac_st_geom,
    IFF(f.sqmeters IS NOT NULL, ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.sqmeters, NULL) AS frac_sqmeters
  FROM fema_structures f
  JOIN parcels p
    ON p.cell_name = f.cell_name
   AND ST_INTERSECTS(f.geom, p.geom)
),
fema_st_edges AS (
  SELECT cell_name, fema_id, parcel_id
  FROM fema_pair_fracs
  WHERE frac_st_geom > 0.5
),
fema_sq_edges AS (
  SELECT cell_name, fema_id, parcel_id
  FROM fema_pair_fracs
  WHERE frac_sqmeters > 0.5
),
nyc_counts AS (
  SELECT n.cell_name, n.nyc_id, COUNT(e.nyc_id) AS edge_count
  FROM nyc_footprints n
  LEFT JOIN nyc_edges e
    ON e.cell_name = n.cell_name
   AND e.nyc_id = n.nyc_id
  GROUP BY n.cell_name, n.nyc_id
),
fema_st_counts AS (
  SELECT f.cell_name, f.fema_id, COUNT(e.fema_id) AS edge_count
  FROM fema_structures f
  LEFT JOIN fema_st_edges e
    ON e.cell_name = f.cell_name
   AND e.fema_id = f.fema_id
  GROUP BY f.cell_name, f.fema_id
),
fema_sq_counts AS (
  SELECT f.cell_name, f.fema_id, COUNT(e.fema_id) AS edge_count
  FROM fema_structures f
  LEFT JOIN fema_sq_edges e
    ON e.cell_name = f.cell_name
   AND e.fema_id = f.fema_id
  GROUP BY f.cell_name, f.fema_id
),
parcel_counts AS (
  SELECT cell_name, ANY_VALUE(h3_cell) AS h3_cell, COUNT(*) AS parcel_count
  FROM parcels
  GROUP BY cell_name
),
nyc_split AS (
  SELECT
    cell_name,
    COUNT(*) AS nyc_footprint_count,
    COUNT_IF(edge_count = 1) AS nyc_exactly_one_count,
    COUNT_IF(edge_count = 0) AS nyc_zero_count,
    COUNT_IF(edge_count > 1) AS nyc_multi_count
  FROM nyc_counts
  GROUP BY cell_name
),
fema_st_split AS (
  SELECT
    cell_name,
    COUNT(*) AS fema_structure_count,
    COUNT_IF(edge_count = 1) AS fema_exactly_one_count,
    COUNT_IF(edge_count = 0) AS fema_zero_count,
    COUNT_IF(edge_count > 1) AS fema_multi_count
  FROM fema_st_counts
  GROUP BY cell_name
),
fema_sq_split AS (
  SELECT
    cell_name,
    COUNT_IF(edge_count = 1) AS fema_sq_exactly_one_count,
    COUNT_IF(edge_count = 0) AS fema_sq_zero_count,
    COUNT_IF(edge_count > 1) AS fema_sq_multi_count
  FROM fema_sq_counts
  GROUP BY cell_name
),
parcel_components AS (
  SELECT p.cell_name, 1 + COUNT(DISTINCT n.nyc_id) + COUNT(DISTINCT f.fema_id) AS component_size
  FROM parcels p
  LEFT JOIN nyc_edges n
    ON n.cell_name = p.cell_name
   AND n.parcel_id = p.parcel_id
  LEFT JOIN fema_st_edges f
    ON f.cell_name = p.cell_name
   AND f.parcel_id = p.parcel_id
  GROUP BY p.cell_name, p.parcel_id
),
nyc_zero_components AS (
  SELECT cell_name, 1 AS component_size
  FROM nyc_counts
  WHERE edge_count = 0
),
fema_zero_components AS (
  SELECT cell_name, 1 AS component_size
  FROM fema_st_counts
  WHERE edge_count = 0
),
components AS (
  SELECT * FROM parcel_components
  UNION ALL
  SELECT * FROM nyc_zero_components
  UNION ALL
  SELECT * FROM fema_zero_components
),
component_summary AS (
  SELECT
    cell_name,
    COUNT(*) AS merged_component_count,
    AVG(component_size) AS merged_mean_component_size,
    MAX(component_size) AS merged_max_component_size
  FROM components
  GROUP BY cell_name
),
component_hist AS (
  SELECT cell_name, component_size, COUNT(*) AS component_count
  FROM components
  GROUP BY cell_name, component_size
),
component_hist_string AS (
  SELECT
    cell_name,
    LISTAGG(TO_VARCHAR(component_size) || ':' || TO_VARCHAR(component_count), ', ')
      WITHIN GROUP (ORDER BY component_size) AS merged_component_size_histogram
  FROM component_hist
  GROUP BY cell_name
)
SELECT
  pc.cell_name,
  pc.h3_cell,
  pc.parcel_count,
  ns.nyc_footprint_count,
  ns.nyc_exactly_one_count,
  ROUND(100.0 * ns.nyc_exactly_one_count / NULLIF(ns.nyc_footprint_count, 0), 2) AS nyc_exactly_one_pct,
  ns.nyc_zero_count,
  ROUND(100.0 * ns.nyc_zero_count / NULLIF(ns.nyc_footprint_count, 0), 2) AS nyc_zero_pct,
  ns.nyc_multi_count,
  fs.fema_structure_count,
  fs.fema_exactly_one_count,
  ROUND(100.0 * fs.fema_exactly_one_count / NULLIF(fs.fema_structure_count, 0), 2) AS fema_exactly_one_pct,
  fs.fema_zero_count,
  ROUND(100.0 * fs.fema_zero_count / NULLIF(fs.fema_structure_count, 0), 2) AS fema_zero_pct,
  fs.fema_multi_count,
  fq.fema_sq_exactly_one_count,
  fq.fema_sq_zero_count,
  fq.fema_sq_multi_count,
  cs.merged_component_count,
  ROUND(cs.merged_mean_component_size, 3) AS merged_mean_component_size,
  cs.merged_max_component_size,
  ch.merged_component_size_histogram,
  CASE
    WHEN ns.nyc_exactly_one_count + ns.nyc_zero_count + ns.nyc_multi_count = ns.nyc_footprint_count
     AND fs.fema_exactly_one_count + fs.fema_zero_count + fs.fema_multi_count = fs.fema_structure_count
     AND ns.nyc_multi_count = 0
     AND fs.fema_multi_count = 0
    THEN 'PASS'
    ELSE 'FAIL'
  END AS sanity_gate
FROM parcel_counts pc
JOIN nyc_split ns ON ns.cell_name = pc.cell_name
JOIN fema_st_split fs ON fs.cell_name = pc.cell_name
JOIN fema_sq_split fq ON fq.cell_name = pc.cell_name
JOIN component_summary cs ON cs.cell_name = pc.cell_name
JOIN component_hist_string ch ON ch.cell_name = pc.cell_name
ORDER BY pc.cell_name;
```

## T3 — Dense Manhattan No-Majority Characterization

Status: complete for dense Manhattan cell `882a100d8bfffff`.

Population characterized: the 366 NYC footprints with zero Appendix-D-compatible
`SHAPE_AREA` majority parcel in the dense Manhattan cell.

| No-majority footprints | 0 intersecting parcels | 1 intersecting parcel | Exactly 2 intersecting parcels | >2 intersecting parcels | Intersecting-parcel histogram |
|---:|---:|---:|---:|---:|---|
| 366 | 17 | 2 | 311 (84.97%) | 36 | `0:17, 1:2, 2:311, 3:29, 4:5, 6:1, 7:1` |

Top-two overlap fractions:

| Denominator | Avg top-1 | Avg top-2 | Median top-2 | P90 top-2 | Top-2 >= 80% | Top-2 >= 90% | Top-2 >= 99% |
|---|---:|---:|---:|---:|---:|---:|---:|
| `SHAPE_AREA` | 0.4526 | 0.5385 | 0.5737 | 0.5738 | 0 | 0 | 0 |
| `ST_AREA(footprint)` | 0.7888 | 0.9385 | 1.0000 | 1.0000 | 345 | 341 | 300 |

T3 finding:

- Under the Appendix-D-compatible denominator, 311 of 366 no-majority footprints (84.97%)
  are exactly two-parcel straddlers by intersection count.
- Under the literal geometric denominator, 344 of 366 would already have a >50% top parcel.
  So the 16% no-majority population is not purely a product population; most of it is created
  by the `SHAPE_AREA` denominator choice.
- The actual top-two geometry coverage is high: 341 of 366 have >=90% of their
  `ST_AREA(footprint)` in the top two parcels, and 300 have >=99%.

T3 SQL:

```sql
WITH parcels AS (
  SELECT
    COALESCE(NULLIF(BBL, ''), 'parcel_row:' || TO_VARCHAR(SOURCE_ROW_NUMBER)) AS parcel_id,
    GEOM_GEOG AS geom
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
  WHERE IS_CURRENT_RELEASE = TRUE
    AND H3_R8 = H3_STRING_TO_INT('882a100d8bfffff')
    AND GEOM_GEOG IS NOT NULL
),
footprints AS (
  SELECT
    COALESCE(TO_VARCHAR(OBJECTID), 'nyc_row:' || TO_VARCHAR(SOURCE_ROW_NUMBER)) AS footprint_id,
    GEOM_GEOG AS geom,
    NULLIF(SHAPE_AREA, 0) AS shape_area_source,
    NULLIF(ST_AREA(GEOM_GEOG), 0) AS st_area_geom
  FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT
  WHERE IS_ACTIVE_FOOTPRINT = TRUE
    AND H3_R8 = '882a100d8bfffff'
    AND GEOM_GEOG IS NOT NULL
),
pairs AS (
  SELECT
    f.footprint_id,
    p.parcel_id,
    ST_AREA(ST_INTERSECTION(f.geom, p.geom)) AS intersection_area,
    ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.shape_area_source AS shape_frac,
    ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.st_area_geom AS st_frac
  FROM footprints f
  JOIN parcels p ON ST_INTERSECTS(f.geom, p.geom)
  WHERE f.shape_area_source IS NOT NULL
    AND f.st_area_geom IS NOT NULL
),
ranked_pairs AS (
  SELECT
    *,
    ROW_NUMBER() OVER (PARTITION BY footprint_id ORDER BY intersection_area DESC, parcel_id) AS rn
  FROM pairs
),
per_footprint AS (
  SELECT
    f.footprint_id,
    COUNT(r.parcel_id) AS intersecting_parcel_count,
    COUNT_IF(r.shape_frac > 0.5) AS shape_majority_count,
    COUNT_IF(r.st_frac > 0.5) AS st_majority_count,
    COALESCE(SUM(IFF(r.rn = 1, r.intersection_area, 0)) / ANY_VALUE(f.shape_area_source), 0) AS top1_shape_frac,
    COALESCE(SUM(IFF(r.rn <= 2, r.intersection_area, 0)) / ANY_VALUE(f.shape_area_source), 0) AS top2_shape_frac,
    COALESCE(SUM(IFF(r.rn = 1, r.intersection_area, 0)) / ANY_VALUE(f.st_area_geom), 0) AS top1_st_frac,
    COALESCE(SUM(IFF(r.rn <= 2, r.intersection_area, 0)) / ANY_VALUE(f.st_area_geom), 0) AS top2_st_frac
  FROM footprints f
  LEFT JOIN ranked_pairs r ON r.footprint_id = f.footprint_id
  GROUP BY f.footprint_id
),
no_majority AS (
  SELECT *
  FROM per_footprint
  WHERE shape_majority_count = 0
),
hist AS (
  SELECT intersecting_parcel_count, COUNT(*) AS footprint_count
  FROM no_majority
  GROUP BY intersecting_parcel_count
),
hist_string AS (
  SELECT
    LISTAGG(TO_VARCHAR(intersecting_parcel_count) || ':' || TO_VARCHAR(footprint_count), ', ')
      WITHIN GROUP (ORDER BY intersecting_parcel_count) AS intersecting_parcel_histogram
  FROM hist
)
SELECT
  COUNT(*) AS no_majority_count,
  COUNT_IF(intersecting_parcel_count = 0) AS zero_intersections_count,
  COUNT_IF(intersecting_parcel_count = 1) AS one_intersection_count,
  COUNT_IF(intersecting_parcel_count = 2) AS exactly_two_intersections_count,
  ROUND(100.0 * COUNT_IF(intersecting_parcel_count = 2) / NULLIF(COUNT(*), 0), 2) AS exactly_two_intersections_pct,
  COUNT_IF(intersecting_parcel_count > 2) AS more_than_two_intersections_count,
  ROUND(AVG(top1_shape_frac), 4) AS avg_top1_shape_frac,
  ROUND(AVG(top2_shape_frac), 4) AS avg_top2_shape_frac,
  ROUND(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY top2_shape_frac), 4) AS median_top2_shape_frac,
  ROUND(PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY top2_shape_frac), 4) AS p90_top2_shape_frac,
  COUNT_IF(top2_shape_frac >= 0.8) AS top2_shape_ge_80_count,
  COUNT_IF(top2_shape_frac >= 0.9) AS top2_shape_ge_90_count,
  COUNT_IF(top2_shape_frac >= 0.99) AS top2_shape_ge_99_count,
  ROUND(AVG(top1_st_frac), 4) AS avg_top1_st_frac,
  ROUND(AVG(top2_st_frac), 4) AS avg_top2_st_frac,
  ROUND(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY top2_st_frac), 4) AS median_top2_st_frac,
  ROUND(PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY top2_st_frac), 4) AS p90_top2_st_frac,
  COUNT_IF(top1_st_frac > 0.5) AS would_resolve_under_literal_st_count,
  COUNT_IF(top2_st_frac >= 0.8) AS top2_st_ge_80_count,
  COUNT_IF(top2_st_frac >= 0.9) AS top2_st_ge_90_count,
  COUNT_IF(top2_st_frac >= 0.99) AS top2_st_ge_99_count,
  ANY_VALUE(h.intersecting_parcel_histogram) AS intersecting_parcel_histogram
FROM no_majority
CROSS JOIN hist_string h;
```

## T4 — Optional FEMA-vs-NYC Footprint Agreement

Status: complete for Queens cell `882a103b6bfffff`.

Question: do NYC and FEMA see the same number of structures per parcel under predicate C?

| Parcels | NYC matched | FEMA matched | Equal-count parcels | Equal nonzero | Both zero | NYC > FEMA | FEMA > NYC | Sum abs diff | Avg abs diff | Max abs diff | Diff >=2 | Diff >=5 | Diff >=10 |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1,502 | 1,753 | 1,078 | 829 (55.19%) | 665 | 164 | 634 | 39 | 755 | 0.503 | 3 | 81 | 0 | 0 |

Over-segmentation against MapPLUTO `NUMBLDGS`:

| Parcels with `NUMBLDGS` | NYC count > `CEIL(NUMBLDGS)` | FEMA count > `CEIL(NUMBLDGS)` | Max NYC over | Max FEMA over |
|---:|---:|---:|---:|---:|
| 1,502 | 5 | 11 | 1 | 1 |

Largest disagreements:

| BBL | Address | `NUMBLDGS` | NYC count | FEMA count | Abs diff | Direction | NYC over `NUMBLDGS` | FEMA over `NUMBLDGS` |
|---|---|---:|---:|---:|---:|---|---|---|
| `4120820023` | 130-50 146 STREET | 3 | 3 | 0 | 3 | NYC_GT_FEMA | false | false |
| `4120470066` | 123-24 145 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120470067` | 123-26 145 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120480093` | 123-27 145 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120490111` | 123-50 146 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120580013` | 128-07 140 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120580188` | 140-20 SUTTER AVENUE | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120600083` | 142-42 BASCOM AVENUE | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120600289` | 127-20 143 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120610001` | 143-02 ROCKAWAY BOULEVARD | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120610007` | 143-10 ROCKAWAY BOULEVARD | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120610009` | 143-16 ROCKAWAY BOULEVARD | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120610036` | 127-09 143 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120630137` | 145-16 ROCKAWAY BOULEVARD | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120640032` | 145-46 ROCKAWAY BOULEVARD | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120640069` | 145-13 SUTTER AVENUE | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120680117` | 128-20 145 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120690138` | 129-36 145 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120700094` | 129-05 145 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |
| `4120700109` | 128-01 145 STREET | 2 | 2 | 0 | 2 | NYC_GT_FEMA | false | false |

T4 finding:

- The two footprint sources agree by per-parcel structure count on 829 of 1,502 parcels
  (55.19%). The disagreement is asymmetric: NYC has more matched footprints on 634 parcels,
  FEMA has more on 39.
- Disagreements are shallow. No parcel differs by 5 or more; the max absolute difference is 3.
- Within-source exclusivity has little to catch at this cell's current predicate stage:
  over-`NUMBLDGS` cases exist but are small (5 NYC parcels, 11 FEMA parcels, max overage 1).
  The main disagreement is missing or differently segmented source coverage, not massive
  over-segmentation.

T4 summary SQL:

```sql
WITH cell AS (
  SELECT 'QN_1500' AS cell_name, '882a103b6bfffff' AS h3_cell, H3_STRING_TO_INT('882a103b6bfffff') AS h3_r8_int
),
parcels AS (
  SELECT
    c.cell_name,
    COALESCE(NULLIF(REGEXP_REPLACE(p.BBL, '\\.0$', ''), ''), 'parcel_row:' || TO_VARCHAR(p.SOURCE_ROW_NUMBER)) AS parcel_id,
    REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm,
    p.ADDRESS,
    p.NUMBLDGS,
    p.GEOM_GEOG AS geom
  FROM cell c
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p ON p.H3_R8 = c.h3_r8_int
  WHERE p.IS_CURRENT_RELEASE = TRUE
    AND p.GEOM_GEOG IS NOT NULL
),
nyc_footprints AS (
  SELECT
    c.cell_name,
    COALESCE(TO_VARCHAR(f.OBJECTID), 'nyc_row:' || TO_VARCHAR(f.SOURCE_ROW_NUMBER)) AS nyc_id,
    f.GEOM_GEOG AS geom,
    NULLIF(f.SHAPE_AREA, 0) AS shape_area_source
  FROM cell c
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f ON f.H3_R8 = c.h3_cell
  WHERE f.IS_ACTIVE_FOOTPRINT = TRUE
    AND f.GEOM_GEOG IS NOT NULL
),
fema_structures AS (
  SELECT
    c.cell_name,
    f.PROVIDER_FEATURE_ID AS fema_id,
    f.GEOM_GEOG AS geom,
    NULLIF(ST_AREA(f.GEOM_GEOG), 0) AS st_area_geom
  FROM cell c
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_FEATURE_H3_COVERAGE cov
    ON cov.H3_RESOLUTION = 8
   AND cov.H3_CELL = c.h3_cell
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f ON f.PROVIDER_FEATURE_ID = cov.PROVIDER_FEATURE_ID
  WHERE f.GEOM_GEOG IS NOT NULL
),
nyc_edges AS (
  SELECT n.cell_name, n.nyc_id, p.parcel_id
  FROM nyc_footprints n
  JOIN parcels p
    ON p.cell_name = n.cell_name
   AND ST_INTERSECTS(n.geom, p.geom)
  WHERE n.shape_area_source IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(n.geom, p.geom)) / n.shape_area_source > 0.5
),
fema_edges AS (
  SELECT f.cell_name, f.fema_id, p.parcel_id
  FROM fema_structures f
  JOIN parcels p
    ON p.cell_name = f.cell_name
   AND ST_INTERSECTS(f.geom, p.geom)
  WHERE f.st_area_geom IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.st_area_geom > 0.5
),
per_parcel AS (
  SELECT
    p.parcel_id,
    ANY_VALUE(p.bbl_norm) AS bbl_norm,
    ANY_VALUE(p.ADDRESS) AS address,
    ANY_VALUE(p.NUMBLDGS) AS num_bldgs,
    COUNT(DISTINCT n.nyc_id) AS nyc_count,
    COUNT(DISTINCT f.fema_id) AS fema_count
  FROM parcels p
  LEFT JOIN nyc_edges n ON n.parcel_id = p.parcel_id
  LEFT JOIN fema_edges f ON f.parcel_id = p.parcel_id
  GROUP BY p.parcel_id
)
SELECT
  COUNT(*) AS parcel_count,
  SUM(nyc_count) AS total_nyc_matched,
  SUM(fema_count) AS total_fema_matched,
  COUNT_IF(nyc_count = fema_count) AS equal_count_parcels,
  ROUND(100.0 * COUNT_IF(nyc_count = fema_count) / COUNT(*), 2) AS equal_count_pct,
  COUNT_IF(nyc_count = fema_count AND nyc_count > 0) AS equal_nonzero_parcels,
  COUNT_IF(nyc_count = 0 AND fema_count = 0) AS both_zero_parcels,
  COUNT_IF(nyc_count > fema_count) AS nyc_gt_fema_parcels,
  COUNT_IF(fema_count > nyc_count) AS fema_gt_nyc_parcels,
  SUM(ABS(nyc_count - fema_count)) AS sum_abs_count_diff,
  ROUND(AVG(ABS(nyc_count - fema_count)), 3) AS avg_abs_count_diff,
  MAX(ABS(nyc_count - fema_count)) AS max_abs_count_diff,
  COUNT_IF(ABS(nyc_count - fema_count) >= 2) AS abs_diff_ge_2_parcels,
  COUNT_IF(ABS(nyc_count - fema_count) >= 5) AS abs_diff_ge_5_parcels,
  COUNT_IF(ABS(nyc_count - fema_count) >= 10) AS abs_diff_ge_10_parcels,
  COUNT_IF(num_bldgs IS NOT NULL AND num_bldgs >= 0) AS num_bldgs_known_parcels,
  COUNT_IF(num_bldgs IS NOT NULL AND num_bldgs >= 0 AND nyc_count > CEIL(num_bldgs)) AS nyc_over_num_bldgs_parcels,
  COUNT_IF(num_bldgs IS NOT NULL AND num_bldgs >= 0 AND fema_count > CEIL(num_bldgs)) AS fema_over_num_bldgs_parcels,
  MAX(IFF(num_bldgs IS NOT NULL AND num_bldgs >= 0, nyc_count - CEIL(num_bldgs), NULL)) AS max_nyc_over_num_bldgs,
  MAX(IFF(num_bldgs IS NOT NULL AND num_bldgs >= 0, fema_count - CEIL(num_bldgs), NULL)) AS max_fema_over_num_bldgs
FROM per_parcel;
```

T4 top-disagreement SQL:

```sql
WITH cell AS (
  SELECT 'QN_1500' AS cell_name, '882a103b6bfffff' AS h3_cell, H3_STRING_TO_INT('882a103b6bfffff') AS h3_r8_int
),
parcels AS (
  SELECT
    c.cell_name,
    COALESCE(NULLIF(REGEXP_REPLACE(p.BBL, '\\.0$', ''), ''), 'parcel_row:' || TO_VARCHAR(p.SOURCE_ROW_NUMBER)) AS parcel_id,
    REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm,
    p.ADDRESS,
    p.NUMBLDGS,
    p.GEOM_GEOG AS geom
  FROM cell c
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p ON p.H3_R8 = c.h3_r8_int
  WHERE p.IS_CURRENT_RELEASE = TRUE
    AND p.GEOM_GEOG IS NOT NULL
),
nyc_footprints AS (
  SELECT
    c.cell_name,
    COALESCE(TO_VARCHAR(f.OBJECTID), 'nyc_row:' || TO_VARCHAR(f.SOURCE_ROW_NUMBER)) AS nyc_id,
    f.GEOM_GEOG AS geom,
    NULLIF(f.SHAPE_AREA, 0) AS shape_area_source
  FROM cell c
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT f ON f.H3_R8 = c.h3_cell
  WHERE f.IS_ACTIVE_FOOTPRINT = TRUE
    AND f.GEOM_GEOG IS NOT NULL
),
fema_structures AS (
  SELECT
    c.cell_name,
    f.PROVIDER_FEATURE_ID AS fema_id,
    f.GEOM_GEOG AS geom,
    NULLIF(ST_AREA(f.GEOM_GEOG), 0) AS st_area_geom
  FROM cell c
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_FEATURE_H3_COVERAGE cov
    ON cov.H3_RESOLUTION = 8
   AND cov.H3_CELL = c.h3_cell
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f ON f.PROVIDER_FEATURE_ID = cov.PROVIDER_FEATURE_ID
  WHERE f.GEOM_GEOG IS NOT NULL
),
nyc_edges AS (
  SELECT n.cell_name, n.nyc_id, p.parcel_id
  FROM nyc_footprints n
  JOIN parcels p
    ON p.cell_name = n.cell_name
   AND ST_INTERSECTS(n.geom, p.geom)
  WHERE n.shape_area_source IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(n.geom, p.geom)) / n.shape_area_source > 0.5
),
fema_edges AS (
  SELECT f.cell_name, f.fema_id, p.parcel_id
  FROM fema_structures f
  JOIN parcels p
    ON p.cell_name = f.cell_name
   AND ST_INTERSECTS(f.geom, p.geom)
  WHERE f.st_area_geom IS NOT NULL
    AND ST_AREA(ST_INTERSECTION(f.geom, p.geom)) / f.st_area_geom > 0.5
),
per_parcel AS (
  SELECT
    p.parcel_id,
    ANY_VALUE(p.bbl_norm) AS bbl_norm,
    ANY_VALUE(p.ADDRESS) AS address,
    ANY_VALUE(p.NUMBLDGS) AS num_bldgs,
    COUNT(DISTINCT n.nyc_id) AS nyc_count,
    COUNT(DISTINCT f.fema_id) AS fema_count
  FROM parcels p
  LEFT JOIN nyc_edges n ON n.parcel_id = p.parcel_id
  LEFT JOIN fema_edges f ON f.parcel_id = p.parcel_id
  GROUP BY p.parcel_id
)
SELECT
  bbl_norm,
  address,
  num_bldgs,
  nyc_count,
  fema_count,
  ABS(nyc_count - fema_count) AS abs_count_diff,
  CASE
    WHEN nyc_count > fema_count THEN 'NYC_GT_FEMA'
    WHEN fema_count > nyc_count THEN 'FEMA_GT_NYC'
    ELSE 'EQUAL'
  END AS direction,
  IFF(num_bldgs IS NOT NULL AND num_bldgs >= 0 AND nyc_count > CEIL(num_bldgs), TRUE, FALSE) AS nyc_over_num_bldgs,
  IFF(num_bldgs IS NOT NULL AND num_bldgs >= 0 AND fema_count > CEIL(num_bldgs), TRUE, FALSE) AS fema_over_num_bldgs
FROM per_parcel
WHERE nyc_count <> fema_count
ORDER BY ABS(nyc_count - fema_count) DESC, bbl_norm
LIMIT 20;
```
