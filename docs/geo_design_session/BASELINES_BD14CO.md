# bd-14co Baseline Measurements

Date: 2026-08-16

Agent: PearlSparrow

Scope: five-borough rows in
`EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED` where
`COUNTY_FIPS in ('36005','36047','36061','36081','36085')`.

Data access discipline: all citable numbers below came from
`cmdrvl orchestrator query --tenant salt --timeout 300 --raw` and from returned
`tool_responses[*].structuredContent`, not Loom prose.

MapPLUTO snapshot pinned by structured result:

| release | release_dt | valid_from | valid_to | current | geom_crs | source_geom_crs | lots |
|---|---:|---:|---:|---|---|---|---:|
| 26v1 | 2026-05-01 | 2026-05-01 | null | true | EPSG:4326 | EPSG:2263 | 856,614 |

## Denominators

Five-borough geocode table denominator from the R1/R2 geometry query:

| measure | count |
|---|---:|
| rows | 6,682 |
| distinct `PROPERTY_ADDRESS` | 5,265 |
| distinct surrogate property keys | 6,315 |
| distinct non-null geocoded points | 4,076 |
| ungeocoded rows in this table/scope | 0 |
| placeholder rows (`VARIOUS`, `N/A`, etc.) | 2 |
| placeholder distinct addresses | 1 |
| placeholder surrogate property keys | 2 |

Surrogate property key used where needed:
`PROPERTY_NAME | PROPERTY_ADDRESS | PROPERTY_CITY | PROPERTY_STATE | COUNTY_FIPS`.
This table has no deal, loan, or durable property identifier, so these are
measurement grains, not scored CMBS position keys.

## R1: Geometry-Only PIP Baseline

Grain: distinct non-null `(LATITUDE, LONGITUDE)` points. Predicate:
`ST_CONTAINS(MapPLUTO.GEOM_GEOG, ST_POINT(LONGITUDE, LATITUDE))`.

| denominator points | hit points | hit rate | miss points | single-lot points | multi-lot points | multi-lot rate | max lots |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 4,076 | 3,858 | 94.65% | 218 | 3,858 | 0 | 0.00% | 1 |

Structured rerun of the prior case-8 headline query now returns only
`LOTS_CONTAINING = 1` for 3,858 hit points. A bounded predicate check also returned
the same one-lot distribution for `ST_CONTAINS` and `ST_INTERSECTS`.

Conclusion: on current MapPLUTO `26v1`, the fan-out-aware geometry baseline is
94.65% point coverage and the current multi-lot rate is 0.00%. The earlier 157
multi-lot note is not reproducible against the current structured result and
should not be quoted for this pinned release without recovering its exact older
snapshot/query.

## R2: Tier Breakdowns

Tier assignment at distinct-point grain uses the single `ACCURACY_TYPE` for a
point when unique; points observed with more than one tier are reported as
`mixed` so the denominator reconciles to 4,076.

### Geometry PIP By Tier

| accuracy_type | distinct points | PIP hits | hit rate | multi-lot points | house-number comparable hits | any house-number agreement | agreement rate |
|---|---:|---:|---:|---:|---:|---:|---:|
| ALL | 4,076 | 3,858 | 94.65% | 0 | 3,841 | 2,743 | 71.41% |
| intersection | 19 | 2 | 10.53% | 0 | 2 | 0 | 0.00% |
| mixed | 87 | 87 | 100.00% | 0 | 87 | 58 | 66.67% |
| nearest_rooftop_match | 344 | 344 | 100.00% | 0 | 343 | 166 | 48.40% |
| place | 30 | 23 | 76.67% | 0 | 22 | 0 | 0.00% |
| range_interpolation | 315 | 167 | 53.02% | 0 | 165 | 23 | 13.94% |
| rooftop | 3,216 | 3,213 | 99.91% | 0 | 3,200 | 2,496 | 78.00% |
| street_center | 65 | 22 | 33.85% | 0 | 22 | 0 | 0.00% |

The silent-error slice is clear: `nearest_rooftop_match` has 100.00% PIP
coverage but only 48.40% house-number agreement among comparable hit points.

### Naive Address-String Baseline By Tier

Grain: distinct `(PROPERTY_ADDRESS, COUNTY_FIPS)` address-borough keys. The
normalization is deliberately dumb and matches the recorded baseline: uppercase
and strip every non-alphanumeric character on both CMBS and MapPLUTO `ADDRESS`,
then exact match scoped by MapPLUTO borough.

| accuracy_type | address-county keys | matched keys | coverage | unmatched | multi-match keys | house-number comparable matches | house-number agreements | agreement rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 5,269 | 1,522 | 28.89% | 3,747 | 0 | 1,522 | 1,487 | 97.70% |
| intersection | 20 | 0 | 0.00% | 20 | 0 | 0 | 0 | n/a |
| mixed | 47 | 10 | 21.28% | 37 | 0 | 10 | 10 | 100.00% |
| nearest_rooftop_match | 593 | 9 | 1.52% | 584 | 0 | 9 | 0 | 0.00% |
| place | 43 | 0 | 0.00% | 43 | 0 | 0 | 0 | n/a |
| range_interpolation | 340 | 82 | 24.12% | 258 | 0 | 82 | 79 | 96.34% |
| rooftop | 4,160 | 1,420 | 34.13% | 2,740 | 0 | 1,420 | 1,398 | 98.45% |
| street_center | 66 | 1 | 1.52% | 65 | 0 | 1 | 0 | 0.00% |

