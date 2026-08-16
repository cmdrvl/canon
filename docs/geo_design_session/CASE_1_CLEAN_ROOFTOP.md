# bd-tccn Case 1 - Clean Rooftop

Case property: **One Grace Court Corporation**, `1 Grace Court`, Brooklyn, NY.

Verdict: **resolved singleton**. The adjudicated answer is one property member set:
MapPLUTO parcel `3002510001`, NYC BIN `3002109`, FEMA structure
`e35efe35-fb33-48d3-b6d7-e49a1bca3fc3`, and Microsoft footprint hash
`b14ecb3b56bb9e690fbde70ea470982f343f5b37c62188b51a8dfc2630335186`.

Standing measurements cited, not rederived:

- Appendix E: naive address-string baseline is 28.89%; rooftop PIP hit rate is 99.91%;
  nearest_rooftop_match is the silent-error tier.
- Appendix F: canonical footprint-to-parcel predicate is geometric
  `ST_AREA(intersection)/ST_AREA(footprint) > 0.5`; source asserted area fields are not
  denominators.
- Appendix G: cost model is component-wise; tile-wide feature counts are not the solver
  cost model.

## Source Availability

Returned structured result:

| source | NY rows |
|---|---:|
| FEMA USA Structures | 5,015,922 |
| Microsoft GlobalML | 5,424,624 |
| Overture features | 0 |
| Overture buildings | 0 |
| Overture places | 0 |
| MapPLUTO | 856,614 |
| NYC building footprints | 1,081,999 |

SQL:

```sql
SELECT 'fema' AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT WHERE STATE_FIPS = '36'
UNION ALL SELECT 'microsoft' AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT WHERE STATE = 'NY'
UNION ALL SELECT 'overture_features' AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT WHERE STATE = 'NY'
UNION ALL SELECT 'overture_buildings' AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.OVERTURE_MAPS_BUILDINGS_HOT WHERE STATE = 'NY'
UNION ALL SELECT 'overture_places' AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.OVERTURE_MAPS_PLACES_HOT WHERE STATE = 'NY'
UNION ALL SELECT 'mappluto' AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
UNION ALL SELECT 'nyc_footprints' AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT;
```

## Selection Rule

Reproducible selector: five-borough CMBS geocode row; rooftop; one containing MapPLUTO
lot; no multi-address separators; parsed number/street present; MapPLUTO `NUMBLDGS = 1`;
and Appendix E naive address normalization agrees with MapPLUTO.

First returned row:

| field | value |
|---|---|
| PROPERTY_NAME | One Grace Court Corporation |
| PROPERTY_ADDRESS | 1 Grace Court |
| PROPERTY_CITY / STATE / ZIP | Brooklyn / NY / 11201 |
| PROPERTY_COUNTY | Kings |
| NUMBER / STREET | 1 / Grace Ct |
| ACCURACY_TYPE / SCORE | rooftop / 1.00 |
| SOURCE / ASOF | City of New York / 2025-01-01 |
| LATITUDE / LONGITUDE | 40.6946310 / -73.9984880 |
| BBL_NORM | 3002510001 |
| PLUTO_ADDRESS | 1 GRACE COURT |
| BLDGCLASS / LANDUSE | D4 / 03 |
| NUMBLDGS / NUMFLOORS | 1 / 6 |
| BLDGAREA / LOTAREA | 46,080 / 8,623 |
| YEARBUILT | 1925 |
| OWNERNAME | ONE GRACE COURT CORPORATION |
| RELEASE / RELEASE_DT | 26v1 / 2026-05-01 |

SQL:

