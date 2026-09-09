-- D3/G3 selected-deal loan-to-H7 mapping for bd-2ocv.
--
-- Grain: one row per loan in each selected deal, not one row per reached H7
-- subject. Rows without an H7 subject are retained as reach-none rows with
-- reach_none_reason = not_in_h7_cohort so the denominator remains the full
-- public deal loan count.
--
-- The selected deals come from scripts/geo_measurements/d3_deal_selection.sql.
-- Fresh cmdrvl-data MCP probe on 2026-09-09 measured 56 rows for rank 1 and
-- 55 rows for rank 2, with H7 subject counts 3 and 2 respectively.

WITH
selected_deals AS (
  SELECT
    column1::NUMBER(9, 0) AS selection_rank,
    column2::TEXT AS accession,
    column3::TEXT AS deal_id
  FROM VALUES
    (1, '0001539497-19-000868', '1774962'),
    (2, '0001539497-19-002255', '1794303')
),
selected_h7_subjects AS (
  SELECT
    column1::TEXT AS loan_key,
    column2::TEXT AS subject_id,
    column3::TEXT AS document_id,
    column4::TEXT AS truth_plane,
    column5::TEXT AS reach_status,
    column6::TEXT AS deed_ids_pipe
  FROM VALUES
    (
      '0048d206538d873999f3c2d49f259d31',
      'h7-subject:round-exact-lender:7b694e770de8331827bf785d9303bf2d5d8857fdc235011fa2a9b0c2cf37dbf6',
      '2019050100901003',
      'round_exact_lender_party',
      'full',
      '2019050100901003'
    ),
    (
      '5387c521ca6a6f0dc15c6badec8c0134',
      'h7-subject:round-exact-lender:68f1e66c57a4b35a301701ef284c0ad4fcbae8459fd3d193bf760c1f3a8b5e12',
      '2019020700582001',
      'round_exact_lender_party',
      'full',
      '2019020700582001'
    ),
    (
      '6bfe47de21ff7d7e24bf6464871dea9f',
      'h7-subject:non-round:e1ee05351a5b681c0dcda44e19dd575c686c3c630809c137bd64cc3232bc256c',
      '2019050100411003',
      'non_round_amount_date_legal_borough',
      'partial',
      '2019050100411003'
    ),
    (
      '59d308b789056c1be5bbee76a1132dc1',
      'h7-subject:non-round:f970390c493da3e962182756639afd861946bb781d236481165898bd617f4952',
      '2019112600688009',
      'non_round_amount_date_legal_borough',
      'full',
      '2019112600688009'
    ),
    (
      'd34cdcf84fc0badf2d2c364af2ec6ebd',
      'h7-subject:non-round:607a641f16e5c5a60a0490d5b38d606e4d15bb34078183255c3f84935c6f56e6',
      '2019112600816007',
      'non_round_amount_date_legal_borough',
      'full',
      '2019112600816007'
    )
),
deal_loans AS (
  SELECT
    d.selection_rank,
    li.first_seen_filing_id AS accession,
    li.cik AS deal_id,
    li.company_name,
    li.assetnumber,
    li.loan_key,
    li.period_fact_build_id,
    li.first_seen_filing_date,
    li.dataset_code
  FROM selected_deals d
  JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li
    ON li.first_seen_filing_id = d.accession
   AND li.cik = d.deal_id
  WHERE li.loan_key IS NOT NULL
)
SELECT
  'canon_geo_ledger_loan_refs.v0' AS row_contract,
  selection_rank,
  accession,
  deal_id,
  company_name,
  assetnumber,
  l.loan_key AS loan_id,
  h.subject_id,
  h.document_id,
  IFF(h.deed_ids_pipe IS NULL, ARRAY_CONSTRUCT(), SPLIT(h.deed_ids_pipe, '|')) AS deed_ids,
  h.truth_plane,
  COALESCE(h.reach_status, 'none') AS candidate_reach,
  CASE
    WHEN h.loan_key IS NULL THEN 'not_in_h7_cohort'
    WHEN h.reach_status = 'none' THEN 'h7_reach_none'
    ELSE NULL
  END AS reach_none_reason,
  period_fact_build_id,
  TO_VARCHAR(first_seen_filing_date, 'YYYY-MM-DD') AS first_seen_filing_date,
  dataset_code AS source_dataset
FROM deal_loans l
LEFT JOIN selected_h7_subjects h
  ON h.loan_key = l.loan_key
ORDER BY selection_rank, TRY_TO_NUMBER(assetnumber), assetnumber, loan_key;
