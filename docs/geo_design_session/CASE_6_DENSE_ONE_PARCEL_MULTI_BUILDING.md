# bd-tccn Case 6 - Dense One-Parcel Multi-Building

Case property: **305 East 72nd Street, Manhattan, NY**.

Verdict: **parcel singleton, building-level residual**. The parcel answer is
`1014477501` / `305 EAST 72 STREET`, but that parcel contains two NYC building
footprints:

- BIN `1076314`, 162 ft roof height, 875.728228 sq m footprint
- BIN `1085187`, 66 ft roof height, 756.981289 sq m footprint

The CMBS rooftop point falls inside BIN `1076314`, but the assertion field names a
through-block property with multiple addresses on East 72nd Street, Second Avenue, and East
73rd Street. If the product question is "which parcel?", the answer is a singleton. If the
question is "which building?", a BBL-only answer false-merges two buildings unless the
solver is explicitly allowed to interpret the rooftop point as building-level evidence.

Standing measurements cited, not rederived:

- Appendix B: proximity percolates in this exact dense tile; fixed-radius adjacency is not
  a safe decomposer.
- Appendix E: geocode tier and address baselines have silent-error populations.
- Appendix F: footprint-to-parcel edges use geometric
  `ST_AREA(intersection)/ST_AREA(footprint) > 0.5`.
- Appendix G: parcel-star sizing reached high local degree; this case is exactly why the
  cost model is component-wise rather than tile-wide.

## Selection Rule

The selector enumerated rooftop geocode assertions with multi-frontage address text whose
containing MapPLUTO parcel reports `NUMBLDGS >= 2`.

Relevant selector output:

| rank | property | address | BBL | MapPLUTO address | NUMBLDGS | BLDGAREA | LOTAREA | year |
|---:|---|---|---|---|---:|---:|---:|---:|
| 1 | Crocheron Tenants Corp. | 36-21 170th Street, ... | `4053010017` | 36-31 170 STREET | 3 | 16,229 | 27,680 | 1951 |
| 2 | 305 EAST 72ND STREET | 305 EAST 72ND STREET, A/K/A ... | `1014477501` | 305 EAST 72 STREET | 2 | 194,949 | 37,800 | 1961 |
| 3 | 305 East 72nd Street | 305 East 72nd Street, A/K/A ... | `1014477501` | 305 EAST 72 STREET | 2 | 194,949 | 37,800 | 1961 |
| 4 | Southridge Cooperative, Section 1, Inc. | 33-05 92nd Street, ... | `4014400001` | 33-25 92 STREET | 6 | 395,940 | 115,000 | 1958 |

The selected candidate is the first Manhattan through-block candidate with `NUMBLDGS=2`
and a compact, already-measured dense tile.

Selector SQL:

```sql
WITH assertion_rows AS (
  SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, PROPERTY_STATE, PROPERTY_ZIP,
         PROPERTY_COUNTY, COUNTY_FIPS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
         SOURCE, ASOF, LATITUDE, LONGITUDE, ST_POINT(LONGITUDE, LATITUDE) AS pt,
         LENGTH(PROPERTY_ADDRESS) AS address_len
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
    AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
    AND ACCURACY_TYPE = 'rooftop'
    AND (PROPERTY_ADDRESS ILIKE '%A/K/A%' OR PROPERTY_ADDRESS ILIKE '%,%')
), contains_edges AS (
  SELECT r.*, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm, p.ADDRESS AS mappluto_address,
         p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS, p.BLDGAREA, p.LOTAREA, p.NUMFLOORS,
         p.YEARBUILT, p.OWNERNAME
  FROM assertion_rows r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, r.pt)
)
SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, COUNTY_FIPS, NUMBER, STREET,
       ACCURACY_TYPE, ACCURACY_SCORE, SOURCE, ASOF, LATITUDE, LONGITUDE,
       bbl_norm, mappluto_address, BLDGCLASS, LANDUSE, NUMBLDGS, BLDGAREA,
       LOTAREA, NUMFLOORS, YEARBUILT, OWNERNAME, address_len
FROM contains_edges
WHERE NUMBLDGS >= 2
ORDER BY address_len DESC, BLDGAREA DESC, PROPERTY_ADDRESS
LIMIT 25;
```

