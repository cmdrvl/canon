# bd-tccn Case 4 - Chimera Multi-Street Address

Case property: **199, 201, 203, 205 First Avenue and 349 & 351 East 12th
Street**, Manhattan, NY.

Verdict: **resolved multi-address assemblage core, with an address-set gap**.
The landed evidence supports six member parcels:

- `1004540045` / `197 1 AVENUE`
- `1004540044` / `199 1 AVENUE`
- `1004540043` / `201 1 AVENUE`
- `1004540042` / `203 1 AVENUE`
- `1004540041` / `205 1 AVENUE`
- `1004540046` / `349 EAST 12 STREET`

The asserted `351` and `353 EAST 12 STREET` aliases do not appear as MapPLUTO
primary addresses, and NYC PAD or another per-building/per-frontage address-set source is
not landed. The answer should therefore carry the six-parcel core plus an unresolved alias
membership question, not collapse to one BBL.

The geocoder parsed `NUMBER=199` from a First Avenue member and `STREET=E 12th St` from a
different member. The synthesized parsed address `199 EAST 12 STREET` is not asserted by
the source field and has zero MapPLUTO matches. Canon geo must treat this as a chimera
parse and refuse the parsed address as corroboration.

Standing measurements cited, not rederived:

- Appendix E: `nearest_rooftop_match` and other geocode tiers have silent-error behavior;
  naive address-string matching has low coverage.
- Appendix F: footprint-to-parcel edges use geometric
  `ST_AREA(intersection)/ST_AREA(footprint) > 0.5`; source asserted area fields are never
  denominators.
- Appendix G: the cost model is component-wise; this is a small six-parcel component
  inside dense Manhattan tiles.

## Selection Rule

The selector enumerated five-borough geocode rows with multi-address separators and at
least two street tokens, ordered by address-field complexity. The first two returned rows
are the same property in upper/title case variants.

Structured selector output:

| rank | property_name | parsed number | parsed street | accuracy | score | asof | lat | lon | address_len | street_tokens |
|---:|---|---:|---|---|---:|---|---:|---:|---:|---:|
| 1 | 199, 201, 203, 205 FIRST AVENUE AND 349 & 351 EAST 12TH STREET | 199 | E 12th St | range_interpolation | 1.00 | 2025-01-01 | 40.7320950 | -73.9885340 | 194 | 9 |
| 2 | 199, 201, 203, 205 First Avenue and 349 & 351 East 12th Street | 199 | E 12th St | range_interpolation | 1.00 | 2025-01-01 | 40.7320950 | -73.9885340 | 194 | 9 |
| 3 | Alley Pond Owners Corp. | n/a | 78th Rd | street_center | 0.86 | 2025-05-01 | 40.7048620 | -73.8673030 | 165 | 5 |
| 4 | Forest Hills South Owners, Inc. | 77-15 | 113th St | rooftop | 0.95 | 2025-01-01 | 40.7165140 | -73.8320180 | 161 | 8 |
| 5 | Crocheron Tenants Corp. | 36-21 | 170th St | rooftop | 0.95 | 2025-01-01 | 40.7621610 | -73.7959160 | 154 | 7 |

Selector SQL:

```sql
SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, PROPERTY_STATE, PROPERTY_ZIP,
       PROPERTY_COUNTY, COUNTY_FIPS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
       SOURCE, ASOF, LATITUDE, LONGITUDE,
       LENGTH(PROPERTY_ADDRESS) AS address_len,
       REGEXP_COUNT(UPPER(PROPERTY_ADDRESS), '(STREET|ST\.|AVENUE|AVE\.|ROAD|RD\.|BROADWAY|BOULEVARD|BLVD\.|PLACE|PL\.|DRIVE|DR\.|PARKWAY|PKWY)') AS street_token_count
FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
  AND PROPERTY_ADDRESS IS NOT NULL
  AND LATITUDE IS NOT NULL
  AND LONGITUDE IS NOT NULL
  AND (PROPERTY_ADDRESS ILIKE '%,%' OR PROPERTY_ADDRESS ILIKE '% A/K/A %'
       OR PROPERTY_ADDRESS ILIKE '% AND %' OR PROPERTY_ADDRESS ILIKE '% & %')
  AND REGEXP_COUNT(UPPER(PROPERTY_ADDRESS), '(STREET|ST\.|AVENUE|AVE\.|ROAD|RD\.|BROADWAY|BOULEVARD|BLVD\.|PLACE|PL\.|DRIVE|DR\.|PARKWAY|PKWY)') >= 2
ORDER BY address_len DESC, PROPERTY_ADDRESS
LIMIT 5;
```

