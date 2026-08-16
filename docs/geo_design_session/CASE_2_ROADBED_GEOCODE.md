# bd-tccn Case 2 - Roadbed Geocode

Case property: **982 Madison Street**, Brooklyn, NY.

Verdict: **resolved singleton by address constraint; geocode alone abstains**. The
interpolated point is in the street and no MapPLUTO parcel contains it. The address channel
selects MapPLUTO parcel `3033660028`, and NYC footprint BIN `3422548` majority-links to
that parcel. FEMA and Microsoft do not supply majority-linked corroborating footprints for
the target parcel.

Standing measurements cited, not rederived:

- Appendix E: range_interpolation has only 53.02% PIP hit rate and 13.94% house-number
  agreement among comparable hits.
- Appendix F: footprint-to-parcel edges use the geometric area denominator:
  `ST_AREA(intersection)/ST_AREA(footprint) > 0.5`.
- Appendix G: dense tiles are costed by components, not by raw tile row count.

## Selection Rule

Reproducible selector: five-borough CMBS geocode row; `accuracy_type` in
`range_interpolation`, `street_center`, or `intersection`; no multi-address separators;
parsed number/street present; zero containing MapPLUTO lots; an Appendix E naive address
match within 75 m; and a measured 50 m neighborhood.

First returned row:

| field | value |
|---|---|
| PROPERTY_NAME | 982 Madison Street |
| PROPERTY_ADDRESS | 982 Madison Street |
| PROPERTY_CITY / STATE / ZIP | Brooklyn / NY / 11221 |
| PROPERTY_COUNTY | Kings |
| NUMBER / STREET | 982 / Madison St |
| ACCURACY_TYPE / SCORE | range_interpolation / 1.00 |
| SOURCE / ASOF | TIGER/Line from the US Census Bureau / 2026-01-01 |
| LATITUDE / LONGITUDE | 40.6891460 / -73.9189260 |
| address-selected BBL | 3033660028 |
| PLUTO_ADDRESS | 982 MADISON STREET |
| BLDGCLASS / LANDUSE | D3 / 03 |
| NUMBLDGS / NUMFLOORS | 1 / 5 |
| BLDGAREA / LOTAREA | 14,745 / 5,625 |
| YEARBUILT | 2017 |
| OWNERNAME | 982 MADISON GROUP LLC |
| address-selected parcel distance | 10.809029 m |
| lots within 50 m | 58 |
| nearest lot distance | 5.969016 m |

The nearest-lot probe is not the address-selected lot: it is BBL `3033570147`,
`981 MADISON STREET`.

SQL:

