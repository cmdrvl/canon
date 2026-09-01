-- Appendix H.7 candidate-strategy comparison over accepted multi-BBL truth.
--
-- This file reconstructs the release-pinned accepted H7 subjects, then
-- compares two upstream candidate selectors over the identical subject/release
-- denominator:
--   * H3 r8 home cell plus k1 parcel membership.
--   * Point-in-parcel to all parcels in the same six-digit BBL block.
--
-- Candidate membership is constructed before the accepted legal BBL array is
-- flattened for reach scoring. The union cascade row is reach accounting only;
-- it is not a loan-level monolithic solve input.
--
-- Byte-substitute only:
--   '__BD7BCP_H7_CURRENT_BRIDGE_BUILD_ID__'

WITH
params AS (
  SELECT
    'h7_candidate_strategy_comparison_row.v0'::TEXT AS row_contract,
    '__BD7BCP_H7_CURRENT_BRIDGE_BUILD_ID__'::TEXT AS bridge_build_id,
    '2026-08-10'::DATE AS acris_release_dt,
    'NY'::TEXT AS property_state,
    'nyc_filed_collateral_slice'::TEXT AS collateral_scope,
    'ROUND(value * 100, 0)::NUMBER(38,0)'::TEXT
      AS amount_cents_quantization,
    10000000::NUMBER(38,0) AS round_amount_lattice_cents,
    45::NUMBER(9,0) AS max_recording_offset_days,
    8::NUMBER(9,0) AS h3_resolution,
    1::NUMBER(9,0) AS halo_k
),
sentinel_markers AS (
  SELECT
    ('__BD7BCP_H7_' || 'CURRENT_BRIDGE_BUILD_ID__')::TEXT
      AS bridge_build_id_unbound_marker
),
release_pins AS (
  SELECT * FROM VALUES
    ('26v1', '2026-05-01'::DATE, 'shoreline_clipped'),
    ('26v2', '2026-08-01'::DATE, 'shoreline_clipped')
  AS p(release, release_dt, variant)
),
release_pin_stats AS (
  SELECT
    COUNT(*) AS release_pin_rows,
    COUNT(DISTINCT release || '|' || TO_VARCHAR(release_dt) || '|' || variant)
      AS distinct_release_pins
  FROM release_pins
),
candidate_selectors AS (
  SELECT * FROM VALUES
    (1, 'h3_r8_k1',
      'h3_centroid_section_membership',
      'single_selector'),
    (2, 'pip_six_digit_bbl_block',
      'point_in_parcel_then_six_digit_bbl_block',
      'single_selector'),
    (3, 'union_cascade_h3_then_pip_block',
      'union_of_h3_r8_k1_and_pip_block',
      'union_cascade_reach_accounting_only')
  AS s(selector_order, candidate_selector, candidate_strategy, selector_role)
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
    IFF(
      MOD(c.amount_cents,
        (SELECT round_amount_lattice_cents FROM params)) = 0,
      'round_exact_lender_party',
      'non_round_amount_date_legal_borough'
    ) AS truth_plane
  FROM loan_counts c
  JOIN loan_filed_county_edges e USING (loan_key)
  WHERE c.distinct_originationdate = 1
    AND c.distinct_amount_cents = 1
    AND c.amount_cents <> 0
    AND c.distinct_originatorname <= 1
    AND c.distinct_originator_match_text <= 1
    AND c.distinct_filed_borough > 0
),
master_candidates_non_round AS (
  SELECT DISTINCT
    l.loan_key,
    l.truth_plane,
    l.association_plane,
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
),
master_candidates_round AS (
  SELECT DISTINCT
    l.loan_key,
    l.truth_plane,
    l.association_plane,
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
  SELECT DISTINCT loan_key, truth_plane, association_plane, filed_boroughs,
    document_id
  FROM master_candidates_non_round
  UNION
  SELECT DISTINCT loan_key, truth_plane, association_plane, filed_boroughs,
    document_id
  FROM master_candidates_round
),
candidate_filed_boroughs AS (
  SELECT DISTINCT
    c.loan_key,
    c.truth_plane,
    c.association_plane,
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
    c.association_plane,
    c.document_id,
    c.filed_borough,
    COALESCE(
      NULLIF(REGEXP_REPLACE(TO_VARCHAR(l.bbl), '[.]0$', ''), ''),
      TO_VARCHAR(l.legal_borough::NUMBER(38,0))
        || LPAD(TO_VARCHAR(l.block::NUMBER(38,0)), 5, '0')
        || LPAD(TO_VARCHAR(l.lot::NUMBER(38,0)), 4, '0')
    ) AS legal_bbl
  FROM candidate_filed_boroughs c
  JOIN EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS l
    ON l.release_dt = (SELECT acris_release_dt FROM params)
   AND l.document_id = c.document_id
   AND l.legal_borough = c.filed_borough
  WHERE l.block IS NOT NULL
    AND l.lot IS NOT NULL
),
legal_documents AS (
  SELECT
    loan_key,
    truth_plane,
    association_plane,
    document_id,
    COUNT(DISTINCT legal_bbl) AS bbl_count,
    ARRAY_AGG(DISTINCT legal_bbl) WITHIN GROUP (ORDER BY legal_bbl)
      AS truth_bbls
  FROM legal_edges
  WHERE legal_bbl IS NOT NULL
  GROUP BY loan_key, truth_plane, association_plane, document_id
),
loan_legal_disposition AS (
  SELECT
    loan_key,
    truth_plane,
    association_plane,
    COUNT(*) AS legal_document_count
  FROM legal_documents
  GROUP BY loan_key, truth_plane, association_plane
),
accepted_subjects AS (
  SELECT
    d.truth_plane || '|' || d.loan_key AS subject_id,
    d.loan_key,
    d.truth_plane,
    d.association_plane,
    d.document_id,
    d.bbl_count AS truth_bbl_count,
    d.truth_bbls,
    l.filed_boroughs
  FROM loan_legal_disposition x
  JOIN legal_documents d
    ON d.loan_key = x.loan_key
   AND d.truth_plane = x.truth_plane
   AND d.association_plane = x.association_plane
  JOIN eligible_loans l
    ON l.loan_key = d.loan_key
   AND l.truth_plane = d.truth_plane
   AND l.association_plane = d.association_plane
  WHERE x.legal_document_count = 1
    AND d.bbl_count > 1
),
subject_releases AS (
  SELECT
    s.subject_id,
    s.loan_key,
    s.truth_plane,
    s.association_plane,
    s.document_id,
    s.truth_bbl_count,
    s.truth_bbls,
    s.filed_boroughs,
    r.release,
    r.release_dt,
    r.variant
  FROM accepted_subjects s
  CROSS JOIN release_pins r
),
selector_subject_releases AS (
  SELECT
    sr.*,
    cs.selector_order,
    cs.candidate_selector,
    cs.candidate_strategy,
    cs.selector_role
  FROM subject_releases sr
  CROSS JOIN candidate_selectors cs
),
accepted_subject_stats AS (
  SELECT
    COUNT(*) AS accepted_subjects,
    COUNT(DISTINCT subject_id) AS distinct_accepted_subjects,
    COUNT_IF(truth_plane = 'non_round_amount_date_legal_borough')
      AS non_round_accepted_subjects,
    COUNT_IF(truth_plane = 'round_exact_lender_party')
      AS round_accepted_subjects,
    COUNT_IF(truth_bbl_count <> ARRAY_SIZE(truth_bbls))
      AS truth_bbl_count_mismatch_subjects
  FROM accepted_subjects
),
subject_release_stats AS (
  SELECT
    COUNT(*) AS subject_release_rows,
    COUNT(DISTINCT release) AS release_count
  FROM subject_releases
),
guard_failures AS (
  SELECT failure_reason
  FROM (
    SELECT
      'bridge_build_id_sentinel_unsubstituted' AS failure_reason,
      (SELECT bridge_build_id FROM params)
        = (SELECT bridge_build_id_unbound_marker
           FROM sentinel_markers) AS failed
    UNION ALL
    SELECT 'accepted_subject_count_empty',
      (SELECT accepted_subjects FROM accepted_subject_stats)
        = 0
    UNION ALL
    SELECT 'accepted_subject_repeats',
      (SELECT accepted_subjects FROM accepted_subject_stats)
        <> (SELECT distinct_accepted_subjects FROM accepted_subject_stats)
    UNION ALL
    SELECT 'non_round_denominator_empty',
      (SELECT non_round_accepted_subjects FROM accepted_subject_stats) = 0
    UNION ALL
    SELECT 'round_denominator_empty',
      (SELECT round_accepted_subjects FROM accepted_subject_stats) = 0
    UNION ALL
    SELECT 'truth_bbl_count_mismatch',
      (SELECT truth_bbl_count_mismatch_subjects FROM accepted_subject_stats) <> 0
    UNION ALL
    SELECT 'duplicate_release_pin',
      (SELECT release_pin_rows FROM release_pin_stats)
        <> (SELECT distinct_release_pins FROM release_pin_stats)
    UNION ALL
    SELECT 'release_count_mismatch',
      (SELECT release_count FROM subject_release_stats)
        <> (SELECT release_pin_rows FROM release_pin_stats)
    UNION ALL
    SELECT 'subject_release_denominator_mismatch',
      (SELECT subject_release_rows FROM subject_release_stats)
        <> (SELECT accepted_subjects FROM accepted_subject_stats)
          * (SELECT release_pin_rows FROM release_pin_stats)
  )
  WHERE failed
),
guard_summary AS (
  SELECT
    IFF(COUNT(*) = 0, 'ok', 'refused') AS guard_status,
    LISTAGG(failure_reason, '|') WITHIN GROUP (ORDER BY failure_reason)
      AS refusal_reason
  FROM guard_failures
),
points AS (
  SELECT DISTINCT
    s.subject_id,
    s.loan_key,
    s.truth_plane,
    s.association_plane,
    lip.property_key,
    lip.latitude,
    lip.longitude,
    m.filed_borough,
    H3_POINT_TO_CELL_STRING(
      ST_MAKEPOINT(lip.longitude, lip.latitude),
      (SELECT h3_resolution FROM params)
    ) AS home_cell
  FROM accepted_subjects s
  JOIN EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY lip
    ON lip.build_id = (SELECT bridge_build_id FROM params)
   AND lip.loan_key = s.loan_key
  JOIN filed_county_map m
    ON UPPER(TRIM(lip.propertycounty)) = m.propertycounty
  WHERE (SELECT guard_status FROM guard_summary) = 'ok'
    AND lip.propertystate = (SELECT property_state FROM params)
    AND lip.latitude IS NOT NULL
    AND lip.longitude IS NOT NULL
    AND ARRAY_CONTAINS(m.filed_borough::VARIANT, s.filed_boroughs)
),
point_work_cells AS (
  SELECT DISTINCT
    p.subject_id,
    p.loan_key,
    p.truth_plane,
    p.association_plane,
    p.property_key,
    p.home_cell,
    H3_STRING_TO_INT(cell.value::TEXT) AS work_cell_int
  FROM points p,
    LATERAL FLATTEN(
      input => H3_GRID_DISK(
        p.home_cell,
        (SELECT halo_k FROM params)
      )
    ) cell
),
h3_parcel_index AS (
  SELECT
    h.release,
    h.release_dt,
    h.feature_type AS variant,
    h.bbl_key,
    h.h3_r8_int
  FROM EDGAR_DB.DBT_STAGING_GEO.STG_GEO_GEOMETRY_HOT_KEYS h
  JOIN release_pins pin
    ON h.release = pin.release
   AND h.release_dt = pin.release_dt
   AND h.feature_type = pin.variant
  WHERE h.source_system = 'nyc_dcp'
    AND h.source_table = 'nyc_dcp_mappluto_hot'
    AND h.dataset = 'mappluto'
    AND h.state_key = 'NY'
    AND h.bbl_key_status = 'valid'
    AND h.h3_r8_status = 'valid'
    AND h.key_validation_status = 'valid'
    AND h.bbl_key IS NOT NULL
    AND h.h3_r8_int IS NOT NULL
),
h3_candidate_members AS (
  SELECT DISTINCT
    w.subject_id,
    w.loan_key,
    w.truth_plane,
    w.association_plane,
    p.release,
    p.release_dt,
    p.variant,
    p.bbl_key AS candidate_bbl
  FROM point_work_cells w
  JOIN h3_parcel_index p
    ON p.h3_r8_int = w.work_cell_int
),
parcels AS (
  SELECT
    p.release,
    p.release_dt,
    p.variant,
    p.bbl_key,
    SUBSTR(p.bbl_key, 1, 6) AS block_key,
    p.geom_geog,
    p.bbox_xmin,
    p.bbox_ymin,
    p.bbox_xmax,
    p.bbox_ymax
  FROM EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_MAPPLUTO_PARCEL_VINTAGES p
  JOIN release_pins pin
    ON p.release = pin.release
   AND p.release_dt = pin.release_dt
   AND p.variant = pin.variant
  WHERE p.bbl_key_status = 'valid'
    AND p.key_validation_status = 'valid'
    AND p.bbl_key IS NOT NULL
    AND p.geom_geog IS NOT NULL
),
pip_edges AS (
  SELECT DISTINCT
    p.subject_id,
    p.loan_key,
    p.truth_plane,
    p.association_plane,
    p.property_key,
    parcel.release,
    parcel.release_dt,
    parcel.variant,
    parcel.bbl_key AS pip_bbl,
    parcel.block_key
  FROM points p
  JOIN parcels parcel
    ON p.longitude BETWEEN parcel.bbox_xmin AND parcel.bbox_xmax
   AND p.latitude BETWEEN parcel.bbox_ymin AND parcel.bbox_ymax
   AND ST_CONTAINS(
     parcel.geom_geog,
     ST_MAKEPOINT(p.longitude, p.latitude)
   )
),
pip_blocks AS (
  SELECT DISTINCT
    subject_id,
    loan_key,
    truth_plane,
    association_plane,
    property_key,
    release,
    release_dt,
    variant,
    block_key
  FROM pip_edges
),
pip_candidate_members AS (
  SELECT DISTINCT
    b.subject_id,
    b.loan_key,
    b.truth_plane,
    b.association_plane,
    b.release,
    b.release_dt,
    b.variant,
    p.bbl_key AS candidate_bbl
  FROM pip_blocks b
  JOIN parcels p
    ON p.release = b.release
   AND p.release_dt = b.release_dt
   AND p.variant = b.variant
   AND p.block_key = b.block_key
),
selector_candidate_members AS (
  SELECT DISTINCT
    'h3_r8_k1'::TEXT AS candidate_selector,
    subject_id,
    loan_key,
    truth_plane,
    association_plane,
    release,
    release_dt,
    variant,
    candidate_bbl
  FROM h3_candidate_members
  UNION
  SELECT DISTINCT
    'pip_six_digit_bbl_block'::TEXT AS candidate_selector,
    subject_id,
    loan_key,
    truth_plane,
    association_plane,
    release,
    release_dt,
    variant,
    candidate_bbl
  FROM pip_candidate_members
),
cascade_candidate_members AS (
  SELECT DISTINCT
    'union_cascade_h3_then_pip_block'::TEXT AS candidate_selector,
    subject_id,
    loan_key,
    truth_plane,
    association_plane,
    release,
    release_dt,
    variant,
    candidate_bbl
  FROM selector_candidate_members
),
candidate_members AS (
  SELECT * FROM selector_candidate_members
  UNION
  SELECT * FROM cascade_candidate_members
),
h3_candidate_counts AS (
  SELECT
    subject_id,
    truth_plane,
    association_plane,
    release,
    COUNT(DISTINCT candidate_bbl) AS h3_candidate_bbls
  FROM h3_candidate_members
  GROUP BY subject_id, truth_plane, association_plane, release
),
pip_candidate_counts AS (
  SELECT
    subject_id,
    truth_plane,
    association_plane,
    release,
    COUNT(DISTINCT candidate_bbl) AS pip_candidate_bbls
  FROM pip_candidate_members
  GROUP BY subject_id, truth_plane, association_plane, release
),
h3_pip_overlap_counts AS (
  SELECT
    h.subject_id,
    h.truth_plane,
    h.association_plane,
    h.release,
    COUNT(DISTINCT h.candidate_bbl) AS h3_pip_overlap_bbls
  FROM h3_candidate_members h
  JOIN pip_candidate_members p
    ON p.subject_id = h.subject_id
   AND p.truth_plane = h.truth_plane
   AND p.association_plane = h.association_plane
   AND p.release = h.release
   AND p.candidate_bbl = h.candidate_bbl
  GROUP BY h.subject_id, h.truth_plane, h.association_plane, h.release
),
candidate_counts AS (
  SELECT
    candidate_selector,
    subject_id,
    truth_plane,
    association_plane,
    release,
    COUNT(DISTINCT candidate_bbl) AS candidate_bbl_count
  FROM candidate_members
  GROUP BY candidate_selector, subject_id, truth_plane, association_plane,
    release
),
selector_release_counts AS (
  SELECT
    candidate_selector,
    release,
    COUNT(DISTINCT subject_id) AS selector_release_subjects
  FROM selector_subject_releases
  GROUP BY candidate_selector, release
),
truth_edges AS (
  SELECT
    sr.candidate_selector,
    sr.subject_id,
    sr.loan_key,
    sr.truth_plane,
    sr.association_plane,
    sr.release,
    truth.value::TEXT AS truth_bbl
  FROM selector_subject_releases sr,
    LATERAL FLATTEN(input => sr.truth_bbls) truth
),
truth_hits AS (
  SELECT
    t.candidate_selector,
    t.subject_id,
    t.truth_plane,
    t.association_plane,
    t.release,
    COUNT(DISTINCT t.truth_bbl) AS truth_bbl_count,
    COUNT(DISTINCT IFF(c.candidate_bbl IS NOT NULL, t.truth_bbl, NULL))
      AS reached_truth_bbls
  FROM truth_edges t
  LEFT JOIN candidate_members c
    ON c.candidate_selector = t.candidate_selector
   AND c.subject_id = t.subject_id
   AND c.truth_plane = t.truth_plane
   AND c.association_plane = t.association_plane
   AND c.release = t.release
   AND c.candidate_bbl = t.truth_bbl
  GROUP BY t.candidate_selector, t.subject_id, t.truth_plane,
    t.association_plane, t.release
),
subject_reach AS (
  SELECT
    sr.selector_order,
    sr.candidate_selector,
    sr.candidate_strategy,
    sr.selector_role,
    sr.subject_id,
    sr.loan_key,
    sr.truth_plane,
    sr.association_plane,
    sr.release,
    sr.release_dt,
    sr.variant,
    sr.truth_bbl_count AS declared_truth_bbl_count,
    h.truth_bbl_count,
    h.reached_truth_bbls,
    COALESCE(c.candidate_bbl_count, 0) AS candidate_bbl_count,
    COALESCE(h3.h3_candidate_bbls, 0) AS h3_candidate_bbl_count,
    COALESCE(pip.pip_candidate_bbls, 0) AS pip_candidate_bbl_count,
    COALESCE(o.h3_pip_overlap_bbls, 0) AS h3_pip_overlap_bbl_count,
    CASE
      WHEN h.reached_truth_bbls = h.truth_bbl_count THEN 'full'
      WHEN h.reached_truth_bbls = 0 THEN 'none'
      ELSE 'partial'
    END AS reach_status
  FROM selector_subject_releases sr
  JOIN truth_hits h
    ON h.candidate_selector = sr.candidate_selector
   AND h.subject_id = sr.subject_id
   AND h.truth_plane = sr.truth_plane
   AND h.association_plane = sr.association_plane
   AND h.release = sr.release
  LEFT JOIN candidate_counts c
    ON c.candidate_selector = sr.candidate_selector
   AND c.subject_id = sr.subject_id
   AND c.truth_plane = sr.truth_plane
   AND c.association_plane = sr.association_plane
   AND c.release = sr.release
  LEFT JOIN h3_candidate_counts h3
    ON h3.subject_id = sr.subject_id
   AND h3.truth_plane = sr.truth_plane
   AND h3.association_plane = sr.association_plane
   AND h3.release = sr.release
  LEFT JOIN pip_candidate_counts pip
    ON pip.subject_id = sr.subject_id
   AND pip.truth_plane = sr.truth_plane
   AND pip.association_plane = sr.association_plane
   AND pip.release = sr.release
  LEFT JOIN h3_pip_overlap_counts o
    ON o.subject_id = sr.subject_id
   AND o.truth_plane = sr.truth_plane
   AND o.association_plane = sr.association_plane
   AND o.release = sr.release
)
SELECT
  (SELECT row_contract FROM params) AS row_contract,
  g.guard_status,
  g.refusal_reason,
  (SELECT bridge_build_id FROM params) AS bridge_build_id,
  (SELECT acris_release_dt FROM params) AS acris_release_dt,
  (SELECT property_state FROM params) AS property_state,
  (SELECT collateral_scope FROM params) AS collateral_scope,
  (SELECT h3_resolution FROM params) AS h3_resolution,
  (SELECT halo_k FROM params) AS halo_k,
  r.candidate_selector,
  r.candidate_strategy,
  r.selector_role,
  r.truth_plane,
  r.association_plane,
  r.release,
  r.release_dt,
  r.variant,
  (SELECT accepted_subjects FROM accepted_subject_stats)
    AS global_accepted_subjects,
  sc.selector_release_subjects,
  COUNT(*) AS accepted_subjects,
  COUNT(DISTINCT r.subject_id) AS distinct_subjects,
  COUNT_IF(r.candidate_bbl_count > 0) AS candidate_reached_subjects,
  COUNT_IF(r.candidate_bbl_count = 0) AS no_candidate_subjects,
  COUNT_IF(r.reach_status = 'full') AS full_reach_subjects,
  COUNT_IF(r.reach_status = 'partial') AS partial_reach_subjects,
  COUNT_IF(r.reach_status = 'none') AS no_reach_subjects,
  SUM(r.truth_bbl_count) AS truth_bbl_edges,
  SUM(r.reached_truth_bbls) AS reached_truth_bbl_edges,
  MIN(r.candidate_bbl_count) AS min_candidate_bbls,
  MEDIAN(r.candidate_bbl_count) AS median_candidate_bbls,
  PERCENTILE_CONT(0.9) WITHIN GROUP (ORDER BY r.candidate_bbl_count)
    AS p90_candidate_bbls,
  MAX(r.candidate_bbl_count) AS max_candidate_bbls,
  SUM(r.candidate_bbl_count) AS candidate_bbl_edges,
  SUM(r.h3_candidate_bbl_count) AS h3_candidate_bbl_edges,
  SUM(r.pip_candidate_bbl_count) AS pip_candidate_bbl_edges,
  SUM(r.h3_pip_overlap_bbl_count) AS h3_pip_overlap_bbl_edges,
  SUM(IFF(
    r.candidate_selector = 'union_cascade_h3_then_pip_block',
    r.h3_pip_overlap_bbl_count,
    IFF(r.candidate_bbl_count > 0, r.h3_pip_overlap_bbl_count, 0)
  )) AS overlap_candidate_bbl_edges,
  COUNT_IF(r.reached_truth_bbls > r.truth_bbl_count)
    AS reach_accounting_failures,
  COUNT_IF(r.reached_truth_bbls > r.candidate_bbl_count)
    AS reached_exceeds_candidate_failures,
  COUNT_IF(r.truth_bbl_count <> r.declared_truth_bbl_count)
    AS truth_count_mismatch_subjects,
  COUNT_IF(
    r.candidate_selector = 'union_cascade_h3_then_pip_block'
    AND r.candidate_bbl_count <>
      r.h3_candidate_bbl_count + r.pip_candidate_bbl_count
        - r.h3_pip_overlap_bbl_count
  ) AS union_cardinality_failures,
  COUNT_IF(r.h3_pip_overlap_bbl_count > r.h3_candidate_bbl_count
    OR r.h3_pip_overlap_bbl_count > r.pip_candidate_bbl_count)
    AS overlap_accounting_failures,
  sc.selector_release_subjects = (SELECT accepted_subjects
                                  FROM accepted_subject_stats)
    AS selector_denominator_guard,
  COUNT(*) = COUNT(DISTINCT r.subject_id)
    AS distinct_membership_guard,
  COUNT_IF(r.reach_status = 'full')
    + COUNT_IF(r.reach_status = 'partial')
    + COUNT_IF(r.reach_status = 'none') = COUNT(*)
    AS reach_partition_guard,
  COUNT_IF(r.reached_truth_bbls > r.truth_bbl_count) = 0
    AS reached_lte_truth_guard,
  COUNT_IF(r.reached_truth_bbls > r.candidate_bbl_count) = 0
    AS reached_lte_candidate_guard,
  COUNT_IF(
    r.candidate_selector = 'h3_r8_k1'
      AND r.candidate_bbl_count <> r.h3_candidate_bbl_count
  ) + COUNT_IF(
    r.candidate_selector = 'pip_six_digit_bbl_block'
      AND r.candidate_bbl_count <> r.pip_candidate_bbl_count
  ) + COUNT_IF(
    r.candidate_selector = 'union_cascade_h3_then_pip_block'
      AND r.candidate_bbl_count <>
        r.h3_candidate_bbl_count + r.pip_candidate_bbl_count
          - r.h3_pip_overlap_bbl_count
  ) = 0 AS selector_cardinality_guard,
  COUNT(*) > 0 AND SUM(r.truth_bbl_count) > 0
    AS complete_denominator_guard
