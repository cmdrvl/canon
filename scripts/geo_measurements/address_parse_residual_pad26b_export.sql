-- bd-158y Canon Geo address-parse residual: bounded live PAD unresolved export.
--
-- Purpose:
--   Produce a bounded, non-fixture export of the measured query-side address
--   parse residual identified in PLAN_CANON_GEO Appendix M:
--   5,269 raw (PROPERTY_ADDRESS, COUNTY_FIPS) keys, PAD 26B range-aware
--   membership, and 1,339 PAD-unresolved keys. This is not a parser output and
--   not a full canon runtime artifact; it is a measured corpus handoff for the
--   bd-158y grammar work.
--
-- Execution contract:
--   One read-only SELECT. No session variables. The default export_row_cap is 25
--   so the query is a bounded positive capability. The full residual denominator
--   remains 1,339 and is never reduced to the emitted row cap.
--
-- Pinned sources:
--   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
--     query-as-of cutoff: ASOF <= 2026-08-01
--   EDGAR_DB.SOURCE.NYC_DCP_PAD_ADDRESS_HOT
--     PAD release: 26B, RELEASE_DT 2026-05-01, IS_CURRENT_RELEASE = TRUE
--
-- Live receipts:
--   01c6c258-0821-aa0e-006c-c703088ec5da reproduced the 5,269 / 3,930 /
--   1,339 / 2,337 / 1,593 / 6,469 control in 3,000 ms.
--   01c6c25d-0821-ab8c-006c-c703088f36ce returned 25 bounded rows in
--   8,163 ms with the corrected denominator-vs-row-cap guard.
--   01c6c25b-0821-ab8c-006c-c703088f34da attempted a full 1,339-row export and
--   was canceled at 45 s; it is discarded evidence and not a positive receipt.

