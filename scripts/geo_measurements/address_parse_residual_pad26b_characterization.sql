-- bd-23ux Canon Geo PAD 26B address residual characterization.
--
-- Purpose:
--   Characterize the 1,339 PAD-unresolved address-county keys measured by
--   bd-158y without changing the denominator. This is not parser acceptance
--   proof and not a reduced count; it is a disjoint/exhaustive explanation of
--   the current residual over the same 5,269-key PAD 26B control population.
--
-- Execution contract:
--   One read-only SELECT. No session variables. The query emits class rows only
--   when the original denominator, release pin, PAD source hash singularity,
--   and class integrity checks hold. A drifted release, filtered corpus,
--   non-exhaustive classifier, overlapping classifier, or truncated/canceled
--   run is not positive evidence.
--
-- Pinned sources:
--   EDGAR_DB.DBT_WRANGLING_EDGAR.WRGL_EDGAR_CMBS_GEOCODES__STRUCTURED
--     query-as-of cutoff: ASOF <= 2026-08-01
--   EDGAR_DB.SOURCE.NYC_DCP_PAD_ADDRESS_HOT
--     PAD release: 26B, RELEASE_DT 2026-05-01, IS_CURRENT_RELEASE = TRUE

WITH params AS (
  SELECT
    'canon_geo_address_parse_residual_characterization.v0' AS contract,
    'bd-23ux_pad26b_residual_characterization_live' AS measurement_id,
    DATE '2026-09-02' AS query_as_of,
    DATE '2026-08-01' AS geocode_asof_cutoff,
    '26B' AS pad_release,
    DATE '2026-05-01' AS pad_release_dt,
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
       AND REGEXP_LIKE(REGEXP_REPLACE(UPPER(RAW_NUM), '[^0-9A-Z-]+', ''), '[0-9]+-[0-9]+[A-Z]?')
        THEN TRY_TO_NUMBER(REGEXP_REPLACE(RAW_NUM, '[^0-9]+', ''))
      ELSE TRY_TO_NUMBER(REGEXP_SUBSTR(RAW_NUM, '[0-9]+', 1, 1))
    END AS LO_NUM,
    CASE
      WHEN COUNTY_FIPS = '36081'
       AND REGEXP_LIKE(REGEXP_REPLACE(UPPER(RAW_NUM), '[^0-9A-Z-]+', ''), '[0-9]+-[0-9]+[A-Z]?')
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
street_support AS (
  SELECT
    ak.PROPERTY_ADDRESS,
    ak.COUNTY_FIPS,
    COUNT(DISTINCT p.BBL_KEY) AS PAD_STREET_BBLS
  FROM ak
  JOIN i_st AS i
    ON i.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND i.COUNTY_FIPS = ak.COUNTY_FIPS
  JOIN p_st AS p
    ON p.BORO = ak.BORO
   AND p.STREET_NORM = i.STREET_NORM
  GROUP BY ak.PROPERTY_ADDRESS, ak.COUNTY_FIPS
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
    COALESCE(MAX(ss.PAD_STREET_BBLS), 0) AS PAD_STREET_BBLS,
    COALESCE(MAX(nf.HAS_QN_HYPHEN), 0) AS HAS_QN_HYPHEN,
    COALESCE(MAX(nf.HAS_NON_QN_HYPHEN), 0) AS HAS_NON_QN_HYPHEN,
    COALESCE(MAX(nf.HAS_SLASH), 0) AS HAS_SLASH,
    COALESCE(MAX(nf.HAS_PAIR_WORD), 0) AS HAS_PAIR_WORD,
    MAX(IFF(REGEXP_LIKE(UPPER(ak.PROPERTY_ADDRESS), '.*(A[^A-Z0-9]*K[^A-Z0-9]*A|AKA|ALSO KNOWN|FKA|FORMERLY KNOWN|;|,.*[0-9]).*'), 1, 0)) AS HAS_ALIAS_OR_MULTI_TEXT,
    MAX(IFF(REGEXP_LIKE(UPPER(ak.PROPERTY_ADDRESS), '.*(P[[:space:]]*O[[:space:]]*BOX|POST OFFICE BOX|LOCKBOX|BOX[[:space:]]+[0-9]).*'), 1, 0)) AS HAS_MAILBOX_TEXT,
    MAX(IFF(REGEXP_LIKE(UPPER(ak.PROPERTY_ADDRESS), '(VARIOUS|MULTIPLE|BROOKLYN PACKAGE|MANHATTAN PACKAGE|QUEENS PACKAGE|BRONX PACKAGE|STATEN ISLAND PACKAGE|C/O|CARE OF).*'), 1, 0)) AS HAS_PLACEHOLDER_TEXT
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
  LEFT JOIN street_support AS ss
    ON ss.PROPERTY_ADDRESS = ak.PROPERTY_ADDRESS
   AND ss.COUNTY_FIPS = ak.COUNTY_FIPS
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
residual AS (
  SELECT *
  FROM ks
  WHERE PAD_BBLS = 0
),
classified AS (
  SELECT
    *,
    CASE
      WHEN HAS_PLACEHOLDER_TEXT = 1 OR HAS_MAILBOX_TEXT = 1 THEN 'placeholder_or_non_street_delivery_form'
      WHEN HAS_ALIAS_OR_MULTI_TEXT = 1 THEN 'alias_or_multi_address_string'
      WHEN HAS_QN_HYPHEN = 1 THEN 'queens_hyphenate_unmatched'
      WHEN HAS_NON_QN_HYPHEN = 1 OR HAS_SLASH = 1 OR HAS_PAIR_WORD = 1 THEN 'compound_or_range_house_number'
      WHEN INPUT_STREETS = 0 THEN 'missing_structured_street'
      WHEN INPUT_NUMBER_RANGES = 0 THEN 'missing_or_unparsed_house_number'
      WHEN PAD_STREET_BBLS > 0 THEN 'pad_street_present_number_absent'
      ELSE 'street_not_seen_in_pad_borough'
    END AS residual_class,
    CASE
      WHEN HAS_PLACEHOLDER_TEXT = 1 OR HAS_MAILBOX_TEXT = 1 THEN 'structurally_unresolvable'
      WHEN HAS_ALIAS_OR_MULTI_TEXT = 1 THEN 'fixable_here'
      WHEN HAS_QN_HYPHEN = 1 THEN 'fixable_here'
      WHEN HAS_NON_QN_HYPHEN = 1 OR HAS_SLASH = 1 OR HAS_PAIR_WORD = 1 THEN 'fixable_here'
      WHEN INPUT_STREETS = 0 THEN 'fixable_here'
      WHEN INPUT_NUMBER_RANGES = 0 THEN 'fixable_here'
      WHEN PAD_STREET_BBLS > 0 THEN 'fixable_upstream'
      ELSE 'fixable_here'
    END AS disposition,
    CASE
      WHEN HAS_PLACEHOLDER_TEXT = 1 OR HAS_MAILBOX_TEXT = 1 THEN 'Placeholder package names, care-of labels, or mailbox/non-street delivery forms are not legal parcel address keys.'
      WHEN HAS_ALIAS_OR_MULTI_TEXT = 1 THEN 'One source key names aliases, condominium/unit forms, or multiple addresses; query-side parsing must split all readings before PAD membership.'
      WHEN HAS_QN_HYPHEN = 1 THEN 'Queens hyphenated house number survived normalization but found no PAD member under the current street/range predicate.'
      WHEN HAS_NON_QN_HYPHEN = 1 OR HAS_SLASH = 1 OR HAS_PAIR_WORD = 1 THEN 'Compound, slash, or range house-number expression needs a multi-reading parse or an upstream source correction.'
      WHEN INPUT_STREETS = 0 THEN 'Structured geocoder row did not expose a street token, so the raw address string has to be parsed by Canon before PAD lookup.'
      WHEN INPUT_NUMBER_RANGES = 0 THEN 'Structured geocoder row did not expose a parseable house number/range.'
      WHEN PAD_STREET_BBLS > 0 THEN 'The normalized street exists in PAD for the borough, but the house-number range has no current PAD row.'
      ELSE 'The normalized input street is not present in PAD for the borough under this tokenizer/SND-normalization path.'
    END AS class_description
  FROM residual
),
classified_ranked AS (
  SELECT
    *,
    ROW_NUMBER() OVER (
      PARTITION BY residual_class
      ORDER BY COUNTY_FIPS, PROPERTY_ADDRESS
    ) AS class_example_rank
  FROM classified
),
class_counts AS (
  SELECT
    residual_class,
    MIN(disposition) AS disposition,
    MIN(class_description) AS class_description,
    COUNT(*) AS key_count,
    MAX(IFF(class_example_rank = 1, PROPERTY_ADDRESS || '|' || COUNTY_FIPS, NULL)) AS example_key_1,
    MAX(IFF(class_example_rank = 2, PROPERTY_ADDRESS || '|' || COUNTY_FIPS, NULL)) AS example_key_2,
    MAX(IFF(class_example_rank = 3, PROPERTY_ADDRESS || '|' || COUNTY_FIPS, NULL)) AS example_key_3
  FROM classified_ranked
  GROUP BY residual_class
),
integrity AS (
  SELECT
    COALESCE(SUM(key_count), 0) AS classified_total,
    COUNT(*) AS class_rows,
    COUNT(DISTINCT residual_class) AS distinct_class_rows
  FROM class_counts
),
key_integrity AS (
  SELECT
    COUNT(*) AS classified_key_rows,
    COUNT(DISTINCT PROPERTY_ADDRESS || '|' || COUNTY_FIPS) AS distinct_classified_keys,
    COUNT_IF(residual_class IS NULL) AS unclassified_key_rows
  FROM classified
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
      AND i.classified_total = p.expected_pad_unresolved_keys
      AND i.class_rows = i.distinct_class_rows
      AND k.classified_key_rows = k.distinct_classified_keys
      AND k.unclassified_key_rows = 0
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
      WHEN i.classified_total <> p.expected_pad_unresolved_keys THEN 'classification_not_exhaustive'
      WHEN i.class_rows <> i.distinct_class_rows THEN 'classification_overlapping_or_duplicate_class'
      WHEN k.classified_key_rows <> k.distinct_classified_keys THEN 'classification_overlapping_key'
      WHEN k.unclassified_key_rows <> 0 THEN 'classification_unclassified_key'
      WHEN s.pad_address_source_hash_count <> 1 THEN 'pad_source_hash_not_singular'
      ELSE NULL
    END AS guard_detail
  FROM params AS p,
       totals AS t,
       source_hashes AS s,
       integrity AS i,
       key_integrity AS k
),
class_rows AS (
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
    CASE cc.residual_class
      WHEN 'placeholder_or_non_street_delivery_form' THEN 1
      WHEN 'alias_or_multi_address_string' THEN 2
      WHEN 'queens_hyphenate_unmatched' THEN 3
      WHEN 'compound_or_range_house_number' THEN 4
      WHEN 'missing_structured_street' THEN 5
      WHEN 'missing_or_unparsed_house_number' THEN 6
      WHEN 'pad_street_present_number_absent' THEN 7
      WHEN 'street_not_seen_in_pad_borough' THEN 8
      ELSE 99
    END AS class_order,
    cc.residual_class,
    cc.disposition,
    cc.class_description,
    cc.key_count,
    ROUND(100.0 * cc.key_count / NULLIF(t.pad_unresolved_keys, 0), 2) AS pct_of_residual,
    cc.example_key_1,
    cc.example_key_2,
    cc.example_key_3,
    t.address_county_keys,
    t.parsed_street_keys,
    t.parsed_number_keys,
    t.pad_matched_keys,
    t.pad_unresolved_keys,
    t.pad_unique_keys,
    t.pad_multi_bbl_keys,
    t.pad_bbl_edges,
    t.max_pad_bbls_per_key,
    i.classified_total,
    i.class_rows,
    i.distinct_class_rows,
    k.classified_key_rows,
    k.distinct_classified_keys,
    k.unclassified_key_rows,
    s.pad_address_source_zip_sha256,
    s.pad_address_rows
  FROM class_counts AS cc,
       params AS p,
       totals AS t,
       source_hashes AS s,
       integrity AS i,
       key_integrity AS k,
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
    CAST(NULL AS NUMBER) AS class_order,
    CAST(NULL AS TEXT) AS residual_class,
    CAST(NULL AS TEXT) AS disposition,
    CAST(NULL AS TEXT) AS class_description,
    CAST(NULL AS NUMBER) AS key_count,
    CAST(NULL AS NUMBER) AS pct_of_residual,
    CAST(NULL AS TEXT) AS example_key_1,
    CAST(NULL AS TEXT) AS example_key_2,
    CAST(NULL AS TEXT) AS example_key_3,
    t.address_county_keys,
    t.parsed_street_keys,
    t.parsed_number_keys,
    t.pad_matched_keys,
    t.pad_unresolved_keys,
    t.pad_unique_keys,
    t.pad_multi_bbl_keys,
    t.pad_bbl_edges,
    t.max_pad_bbls_per_key,
    i.classified_total,
    i.class_rows,
    i.distinct_class_rows,
    k.classified_key_rows,
    k.distinct_classified_keys,
    k.unclassified_key_rows,
    s.pad_address_source_zip_sha256,
    s.pad_address_rows
  FROM params AS p,
       totals AS t,
       source_hashes AS s,
       integrity AS i,
       key_integrity AS k,
       guard AS g
  WHERE NOT g.guard_ok
)
SELECT *
FROM class_rows
UNION ALL
SELECT *
FROM guard_failure_rows
ORDER BY row_status, class_order