This reconciles to the prior baseline: 1,522 / 5,269 = 28.89%, unique when it
fires.

## R3: Corrected Chimera Rate

The corrected chimera detector normalizes both sides before comparing:

- uppercase
- punctuation to spaces
- directionals normalized (`NORTH -> N`, etc.)
- common suffixes normalized (`STREET -> ST`, `AVENUE -> AVE`, etc.)
- spelled ordinals 1-12 normalized
- numeric ordinal suffixes stripped (`72ND -> 72`)

A row is comparable when parsed `NUMBER` and `STREET` are both present. A corrected
chimera row is comparable and either the normalized parsed number is absent from
the normalized raw address or the normalized parsed street is absent from the
normalized raw address.

| accuracy_type | rows | comparable rows | both number/street null | number not in raw | street not in raw | corrected chimera rows | rate of comparable | rate of all rows |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ALL | 6,682 | 6,528 | 51 | 229 | 238 | 449 | 6.88% | 6.72% |
| intersection | 24 | 0 | 0 | 0 | 0 | 0 | n/a | 0.00% |
| nearest_rooftop_match | 688 | 688 | 0 | 86 | 27 | 100 | 14.53% | 14.53% |
| place | 51 | 0 | 51 | 0 | 0 | 0 | n/a | 0.00% |
| range_interpolation | 404 | 404 | 0 | 2 | 22 | 24 | 5.94% | 5.94% |
| rooftop | 5,436 | 5,436 | 0 | 141 | 189 | 325 | 5.98% | 5.98% |
| street_center | 79 | 0 | 0 | 0 | 0 | 0 | n/a | 0.00% |

Nearest-rooftop is again the important tier: 14.53% corrected chimera rate, more
than 2x rooftop's 5.98%.

## R4: Property-Cardinality / Fan-Out Distribution

This is not a true legal parcel-set cardinality distribution. It is the fan-out
visible from the geocode table by PIP to MapPLUTO, using the surrogate property
key above. True parcel-set precision/recall still depends on bd-179b's
address-independent ACRIS ground truth.

### Surrogate Property Key To Distinct PIP Lots

| distinct lots hit | property keys | share |
|---:|---:|---:|
| 0 | 279 | 4.42% |
| 1 | 6,033 | 95.53% |
| 2 | 3 | 0.05% |

### Surrogate Property Key To Distinct Geocode Points

| distinct points | property keys | share |
|---:|---:|---:|
| 1 | 6,306 | 99.86% |
| 2 | 9 | 0.14% |

### Address-County Key To Distinct PIP Lots

| distinct lots hit | address-county keys | share |
|---:|---:|---:|
| 0 | 240 | 4.55% |
| 1 | 5,010 | 95.08% |
| 2 | 19 | 0.36% |

## Kill-Criterion Statement

Precision remains out of scope for bd-14co because scoring either baseline against
PLUTO address-to-BBL as "truth" is circular; address-independent ACRIS ground
truth is bd-179b.

The deterministic cascade must beat two baseline points on the
coverage/precision plane:

- Naive address string: 28.89% coverage at address-string-match precision,
  unique when it fires; precision still unmeasured.
- Geometry-only PIP: 94.65% distinct-point coverage on current MapPLUTO `26v1`,
  with precision still unmeasured and nearest-rooftop carrying the largest
  silent-error signature.

A cascade that only exceeds 28.89% coverage has beaten nothing; geometry already
does that. The cascade must deliver materially more coverage than address-string
matching while maintaining address-grade precision, and higher precision than
geometry-only near geometry's coverage.

## SQL

### MapPLUTO Release Pin

```sql
SELECT
    RELEASE,
    RELEASE_DT,
    VALID_FROM_RELEASE_DT,
    VALID_TO_RELEASE_DT,
    IS_CURRENT_RELEASE,
    GEOM_CRS,
    SOURCE_GEOM_CRS,
    COUNT(*) AS lots
FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
GROUP BY 1,2,3,4,5,6,7
ORDER BY RELEASE_DT DESC, RELEASE;
```

### Geometry PIP Distinct-Point Tier Query

