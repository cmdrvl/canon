-- Appendix H.7 / E4 consensus-document-ambiguous truth extension.
--
-- Purpose: investigate the honest five-subject E4 deficit without weakening
-- the controlling H.7 truth gates. This single SELECT recomputes the same
-- release-pinned H.7 candidate/legal relation, starts only from loans that are
-- ambiguous at document identity, and admits a new subject only when every
-- candidate document's legal BBL set is the same byte-stable set.
--
-- This is an address-independent ACRIS truth-stratum measurement, not a Canon
-- population-row artifact, not an independent evidence vote, and not pooled
-- precision. It does not use query-side address fields, geocoded county,
-- parcel-release candidate geometry, or composition output.
--
-- Output contract: h7_e4_consensus_truth_extension_row.v0
-- Positive result requirement: a live run must return nonzero rows, preserve
-- the query id, and report admitted subject counts separately by truth plane.
-- Fewer than five genuinely new subjects is a valid negative finding.
--
-- Retention guard: the historical bridge build id below is intentionally not
-- replaced by a newer PROPERTY_MART build. If that build is no longer retained
-- in the current warehouse snapshot, this query must emit explicit guard
-- failures rather than treating a current build as equivalent or silently
-- returning zero rows.

WITH
params AS (
  SELECT
    'h7_e4_consensus_truth_extension_row.v0'::TEXT AS row_contract,
    'h7_e4_consensus_document_ambiguous_truth_extension'::TEXT
      AS query_purpose,
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    'NY'::TEXT AS property_state,
    'nyc_filed_collateral_slice'::TEXT AS collateral_scope,
    'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT
      AS amount_cents_quantization,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    45::NUMBER(9,0) AS max_recording_offset_days,
    2974::NUMBER(38,0) AS expected_h7_eligible_loans,
    71::NUMBER(38,0) AS expected_h7_accepted_multi_bbl_loans,
    100::NUMBER(38,0) AS consensus_subject_row_cap
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
known_gate_v2_h4_extension_keys AS (
  SELECT
    NULL::TEXT AS loan_key,
    NULL::TEXT AS extension_key
  WHERE FALSE
),
known_extension_key_stats AS (
  SELECT COUNT(*) AS known_extension_key_rows
  FROM known_gate_v2_h4_extension_keys
),
expected_truth_planes AS (
  SELECT * FROM VALUES
    ('non_round_amount_date_legal_borough', 653, 35),
    ('round_exact_lender_party', 2321, 36)
  AS p(
    truth_plane,
    expected_eligible_loans,
    expected_accepted_h7_multi_bbl_loans
  )
),
bridge_pin_stats AS (
  SELECT
    COUNT(*) AS bridge_rows,
    COUNT(DISTINCT loan_key) AS bridge_distinct_loans
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
  WHERE lip.build_id = (SELECT bridge_build_id FROM params)
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
      WITHIN GROUP (ORDER BY filed_borough) AS filed_boroughs
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
    m.recorded_borough::NUMBER(38,0) AS diagnostic_recorded_borough,
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
    m.recorded_borough::NUMBER(38,0) AS diagnostic_recorded_borough,
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
candidate_document_rows AS (
  SELECT * FROM master_candidates_non_round
  UNION ALL
  SELECT * FROM master_candidates_round
),
candidate_documents AS (
  SELECT DISTINCT *
  FROM candidate_document_rows
),
candidate_document_counts AS (
  SELECT
    truth_plane,
    loan_key,
    COUNT(*) AS candidate_document_count
  FROM candidate_documents
  GROUP BY truth_plane, loan_key
),
candidate_filed_boroughs AS (
  SELECT
    c.*,
    filed.value::NUMBER(38,0) AS filed_borough
  FROM candidate_documents c,
    LATERAL FLATTEN(input => c.filed_boroughs) filed
  WHERE filed.value::NUMBER(38,0) IN (1, 2, 3, 4, 5)
),
legal_edges AS (
  SELECT DISTINCT
    c.loan_key,
    c.truth_plane,
    c.document_id,
    c.filed_borough,
    l.legal_borough::NUMBER(38,0) AS legal_borough,
    l.bbl::TEXT AS raw_legal_bbl,
    LPAD(l.legal_borough::TEXT, 1, '0')
      || LPAD(l.block::TEXT, 5, '0')
      || LPAD(l.lot::TEXT, 4, '0') AS normalized_legal_bbl,
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
document_legal_sets AS (
  SELECT
    loan_key,
    truth_plane,
    document_id,
    COUNT(DISTINCT normalized_legal_bbl) AS document_bbl_count,
    LISTAGG(DISTINCT normalized_legal_bbl, '|')
      WITHIN GROUP (ORDER BY normalized_legal_bbl) AS document_bbl_set_key,
    ARRAY_AGG(DISTINCT normalized_legal_bbl)
      WITHIN GROUP (ORDER BY normalized_legal_bbl) AS document_bbls
  FROM legal_edges
  GROUP BY loan_key, truth_plane, document_id
),
legal_source_records AS (
  SELECT
    loan_key,
    truth_plane,
    document_id,
    ARRAY_AGG(DISTINCT acris_legal_source_record_id)
      WITHIN GROUP (ORDER BY acris_legal_source_record_id)
      AS acris_legal_source_record_ids,
    ARRAY_AGG(DISTINCT acris_legal_raw_csv_sha256)
      WITHIN GROUP (ORDER BY acris_legal_raw_csv_sha256)
      AS acris_legal_raw_csv_sha256s,
    ARRAY_AGG(DISTINCT acris_legal_filename)
      WITHIN GROUP (ORDER BY acris_legal_filename)
      AS acris_legal_filenames
  FROM legal_edges
  GROUP BY loan_key, truth_plane, document_id
),
legal_documents AS (
  SELECT s.*, r.acris_legal_source_record_ids,
    r.acris_legal_raw_csv_sha256s, r.acris_legal_filenames
  FROM document_legal_sets s
  JOIN legal_source_records r
    USING (loan_key, truth_plane, document_id)
),
loan_document_sets AS (
  SELECT
    c.loan_key,
    c.truth_plane,
    ANY_VALUE(c.association_plane) AS association_plane,
    ANY_VALUE(c.property_keys) AS property_keys,
    ANY_VALUE(c.originationdate) AS originationdate,
    ANY_VALUE(c.amount_cents) AS amount_cents,
    ANY_VALUE(c.originatorname) AS originatorname,
    ANY_VALUE(c.originator_match_text) AS originator_match_text,
    ANY_VALUE(c.filed_counties) AS filed_counties,
    ANY_VALUE(c.filed_boroughs) AS filed_boroughs,
    ANY_VALUE(c.filed_county_borough_edges) AS filed_county_borough_edges,
    ANY_VALUE(c.distinct_counts) AS distinct_counts,
    ANY_VALUE(c.bridge_source_record_ids) AS bridge_source_record_ids,
    COUNT(*) AS candidate_document_count,
    COUNT(ld.document_id) AS legal_document_count,
    COUNT(*) - COUNT(ld.document_id) AS candidate_without_legal_document_count,
    MIN(ld.document_bbl_count) AS min_document_bbl_count,
    MAX(ld.document_bbl_count) AS max_document_bbl_count,
    COUNT(DISTINCT ld.document_bbl_set_key) AS distinct_bbl_set_count,
    MAX(ld.document_bbl_set_key) AS consensus_bbl_set_key,
    MAX(ld.document_bbl_count) AS consensus_bbl_count
  FROM candidate_documents c
  LEFT JOIN legal_documents ld
    ON ld.loan_key = c.loan_key
   AND ld.truth_plane = c.truth_plane
   AND ld.document_id = c.document_id
  GROUP BY c.loan_key, c.truth_plane
),
loan_document_sources AS (
  SELECT
    loan_key,
    truth_plane,
    ARRAY_AGG(DISTINCT document_id) WITHIN GROUP (ORDER BY document_id)
      AS candidate_document_ids,
    ARRAY_AGG(DISTINCT acris_master_source_record_id)
      WITHIN GROUP (ORDER BY acris_master_source_record_id)
      AS acris_master_source_record_ids,
    ARRAY_AGG(DISTINCT acris_master_raw_csv_sha256)
      WITHIN GROUP (ORDER BY acris_master_raw_csv_sha256)
      AS acris_master_raw_csv_sha256s,
    ARRAY_AGG(DISTINCT acris_master_filename)
      WITHIN GROUP (ORDER BY acris_master_filename)
      AS acris_master_filenames,
    ARRAY_AGG(DISTINCT acris_party_source_record_id)
      WITHIN GROUP (ORDER BY acris_party_source_record_id)
      AS acris_party_source_record_ids,
    ARRAY_AGG(DISTINCT acris_party_raw_csv_sha256)
      WITHIN GROUP (ORDER BY acris_party_raw_csv_sha256)
      AS acris_party_raw_csv_sha256s,
    ARRAY_AGG(DISTINCT acris_party_filename)
      WITHIN GROUP (ORDER BY acris_party_filename)
      AS acris_party_filenames
  FROM candidate_documents
  GROUP BY loan_key, truth_plane
),
loan_legal_sources AS (
  SELECT
    loan_key,
    truth_plane,
    ARRAY_AGG(DISTINCT acris_legal_source_record_id)
      WITHIN GROUP (ORDER BY acris_legal_source_record_id)
      AS acris_legal_source_record_ids,
    ARRAY_AGG(DISTINCT acris_legal_raw_csv_sha256)
      WITHIN GROUP (ORDER BY acris_legal_raw_csv_sha256)
      AS acris_legal_raw_csv_sha256s,
    ARRAY_AGG(DISTINCT acris_legal_filename)
      WITHIN GROUP (ORDER BY acris_legal_filename)
      AS acris_legal_filenames
  FROM legal_edges
  GROUP BY loan_key, truth_plane
),
consensus_truth_bbls AS (
  SELECT
    l.loan_key,
    l.truth_plane,
    ARRAY_AGG(DISTINCT e.normalized_legal_bbl)
      WITHIN GROUP (ORDER BY e.normalized_legal_bbl) AS consensus_bbls
  FROM loan_document_sets l
  JOIN legal_edges e
    ON e.loan_key = l.loan_key
   AND e.truth_plane = l.truth_plane
  WHERE l.distinct_bbl_set_count = 1
  GROUP BY l.loan_key, l.truth_plane
),
loan_legal_disposition AS (
  SELECT
    loan_key,
    truth_plane,
    COUNT(*) AS legal_document_count,
    MAX(document_bbl_count) AS accepted_bbl_count
  FROM legal_documents
  GROUP BY loan_key, truth_plane
),
accepted_h7_multi_bbl_keys AS (
  SELECT
    d.loan_key,
    d.truth_plane
  FROM loan_legal_disposition d
  WHERE d.legal_document_count = 1
    AND d.accepted_bbl_count > 1
),
candidate_summary AS (
  SELECT
    truth_plane,
    COUNT(DISTINCT loan_key) AS candidate_loans,
    COUNT(DISTINCT loan_key || '|' || document_id) AS candidate_loan_documents
  FROM candidate_documents
  GROUP BY truth_plane
),
legal_summary AS (
  SELECT
    truth_plane,
    COUNT(DISTINCT loan_key) AS legal_confirmed_candidate_loans,
    COUNT(DISTINCT loan_key || '|' || document_id)
      AS legal_confirmed_loan_documents
  FROM legal_documents
  GROUP BY truth_plane
),
accepted_summary AS (
  SELECT
    truth_plane,
    COUNT_IF(legal_document_count = 1) AS accepted_h7_loans,
    COUNT_IF(legal_document_count > 1)
      AS ambiguous_document_identity_loans,
    COUNT_IF(legal_document_count = 1 AND accepted_bbl_count > 1)
      AS accepted_h7_multi_bbl_loans
  FROM loan_legal_disposition
  GROUP BY truth_plane
),
eligible_summary AS (
  SELECT
    truth_plane,
    COUNT(*) AS eligible_loans
  FROM eligible_loans
  GROUP BY truth_plane
),
ambiguous_consensus_classification AS (
  SELECT
    l.*,
    IFF(a.loan_key IS NOT NULL, TRUE, FALSE)
      AS overlaps_accepted_h7_multi_bbl,
    IFF(k.loan_key IS NOT NULL, TRUE, FALSE)
      AS overlaps_known_gate_v2_h4_extension_key,
    CASE
      WHEN a.loan_key IS NOT NULL THEN 'accepted_71_contamination'
      WHEN k.loan_key IS NOT NULL THEN 'known_gate_v2_h4_extension_duplicate'
      WHEN l.candidate_without_legal_document_count <> 0 THEN 'missing_legal_rows'
      WHEN l.legal_document_count < 2 THEN 'not_document_ambiguous'
      WHEN l.min_document_bbl_count < 2 THEN 'document_bbl_set_not_multi_bbl'
      WHEN l.distinct_bbl_set_count <> 1 THEN 'document_bbl_set_disagreement'
      ELSE 'admitted_consensus_document_ambiguous'
    END AS consensus_disposition
  FROM loan_document_sets l
  LEFT JOIN accepted_h7_multi_bbl_keys a
    ON a.loan_key = l.loan_key
  LEFT JOIN known_gate_v2_h4_extension_keys k
    ON k.loan_key = l.loan_key
  WHERE l.legal_document_count > 1
),
consensus_subjects AS (
  SELECT c.*, b.consensus_bbls,
    s.candidate_document_ids,
    s.acris_master_source_record_ids,
    s.acris_master_raw_csv_sha256s,
    s.acris_master_filenames,
    s.acris_party_source_record_ids,
    s.acris_party_raw_csv_sha256s,
    s.acris_party_filenames,
    lg.acris_legal_source_record_ids,
    lg.acris_legal_raw_csv_sha256s,
    lg.acris_legal_filenames
  FROM ambiguous_consensus_classification c
  JOIN consensus_truth_bbls b
    ON b.loan_key = c.loan_key
   AND b.truth_plane = c.truth_plane
  JOIN loan_document_sources s
    ON s.loan_key = c.loan_key
   AND s.truth_plane = c.truth_plane
  JOIN loan_legal_sources lg
    ON lg.loan_key = c.loan_key
   AND lg.truth_plane = c.truth_plane
  WHERE c.consensus_disposition = 'admitted_consensus_document_ambiguous'
    AND c.candidate_without_legal_document_count = 0
    AND c.legal_document_count >= 2
    AND c.min_document_bbl_count >= 2
    AND c.distinct_bbl_set_count = 1
    AND NOT c.overlaps_accepted_h7_multi_bbl
    AND NOT c.overlaps_known_gate_v2_h4_extension_key
),
consensus_summary AS (
  SELECT
    truth_plane,
    COUNT_IF(consensus_disposition = 'admitted_consensus_document_ambiguous')
      AS admitted_consensus_subjects,
    COUNT_IF(consensus_disposition = 'accepted_71_contamination')
      AS rejected_accepted_71_contamination_loans,
    COUNT_IF(consensus_disposition = 'known_gate_v2_h4_extension_duplicate')
      AS rejected_known_extension_duplicate_loans,
    COUNT_IF(consensus_disposition = 'missing_legal_rows')
      AS rejected_missing_legal_rows_loans,
    COUNT_IF(consensus_disposition = 'not_document_ambiguous')
      AS rejected_not_document_ambiguous_loans,
    COUNT_IF(consensus_disposition = 'document_bbl_set_not_multi_bbl')
      AS rejected_document_bbl_set_not_multi_bbl_loans,
    COUNT_IF(consensus_disposition = 'document_bbl_set_disagreement')
      AS rejected_document_bbl_set_disagreement_loans
  FROM ambiguous_consensus_classification
  GROUP BY truth_plane
),
plane_denominators AS (
  SELECT
    ep.truth_plane,
    COALESCE(e.eligible_loans, 0) AS eligible_loans,
    COALESCE(c.candidate_loans, 0) AS candidate_loans,
    COALESCE(c.candidate_loan_documents, 0) AS candidate_loan_documents,
    COALESCE(l.legal_confirmed_candidate_loans, 0)
      AS legal_confirmed_candidate_loans,
    COALESCE(l.legal_confirmed_loan_documents, 0)
      AS legal_confirmed_loan_documents,
    COALESCE(a.accepted_h7_loans, 0) AS accepted_h7_loans,
    COALESCE(a.ambiguous_document_identity_loans, 0)
      AS ambiguous_document_identity_loans,
    COALESCE(c.candidate_loans, 0)
      - COALESCE(l.legal_confirmed_candidate_loans, 0)
      AS candidate_without_legal_loans,
    COALESCE(e.eligible_loans, 0) - COALESCE(c.candidate_loans, 0)
      AS no_candidate_loans,
    COALESCE(a.accepted_h7_multi_bbl_loans, 0)
      AS accepted_h7_multi_bbl_loans,
    COALESCE(x.admitted_consensus_subjects, 0)
      AS admitted_consensus_subjects,
    COALESCE(x.rejected_accepted_71_contamination_loans, 0)
      AS rejected_accepted_71_contamination_loans,
    COALESCE(x.rejected_known_extension_duplicate_loans, 0)
      AS rejected_known_extension_duplicate_loans,
    COALESCE(x.rejected_missing_legal_rows_loans, 0)
      AS rejected_missing_legal_rows_loans,
    COALESCE(x.rejected_not_document_ambiguous_loans, 0)
      AS rejected_not_document_ambiguous_loans,
    COALESCE(x.rejected_document_bbl_set_not_multi_bbl_loans, 0)
      AS rejected_document_bbl_set_not_multi_bbl_loans,
    COALESCE(x.rejected_document_bbl_set_disagreement_loans, 0)
      AS rejected_document_bbl_set_disagreement_loans,
    COALESCE(e.eligible_loans, 0) = COALESCE(c.candidate_loans, 0)
      + (COALESCE(e.eligible_loans, 0) - COALESCE(c.candidate_loans, 0))
      AS eligible_denominator_reconciles,
    COALESCE(c.candidate_loans, 0)
      = COALESCE(l.legal_confirmed_candidate_loans, 0)
        + (COALESCE(c.candidate_loans, 0)
          - COALESCE(l.legal_confirmed_candidate_loans, 0))
      AS candidate_denominator_reconciles,
    COALESCE(l.legal_confirmed_candidate_loans, 0)
      = COALESCE(a.accepted_h7_loans, 0)
        + COALESCE(a.ambiguous_document_identity_loans, 0)
      AS legal_denominator_reconciles,
    COALESCE(a.ambiguous_document_identity_loans, 0)
      = COALESCE(x.admitted_consensus_subjects, 0)
        + COALESCE(x.rejected_accepted_71_contamination_loans, 0)
        + COALESCE(x.rejected_known_extension_duplicate_loans, 0)
        + COALESCE(x.rejected_missing_legal_rows_loans, 0)
        + COALESCE(x.rejected_not_document_ambiguous_loans, 0)
        + COALESCE(x.rejected_document_bbl_set_not_multi_bbl_loans, 0)
        + COALESCE(x.rejected_document_bbl_set_disagreement_loans, 0)
      AS ambiguous_consensus_denominator_reconciles
  FROM expected_truth_planes ep
  LEFT JOIN eligible_summary e USING (truth_plane)
  LEFT JOIN candidate_summary c USING (truth_plane)
  LEFT JOIN legal_summary l USING (truth_plane)
  LEFT JOIN accepted_summary a USING (truth_plane)
  LEFT JOIN consensus_summary x USING (truth_plane)
),
output_stats AS (
  SELECT
    COUNT(*) AS consensus_subject_rows,
    COUNT(DISTINCT loan_key) AS distinct_consensus_loans,
    COUNT(DISTINCT loan_key || '|' || truth_plane) AS distinct_consensus_subjects,
    COUNT_IF(overlaps_accepted_h7_multi_bbl) AS accepted_71_overlap_rows,
    COUNT_IF(overlaps_known_gate_v2_h4_extension_key) AS known_extension_overlap_rows,
    COUNT_IF(candidate_without_legal_document_count <> 0) AS missing_legal_rows,
    COUNT_IF(legal_document_count < 2) AS under_minimum_document_rows,
    COUNT_IF(min_document_bbl_count < 2) AS under_minimum_bbl_rows,
    COUNT_IF(distinct_bbl_set_count <> 1) AS set_disagreement_rows,
    COUNT_IF(consensus_bbl_count <> ARRAY_SIZE(consensus_bbls))
      AS consensus_bbl_count_mismatch_rows,
    COUNT_IF(acris_master_source_record_ids IS NULL
      OR ARRAY_SIZE(acris_master_source_record_ids) = 0)
      AS missing_master_source_rows,
    COUNT_IF(truth_plane = 'round_exact_lender_party'
      AND (acris_party_source_record_ids IS NULL
        OR ARRAY_SIZE(acris_party_source_record_ids) = 0))
      AS missing_round_party_source_rows,
    COUNT_IF(truth_plane = 'non_round_amount_date_legal_borough'
      AND acris_party_source_record_ids IS NOT NULL
      AND ARRAY_SIZE(acris_party_source_record_ids) <> 0)
      AS non_round_party_source_leakage_rows,
    COUNT_IF(acris_legal_source_record_ids IS NULL
      OR ARRAY_SIZE(acris_legal_source_record_ids) = 0)
      AS missing_legal_source_rows
  FROM consensus_subjects
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT 'historical_bridge_build_not_retained_in_current_snapshot'
      AS failure_reason,
      (SELECT bridge_rows FROM bridge_pin_stats) = 0 AS failed
    UNION ALL
    SELECT 'truth_plane_summary_missing',
      (SELECT COUNT(*) FROM plane_denominators)
        <> (SELECT COUNT(*) FROM expected_truth_planes)
    UNION ALL
    SELECT 'eligible_plane_population_count_mismatch',
      COALESCE((SELECT SUM(eligible_loans) FROM plane_denominators), 0)
        <> (SELECT expected_h7_eligible_loans FROM params)
    UNION ALL
    SELECT 'truth_plane_eligible_count_mismatch',
      EXISTS (
        SELECT 1
        FROM plane_denominators p
        JOIN expected_truth_planes e USING (truth_plane)
        WHERE p.eligible_loans <> e.expected_eligible_loans
      )
    UNION ALL
    SELECT 'truth_plane_multi_bbl_count_mismatch',
      EXISTS (
        SELECT 1
        FROM plane_denominators p
        JOIN expected_truth_planes e USING (truth_plane)
        WHERE p.accepted_h7_multi_bbl_loans
          <> e.expected_accepted_h7_multi_bbl_loans
      )
    UNION ALL
    SELECT 'eligible_denominator_accounting_failure',
      EXISTS (
        SELECT 1 FROM plane_denominators
        WHERE NOT eligible_denominator_reconciles
      ) AS failed
    UNION ALL
    SELECT 'candidate_denominator_accounting_failure',
      EXISTS (
        SELECT 1 FROM plane_denominators
        WHERE NOT candidate_denominator_reconciles
      )
    UNION ALL
    SELECT 'legal_denominator_accounting_failure',
      EXISTS (
        SELECT 1 FROM plane_denominators
        WHERE NOT legal_denominator_reconciles
      )
    UNION ALL
    SELECT 'ambiguous_consensus_denominator_accounting_failure',
      EXISTS (
        SELECT 1 FROM plane_denominators
        WHERE NOT ambiguous_consensus_denominator_reconciles
      )
    UNION ALL
    SELECT 'accepted_71_population_count_mismatch',
      COALESCE((
        SELECT SUM(accepted_h7_multi_bbl_loans) FROM plane_denominators
      ), 0)
        <> (SELECT expected_h7_accepted_multi_bbl_loans FROM params)
    UNION ALL
    SELECT 'consensus_subject_row_cap_exceeded',
      (SELECT consensus_subject_rows FROM output_stats)
        > (SELECT consensus_subject_row_cap FROM params)
    UNION ALL
    SELECT 'consensus_duplicate_subject',
      (SELECT consensus_subject_rows FROM output_stats)
        <> (SELECT distinct_consensus_subjects FROM output_stats)
    UNION ALL
    SELECT 'consensus_duplicate_loan_cross_plane',
      (SELECT distinct_consensus_loans FROM output_stats)
        <> (SELECT distinct_consensus_subjects FROM output_stats)
    UNION ALL
    SELECT 'accepted_71_contamination',
      (SELECT accepted_71_overlap_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'known_gate_v2_h4_extension_duplicate',
      (SELECT known_extension_overlap_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'admitted_consensus_missing_legal_rows',
      (SELECT missing_legal_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'admitted_consensus_under_document_floor',
      (SELECT under_minimum_document_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'admitted_consensus_under_bbl_floor',
      (SELECT under_minimum_bbl_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'admitted_consensus_bbl_set_disagreement',
      (SELECT set_disagreement_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'consensus_bbl_count_mismatch',
      (SELECT consensus_bbl_count_mismatch_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'candidate_master_source_missing',
      (SELECT missing_master_source_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'round_party_source_missing',
      (SELECT missing_round_party_source_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'non_round_party_source_leakage',
      (SELECT non_round_party_source_leakage_rows FROM output_stats) <> 0
    UNION ALL
    SELECT 'legal_source_missing',
      (SELECT missing_legal_source_rows FROM output_stats) <> 0
  )
  WHERE failed
),
guard_summary AS (
  SELECT
    IFF(COUNT(*) = 0, 'ok', 'refused') AS guard_status,
    MIN(failure_reason) AS refusal_reason
  FROM guard_failures
),
summary_output AS (
  SELECT
    (SELECT row_contract FROM params) AS row_contract,
    'plane_summary'::TEXT AS row_kind,
    (SELECT query_purpose FROM params) AS query_purpose,
    g.guard_status,
    g.refusal_reason,
    p.truth_plane,
    NULL::TEXT AS loan_key,
    NULL::TEXT AS association_plane,
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
    IFF((SELECT known_extension_key_rows FROM known_extension_key_stats) = 0,
      'not_available_in_described_warehouse_tables',
      'available_and_applied') AS known_gate_v2_h4_extension_key_dedupe_status,
    p.eligible_loans,
    p.candidate_loans,
    p.candidate_loan_documents,
    p.legal_confirmed_candidate_loans,
    p.legal_confirmed_loan_documents,
    p.accepted_h7_loans,
    p.ambiguous_document_identity_loans,
    p.candidate_without_legal_loans,
    p.no_candidate_loans,
    p.accepted_h7_multi_bbl_loans,
    p.admitted_consensus_subjects,
    p.rejected_accepted_71_contamination_loans,
    p.rejected_known_extension_duplicate_loans,
    p.rejected_missing_legal_rows_loans,
    p.rejected_not_document_ambiguous_loans,
    p.rejected_document_bbl_set_not_multi_bbl_loans,
    p.rejected_document_bbl_set_disagreement_loans,
    p.eligible_denominator_reconciles,
    p.candidate_denominator_reconciles,
    p.legal_denominator_reconciles,
    p.ambiguous_consensus_denominator_reconciles,
    NULL::NUMBER(38,0) AS property_keys,
    NULL::NUMBER(38,0) AS amount_cents,
    NULL::DATE AS originationdate,
    NULL::TEXT AS originatorname,
    NULL::TEXT AS originator_match_text,
    NULL::NUMBER(38,0) AS candidate_document_count,
    NULL::NUMBER(38,0) AS legal_document_count,
    NULL::NUMBER(38,0) AS candidate_without_legal_document_count,
    NULL::NUMBER(38,0) AS min_document_bbl_count,
    NULL::NUMBER(38,0) AS max_document_bbl_count,
    NULL::NUMBER(38,0) AS distinct_bbl_set_count,
    NULL::NUMBER(38,0) AS consensus_bbl_count,
    NULL::TEXT AS consensus_bbl_set_key,
    NULL::VARIANT AS consensus_bbls,
    NULL::VARIANT AS filed_counties,
    NULL::VARIANT AS filed_boroughs,
    NULL::VARIANT AS filed_county_borough_edges,
    NULL::VARIANT AS distinct_counts,
    NULL::VARIANT AS bridge_source_record_ids,
    NULL::VARIANT AS candidate_document_ids,
    NULL::VARIANT AS acris_master_source_record_ids,
    NULL::VARIANT AS acris_master_raw_csv_sha256s,
    NULL::VARIANT AS acris_master_filenames,
    NULL::VARIANT AS acris_party_source_record_ids,
    NULL::VARIANT AS acris_party_raw_csv_sha256s,
    NULL::VARIANT AS acris_party_filenames,
    NULL::VARIANT AS acris_legal_source_record_ids,
    NULL::VARIANT AS acris_legal_raw_csv_sha256s,
    NULL::VARIANT AS acris_legal_filenames
  FROM plane_denominators p
  CROSS JOIN guard_summary g
),
consensus_output AS (
  SELECT
    (SELECT row_contract FROM params) AS row_contract,
    'consensus_subject'::TEXT AS row_kind,
    (SELECT query_purpose FROM params) AS query_purpose,
    g.guard_status,
    g.refusal_reason,
    c.truth_plane,
    c.loan_key,
    c.association_plane,
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
    IFF((SELECT known_extension_key_rows FROM known_extension_key_stats) = 0,
      'not_available_in_described_warehouse_tables',
      'available_and_applied') AS known_gate_v2_h4_extension_key_dedupe_status,
    p.eligible_loans,
    p.candidate_loans,
    p.candidate_loan_documents,
    p.legal_confirmed_candidate_loans,
    p.legal_confirmed_loan_documents,
    p.accepted_h7_loans,
    p.ambiguous_document_identity_loans,
    p.candidate_without_legal_loans,
    p.no_candidate_loans,
    p.accepted_h7_multi_bbl_loans,
    p.admitted_consensus_subjects,
    p.rejected_accepted_71_contamination_loans,
    p.rejected_known_extension_duplicate_loans,
    p.rejected_missing_legal_rows_loans,
    p.rejected_not_document_ambiguous_loans,
    p.rejected_document_bbl_set_not_multi_bbl_loans,
    p.rejected_document_bbl_set_disagreement_loans,
    p.eligible_denominator_reconciles,
    p.candidate_denominator_reconciles,
    p.legal_denominator_reconciles,
    p.ambiguous_consensus_denominator_reconciles,
    c.property_keys,
    c.amount_cents,
    c.originationdate,
    c.originatorname,
    c.originator_match_text,
    c.candidate_document_count,
    c.legal_document_count,
    c.candidate_without_legal_document_count,
    c.min_document_bbl_count,
    c.max_document_bbl_count,
    c.distinct_bbl_set_count,
    c.consensus_bbl_count,
    c.consensus_bbl_set_key,
    c.consensus_bbls,
    c.filed_counties,
    c.filed_boroughs,
    c.filed_county_borough_edges,
    c.distinct_counts,
    c.bridge_source_record_ids,
    c.candidate_document_ids,
    c.acris_master_source_record_ids,
    c.acris_master_raw_csv_sha256s,
    c.acris_master_filenames,
    c.acris_party_source_record_ids,
    c.acris_party_raw_csv_sha256s,
    c.acris_party_filenames,
    c.acris_legal_source_record_ids,
    c.acris_legal_raw_csv_sha256s,
    c.acris_legal_filenames
  FROM consensus_subjects c
  JOIN plane_denominators p USING (truth_plane)
  CROSS JOIN guard_summary g
  WHERE g.guard_status = 'ok'
),
guard_output AS (
  SELECT
    (SELECT row_contract FROM params) AS row_contract,
    'guard_failure'::TEXT AS row_kind,
    (SELECT query_purpose FROM params) AS query_purpose,
    g.guard_status,
    f.failure_reason AS refusal_reason,
    NULL::TEXT AS truth_plane,
    NULL::TEXT AS loan_key,
    NULL::TEXT AS association_plane,
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
    IFF((SELECT known_extension_key_rows FROM known_extension_key_stats) = 0,
      'not_available_in_described_warehouse_tables',
      'available_and_applied') AS known_gate_v2_h4_extension_key_dedupe_status,
    NULL::NUMBER(38,0) AS eligible_loans,
    NULL::NUMBER(38,0) AS candidate_loans,
    NULL::NUMBER(38,0) AS candidate_loan_documents,
    NULL::NUMBER(38,0) AS legal_confirmed_candidate_loans,
    NULL::NUMBER(38,0) AS legal_confirmed_loan_documents,
    NULL::NUMBER(38,0) AS accepted_h7_loans,
    NULL::NUMBER(38,0) AS ambiguous_document_identity_loans,
    NULL::NUMBER(38,0) AS candidate_without_legal_loans,
    NULL::NUMBER(38,0) AS no_candidate_loans,
    NULL::NUMBER(38,0) AS accepted_h7_multi_bbl_loans,
    NULL::NUMBER(38,0) AS admitted_consensus_subjects,
    NULL::NUMBER(38,0) AS rejected_accepted_71_contamination_loans,
    NULL::NUMBER(38,0) AS rejected_known_extension_duplicate_loans,
    NULL::NUMBER(38,0) AS rejected_missing_legal_rows_loans,
    NULL::NUMBER(38,0) AS rejected_not_document_ambiguous_loans,
    NULL::NUMBER(38,0) AS rejected_document_bbl_set_not_multi_bbl_loans,
    NULL::NUMBER(38,0) AS rejected_document_bbl_set_disagreement_loans,
    NULL::BOOLEAN AS eligible_denominator_reconciles,
    NULL::BOOLEAN AS candidate_denominator_reconciles,
    NULL::BOOLEAN AS legal_denominator_reconciles,
    NULL::BOOLEAN AS ambiguous_consensus_denominator_reconciles,
    NULL::NUMBER(38,0) AS property_keys,
    NULL::NUMBER(38,0) AS amount_cents,
    NULL::DATE AS originationdate,
    NULL::TEXT AS originatorname,
    NULL::TEXT AS originator_match_text,
    NULL::NUMBER(38,0) AS candidate_document_count,
    NULL::NUMBER(38,0) AS legal_document_count,
    NULL::NUMBER(38,0) AS candidate_without_legal_document_count,
    NULL::NUMBER(38,0) AS min_document_bbl_count,
    NULL::NUMBER(38,0) AS max_document_bbl_count,
    NULL::NUMBER(38,0) AS distinct_bbl_set_count,
    NULL::NUMBER(38,0) AS consensus_bbl_count,
    NULL::TEXT AS consensus_bbl_set_key,
    NULL::VARIANT AS consensus_bbls,
    NULL::VARIANT AS filed_counties,
    NULL::VARIANT AS filed_boroughs,
    NULL::VARIANT AS filed_county_borough_edges,
    NULL::VARIANT AS distinct_counts,
    NULL::VARIANT AS bridge_source_record_ids,
    NULL::VARIANT AS candidate_document_ids,
    NULL::VARIANT AS acris_master_source_record_ids,
    NULL::VARIANT AS acris_master_raw_csv_sha256s,
    NULL::VARIANT AS acris_master_filenames,
    NULL::VARIANT AS acris_party_source_record_ids,
    NULL::VARIANT AS acris_party_raw_csv_sha256s,
    NULL::VARIANT AS acris_party_filenames,
    NULL::VARIANT AS acris_legal_source_record_ids,
    NULL::VARIANT AS acris_legal_raw_csv_sha256s,
    NULL::VARIANT AS acris_legal_filenames
  FROM guard_failures f
  CROSS JOIN guard_summary g
)
SELECT *
FROM summary_output
UNION ALL
SELECT *
FROM consensus_output
UNION ALL
SELECT *
FROM guard_output
ORDER BY row_kind, truth_plane, loan_key