```sql
WITH scope AS (
  SELECT *
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
    AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
    AND ACCURACY_TYPE IN ('range_interpolation','street_center','intersection')
    AND NUMBER IS NOT NULL AND STREET IS NOT NULL
    AND PROPERTY_ADDRESS NOT ILIKE '%/%'
    AND PROPERTY_ADDRESS NOT ILIKE '%&%'
    AND PROPERTY_ADDRESS NOT ILIKE '% AND %'
    AND PROPERTY_ADDRESS NOT ILIKE '%,%'
    AND PROPERTY_ADDRESS NOT ILIKE '% A/K/A %'
),
containment AS (
  SELECT s.PROPERTY_NAME, s.PROPERTY_ADDRESS, s.PROPERTY_CITY, s.PROPERTY_STATE,
         s.PROPERTY_ZIP, s.PROPERTY_COUNTY, s.COUNTY_FIPS, s.NUMBER, s.STREET,
         s.ACCURACY_TYPE, s.ACCURACY_SCORE, s.SOURCE, s.ASOF, s.LATITUDE, s.LONGITUDE,
         COUNT(DISTINCT p.BBL) AS containing_lots
  FROM scope s
  LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
  GROUP BY 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
),
uncontained AS (
  SELECT * FROM containment WHERE containing_lots = 0
),
address_matches AS (
  SELECT u.PROPERTY_NAME, u.PROPERTY_ADDRESS, u.PROPERTY_CITY, u.PROPERTY_STATE,
         u.PROPERTY_ZIP, u.PROPERTY_COUNTY, u.COUNTY_FIPS, u.NUMBER, u.STREET,
         u.ACCURACY_TYPE, u.ACCURACY_SCORE, u.SOURCE, u.ASOF, u.LATITUDE, u.LONGITUDE,
         p.BOROUGH, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS BBL_NORM,
         p.ADDRESS AS PLUTO_ADDRESS, p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS, p.BLDGAREA,
         p.LOTAREA, p.NUMFLOORS, p.YEARBUILT, p.OWNERNAME,
         ST_DISTANCE(p.GEOM_GEOG, ST_POINT(u.LONGITUDE, u.LATITUDE)) AS distance_m
  FROM uncontained u
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.BOROUGH = CASE u.COUNTY_FIPS
                     WHEN '36005' THEN 'BX'
                     WHEN '36047' THEN 'BK'
                     WHEN '36061' THEN 'MN'
                     WHEN '36081' THEN 'QN'
                     WHEN '36085' THEN 'SI'
                   END
   AND REGEXP_REPLACE(UPPER(TRIM(u.PROPERTY_ADDRESS)), '[^A-Z0-9]', '') =
       REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '')
   AND ST_DWITHIN(p.GEOM_GEOG, ST_POINT(u.LONGITUDE, u.LATITUDE), 75)
),
neighbourhood AS (
  SELECT u.PROPERTY_ADDRESS, u.SOURCE, u.ASOF, u.LATITUDE, u.LONGITUDE,
         COUNT(DISTINCT p.BBL) AS lots_within_50m,
         MIN(ST_DISTANCE(p.GEOM_GEOG, ST_POINT(u.LONGITUDE, u.LATITUDE))) AS nearest_lot_m,
         ARRAY_AGG(DISTINCT REGEXP_REPLACE(p.BBL, '\\.0$', ''))
           WITHIN GROUP (ORDER BY REGEXP_REPLACE(p.BBL, '\\.0$', '')) AS bbls_within_50m
  FROM uncontained u
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_DWITHIN(p.GEOM_GEOG, ST_POINT(u.LONGITUDE, u.LATITUDE), 50)
  GROUP BY 1,2,3,4,5
)
SELECT a.*, n.lots_within_50m, n.nearest_lot_m, n.bbls_within_50m
FROM address_matches a
JOIN neighbourhood n
  ON a.PROPERTY_ADDRESS = n.PROPERTY_ADDRESS
 AND a.SOURCE = n.SOURCE
 AND a.ASOF = n.ASOF
 AND a.LATITUDE = n.LATITUDE
 AND a.LONGITUDE = n.LONGITUDE
ORDER BY n.lots_within_50m DESC, a.distance_m ASC, a.PROPERTY_ADDRESS
LIMIT 25;
```

## Assertion Row - Six Contract Fields

| contract field | case value |
|---|---|
| geocode | present: `(-73.9189260, 40.6891460)`, `accuracy_type=range_interpolation`, `accuracy_score=1.00`, source `TIGER/Line from the US Census Bureau`, `ASOF=2026-01-01` |
| address | present: raw `982 Madison Street`; parsed `NUMBER=982`, `STREET=Madison St`; locale NYC/Brooklyn |
| geometry | absent in CMBS geocode row |
| building size | absent in CMBS geocode row |
| year built | absent in CMBS geocode row |
| property type | absent in CMBS geocode row |

The geocode is present but not a point containment claim. Under rho it must become a
relaxed line/interpolation uncertainty constraint, not a parcel winner.

## Tile Inventory

Anchor point: `(-73.918926, 40.689146)`.