```sql
WITH scope_rows AS (
    SELECT *
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
excluded AS (
    SELECT
        COUNT(*) AS rows_all,
        COUNT(DISTINCT PROPERTY_ADDRESS) AS distinct_addresses_all,
        COUNT(DISTINCT COALESCE(PROPERTY_NAME,'') || '|' || COALESCE(PROPERTY_ADDRESS,'') || '|' || COALESCE(PROPERTY_CITY,'') || '|' || COALESCE(PROPERTY_STATE,'') || '|' || COALESCE(COUNTY_FIPS,'')) AS distinct_property_keys_all,
        SUM(IFF(LATITUDE IS NULL OR LONGITUDE IS NULL, 1, 0)) AS ungeocoded_rows,
        COUNT(DISTINCT IFF(LATITUDE IS NULL OR LONGITUDE IS NULL, PROPERTY_ADDRESS, NULL)) AS ungeocoded_distinct_addresses,
        COUNT(DISTINCT IFF(LATITUDE IS NULL OR LONGITUDE IS NULL, COALESCE(PROPERTY_NAME,'') || '|' || COALESCE(PROPERTY_ADDRESS,'') || '|' || COALESCE(PROPERTY_CITY,'') || '|' || COALESCE(PROPERTY_STATE,'') || '|' || COALESCE(COUNTY_FIPS,''), NULL)) AS ungeocoded_property_keys,
        SUM(IFF(TRIM(UPPER(COALESCE(PROPERTY_ADDRESS,''))) IN ('VARIOUS','VARIOUS ADDRESSES','VARIOUS LOCATIONS','N/A','NA','UNKNOWN','-',''), 1, 0)) AS placeholder_rows,
        COUNT(DISTINCT IFF(TRIM(UPPER(COALESCE(PROPERTY_ADDRESS,''))) IN ('VARIOUS','VARIOUS ADDRESSES','VARIOUS LOCATIONS','N/A','NA','UNKNOWN','-',''), PROPERTY_ADDRESS, NULL)) AS placeholder_distinct_addresses,
        COUNT(DISTINCT IFF(TRIM(UPPER(COALESCE(PROPERTY_ADDRESS,''))) IN ('VARIOUS','VARIOUS ADDRESSES','VARIOUS LOCATIONS','N/A','NA','UNKNOWN','-',''), COALESCE(PROPERTY_NAME,'') || '|' || COALESCE(PROPERTY_ADDRESS,'') || '|' || COALESCE(PROPERTY_CITY,'') || '|' || COALESCE(PROPERTY_STATE,'') || '|' || COALESCE(COUNTY_FIPS,''), NULL)) AS placeholder_property_keys
    FROM scope_rows
),
point_rows AS (
    SELECT *
    FROM scope_rows
    WHERE LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
),
points AS (
    SELECT
        LATITUDE,
        LONGITUDE,
        IFF(COUNT(DISTINCT ACCURACY_TYPE) = 1, MIN(ACCURACY_TYPE), 'mixed') AS accuracy_type,
        COUNT(*) AS source_rows_at_point,
        COUNT(DISTINCT ACCURACY_TYPE) AS accuracy_types_at_point,
        COUNT(DISTINCT REGEXP_REPLACE(UPPER(COALESCE(NUMBER,'')), '[^0-9A-Z]', '')) AS parsed_number_variants_at_point
    FROM point_rows
    GROUP BY LATITUDE, LONGITUDE
),
point_numbers AS (
    SELECT DISTINCT
        LATITUDE,
        LONGITUDE,
        REGEXP_REPLACE(UPPER(NUMBER), '[^0-9A-Z]', '') AS geocode_number_norm
    FROM point_rows
    WHERE NUMBER IS NOT NULL
      AND REGEXP_REPLACE(UPPER(NUMBER), '[^0-9A-Z]', '') <> ''
),
pip_edges AS (
    SELECT
        pts.LATITUDE,
        pts.LONGITUDE,
        pts.accuracy_type,
        p.BBL,
        p.ADDRESS AS pluto_address,
        REGEXP_REPLACE(UPPER(COALESCE(REGEXP_SUBSTR(p.ADDRESS, '^[0-9]+(-[0-9]+)?'), '')), '[^0-9A-Z]', '') AS pluto_house_norm
    FROM points pts
    JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(pts.LONGITUDE, pts.LATITUDE))
),
edge_agreement AS (
    SELECT
        e.*,
        IFF(e.pluto_house_norm <> '' AND EXISTS (
            SELECT 1
            FROM point_numbers n
            WHERE n.LATITUDE = e.LATITUDE
              AND n.LONGITUDE = e.LONGITUDE
              AND n.geocode_number_norm = e.pluto_house_norm
        ), 1, 0) AS house_number_agrees
    FROM pip_edges e
),
point_hit_summary AS (
    SELECT
        pts.LATITUDE,
        pts.LONGITUDE,
        pts.accuracy_type,
        pts.source_rows_at_point,
        pts.accuracy_types_at_point,
        pts.parsed_number_variants_at_point,
        COUNT(DISTINCT ea.BBL) AS lots_containing,
        COUNT(ea.BBL) AS lot_edges,
        SUM(IFF(ea.pluto_house_norm <> '', 1, 0)) AS pluto_house_edges,
        MAX(COALESCE(ea.house_number_agrees, 0)) AS any_house_number_agrees,
        SUM(COALESCE(ea.house_number_agrees, 0)) AS house_number_agree_edges
    FROM points pts
    LEFT JOIN edge_agreement ea
      ON ea.LATITUDE = pts.LATITUDE
     AND ea.LONGITUDE = pts.LONGITUDE
    GROUP BY pts.LATITUDE, pts.LONGITUDE, pts.accuracy_type, pts.source_rows_at_point, pts.accuracy_types_at_point, pts.parsed_number_variants_at_point
),
agg AS (
    SELECT
        accuracy_type,
        COUNT(*) AS distinct_points,
        SUM(IFF(lots_containing > 0, 1, 0)) AS pip_hit_points,
        SUM(IFF(lots_containing = 0, 1, 0)) AS pip_miss_points,
        SUM(IFF(lots_containing = 1, 1, 0)) AS single_lot_points,
        SUM(IFF(lots_containing > 1, 1, 0)) AS multi_lot_points,
        SUM(lots_containing) AS lot_edges,
        MAX(lots_containing) AS max_lots_containing,
        SUM(IFF(lots_containing > 0 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0, 1, 0)) AS house_num_comparable_hit_points,
        SUM(IFF(lots_containing > 0 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0 AND any_house_number_agrees = 1, 1, 0)) AS points_with_any_house_num_agreement,
        SUM(IFF(lots_containing = 1 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0, 1, 0)) AS comparable_single_lot_points,
        SUM(IFF(lots_containing = 1 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0 AND any_house_number_agrees = 1, 1, 0)) AS single_lot_points_house_num_agree,
        SUM(house_number_agree_edges) AS house_num_agree_edges
    FROM point_hit_summary
    GROUP BY accuracy_type
),
all_agg AS (
    SELECT
        'ALL' AS accuracy_type,
        COUNT(*) AS distinct_points,
        SUM(IFF(lots_containing > 0, 1, 0)) AS pip_hit_points,
        SUM(IFF(lots_containing = 0, 1, 0)) AS pip_miss_points,
        SUM(IFF(lots_containing = 1, 1, 0)) AS single_lot_points,
        SUM(IFF(lots_containing > 1, 1, 0)) AS multi_lot_points,
        SUM(lots_containing) AS lot_edges,
        MAX(lots_containing) AS max_lots_containing,
        SUM(IFF(lots_containing > 0 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0, 1, 0)) AS house_num_comparable_hit_points,
        SUM(IFF(lots_containing > 0 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0 AND any_house_number_agrees = 1, 1, 0)) AS points_with_any_house_num_agreement,
        SUM(IFF(lots_containing = 1 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0, 1, 0)) AS comparable_single_lot_points,
        SUM(IFF(lots_containing = 1 AND parsed_number_variants_at_point > 0 AND pluto_house_edges > 0 AND any_house_number_agrees = 1, 1, 0)) AS single_lot_points_house_num_agree,
        SUM(house_number_agree_edges) AS house_num_agree_edges
    FROM point_hit_summary
),
combined AS (
    SELECT * FROM all_agg
    UNION ALL
    SELECT * FROM agg
)
SELECT
    'GEOMETRY_PIP_BY_DISTINCT_POINT' AS section,
    c.accuracy_type,
    c.distinct_points,
    c.pip_hit_points,
    ROUND(100.0 * c.pip_hit_points / NULLIF(c.distinct_points, 0), 2) AS pip_hit_rate_pct,
    c.pip_miss_points,
    c.single_lot_points,
    ROUND(100.0 * c.single_lot_points / NULLIF(c.distinct_points, 0), 2) AS single_lot_rate_pct,
    c.multi_lot_points,
    ROUND(100.0 * c.multi_lot_points / NULLIF(c.distinct_points, 0), 2) AS multi_lot_rate_pct,
    c.lot_edges,
    ROUND(c.lot_edges / NULLIF(c.distinct_points, 0), 4) AS avg_lot_edges_per_point,
    c.max_lots_containing,
    c.house_num_comparable_hit_points,
    c.points_with_any_house_num_agreement,
    ROUND(100.0 * c.points_with_any_house_num_agreement / NULLIF(c.house_num_comparable_hit_points, 0), 2) AS house_num_agree_rate_pct,
    c.comparable_single_lot_points,
    c.single_lot_points_house_num_agree,
    ROUND(100.0 * c.single_lot_points_house_num_agree / NULLIF(c.comparable_single_lot_points, 0), 2) AS single_lot_house_num_agree_rate_pct,
    c.house_num_agree_edges,
    e.rows_all,
    e.distinct_addresses_all,
    e.distinct_property_keys_all,
    e.ungeocoded_rows,
    e.ungeocoded_distinct_addresses,
    e.ungeocoded_property_keys,
    e.placeholder_rows,
    e.placeholder_distinct_addresses,
    e.placeholder_property_keys
FROM combined c
CROSS JOIN excluded e
ORDER BY IFF(c.accuracy_type = 'ALL', 0, 1), c.accuracy_type;
```