The first attempt used `p.UNITSTOTAL` and failed because that column is not present in
`NYC_DCP_MAPPLUTO_HOT`. The corrected selector above drops that column.

## Assertion Rows - Six Contract Fields

| address variant | parsed | accuracy | score | asof | lat | lon | containing BBL |
|---|---|---|---:|---|---:|---:|---|
| upper case | 305 E 72nd St | rooftop | 0.95 | 2025-01-01 | 40.7688430 | -73.9580550 | `1014477501` |
| title case | 305 E 72nd St | rooftop | 0.95 | 2025-01-01 | 40.7688430 | -73.9580550 | `1014477501` |

| contract field | case value |
|---|---|
| geocode | present: rooftop point `(-73.9580550, 40.7688430)` |
| address | present: five asserted frontages/ranges across three streets |
| geometry | absent in CMBS geocode row |
| building size | absent in CMBS geocode row |
| year built | absent in CMBS geocode row |
| property type | absent in CMBS geocode row |

## Baseline Outputs

| assertion variant | containing lots | containing BBL/address | NUMBLDGS | raw exact matches | parsed exact matches |
|---|---:|---|---:|---:|---:|
| upper case | 1 | `1014477501` / 305 EAST 72 STREET | 2 | 0 | 0 |
| title case | 1 | `1014477501` / 305 EAST 72 STREET | 2 | 0 | 0 |

Point-in-polygon resolves the parcel but does not by itself answer building-level identity.
Raw and parsed exact address matching do not recover the parcel because the input carries
range and frontage text and the geocode parser emits abbreviated `E 72nd St`.

SQL:

```sql
WITH assertion_rows AS (
  SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, PROPERTY_STATE, PROPERTY_ZIP,
         PROPERTY_COUNTY, COUNTY_FIPS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
         SOURCE, ASOF, LATITUDE, LONGITUDE, ST_POINT(LONGITUDE, LATITUDE) AS pt,
         REGEXP_REPLACE(UPPER(TRIM(PROPERTY_ADDRESS)), '[^A-Z0-9]', '') AS raw_addr_norm,
         REGEXP_REPLACE(UPPER(TRIM(NUMBER || ' ' || STREET)), '[^A-Z0-9]', '') AS parsed_addr_norm
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS = '36061'
    AND PROPERTY_ADDRESS ILIKE '%305%EAST%72%1392%1396%2ND%AVENUE%'
), contains_edges AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ASOF, r.LATITUDE, r.LONGITUDE,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm, p.ADDRESS,
         p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS, p.BLDGAREA, p.LOTAREA, p.NUMFLOORS,
         p.YEARBUILT, p.OWNERNAME
  FROM assertion_rows r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, r.pt)
), raw_address_matches AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ASOF, r.LATITUDE, r.LONGITUDE,
         COUNT(p.BBL) AS raw_exact_address_matches
  FROM assertion_rows r
  LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.BOROUGH = 'MN'
   AND REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '') = r.raw_addr_norm
  GROUP BY 1,2,3,4,5,6
), parsed_address_matches AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ASOF, r.LATITUDE, r.LONGITUDE,
         COUNT(p.BBL) AS parsed_exact_address_matches
  FROM assertion_rows r
  LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.BOROUGH = 'MN'
   AND REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '') = r.parsed_addr_norm
  GROUP BY 1,2,3,4,5,6
)
SELECT r.PROPERTY_NAME, r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ACCURACY_TYPE,
       r.ACCURACY_SCORE, r.SOURCE, r.ASOF, r.LATITUDE, r.LONGITUDE,
       COUNT(DISTINCT ce.bbl_norm) AS containing_lots,
       ce.bbl_norm AS containing_bbl, ce.ADDRESS AS containing_address,
       ce.BLDGCLASS, ce.LANDUSE, ce.NUMBLDGS, ce.BLDGAREA, ce.LOTAREA,
       ce.NUMFLOORS, ce.YEARBUILT, ce.OWNERNAME,
       ram.raw_exact_address_matches, pam.parsed_exact_address_matches
FROM assertion_rows r
LEFT JOIN contains_edges ce
  ON r.PROPERTY_ADDRESS = ce.PROPERTY_ADDRESS AND r.NUMBER = ce.NUMBER
 AND r.STREET = ce.STREET AND r.ASOF = ce.ASOF
 AND r.LATITUDE = ce.LATITUDE AND r.LONGITUDE = ce.LONGITUDE
LEFT JOIN raw_address_matches ram
  ON r.PROPERTY_ADDRESS = ram.PROPERTY_ADDRESS AND r.NUMBER = ram.NUMBER
 AND r.STREET = ram.STREET AND r.ASOF = ram.ASOF
 AND r.LATITUDE = ram.LATITUDE AND r.LONGITUDE = ram.LONGITUDE
LEFT JOIN parsed_address_matches pam
  ON r.PROPERTY_ADDRESS = pam.PROPERTY_ADDRESS AND r.NUMBER = pam.NUMBER
 AND r.STREET = pam.STREET AND r.ASOF = pam.ASOF
 AND r.LATITUDE = pam.LATITUDE AND r.LONGITUDE = pam.LONGITUDE
GROUP BY r.PROPERTY_NAME, r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ACCURACY_TYPE,
         r.ACCURACY_SCORE, r.SOURCE, r.ASOF, r.LATITUDE, r.LONGITUDE,
         ce.bbl_norm, ce.ADDRESS, ce.BLDGCLASS, ce.LANDUSE, ce.NUMBLDGS,
         ce.BLDGAREA, ce.LOTAREA, ce.NUMFLOORS, ce.YEARBUILT, ce.OWNERNAME,
         ram.raw_exact_address_matches, pam.parsed_exact_address_matches
ORDER BY r.PROPERTY_NAME;
```

