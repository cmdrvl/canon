-- canon geo worked-case corpus (bd-tccn) -- STEP 2: FULL TILE ASSEMBLY
--
-- Given one case's anchor point, pull EVERY source at EVERY level for the tile around
-- it. This is the query that produces "the full picture" a case needs, and it is the one
-- that fires the moment New York finishes landing.
--
-- STATUS 2026-08-14: NEW YORK IS NOT LOADED in FEMA (alphabetical, at AR), Microsoft
-- (179/2415 US files) or Overture (DDL applied, 0 rows). Run 00_discovery.sql Q3 first;
-- the MapPLUTO block below works today, the rest returns empty until then.
--
-- ===========================================================================
-- TWO DESIGN DECISIONS THAT ARE NOT ARBITRARY. Read before editing.
-- ===========================================================================
--
-- 1. THE TILE EXTENT IS DEFINED BY DISTANCE, NOT BY AN H3 k-RING CALL.
--    edgar-elt br-2wir records that Snowflake's H3 helpers returned MALFORMED cell
--    boundaries (edges computing to ~27-48 m against an expected r9 edge of ~174 m) and
--    a k-ring that listed one neighbour THREE TIMES. Cell ASSIGNMENT may be fine while
--    cell GEOMETRY is not, so:
--        H3_R8 is used ONLY as a cheap partition prefilter (it prunes; it does not
--        define the answer).
--        The actual extent is ST_DWITHIN in metres, which is exact and does not
--        inherit the defect.
--    Canon will compute its own cells with a vetted Rust H3 library (bd-2b9d). This
--    file must not encode a dependency on warehouse H3 geometry.
--
--    RADIUS CHOICE: 500 m approximates an r8 cell (~0.737 km2), which per the
--    2026-08-14 landing review is almost exactly the epic's intended work unit
--    (r9 + k-ring 1 = 7 cells = 0.737 km2). Adjust with the parameter, not by editing.
--
-- 2. MEMBERSHIP IS CENTROID-ANCHORED, WITH POLYGON OVERLAP AS A SUPPLEMENT.
--    cmdrvl-curves bd-3fyv measured this live: H3_POLYGON_TO_CELLS produced only 116
--    coverage rows for 354,804 building rows, because polygon-to-cells returns cells
--    whose CENTRES fall inside the polygon and a building almost never contains an r8
--    cell centre. Anchoring on the feature centroid is the reliable direction;
--    ST_INTERSECTS against the tile disc catches large features (parcels, campuses)
--    that a centroid alone would miss.
--
-- ===========================================================================
SET tile_lon    = -73.9580550;   -- <<< case anchor longitude
SET tile_lat    =  40.7688430;   -- <<< case anchor latitude
SET tile_radius = 500;           -- metres; ~ one r8 cell
SET case_id     = 'case6_305_e_72nd';

-- ---------------------------------------------------------------------------
-- LEVEL: PARCEL -- MapPLUTO (landed). The client's licensed layer substitutes here
-- under BYOP; MapPLUTO stands in for it on the NYC proving ground.
-- Note BBL is stored as a trailing-".0" string and is NOT join-ready as an exact
-- identifier -- canon's exact byte match would fail "1014477501" against
-- "1014477501.0". Normalization is emitted here so the corpus records the raw form
-- and the normalized form side by side.
-- ---------------------------------------------------------------------------
SELECT  $case_id                                   AS case_id,
        'parcel'                                   AS entity_level,
        'mappluto'                                 AS src,
        p.RELEASE                                  AS src_vintage,
        p.BBL                                      AS native_id_raw,
        REGEXP_REPLACE(p.BBL, '\\.0$', '')         AS native_id_normalized,
        p.ADDRESS                                  AS src_address,
        p.BLDGCLASS                                AS src_class,
        p.LANDUSE                                  AS src_landuse,
        p.BLDGAREA                                 AS src_building_area,
        p.LOTAREA                                  AS src_lot_area,
        p.NUMBLDGS                                 AS src_num_buildings,
        p.NUMFLOORS                                AS src_num_floors,
        p.YEARBUILT                                AS src_year_built,
        p.OWNERNAME                                AS src_owner,
        'observed'                                 AS attr_provenance,
        NULL                                       AS license_class,
        ST_DISTANCE(p.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat)) AS metres_from_anchor,
        ST_CONTAINS(p.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat))  AS contains_anchor,
        p.CENTROID_LON, p.CENTROID_LAT,
        p.H3_R7, p.H3_R8,
        p.GEOM_CRS, p.SOURCE_GEOM_CRS,             -- reprojection provenance -> bd-3nc7
        ST_ASWKT(p.GEOM_GEOG)                      AS geom_wkt  -- DECISION fidelity
