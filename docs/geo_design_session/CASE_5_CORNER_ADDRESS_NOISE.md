# bd-tccn Case 5 - Corner Address As Noise

Case property: **Soho Plaza Corp., 66 Crosby Street a/k/a 514 Broadway**,
Manhattan, NY.

Verdict: **resolved singleton**: `1004830013` / MapPLUTO `514 BROADWAY` / NYC
footprint BIN `1007235`.

This is the inverse of Case 4. The geocode parses and points to the Crosby Street frontage,
while MapPLUTO's primary address is Broadway. A strict parsed-address equality rule would
reject the correct parcel. Geometry and address-set semantics must win: `66 CROSBY STREET`
and `514 BROADWAY` are two frontages for the same one-building parcel.

Standing measurements cited, not rederived:

- Appendix E: address-only and geocode-tier baselines both have silent-error modes.
- Appendix F: footprint-to-parcel edges use geometric
  `ST_AREA(intersection)/ST_AREA(footprint) > 0.5`.
- Appendix G: this singleton resolves inside a dense component-wise work unit.

## Selection Rule

The selector enumerated rooftop `A/K/A` rows whose geocode point is inside exactly one
MapPLUTO parcel but whose parsed address differs from the containing parcel's primary
MapPLUTO address. The selected row is the third returned candidate and is one of the
operator-supplied Case 5 candidates in the bead comments.

Relevant selector output:

| rank | property | address | parsed | containing BBL | MapPLUTO address | buildings | BLDGAREA | year |
|---:|---|---|---|---|---|---:|---:|---:|
| 1 | 39 Suydam | 39 Suydam a/k/a 692 Bushwick Avenue | 39 Suydam St | `3032040032` | 692 BUSHWICK AVENUE | 1 | 33,866 | 2013 |
| 2 | 45 John Street | 45 John Street a/k/a 1 Dutch Street | 45 John St | `1000787508` | 45 JOHN STREET | 1 | 81,199 | 1908 |
| 3 | Soho Plaza Corp. | 66 Crosby Street a/k/a 514 Broadway | 66 Crosby St | `1004830013` | 514 BROADWAY | 1 | 76,550 | 1881 |

Selector SQL:

```sql
WITH assertion_rows AS (
  SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, PROPERTY_STATE, PROPERTY_ZIP,
         PROPERTY_COUNTY, COUNTY_FIPS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
         SOURCE, ASOF, LATITUDE, LONGITUDE, ST_POINT(LONGITUDE, LATITUDE) AS pt,
         REGEXP_REPLACE(UPPER(TRIM(NUMBER || ' ' || STREET)), '[^A-Z0-9]', '') AS parsed_addr_norm,
         LENGTH(PROPERTY_ADDRESS) AS address_len
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
  WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
    AND PROPERTY_ADDRESS ILIKE '%A/K/A%'
    AND LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
    AND NUMBER IS NOT NULL AND STREET IS NOT NULL
    AND ACCURACY_TYPE = 'rooftop'
), contains_edges AS (
  SELECT r.*, REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm, p.BOROUGH,
         p.ADDRESS AS mappluto_address, p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS,
         p.BLDGAREA, p.LOTAREA, p.NUMFLOORS, p.YEARBUILT, p.OWNERNAME,
         REGEXP_REPLACE(UPPER(TRIM(p.ADDRESS)), '[^A-Z0-9]', '') AS mappluto_addr_norm
  FROM assertion_rows r
  JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
    ON ST_CONTAINS(p.GEOM_GEOG, r.pt)
), rollup AS (
  SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, PROPERTY_STATE, PROPERTY_ZIP,
         PROPERTY_COUNTY, COUNTY_FIPS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
         SOURCE, ASOF, LATITUDE, LONGITUDE, parsed_addr_norm, address_len,
         COUNT(DISTINCT bbl_norm) AS containing_lots,
         MIN(bbl_norm) AS bbl_norm, MIN(mappluto_address) AS mappluto_address,
         MIN(BLDGCLASS) AS bldgclass, MIN(LANDUSE) AS landuse, MIN(NUMBLDGS) AS numbldgs,
         MIN(BLDGAREA) AS bldgarea, MIN(LOTAREA) AS lotarea, MIN(NUMFLOORS) AS numfloors,
         MIN(YEARBUILT) AS yearbuilt, MIN(OWNERNAME) AS ownername,
         MIN(mappluto_addr_norm) AS mappluto_addr_norm
  FROM contains_edges
  GROUP BY PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, PROPERTY_STATE, PROPERTY_ZIP,
           PROPERTY_COUNTY, COUNTY_FIPS, NUMBER, STREET, ACCURACY_TYPE, ACCURACY_SCORE,
           SOURCE, ASOF, LATITUDE, LONGITUDE, parsed_addr_norm, address_len
)
SELECT PROPERTY_NAME, PROPERTY_ADDRESS, PROPERTY_CITY, COUNTY_FIPS, NUMBER, STREET,
       ACCURACY_TYPE, ACCURACY_SCORE, SOURCE, ASOF, LATITUDE, LONGITUDE,
       containing_lots, bbl_norm, mappluto_address, bldgclass, landuse, numbldgs,
       bldgarea, lotarea, numfloors, yearbuilt, ownername,
       IFF(parsed_addr_norm = mappluto_addr_norm, 1, 0) AS parsed_equals_mappluto_address,
       address_len
FROM rollup
WHERE containing_lots = 1
  AND parsed_addr_norm <> mappluto_addr_norm
ORDER BY address_len, PROPERTY_ADDRESS
LIMIT 25;
```