## Address-Set Probes

MapPLUTO carries only the primary frontage:

| probe address | probe type | BBL | MapPLUTO address | match |
|---|---|---|---|---:|
| 305 EAST 72 STREET | primary_frontage | `1014477501` | 305 EAST 72 STREET | 1 |
| 301 EAST 72 STREET | asserted_range_endpoint | n/a | n/a | 0 |
| 303 EAST 72 STREET | asserted_range_middle | n/a | n/a | 0 |
| 301-305 EAST 72 STREET | asserted_range_token | n/a | n/a | 0 |
| 1392 2 AVENUE | asserted_range_endpoint | n/a | n/a | 0 |
| 1394 2 AVENUE | asserted_range_middle | n/a | n/a | 0 |
| 1396 2 AVENUE | asserted_range_endpoint | n/a | n/a | 0 |
| 1398 2 AVENUE | asserted_range_endpoint | n/a | n/a | 0 |
| 1400 2 AVENUE | asserted_range_middle | n/a | n/a | 0 |
| 1402 2 AVENUE | asserted_range_endpoint | n/a | n/a | 0 |
| 300 EAST 73 STREET | asserted_range_endpoint | n/a | n/a | 0 |
| 302 EAST 73 STREET | asserted_range_endpoint | n/a | n/a | 0 |
| 300-302 EAST 73 STREET | asserted_range_token | n/a | n/a | 0 |

SQL:

```sql
WITH probes AS (
  SELECT '305 EAST 72 STREET' AS probe_address, 'primary_frontage' AS probe_type UNION ALL
  SELECT '301 EAST 72 STREET', 'asserted_range_endpoint' UNION ALL
  SELECT '303 EAST 72 STREET', 'asserted_range_middle' UNION ALL
  SELECT '301-305 EAST 72 STREET', 'asserted_range_token' UNION ALL
  SELECT '1392 2 AVENUE', 'asserted_range_endpoint' UNION ALL
  SELECT '1394 2 AVENUE', 'asserted_range_middle' UNION ALL
  SELECT '1396 2 AVENUE', 'asserted_range_endpoint' UNION ALL
  SELECT '1398 2 AVENUE', 'asserted_range_endpoint' UNION ALL
  SELECT '1400 2 AVENUE', 'asserted_range_middle' UNION ALL
  SELECT '1402 2 AVENUE', 'asserted_range_endpoint' UNION ALL
  SELECT '300 EAST 73 STREET', 'asserted_range_endpoint' UNION ALL
  SELECT '302 EAST 73 STREET', 'asserted_range_endpoint' UNION ALL
  SELECT '300-302 EAST 73 STREET', 'asserted_range_token'
), norm AS (
  SELECT probe_address, probe_type,
         REGEXP_REPLACE(UPPER(TRIM(probe_address)), '[^A-Z0-9]', '') AS addr_norm
  FROM probes
), joined AS (
  SELECT n.probe_address, n.probe_type,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm,
         p.ADDRESS AS mappluto_address
  FROM norm n
  LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.BOROUGH = 'MN'
   AND REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '') = n.addr_norm
)
SELECT probe_address, probe_type, bbl_norm, mappluto_address,
       IFF(bbl_norm IS NULL, 0, 1) AS has_match
FROM joined
ORDER BY probe_type, probe_address, bbl_norm;
```

## Source Observations

Predicate-C source observations:

| source | native id | level | area | height/floors | year | area ratio | source address |
|---|---|---|---:|---:|---:|---:|---|
| MapPLUTO | `1014477501` | parcel | 194,949 sq ft | 17 floors | 1961 | n/a | 305 EAST 72 STREET |
| NYC footprint | `1076314` | building | 875.728228 sq m | 162 ft | 1961 | 1.000000 | n/a |
| NYC footprint | `1085187` | building | 756.981289 sq m | 66 ft | 1961 | 1.000000 | n/a |
| FEMA | `061bdd6d-55e6-43d8-bc15-6a4bebb3927d` | building | 11,387.761719 sq ft | 18.44 ft | n/a | 0.554774 | 1400 2 AVENUE |

Microsoft returned zero majority-linked rows for the parcel. Overture remains empty in the
tile inventory below.

SQL:

```sql
WITH target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE REGEXP_REPLACE(p.BBL, '\\.0$', '') = '1014477501'
), observations AS (
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

## Geocode Point Versus Building Footprints

The rooftop point selects one building footprint if the source-specific relaxation permits
that interpretation:

| assertion variant | BIN | contains point | distance to footprint m | distance to footprint centroid m |
|---|---|---:|---:|---:|
| upper case | `1076314` | 1 | 0.000000 | 18.564685 |
| upper case | `1085187` | 0 | 34.329666 | 49.034781 |
| title case | `1076314` | 1 | 0.000000 | 18.564685 |
| title case | `1085187` | 0 | 34.329666 | 49.034781 |

SQL:

```sql
WITH assertion_rows AS (
  SELECT PROPERTY_NAME, PROPERTY_ADDRESS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
         SOURCE, ASOF, LATITUDE, LONGITUDE, ST_POINT(LONGITUDE, LATITUDE) AS pt
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS = '36061'
    AND PROPERTY_ADDRESS ILIKE '%305%EAST%72%1392%1396%2ND%AVENUE%'
), target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE REGEXP_REPLACE(p.BBL, '\\.0$', '') = '1014477501'
), bldgs AS (
  SELECT b.BIN, b.HEIGHT_ROOF, b.CONSTRUCTION_YEAR, ST_AREA(b.GEOM_GEOG) AS footprint_area_sqm,
         ST_CENTROID(b.GEOM_GEOG) AS centroid_geog, b.GEOM_GEOG
  FROM target p
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= p.BBOX_XMAX AND b.BBOX_XMAX >= p.BBOX_XMIN
   AND b.BBOX_YMIN <= p.BBOX_YMAX AND b.BBOX_YMAX >= p.BBOX_YMIN
   AND ST_INTERSECTS(b.GEOM_GEOG, p.GEOM_GEOG)
   AND ST_AREA(ST_INTERSECTION(b.GEOM_GEOG, p.GEOM_GEOG)) / NULLIF(ST_AREA(b.GEOM_GEOG), 0) > 0.5
)
SELECT r.PROPERTY_NAME, r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ACCURACY_TYPE,
       r.ACCURACY_SCORE, r.ASOF, r.LATITUDE, r.LONGITUDE,
       b.BIN, b.HEIGHT_ROOF, b.CONSTRUCTION_YEAR, b.footprint_area_sqm,
       IFF(ST_CONTAINS(b.GEOM_GEOG, r.pt), 1, 0) AS footprint_contains_geocode,
       ST_DISTANCE(b.GEOM_GEOG, r.pt) AS distance_to_footprint_m,
       ST_DISTANCE(b.centroid_geog, r.pt) AS distance_to_footprint_centroid_m
