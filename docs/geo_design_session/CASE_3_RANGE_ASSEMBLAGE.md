# bd-tccn Case 3 - Range Address Assemblage

Case property: **107-109-111 North 9th Street**, Brooklyn, NY.

Verdict: **resolved assemblage**. The answer is not one BBL. It is a three-parcel,
three-building member set:

- `3023030029` / `107 NORTH 9 STREET` / NYC BIN `3061657`
- `3023030028` / `109 NORTH 9 STREET` / NYC BIN `3061656`
- `3023030027` / `111 NORTH 9 STREET` / NYC BIN `3061655`

FEMA and Microsoft do not provide majority-linked member footprints for these parcels.

Standing measurements cited, not rederived:

- Appendix E: `nearest_rooftop_match` is a silent-error tier; naive address-string has low
  coverage and cannot express ranges.
- Appendix F: footprint-to-parcel edges use geometric
  `ST_AREA(intersection)/ST_AREA(footprint) > 0.5`.
- Appendix G: the cost model is component-wise; this case is a small assemblage component
  inside a dense tile.

## Selection Rule

Range/split-address enumeration was run over non-Queens five-borough rows so Queens
hyphenated grid addresses were not misread as ranges. The selected candidate appears with
multiple assertions and parse choices: 2025 rows parse to `109`, while the 2026 row parses
to `107`.

Selector SQL:

```sql
SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, PROPERTY_STATE, PROPERTY_ZIP,
       PROPERTY_COUNTY, COUNTY_FIPS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
       SOURCE, ASOF, LATITUDE, LONGITUDE
FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
WHERE COUNTY_FIPS IN ('36005','36047','36061','36085')
  AND LATITUDE IS NOT NULL
  AND LONGITUDE IS NOT NULL
  AND (PROPERTY_ADDRESS RLIKE '.*[0-9]+ ?[-/] ?[0-9]+.*'
       OR PROPERTY_ADDRESS ILIKE '% TO %')
ORDER BY PROPERTY_ADDRESS, ASOF, SOURCE
LIMIT 50;
```

Structured selector output included these relevant rows:

| property_address | number | accuracy_type | score | asof | lat | lon |
|---|---:|---|---:|---|---:|---:|
| 107 109-111 North 9th Street | 109 | nearest_rooftop_match | 0.80 | 2025-01-01 | 40.7201080 | -73.9582040 |
| 107-109-111 N 9TH ST | 109 | nearest_rooftop_match | 0.90 | 2025-01-01 | 40.7201080 | -73.9582040 |
| 107-109-111 North 9th Street | 109 | nearest_rooftop_match | 0.90 | 2025-01-01 | 40.7201080 | -73.9582040 |
| 107-109-111 North 9th Street | 107 | rooftop | 0.99 | 2026-08-01 | 40.7201600 | -73.9582760 |

## Assertion Row - Six Contract Fields

The clearest row is the current 2026 assertion:

| contract field | case value |
|---|---|
| geocode | present: `(-73.9582760, 40.7201600)`, `accuracy_type=rooftop`, `accuracy_score=0.99`, `ASOF=2026-08-01` |
| address | present: raw `107-109-111 North 9th Street`; parsed `NUMBER=107`, `STREET=N 9th St`; locale NYC/Brooklyn |
| geometry | absent in CMBS geocode row |
| building size | absent in CMBS geocode row |
| year built | absent in CMBS geocode row |
| property type | absent in CMBS geocode row |

The previous 2025 assertions are also part of the case because they parse the same range
to `109` and geocode to the middle lot.

## Geocode Rollup

