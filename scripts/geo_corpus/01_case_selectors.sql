-- canon geo worked-case corpus (bd-tccn) -- STEP 1: CASE SELECTORS
--
-- These find INSTANCES OF EACH CASE SHAPE. They are the "recorded selection rule" the
-- corpus requires: subjects are runtime-selected by query, never hand-picked, so the
-- corpus is regenerable and expandable rather than seven cherry-picked rows.
-- (AGENTS.md proof-class discipline: hand-picked development fixtures are not proof.)
--
-- ALL OF THESE RUN TODAY. They need only the geocode book and MapPLUTO, both landed.
-- The FULL TILE for each selected case needs 02_tile_assembly.sql and the national
-- sources, which have not reached New York yet.
--
-- GRAIN WARNING, APPLIES TO EVERY QUERY BELOW. The geocode table is
-- property x SOURCE x ASOF -- Geocodio's per-result source attribution appended, not
-- upserted. 6,682 five-borough rows are NOT 6,682 properties. Every selector below
-- reports rows AND a deduplicated count so no downstream rate is computed over the
-- wrong denominator.

-- Shared scope. NYC five boroughs.
--   36005 Bronx | 36047 Brooklyn | 36061 Manhattan | 36081 Queens | 36085 Staten Island
-- Reused by every selector via CTE rather than repeated literals.

-- ===========================================================================
-- CASE 1 -- CLEAN. Rooftop geocode, exactly one containing lot, address agrees.
-- The ablation control. Every other case is measured against what this one does.
-- ===========================================================================
WITH scope AS (
    SELECT *
    FROM   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE  COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND  LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
),
hits AS (
    SELECT  s.PROPERTY_NAME, s.PROPERTY_ADDRESS, s.NUMBER, s.STREET,
            s.ACCURACY_TYPE, s.ACCURACY_SCORE, s.SOURCE, s.ASOF,
            s.LATITUDE, s.LONGITUDE,
            p.BBL, p.ADDRESS AS pluto_address, p.BLDGCLASS, p.NUMBLDGS,
            p.BLDGAREA, p.LOTAREA, p.YEARBUILT
    FROM    scope s
    JOIN    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON    ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
    WHERE   s.ACCURACY_TYPE = 'rooftop'
),
containment_count AS (
    SELECT PROPERTY_ADDRESS, SOURCE, ASOF, COUNT(DISTINCT BBL) AS lots_containing
    FROM   hits GROUP BY 1,2,3
)
SELECT  h.*
FROM    hits h
JOIN    containment_count c
  ON    h.PROPERTY_ADDRESS = c.PROPERTY_ADDRESS
 AND    h.SOURCE = c.SOURCE AND h.ASOF = c.ASOF
WHERE   c.lots_containing = 1              -- unambiguous containment
  AND   h.NUMBLDGS = 1                     -- one building on the lot
  AND   h.PROPERTY_ADDRESS NOT ILIKE '%/%' -- no multi-address separators
  AND   h.PROPERTY_ADDRESS NOT ILIKE '%&%'
  AND   h.PROPERTY_ADDRESS NOT ILIKE '% AND %'
  AND   h.PROPERTY_ADDRESS NOT ILIKE '%,%'
  AND   h.NUMBER IS NOT NULL AND h.STREET IS NOT NULL
LIMIT 25;

-- ===========================================================================
-- CASE 2 -- GEOCODE IN THE ROADBED. No lot contains the point.
-- OC-0048 calls the interpolated tier the actual canon geo workload. This is also the
-- population where snap-to-nearest is forbidden and demonstrably catastrophic:
-- "241/249 West 74th Street" has its nearest lot 8.4 m away and it is a DIFFERENT
-- building, with 23 lots inside 50 m.
-- Returns the near-miss neighbourhood so the corpus can show what the cascade must
-- choose among rather than only that it failed.
-- ===========================================================================
WITH scope AS (
    SELECT *
    FROM   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE  COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND  LATITUDE IS NOT NULL AND LONGITUDE IS NOT NULL
      AND  ACCURACY_TYPE IN ('range_interpolation','street_center','intersection')
),
uncontained AS (
    SELECT  s.*
    FROM    scope s
    WHERE   NOT EXISTS (
                SELECT 1
                FROM   EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
                WHERE  ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
            )
)
SELECT  u.PROPERTY_ADDRESS, u.NUMBER, u.STREET, u.ACCURACY_TYPE, u.SOURCE, u.ASOF,
        u.LATITUDE, u.LONGITUDE,
        COUNT(p.BBL)                                             AS lots_within_50m,
        MIN(ST_DISTANCE(p.GEOM_GEOG, ST_POINT(u.LONGITUDE, u.LATITUDE))) AS nearest_lot_m,
        -- The nearest lot's address is the thing snap-to-nearest would wrongly pick.
        MIN(p.ADDRESS) KEEP (DENSE_RANK FIRST ORDER BY
              ST_DISTANCE(p.GEOM_GEOG, ST_POINT(u.LONGITUDE, u.LATITUDE)))
                                                                 AS nearest_lot_address