FROM assertion_rows r
CROSS JOIN bldgs b
ORDER BY r.PROPERTY_NAME, b.BIN;
```

## Independent ACRIS Evidence

Direct ACRIS lookup on MapPLUTO BBL `1014477501` returned a structured zero:

```sql
WITH legals AS (
  SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
         PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER,
         ROW_NUMBER() OVER (
           PARTITION BY BBL
           ORDER BY GOOD_THROUGH_DATE DESC NULLS LAST, DOCUMENT_ID DESC
         ) AS rn
  FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT
  WHERE BBL = '1014477501'
    AND BOROUGH = 1
    AND BLOCK = 1447
    AND LOT = 7501
)
SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
       PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER
FROM legals
WHERE rn <= 10
ORDER BY rn;
```

The broader block/address probe returned `30` rows, all on condo unit BBLs such as
`1014471002` and `1014471001`, not the MapPLUTO `7501` BBL. The first rows are:

| BBL | document | street | unit | type | good_through |
|---|---|---|---|---|---|
| `1014471002` | 2026061700624001 | 305 EAST 72 STREET | 3HN | SP | 2026-06-30 |
| `1014471002` | 2026052000609001 | 305 EAST 72ND STREET | 10-DS | SP | 2026-05-31 |
| `1014471002` | 2026042800823001 | 305 EAST 72 STREET | 13H | SP | 2026-04-30 |
| `1014471002` | 2026042000074001 | 305 EAST 72 STREET | 3DS | SP | 2026-04-30 |
| `1014471002` | 2026032600998001 | 305 EAST 72 STREET | 4CN | SP | 2026-03-31 |
| `1014471001` | 2024121600890001 | 305 EAST 72 STREET | 16-B | SP | 2024-12-31 |

SQL:

```sql
SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
       PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER
FROM EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT
WHERE BOROUGH = 1
  AND BLOCK = 1447
  AND (
    (STREET_NUMBER IN ('300','301','302','303','305') AND UPPER(STREET_NAME) LIKE '%7%2%ST%')
    OR (STREET_NUMBER IN ('1392','1394','1396','1398','1400','1402') AND UPPER(STREET_NAME) LIKE '%2%AVENUE%')
    OR BBL = '1014477501'
  )