### Prior Multi-Lot Reconciliation Query

```sql
SELECT lots_containing, COUNT(*) AS points
FROM   (SELECT s.LATITUDE, s.LONGITUDE, COUNT(DISTINCT p.BBL) AS lots_containing
        FROM  (SELECT DISTINCT LATITUDE, LONGITUDE
               FROM  EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
               WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
                 AND LATITUDE IS NOT NULL) s
        JOIN   EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
          ON   ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
        GROUP BY 1,2)
GROUP BY 1 ORDER BY 1;
```

### Predicate Check Query

```sql
WITH pts AS (
    SELECT DISTINCT LATITUDE, LONGITUDE
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
),
contains_hits AS (
    SELECT pts.LATITUDE, pts.LONGITUDE, COUNT(DISTINCT p.BBL) AS lots_containing
    FROM pts
    JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(pts.LONGITUDE, pts.LATITUDE))
    GROUP BY 1,2
),
intersects_hits AS (
    SELECT pts.LATITUDE, pts.LONGITUDE, COUNT(DISTINCT p.BBL) AS lots_intersecting
    FROM pts
    JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON ST_INTERSECTS(p.GEOM_GEOG, ST_POINT(pts.LONGITUDE, pts.LATITUDE))
    GROUP BY 1,2
)
SELECT 'ST_CONTAINS' AS predicate, lots_containing AS lot_count, COUNT(*) AS points
FROM contains_hits
GROUP BY 1,2
UNION ALL
SELECT 'ST_INTERSECTS' AS predicate, lots_intersecting AS lot_count, COUNT(*) AS points
FROM intersects_hits
GROUP BY 1,2
ORDER BY predicate, lot_count;
```