```sql
WITH scope AS (
  SELECT *
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
    AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
    AND ACCURACY_TYPE = 'rooftop'
    AND NUMBER IS NOT NULL AND STREET IS NOT NULL
    AND PROPERTY_ADDRESS NOT ILIKE '%/%'
    AND PROPERTY_ADDRESS NOT ILIKE '%&%'
    AND PROPERTY_ADDRESS NOT ILIKE '% AND %'
    AND PROPERTY_ADDRESS NOT ILIKE '%,%'
    AND PROPERTY_ADDRESS NOT ILIKE '% A/K/A %'
),
hits AS (
  SELECT s.PROPERTY_NAME, s.PROPERTY_ADDRESS, s.PROPERTY_CITY, s.PROPERTY_STATE,
         s.PROPERTY_ZIP, s.PROPERTY_COUNTY, s.NUMBER, s.STREET, s.UNIT_TYPE,
         s.UNIT_NUMBER, s.ACCURACY_TYPE, s.ACCURACY_SCORE, s.SOURCE, s.ASOF,
         s.LATITUDE, s.LONGITUDE, p.BBL, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS BBL_NORM,
         p.ADDRESS AS PLUTO_ADDRESS, p.BOROUGH, p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS,
         p.BLDGAREA, p.LOTAREA, p.NUMFLOORS, p.YEARBUILT, p.OWNERNAME, p.RELEASE,
         p.RELEASE_DT,
         REGEXP_REPLACE(UPPER(TRIM(s.PROPERTY_ADDRESS)), '[^A-Z0-9]', '') AS ASSERTION_ADDR_NORM,
         REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '') AS PLUTO_ADDR_NORM
  FROM scope s
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
),
one_lot AS (
  SELECT PROPERTY_ADDRESS, SOURCE, ASOF, LATITUDE, LONGITUDE,
         COUNT(DISTINCT BBL_NORM) AS lots_containing
  FROM hits
  GROUP BY 1,2,3,4,5
  HAVING COUNT(DISTINCT BBL_NORM) = 1
)
SELECT h.*
FROM hits h
JOIN one_lot ol
  ON h.PROPERTY_ADDRESS = ol.PROPERTY_ADDRESS
 AND h.SOURCE = ol.SOURCE
 AND h.ASOF = ol.ASOF
 AND h.LATITUDE = ol.LATITUDE
 AND h.LONGITUDE = ol.LONGITUDE
WHERE h.NUMBLDGS = 1
  AND h.ASSERTION_ADDR_NORM = h.PLUTO_ADDR_NORM
ORDER BY h.ACCURACY_SCORE DESC, h.PROPERTY_ADDRESS
LIMIT 10;
```

## Assertion Row - Six Contract Fields

| contract field | case value |
|---|---|
| geocode | present: `(-73.9984880, 40.6946310)`, `accuracy_type=rooftop`, `accuracy_score=1.00`, source `City of New York`, `ASOF=2025-01-01` |
| address | present: raw `1 Grace Court`; parsed `NUMBER=1`, `STREET=Grace Ct`; locale NYC/Brooklyn |
| geometry | absent in CMBS geocode row |
| building size | absent in CMBS geocode row |
| year built | absent in CMBS geocode row |
| property type | absent in CMBS geocode row |

Absence is not failure under bd-uilx. In this case, geocode and address are enough to
admit the tile; MapPLUTO and footprints supply observations, not client assertions.

## Tile Inventory

Anchor point: `(-73.998488, 40.694631)`. The 150 m radius is the local evidence tile; the
500 m radius is the staged r8-like work-unit proxy from `scripts/geo_corpus/02_tile_assembly.sql`.

| source | level | radius m | rows |
|---|---|---:|---:|
| cmbs_geocode | assertion | 150 | 5 |
| FEMA USA Structures | building | 150 | 26 |
| MapPLUTO | parcel | 150 | 108 |
| Microsoft GlobalML | building | 150 | 26 |
| NYC building footprints active | building | 150 | 108 |
| Overture features | mixed | 150 | 0 |
| cmbs_geocode | assertion | 500 | 59 |
| FEMA USA Structures | building | 500 | 137 |
| MapPLUTO | parcel | 500 | 921 |
| Microsoft GlobalML | building | 500 | 144 |
| NYC building footprints active | building | 500 | 937 |
| Overture features | mixed | 500 | 0 |

SQL:

```sql
WITH anchor AS (SELECT -73.998488::FLOAT AS lon, 40.694631::FLOAT AS lat),
radii AS (SELECT 150 AS radius_m UNION ALL SELECT 500 AS radius_m),
counts AS (
  SELECT 'cmbs_geocode' AS src, 'assertion' AS entity_level, r.radius_m, COUNT(*) AS n
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g
    ON g.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
   AND g.LATITUDE IS NOT NULL AND g.LONGITUDE IS NOT NULL
   AND ST_DWITHIN(ST_POINT(g.LONGITUDE, g.LATITUDE), ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT 'mappluto', 'parcel', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_DWITHIN(p.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT 'nyc_building_footprints_active', 'building', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= a.lon + 0.01 AND b.BBOX_XMAX >= a.lon - 0.01
   AND b.BBOX_YMIN <= a.lat + 0.01 AND b.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(b.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT 'fema_usa_structures', 'building', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= a.lon + 0.01 AND f.BBOX_XMAX >= a.lon - 0.01
   AND f.BBOX_YMIN <= a.lat + 0.01 AND f.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(f.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT 'microsoft_globalml', 'building', r.radius_m, COUNT(*)
  FROM radii r CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= a.lon + 0.01 AND m.BBOX_XMAX >= a.lon - 0.01
   AND m.BBOX_YMIN <= a.lat + 0.01 AND m.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(m.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT 'overture_features', 'mixed', r.radius_m, COUNT(o.PROVIDER_FEATURE_ID)
  FROM radii r CROSS JOIN anchor a
  LEFT JOIN EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
    ON o.STATE = 'NY'
   AND ST_DWITHIN(o.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
)
SELECT * FROM counts ORDER BY radius_m, src, entity_level;
```

## Property Observations

Rows directly tied to target parcel `3002510001` under the canonical Appendix F
footprint majority predicate:

| source | level | vintage | native id | address/class | source area | height/floors | year | area ratio to parcel | contains anchor |
|---|---|---|---|---|---:|---:|---:|---:|---|
| FEMA USA Structures | building | 2025-06-06 | e35efe35-fb33-48d3-b6d7-e49a1bca3fc3 | 1 GRACE COURT / Residential / Multi - Family Dwelling | 5,482.625 sq ft | 19.52 m | n/a | 0.989260 | true |
| MapPLUTO | parcel | 2026-05-01 | 3002510001 | 1 GRACE COURT / D4 / 03 | 46,080 sq ft | 6 floors | 1925 | n/a | true |
| Microsoft GlobalML | building | 2026-07-24 | b14ecb3b56bb9e690fbde70ea470982f343f5b37c62188b51a8dfc2630335186 | n/a | 638.600744 sq m | 18.783552 m | n/a | 0.960002 | true |
| NYC building footprints | building | 2026-08-09 | BIN 3002109 | feature 2100 / Photogrammetric | 567.994821 sq m | 67.28 ft roof | 1925 | 0.981303 | true |

Sanity gates:

| gate | count |
|---|---:|
| containing_mappluto_lots_for_geocode | 1 |
| fema_structures_majority_to_target | 1 |
| microsoft_footprints_majority_to_target | 1 |
| nyc_active_footprints_majority_to_target | 1 |
| overture_features_in_ny | 0 |

Observation SQL:

```sql
WITH anchor AS (SELECT -73.998488::FLOAT AS lon, 40.694631::FLOAT AS lat),
target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE REGEXP_REPLACE(p.BBL, '\\.0$', '') = '3002510001'
),
observations AS (
  SELECT 'mappluto' AS src, 'parcel' AS entity_level, p.RELEASE_DT::TEXT AS src_vintage,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS native_id, p.ADDRESS AS src_address,
         p.BLDGCLASS AS src_class, p.LANDUSE AS src_landuse, p.BLDGAREA::FLOAT AS src_area,
         p.LOTAREA::FLOAT AS lot_area, p.NUMBLDGS::FLOAT AS num_buildings,
         p.NUMFLOORS::FLOAT AS height_or_floors, p.YEARBUILT::FLOAT AS year_built,
         p.OWNERNAME AS owner_or_name, 'observed' AS attr_provenance,
         NULL::TEXT AS license_terms, NULL::FLOAT AS area_ratio_to_target_parcel,
         ST_CONTAINS(p.GEOM_GEOG, ST_POINT(a.lon, a.lat)) AS contains_anchor,
         ST_DISTANCE(p.GEOM_GEOG, ST_POINT(a.lon, a.lat)) AS metres_from_anchor
  FROM target p CROSS JOIN anchor a
  UNION ALL
  SELECT 'nyc_building_footprints', 'building', b.RELEASE_DT::TEXT, b.BIN,
         NULL, b.FEATURE_CODE, b.GEOM_SOURCE, ST_AREA(b.GEOM_GEOG)::FLOAT,
         NULL::FLOAT, 1::FLOAT, b.HEIGHT_ROOF::FLOAT, b.CONSTRUCTION_YEAR::FLOAT,
         NULL, 'observed', b.LICENSE_TERMS,
         (ST_AREA(ST_INTERSECTION(b.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(b.GEOM_GEOG), 0))::FLOAT,
         ST_CONTAINS(b.GEOM_GEOG, ST_POINT(a.lon, a.lat)),
         ST_DISTANCE(b.GEOM_GEOG, ST_POINT(a.lon, a.lat))
  FROM target p CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= p.BBOX_XMAX AND b.BBOX_XMAX >= p.BBOX_XMIN
   AND b.BBOX_YMIN <= p.BBOX_YMAX AND b.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(b.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(b.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(b.GEOM_GEOG), 0) > 0.5
  UNION ALL
  SELECT 'fema_usa_structures', 'building', f.RELEASE_DT::TEXT, f.PROVIDER_FEATURE_ID,
         f.PROP_ADDR, f.OCC_CLS, f.PRIM_OCC, f.SQFEET::FLOAT, NULL::FLOAT, 1::FLOAT,
         f.HEIGHT::FLOAT, NULL::FLOAT, NULL, COALESCE(f.OCCUPANCY_PROVENANCE, 'modelled'),
         f.LICENSE_TERMS,
         (ST_AREA(ST_INTERSECTION(f.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(f.GEOM_GEOG), 0))::FLOAT,
         ST_CONTAINS(f.GEOM_GEOG, ST_POINT(a.lon, a.lat)),
         ST_DISTANCE(f.GEOM_GEOG, ST_POINT(a.lon, a.lat))
  FROM target p CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= p.BBOX_XMAX AND f.BBOX_XMAX >= p.BBOX_XMIN
   AND f.BBOX_YMIN <= p.BBOX_YMAX AND f.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(f.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(f.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(f.GEOM_GEOG), 0) > 0.5
  UNION ALL
  SELECT 'microsoft_globalml', 'building', m.RELEASE_DT::TEXT, m.PROVIDER_FEATURE_ID,
         NULL, NULL, NULL, ST_AREA(m.GEOM_GEOG)::FLOAT, NULL::FLOAT, 1::FLOAT,
         m.HEIGHT::FLOAT, NULL::FLOAT, NULL, 'modelled', m.LICENSE_TERMS,
         (ST_AREA(ST_INTERSECTION(m.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(m.GEOM_GEOG), 0))::FLOAT,
         ST_CONTAINS(m.GEOM_GEOG, ST_POINT(a.lon, a.lat)),
         ST_DISTANCE(m.GEOM_GEOG, ST_POINT(a.lon, a.lat))
  FROM target p CROSS JOIN anchor a
  JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= p.BBOX_XMAX AND m.BBOX_XMAX >= p.BBOX_XMIN
   AND m.BBOX_YMIN <= p.BBOX_YMAX AND m.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(m.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(m.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(m.GEOM_GEOG), 0) > 0.5
)
SELECT * FROM observations ORDER BY src, native_id;
```

Sanity SQL:

```sql
WITH anchor AS (SELECT -73.998488::FLOAT AS lon, 40.694631::FLOAT AS lat),
target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE REGEXP_REPLACE(p.BBL, '\\.0$', '') = '3002510001'
),
gates AS (
  SELECT 'containing_mappluto_lots_for_geocode' AS gate,
         COUNT(DISTINCT REGEXP_REPLACE(p.BBL, '\\.0$', '')) AS n
  FROM anchor a
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(a.lon, a.lat))
  UNION ALL
  SELECT 'nyc_active_footprints_majority_to_target', COUNT(*)
  FROM target p
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= p.BBOX_XMAX AND b.BBOX_XMAX >= p.BBOX_XMIN
   AND b.BBOX_YMIN <= p.BBOX_YMAX AND b.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(b.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(b.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(b.GEOM_GEOG), 0) > 0.5
  UNION ALL
  SELECT 'fema_structures_majority_to_target', COUNT(*)
  FROM target p
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= p.BBOX_XMAX AND f.BBOX_XMAX >= p.BBOX_XMIN
   AND f.BBOX_YMIN <= p.BBOX_YMAX AND f.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(f.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(f.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(f.GEOM_GEOG), 0) > 0.5
  UNION ALL
  SELECT 'microsoft_footprints_majority_to_target', COUNT(*)
  FROM target p
  JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= p.BBOX_XMAX AND m.BBOX_XMAX >= p.BBOX_XMIN
   AND m.BBOX_YMIN <= p.BBOX_YMAX AND m.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(m.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(m.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(m.GEOM_GEOG), 0) > 0.5
  UNION ALL
  SELECT 'overture_features_in_ny', COUNT(*)
  FROM EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
  WHERE o.STATE = 'NY'
)
SELECT * FROM gates ORDER BY gate;
```

## Independent ACRIS Evidence

ACRIS legals were queried by BBL plus borough/block/lot. The query returned 20 rows under
the limit; latest five shown here.

| document_id | BBL | street | unit | property_type | good_through_date |
|---|---|---|---|---|---|
| 2025102400176001 | 3002510001 | 1 GRACE COURT | 4C | SP | 2025-11-30 |
| 2023011300874001 | 3002510001 | 1 GRACE COURT | 6A | SP | 2023-01-31 |
| 2022091501252001 | 3002510001 | 1 GRACE COURT | 3C | SP | 2022-10-31 |
| 2022090100094001 | 3002510001 | 1 GRACE CT | 6D | SP | 2022-09-30 |
| 2022031400958001 | 3002510001 | 1 GRACE COURT | 5D | SP | 2022-03-31 |

SQL:

```sql
SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
       PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER
FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT
WHERE BBL = '3002510001'
  AND BOROUGH = 3
  AND BLOCK = 251
  AND LOT = 1
ORDER BY GOOD_THROUGH_DATE DESC NULLS LAST, DOCUMENT_ID DESC
LIMIT 20;
```

ACRIS master/party rows for the latest five legal documents:

| document_id | doc_type | document_date | amount | CRFN | parties |
|---|---|---:|---:|---|---|
| 2025102400176001 | RPTT&RET | 2025-10-20 | 570,000 | 2025000300316 | SHOTBOLT/SON parties |
| 2023011300874001 | RPTT&RET | 2022-11-07 | 0 | 2023000014758 | WAGMAN / trustees |
| 2022090100094001 | RPTT&RET | 2022-08-25 | 680,000 | 2022000367960 | POLENBERG / EGAN |
| 2022091501252001 | RPTT&RET | 2022-08-23 | 600,000 | 2022000379914 | POST/PRESTON / MCGILL parties |
| 2022031400958001 | RPTT&RET | 2022-03-01 | 700,000 | 2022000119892 | ENGEL / WINKLER |

SQL:

```sql
WITH docs AS (
  SELECT COLUMN1 AS DOCUMENT_ID
  FROM VALUES
    ('2025102400176001'),
    ('2023011300874001'),
    ('2022091501252001'),
    ('2022090100094001'),
    ('2022031400958001')
),
master_rows AS (
  SELECT m.DOCUMENT_ID, m.DOC_TYPE, m.DOCUMENT_DATE, m.DOCUMENT_AMT,
         m.RECORDED_DATETIME, m.CRFN
  FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT m
  JOIN docs d ON m.DOCUMENT_ID = d.DOCUMENT_ID
),
party_rows AS (
  SELECT p.DOCUMENT_ID,
         ARRAY_AGG(DISTINCT p.PARTY_TYPE || ':' || p.NAME)
           WITHIN GROUP (ORDER BY p.PARTY_TYPE || ':' || p.NAME) AS PARTIES
  FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT p
  JOIN docs d ON p.DOCUMENT_ID = d.DOCUMENT_ID
  GROUP BY 1
)
SELECT mr.DOCUMENT_ID, mr.DOC_TYPE, mr.DOCUMENT_DATE, mr.DOCUMENT_AMT,
       mr.RECORDED_DATETIME, mr.CRFN, pr.PARTIES
FROM master_rows mr
LEFT JOIN party_rows pr ON mr.DOCUMENT_ID = pr.DOCUMENT_ID
ORDER BY mr.DOCUMENT_DATE DESC NULLS LAST, mr.RECORDED_DATETIME DESC NULLS LAST;
```

Adjudication note: ACRIS is independent of the CMBS geocode assertion and confirms the
same BBL/address family, including both `COURT` and `CT` spellings. It is not used as the
resolver in this case; it is the anti-circular check.

## Baseline Outputs

| baseline | match count | BBLs | addresses |
|---|---:|---|---|
| naive_address_string | 1 | 3002510001 | 1 GRACE COURT |
| spatial_pip_then_address_tiebreak | 1 | 3002510001 | 1 GRACE COURT |

SQL:

```sql
WITH assertion AS (
  SELECT '1 Grace Court' AS property_address, '36047' AS county_fips, 'BK' AS borough,
         -73.998488::FLOAT AS lon, 40.694631::FLOAT AS lat
),
cm_norm AS (
  SELECT property_address, county_fips, borough,
         REGEXP_REPLACE(UPPER(TRIM(property_address)), '[^A-Z0-9]', '') AS norm_addr,
         lon, lat
  FROM assertion
),
pluto_norm AS (
  SELECT BOROUGH, REGEXP_REPLACE(BBL, '\\.0$', '') AS BBL_NORM, ADDRESS AS pluto_address,
         REGEXP_REPLACE(UPPER(TRIM(ADDRESS)), '[^A-Z0-9]', '') AS norm_addr
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
),
naive_edges AS (
  SELECT c.property_address, p.BBL_NORM, p.pluto_address
  FROM cm_norm c
  JOIN pluto_norm p
    ON c.borough = p.BOROUGH
   AND c.norm_addr = p.norm_addr
),
pip_edges AS (
  SELECT c.property_address, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS BBL_NORM,
         p.ADDRESS AS pluto_address
  FROM cm_norm c
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(c.lon, c.lat))
)
SELECT 'naive_address_string' AS baseline, COUNT(DISTINCT BBL_NORM) AS match_count,
       ARRAY_AGG(DISTINCT BBL_NORM) AS bbls, ARRAY_AGG(DISTINCT pluto_address) AS addresses
FROM naive_edges
UNION ALL
SELECT 'spatial_pip_then_address_tiebreak' AS baseline, COUNT(DISTINCT BBL_NORM) AS match_count,
       ARRAY_AGG(DISTINCT BBL_NORM) AS bbls, ARRAY_AGG(DISTINCT pluto_address) AS addresses
FROM pip_edges;
```

## Typed Answer

