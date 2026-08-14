-- canon geo worked-case corpus (bd-tccn) -- STEP 0: SCHEMA DISCOVERY
--
-- RUN THIS FIRST AND EVERY TIME A NEW SOURCE FINISHES LANDING.
--
-- Why this file exists at all: three claims in the 2026-08-14 design session came from an
-- LLM's prose summary rather than from returned values, and all three were wrong -- most
-- concretely a report of "UNITSTOTAL 178" for a MapPLUTO lot when UNITSTOTAL is not a
-- column in the landed table. Nothing in this corpus cites a column that has not been
-- confirmed present by the queries below.
--
-- Every subsequent file in scripts/geo_corpus/ assumes the column names confirmed here.
-- If a name differs, fix it HERE and in the dependent file, never by guessing at the
-- call site.
--
-- Landing status as of 2026-08-14 (cmdrvl-curves in-progress beads):
--   NYC_DCP_MAPPLUTO_HOT                          LANDED, 856,614 lots, 26v1
--   FEMA_USA_STRUCTURES_HOT                       LANDING, alphabetical by state, at AR
--   MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT    LANDING, 179/2415 US files
--   OVERTURE_MAPS_FEATURES_HOT                    DDL APPLIED, 0 rows
--   NYC building footprints / BIN bridge          LANDED per cmdrvl-curves bd-397d,
--                                                 TABLE NAME NOT YET CONFIRMED -- see Q6
-- NEW YORK IS NOT LOADED IN ANY OF THE THREE NATIONAL SOURCES YET. These queries are
-- staged to fire the moment it is.

-- ---------------------------------------------------------------------------
-- Q1. Which geo tables exist, and how many rows does each carry right now?
--     Run this before every corpus extraction so the corpus records what was
--     actually available at extraction time.
-- ---------------------------------------------------------------------------
SELECT  table_schema,
        table_name,
        row_count,
        bytes,
        last_altered
FROM    EDGAR_DB.INFORMATION_SCHEMA.TABLES
WHERE   table_schema = 'SOURCE'
  AND  (table_name LIKE '%MAPPLUTO%'
     OR table_name LIKE '%FEMA_USA_STRUCTURES%'
     OR table_name LIKE '%MICROSOFT_GLOBALML%'
     OR table_name LIKE '%OVERTURE_MAPS%'
     OR table_name LIKE '%FOOTPRINT%'
     OR table_name LIKE '%BIN%')
ORDER BY table_name;

-- ---------------------------------------------------------------------------
-- Q2. Literal column list per source table.
--     Confirms the geometry contract (geom_geog / centroid / bbox / h3_r7 / h3_r8)
--     that cmdrvl-curves bd-qbno specifies is actually present on each.
-- ---------------------------------------------------------------------------
SELECT  table_name,
        ordinal_position,
        column_name,
        data_type,
        is_nullable
FROM    EDGAR_DB.INFORMATION_SCHEMA.COLUMNS
WHERE   table_schema = 'SOURCE'
  AND   table_name IN (
            'NYC_DCP_MAPPLUTO_HOT',
            'FEMA_USA_STRUCTURES_HOT',
            'MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT',
            'OVERTURE_MAPS_FEATURES_HOT'
        )
ORDER BY table_name, ordinal_position;

-- ---------------------------------------------------------------------------
-- Q3. How much of New York has actually landed per source?
--     THIS IS THE GATE ON THE FULL-TILE CORPUS. Until these return non-zero for
--     the national sources, cases can only be built on the MapPLUTO layer.
-- ---------------------------------------------------------------------------
SELECT 'fema'      AS src, COUNT(*) AS ny_rows FROM EDGAR_DB.SOURCE.FEMA_USA_STRUCTURES_HOT
    WHERE STATE = 'NY'
UNION ALL
SELECT 'microsoft', COUNT(*) FROM EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT
    WHERE STATE = 'NY'
UNION ALL
SELECT 'overture',  COUNT(*) FROM EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT
    WHERE STATE = 'NY'
UNION ALL
SELECT 'mappluto',  COUNT(*) FROM EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT;