FROM    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
WHERE   ST_DWITHIN(p.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat), $tile_radius);

-- ---------------------------------------------------------------------------
-- LEVEL: BUILDING -- FEMA USA Structures.
-- Supplies OCCUPANCY and SQFEET, which are the attribute channels bd-272d anchors on.
--
-- OCCUPANCY IS MODELLED, NOT SURVEYED (cmdrvl-curves bd-1ssr requires a provenance flag
-- "so downstream never treats it as surveyed"). It is emitted below as
-- attr_provenance='modelled' and MUST be used as SUPPORT evidence only -- never as the
-- hard eliminative type filter in assemblage mode (bd-2zdz), because a hard filter on a
-- wrong modelled class removes the correct candidate before anything is scored.
--
-- COLUMN NAMES: confirmed on the SOURCE layer per bd-1ssr (UUID, BUILD_ID, OCC_CLS,
-- PRIM_OCC, PROP_ADDR, SQFEET, FIPS). The HOT table adds the shared geometry contract.
-- RE-CONFIRM WITH 00_discovery.sql Q2 BEFORE FIRST RUN.
-- ---------------------------------------------------------------------------
SELECT  $case_id                                   AS case_id,
        'building'                                 AS entity_level,
        'fema_usa_structures'                      AS src,
        f.RELEASE_DT                               AS src_vintage,
        f.UUID                                     AS native_id_raw,
        f.UUID                                     AS native_id_normalized,
        f.PROP_ADDR                                AS src_address,
        f.OCC_CLS                                  AS src_class,
        f.PRIM_OCC                                 AS src_landuse,
        f.SQFEET                                   AS src_building_area,
        NULL                                       AS src_lot_area,
        1                                          AS src_num_buildings,
        NULL                                       AS src_num_floors,
        NULL                                       AS src_year_built,   -- FEMA has none
        NULL                                       AS src_owner,
        'modelled'                                 AS attr_provenance,  -- SEE NOTE ABOVE
        'CC-BY-4.0'                                AS license_class,
        ST_DISTANCE(f.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat)) AS metres_from_anchor,
        ST_CONTAINS(f.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat))  AS contains_anchor,
        f.CENTROID_LON, f.CENTROID_LAT,
        f.H3_R7, f.H3_R8,
        NULL, NULL,
        ST_ASWKT(f.GEOM_GEOG)                      AS geom_wkt
FROM    EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT f
WHERE   f.STATE = 'NY'                                                   -- partition prune
  AND   ST_DWITHIN(f.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat), $tile_radius);

-- ---------------------------------------------------------------------------
-- LEVEL: BUILDING -- Microsoft GlobalML footprints.
-- ALIAS-LESS PARTICIPANT (bd-2cbs rule 4). Verified in OC-0047 against the Overture GERS
-- bridge spec, which excludes Microsoft explicitly because it has "no meaningful IDs for
-- reverse lookup", and confirmed in cmdrvl-curves bd-3fyv which derives "a deterministic
-- feature hash because no stable id".
-- THAT HASH IS NOT AN ALIAS AND MUST NEVER BE MINTED INTO A REGISTRY -- it changes
-- whenever the geometry changes, which violates the tile identifier stability contract
-- (bd-12gh). It is emitted here only so the corpus can join rows within one extraction.
-- Microsoft contributes GEOMETRY and HEIGHT; height feeds gross area for the assemblage
-- subset-sum (bd-2zdz).
-- ---------------------------------------------------------------------------
SELECT  $case_id                                   AS case_id,
        'building'                                 AS entity_level,
        'microsoft_globalml'                       AS src,
        m.RELEASE_DT                               AS src_vintage,
        m.FEATURE_HASH                             AS native_id_raw,   -- NOT an alias
        NULL                                       AS native_id_normalized,
        NULL                                       AS src_address,
        NULL                                       AS src_class,
        NULL                                       AS src_landuse,
        NULL                                       AS src_building_area,
        NULL                                       AS src_lot_area,
        1                                          AS src_num_buildings,
        m.HEIGHT                                   AS src_num_floors,  -- height, not floors
        NULL                                       AS src_year_built,
        NULL                                       AS src_owner,
        'modelled'                                 AS attr_provenance,
        'CDLA-Permissive-2.0'                      AS license_class,
        ST_DISTANCE(m.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat)) AS metres_from_anchor,
        ST_CONTAINS(m.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat))  AS contains_anchor,
        m.CENTROID_LON, m.CENTROID_LAT,
        m.H3_R7, m.H3_R8,
        NULL, NULL,
        ST_ASWKT(m.GEOM_GEOG)                      AS geom_wkt