| property_address | number | tier | asof | containing lots | containing BBL/address | nearest BBL/address |
|---|---:|---|---|---:|---|---|
| 107 109-111 North 9th Street | 109 | nearest_rooftop_match | 2025-01-01 | 1 | `3023030028` / 109 NORTH 9 STREET | `3023030028` / 109 NORTH 9 STREET |
| 107-109-111 N 9TH ST | 109 | nearest_rooftop_match | 2025-01-01 | 1 | `3023030028` / 109 NORTH 9 STREET | `3023030028` / 109 NORTH 9 STREET |
| 107-109-111 North 9th Street | 109 | nearest_rooftop_match | 2025-01-01 | 1 | `3023030028` / 109 NORTH 9 STREET | `3023030028` / 109 NORTH 9 STREET |
| 107-109-111 North 9th Street | 107 | rooftop | 2026-08-01 | 1 | `3023030029` / 107 NORTH 9 STREET | `3023030029` / 107 NORTH 9 STREET |

SQL:

```sql
WITH assertion_rows AS (
  SELECT PROPERTY_NAME, PROPERTY_ADDRESS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
         SOURCE, ASOF, LATITUDE, LONGITUDE
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS = '36047'
    AND (PROPERTY_ADDRESS ILIKE '%107%109%111%N%9%'
         OR PROPERTY_ADDRESS ILIKE '%107%109%111%North%9%')
),
contains_edges AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.ACCURACY_TYPE, r.ACCURACY_SCORE, r.ASOF,
         r.LATITUDE, r.LONGITUDE, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS BBL_NORM,
         p.ADDRESS,
         ST_DISTANCE(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE)) AS distance_m
  FROM assertion_rows r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE))
),
nearest_edges AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.ACCURACY_TYPE, r.ACCURACY_SCORE, r.ASOF,
         r.LATITUDE, r.LONGITUDE, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS BBL_NORM,
         p.ADDRESS,
         ST_DISTANCE(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE)) AS distance_m
  FROM assertion_rows r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_DWITHIN(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE), 50)
  QUALIFY ROW_NUMBER() OVER (
    PARTITION BY r.PROPERTY_ADDRESS, r.NUMBER, r.ACCURACY_TYPE, r.ASOF, r.LATITUDE, r.LONGITUDE
    ORDER BY ST_DISTANCE(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE)),
             REGEXP_REPLACE(p.BBL, '\\.0$', '')
  ) = 1
),
rollup AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.ACCURACY_TYPE, r.ACCURACY_SCORE, r.ASOF,
         r.LATITUDE, r.LONGITUDE, COUNT(DISTINCT ce.BBL_NORM) AS containing_lots,
         ARRAY_AGG(DISTINCT ce.BBL_NORM) AS containing_bbls,
         ARRAY_AGG(DISTINCT ce.ADDRESS) AS containing_addresses
  FROM assertion_rows r
  LEFT JOIN contains_edges ce
    ON r.PROPERTY_ADDRESS = ce.PROPERTY_ADDRESS
   AND r.NUMBER = ce.NUMBER
   AND r.ACCURACY_TYPE = ce.ACCURACY_TYPE
   AND r.ASOF = ce.ASOF
   AND r.LATITUDE = ce.LATITUDE
   AND r.LONGITUDE = ce.LONGITUDE
  GROUP BY 1,2,3,4,5,6,7
)
SELECT rollup.*, ne.BBL_NORM AS nearest_bbl, ne.ADDRESS AS nearest_address,
       ne.distance_m AS nearest_distance_m
FROM rollup
LEFT JOIN nearest_edges ne
  ON rollup.PROPERTY_ADDRESS = ne.PROPERTY_ADDRESS
 AND rollup.NUMBER = ne.NUMBER
 AND rollup.ACCURACY_TYPE = ne.ACCURACY_TYPE
 AND rollup.ASOF = ne.ASOF
 AND rollup.LATITUDE = ne.LATITUDE
 AND rollup.LONGITUDE = ne.LONGITUDE
ORDER BY ASOF, PROPERTY_ADDRESS;
```

## Member Set Evidence

MapPLUTO address query for the three range endpoints:

