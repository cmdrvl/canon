-- NYC H.7 observer source-population export for bd-2g4z.
--
-- This is the pre-observer source query for
-- scripts/geo_observer/populations/nyc_h7_observer_source_population.v0.json.
-- It is a retained acquisition-side query, not a Canon runtime network path.
-- The observer population is selected from the complete-window rows emitted by
-- this query using splitmix64 and the seed recorded in
-- nyc_h7_observer_population.v0.json.

WITH
params AS (
  SELECT
    'd5ddd2d9-07dc-44d6-bf8b-b7bfc373dbc3'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    '26v2'::TEXT AS mappluto_release,
    'NY'::TEXT AS property_state,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    45::NUMBER(9,0) AS max_recording_offset_days
),
filed_county_map AS (
  SELECT * FROM VALUES
    ('NEW YORK', 1), ('MANHATTAN', 1), ('NY061', 1),
    ('BRONX', 2), ('KINGS', 3), ('BROOKLYN', 3),
    ('QUEENS', 4), ('RICHMOND', 5)
  AS m(propertycounty, filed_borough)
),
mortgage_doc_types AS (
  SELECT column1::TEXT AS doc_type
  FROM VALUES ('MTGE'), ('M&CON'), ('CMTG'), ('SMTG'), ('MMTG'), ('SPRD')
),
lender_party_roles AS (
  SELECT * FROM VALUES
    ('CMTG', '2'), ('M&CON', '2'), ('MMTG', '1'),
    ('MTGE', '2'), ('SMTG', '2'), ('SPRD', '2')
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
    m.filed_borough
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
  JOIN params p ON lip.build_id = p.bridge_build_id
  LEFT JOIN filed_county_map m
    ON UPPER(TRIM(lip.propertycounty)) = m.propertycounty
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
    MAX(originationdate) AS originationdate,
    MAX(amount_cents) AS amount_cents,
    MAX(originator_match_text) AS originator_match_text,
    ARRAY_AGG(DISTINCT filed_borough)
      WITHIN GROUP (ORDER BY filed_borough) AS filed_boroughs
  FROM bridge_rows
  WHERE property_state = (SELECT property_state FROM params)
    AND filed_borough IS NOT NULL
  GROUP BY loan_key
),
eligible_loans AS (
  SELECT
    loan_key,
    property_keys,
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
    l.property_keys,
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
    l.property_keys,
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
  SELECT DISTINCT loan_key, property_keys, truth_plane, filed_boroughs, document_id
  FROM master_candidates
),
candidate_filed_boroughs AS (
  SELECT DISTINCT
    c.loan_key,
    c.property_keys,
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
    c.property_keys,
    c.truth_plane,
    c.document_id,
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
    property_keys,
    truth_plane,
    document_id,
    COUNT(DISTINCT legal_bbl) AS bbl_count
  FROM legal_edges
  GROUP BY loan_key, property_keys, truth_plane, document_id
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
accepted_edges AS (
  SELECT l.*
  FROM legal_edges l
  JOIN legal_documents d USING (loan_key, truth_plane, document_id)
  JOIN loan_legal_disposition disp USING (loan_key, truth_plane)
  WHERE disp.legal_document_count = 1
    AND d.bbl_count > 1
),
source_rows AS (
  SELECT
    e.loan_key AS subject_id,
    e.truth_plane,
    e.document_id,
    MAX(e.property_keys) AS bridge_property_keys,
    LISTAGG(DISTINCT e.legal_bbl, ',') WITHIN GROUP (ORDER BY e.legal_bbl)
      AS parcel_ids_csv,
    COUNT(DISTINCT e.legal_bbl) AS parcel_count,
    COUNT(DISTINCT mp.bbl_key) AS mappluto_parcels_found,
    MIN(mp.bbox_xmin) AS bbox_xmin,
    MIN(mp.bbox_ymin) AS bbox_ymin,
    MAX(mp.bbox_xmax) AS bbox_xmax,
    MAX(mp.bbox_ymax) AS bbox_ymax,
    LISTAGG(DISTINCT mp.bbl_key, ',') WITHIN GROUP (ORDER BY mp.bbl_key)
      AS mapped_parcel_ids_csv
  FROM accepted_edges e
  LEFT JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES mp
    ON mp.release = (SELECT mappluto_release FROM params)
   AND mp.variant = 'shoreline_clipped'
   AND mp.bbl_key = e.legal_bbl
  GROUP BY e.loan_key, e.truth_plane, e.document_id
)
SELECT
  subject_id,
  truth_plane,
  document_id,
  bridge_property_keys,
  parcel_count,
  parcel_ids_csv,
  mappluto_parcels_found,
  COALESCE(mapped_parcel_ids_csv, '') AS mapped_parcel_ids_csv,
  bbox_xmin,
  bbox_ymin,
  bbox_xmax,
  bbox_ymax,
  mappluto_parcels_found = parcel_count AS complete_observer_window
FROM source_rows
ORDER BY truth_plane, subject_id;