## Assertion Rows - Six Contract Fields

Two CMBS geocode rows are present for the same property and point:

| address | parsed | accuracy | score | asof | lat | lon |
|---|---|---|---:|---|---:|---:|
| 66 Crosby Street aka 514 Broadway | 66 Crosby St | rooftop | 1.00 | 2025-01-01 | 40.7223550 | -73.9983570 |
| 66 Crosby Street a/k/a 514 Broadway | 66 Crosby St | rooftop | 0.95 | 2025-01-01 | 40.7223550 | -73.9983570 |

| contract field | case value |
|---|---|
| geocode | present: rooftop point `(-73.9983570, 40.7223550)` |
| address | present: raw field contains two frontages; parsed address is only `66 Crosby St` |
| geometry | absent in CMBS geocode row |
| building size | absent in CMBS geocode row |
| year built | absent in CMBS geocode row |
| property type | absent in CMBS geocode row |

## Baseline Outputs

| assertion variant | containing lots | containing BBL/address | nearest distance m | raw exact matches | parsed exact matches |
|---|---:|---|---:|---:|---:|
| `aka` | 1 | `1004830013` / 514 BROADWAY | 0.0 | 0 | 0 |
| `a/k/a` | 1 | `1004830013` / 514 BROADWAY | 0.0 | 0 | 0 |