| house | BBL | address | owner | BLDGAREA | LOTAREA | year |
|---:|---|---|---|---:|---:|---:|
| 107 | 3023030029 | 107 NORTH 9 STREET | 107-109-111 NORTH 9TH STREET LLC | 3,750 | 2,500 | 1910 |
| 109 | 3023030028 | 109 NORTH 9 STREET | 107-109-111 NORTH 9TH STREET LLC | 4,050 | 2,500 | 1910 |
| 111 | 3023030027 | 111 NORTH 9 STREET | 107-109-111 NORTH 9TH STREET LLC | 4,050 | 2,500 | 1910 |

SQL:

```sql
WITH nums AS (SELECT '107' AS house UNION ALL SELECT '109' UNION ALL SELECT '111'),
candidates AS (
  SELECT n.house, p.BOROUGH, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS BBL_NORM,
         p.ADDRESS, p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS, p.BLDGAREA, p.LOTAREA,
         p.NUMFLOORS, p.YEARBUILT, p.OWNERNAME, p.CENTROID_LON, p.CENTROID_LAT
  FROM nums n
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.BOROUGH = 'BK'
   AND REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '') =
       REGEXP_REPLACE(UPPER(TRIM(n.house || ' NORTH 9 STREET')), '[^A-Z0-9]', '')
)
SELECT * FROM candidates ORDER BY house, BBL_NORM;
```

Footprint observations:

| source | native id | member parcel | area | height | year | area ratio |
|---|---|---|---:|---:|---:|---:|
| NYC footprint | 3061655 | 3023030027 | 106.640895 sq m | 34.48 ft | 1910 | 0.964900 |
| NYC footprint | 3061656 | 3023030028 | 120.291983 sq m | 33.48 ft | 1910 | 0.965983 |
| NYC footprint | 3061657 | 3023030029 | 105.088461 sq m | 34.48 ft | 1910 | 0.901519 |

Same SQL also returned the three MapPLUTO rows above and no FEMA/Microsoft majority rows:

```sql
WITH target_bbls AS (
  SELECT '3023030029' AS bbl_norm UNION ALL SELECT '3023030028' UNION ALL SELECT '3023030027'
),
target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  JOIN target_bbls t ON REGEXP_REPLACE(p.BBL, '\\.0$', '') = t.bbl_norm
),
observations AS (
  SELECT 'mappluto' AS src, 'parcel' AS entity_level, p.RELEASE_DT::TEXT AS src_vintage,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS native_id,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS member_parcel_bbl,
         p.ADDRESS AS src_address, p.BLDGCLASS AS src_class, p.LANDUSE AS src_landuse,
         p.BLDGAREA::FLOAT AS src_area, p.LOTAREA::FLOAT AS lot_area,
         p.NUMBLDGS::FLOAT AS num_buildings, p.NUMFLOORS::FLOAT AS height_or_floors,
         p.YEARBUILT::FLOAT AS year_built, p.OWNERNAME AS owner_name,
         'observed' AS attr_provenance, NULL::TEXT AS license_terms,
         NULL::FLOAT AS area_ratio_to_member_parcel
  FROM target p
  UNION ALL
  SELECT 'nyc_building_footprints', 'building', b.RELEASE_DT::TEXT, b.BIN,
         REGEXP_REPLACE(p.BBL, '\\.0$', ''), NULL, b.FEATURE_CODE, b.GEOM_SOURCE,
         ST_AREA(b.GEOM_GEOG)::FLOAT, NULL::FLOAT, 1::FLOAT, b.HEIGHT_ROOF::FLOAT,
         b.CONSTRUCTION_YEAR::FLOAT, NULL, 'observed', b.LICENSE_TERMS,
         (ST_AREA(ST_INTERSECTION(b.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(b.GEOM_GEOG), 0))::FLOAT
  FROM target p
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= p.BBOX_XMAX AND b.BBOX_XMAX >= p.BBOX_XMIN
   AND b.BBOX_YMIN <= p.BBOX_YMAX AND b.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(b.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(b.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(b.GEOM_GEOG), 0) > 0.5
  UNION ALL
  SELECT 'fema_usa_structures', 'building', f.RELEASE_DT::TEXT, f.PROVIDER_FEATURE_ID,
         REGEXP_REPLACE(p.BBL, '\\.0$', ''), f.PROP_ADDR, f.OCC_CLS, f.PRIM_OCC,
         f.SQFEET::FLOAT, NULL::FLOAT, 1::FLOAT, f.HEIGHT::FLOAT, NULL::FLOAT, NULL,
         COALESCE(f.OCCUPANCY_PROVENANCE, 'modelled'), f.LICENSE_TERMS,
         (ST_AREA(ST_INTERSECTION(f.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(f.GEOM_GEOG), 0))::FLOAT
  FROM target p
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= p.BBOX_XMAX AND f.BBOX_XMAX >= p.BBOX_XMIN
   AND f.BBOX_YMIN <= p.BBOX_YMAX AND f.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(f.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(f.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(f.GEOM_GEOG), 0) > 0.5
  UNION ALL
  SELECT 'microsoft_globalml', 'building', m.RELEASE_DT::TEXT, m.PROVIDER_FEATURE_ID,
         REGEXP_REPLACE(p.BBL, '\\.0$', ''), NULL, NULL, NULL,
         ST_AREA(m.GEOM_GEOG)::FLOAT, NULL::FLOAT, 1::FLOAT, m.HEIGHT::FLOAT,
         NULL::FLOAT, NULL, 'modelled', m.LICENSE_TERMS,
         (ST_AREA(ST_INTERSECTION(m.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(m.GEOM_GEOG), 0))::FLOAT
  FROM target p
  JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= p.BBOX_XMAX AND m.BBOX_XMAX >= p.BBOX_XMIN
   AND m.BBOX_YMIN <= p.BBOX_YMAX AND m.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(m.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(m.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(m.GEOM_GEOG), 0) > 0.5
)
SELECT * FROM observations ORDER BY entity_level, src, member_parcel_bbl, native_id;
```

## Tile Inventory

Anchor point: 2025 `109` geocode point `(-73.958204, 40.720108)`.

| source | level | radius m | rows |
|---|---|---:|---:|
| cmbs_geocode | assertion | 150 | 11 |
| FEMA USA Structures | building | 150 | 20 |
| MapPLUTO | parcel | 150 | 160 |
| Microsoft GlobalML | building | 150 | 34 |
| NYC building footprints active | building | 150 | 180 |
| Overture features | mixed | 150 | 0 |
| cmbs_geocode | assertion | 500 | 33 |
| FEMA USA Structures | building | 500 | 165 |
| MapPLUTO | parcel | 500 | 842 |
| Microsoft GlobalML | building | 500 | 174 |
| NYC building footprints active | building | 500 | 921 |
| Overture features | mixed | 500 | 0 |

SQL:

```sql
WITH anchor AS (SELECT -73.958204::FLOAT AS lon, 40.720108::FLOAT AS lat),
radii AS (SELECT 150 AS radius_m UNION ALL SELECT 500 AS radius_m),
counts AS (
  SELECT 'cmbs_geocode' AS src, 'assertion' AS entity_level, r.radius_m, COUNT(*) AS n
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g
    ON g.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
   AND g.LATITUDE IS NOT NULL AND g.LONGITUDE IS NOT NULL
   AND ST_DWITHIN(ST_POINT(g.LONGITUDE, g.LATITUDE), ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL SELECT 'mappluto', 'parcel', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_DWITHIN(p.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL SELECT 'nyc_building_footprints_active', 'building', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= a.lon + 0.01 AND b.BBOX_XMAX >= a.lon - 0.01
   AND b.BBOX_YMIN <= a.lat + 0.01 AND b.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(b.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL SELECT 'fema_usa_structures', 'building', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= a.lon + 0.01 AND f.BBOX_XMAX >= a.lon - 0.01
   AND f.BBOX_YMIN <= a.lat + 0.01 AND f.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(f.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL SELECT 'microsoft_globalml', 'building', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= a.lon + 0.01 AND m.BBOX_XMAX >= a.lon - 0.01
   AND m.BBOX_YMIN <= a.lat + 0.01 AND m.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(m.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL SELECT 'overture_features', 'mixed', r.radius_m, COUNT(o.PROVIDER_FEATURE_ID)
  FROM radii r CROSS JOIN anchor a
  LEFT JOIN EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
    ON o.STATE = 'NY'
   AND ST_DWITHIN(o.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
)
SELECT * FROM counts ORDER BY radius_m, src, entity_level;
```