| source | level | radius m | rows |
|---|---|---:|---:|
| cmbs_geocode | assertion | 150 | 7 |
| FEMA USA Structures | building | 150 | 28 |
| MapPLUTO | parcel | 150 | 212 |
| Microsoft GlobalML | building | 150 | 32 |
| NYC building footprints active | building | 150 | 205 |
| Overture features | mixed | 150 | 0 |
| cmbs_geocode | assertion | 500 | 20 |
| FEMA USA Structures | building | 500 | 327 |
| MapPLUTO | parcel | 500 | 1,992 |
| Microsoft GlobalML | building | 500 | 276 |
| NYC building footprints active | building | 500 | 1,987 |
| Overture features | mixed | 500 | 0 |

SQL:

```sql
WITH anchor AS (SELECT -73.918926::FLOAT AS lon, 40.689146::FLOAT AS lat),
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

Rows directly tied to target parcel `3033660028`:

| source | level | native id | address/class | source area | height/floors | year | area ratio to parcel | contains anchor | distance from anchor |
|---|---|---|---|---:|---:|---:|---:|---|---:|
| MapPLUTO | parcel | 3033660028 | 982 MADISON STREET / D3 / 03 | 14,745 sq ft | 5 floors | 2017 | n/a | false | 10.809029 m |
| NYC building footprints | building | BIN 3422548 | feature 2100 / Other (Manual) | 319.647012 sq m | 70 ft roof | 2019 | 0.934267 | false | 12.515539 m |

Explicit gate counts:

| gate | count |
|---|---:|
| containing_mappluto_lots_for_geocode | 0 |
| mappluto_lots_within_50m | 58 |
| nyc_active_footprints_majority_to_target | 1 |
| fema_structures_majority_to_target | 0 |
| microsoft_footprints_majority_to_target | 0 |
| overture_features_in_ny | 0 |

Observation SQL:

```sql
WITH anchor AS (SELECT -73.918926::FLOAT AS lon, 40.689146::FLOAT AS lat),
target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE REGEXP_REPLACE(p.BBL, '\\.0$', '') = '3033660028'
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

Gate SQL:

```sql
WITH anchor AS (SELECT -73.918926::FLOAT AS lon, 40.689146::FLOAT AS lat),
target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE REGEXP_REPLACE(p.BBL, '\\.0$', '') = '3033660028'
),
gates AS (
  SELECT 'containing_mappluto_lots_for_geocode' AS gate,
         COUNT(DISTINCT REGEXP_REPLACE(p.BBL, '\\.0$', '')) AS n
  FROM anchor a
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, ST_POINT(a.lon, a.lat))
  UNION ALL
  SELECT 'mappluto_lots_within_50m',
         COUNT(DISTINCT REGEXP_REPLACE(p.BBL, '\\.0$', ''))
  FROM anchor a
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_DWITHIN(p.GEOM_GEOG, ST_POINT(a.lon, a.lat), 50)
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
  LEFT JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= p.BBOX_XMAX AND f.BBOX_XMAX >= p.BBOX_XMIN
   AND f.BBOX_YMIN <= p.BBOX_YMAX AND f.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(f.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(f.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(f.GEOM_GEOG), 0) > 0.5
  WHERE f.PROVIDER_FEATURE_ID IS NOT NULL
  UNION ALL
  SELECT 'microsoft_footprints_majority_to_target', COUNT(*)
  FROM target p
  LEFT JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= p.BBOX_XMAX AND m.BBOX_XMAX >= p.BBOX_XMIN
   AND m.BBOX_YMIN <= p.BBOX_YMAX AND m.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(m.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(m.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(m.GEOM_GEOG), 0) > 0.5
  WHERE m.PROVIDER_FEATURE_ID IS NOT NULL
  UNION ALL
  SELECT 'overture_features_in_ny', COUNT(*)
  FROM EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
  WHERE o.STATE = 'NY'
)
SELECT * FROM gates ORDER BY gate;
```

## Independent ACRIS Evidence

ACRIS legals were queried by BBL plus borough/block/lot. The query returned 20 rows under
the limit; latest six shown here because they demonstrate the address range on the same
BBL.