### Address Baseline Tier Query

```sql
WITH scope_rows AS (
    SELECT *
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
address_keys AS (
    SELECT
        PROPERTY_ADDRESS,
        COUNTY_FIPS,
        CASE COUNTY_FIPS
            WHEN '36005' THEN 'BX'
            WHEN '36047' THEN 'BK'
            WHEN '36061' THEN 'MN'
            WHEN '36081' THEN 'QN'
            WHEN '36085' THEN 'SI'
        END AS borough,
        IFF(COUNT(DISTINCT ACCURACY_TYPE) = 1, MIN(ACCURACY_TYPE), 'mixed') AS accuracy_type,
        COUNT(*) AS source_rows_for_address,
        COUNT(DISTINCT ACCURACY_TYPE) AS accuracy_types_for_address,
        COUNT(DISTINCT REGEXP_REPLACE(UPPER(COALESCE(NUMBER,'')), '[^0-9A-Z]', '')) AS parsed_number_variants_for_address
    FROM scope_rows
    GROUP BY PROPERTY_ADDRESS, COUNTY_FIPS
),
address_numbers AS (
    SELECT DISTINCT
        PROPERTY_ADDRESS,
        COUNTY_FIPS,
        REGEXP_REPLACE(UPPER(NUMBER), '[^0-9A-Z]', '') AS geocode_number_norm
    FROM scope_rows
    WHERE NUMBER IS NOT NULL
      AND REGEXP_REPLACE(UPPER(NUMBER), '[^0-9A-Z]', '') <> ''
),
cm_norm AS (
    SELECT
        PROPERTY_ADDRESS,
        COUNTY_FIPS,
        borough,
        accuracy_type,
        source_rows_for_address,
        accuracy_types_for_address,
        parsed_number_variants_for_address,
        REGEXP_REPLACE(UPPER(TRIM(PROPERTY_ADDRESS)), '[^A-Z0-9]', '') AS norm_addr
    FROM address_keys
),
pluto_norm AS (
    SELECT
        BOROUGH,
        BBL,
        ADDRESS AS pluto_address,
        REGEXP_REPLACE(UPPER(TRIM(ADDRESS)), '[^A-Z0-9]', '') AS norm_addr,
        REGEXP_REPLACE(UPPER(COALESCE(REGEXP_SUBSTR(ADDRESS, '^[0-9]+(-[0-9]+)?'), '')), '[^0-9A-Z]', '') AS pluto_house_norm
    FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
),
match_edges AS (
    SELECT
        c.PROPERTY_ADDRESS,
        c.COUNTY_FIPS,
        c.accuracy_type,
        c.parsed_number_variants_for_address,
        p.BBL,
        p.pluto_address,
        p.pluto_house_norm,
        IFF(p.pluto_house_norm <> '' AND EXISTS (
            SELECT 1
            FROM address_numbers n
            WHERE n.PROPERTY_ADDRESS = c.PROPERTY_ADDRESS
              AND n.COUNTY_FIPS = c.COUNTY_FIPS
              AND n.geocode_number_norm = p.pluto_house_norm
        ), 1, 0) AS house_number_agrees
    FROM cm_norm c
    JOIN pluto_norm p
      ON p.BOROUGH = c.borough
     AND p.norm_addr = c.norm_addr
),
key_summary AS (
    SELECT
        c.PROPERTY_ADDRESS,
        c.COUNTY_FIPS,
        c.accuracy_type,
        c.parsed_number_variants_for_address,
        COUNT(DISTINCT m.BBL) AS matched_lots,
        COUNT(m.BBL) AS match_edges,
        SUM(IFF(m.pluto_house_norm <> '', 1, 0)) AS pluto_house_edges,
        MAX(COALESCE(m.house_number_agrees, 0)) AS any_house_number_agrees,
        SUM(COALESCE(m.house_number_agrees, 0)) AS house_number_agree_edges
    FROM cm_norm c
    LEFT JOIN match_edges m
      ON m.PROPERTY_ADDRESS = c.PROPERTY_ADDRESS
     AND m.COUNTY_FIPS = c.COUNTY_FIPS
    GROUP BY c.PROPERTY_ADDRESS, c.COUNTY_FIPS, c.accuracy_type, c.parsed_number_variants_for_address
),
agg AS (
    SELECT
        accuracy_type,
        COUNT(*) AS address_county_keys,
        SUM(IFF(matched_lots > 0, 1, 0)) AS matched_keys,
        SUM(IFF(matched_lots = 0, 1, 0)) AS unmatched_keys,
        SUM(IFF(matched_lots = 1, 1, 0)) AS exactly_one_lot_keys,
        SUM(IFF(matched_lots > 1, 1, 0)) AS multi_match_keys,
        SUM(matched_lots) AS lot_edges,
        MAX(matched_lots) AS max_lots_matched,
        SUM(IFF(matched_lots > 0 AND parsed_number_variants_for_address > 0 AND pluto_house_edges > 0, 1, 0)) AS house_num_comparable_matched_keys,
        SUM(IFF(matched_lots > 0 AND parsed_number_variants_for_address > 0 AND pluto_house_edges > 0 AND any_house_number_agrees = 1, 1, 0)) AS matched_keys_with_house_num_agreement,
        SUM(house_number_agree_edges) AS house_num_agree_edges
    FROM key_summary
    GROUP BY accuracy_type
),
all_agg AS (
    SELECT
        'ALL' AS accuracy_type,
        COUNT(*) AS address_county_keys,
        SUM(IFF(matched_lots > 0, 1, 0)) AS matched_keys,
        SUM(IFF(matched_lots = 0, 1, 0)) AS unmatched_keys,
        SUM(IFF(matched_lots = 1, 1, 0)) AS exactly_one_lot_keys,
        SUM(IFF(matched_lots > 1, 1, 0)) AS multi_match_keys,
        SUM(matched_lots) AS lot_edges,
        MAX(matched_lots) AS max_lots_matched,
        SUM(IFF(matched_lots > 0 AND parsed_number_variants_for_address > 0 AND pluto_house_edges > 0, 1, 0)) AS house_num_comparable_matched_keys,
        SUM(IFF(matched_lots > 0 AND parsed_number_variants_for_address > 0 AND pluto_house_edges > 0 AND any_house_number_agrees = 1, 1, 0)) AS matched_keys_with_house_num_agreement,
        SUM(house_number_agree_edges) AS house_num_agree_edges
    FROM key_summary
),
combined AS (
    SELECT * FROM all_agg
    UNION ALL
    SELECT * FROM agg
)
SELECT
    'NAIVE_ADDRESS_STRING_STRIP_NONALNUM_BY_ADDRESS_COUNTY' AS section,
    accuracy_type,
    address_county_keys,
    matched_keys,
    ROUND(100.0 * matched_keys / NULLIF(address_county_keys, 0), 2) AS matched_coverage_pct,
    unmatched_keys,
    exactly_one_lot_keys,
    multi_match_keys,
    ROUND(100.0 * multi_match_keys / NULLIF(address_county_keys, 0), 2) AS multi_match_rate_pct,
    lot_edges,
    max_lots_matched,
    house_num_comparable_matched_keys,
    matched_keys_with_house_num_agreement,
    ROUND(100.0 * matched_keys_with_house_num_agreement / NULLIF(house_num_comparable_matched_keys, 0), 2) AS house_num_agree_rate_pct,
    house_num_agree_edges
FROM combined
ORDER BY IFF(accuracy_type = 'ALL', 0, 1), accuracy_type;
```

