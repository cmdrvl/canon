-- Appendix H.7 bounded candidate/control SQL.
--
-- This script is acquisition-side SQL, not Canon runtime logic. It emits a
-- non-population, non-evidentiary ACRIS master/party candidate-control payload
-- over the raw-filed NYC collateral slice. The candidate-control stage remains
-- over the 2,974 eligible raw PROPERTYSTATE='NY' loans; selected multi-BBL
-- truth is defined only after ACRIS LEGALS yields truth_parcels/BBLs >= 2. It
-- does not join ACRIS LEGALS or MapPLUTO and therefore cannot emit
-- canon_geo_h7_population_rows.v0, selected multi-BBL truth, candidate parcel
-- sets, or solver-ready population rows. Do
-- not cite this file as the exact SQL text for any 01c6bd* receipt unless a
-- named section is byte/normalization equivalent to the actually executed
-- warehouse query and the receipt carries that section's content hash.
--
-- Load-bearing controls:
-- * Bridge build: 3aed6660-ce1c-46a9-aeb2-7296c134ce8f.
-- * ACRIS release: RELEASE_DT = 2026-08-10.
-- * Filed scope is raw PROPERTYSTATE = 'NY' plus raw PROPERTYCOUNTY mapping.
--   Geocoded COUNTY_FIPS is projected as diagnostic-only metadata.
-- * Mixed-state loans are evaluated only as their NYC filed-collateral slice
--   here; this SQL does not prove full national collateral composition.
-- * Amount equality is exact only after
--   ROUND(value * 100, 0)::NUMBER(38,0).
-- * The round classifier uses the $100k cents lattice
--   MOD(amount_cents, 10000000) = 0, which includes $1m multiples.
-- * The exact lender transform is
--   TRIM(REGEXP_REPLACE(UPPER(name), '[^A-Z0-9 ]', ' ')).
--
-- External fresh controls recorded 2026-08-30, retained here as context only:
-- * 01c6bd17-0821-a0dc-006c-c703088c2796, 197 ms, 7 rows:
--   raw PROPERTYSTATE='NY' + filed county reproduced 2,974 loans =
--   653 non-round + 2,321 round. The raw county-only 3,016 and geocoder
--   COUNTY_FIPS 647/2,291 controls are diagnostic-only.
-- * 01c6bd19-0821-9afc-006c-c703088c0936, 313 ms, 2 rows:
--   originator availability drifted from archived G7; preserve discrepancy.
-- * 01c6bd25-0821-a0dc-006c-c703088c27be, 42,031 ms:
--   fresh round candidate aggregation found 2,317 / 311 / 439 versus
--   archived 2,173 / 182 / 277. The cached repeat is not independent.
-- * 01c6bd28-0821-a0dc-006c-c703088c27c6, 45,044 ms:
--   round legal residual cancelled with 000604/57014; no legal counts from
--   that attempt are admissible.