FROM    uncontained u
LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  ON    ST_DWITHIN(p.GEOM_GEOG, ST_POINT(u.LONGITUDE, u.LATITUDE), 50)
GROUP BY 1,2,3,4,5,6,7,8
ORDER BY lots_within_50m DESC
LIMIT 25;

-- ===========================================================================
-- CASE 3 -- SPLIT / RANGE ADDRESS.
-- MUST EXCLUDE QUEENS. 756 five-borough rows carry a hyphen in the parsed NUMBER and
-- the sampled population is ENTIRELY Queens GRID addresses (47-27 Little Neck Pkwy),
-- which are single atomic house numbers, not ranges. A range rule applied there
-- produces garbage on 756 rows. Range semantics live in PROPERTY_ADDRESS under a
-- different separator set. This selector reports both populations separately so the
-- distinction stays visible.
-- ===========================================================================
SELECT  CASE WHEN COUNTY_FIPS = '36081' THEN 'queens_grid_candidate'
             ELSE 'range_candidate' END                AS population,
        COUNTY_FIPS,
        COUNT(*)                                       AS rows_all,
        COUNT(DISTINCT PROPERTY_ADDRESS)               AS distinct_addresses,
        COUNT(DISTINCT PROPERTY_ADDRESS || '|' || COALESCE(PROPERTY_NAME,''))
                                                       AS distinct_property_keys
FROM    EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
WHERE   COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
  AND  (NUMBER LIKE '%-%' OR PROPERTY_ADDRESS RLIKE '.*[0-9]+ ?[-/] ?[0-9]+.*')
GROUP BY 1,2
ORDER BY 1,2;

-- Instances, non-Queens, for the actual range case:
SELECT  PROPERTY_ADDRESS, PROPERTY_NAME, NUMBER, STREET, ACCURACY_TYPE,
        SOURCE, ASOF, LATITUDE, LONGITUDE, COUNTY_FIPS
FROM    EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
WHERE   COUNTY_FIPS IN ('36005','36047','36061','36085')   -- Queens excluded
  AND   PROPERTY_ADDRESS RLIKE '.*[0-9]+ ?[-/] ?[0-9]+ .*'
ORDER BY PROPERTY_ADDRESS
LIMIT 25;

-- ===========================================================================
-- CASE 4 -- MULTI-ADDRESS FIELD, TWO OR MORE STREETS.
-- Six separators observed in the wild, not one. Measured counts, five boroughs:
--   '/' 297 | ',' 129 | '&' 93 | ' AND ' 44 | ' A/K/A ' 9 | plus bare whitespace
-- Together roughly 7-8% of the NYC book, which makes this a population, not a tail.
-- bd-1zph scopes normalization and ranges; it does NOT scope multi-address fields.
--
-- ALSO DETECTS THE CHIMERA PARSE: rows where the parsed NUMBER + STREET pair does not
-- appear in PROPERTY_ADDRESS at all -- a fabricated address no source asserts.
-- Confirmed instances: "241/249 West 74th Street" -> "241 W 49th St" at ROOFTOP tier;
-- "47-19/47-27 a/k/a 47-27 Little Neck Parkway" -> NUMBER "47-10".
-- This check is purely local, needs no external data, and sizes a silent-error
-- population the accuracy_type tiers do not capture at all.
-- ===========================================================================
WITH scope AS (
    SELECT *
    FROM   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE  COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
),
sep AS (
    SELECT  *,
            CASE WHEN PROPERTY_ADDRESS ILIKE '% A/K/A %' THEN 'a_k_a'
                 WHEN PROPERTY_ADDRESS LIKE  '%/%'       THEN 'slash'
                 WHEN PROPERTY_ADDRESS LIKE  '%,%'       THEN 'comma'
                 WHEN PROPERTY_ADDRESS LIKE  '%&%'       THEN 'ampersand'
                 WHEN PROPERTY_ADDRESS ILIKE '% AND %'   THEN 'and'
                 WHEN PROPERTY_ADDRESS RLIKE '.*[0-9]  +[0-9].*' THEN 'whitespace'
                 ELSE 'none' END AS separator
    FROM scope
)
SELECT  separator,
        COUNT(*)                                          AS rows_all,
        COUNT(DISTINCT PROPERTY_ADDRESS)                  AS distinct_addresses,
        -- chimera: parsed street not present in the asserted address
        SUM(CASE WHEN STREET IS NOT NULL
                  AND PROPERTY_ADDRESS NOT ILIKE '%' || STREET || '%'
                 THEN 1 ELSE 0 END)                       AS chimera_street_rows,
        -- never parsed at all: the hardest rows, invisible to the hit-rate metric
        SUM(CASE WHEN NUMBER IS NULL AND STREET IS NULL THEN 1 ELSE 0 END)
                                                          AS never_parsed_rows