-- ---------------------------------------------------------------------------
-- Q4. Overture theme/type inventory once it lands.
--     bd-2cbs assigns each source a LEVEL. Overture is the only source that spans
--     levels: buildings -> building level, places -> POI level, addresses -> address
--     channel. Confirm the discriminator column before writing level assignment.
-- ---------------------------------------------------------------------------
SELECT  THEME, TYPE, LICENSE_CLASS, COUNT(*) AS n
FROM    EDGAR_DB.SOURCE.OVERTURE_MAPS_FEATURES_HOT
GROUP BY 1,2,3
ORDER BY n DESC;

-- ---------------------------------------------------------------------------
-- Q5. MapPLUTO release inventory -- IS THE ARCHIVE ACTUALLY SINGLE-RELEASE?
--     OC-0048 states only the current release landed and treats the version archive
--     (cmdrvl-curves bd-33t6) as open. But the table is STRUCTURALLY BITEMPORAL --
--     RELEASE, RELEASE_DT, VALID_FROM_RELEASE_DT, VALID_TO_RELEASE_DT and
--     IS_CURRENT_RELEASE are all present. If more than one release is loaded, case 9's
--     temporal arc is constructible TODAY and bd-33t6 is closer to done than recorded.
--     CHECK BEFORE ASSUMING BLOCKED.
-- ---------------------------------------------------------------------------
SELECT  RELEASE,
        RELEASE_DT,
        IS_CURRENT_RELEASE,
        MIN(VALID_FROM_RELEASE_DT) AS valid_from,
        MAX(VALID_TO_RELEASE_DT)   AS valid_to,
        COUNT(*)                   AS lots
FROM    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
GROUP BY 1,2,3
ORDER BY RELEASE_DT;

-- ---------------------------------------------------------------------------
-- Q6. Locate the NYC building footprints / BIN bridge table.
--     Landed per cmdrvl-curves bd-397d and it is the HIGHEST-PRIORITY missing pull:
--     it is what turns case 6 from a parcel-level case into a building-level one,
--     and it is already available. Name unconfirmed.
-- ---------------------------------------------------------------------------
SELECT  table_schema, table_name, row_count
FROM    EDGAR_DB.INFORMATION_SCHEMA.TABLES
WHERE   table_name ILIKE '%FOOTPRINT%'
   OR   table_name ILIKE '%NYC%BIN%'
   OR   table_name ILIKE '%BUILDING%'
ORDER BY table_schema, table_name;

-- ---------------------------------------------------------------------------
-- Q7. Confirm the geocode table's grain and its reprojection provenance.
--     VERIFIED 2026-08-14: grain is PROPERTY_NAME + PROPERTY_ADDRESS + PROPERTY_CITY
--     + PROPERTY_STATE + SOURCE + ASOF, with multiple Geocodio source attributions
--     appended rather than upserted. THE ROW COUNT IS NOT A PROPERTY COUNT and every
--     denominator downstream must dedupe to a declared grain first.
-- ---------------------------------------------------------------------------
SELECT  SOURCE,
        COUNT(*)                                  AS rows_all,
        COUNT(DISTINCT PROPERTY_ADDRESS)          AS distinct_addresses,
        COUNT(DISTINCT ASOF)                      AS distinct_asof,
        MIN(ASOF)                                 AS first_asof,
        MAX(ASOF)                                 AS last_asof
FROM    EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
WHERE   COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
GROUP BY 1
ORDER BY rows_all DESC;

-- ---------------------------------------------------------------------------
-- Q8. MapPLUTO reprojection provenance -- feeds bd-3nc7 (predicate regime).
--     GEOM_CRS and SOURCE_GEOM_CRS are projected columns, so the EPSG:2263 -> WGS84
--     transform the predicate-regime bead has to reason about is documented in the
--     table rather than needing archaeology. Read this before starting bd-3nc7.
-- ---------------------------------------------------------------------------
SELECT  GEOM_CRS, SOURCE_GEOM_CRS, COUNT(*) AS lots
FROM    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT
GROUP BY 1,2;
