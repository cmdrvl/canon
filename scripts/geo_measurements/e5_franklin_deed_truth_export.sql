-- canon geo D7/E5 deed-grain truth source guard and export input.
--
-- This query intentionally returns recorder-source availability metadata until
-- a county-recorder deed/mortgage index lands in EDGAR. Canon consumes only the
-- generic canon_geo_deed_index_rows.v0 row shape after that landing is present.
-- It is a truth-scoring input, not candidate or blocking evidence.

WITH params AS (
    SELECT
        'canon_geo_deed_index_rows.v0'::TEXT AS output_contract,
        'deed_grain_instrument'::TEXT AS truth_plane,
        'fixture_class_not_scored'::TEXT AS absent_proof_class,
        'observed_warehouse_snapshot'::TEXT AS present_proof_class,
        'release/instrument_id'::TEXT AS required_natural_key
),
required_columns AS (
    SELECT column_name
    FROM VALUES
        ('INSTRUMENT_ID'),
        ('INSTRUMENT_TYPE'),
        ('PARCEL_IDS'),
        ('RECORDING_DATE'),
        ('AMOUNT_CENTS'),
        ('LENDER_PARTY_BLAKE3'),
        ('BORROWER_PARTY_BLAKE3'),
        ('SOURCE_RELEASE'),
        ('RELEASE_DT'),
        ('SOURCE_SHA256'),
        ('PARSER_VERSION'),
        ('LICENSE_TERMS'),
        ('ATTRIBUTION_TEXT'),
        ('ROW_BLAKE3')
        AS required(column_name)
),
candidate_tables AS (
    SELECT
        table_catalog,
        table_schema,
        table_name,
        table_type
    FROM edgar_db.information_schema.tables
    WHERE table_schema IN ('SOURCE', 'DBT_STAGING_GEO', 'PROPERTY_MART')
      AND (
          table_name ILIKE '%RECORDER%'
          OR table_name ILIKE '%MORTGAGE%'
          OR table_name ILIKE '%DEED%'
          OR table_name ILIKE '%INSTRUMENT%'
          OR table_name ILIKE '%CONVEY%'
          OR table_name ILIKE '%LIEN%'
      )
      AND (
          table_name ILIKE '%FRANKLIN%'
          OR table_name ILIKE '%COUNTY%'
          OR table_name ILIKE '%OHIO%'
      )
),
candidate_columns AS (
    SELECT
        c.table_catalog,
        c.table_schema,
        c.table_name,
        UPPER(c.column_name) AS column_name
    FROM edgar_db.information_schema.columns c
    INNER JOIN candidate_tables t
        ON c.table_catalog = t.table_catalog
       AND c.table_schema = t.table_schema
       AND c.table_name = t.table_name
),
table_scores AS (
    SELECT
        t.table_catalog,
        t.table_schema,
        t.table_name,
        t.table_type,
        COUNT_IF(rc.column_name IS NOT NULL) AS required_columns_present,
        ARRAY_AGG(rc.column_name) WITHIN GROUP (ORDER BY rc.column_name) AS present_required_columns
    FROM candidate_tables t
    LEFT JOIN candidate_columns c
        ON t.table_catalog = c.table_catalog
       AND t.table_schema = c.table_schema
       AND t.table_name = c.table_name
    LEFT JOIN required_columns rc
        ON c.column_name = rc.column_name
    GROUP BY
        t.table_catalog,
        t.table_schema,
        t.table_name,
        t.table_type
),
best_table AS (
    SELECT *
    FROM table_scores
    QUALIFY ROW_NUMBER() OVER (
        ORDER BY required_columns_present DESC, table_schema, table_name
    ) = 1
),
missing_columns AS (
    SELECT rc.column_name
    FROM required_columns rc
    LEFT JOIN best_table bt
        ON ARRAY_CONTAINS(rc.column_name::VARIANT, bt.present_required_columns)
    WHERE bt.table_name IS NULL
       OR bt.required_columns_present < (SELECT COUNT(*) FROM required_columns)
       OR NOT ARRAY_CONTAINS(rc.column_name::VARIANT, bt.present_required_columns)
)
SELECT
    p.output_contract,
    p.truth_plane,
    CASE
        WHEN bt.table_name IS NULL THEN p.absent_proof_class
        WHEN bt.required_columns_present = (SELECT COUNT(*) FROM required_columns)
            THEN p.present_proof_class
        ELSE p.absent_proof_class
    END AS proof_class,
    CASE
        WHEN bt.table_name IS NULL THEN 'recorder_source_not_landed'
        WHEN bt.required_columns_present = (SELECT COUNT(*) FROM required_columns)
            THEN 'recorder_source_shape_available'
        ELSE 'recorder_source_shape_incomplete'
    END AS recorder_source_status,
    CASE
        WHEN bt.table_name IS NULL THEN NULL
        ELSE bt.table_catalog || '.' || bt.table_schema || '.' || bt.table_name
    END AS source_table,
    p.required_natural_key AS natural_key,
    1::NUMBER AS measurement_guard_rows,
    (SELECT COUNT(*) FROM required_columns)::NUMBER AS required_column_count,
    COALESCE(bt.required_columns_present, 0)::NUMBER AS present_required_column_count,
    (SELECT COUNT(*) FROM table_scores)::NUMBER AS candidate_table_count,
    6::NUMBER AS source_pin_field_count,
    'SOURCE_RELEASE,RELEASE_DT,SOURCE_SHA256,PARSER_VERSION,LICENSE_TERMS,ATTRIBUTION_TEXT'::TEXT
        AS required_source_pin_fields,
    COALESCE(
        LISTAGG(mc.column_name, ',') WITHIN GROUP (ORDER BY mc.column_name),
        ''
    ) AS missing_columns,
    COALESCE(
        (
            SELECT LISTAGG(table_schema || '.' || table_name || ':' || required_columns_present, ',')
                WITHIN GROUP (ORDER BY required_columns_present DESC, table_schema, table_name)
            FROM table_scores
        ),
        ''
    ) AS candidate_tables,
    'truth_plane_only_not_candidate_evidence'::TEXT AS truth_scoring_role,
    'unique_plus_non_unique_discarded_plus_no_match_equals_loans'::TEXT AS denominator_policy,
    'mortgage_instrument_exact_amount_recording_window_unique_required'::TEXT AS match_policy
FROM (SELECT 1 AS singleton) seed
CROSS JOIN params p
LEFT JOIN best_table bt
    ON TRUE
LEFT JOIN missing_columns mc
    ON TRUE
GROUP BY
    p.output_contract,
    p.truth_plane,
    p.absent_proof_class,
    p.present_proof_class,
    p.required_natural_key,
    bt.table_catalog,
    bt.table_schema,
    bt.table_name,
    bt.table_type,
    bt.required_columns_present,
    bt.present_required_columns;