## Independent ACRIS Evidence

ACRIS latest-five-per-BBL output confirms the three parcel addresses:

| BBL | example document | street | unit | property_type | good_through |
|---|---|---|---|---|---|
| 3023030027 | 2024121700572001 | 111 NORTH 9TH STREET | 2L | SP | 2024-12-31 |
| 3023030028 | 2025012700925001 | 109 NORTH 9TH STREET | 2L | SP | 2025-01-31 |
| 3023030029 | 2021101900393060 | 107 NORTH 9TH STREET | n/a | AP | 2021-10-31 |

Shared document ids `2021101900393060` and `2021101900393059` appear across all three BBLs,
which is independent evidence that the three lots participate in one recorded property
event.

SQL:

```sql
WITH legals AS (
  SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
         PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER,
         ROW_NUMBER() OVER (
           PARTITION BY BBL
           ORDER BY GOOD_THROUGH_DATE DESC NULLS LAST, DOCUMENT_ID DESC
         ) AS rn
  FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT
  WHERE BBL IN ('3023030027','3023030028','3023030029')
    AND BOROUGH = 3
    AND BLOCK = 2303
    AND LOT IN (27,28,29)
)
SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
       PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER
FROM legals
WHERE rn <= 5
ORDER BY BBL, rn;
```

## Baseline Outputs

| assertion row | naive address | PIP plus tiebreak |
|---|---|---|
| 2025 row parsed 109 | 0 matches | `3023030028` only |
| 2026 row parsed 107 | 0 matches | `3023030029` only |

SQL:

```sql
WITH assertion_rows AS (
  SELECT PROPERTY_ADDRESS, NUMBER, ACCURACY_TYPE, ACCURACY_SCORE, ASOF, LATITUDE,
         LONGITUDE, CASE COUNTY_FIPS WHEN '36047' THEN 'BK' END AS borough
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS = '36047'
    AND (PROPERTY_ADDRESS ILIKE '%107%109%111%N%9%'
         OR PROPERTY_ADDRESS ILIKE '%107%109%111%North%9%')
),
pluto_norm AS (
  SELECT BOROUGH, REGEXP_REPLACE(BBL, '\\.0$', '') AS BBL_NORM, ADDRESS AS pluto_address,
         REGEXP_REPLACE(UPPER(TRIM(ADDRESS)), '[^A-Z0-9]', '') AS norm_addr
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
),
baseline AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.ACCURACY_TYPE, r.ASOF,
         'naive_address_string' AS baseline, COUNT(DISTINCT p.BBL_NORM) AS match_count,
         ARRAY_AGG(DISTINCT p.BBL_NORM) AS bbls,
         ARRAY_AGG(DISTINCT p.pluto_address) AS addresses
  FROM assertion_rows r
  LEFT JOIN pluto_norm p
    ON r.borough = p.BOROUGH
   AND REGEXP_REPLACE(UPPER(TRIM(r.PROPERTY_ADDRESS)), '[^A-Z0-9]', '') = p.norm_addr
  GROUP BY 1,2,3,4,5
  UNION ALL
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.ACCURACY_TYPE, r.ASOF,
         'spatial_pip_then_address_tiebreak',
         COUNT(DISTINCT REGEXP_REPLACE(p.BBL, '\\.0$', '')),
         ARRAY_AGG(DISTINCT REGEXP_REPLACE(p.BBL, '\\.0$', '')),
         ARRAY_AGG(DISTINCT p.ADDRESS)
  FROM assertion_rows r
  LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(r.LONGITUDE, r.LATITUDE))
  GROUP BY 1,2,3,4,5
)
SELECT * FROM baseline ORDER BY ASOF, PROPERTY_ADDRESS, baseline;
```