### Corrected Chimera Query

```sql
WITH scope_rows AS (
    SELECT *
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
base AS (
    SELECT
        ACCURACY_TYPE,
        PROPERTY_NAME,
        PROPERTY_ADDRESS,
        PROPERTY_CITY,
        PROPERTY_STATE,
        COUNTY_FIPS,
        NUMBER,
        STREET,
        ' ' || REGEXP_REPLACE(UPPER(COALESCE(PROPERTY_ADDRESS,'')), '[^A-Z0-9]+', ' ') || ' ' AS raw0,
        ' ' || REGEXP_REPLACE(UPPER(COALESCE(STREET,'')), '[^A-Z0-9]+', ' ') || ' ' AS street0,
        REGEXP_REPLACE(UPPER(COALESCE(NUMBER,'')), '[^0-9A-Z]', '') AS number_norm,
        REGEXP_REPLACE(UPPER(COALESCE(PROPERTY_ADDRESS,'')), '[^0-9A-Z]', '') AS raw_alnum
    FROM scope_rows
),
ord_words AS (
    SELECT *,
        REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(raw0,
            ' FIRST ', ' 1ST '), ' SECOND ', ' 2ND '), ' THIRD ', ' 3RD '), ' FOURTH ', ' 4TH '), ' FIFTH ', ' 5TH '),
            ' SIXTH ', ' 6TH '), ' SEVENTH ', ' 7TH '), ' EIGHTH ', ' 8TH '), ' NINTH ', ' 9TH '), ' TENTH ', ' 10TH '),
            ' ELEVENTH ', ' 11TH '), ' TWELFTH ', ' 12TH ') AS raw1,
        REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(street0,
            ' FIRST ', ' 1ST '), ' SECOND ', ' 2ND '), ' THIRD ', ' 3RD '), ' FOURTH ', ' 4TH '), ' FIFTH ', ' 5TH '),
            ' SIXTH ', ' 6TH '), ' SEVENTH ', ' 7TH '), ' EIGHTH ', ' 8TH '), ' NINTH ', ' 9TH '), ' TENTH ', ' 10TH '),
            ' ELEVENTH ', ' 11TH '), ' TWELFTH ', ' 12TH ') AS street1
    FROM base
),
dir_suffix AS (
    SELECT *,
        REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(raw1,
            ' NORTH ', ' N '), ' SOUTH ', ' S '), ' EAST ', ' E '), ' WEST ', ' W '),
            ' STREET ', ' ST '), ' AVENUE ', ' AVE '), ' ROAD ', ' RD '), ' BOULEVARD ', ' BLVD '), ' PLACE ', ' PL '),
            ' DRIVE ', ' DR '), ' LANE ', ' LN '), ' COURT ', ' CT '), ' PARKWAY ', ' PKWY '), ' HIGHWAY ', ' HWY '),
            ' TERRACE ', ' TER '), ' CIRCLE ', ' CIR '), ' EXPRESSWAY ', ' EXPY '), ' PLAZA ', ' PLZ ') AS raw2,
        REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(street1,
            ' NORTH ', ' N '), ' SOUTH ', ' S '), ' EAST ', ' E '), ' WEST ', ' W '),
            ' STREET ', ' ST '), ' AVENUE ', ' AVE '), ' ROAD ', ' RD '), ' BOULEVARD ', ' BLVD '), ' PLACE ', ' PL '),
            ' DRIVE ', ' DR '), ' LANE ', ' LN '), ' COURT ', ' CT '), ' PARKWAY ', ' PKWY '), ' HIGHWAY ', ' HWY '),
            ' TERRACE ', ' TER '), ' CIRCLE ', ' CIR '), ' EXPRESSWAY ', ' EXPY '), ' PLAZA ', ' PLZ ') AS street2
    FROM ord_words
),
normalized AS (
    SELECT *,
        TRIM(REGEXP_REPLACE(REGEXP_REPLACE(raw2, '([0-9]+)(ST|ND|RD|TH)', '\\1'), ' +', ' ')) AS raw_norm,
        TRIM(REGEXP_REPLACE(REGEXP_REPLACE(street2, '([0-9]+)(ST|ND|RD|TH)', '\\1'), ' +', ' ')) AS street_norm
    FROM dir_suffix
),
classified AS (
    SELECT
        ACCURACY_TYPE,
        PROPERTY_NAME,
        PROPERTY_ADDRESS,
        PROPERTY_CITY,
        PROPERTY_STATE,
        COUNTY_FIPS,
        NUMBER,
        STREET,
        number_norm,
        raw_norm,
        street_norm,
        IFF(NUMBER IS NOT NULL AND STREET IS NOT NULL AND number_norm <> '' AND street_norm <> '', 1, 0) AS comparable,
        IFF(number_norm <> '' AND POSITION(number_norm IN raw_alnum) > 0, 1, 0) AS number_in_raw,
        IFF(street_norm <> '' AND POSITION(' ' || street_norm || ' ' IN ' ' || raw_norm || ' ') > 0, 1, 0) AS street_in_raw
    FROM normalized
),
row_flags AS (
    SELECT *,
        IFF(comparable = 1 AND number_in_raw = 0, 1, 0) AS number_not_in_raw,
        IFF(comparable = 1 AND street_in_raw = 0, 1, 0) AS street_not_in_raw,
        IFF(comparable = 1 AND (number_in_raw = 0 OR street_in_raw = 0), 1, 0) AS corrected_chimera
    FROM classified
),
agg AS (
    SELECT
        ACCURACY_TYPE,
        COUNT(*) AS rows_all,
        COUNT(DISTINCT PROPERTY_ADDRESS) AS distinct_addresses,
        COUNT(DISTINCT COALESCE(PROPERTY_NAME,'') || '|' || COALESCE(PROPERTY_ADDRESS,'') || '|' || COALESCE(PROPERTY_CITY,'') || '|' || COALESCE(PROPERTY_STATE,'') || '|' || COALESCE(COUNTY_FIPS,'')) AS distinct_property_keys,
        SUM(comparable) AS comparable_rows,
        SUM(IFF(NUMBER IS NULL AND STREET IS NULL, 1, 0)) AS number_and_street_both_null_rows,
        SUM(number_not_in_raw) AS number_not_in_raw_rows,
        SUM(street_not_in_raw) AS street_not_in_raw_rows,
        SUM(corrected_chimera) AS corrected_chimera_rows
    FROM row_flags
    GROUP BY ACCURACY_TYPE
),
all_agg AS (
    SELECT
        'ALL' AS ACCURACY_TYPE,
        COUNT(*) AS rows_all,
        COUNT(DISTINCT PROPERTY_ADDRESS) AS distinct_addresses,
        COUNT(DISTINCT COALESCE(PROPERTY_NAME,'') || '|' || COALESCE(PROPERTY_ADDRESS,'') || '|' || COALESCE(PROPERTY_CITY,'') || '|' || COALESCE(PROPERTY_STATE,'') || '|' || COALESCE(COUNTY_FIPS,'')) AS distinct_property_keys,
        SUM(comparable) AS comparable_rows,
        SUM(IFF(NUMBER IS NULL AND STREET IS NULL, 1, 0)) AS number_and_street_both_null_rows,
        SUM(number_not_in_raw) AS number_not_in_raw_rows,
        SUM(street_not_in_raw) AS street_not_in_raw_rows,
        SUM(corrected_chimera) AS corrected_chimera_rows
    FROM row_flags
),
combined AS (
    SELECT * FROM all_agg
    UNION ALL
    SELECT * FROM agg
)
SELECT
    'CORRECTED_CHIMERA_TWO_SIDED_NORMALIZATION' AS section,
    ACCURACY_TYPE,
    rows_all,
    distinct_addresses,
    distinct_property_keys,
    comparable_rows,
    number_and_street_both_null_rows,
    number_not_in_raw_rows,
    street_not_in_raw_rows,
    corrected_chimera_rows,
    ROUND(100.0 * corrected_chimera_rows / NULLIF(comparable_rows, 0), 2) AS chimera_rate_comparable_pct,
    ROUND(100.0 * corrected_chimera_rows / NULLIF(rows_all, 0), 2) AS chimera_rate_all_rows_pct
FROM combined
ORDER BY IFF(ACCURACY_TYPE = 'ALL', 0, 1), ACCURACY_TYPE;
```