FROM    sep
GROUP BY 1
ORDER BY rows_all DESC;

-- ===========================================================================
-- CASE 5 -- CORNER BUILDING, TWO ADDRESSES, ONE STRUCTURE.
-- THE INVERSE OF CASES 3 AND 4: here the address channel ACTIVELY MISLEADS. Two
-- completely different addresses denote the same building, and address disagreement
-- must be allowed to LOSE to geometry.
-- Cases 4 and 5 together are the calibration pair for bd-uilx's channel-agreement rule:
-- one where the address decides, one where it must not.
-- Confirmed candidates: "66 Crosby Street a/k/a 514 Broadway";
-- "271 EAST 197TH STREET A/K/A 2825 BAINBRIDGE AVENUE";
-- "57 PRINCE STREET A/K/A 273-279 LAFAYETTE STREET".
-- ===========================================================================
WITH aka AS (
    SELECT  PROPERTY_ADDRESS, PROPERTY_NAME, NUMBER, STREET, ACCURACY_TYPE,
            SOURCE, ASOF, LATITUDE, LONGITUDE, COUNTY_FIPS
    FROM    EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE   COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND   PROPERTY_ADDRESS ILIKE '% A/K/A %'
      AND   LATITUDE IS NOT NULL
)
SELECT  a.*,
        p.BBL, p.ADDRESS AS pluto_address, p.BLDGCLASS, p.NUMBLDGS,
        p.BLDGAREA, p.LOTAREA, p.YEARBUILT, p.OWNERNAME,
        -- does MapPLUTO's single address match EITHER asserted frontage?
        CASE WHEN a.PROPERTY_ADDRESS ILIKE '%' || p.ADDRESS || '%'
             THEN 'pluto_address_is_one_of_the_asserted'
             ELSE 'pluto_address_matches_NEITHER' END AS address_set_finding
FROM    aka a
LEFT JOIN EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  ON    ST_CONTAINS(p.GEOM_GEOG, ST_POINT(a.LONGITUDE, a.LATITUDE))
ORDER BY a.PROPERTY_ADDRESS;

-- ===========================================================================
-- CASE 6 -- N BUILDINGS TO ONE PARCEL.
-- Geometry cannot discriminate: every candidate building is inside the same lot, so
-- point-in-polygon is useless past the first step. Size, type and POI/tenant evidence
-- must carry it -- which is why the BIN bridge and Overture places matter here.
-- This is also where the FALSE-MERGE risk lives (bd-1oy8): resolving to the BBL merges
-- buildings that may sit in entirely different corpora.
-- ===========================================================================
WITH scope AS (
    SELECT *
    FROM   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE  COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND  LATITUDE IS NOT NULL
)
SELECT  s.PROPERTY_ADDRESS, s.PROPERTY_NAME, s.ACCURACY_TYPE, s.SOURCE, s.ASOF,
        s.LATITUDE, s.LONGITUDE,
        p.BBL, p.ADDRESS AS pluto_address, p.BLDGCLASS, p.LANDUSE,
        p.NUMBLDGS, p.BLDGAREA, p.LOTAREA, p.NUMFLOORS, p.YEARBUILT, p.OWNERNAME
FROM    scope s
JOIN    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
  ON    ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
WHERE   p.NUMBLDGS >= 2
ORDER BY p.NUMBLDGS DESC, p.BLDGAREA DESC
LIMIT 25;

-- ===========================================================================
-- CASE 7 -- M PARCELS TO N BUILDINGS, AND THE UNDECIDABLE MEMBER OF THE CORPUS.
-- The hardest rows never parse at all: multi-street, multi-range, Queens-grid
-- hyphenates throughout, degrading to accuracy_type 'place' with NULL NUMBER and
-- NULL STREET. Confirmed instance:
--   "95-38, 95-40 to 95-44, 96-42 to 96-70 & 95-56 to 95-60 Queens Boulevard,
--    63-73 to 63-79 Saunders Street, 94-14 to 94-24 and 95-11 to 95-19 63rd Drive"
--
-- DENOMINATOR FINDING: these rows NEVER ENTER THE GEOMETRIC PIPELINE. A place centroid
-- is not a usable point, so they are neither hits nor misses in the 95.49% hit rate --
-- they are ABSENT from it. And they are precisely the rows where the property is most
-- complex and the answer is worth the most. bd-14co must count them explicitly.
-- ===========================================================================
SELECT  ACCURACY_TYPE,
        COUNT(*)                                             AS rows_all,
        COUNT(DISTINCT PROPERTY_ADDRESS)                     AS distinct_addresses,
        SUM(CASE WHEN NUMBER IS NULL AND STREET IS NULL THEN 1 ELSE 0 END)
                                                             AS never_parsed,
        SUM(CASE WHEN PROPERTY_ADDRESS LIKE '%,%'
                   OR PROPERTY_ADDRESS LIKE '%&%'
                   OR PROPERTY_ADDRESS ILIKE '% AND %' THEN 1 ELSE 0 END)
                                                             AS multi_address_rows