WITH params AS (
  SELECT
    'canon_geo_address_parse_residual_export.v0' AS contract,
    'bd-158y_address_parse_residual_pad26b_live' AS measurement_id,
    DATE '2026-08-31' AS query_as_of,
    DATE '2026-08-01' AS geocode_asof_cutoff,
    '26B' AS pad_release,
    DATE '2026-05-01' AS pad_release_dt,
    25 AS export_row_cap,
    5269 AS expected_address_county_keys,
    3930 AS expected_pad_matched_keys,
    1339 AS expected_pad_unresolved_keys,
    2337 AS expected_pad_unique_keys,
    1593 AS expected_pad_multi_bbl_keys,
    6469 AS expected_pad_bbl_edges
),
scope_rows AS (
  SELECT
    g.PROPERTY_ADDRESS,
    g.COUNTY_FIPS,
    g.ACCURACY_TYPE,
    g.NUMBER,
    g.STREET,
    g.ASOF
  FROM EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED AS g,
       params AS p
  WHERE g.COUNTY_FIPS IN ('36005', '36047', '36061', '36081', '36085')
    AND g.PROPERTY_ADDRESS IS NOT NULL
    AND g.ASOF IS NOT NULL
    AND g.ASOF <= p.geocode_asof_cutoff
),
ak AS (
  SELECT
    PROPERTY_ADDRESS,
    COUNTY_FIPS,
    CASE COUNTY_FIPS
      WHEN '36005' THEN 2
      WHEN '36047' THEN 3
      WHEN '36061' THEN 1
      WHEN '36081' THEN 4
      WHEN '36085' THEN 5
    END AS BORO,
    CASE
      WHEN COUNT(DISTINCT ACCURACY_TYPE) = 1 THEN MIN(ACCURACY_TYPE)
      ELSE 'mixed'
    END AS ACCURACY_TYPE,
    MIN(ASOF) AS MIN_GEO_ASOF,
    MAX(ASOF) AS MAX_GEO_ASOF
  FROM scope_rows
  GROUP BY PROPERTY_ADDRESS, COUNTY_FIPS
),
raw_nums AS (
  SELECT PROPERTY_ADDRESS, COUNTY_FIPS, NUMBER AS RAW_NUM
  FROM scope_rows
  WHERE NUMBER IS NOT NULL
  UNION ALL
  SELECT
    PROPERTY_ADDRESS,
    COUNTY_FIPS,
    REGEXP_SUBSTR(
      PROPERTY_ADDRESS,
      '^[[:space:]]*[0-9]+[A-Z]?[[:space:]]*((-|/|&|AND)[[:space:]]*[0-9]+[A-Z]?)?'
    ) AS RAW_NUM
  FROM ak
),
num_flags AS (
  SELECT
    PROPERTY_ADDRESS,
    COUNTY_FIPS,
    MAX(IFF(POSITION('-' IN COALESCE(RAW_NUM, '')) > 0, 1, 0)) AS HAS_HYPHEN,
    MAX(IFF(COUNTY_FIPS = '36081' AND POSITION('-' IN COALESCE(RAW_NUM, '')) > 0, 1, 0)) AS HAS_QN_HYPHEN,
    MAX(IFF(COUNTY_FIPS <> '36081' AND POSITION('-' IN COALESCE(RAW_NUM, '')) > 0, 1, 0)) AS HAS_NON_QN_HYPHEN,
    MAX(IFF(POSITION('/' IN COALESCE(RAW_NUM, '')) > 0, 1, 0)) AS HAS_SLASH,
    MAX(IFF(POSITION('&' IN COALESCE(RAW_NUM, '')) > 0 OR UPPER(COALESCE(RAW_NUM, '')) LIKE '%AND%', 1, 0)) AS HAS_PAIR_WORD
  FROM raw_nums
  WHERE RAW_NUM IS NOT NULL
  GROUP BY PROPERTY_ADDRESS, COUNTY_FIPS
),
nums AS (
  SELECT DISTINCT
    PROPERTY_ADDRESS,
    COUNTY_FIPS,
    CASE
      WHEN COUNTY_FIPS = '36081'
       AND REGEXP_LIKE(REGEXP_REPLACE(UPPER(RAW_NUM), '[^0-9A-Z-]+', ''), '^[0-9]+-[0-9]+[A-Z]?$')
        THEN TRY_TO_NUMBER(REGEXP_REPLACE(RAW_NUM, '[^0-9]+', ''))
      ELSE TRY_TO_NUMBER(REGEXP_SUBSTR(RAW_NUM, '[0-9]+', 1, 1))
    END AS LO_NUM,
    CASE
      WHEN COUNTY_FIPS = '36081'
       AND REGEXP_LIKE(REGEXP_REPLACE(UPPER(RAW_NUM), '[^0-9A-Z-]+', ''), '^[0-9]+-[0-9]+[A-Z]?$')
        THEN TRY_TO_NUMBER(REGEXP_REPLACE(RAW_NUM, '[^0-9]+', ''))
      ELSE COALESCE(
        TRY_TO_NUMBER(REGEXP_SUBSTR(RAW_NUM, '[0-9]+', 1, 2)),
        TRY_TO_NUMBER(REGEXP_SUBSTR(RAW_NUM, '[0-9]+', 1, 1))
      )
    END AS HI_NUM
  FROM raw_nums
  WHERE RAW_NUM IS NOT NULL
),
street_src AS (
  SELECT
    'i' AS SRC,
    PROPERTY_ADDRESS,
    COUNTY_FIPS,
    CAST(NULL AS NUMBER) AS BORO,
    CAST(NULL AS TEXT) AS BBL_KEY,
    CAST(NULL AS TEXT) AS BIN_KEY,
    CAST(NULL AS NUMBER) AS LOW_INT,
    CAST(NULL AS NUMBER) AS HIGH_INT,
    STREET AS RAW_STREET
  FROM scope_rows
  WHERE STREET IS NOT NULL
  UNION ALL
  SELECT
    'p',
    CAST(NULL AS TEXT),
    CAST(NULL AS TEXT),
    a.BORO,
    a.BBL_KEY,
    a.BIN_KEY,
    a.LOW_HOUSE_NUMBER_INT,
    COALESCE(a.HIGH_HOUSE_NUMBER_INT, a.LOW_HOUSE_NUMBER_INT),
    a.STREET_NAME_NORMALIZED
  FROM EDGAR_DB.SOURCE.NYC_DCP_PAD_ADDRESS_HOT AS a,
       params AS p
  WHERE a.RELEASE = p.pad_release
    AND a.RELEASE_DT = p.pad_release_dt
    AND a.IS_CURRENT_RELEASE = TRUE
    AND a.BBL_KEY IS NOT NULL
    AND a.LOW_HOUSE_NUMBER_INT IS NOT NULL
    AND a.STREET_NAME_NORMALIZED IS NOT NULL
),
tok AS (
  SELECT
    SRC,
    PROPERTY_ADDRESS,
    COUNTY_FIPS,
    BORO,
    BBL_KEY,
    BIN_KEY,
    LOW_INT,
    HIGH_INT,
    RAW_STREET,
    t.index,
    DECODE(
      t.value,
      'NORTH', 'N',
      'SOUTH', 'S',
      'EAST', 'E',
      'WEST', 'W',
      'STREET', 'ST',
      'AVENUE', 'AVE',
      'ROAD', 'RD',
      'BOULEVARD', 'BLVD',
      'PLACE', 'PL',
      'DRIVE', 'DR',
      'LANE', 'LN',
      'COURT', 'CT',
      'PARKWAY', 'PKWY',
      'HIGHWAY', 'HWY',
      'TERRACE', 'TER',
      'CIRCLE', 'CIR',
      'EXPRESSWAY', 'EXPY',
      'PLAZA', 'PLZ',
      'FIRST', '1',
      'SECOND', '2',
      'THIRD', '3',
      'FOURTH', '4',
      'FIFTH', '5',
      'SIXTH', '6',
      'SEVENTH', '7',
      'EIGHTH', '8',
      'NINTH', '9',
      'TENTH', '10',
      'ELEVENTH', '11',
      'TWELFTH', '12',
      REGEXP_REPLACE(t.value, '^([0-9]+)(ST|ND|RD|TH)$', '\\1')
    ) AS V
  FROM street_src,
       LATERAL SPLIT_TO_TABLE(REGEXP_REPLACE(UPPER(COALESCE(RAW_STREET, '')), '[^A-Z0-9]+', ' '), ' ') AS t
  WHERE t.value <> ''
),
st AS (
  SELECT
    SRC,
    PROPERTY_ADDRESS,
    COUNTY_FIPS,
    BORO,
    BBL_KEY,
    BIN_KEY,
    LOW_INT,
    HIGH_INT,
    RAW_STREET,
    LISTAGG(V, ' ') WITHIN GROUP (ORDER BY index) AS STREET_NORM
  FROM tok
  GROUP BY SRC, PROPERTY_ADDRESS, COUNTY_FIPS, BORO, BBL_KEY, BIN_KEY, LOW_INT, HIGH_INT, RAW_STREET
),
i_st AS (
  SELECT DISTINCT PROPERTY_ADDRESS, COUNTY_FIPS, STREET_NORM
  FROM st
  WHERE SRC = 'i'
),
p_st AS (
  SELECT DISTINCT BORO, BBL_KEY, BIN_KEY, LOW_INT, HIGH_INT, STREET_NORM
  FROM st
  WHERE SRC = 'p'
),
pad_edges AS (
  SELECT DISTINCT
    ak.PROPERTY_ADDRESS,
    ak.COUNTY_FIPS,
    p.BBL_KEY
  FROM ak
  JOIN i_st AS i
    ON i.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND i.COUNTY_FIPS = ak.COUNTY_FIPS
  JOIN nums AS n
    ON n.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND n.COUNTY_FIPS = ak.COUNTY_FIPS
  JOIN p_st AS p
    ON p.BORO = ak.BORO
   AND p.STREET_NORM = i.STREET_NORM
   AND n.LO_NUM IS NOT NULL
   AND n.HI_NUM IS NOT NULL
   AND n.LO_NUM <= p.HIGH_INT
   AND n.HI_NUM >= p.LOW_INT
),
ks AS (
  SELECT
    ak.PROPERTY_ADDRESS,
    ak.COUNTY_FIPS,
    ak.BORO,
    ak.ACCURACY_TYPE,
    ak.MIN_GEO_ASOF,
    ak.MAX_GEO_ASOF,
    COUNT(DISTINCT i.STREET_NORM) AS INPUT_STREETS,
    COUNT(DISTINCT n.LO_NUM || '-' || n.HI_NUM) AS INPUT_NUMBER_RANGES,
    COUNT(DISTINCT pe.BBL_KEY) AS PAD_BBLS,
    COALESCE(MAX(nf.HAS_HYPHEN), 0) AS HAS_HYPHEN,
    COALESCE(MAX(nf.HAS_QN_HYPHEN), 0) AS HAS_QN_HYPHEN,
    COALESCE(MAX(nf.HAS_NON_QN_HYPHEN), 0) AS HAS_NON_QN_HYPHEN,
    COALESCE(MAX(nf.HAS_SLASH), 0) AS HAS_SLASH,
    COALESCE(MAX(nf.HAS_PAIR_WORD), 0) AS HAS_PAIR_WORD
  FROM ak
  LEFT JOIN i_st AS i
    ON i.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND i.COUNTY_FIPS = ak.COUNTY_FIPS
  LEFT JOIN nums AS n
    ON n.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND n.COUNTY_FIPS = ak.COUNTY_FIPS
  LEFT JOIN num_flags AS nf
    ON nf.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND nf.COUNTY_FIPS = ak.COUNTY_FIPS
  LEFT JOIN pad_edges AS pe
    ON pe.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND pe.COUNTY_FIPS = ak.COUNTY_FIPS
  GROUP BY
    ak.PROPERTY_ADDRESS,
    ak.COUNTY_FIPS,
    ak.BORO,
    ak.ACCURACY_TYPE,
    ak.MIN_GEO_ASOF,
    ak.MAX_GEO_ASOF
),
totals AS (
  SELECT
    COUNT(*) AS address_county_keys,
    SUM(IFF(INPUT_STREETS > 0, 1, 0)) AS parsed_street_keys,
    SUM(IFF(INPUT_NUMBER_RANGES > 0, 1, 0)) AS parsed_number_keys,
    SUM(IFF(PAD_BBLS > 0, 1, 0)) AS pad_matched_keys,
    SUM(IFF(PAD_BBLS = 0, 1, 0)) AS pad_unresolved_keys,
    SUM(IFF(PAD_BBLS = 1, 1, 0)) AS pad_unique_keys,
    SUM(IFF(PAD_BBLS > 1, 1, 0)) AS pad_multi_bbl_keys,
    SUM(PAD_BBLS) AS pad_bbl_edges,
    MAX(PAD_BBLS) AS max_pad_bbls_per_key
  FROM ks
),
source_hashes AS (
  SELECT
    COUNT(DISTINCT SOURCE_ZIP_SHA256) AS pad_address_source_hash_count,
    MIN(SOURCE_ZIP_SHA256) AS pad_address_source_zip_sha256,
    COUNT(*) AS pad_address_rows
  FROM EDGAR_DB.SOURCE.NYC_DCP_PAD_ADDRESS_HOT AS a,
       params AS p
  WHERE a.RELEASE = p.pad_release
    AND a.RELEASE_DT = p.pad_release_dt
    AND a.IS_CURRENT_RELEASE = TRUE
),
guard AS (
  SELECT
    IFF(
      t.address_county_keys = p.expected_address_county_keys
      AND t.pad_matched_keys = p.expected_pad_matched_keys
      AND t.pad_unresolved_keys = p.expected_pad_unresolved_keys
      AND t.pad_unique_keys = p.expected_pad_unique_keys
      AND t.pad_multi_bbl_keys = p.expected_pad_multi_bbl_keys
      AND t.pad_bbl_edges = p.expected_pad_bbl_edges
      AND t.pad_unresolved_keys > 0
      AND p.export_row_cap > 0
      AND s.pad_address_source_hash_count = 1,
      TRUE,
      FALSE
    ) AS guard_ok,
    CASE
      WHEN t.address_county_keys <> p.expected_address_county_keys THEN 'denominator_drift:address_county_keys'
      WHEN t.pad_matched_keys <> p.expected_pad_matched_keys THEN 'denominator_drift:pad_matched_keys'
      WHEN t.pad_unresolved_keys <> p.expected_pad_unresolved_keys THEN 'denominator_drift:pad_unresolved_keys'
      WHEN t.pad_unique_keys <> p.expected_pad_unique_keys THEN 'denominator_drift:pad_unique_keys'
      WHEN t.pad_multi_bbl_keys <> p.expected_pad_multi_bbl_keys THEN 'denominator_drift:pad_multi_bbl_keys'
      WHEN t.pad_bbl_edges <> p.expected_pad_bbl_edges THEN 'denominator_drift:pad_bbl_edges'
      WHEN t.pad_unresolved_keys <= 0 THEN 'empty_residual_denominator'
      WHEN p.export_row_cap <= 0 THEN 'invalid_export_row_cap'
      WHEN s.pad_address_source_hash_count <> 1 THEN 'pad_source_hash_not_singular'
      ELSE NULL
    END AS guard_detail
  FROM params AS p,
       totals AS t,
       source_hashes AS s
),
export_rows AS (
  SELECT
    ROW_NUMBER() OVER (ORDER BY COUNTY_FIPS, PROPERTY_ADDRESS) AS export_row_number,
    *
  FROM ks
  WHERE PAD_BBLS = 0
),
bounded AS (
  SELECT e.*
  FROM export_rows AS e,
       params AS p
  WHERE e.export_row_number <= p.export_row_cap
),
ok_rows AS (
  SELECT
    p.contract,
    p.measurement_id,
    'live' AS proof_class,
    'ok' AS row_status,
    CAST(NULL AS TEXT) AS guard_detail,
    p.query_as_of,
    p.geocode_asof_cutoff,
    p.pad_release,
    p.pad_release_dt,
    b.export_row_number,
    b.PROPERTY_ADDRESS,
    b.COUNTY_FIPS,
    b.BORO,
    b.ACCURACY_TYPE,
    b.MIN_GEO_ASOF,
    b.MAX_GEO_ASOF,
    b.INPUT_STREETS,
    b.INPUT_NUMBER_RANGES,
    b.HAS_HYPHEN,
    b.HAS_QN_HYPHEN,
    b.HAS_NON_QN_HYPHEN,
    b.HAS_SLASH,
    b.HAS_PAIR_WORD,
    t.address_county_keys,
    t.parsed_street_keys,
    t.parsed_number_keys,
    t.pad_matched_keys,
    t.pad_unresolved_keys,
    t.pad_unique_keys,
    t.pad_multi_bbl_keys,
    t.pad_bbl_edges,
    t.max_pad_bbls_per_key,
    p.export_row_cap,
    COUNT(*) OVER () AS bounded_result_rows,
    s.pad_address_source_zip_sha256,
    'not_exposed_by_wrgl_geocode_table' AS geocode_source_hash_status
  FROM bounded AS b,
       params AS p,
       totals AS t,
       source_hashes AS s,
       guard AS g
  WHERE g.guard_ok
),
guard_failure_rows AS (
  SELECT
    p.contract,
    p.measurement_id,
    'not_positive_evidence' AS proof_class,
    'guard_failure' AS row_status,
    g.guard_detail,
    p.query_as_of,
    p.geocode_asof_cutoff,
    p.pad_release,
    p.pad_release_dt,
    CAST(NULL AS NUMBER) AS export_row_number,
    CAST(NULL AS TEXT) AS PROPERTY_ADDRESS,
    CAST(NULL AS TEXT) AS COUNTY_FIPS,
    CAST(NULL AS NUMBER) AS BORO,
    CAST(NULL AS TEXT) AS ACCURACY_TYPE,
    CAST(NULL AS DATE) AS MIN_GEO_ASOF,
    CAST(NULL AS DATE) AS MAX_GEO_ASOF,
    CAST(NULL AS NUMBER) AS INPUT_STREETS,
    CAST(NULL AS NUMBER) AS INPUT_NUMBER_RANGES,
    CAST(NULL AS NUMBER) AS HAS_HYPHEN,
    CAST(NULL AS NUMBER) AS HAS_QN_HYPHEN,
    CAST(NULL AS NUMBER) AS HAS_NON_QN_HYPHEN,
    CAST(NULL AS NUMBER) AS HAS_SLASH,
    CAST(NULL AS NUMBER) AS HAS_PAIR_WORD,
    t.address_county_keys,
    t.parsed_street_keys,
    t.parsed_number_keys,
    t.pad_matched_keys,
    t.pad_unresolved_keys,
    t.pad_unique_keys,
    t.pad_multi_bbl_keys,
    t.pad_bbl_edges,
    t.max_pad_bbls_per_key,
    p.export_row_cap,
    CAST(0 AS NUMBER) AS bounded_result_rows,
    s.pad_address_source_zip_sha256,
    'not_exposed_by_wrgl_geocode_table' AS geocode_source_hash_status
  FROM params AS p,
       totals AS t,
       source_hashes AS s,
       guard AS g
  WHERE NOT g.guard_ok
)
SELECT *
FROM ok_rows
UNION ALL
SELECT *
FROM guard_failure_rows
ORDER BY row_status, export_row_number