FROM    EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT m
WHERE   m.STATE = 'NY'
  AND   ST_DWITHIN(m.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat), $tile_radius);

-- ---------------------------------------------------------------------------
-- LEVELS: BUILDING and POI -- Overture Maps.
-- Overture is the only source spanning levels, so THEME/TYPE is the level discriminator.
--   theme=buildings -> building level, carries GERS id, height, num_floors, class
--   theme=places    -> POI level (this is the Foursquare-derived places data)
--   theme=addresses -> ALPHA, NO STABLE GERS ID, do not build a contract against it
--                      (bd-30ho). It is NOT the address-set source bd-35qg needs.
--
-- LICENSE_CLASS IS SELECTED DELIBERATELY. Overture buildings and divisions are
-- ODbL-1.0, which is SHARE-ALIKE, while places are a per-source CDLA/Apache/CC0 mix.
-- bd-30ho: "License segmentation by source array is REQUIRED before any derived product
-- ships. Treat as a build-time gate, not a later cleanup." Carrying license class per
-- FEATURE is what makes the segregation option in bd-pq5w possible.
--
-- THE POI LEVEL IS WHERE canon entity's EXISTING NAME MACHINERY APPLIES (bd-2cbs rule 3)
-- -- namekit, rapidfuzz, tfidf_cosine, the cmbs_tenant_label profile pattern. It is also
-- what breaks the tie in case 6, where every candidate building sits on one lot and
-- geometry cannot discriminate.
-- ---------------------------------------------------------------------------
SELECT  $case_id                                   AS case_id,
        CASE o.THEME WHEN 'buildings' THEN 'building'
                     WHEN 'places'    THEN 'poi'
                     WHEN 'addresses' THEN 'address'
                     ELSE o.THEME END              AS entity_level,
        'overture_' || o.THEME                     AS src,
        o.RELEASE                                  AS src_vintage,
        o.ID                                       AS native_id_raw,   -- GERS, non-address
        o.ID                                       AS native_id_normalized,
        o.ADDRESS                                  AS src_address,
        o.CLASS                                    AS src_class,
        o.SUBTYPE                                  AS src_landuse,
        NULL                                       AS src_building_area,
        NULL                                       AS src_lot_area,
        1                                          AS src_num_buildings,
        o.NUM_FLOORS                               AS src_num_floors,
        NULL                                       AS src_year_built,
        o.NAME                                     AS src_owner,       -- POI name
        'observed'                                 AS attr_provenance,
        o.LICENSE_CLASS                            AS license_class,   -- ODbL segregation
        ST_DISTANCE(o.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat)) AS metres_from_anchor,
        ST_CONTAINS(o.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat))  AS contains_anchor,
        o.CENTROID_LON, o.CENTROID_LAT,
        o.H3_R7, o.H3_R8,
        NULL, NULL,
        ST_ASWKT(o.GEOM_GEOG)                      AS geom_wkt
FROM    EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT o
WHERE   o.STATE = 'NY'
  AND   ST_DWITHIN(o.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat), $tile_radius);