```yaml
case_id: case_1_clean_rooftop
as_of:
  assertion: 2025-01-01
  mappluto_release: 2026-05-01
  nyc_footprints_release: 2026-08-09
  fema_release: 2025-06-06
  microsoft_release: 2026-07-24
abstention_state: resolved_singleton
members:
  - level: parcel
    source: mappluto
    native_id: "3002510001"
  - level: building
    source: nyc_building_footprints
    native_id: "3002109"
  - level: building
    source: fema_usa_structures
    native_id: "e35efe35-fb33-48d3-b6d7-e49a1bca3fc3"
    role: corroborating_footprint
  - level: building
    source: microsoft_globalml
    native_id: "b14ecb3b56bb9e690fbde70ea470982f343f5b37c62188b51a8dfc2630335186"
    role: aliasless_corroborating_footprint
relations:
  - type: geocode_point_within_parcel
    from: assertion.geocode
    to: mappluto:3002510001
    count_at_level: 1
  - type: area_majority_footprint_within_parcel
    from: nyc_building_footprints:3002109
    to: mappluto:3002510001
    area_ratio: 0.981303143280889
  - type: area_majority_footprint_within_parcel
    from: fema_usa_structures:e35efe35-fb33-48d3-b6d7-e49a1bca3fc3
    to: mappluto:3002510001
    area_ratio: 0.9892604134423281
  - type: area_majority_footprint_within_parcel
    from: microsoft_globalml:b14ecb3b56bb9e690fbde70ea470982f343f5b37c62188b51a8dfc2630335186
    to: mappluto:3002510001
    area_ratio: 0.9600021832122758
```

## Reasoning Trace And Ablation

1. Admission: the assertion has a declared rooftop geocode tier and a raw address with
   parsed components. Geometry, building size, year built, and property type are absent.
2. Geocode constraint: the rooftop point is contained by exactly one MapPLUTO parcel,
   `3002510001`.
3. Address constraint: Appendix E normalization maps `1 Grace Court` to the same parcel
   address, `1 GRACE COURT`; the naive baseline also returns exactly one BBL.
4. Building constraint: Appendix F area-majority links exactly one NYC footprint, one FEMA
   structure, and one Microsoft footprint to the same parcel; all contain the rooftop
   point.
5. Independent check: ACRIS legals for BBL `3002510001` carry `1 GRACE COURT` / `1 GRACE
   CT`, confirming the BBL/address family without using the geocode algorithm.

Ablations:

| removed signal | result |
|---|---|
| address | still a singleton at parcel level from rooftop PIP; footprints still corroborate building level |
| geocode | naive address-string still returns singleton `3002510001`; no building-level link without spatial footprint step |
| NYC footprints | parcel still resolves; building-level answer loses the observed BIN and falls back to FEMA/Microsoft corroboration |
| FEMA | no change to singleton; confirms Appendix F behavior that FEMA refines without being load-bearing in this clean case |
| Microsoft | no change to singleton; Microsoft is aliasless corroboration only |
| ACRIS | algorithmic answer unchanged; independent anti-circular evidence removed |

## What Canon Must Do

Operator sequence:

1. Validate six-field contract and record absent fields explicitly.
2. Map rooftop geocode through its declared relaxation: a point/small high-confidence
   geometry constraint, not an untyped lat/lon.
3. Generate bounded tile candidates; `within` returns a set, even when the set has size 1.
4. Normalize source identifiers for comparison only: MapPLUTO raw `3002510001.0` and
   normalized `3002510001` must both be preserved.
5. Apply the Appendix F area-majority predicate using geometric denominators only.
6. Accept `resolved_singleton` only after all available channels either agree or are
   absent; do not average disagreement away.
7. Carry source vintage and license/provenance per observation. FEMA occupancy is modelled
   support evidence, not a hard eliminative fact.

What this reveals:

- This is the ablation control: both baselines get the row right, and the constraint model
  also reaches a singleton with building-level corroboration.
- Even the clean property has a nearby duplicate assertion spelling (`1 GRACE CT`) that
  fails Appendix E's naive normalization. The case selection must pin the exact assertion
  row; otherwise the baseline result changes while the property does not.
- Overture contributes no NYC evidence here because its landed NY count is zero. That is a
  source-availability finding, not an abstention.
