select
  namespace,
  normalized_key,
  count(*) as source_alias_count,
  count(distinct canonical_id) as canonical_id_count
from {{ ref('canon_registry_seed') }}
group by 1, 2
having count(distinct canonical_id) > 1