FROM    EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
WHERE   COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
GROUP BY 1
ORDER BY rows_all DESC;

-- ===========================================================================
-- CASE 8 -- MULTI-CONTAINMENT. One point inside two or more legitimate lots.
-- 237 Park Avenue: a ROOFTOP geocode inside BBL 1012920026 and 1012920001, both
-- ST_CONTAINS true. 157 five-borough CMBS points fall in more than one lot, max 4,
-- and the population is largely condo unit BBLs overlapping their parent lot.
--
-- NEVER COLLAPSE WITH MAX / LIMIT 1 / FIRST-RETURNED. The whole point of this case is
-- that `within` must return a SET. Emitting one match silently discards a real
-- ambiguity. The multi-lot RATE is a first-class metric (bd-bmee).
-- ===========================================================================
WITH scope AS (
    SELECT DISTINCT PROPERTY_ADDRESS, PROPERTY_NAME, SOURCE, ASOF,
           ACCURACY_TYPE, LATITUDE, LONGITUDE
    FROM   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE  COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND  LATITUDE IS NOT NULL
),
multi AS (
    SELECT  s.PROPERTY_ADDRESS, s.PROPERTY_NAME, s.SOURCE, s.ASOF, s.ACCURACY_TYPE,
            s.LATITUDE, s.LONGITUDE,
            COUNT(DISTINCT p.BBL)          AS lots_containing,
            ARRAY_AGG(DISTINCT p.BBL)      AS bbl_set,
            ARRAY_AGG(DISTINCT p.ADDRESS)  AS address_set,
            ARRAY_AGG(DISTINCT p.OWNERNAME) AS owner_set
    FROM    scope s
    JOIN    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON    ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
    GROUP BY 1,2,3,4,5,6,7
)
SELECT * FROM multi
WHERE  lots_containing >= 2
ORDER BY lots_containing DESC, PROPERTY_ADDRESS;

-- Headline rate for bd-bmee, deduplicated to distinct points:
SELECT  lots_containing, COUNT(*) AS points
FROM   (SELECT s.LATITUDE, s.LONGITUDE, COUNT(DISTINCT p.BBL) AS lots_containing
        FROM  (SELECT DISTINCT LATITUDE, LONGITUDE
               FROM  EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
               WHERE COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
                 AND LATITUDE IS NOT NULL) s
        JOIN   EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
          ON   ST_CONTAINS(p.GEOM_GEOG, ST_POINT(s.LONGITUDE, s.LATITUDE))
        GROUP BY 1,2)
GROUP BY 1 ORDER BY 1;

-- ===========================================================================
-- CASE 9 -- MULTI-CORPUS SHARED PARCEL. The false-merge case.
-- BLOCKED, AND THE BLOCKER IS NOW NAMED: the geocode table carries NO deal, loan or
-- property identifier column at all (verified 2026-08-14). So "which CMBS position
-- does this belong to" is unanswerable from this table, and the false-merge analysis
-- -- which turns on positions from DIFFERENT corpora sharing a parcel -- requires an
-- upstream join nobody had identified as a prerequisite.
--
-- STEP 1 IS TO FIND THAT UPSTREAM TABLE. Until then this selector can only measure the
-- SHAPE of the exposure: how many distinct property assertions share a BBL.
-- ===========================================================================
WITH pts AS (
    SELECT DISTINCT PROPERTY_ADDRESS, PROPERTY_NAME, LATITUDE, LONGITUDE
    FROM   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
    WHERE  COUNTY_FIPS IN ('36005','36047','36061','36081','36085')
      AND  LATITUDE IS NOT NULL
),
by_bbl AS (
    SELECT  p.BBL, p.ADDRESS AS pluto_address, p.NUMBLDGS, p.OWNERNAME,
            COUNT(DISTINCT pts.PROPERTY_ADDRESS) AS distinct_assertions,
            ARRAY_AGG(DISTINCT pts.PROPERTY_ADDRESS) AS asserted_addresses
    FROM    pts
    JOIN    EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT p
      ON    ST_CONTAINS(p.GEOM_GEOG, ST_POINT(pts.LONGITUDE, pts.LATITUDE))
    GROUP BY 1,2,3,4
)
SELECT * FROM by_bbl
WHERE  distinct_assertions >= 2          -- two or more properties resolving to one BBL
ORDER BY distinct_assertions DESC, NUMBLDGS DESC
LIMIT 50;