FROM subject_reach r
JOIN selector_release_counts sc
  ON sc.candidate_selector = r.candidate_selector
 AND sc.release = r.release
CROSS JOIN guard_summary g
WHERE g.guard_status = 'ok'
GROUP BY
  g.guard_status,
  g.refusal_reason,
  r.selector_order,
  r.candidate_selector,
  r.candidate_strategy,
  r.selector_role,
  r.truth_plane,
  r.association_plane,
  r.release,
  r.release_dt,
  r.variant,
  sc.selector_release_subjects
UNION ALL

SELECT
  (SELECT row_contract FROM params) AS row_contract,
  (SELECT guard_status FROM guard_summary) AS guard_status,
  (SELECT refusal_reason FROM guard_summary) AS refusal_reason,
  (SELECT bridge_build_id FROM params) AS bridge_build_id,
  (SELECT acris_release_dt FROM params) AS acris_release_dt,
  (SELECT property_state FROM params) AS property_state,
  (SELECT collateral_scope FROM params) AS collateral_scope,
  (SELECT h3_resolution FROM params) AS h3_resolution,
  (SELECT halo_k FROM params) AS halo_k,
  NULL::TEXT AS candidate_selector,
  NULL::TEXT AS candidate_strategy,
  NULL::TEXT AS selector_role,
  'guard_failure'::TEXT AS truth_plane,
  NULL::TEXT AS association_plane,
  NULL::TEXT AS release,
  NULL::DATE AS release_dt,
  NULL::TEXT AS variant,
  0::NUMBER(38,0) AS global_accepted_subjects,
  0::NUMBER(38,0) AS selector_release_subjects,
  0::NUMBER(38,0) AS accepted_subjects,
  0::NUMBER(38,0) AS distinct_subjects,
  0::NUMBER(38,0) AS candidate_reached_subjects,
  0::NUMBER(38,0) AS no_candidate_subjects,
  0::NUMBER(38,0) AS full_reach_subjects,
  0::NUMBER(38,0) AS partial_reach_subjects,
  0::NUMBER(38,0) AS no_reach_subjects,
  0::NUMBER(38,0) AS truth_bbl_edges,
  0::NUMBER(38,0) AS reached_truth_bbl_edges,
  0::NUMBER(38,0) AS min_candidate_bbls,
  0::NUMBER(38,0) AS median_candidate_bbls,
  0::NUMBER(38,0) AS p90_candidate_bbls,
  0::NUMBER(38,0) AS max_candidate_bbls,
  0::NUMBER(38,0) AS candidate_bbl_edges,
  0::NUMBER(38,0) AS h3_candidate_bbl_edges,
  0::NUMBER(38,0) AS pip_candidate_bbl_edges,
  0::NUMBER(38,0) AS h3_pip_overlap_bbl_edges,
  0::NUMBER(38,0) AS overlap_candidate_bbl_edges,
  0::NUMBER(38,0) AS reach_accounting_failures,
  0::NUMBER(38,0) AS reached_exceeds_candidate_failures,
  0::NUMBER(38,0) AS truth_count_mismatch_subjects,
  0::NUMBER(38,0) AS union_cardinality_failures,
  0::NUMBER(38,0) AS overlap_accounting_failures,
  FALSE AS selector_denominator_guard,
  FALSE AS distinct_membership_guard,
  FALSE AS reach_partition_guard,
  FALSE AS reached_lte_truth_guard,
  FALSE AS reached_lte_candidate_guard,
  FALSE AS selector_cardinality_guard,
  FALSE AS complete_denominator_guard
WHERE (SELECT guard_status FROM guard_summary) <> 'ok'
ORDER BY 10, 13, 14, 15;