## Typed Answer

```yaml
case_id: case_3_range_assemblage
as_of:
  assertion_current: 2026-08-01
  mappluto_release: 2026-05-01
  nyc_footprints_release: 2026-08-09
abstention_state: resolved_assemblage
members:
  - level: parcel
    source: mappluto
    native_id: "3023030029"
    address: "107 NORTH 9 STREET"
  - level: parcel
    source: mappluto
    native_id: "3023030028"
    address: "109 NORTH 9 STREET"
  - level: parcel
    source: mappluto
    native_id: "3023030027"
    address: "111 NORTH 9 STREET"
  - level: building
    source: nyc_building_footprints
    native_id: "3061657"
    contained_by: mappluto:3023030029
  - level: building
    source: nyc_building_footprints
    native_id: "3061656"
    contained_by: mappluto:3023030028
  - level: building
    source: nyc_building_footprints
    native_id: "3061655"
    contained_by: mappluto:3023030027
relations:
  - type: address_range_expands_to_member_parcels
    from: assertion.address
    to:
      - mappluto:3023030029
      - mappluto:3023030028
      - mappluto:3023030027
  - type: shared_owner_support
    owner: "107-109-111 NORTH 9TH STREET LLC"
  - type: shared_acris_document_support
    document_ids:
      - "2021101900393060"
      - "2021101900393059"
```

## Reasoning Trace And Ablation

1. Admission: the assertion has a range address and declared geocode tiers, but no size,
   geometry, year, or type.
2. Geocode constraint alone returns one lot at a time: 2025 rows return `109`, and the
   2026 row returns `107`. That is not the asserted property.
3. Range expansion maps the asserted address interval to `107`, `109`, and `111`.
4. MapPLUTO confirms three adjacent lots with identical owner, class, land use, year, and
   one building each.
5. NYC footprints confirm one building per member parcel under Appendix F.
6. ACRIS independently confirms all three addresses/BBLs and shared document ids.

Ablations:

| removed signal | result |
|---|---|
| range parser | false singleton: PIP returns one endpoint or middle lot |
| geocode | range expansion plus MapPLUTO/ACRIS still resolves the three-parcel set |
| shared owner | still likely from address range and ACRIS; loses one support channel |
| ACRIS | still resolvable from MapPLUTO address range, but loses independent anti-circular evidence |
| footprints | parcel assemblage remains; building-level member set is incomplete |

## What Canon Must Do

Operator sequence:

1. Detect a range/split address and preserve it as an interval/member-set constraint.
2. Expand only in a declared locale. This query excluded Queens so hyphenated grid numbers
   are not misparsed as ranges.
3. Evaluate geocode as support for one member, not as the whole answer.
4. Construct an assemblage member set of parcels and attach one building per parcel via
   area-majority.
5. Use size/type/year as assemblage constraints when the client supplies them. They are
   absent here, so MapPLUTO/ACRIS carry adjudication.

What this reveals:

- A BBL column cannot represent the answer: both baselines either return zero or a false
  singleton.
- The address channel is not merely a tie-breaker; for ranges it is the operator that
  changes the answer type from `parcel` to `set<parcel>`.
- PAD is not required for this specific case because MapPLUTO and ACRIS expose all three
  endpoint addresses, but PAD remains required for cases where alternate/frontage address
  membership is not present in MapPLUTO.