| document_id | BBL | street | property_type | good_through_date |
|---|---|---|---|---|
| 2026042100861008 | 3033660028 | 982 MADISON STREET | CR | 2026-04-30 |
| 2026042100861007 | 3033660028 | 984 MADISON STREET | CR | 2026-04-30 |
| 2026042100861005 | 3033660028 | 982 MADISON STREET | CR | 2026-04-30 |
| 2026042100861004 | 3033660028 | 984 MADISON STREET | CR | 2026-04-30 |
| 2026042100861002 | 3033660028 | 982 MADISON STREET | CR | 2026-04-30 |
| 2026042100861001 | 3033660028 | 984 MADISON STREET | CR | 2026-04-30 |

SQL:

```sql
SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
       PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER
FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT
WHERE BBL = '3033660028'
  AND BOROUGH = 3
  AND BLOCK = 3366
  AND LOT = 28
ORDER BY GOOD_THROUGH_DATE DESC NULLS LAST, DOCUMENT_ID DESC
LIMIT 20;
```

ACRIS master/party rows for the latest five legal documents:

| document_id | doc_type | document_date | CRFN | parties |
|---|---|---:|---|---|
| 2026042100861005 | AALR | 2026-02-12 | 2026000112015 | RMF SUB 5, LLC / LMF COMMERCIAL, LLC |
| 2026042100861007 | ASST | 2026-02-12 | 2026000112017 | LMF COMMERCIAL, LLC / trustee for BBCMS Mortgage Trust 2026-5C40 |
| 2026042100861004 | ASST | 2026-02-12 | 2026000112014 | RMF SUB 5, LLC / LMF COMMERCIAL, LLC |
| 2026042100861008 | AALR | 2026-02-12 | 2026000112018 | LMF COMMERCIAL, LLC / trustee for BBCMS Mortgage Trust 2026-5C40 |
| 2026042100861002 | AALR | 2025-10-24 | 2026000112012 | LMF COMMERCIAL, LLC / RMF SUB 5, LLC |

SQL:

```sql
WITH docs AS (
  SELECT COLUMN1 AS DOCUMENT_ID
  FROM VALUES
    ('2026042100861008'),
    ('2026042100861007'),
    ('2026042100861005'),
    ('2026042100861004'),
    ('2026042100861002')
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

Adjudication note: ACRIS independently ties BBL `3033660028` to `982`, `984`, and
`982-984 MADISON STREET`, and the latest master rows carry commercial mortgage trust
assignments. This supports the address-selected parcel without using the geocode point.

## Baseline Outputs

| baseline | match count | BBLs | addresses | nearest distance |
|---|---:|---|---|---:|
| naive_address_string | 1 | 3033660028 | 982 MADISON STREET | n/a |
| spatial_pip_then_address_tiebreak | 0 | none | none | n/a |
| forbidden_snap_to_nearest_probe | 1 | 3033570147 | 981 MADISON STREET | 5.969016 m |

SQL:

```sql
WITH assertion AS (
  SELECT '982 Madison Street' AS property_address, '36047' AS county_fips, 'BK' AS borough,
         -73.918926::FLOAT AS lon, 40.689146::FLOAT AS lat
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
),
nearest AS (
  SELECT c.property_address, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS nearest_bbl,
         p.ADDRESS AS nearest_address,
         ST_DISTANCE(p.GEOM_GEOG, ST_POINT(c.lon, c.lat)) AS distance_m
  FROM cm_norm c
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_DWITHIN(p.GEOM_GEOG, ST_POINT(c.lon, c.lat), 50)
  QUALIFY ROW_NUMBER() OVER (
    ORDER BY ST_DISTANCE(p.GEOM_GEOG, ST_POINT(c.lon, c.lat)),
             REGEXP_REPLACE(p.BBL, '\\.0$', '')
  ) = 1
)
SELECT 'naive_address_string' AS baseline, COUNT(DISTINCT BBL_NORM) AS match_count,
       ARRAY_AGG(DISTINCT BBL_NORM) AS bbls, ARRAY_AGG(DISTINCT pluto_address) AS addresses,
       NULL::FLOAT AS nearest_distance_m