### Property Fan-Out Query

```sql
WITH scope_rows AS (
    SELECT *
    FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
),
property_rows AS (
    SELECT
        COALESCE(PROPERTY_NAME,'') AS property_name,
        COALESCE(PROPERTY_ADDRESS,'') AS property_address,
        COALESCE(PROPERTY_CITY,'') AS property_city,
        COALESCE(PROPERTY_STATE,'') AS property_state,
        COALESCE(COUNTY_FIPS,'') AS county_fips,
        LATITUDE,
        LONGITUDE,
        SOURCE,
        ASOF
    FROM scope_rows
),
property_edges AS (
    SELECT DISTINCT
        r.property_name,
        r.property_address,
        r.property_city,
        r.property_state,
        r.county_fips,
        p.BBL
    FROM property_rows r
    JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE))
),
property_summary AS (
    SELECT
        r.property_name,
        r.property_address,
        r.property_city,
        r.property_state,
        r.county_fips,
        COUNT(*) AS source_asof_rows,
        COUNT(DISTINCT TO_VARCHAR(r.LATITUDE) || ',' || TO_VARCHAR(r.LONGITUDE)) AS distinct_points,
        COUNT(DISTINCT e.BBL) AS distinct_lots_hit
    FROM property_rows r
    LEFT JOIN property_edges e
      ON e.property_name = r.property_name
     AND e.property_address = r.property_address
     AND e.property_city = r.property_city
     AND e.property_state = r.property_state
     AND e.county_fips = r.county_fips
    GROUP BY r.property_name, r.property_address, r.property_city, r.property_state, r.county_fips
),
address_rows AS (
    SELECT
        COALESCE(PROPERTY_ADDRESS,'') AS property_address,
        COALESCE(COUNTY_FIPS,'') AS county_fips,
        LATITUDE,
        LONGITUDE,
        SOURCE,
        ASOF
    FROM scope_rows
),
address_edges AS (
    SELECT DISTINCT
        r.property_address,
        r.county_fips,
        p.BBL
    FROM address_rows r
    JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE))
),
address_summary AS (
    SELECT
        r.property_address,
        r.county_fips,
        COUNT(*) AS source_asof_rows,
        COUNT(DISTINCT TO_VARCHAR(r.LATITUDE) || ',' || TO_VARCHAR(r.LONGITUDE)) AS distinct_points,
        COUNT(DISTINCT e.BBL) AS distinct_lots_hit
    FROM address_rows r
    LEFT JOIN address_edges e
      ON e.property_address = r.property_address
     AND e.county_fips = r.county_fips
    GROUP BY r.property_address, r.county_fips
),
property_lot_dist AS (
    SELECT 'PROPERTY_KEY_LOT_FANOUT' AS section, TO_VARCHAR(distinct_lots_hit) AS bucket, COUNT(*) AS records, SUM(source_asof_rows) AS source_asof_rows, SUM(distinct_points) AS summed_distinct_points
    FROM property_summary
    GROUP BY distinct_lots_hit
),
property_point_dist AS (
    SELECT 'PROPERTY_KEY_POINT_FANOUT' AS section, TO_VARCHAR(distinct_points) AS bucket, COUNT(*) AS records, SUM(source_asof_rows) AS source_asof_rows, SUM(distinct_lots_hit) AS summed_distinct_points
    FROM property_summary
    GROUP BY distinct_points
),
address_lot_dist AS (
    SELECT 'ADDRESS_COUNTY_LOT_FANOUT' AS section, TO_VARCHAR(distinct_lots_hit) AS bucket, COUNT(*) AS records, SUM(source_asof_rows) AS source_asof_rows, SUM(distinct_points) AS summed_distinct_points
    FROM address_summary
    GROUP BY distinct_lots_hit
),
combined AS (
    SELECT * FROM property_lot_dist
    UNION ALL SELECT * FROM property_point_dist
    UNION ALL SELECT * FROM address_lot_dist
),
totals AS (
    SELECT section, SUM(records) AS total_records
    FROM combined
    GROUP BY section
)
SELECT
    c.section,
    c.bucket,
    c.records,
    ROUND(100.0 * c.records / NULLIF(t.total_records, 0), 2) AS pct_of_section_records,
    c.source_asof_rows,
    c.summed_distinct_points,
    t.total_records AS section_total_records
FROM combined c
JOIN totals t USING (section)
ORDER BY section, TRY_TO_NUMBER(bucket);
```