ORDER BY GOOD_THROUGH_DATE DESC NULLS LAST, DOCUMENT_ID DESC, BBL
LIMIT 30;
```

## Tile Inventory

Anchor point: rooftop geocode `(-73.958055, 40.768843)`.

| radius m | CMBS assertions | MapPLUTO parcels | NYC footprints | FEMA | Microsoft | Overture |
|---:|---:|---:|---:|---:|---:|---:|
| 150 | 14 | 120 | 106 | 23 | 0 | 0 |
| 500 | 105 | 1073 | 1116 | 164 | 21 | 0 |

SQL:

```sql
WITH anchor AS (
  SELECT -73.958055::FLOAT AS lon, 40.768843::FLOAT AS lat
), radii AS (
  SELECT 150::FLOAT AS radius_m UNION ALL SELECT 500::FLOAT
), counts AS (
  SELECT r.radius_m, 'cmbs_geocode' AS src, 'assertion' AS level, COUNT(*) AS n_rows
  FROM anchor a CROSS JOIN radii r
  JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g
    ON g.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
   AND g.LATITUDE IS NOT NULL AND g.LONGITUDE IS NOT NULL
   AND ST_DWITHIN(ST_POINT(g.LONGITUDE, g.LATITUDE), ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT r.radius_m, 'mappluto' AS src, 'parcel' AS level, COUNT(*) AS n_rows
  FROM anchor a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.BBOX_XMIN <= a.lon + 0.01 AND p.BBOX_XMAX >= a.lon - 0.01
   AND p.BBOX_YMIN <= a.lat + 0.01 AND p.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(p.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT r.radius_m, 'nyc_building_footprints_active' AS src, 'building' AS level, COUNT(*) AS n_rows
  FROM anchor a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= a.lon + 0.01 AND b.BBOX_XMAX >= a.lon - 0.01
   AND b.BBOX_YMIN <= a.lat + 0.01 AND b.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(b.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT r.radius_m, 'fema_usa_structures' AS src, 'building' AS level, COUNT(*) AS n_rows
  FROM anchor a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= a.lon + 0.01 AND f.BBOX_XMAX >= a.lon - 0.01
   AND f.BBOX_YMIN <= a.lat + 0.01 AND f.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(f.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT r.radius_m, 'microsoft_globalml' AS src, 'building' AS level, COUNT(*) AS n_rows
  FROM anchor a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= a.lon + 0.01 AND m.BBOX_XMAX >= a.lon - 0.01
   AND m.BBOX_YMIN <= a.lat + 0.01 AND m.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(m.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
  UNION ALL
  SELECT r.radius_m, 'overture_features' AS src, 'mixed' AS level, COUNT(o.GEOM_GEOG) AS n_rows
  FROM anchor a CROSS JOIN radii r
  LEFT JOIN EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
    ON ST_DWITHIN(o.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3
)
SELECT * FROM counts ORDER BY radius_m, src;
```

The Microsoft `150m=0` row was recorded by a follow-up left-join query because the
inventory query above used inner joins for non-Overture nonzero bins:

```sql
WITH anchor AS (
  SELECT -73.958055::FLOAT AS lon, 40.768843::FLOAT AS lat
), radii AS (
  SELECT 150::FLOAT AS radius_m UNION ALL SELECT 500::FLOAT
)
SELECT r.radius_m, 'microsoft_globalml' AS src, 'building' AS level,
       COUNT(m.GEOM_GEOG) AS n_rows
FROM anchor a CROSS JOIN radii r
LEFT JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
  ON m.STATE = 'NY'
 AND m.BBOX_XMIN <= a.lon + 0.01 AND m.BBOX_XMAX >= a.lon - 0.01
 AND m.BBOX_YMIN <= a.lat + 0.01 AND m.BBOX_YMAX >= a.lat - 0.01
 AND ST_DWITHIN(m.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
GROUP BY 1,2,3
ORDER BY radius_m;
```

## Sanity Gates

- Selector retry was successful after dropping absent `UNITSTOTAL`; the failed query is
  recorded as a column-shape finding.
- Baseline rows reconcile: `2` assertion rows, both `containing_lots=1`, both with
  MapPLUTO `NUMBLDGS=2`.
- Address probes reconcile: `13` probes, `1` positive MapPLUTO match and `12` structured
  zeroes.
- Predicate-C observations reconcile: `4` rows = one MapPLUTO parcel, two NYC footprints,
  and one FEMA footprint. Microsoft contributes no majority edge.
- Geocode-to-footprint rows reconcile: `2` assertion variants times `2` NYC footprints =
  `4` rows.
- ACRIS direct lookup on `1014477501` returns `0`; broader block/address lookup returns
  `30` condo-unit legal rows.
- Tile inventory uses explicit Microsoft follow-up for the missing `150m=0` bin.

## Design Decision Forced

Case 6 forces **entity-level output**. A parcel-level solution is not enough when multiple
buildings live on one parcel. The residual answer must be able to say:

- parcel identity: resolved to `1014477501`;
- building candidates under that parcel: `1076314` and `1085187`;
- geocode-derived building candidate: `1076314`, only under a source-specific rooftop
  relaxation;
- missing evidence: PAD/per-BIN address set, tenant/POI, or collateral-specific size/type
  attributes are needed to decide whether the whole property or only one building is meant.

This is the false-merge case: a BBL-only canonical ID would join all corpora on
`1014477501` even if later evidence refers to different buildings on the same parcel.