## Assertion Row - Six Contract Fields

| contract field | case value |
|---|---|
| geocode | present: `(-73.9885340, 40.7320950)`, `accuracy_type=range_interpolation`, `accuracy_score=1.00`, `ASOF=2025-01-01` |
| address | present: raw multi-address field; parser emits `NUMBER=199`, `STREET=E 12th St` |
| geometry | absent in CMBS geocode row |
| building size | absent in CMBS geocode row |
| year built | absent in CMBS geocode row |
| property type | absent in CMBS geocode row |

## Baseline Outputs

Both title/upper-case assertion rows have the same baseline outcome.

| assertion variant | containing lots | nearest lot within 75m | nearest distance m | raw exact address matches | parsed exact address matches |
|---|---:|---|---:|---:|---:|
| upper case | 0 | `1005567502` / 84 3 AVENUE | 5.254258 | 0 | 0 |
| title case | 0 | `1005567502` / 84 3 AVENUE | 5.254258 | 0 | 0 |

This is a triple failure: point-in-polygon has no parcel, snap-to-nearest chooses an
unrelated Third Avenue lot, and the parsed address does not exist in MapPLUTO.

SQL:

```sql
WITH assertion_rows AS (
  SELECT PROPERTY_NAME, PROPERTY_ADDRESS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
         SOURCE, ASOF, LATITUDE, LONGITUDE, ST_POINT(LONGITUDE, LATITUDE) AS pt,
         REGEXP_REPLACE(UPPER(TRIM(PROPERTY_ADDRESS)), '[^A-Z0-9]', '') AS raw_addr_norm,
         REGEXP_REPLACE(UPPER(TRIM(NUMBER || ' ' || STREET)), '[^A-Z0-9]', '') AS parsed_addr_norm
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS = '36061'
    AND PROPERTY_ADDRESS ILIKE '%199%FIRST AVENUE%349%12TH STREET%'
), contains_edges AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ASOF, r.LATITUDE, r.LONGITUDE,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm, p.ADDRESS
  FROM assertion_rows r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, r.pt)
), nearest_edges AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ASOF, r.LATITUDE, r.LONGITUDE,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm, p.ADDRESS,
         ST_DISTANCE(p.GEOM_GEOG, r.pt) AS distance_m
  FROM assertion_rows r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_DWITHIN(p.GEOM_GEOG, r.pt, 75)
  QUALIFY ROW_NUMBER() OVER (
    PARTITION BY r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ASOF, r.LATITUDE, r.LONGITUDE
    ORDER BY ST_DISTANCE(p.GEOM_GEOG, r.pt), REGEXP_REPLACE(p.BBL, '\\.0$', '')
  ) = 1
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
       ne.bbl_norm AS nearest_bbl, ne.ADDRESS AS nearest_address,
       ne.distance_m AS nearest_distance_m,
       ram.raw_exact_address_matches, pam.parsed_exact_address_matches
FROM assertion_rows r
LEFT JOIN contains_edges ce
  ON r.PROPERTY_ADDRESS = ce.PROPERTY_ADDRESS AND r.NUMBER = ce.NUMBER
 AND r.STREET = ce.STREET AND r.ASOF = ce.ASOF
 AND r.LATITUDE = ce.LATITUDE AND r.LONGITUDE = ce.LONGITUDE
LEFT JOIN nearest_edges ne
  ON r.PROPERTY_ADDRESS = ne.PROPERTY_ADDRESS AND r.NUMBER = ne.NUMBER
 AND r.STREET = ne.STREET AND r.ASOF = ne.ASOF
 AND r.LATITUDE = ne.LATITUDE AND r.LONGITUDE = ne.LONGITUDE
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
         ne.bbl_norm, ne.ADDRESS, ne.distance_m,
         ram.raw_exact_address_matches, pam.parsed_exact_address_matches
ORDER BY r.PROPERTY_NAME;
```

## Address-Set Probes