FROM naive_edges
UNION ALL
SELECT 'spatial_pip_then_address_tiebreak', COUNT(DISTINCT BBL_NORM),
       ARRAY_AGG(DISTINCT BBL_NORM), ARRAY_AGG(DISTINCT pluto_address), NULL::FLOAT
FROM pip_edges
UNION ALL
SELECT 'forbidden_snap_to_nearest_probe', COUNT(*), ARRAY_AGG(nearest_bbl),
       ARRAY_AGG(nearest_address), MIN(distance_m)
FROM nearest;
```

## Typed Answer

```yaml
case_id: case_2_roadbed_geocode
as_of:
  assertion: 2026-01-01
  mappluto_release: 2026-05-01
  nyc_footprints_release: 2026-08-09
abstention_state: resolved_singleton_after_geocode_abstention
members:
  - level: parcel
    source: mappluto
    native_id: "3033660028"
  - level: building
    source: nyc_building_footprints
    native_id: "3422548"
relations:
  - type: geocode_point_within_parcel
    from: assertion.geocode
    to: []
    count_at_level: 0
  - type: address_exact_after_appendix_e_normalization
    from: assertion.address
    to: mappluto:3033660028
    count_at_level: 1
  - type: area_majority_footprint_within_parcel
    from: nyc_building_footprints:3422548
    to: mappluto:3033660028
    area_ratio: 0.9342667911458009
  - type: forbidden_nearest_lot
    from: assertion.geocode
    to: mappluto:3033570147
    reason: nearest parcel is not the address-selected parcel
```

## Reasoning Trace And Ablation

1. Admission: the assertion has a declared range interpolation geocode and a parsed raw
   address. Geometry/size/year/type are absent.
2. Geocode constraint: point containment returns the empty set. Canon must abstain at this
   stage; it must not snap.
3. Tile candidate constraint: the 50 m neighborhood has 58 MapPLUTO lots, so proximity is
   not discriminating.
4. Address constraint: Appendix E naive normalization selects exactly one parcel,
   `3033660028`.
5. Building constraint: one NYC footprint majority-links to that address-selected parcel.
   FEMA and Microsoft are absent for this target, so they cannot corroborate.
6. Independent check: ACRIS legals confirm BBL `3033660028` at `982`, `984`, and `982-984`
   Madison Street.

Ablations:

| removed signal | result |
|---|---|
| address | unresolved: geocode has zero containing parcels and 58 lots within 50 m |
| geocode | address-string baseline still resolves to `3033660028`, but loses evidence that this is the roadbed/interpolation tier |
| no-snap rule | wrong parcel risk materializes: nearest probe picks `3033570147` / `981 MADISON STREET` |
| NYC footprints | parcel still resolves by address, but building-level member set loses BIN `3422548` |
| ACRIS | algorithmic answer unchanged; independent confirmation of the address-selected BBL is removed |

## What Canon Must Do

Operator sequence:

1. Admit `range_interpolation` as a relaxed spatial constraint, not as parcel containment.
2. Generate a bounded tile candidate set; here the observed 50 m set is 58 parcels.
3. Refuse nearest-lot collapse. The closest parcel is demonstrably not the address parcel.
4. Apply address as a constraint over the bounded set, preserving the fact that it is the
   deciding channel.
5. Attach building-level evidence only after the parcel is selected, using Appendix F's
   geometric denominator.
6. Report absent corroboration from FEMA/Microsoft as source absence, not disagreement.

What this reveals:

- This is the canonical street-interpolation workload. PIP gives zero, naive address gives
  the answer, and nearest-lot snapping gives a wrong parcel.
- The solver must represent an intermediate abstention: geocode channel abstains, then the
  address channel resolves within the tile.
- PAD/address-set acquisition is not needed to resolve this particular single-address row,
  but it becomes load-bearing for multi-frontage variants of the same shape.