WITH
params AS (
  SELECT
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    'NY'::TEXT AS property_state,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents
),
filed_county_map AS (
  SELECT * FROM VALUES
    ('NEW YORK', 1), ('MANHATTAN', 1), ('NY061', 1),
    ('BRONX', 2),
    ('KINGS', 3), ('BROOKLYN', 3),
    ('QUEENS', 4),
    ('RICHMOND', 5)
  AS m(propertycounty, filed_borough)
),
bridge_rows AS (
  SELECT
    lip.loan_key,
    lip.property_key,
    lip.originatorname,
    lip.originator_match_text,
    lip.originationdate,
    ROUND(lip.originalloanamount * 100, 0)::NUMBER(38,0) AS amount_cents,
    lip.propertystate AS property_state,
    lip.propertycounty AS propertycounty_raw,
    UPPER(TRIM(lip.propertycounty)) AS propertycounty_norm,
    lip.county_fips AS geocoded_county_fips,
    m.filed_borough,
    lip.loan_property_count
  FROM edgar_db.property_mart.loan_issuance_property lip
  JOIN params p ON lip.build_id = p.bridge_build_id
  LEFT JOIN filed_county_map m
    ON UPPER(TRIM(lip.propertycounty)) = m.propertycounty
),
ny_filed_bridge_rows AS (
  SELECT *
  FROM bridge_rows
  WHERE property_state = (SELECT property_state FROM params)
    AND filed_borough IS NOT NULL
),
loan_counts AS (
  SELECT
    loan_key,
    COUNT(DISTINCT property_key) AS property_keys,
    COUNT(DISTINCT IFF(property_state IS NOT NULL, property_state, NULL)) AS distinct_property_state,
    COUNT(DISTINCT IFF(property_state = 'NY', property_state, NULL)) AS distinct_ny_property_state,
    COUNT(DISTINCT filed_borough) AS distinct_filed_borough,
    COUNT(DISTINCT IFF(originatorname IS NOT NULL, originatorname, NULL)) AS distinct_originatorname,
    COUNT(DISTINCT IFF(originator_match_text IS NOT NULL, originator_match_text, NULL)) AS distinct_originator_match_text,
    COUNT(DISTINCT IFF(originationdate IS NOT NULL, originationdate, NULL)) AS distinct_originationdate,
    COUNT(DISTINCT IFF(amount_cents IS NOT NULL, amount_cents, NULL)) AS distinct_originalloanamount,
    MAX(originationdate) AS max_originationdate,
    MAX(amount_cents) AS max_amount_cents,
    MAX(originatorname) AS max_originatorname,
    MAX(originator_match_text) AS max_originator_match_text,
    ARRAY_AGG(DISTINCT filed_borough) WITHIN GROUP (ORDER BY filed_borough) AS filed_boroughs,
    ARRAY_AGG(DISTINCT geocoded_county_fips) WITHIN GROUP (ORDER BY geocoded_county_fips) AS diagnostic_county_fips
  FROM ny_filed_bridge_rows
  GROUP BY loan_key
),
loan_gate AS (
  SELECT
    loan_key,
    property_keys,
    distinct_property_state,
    distinct_ny_property_state,
    distinct_filed_borough,
    distinct_originatorname,
    distinct_originator_match_text,
    distinct_originationdate,
    distinct_originalloanamount,
    IFF(distinct_originationdate = 1, max_originationdate, NULL) AS originationdate,
    IFF(distinct_originalloanamount = 1, max_amount_cents, NULL) AS amount_cents,
    IFF(distinct_originatorname = 1, max_originatorname, NULL) AS originatorname,
    IFF(distinct_originator_match_text = 1, max_originator_match_text, NULL) AS originator_match_text,
    filed_boroughs,
    diagnostic_county_fips
  FROM loan_counts
),
loan_classification AS (
  SELECT
    *,
    CASE
      WHEN distinct_originationdate = 0 THEN 'discard_missing_originationdate'
      WHEN distinct_originationdate > 1 THEN 'discard_ambiguous_originationdate'
      WHEN distinct_originalloanamount = 0 THEN 'discard_missing_originalloanamount'
      WHEN distinct_originalloanamount > 1 THEN 'discard_ambiguous_originalloanamount'
      WHEN distinct_originatorname > 1 THEN 'discard_ambiguous_originatorname'
      WHEN distinct_originator_match_text > 1 THEN 'discard_ambiguous_originator_match_text'
      WHEN distinct_filed_borough = 0 THEN 'discard_no_mapped_filed_borough'
      ELSE 'admissible_for_plane_classification'
    END AS gate_status,
    CASE
      WHEN amount_cents IS NULL THEN NULL
      WHEN MOD(amount_cents, (SELECT round_amount_lattice_cents FROM params)) = 0
        THEN 'round_exact_lender_party'
      ELSE 'non_round_amount_date_legal_borough'
    END AS truth_plane
  FROM loan_gate
),
ny_universe_control AS (
  SELECT
    truth_plane,
    COUNT(*) AS eligible_loans
  FROM loan_classification
  WHERE gate_status = 'admissible_for_plane_classification'
  GROUP BY truth_plane
),
county_only_diagnostic AS (
  SELECT
    COALESCE(property_state, '<null>') AS property_state,
    COUNT(DISTINCT loan_key) AS loans
  FROM bridge_rows
  WHERE filed_borough IS NOT NULL
  GROUP BY property_state
),
geocoder_county_fips_diagnostic AS (
  SELECT
    l.truth_plane,
    COUNT(DISTINCT b.loan_key) AS loans
  FROM bridge_rows b
  JOIN loan_classification l
    ON l.loan_key = b.loan_key
  WHERE b.geocoded_county_fips IN ('36005','36047','36061','36081','36085')
    AND b.property_state = (SELECT property_state FROM params)
    AND l.gate_status = 'admissible_for_plane_classification'
  GROUP BY l.truth_plane
),
originator_availability_diagnostic AS (
  SELECT
    truth_plane,
    COUNT(*) AS eligible_loans,
    COUNT_IF(distinct_originatorname = 1) AS raw_originator_available,
    COUNT_IF(distinct_originator_match_text = 1) AS originator_match_text_available,
    COUNT_IF(distinct_originatorname = 0) AS raw_originator_absent,
    COUNT_IF(distinct_originator_match_text = 0) AS originator_match_text_absent
  FROM loan_classification
  WHERE gate_status = 'admissible_for_plane_classification'
  GROUP BY truth_plane
),
master_candidates_non_round AS (
  SELECT
    l.loan_key,
    'non_round_amount_date_legal_borough' AS truth_plane,
    m.document_id,
    l.amount_cents,
    l.originationdate,
    m.recorded_borough AS legal_borough,
    m.doc_type,
    NULL::TEXT AS lender_match_text,
    NULL::TEXT AS lender_party_type
  FROM loan_classification l
  JOIN edgar_db.source.nyc_acris_real_property_master_ext m
    ON m.release_dt = (SELECT acris_release_dt FROM params)
   AND ROUND(m.document_amt * 100, 0)::NUMBER(38,0) = l.amount_cents
   AND m.document_date BETWEEN l.originationdate AND DATEADD(day, 45, l.originationdate)
  WHERE l.gate_status = 'admissible_for_plane_classification'
    AND l.truth_plane = 'non_round_amount_date_legal_borough'
    AND ARRAY_CONTAINS(m.recorded_borough::VARIANT, l.filed_boroughs)
),
master_candidates_round AS (
  SELECT
    l.loan_key,
    'round_exact_lender_party' AS truth_plane,
    m.document_id,
    l.amount_cents,
    l.originationdate,
    m.recorded_borough AS legal_borough,
    m.doc_type,
    TRIM(REGEXP_REPLACE(UPPER(party.name), '[^A-Z0-9 ]', ' ')) AS lender_match_text,
    party.party_type::TEXT AS lender_party_type
  FROM loan_classification l
  JOIN edgar_db.source.nyc_acris_real_property_master_ext m
    ON m.release_dt = (SELECT acris_release_dt FROM params)
   AND ROUND(m.document_amt * 100, 0)::NUMBER(38,0) = l.amount_cents
   AND m.document_date BETWEEN l.originationdate AND DATEADD(day, 45, l.originationdate)
  JOIN edgar_db.source.nyc_acris_real_property_parties_ext party
    ON party.release_dt = m.release_dt
   AND party.document_id = m.document_id
   AND party.party_type::TEXT = CASE
     WHEN UPPER(TRIM(m.doc_type)) = 'MMTG' THEN '1'
     WHEN UPPER(TRIM(m.doc_type)) IN ('CMTG','M&CON','MTGE','SMTG','SPRD') THEN '2'
     ELSE NULL
   END
   AND TRIM(REGEXP_REPLACE(UPPER(party.name), '[^A-Z0-9 ]', ' ')) = l.originator_match_text
  WHERE l.gate_status = 'admissible_for_plane_classification'
    AND l.truth_plane = 'round_exact_lender_party'
    AND l.originator_match_text IS NOT NULL
    AND ARRAY_CONTAINS(m.recorded_borough::VARIANT, l.filed_boroughs)
),
master_candidates AS (
  SELECT * FROM master_candidates_non_round
  UNION ALL
  SELECT * FROM master_candidates_round
),
candidate_array_export AS (
  SELECT
    truth_plane,
    COUNT(DISTINCT loan_key) AS candidate_loans,
    COUNT(*) AS loan_document_pairs,
    ARRAY_AGG(
      OBJECT_CONSTRUCT_KEEP_NULL(
        'loan_key', loan_key,
        'document_id', document_id,
        'truth_plane', truth_plane,
        'legal_borough', legal_borough,
        'doc_type', doc_type,
        'amount_cents', amount_cents,
        'originationdate', TO_VARCHAR(originationdate),
        'lender_match_text', lender_match_text,
        'lender_party_type', lender_party_type
      )
    ) WITHIN GROUP (ORDER BY loan_key, document_id, legal_borough) AS candidate_rows
  FROM master_candidates
  GROUP BY truth_plane
)
SELECT
  OBJECT_CONSTRUCT(
    'payload_kind', 'h7_acris_master_party_candidate_control.v0',
    'is_population_rows_contract', FALSE,
    'is_evidentiary_legal_residual', FALSE,
    'controls', OBJECT_CONSTRUCT(
      'raw_property_state_ny_universe', (SELECT ARRAY_AGG(OBJECT_CONSTRUCT(*)) FROM ny_universe_control),
      'county_only_diagnostic', (SELECT ARRAY_AGG(OBJECT_CONSTRUCT(*)) FROM county_only_diagnostic),
      'geocoder_county_fips_diagnostic', (SELECT ARRAY_AGG(OBJECT_CONSTRUCT(*)) FROM geocoder_county_fips_diagnostic),
      'originator_availability_diagnostic', (SELECT ARRAY_AGG(OBJECT_CONSTRUCT(*)) FROM originator_availability_diagnostic)
    ),
    'candidate_array_export', (SELECT ARRAY_AGG(OBJECT_CONSTRUCT(*)) FROM candidate_array_export),
    'limitation', 'This payload stops before ACRIS LEGALS and MapPLUTO; it is not canon_geo_h7_population_rows.v0 and cannot prove accepted multi-parcel truth.',
    'next_step', 'A separate bounded legal-residual query must join candidate_rows to ACRIS LEGALS and release-pinned MapPLUTO before any live H.7 population rows can be cited.'
  ) AS h7_population_control_payload;
