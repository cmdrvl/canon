-- Appendix H.7 release-pinned accepted multi-parcel truth export.
--
-- This query emits one row per uniquely accepted multi-BBL loan/document,
-- preserving bridge, MASTER, selected PARTY, and LEGALS provenance. Distinct
-- legal BBLs are nested in deterministic order, so a single 172-BBL subject
-- cannot be silently truncated by the MCP's 200-row response cap.
--
-- This is accepted legal truth, not a canon_geo_h7_population_rows.v0 input.
-- It intentionally has no MapPLUTO candidate release or candidate parcels;
-- candidate reach remains a later, independently measured plane. Source rows
-- are provenance for assertions and are never counted as independent votes.

WITH
params AS (
  SELECT
    'h7_staging_accepted_truth_row.v0'::TEXT AS row_contract,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    'NY'::TEXT AS property_state,
    'nyc_filed_collateral_slice'::TEXT AS collateral_scope,
    'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT
      AS amount_cents_quantization,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    45::NUMBER(9,0) AS max_recording_offset_days,
    200::NUMBER(38,0) AS export_row_cap
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
mortgage_doc_types AS (
  SELECT column1::TEXT AS doc_type
  FROM VALUES ('MTGE'), ('M&CON'), ('CMTG'), ('SMTG'), ('MMTG'), ('SPRD')
),
lender_party_roles AS (
  SELECT * FROM VALUES
    ('CMTG', '2'),
    ('M&CON', '2'),
    ('MMTG', '1'),
    ('MTGE', '2'),
    ('SMTG', '2'),
    ('SPRD', '2')
  AS r(doc_type, lender_party_type)
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
    UPPER(TRIM(lip.propertycounty)) AS propertycounty,
    lip.county_fips AS diagnostic_county_fips,
    m.filed_borough
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
  JOIN params p
    ON lip.build_id = p.bridge_build_id
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
    COUNT(DISTINCT IFF(originationdate IS NOT NULL, originationdate, NULL))
      AS distinct_originationdate,
    COUNT(DISTINCT IFF(amount_cents IS NOT NULL, amount_cents, NULL))
      AS distinct_amount_cents,
    COUNT(DISTINCT IFF(originatorname IS NOT NULL, originatorname, NULL))
      AS distinct_originatorname,
    COUNT(DISTINCT IFF(originator_match_text IS NOT NULL,
      originator_match_text, NULL)) AS distinct_originator_match_text,
    COUNT(DISTINCT propertycounty) AS distinct_filed_county,
    COUNT(DISTINCT filed_borough) AS distinct_filed_borough,
    MAX(originationdate) AS originationdate,
    MAX(amount_cents) AS amount_cents,
    MAX(originatorname) AS originatorname,
    MAX(originator_match_text) AS originator_match_text,
    ARRAY_AGG(DISTINCT propertycounty)
      WITHIN GROUP (ORDER BY propertycounty) AS filed_counties,
    ARRAY_AGG(DISTINCT filed_borough)
      WITHIN GROUP (ORDER BY filed_borough) AS filed_boroughs,
    ARRAY_AGG(DISTINCT diagnostic_county_fips)
      WITHIN GROUP (ORDER BY diagnostic_county_fips)
      AS diagnostic_county_fips
  FROM ny_filed_bridge_rows
  GROUP BY loan_key
),
loan_filed_county_edges AS (
  SELECT
    loan_key,
    ARRAY_AGG(
      OBJECT_CONSTRUCT(
        'filed_county', propertycounty,
        'filed_borough', filed_borough
      )
    ) WITHIN GROUP (ORDER BY propertycounty, filed_borough)
      AS filed_county_borough_edges
  FROM (
    SELECT DISTINCT loan_key, propertycounty, filed_borough
    FROM ny_filed_bridge_rows
  )
  GROUP BY loan_key
),
loan_bridge_source_records AS (
  SELECT
    loan_key,
    ARRAY_AGG(source_record_id) WITHIN GROUP (ORDER BY source_record_id)
      AS bridge_source_record_ids
  FROM (
    SELECT DISTINCT
      loan_key,
      'EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:'
        || (SELECT bridge_build_id FROM params)
        || ':'
        || loan_key
        || ':'
        || COALESCE(TO_VARCHAR(property_key), '<null>') AS source_record_id
    FROM ny_filed_bridge_rows
  )
  GROUP BY loan_key
),
eligible_loans AS (
  SELECT
    c.loan_key,
    c.property_keys,
    IFF(c.property_keys > 1, 'multi_property', 'single_property')
      AS association_plane,
    c.originationdate,
    c.amount_cents,
    c.originatorname,
    c.originator_match_text,
    c.filed_counties,
    c.filed_boroughs,
    e.filed_county_borough_edges,
    c.diagnostic_county_fips,
    r.bridge_source_record_ids,
    OBJECT_CONSTRUCT(
      'originatorname', c.distinct_originatorname,
      'originator_match_text', c.distinct_originator_match_text,
      'originationdate', c.distinct_originationdate,
      'originalloanamount', c.distinct_amount_cents,
      'filed_county', c.distinct_filed_county,
      'filed_borough', c.distinct_filed_borough
    ) AS distinct_counts,
    IFF(
      MOD(c.amount_cents,
        (SELECT round_amount_lattice_cents FROM params)) = 0,
      'round_exact_lender_party',
      'non_round_amount_date_legal_borough'
    ) AS truth_plane
  FROM loan_counts c
  JOIN loan_filed_county_edges e USING (loan_key)
  JOIN loan_bridge_source_records r USING (loan_key)
  WHERE c.distinct_originationdate = 1
    AND c.distinct_amount_cents = 1
    AND c.amount_cents <> 0
    AND c.distinct_originatorname <= 1
    AND c.distinct_originator_match_text <= 1
    AND c.distinct_filed_borough > 0
),
master_candidates_non_round AS (
  SELECT
    l.*,
    m.document_id::TEXT AS document_id,
    m.recorded_borough::NUMBER(38,0) AS recorded_borough,
    m.doc_type_norm::TEXT AS doc_type,
    m.crfn::TEXT AS crfn,
    CAST(m.document_date AS DATE) AS document_date,
    m.recorded_date AS recorded_date,
    DATEDIFF(day, l.originationdate, m.recorded_date)
      AS recording_offset_days,
    'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_MASTER:'
      || TO_VARCHAR(m.release_dt)
      || ':'
      || TO_VARCHAR(m.source_row_number::NUMBER(38,0))
      || ':'
      || m.document_id::TEXT AS acris_master_source_record_id,
    m.raw_csv_sha256::TEXT AS acris_master_raw_csv_sha256,
    m.filename::TEXT AS acris_master_filename,
    NULL::TEXT AS lender_match_text,
    NULL::TEXT AS lender_party_type,
    NULL::TEXT AS acris_party_source_record_id,
    NULL::TEXT AS acris_party_raw_csv_sha256,
    NULL::TEXT AS acris_party_filename
  FROM eligible_loans l
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_MASTER m
    ON m.release_dt = (SELECT acris_release_dt FROM params)
   AND m.document_row_rank = 1
   AND m.amount_cents = l.amount_cents
   AND m.recorded_date BETWEEN l.originationdate
     AND DATEADD(day, (SELECT max_recording_offset_days FROM params),
       l.originationdate)
  JOIN mortgage_doc_types dt
    ON m.doc_type_norm = dt.doc_type
  WHERE l.truth_plane = 'non_round_amount_date_legal_borough'
),
master_candidates_round AS (
  SELECT
    l.*,
    m.document_id::TEXT AS document_id,
    m.recorded_borough::NUMBER(38,0) AS recorded_borough,
    m.doc_type_norm::TEXT AS doc_type,
    m.crfn::TEXT AS crfn,
    CAST(m.document_date AS DATE) AS document_date,
    m.recorded_date AS recorded_date,
    DATEDIFF(day, l.originationdate, m.recorded_date)
      AS recording_offset_days,
    'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_MASTER:'
      || TO_VARCHAR(m.release_dt)
      || ':'
      || TO_VARCHAR(m.source_row_number::NUMBER(38,0))
      || ':'
      || m.document_id::TEXT AS acris_master_source_record_id,
    m.raw_csv_sha256::TEXT AS acris_master_raw_csv_sha256,
    m.filename::TEXT AS acris_master_filename,
    party.party_name_norm::TEXT AS lender_match_text,
    party.party_type::TEXT AS lender_party_type,
    'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:'
      || TO_VARCHAR(party.release_dt)
      || ':'
      || TO_VARCHAR(party.source_row_number::NUMBER(38,0))
      || ':'
      || party.document_id::TEXT AS acris_party_source_record_id,
    party.raw_csv_sha256::TEXT AS acris_party_raw_csv_sha256,
    party.filename::TEXT AS acris_party_filename
  FROM eligible_loans l
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_MASTER m
    ON m.release_dt = (SELECT acris_release_dt FROM params)
   AND m.document_row_rank = 1
   AND m.amount_cents = l.amount_cents
   AND m.recorded_date BETWEEN l.originationdate
     AND DATEADD(day, (SELECT max_recording_offset_days FROM params),
       l.originationdate)
  JOIN mortgage_doc_types dt
    ON m.doc_type_norm = dt.doc_type
  JOIN lender_party_roles role
    ON m.doc_type_norm = role.doc_type
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES party
    ON party.release_dt = m.release_dt
   AND party.document_id = m.document_id
   AND party.party_type::TEXT = role.lender_party_type
   AND party.party_name_norm = l.originator_match_text
  WHERE l.truth_plane = 'round_exact_lender_party'
    AND l.originator_match_text IS NOT NULL
  QUALIFY ROW_NUMBER() OVER (
    PARTITION BY l.loan_key, m.document_id
    ORDER BY
      party.source_row_number::NUMBER(38,0),
      party.raw_csv_sha256::TEXT,
      party.filename::TEXT
  ) = 1
),
candidate_documents AS (
  SELECT * FROM master_candidates_non_round
  UNION ALL
  SELECT * FROM master_candidates_round
),
candidate_filed_boroughs AS (
  SELECT
    c.*,
    filed.value::NUMBER(38,0) AS filed_borough
  FROM candidate_documents c,
    LATERAL FLATTEN(input => c.filed_boroughs) filed
  WHERE filed.value::NUMBER(38,0) IN (1, 2, 3, 4, 5)
),
legal_source_rows AS (
  SELECT DISTINCT
    c.loan_key,
    c.truth_plane,
    c.document_id,
    c.filed_borough,
    l.bbl::TEXT AS legal_bbl,
    'EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS:'
      || TO_VARCHAR(l.release_dt)
      || ':'
      || TO_VARCHAR(l.source_row_number::NUMBER(38,0))
      || ':'
      || l.document_id::TEXT AS acris_legal_source_record_id,
    l.raw_csv_sha256::TEXT AS acris_legal_raw_csv_sha256,
    l.filename::TEXT AS acris_legal_filename
  FROM candidate_filed_boroughs c
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS l
    ON l.release_dt = (SELECT acris_release_dt FROM params)
   AND l.document_id = c.document_id
   AND l.legal_borough = c.filed_borough
  WHERE l.block IS NOT NULL
    AND l.lot IS NOT NULL
    AND l.bbl IS NOT NULL
),
legal_bbls AS (
  SELECT
    loan_key,
    truth_plane,
    document_id,
    COUNT(DISTINCT legal_bbl) AS truth_bbl_count,
    ARRAY_AGG(DISTINCT legal_bbl) WITHIN GROUP (ORDER BY legal_bbl)
      AS truth_bbls
  FROM legal_source_rows
  GROUP BY loan_key, truth_plane, document_id
),
legal_source_records AS (
  SELECT
    loan_key,
    truth_plane,
    document_id,
    ARRAY_AGG(
      OBJECT_CONSTRUCT(
        'source_record_id', acris_legal_source_record_id,
        'raw_csv_sha256', acris_legal_raw_csv_sha256,
        'filename', acris_legal_filename,
        'filed_borough', filed_borough,
        'legal_bbl', legal_bbl
      )
    ) WITHIN GROUP (ORDER BY acris_legal_source_record_id)
      AS acris_legal_source_records
  FROM legal_source_rows
  GROUP BY loan_key, truth_plane, document_id
),
legal_documents AS (
  SELECT b.*, s.acris_legal_source_records
  FROM legal_bbls b
  JOIN legal_source_records s
    USING (loan_key, truth_plane, document_id)
),
loan_legal_disposition AS (
  SELECT
    loan_key,
    truth_plane,
    COUNT(*) AS legal_document_count,
    MAX(truth_bbl_count) AS accepted_bbl_count
  FROM legal_documents
  GROUP BY loan_key, truth_plane
),
eligible_counts AS (
  SELECT truth_plane, COUNT(*) AS eligible_loans
  FROM eligible_loans
  GROUP BY truth_plane
),
candidate_counts AS (
  SELECT
    truth_plane,
    COUNT(DISTINCT loan_key) AS candidate_loans
  FROM candidate_documents
  GROUP BY truth_plane
),
legal_counts AS (
  SELECT
    truth_plane,
    COUNT(DISTINCT loan_key) AS legal_confirmed_candidate_loans
  FROM legal_documents
  GROUP BY truth_plane
),
acceptance_counts AS (
  SELECT
    truth_plane,
    COUNT_IF(legal_document_count = 1) AS accepted_loans,
    COUNT_IF(legal_document_count > 1) AS ambiguous_loans,
    COUNT_IF(legal_document_count = 1 AND accepted_bbl_count > 1)
      AS selected_multi_parcel_loans
  FROM loan_legal_disposition
  GROUP BY truth_plane
),
plane_denominators AS (
  SELECT
    e.truth_plane,
    e.eligible_loans,
    COALESCE(c.candidate_loans, 0) AS candidate_loans,
    COALESCE(l.legal_confirmed_candidate_loans, 0)
      AS legal_confirmed_candidate_loans,
    COALESCE(a.accepted_loans, 0) AS accepted_loans,
    COALESCE(a.ambiguous_loans, 0) AS ambiguous_loans,
    COALESCE(c.candidate_loans, 0)
      - COALESCE(l.legal_confirmed_candidate_loans, 0)
      AS candidate_without_legal_loans,
    e.eligible_loans - COALESCE(c.candidate_loans, 0)
      AS no_candidate_loans,
    COALESCE(a.selected_multi_parcel_loans, 0)
      AS selected_multi_parcel_loans
  FROM eligible_counts e
  LEFT JOIN candidate_counts c USING (truth_plane)
  LEFT JOIN legal_counts l USING (truth_plane)
  LEFT JOIN acceptance_counts a USING (truth_plane)
),
accepted_multi_bbl AS (
  SELECT
    c.*,
    l.truth_bbl_count,
    l.truth_bbls,
    l.acris_legal_source_records
  FROM candidate_documents c
  JOIN legal_documents l
    USING (loan_key, truth_plane, document_id)
  JOIN loan_legal_disposition d
    USING (loan_key, truth_plane)
  WHERE d.legal_document_count = 1
    AND l.truth_bbl_count > 1
),
export_stats AS (
  SELECT COUNT(*) AS export_rows
  FROM accepted_multi_bbl
)
SELECT
  (SELECT row_contract FROM params) AS row_contract,
  (SELECT bridge_build_id FROM params) AS bridge_build_id,
  (SELECT acris_release_dt FROM params) AS acris_release_dt,
  (SELECT property_state FROM params) AS property_state,
  (SELECT collateral_scope FROM params) AS collateral_scope,
  (SELECT amount_cents_quantization FROM params)
    AS amount_cents_quantization,
  (SELECT round_amount_lattice_cents FROM params)
    AS round_amount_lattice_cents,
  (SELECT max_recording_offset_days FROM params)
    AS max_recording_offset_days,
  p.eligible_loans,
  p.candidate_loans,
  p.legal_confirmed_candidate_loans,
  p.accepted_loans,
  p.ambiguous_loans,
  p.candidate_without_legal_loans,
  p.no_candidate_loans,
  p.selected_multi_parcel_loans,
  (SELECT export_rows FROM export_stats) AS whole_export_rows,
  (SELECT export_row_cap FROM params) AS export_row_cap,
  (SELECT export_rows FROM export_stats)
    <= (SELECT export_row_cap FROM params) AS export_row_cap_reconciles,
  a.truth_plane,
  a.loan_key,
  a.property_keys,
  a.association_plane,
  a.amount_cents,
  a.originationdate,
  a.originatorname,
  a.originator_match_text,
  a.filed_counties,
  a.filed_boroughs,
  a.filed_county_borough_edges,
  a.distinct_counts,
  a.diagnostic_county_fips,
  a.bridge_source_record_ids,
  a.document_id,
  a.recorded_borough AS diagnostic_recorded_borough,
  a.doc_type,
  a.crfn,
  a.document_date,
  a.recorded_date,
  a.recording_offset_days,
  a.lender_match_text,
  a.lender_party_type,
  a.acris_master_source_record_id,
  a.acris_master_raw_csv_sha256,
  a.acris_master_filename,
  a.acris_party_source_record_id,
  a.acris_party_raw_csv_sha256,
  a.acris_party_filename,
  a.truth_bbl_count,
  a.truth_bbls,
  a.acris_legal_source_records
FROM accepted_multi_bbl a
JOIN plane_denominators p USING (truth_plane)
ORDER BY a.truth_plane, a.loan_key, a.document_id;