| probe address | probe type | BBL | MapPLUTO address | match |
|---|---|---|---|---:|
| 197 1 AVENUE | asserted_member | `1004540045` | 197 1 AVENUE | 1 |
| 199 1 AVENUE | asserted_member | `1004540044` | 199 1 AVENUE | 1 |
| 201 1 AVENUE | asserted_member | `1004540043` | 201 1 AVENUE | 1 |
| 203 1 AVENUE | asserted_member | `1004540042` | 203 1 AVENUE | 1 |
| 205 1 AVENUE | asserted_member | `1004540041` | 205 1 AVENUE | 1 |
| 349 EAST 12 STREET | asserted_member | `1004540046` | 349 EAST 12 STREET | 1 |
| 351 EAST 12 STREET | asserted_member | n/a | n/a | 0 |
| 353 EAST 12 STREET | asserted_member | n/a | n/a | 0 |
| 351-353 EAST 12 STREET | asserted_member_range | n/a | n/a | 0 |
| 199 EAST 12 STREET | synthesized_parsed_address | n/a | n/a | 0 |

SQL:

```sql
WITH probes AS (
  SELECT '197 1 AVENUE' AS probe_address, 'asserted_member' AS probe_type UNION ALL
  SELECT '199 1 AVENUE', 'asserted_member' UNION ALL
  SELECT '201 1 AVENUE', 'asserted_member' UNION ALL
  SELECT '203 1 AVENUE', 'asserted_member' UNION ALL
  SELECT '205 1 AVENUE', 'asserted_member' UNION ALL
  SELECT '349 EAST 12 STREET', 'asserted_member' UNION ALL
  SELECT '351 EAST 12 STREET', 'asserted_member' UNION ALL
  SELECT '353 EAST 12 STREET', 'asserted_member' UNION ALL
  SELECT '351-353 EAST 12 STREET', 'asserted_member_range' UNION ALL
  SELECT '199 EAST 12 STREET', 'synthesized_parsed_address'
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

## Member Set Evidence

| address | BBL | class | BLDGAREA | LOTAREA | floors | year | owner |
|---|---|---|---:|---:|---:|---:|---|
| 197 1 AVENUE | `1004540045` | S4 | 4,546 | 1,505 | 4 | 1900 | 12TH & 1ST DE LLC |
| 199 1 AVENUE | `1004540044` | S3 | 3,330 | 1,295 | 4 | 1920 | 12TH & 1ST DE LLC |
| 201 1 AVENUE | `1004540043` | S3 | 3,336 | 1,295 | 4 | 1920 | 12TH & 1ST DE LLC |
| 203 1 AVENUE | `1004540042` | S3 | 3,740 | 1,295 | 4 | 1920 | 12TH & 1ST DE LLC |
| 205 1 AVENUE | `1004540041` | S3 | 3,332 | 1,295 | 4 | 1920 | 12TH & 1ST DE LLC |
| 349 EAST 12 STREET | `1004540046` | S9 | 6,164 | 3,123 | 4 | 1920 | 12TH & 1ST DE LLC |

Citywide member-address SQL:

```sql
WITH asserted_addresses AS (
  SELECT '197 1 AVENUE' AS asserted_address UNION ALL
  SELECT '199 1 AVENUE' UNION ALL
  SELECT '201 1 AVENUE' UNION ALL
  SELECT '203 1 AVENUE' UNION ALL
  SELECT '205 1 AVENUE' UNION ALL
  SELECT '349 EAST 12 STREET' UNION ALL
  SELECT '351 EAST 12 STREET' UNION ALL
  SELECT '353 EAST 12 STREET' UNION ALL
  SELECT '351-353 EAST 12 STREET' UNION ALL
  SELECT '199 EAST 12 STREET'
), normalized AS (
  SELECT asserted_address,
         REGEXP_REPLACE(UPPER(TRIM(asserted_address)), '[^A-Z0-9]', '') AS addr_norm
  FROM asserted_addresses
), matches AS (
  SELECT n.asserted_address, p.BOROUGH, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS BBL_NORM,
         p.ADDRESS, p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS, p.BLDGAREA, p.LOTAREA,
         p.NUMFLOORS, p.YEARBUILT, p.OWNERNAME, p.CENTROID_LON, p.CENTROID_LAT,
         CASE WHEN n.asserted_address = '199 EAST 12 STREET' THEN 'synthesized_parsed_address_probe'
              ELSE 'asserted_member_probe' END AS probe_type
  FROM normalized n
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '') = n.addr_norm
   AND p.BOROUGH = 'MN'
)
SELECT * FROM matches ORDER BY probe_type, asserted_address, BBL_NORM;
```

Footprint observations:

| source | native id | member parcel | area | height/floors | year | area ratio |
|---|---|---|---:|---:|---:|---:|
| NYC footprint | 1006494 | `1004540041` | 121.569189 sq m | 39.97 ft | 1920 | 0.950255 |
| NYC footprint | 1006495 | `1004540042` | 114.158494 sq m | 39.64 ft | 1920 | 0.924397 |
| NYC footprint | 1006496 | `1004540043` | 120.638626 sq m | 39.68 ft | 1920 | 0.900260 |
| NYC footprint | 1006497 | `1004540044` | 129.689876 sq m | 39.54 ft | 1920 | 0.883397 |
| NYC footprint | 1006498 | `1004540045` | 133.829425 sq m | 38.22 ft | 1900 | 0.872482 |
| NYC footprint | 1006499 | `1004540046` | 131.747997 sq m | 43.16 ft | 1920 | 0.982049 |

The same query returned six MapPLUTO parcel rows and six NYC footprint rows: `12` rows
total. It returned zero FEMA and zero Microsoft majority-linked rows.

SQL:

```sql
WITH target_bbls AS (
  SELECT '1004540041' AS bbl_norm UNION ALL SELECT '1004540042' UNION ALL
  SELECT '1004540043' UNION ALL SELECT '1004540044' UNION ALL
  SELECT '1004540045' UNION ALL SELECT '1004540046'
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

## Independent ACRIS Evidence

The latest-five-per-BBL ACRIS query returned `30` rows: five per each of the six BBLs.
The newest four document ids are shared across all six lots:

- `2020011300037005`
- `2020011300037004`
- `2020011300037002`
- `2020011300037001`

The next document id `2019102901258019` is also shared across all six. This is independent
document evidence that the six lots participate in one recorded property event.

Representative latest row per BBL:

| BBL | document | street number | street name | property_type | good_through |
|---|---|---:|---|---|---|
| `1004540041` | 2020011300037005 | 205 | 1 AVENUE | CR | 2020-01-31 |
| `1004540042` | 2020011300037005 | 203 | 1 AVENUE | CR | 2020-01-31 |
| `1004540043` | 2020011300037005 | 201 | 1 AVENUE | CR | 2020-01-31 |
| `1004540044` | 2020011300037005 | 199 | 1 AVENUE | CR | 2020-01-31 |
| `1004540045` | 2020011300037005 | 197 | 1 AVENUE | CR | 2020-01-31 |
| `1004540046` | 2020011300037005 | 349 | EAST 12TH STREET | CR | 2020-01-31 |

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
  WHERE BBL IN ('1004540041','1004540042','1004540043','1004540044','1004540045','1004540046')
    AND BOROUGH = 1
    AND BLOCK = 454
    AND LOT IN (41,42,43,44,45,46)
)
SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
       PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER
FROM legals
WHERE rn <= 5
ORDER BY BBL, rn;
```

## Tile Inventory

Two anchors were counted: the bad geocode point and the recovered member-set centroid.
Overture remains zero in both tiles.

| anchor | radius m | CMBS assertions | MapPLUTO parcels | NYC footprints | FEMA | Microsoft | Overture |
|---|---:|---:|---:|---:|---:|---:|---:|
| bad_geocode | 150 | 4 | 103 | 101 | 13 | 13 | 0 |
| bad_geocode | 500 | 102 | 958 | 1012 | 122 | 131 | 0 |
| member_centroid | 150 | 18 | 135 | 142 | 15 | 17 | 0 |
| member_centroid | 500 | 87 | 1232 | 1348 | 122 | 146 | 0 |

SQL:

```sql
WITH target_bbls AS (
  SELECT '1004540041' AS bbl_norm UNION ALL SELECT '1004540042' UNION ALL
  SELECT '1004540043' UNION ALL SELECT '1004540044' UNION ALL
  SELECT '1004540045' UNION ALL SELECT '1004540046'
), anchors AS (
  SELECT 'bad_geocode' AS anchor, -73.988534::FLOAT AS lon, 40.732095::FLOAT AS lat
  UNION ALL
  SELECT 'member_centroid' AS anchor, AVG(p.CENTROID_LON)::FLOAT AS lon, AVG(p.CENTROID_LAT)::FLOAT AS lat
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  JOIN target_bbls t ON REGEXP_REPLACE(p.BBL, '\\.0$', '') = t.bbl_norm
), radii AS (
  SELECT 150::FLOAT AS radius_m UNION ALL SELECT 500::FLOAT
), counts AS (
  SELECT a.anchor, r.radius_m, 'cmbs_geocode' AS src, 'assertion' AS level, COUNT(*) AS n_rows
  FROM anchors a CROSS JOIN radii r
  JOIN EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g
    ON g.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
   AND g.LATITUDE IS NOT NULL AND g.LONGITUDE IS NOT NULL
   AND ST_DWITHIN(ST_POINT(g.LONGITUDE, g.LATITUDE), ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3,4
  UNION ALL
  SELECT a.anchor, r.radius_m, 'mappluto' AS src, 'parcel' AS level, COUNT(*) AS n_rows
  FROM anchors a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON p.BBOX_XMIN <= a.lon + 0.01 AND p.BBOX_XMAX >= a.lon - 0.01
   AND p.BBOX_YMIN <= a.lat + 0.01 AND p.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(p.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3,4
  UNION ALL
  SELECT a.anchor, r.radius_m, 'nyc_building_footprints_active' AS src, 'building' AS level, COUNT(*) AS n_rows
  FROM anchors a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT b
    ON b.IS_ACTIVE_FOOTPRINT
   AND b.BBOX_XMIN <= a.lon + 0.01 AND b.BBOX_XMAX >= a.lon - 0.01
   AND b.BBOX_YMIN <= a.lat + 0.01 AND b.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(b.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3,4
  UNION ALL
  SELECT a.anchor, r.radius_m, 'fema_usa_structures' AS src, 'building' AS level, COUNT(*) AS n_rows
  FROM anchors a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
    ON f.STATE_FIPS = '36'
   AND f.BBOX_XMIN <= a.lon + 0.01 AND f.BBOX_XMAX >= a.lon - 0.01
   AND f.BBOX_YMIN <= a.lat + 0.01 AND f.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(f.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3,4
  UNION ALL
  SELECT a.anchor, r.radius_m, 'microsoft_globalml' AS src, 'building' AS level, COUNT(*) AS n_rows
  FROM anchors a CROSS JOIN radii r
  JOIN EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
    ON m.STATE = 'NY'
   AND m.BBOX_XMIN <= a.lon + 0.01 AND m.BBOX_XMAX >= a.lon - 0.01
   AND m.BBOX_YMIN <= a.lat + 0.01 AND m.BBOX_YMAX >= a.lat - 0.01
   AND ST_DWITHIN(m.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3,4
  UNION ALL
  SELECT a.anchor, r.radius_m, 'overture_features' AS src, 'mixed' AS level, COUNT(o.GEOM_GEOG) AS n_rows
  FROM anchors a CROSS JOIN radii r
  LEFT JOIN EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
    ON ST_DWITHIN(o.GEOM_GEOG, ST_POINT(a.lon, a.lat), r.radius_m)
  GROUP BY 1,2,3,4
)
SELECT * FROM counts ORDER BY anchor, radius_m, src;
```

## Sanity Gates

- Baseline rows reconcile: `2` assertion rows, each with zero containing lots, zero raw
  exact matches, and zero parsed exact matches.
- Address probes reconcile: `10` probes, `6` positive MapPLUTO matches and `4` structured
  zeroes.
- Predicate-C observations reconcile: `12` rows = `6` MapPLUTO parcel rows plus `6` NYC
  footprint rows; FEMA and Microsoft contribute no majority edges for this member set.
- ACRIS latest-five query reconciles: `30` rows = `6` BBLs times `5` rows each.
- Overture contributes `0` rows in all tile inventory bins, matching the landed-table
  inventory.

## Design Decision Forced

Case 4 forces **multi-address field parsing before geocode trust**. A parsed address is not
automatically evidence; it must be checked against the asserted address set. Here the
parser synthesized `199 EAST 12 STREET`, a combination no source asserted and MapPLUTO does
not contain. The sound relaxation is:

- raw multi-address field -> set of candidate address constraints;
- range/interpolation geocode -> weak area/roadbed constraint, not a winner;
- parsed address -> accepted only if it is a member of the raw asserted set;
- missing PAD/per-BIN address set -> explicit residual gap for `351/353 EAST 12 STREET`.

This is the inverse of a simple address-normalization problem. Canon geo needs a
first-class multi-address parser and a chimera-detection rule, otherwise a high-score
geocode can fabricate an address and silently corroborate the wrong tile.
