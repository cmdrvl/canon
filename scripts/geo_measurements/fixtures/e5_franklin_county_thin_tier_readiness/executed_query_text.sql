-- Contract fixture for the cmdrvl-data normalized executed-query-text artifact.
-- This is not independent Snowflake QUERY_HISTORY proof.
SELECT
  'e5_franklin_county_thin_tier_readiness_v0' AS measurement_id,
  '01c6c151-0821-a0dc-006c-c703088daaba' AS query_id,
  'receipt_consistent_only_liveness_not_attested' AS proof_boundary
LIMIT 100000;
