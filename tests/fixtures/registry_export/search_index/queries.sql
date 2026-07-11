-- query: exact_alias_lookup
SELECT canonical_id, canonical_iri, rule_id, source_file
FROM aliases
WHERE alias = 'Alpha Fund II';

-- query: serving_normalization_collision
SELECT normalized_key, CAST(COUNT(*) AS TEXT), CAST(COUNT(DISTINCT canonical_id) AS TEXT)
FROM aliases
WHERE normalized_key = 'ACME'
GROUP BY normalized_key;

-- query: package_metadata
SELECT key, value
FROM metadata
WHERE key IN (
  'cache_policy',
  'registry_package_digest',
  'registry_package_id',
  'registry_package_schema_version',
  'registry_package_version'
)
ORDER BY key;

-- query: capabilities
SELECT capability, CAST(enabled AS TEXT)
FROM capabilities
WHERE capability IN (
  'exact_alias_lookup',
  'mutable_internal_cache',
  'registry_package_trace',
  'standalone_export'
)
ORDER BY capability;