Naive raw-string and parsed-string address matching fail. Point-in-polygon succeeds, but
the apparent address disagreement must not be treated as contradiction.

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
    AND PROPERTY_ADDRESS ILIKE '%66%Crosby%514%Broadway%'
), contains_edges AS (
  SELECT r.PROPERTY_ADDRESS, r.NUMBER, r.STREET, r.ASOF, r.LATITUDE, r.LONGITUDE,
         REGEXP_REPLACE(p.BBL, '\\.0$', '') AS bbl_norm, p.ADDRESS,
         p.BLDGCLASS, p.LANDUSE, p.NUMBLDGS, p.BLDGAREA, p.LOTAREA, p.NUMFLOORS,
         p.YEARBUILT, p.OWNERNAME
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
       ce.bbl_norm AS containing_bbl, ce.ADDRESS AS containing_address,
       ce.BLDGCLASS, ce.LANDUSE, ce.NUMBLDGS, ce.BLDGAREA, ce.LOTAREA,
       ce.NUMFLOORS, ce.YEARBUILT, ce.OWNERNAME,
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
         ce.bbl_norm, ce.ADDRESS, ce.BLDGCLASS, ce.LANDUSE, ce.NUMBLDGS,
         ce.BLDGAREA, ce.LOTAREA, ce.NUMFLOORS, ce.YEARBUILT, ce.OWNERNAME,
         ne.bbl_norm, ne.ADDRESS, ne.distance_m,
         ram.raw_exact_address_matches, pam.parsed_exact_address_matches
ORDER BY r.PROPERTY_NAME;
```

## Address-Set Probes

| probe address | role | BBL | MapPLUTO address | match |
|---|---|---|---|---:|
| 514 BROADWAY | aka_frontage | `1004830013` | 514 BROADWAY | 1 |
| 66 CROSBY STREET | parsed_frontage | n/a | n/a | 0 |

SQL:

```sql
WITH probes AS (
  SELECT '66 CROSBY STREET' AS probe_address, 'parsed_frontage' AS probe_type UNION ALL
  SELECT '514 BROADWAY', 'aka_frontage'
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

| source | native id | entity | parcel | area | floors/height | year | ratio |
|---|---|---|---|---:|---:|---:|---:|
| MapPLUTO | `1004830013` | parcel | `1004830013` | 76,550 sq ft | 6 floors | 1881 | n/a |
| NYC footprint | `1007235` | building | `1004830013` | 1145.119545 sq m | 100.66 ft | 1900 | 0.977523 |

The same query returned exactly `2` rows: one MapPLUTO parcel row and one NYC footprint
row. FEMA and Microsoft returned zero majority-linked rows for this BBL.

SQL:

```sql
WITH target AS (
  SELECT p.*
  FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  WHERE REGEXP_REPLACE(p.BBL, '\\.0$', '') = '1004830013'
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

## Independent ACRIS Evidence

ACRIS latest-ten-per-BBL output returned both frontages on the same BBL:

| document | street | unit | type | good_through |
|---|---|---|---|---|
| 2026032600707001 | 66 CROSBY STREET | 4E | SP | 2026-03-31 |
| 2025061300651001 | 66 CROSBY STREET | 4B | SP | 2025-06-30 |
| 2023090700107001 | 514 BROADWAY | 2GH | SP | 2023-09-30 |
| 2022121600559028 | 66 CROSBY STREET | n/a | AP | 2022-12-31 |
| 2022121600559027 | 66 CROSBY STREET | n/a | AP | 2022-12-31 |

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
  WHERE BBL = '1004830013'
    AND BOROUGH = 1
    AND BLOCK = 483
    AND LOT = 13
)
SELECT BBL, DOCUMENT_ID, BOROUGH, BLOCK, LOT, STREET_NUMBER, STREET_NAME, UNIT,
       PROPERTY_TYPE, GOOD_THROUGH_DATE, RELEASE_DT, SOURCE_ROW_NUMBER
FROM legals
WHERE rn <= 10
ORDER BY rn;
```

## Tile Inventory

Anchor point: rooftop geocode `(-73.998357, 40.722355)`.

| radius m | CMBS assertions | MapPLUTO parcels | NYC footprints | FEMA | Microsoft | Overture |
|---:|---:|---:|---:|---:|---:|---:|
| 150 | 37 | 115 | 118 | 13 | 13 | 0 |
| 500 | 233 | 1313 | 1367 | 137 | 137 | 0 |

SQL:

```sql
WITH anchor AS (
  SELECT -73.998357::FLOAT AS lon, 40.722355::FLOAT AS lat
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

## Sanity Gates

- The assertion rollup returns `2` rows for the same point and property; both have
  `containing_lots=1`, `raw_exact_address_matches=0`, and `parsed_exact_address_matches=0`.
- Address probes return `2` rows: the `aka_frontage` matches the BBL and the
  `parsed_frontage` has a structured zero.
- Predicate-C observations reconcile: `2` rows = one MapPLUTO parcel plus one NYC
  footprint. FEMA and Microsoft contribute no majority edge for this BBL.
- ACRIS query returns `10` rows for one BBL and includes both `66 CROSBY STREET` and
  `514 BROADWAY`.
- Overture contributes `0` rows in both tile inventory bins.

## Design Decision Forced

Case 5 forces **address disagreement to be non-fatal**. The source assertion itself says
the building has multiple frontages. MapPLUTO chooses Broadway as the primary address;
ACRIS records both Crosby and Broadway; the geocode point is inside the Broadway-addressed
parcel. A channel-agreement rule that demands parsed-address equality would reject the true
answer.

The sound relaxation is:

- raw `A/K/A` field -> address set, not one functional address;
- parcel `ADDRESS` -> one observed frontage/primary address, not the complete address set;
- parsed geocode address -> one frontage, not a contradiction when another source uses a
  different frontage;
- geometry containment plus matching address-set member -> singleton resolution.
