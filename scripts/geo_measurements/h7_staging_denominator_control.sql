-- Appendix H.7 release-pinned full-plane denominator control.
--
-- This control measures candidate reach and accepted multi-BBL truth counts
-- over both disjoint H.7 planes. It is not a population-row export: it emits
-- two aggregate rows and therefore cannot supply source evidence records or
-- solver candidate parcels. Use the staged Stage 1/2/3 files for row-level
-- acquisition.
--
-- Truth ordering is load-bearing:
--   1. select the raw filed-state/county loan universe;
--   2. form amount/date candidates, adding exact lender-party equality only
--      for the round plane;
--   3. bind those candidate documents to LEGALS using each loan's filed
--      boroughs, never MASTER.RECORDED_BOROUGH;
--   4. accept only loans with one legal-confirmed candidate document;
--   5. define multi-parcel truth only after acceptance by distinct legal BBLs.
--
-- Candidate reach is upstream of solver truth. The two planes remain separate
-- and their precision must never be pooled. Duplicate ACRIS rows are collapsed
-- at their declared document/party/legal grains and never counted as
-- independent information.

WITH
params AS (
  SELECT
    '3aed6660-ce1c-46a9-aeb2-7296c134ce8f'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    'NY'::TEXT AS property_state,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    45::NUMBER(9,0) AS max_recording_offset_days
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
    COUNT(DISTINCT IFF(originationdate IS NOT NULL, originationdate, NULL))
      AS distinct_originationdate,
    COUNT(DISTINCT IFF(amount_cents IS NOT NULL, amount_cents, NULL))
      AS distinct_amount_cents,
    COUNT(DISTINCT IFF(originatorname IS NOT NULL, originatorname, NULL))
      AS distinct_originatorname,
    COUNT(DISTINCT IFF(originator_match_text IS NOT NULL,
      originator_match_text, NULL)) AS distinct_originator_match_text,
    MAX(originationdate) AS originationdate,
    MAX(amount_cents) AS amount_cents,
    MAX(originator_match_text) AS originator_match_text,
    ARRAY_AGG(DISTINCT filed_borough)
      WITHIN GROUP (ORDER BY filed_borough) AS filed_boroughs
  FROM ny_filed_bridge_rows
  GROUP BY loan_key
),
eligible_loans AS (
  SELECT
    loan_key,
    originationdate,
    amount_cents,
    originator_match_text,
    filed_boroughs,
    IFF(
      MOD(amount_cents, (SELECT round_amount_lattice_cents FROM params)) = 0,
      'round_exact_lender_party',
      'non_round_amount_date_legal_borough'
    ) AS truth_plane
  FROM loan_counts
  WHERE distinct_originationdate = 1
    AND distinct_amount_cents = 1
    AND amount_cents <> 0
    AND distinct_originatorname <= 1
    AND distinct_originator_match_text <= 1
    AND ARRAY_SIZE(filed_boroughs) > 0
),
master_candidates AS (
  SELECT
    l.loan_key,
    l.truth_plane,
    l.filed_boroughs,
    m.document_id::TEXT AS document_id
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

  UNION ALL

  SELECT
    l.loan_key,
    l.truth_plane,
    l.filed_boroughs,
    m.document_id::TEXT AS document_id
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
),
candidate_documents AS (
  SELECT DISTINCT loan_key, truth_plane, filed_boroughs, document_id
  FROM master_candidates
),
candidate_filed_boroughs AS (
  SELECT DISTINCT
    c.loan_key,
    c.truth_plane,
    c.document_id,
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
    l.bbl::TEXT AS legal_bbl
  FROM candidate_filed_boroughs c
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS l
    ON l.release_dt = (SELECT acris_release_dt FROM params)
   AND l.document_id = c.document_id
   AND l.legal_borough = c.filed_borough
  WHERE l.block IS NOT NULL
    AND l.lot IS NOT NULL
    AND l.bbl IS NOT NULL
),
legal_documents AS (
  SELECT
    loan_key,
    truth_plane,
    document_id,
    COUNT(DISTINCT legal_bbl) AS bbl_count
  FROM legal_edges
  GROUP BY loan_key, truth_plane, document_id
),
loan_legal_disposition AS (
  SELECT
    loan_key,
    truth_plane,
    COUNT(*) AS legal_document_count,
    MAX(bbl_count) AS accepted_bbl_count
  FROM legal_documents
  GROUP BY loan_key, truth_plane
),
eligible_counts AS (
  SELECT
    truth_plane,
    COUNT(*) AS eligible_loans,
    COUNT_IF(originator_match_text IS NOT NULL) AS originator_text_loans
  FROM eligible_loans
  GROUP BY truth_plane
),
candidate_counts AS (
  SELECT
    truth_plane,
    COUNT(DISTINCT loan_key) AS candidate_loans,
    COUNT(DISTINCT loan_key || '|' || document_id) AS candidate_loan_documents
  FROM candidate_documents
  GROUP BY truth_plane
),
legal_counts AS (
  SELECT
    truth_plane,
    COUNT(*) AS legal_confirmed_loan_documents,
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
      AS accepted_multi_bbl_loans,
    SUM(IFF(legal_document_count = 1, accepted_bbl_count, 0))
      AS accepted_bbl_edges
  FROM loan_legal_disposition
  GROUP BY truth_plane
),
accepted_multi_bbl_subjects AS (
  SELECT
    d.truth_plane,
    d.loan_key,
    l.document_id,
    l.bbl_count
  FROM loan_legal_disposition d
  JOIN legal_documents l
    ON l.truth_plane = d.truth_plane
   AND l.loan_key = d.loan_key
  WHERE d.legal_document_count = 1
    AND l.bbl_count > 1
),
accepted_multi_bbl_export AS (
  SELECT
    truth_plane,
    ARRAY_AGG(
      OBJECT_CONSTRUCT(
        'loan_key', loan_key,
        'document_id', document_id,
        'bbl_count', bbl_count
      )
    ) WITHIN GROUP (ORDER BY loan_key, document_id)
      AS accepted_multi_bbl_subjects
  FROM accepted_multi_bbl_subjects
  GROUP BY truth_plane
)
SELECT
  e.truth_plane,
  e.eligible_loans,
  e.originator_text_loans,
  COALESCE(c.candidate_loans, 0) AS candidate_loans,
  COALESCE(c.candidate_loan_documents, 0) AS candidate_loan_documents,
  COALESCE(l.legal_confirmed_candidate_loans, 0)
    AS legal_confirmed_candidate_loans,
  COALESCE(l.legal_confirmed_loan_documents, 0)
    AS legal_confirmed_loan_documents,
  COALESCE(a.accepted_loans, 0) AS accepted_loans,
  COALESCE(a.ambiguous_loans, 0) AS ambiguous_loans,
  COALESCE(c.candidate_loans, 0)
    - COALESCE(l.legal_confirmed_candidate_loans, 0)
    AS candidate_without_legal_loans,
  e.eligible_loans - COALESCE(c.candidate_loans, 0) AS no_candidate_loans,
  COALESCE(a.accepted_multi_bbl_loans, 0) AS accepted_multi_bbl_loans,
  COALESCE(a.accepted_bbl_edges, 0) AS accepted_bbl_edges,
  COALESCE(x.accepted_multi_bbl_subjects, [])
    AS accepted_multi_bbl_subjects,
  e.eligible_loans = COALESCE(c.candidate_loans, 0)
    + (e.eligible_loans - COALESCE(c.candidate_loans, 0))
    AS eligible_denominator_reconciles,
  COALESCE(c.candidate_loans, 0)
    = COALESCE(l.legal_confirmed_candidate_loans, 0)
      + (COALESCE(c.candidate_loans, 0)
        - COALESCE(l.legal_confirmed_candidate_loans, 0))
    AS candidate_denominator_reconciles,
  COALESCE(l.legal_confirmed_candidate_loans, 0)
    = COALESCE(a.accepted_loans, 0) + COALESCE(a.ambiguous_loans, 0)
    AS legal_denominator_reconciles
FROM eligible_counts e
LEFT JOIN candidate_counts c USING (truth_plane)
LEFT JOIN legal_counts l USING (truth_plane)
LEFT JOIN acceptance_counts a USING (truth_plane)
LEFT JOIN accepted_multi_bbl_export x USING (truth_plane)
ORDER BY e.truth_plane;