-- ---------------------------------------------------------------------------
-- LEVEL: BUILDING -- NYC building footprints / the BBL-to-BIN bridge.
-- LANDED per cmdrvl-curves bd-397d and the HIGHEST-PRIORITY missing pull: it is what
-- turns case 6 from a parcel-level case into a building-level one, and unlike the
-- national sources it is available RIGHT NOW.
-- TABLE NAME UNCONFIRMED -- resolve with 00_discovery.sql Q6, then fill in below.
-- ---------------------------------------------------------------------------
-- SELECT  $case_id AS case_id, 'building' AS entity_level, 'nyc_footprints' AS src, ...
-- FROM    EDGAR_DB.SOURCE.<CONFIRM_WITH_Q6> b
-- WHERE   ST_DWITHIN(b.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat), $tile_radius);

-- ---------------------------------------------------------------------------
-- THE ASSERTION SIDE. Every client row whose geocode falls in this tile.
-- Deduplicated to the declared grain, because the table is property x SOURCE x ASOF and
-- the row count is NOT a property count. Multiple Geocodio source attributions per
-- property are expected and normal -- and they may DISAGREE, which is what the tile
-- adjudicates (bd-1a12).
-- ---------------------------------------------------------------------------
SELECT  $case_id                                   AS case_id,
        'assertion'                                AS entity_level,
        'cmbs_geocode'                             AS src,
        g.ASOF                                     AS src_vintage,
        g.SOURCE                                   AS geocodio_source_attribution,
        g.PROPERTY_NAME, g.PROPERTY_ADDRESS,
        g.NUMBER, g.STREET, g.UNIT_TYPE, g.UNIT_NUMBER,
        g.ACCURACY_TYPE, g.ACCURACY_SCORE,
        g.LATITUDE, g.LONGITUDE,
        ST_DISTANCE(ST_POINT(g.LONGITUDE, g.LATITUDE),
                    ST_POINT($tile_lon, $tile_lat)) AS metres_from_anchor,
        -- CHIMERA CHECK (bd-1zph): is the parsed street actually present in the
        -- asserted address? Purely local, no external data, and it catches the
        -- ROOFTOP-tier silent errors that accuracy_type provably misses.
        CASE WHEN g.STREET IS NOT NULL
              AND g.PROPERTY_ADDRESS NOT ILIKE '%' || g.STREET || '%'
             THEN TRUE ELSE FALSE END              AS chimera_parse_suspected
FROM    EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED g
WHERE   g.COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
  AND   g.LATITUDE IS NOT NULL
  AND   ST_DWITHIN(ST_POINT(g.LONGITUDE, g.LATITUDE),
                   ST_POINT($tile_lon, $tile_lat), $tile_radius)
ORDER BY metres_from_anchor;

-- ---------------------------------------------------------------------------
-- TILE DISCRIMINATING POWER -- the denominator bd-272d needs.
-- Attribute anchoring is only as strong as the RARITY of the matched tuple IN THIS TILE.
-- "Office at 10,000 sf" is decisive among three offices and worthless among forty. The
-- tile bounds the population, so the rarity is exactly computable -- no global corpus
-- statistics, no approximation. This is canon's existing tfidf rarity weighting
-- (src/entity/tfidf_evidence.rs) applied to attribute tuples instead of tokens.
--
-- ABSTAIN when the tuple is not discriminating, and REPORT this number so
-- "we could not resolve it" distinguishes "the tile is too homogeneous" from
-- "nothing matched".
-- ---------------------------------------------------------------------------
SELECT  p.BLDGCLASS                                       AS class_bucket,
        WIDTH_BUCKET(p.BLDGAREA, 0, 500000, 20)           AS area_bucket,
        WIDTH_BUCKET(p.YEARBUILT, 1850, 2030, 18)         AS year_bucket,
        COUNT(*)                                          AS features_in_bucket,
        COUNT(*) / SUM(COUNT(*)) OVER ()                  AS bucket_share,
        SUM(COUNT(*)) OVER ()                             AS tile_feature_count
FROM    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
WHERE   ST_DWITHIN(p.GEOM_GEOG, ST_POINT($tile_lon, $tile_lat), $tile_radius)
GROUP BY 1,2,3
ORDER BY features_in_bucket DESC;
