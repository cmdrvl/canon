#![forbid(unsafe_code)]

#[path = "entity/apply_streaming.rs"]
mod apply_streaming;
#[path = "entity/bead_parallelism_contract.rs"]
mod bead_parallelism_contract;
#[path = "entity/block_artifact.rs"]
mod block_artifact;
#[path = "entity/block_candidates.rs"]
mod block_candidates;
#[path = "entity/block_diagnostics.rs"]
mod block_diagnostics;
#[path = "entity/block_exact_bucket.rs"]
mod block_exact_bucket;
#[path = "entity/block_exact_bucket_hyperedge.rs"]
mod block_exact_bucket_hyperedge;
#[path = "entity/blocking_golden.rs"]
mod blocking_golden;
#[path = "entity/cache_reload.rs"]
mod cache_reload;
#[path = "entity/index_fixture_support.rs"]
mod index_fixture_support;
#[path = "entity/index_golden.rs"]
mod index_golden;
#[path = "entity/index_io.rs"]
mod index_io;
#[path = "entity/index_ngram.rs"]
mod index_ngram;
#[path = "entity/postings_layout.rs"]
mod postings_layout;
#[path = "entity/prepare_artifact.rs"]
mod prepare_artifact;
#[path = "entity/prepare_dedupe.rs"]
mod prepare_dedupe;
#[path = "entity/prepare_exact_lookup.rs"]
mod prepare_exact_lookup;
#[path = "entity/prepare_golden.rs"]
mod prepare_golden;
#[path = "entity/prepare_streaming.rs"]
mod prepare_streaming;
#[path = "entity/surface_id.rs"]
mod surface_id;

#[path = "entity/profile_regab.rs"]
mod profile_regab;

#[path = "entity/cmbs_hard_negatives.rs"]
mod cmbs_hard_negatives;
#[path = "entity/cmbs_normalization.rs"]
mod cmbs_normalization;
#[path = "entity/cmbs_tenant_id_allocator.rs"]
mod cmbs_tenant_id_allocator;

#[path = "entity/edge_cannot_link.rs"]
mod edge_cannot_link;
#[path = "entity/edge_relation_hints.rs"]
mod edge_relation_hints;
#[path = "entity/edge_score_units.rs"]
mod edge_score_units;
#[path = "entity/edge_support.rs"]
mod edge_support;
#[path = "entity/edge_tfidf.rs"]
mod edge_tfidf;

#[path = "entity/regab_normalization.rs"]
mod regab_normalization;
#[path = "entity/solve_budget.rs"]
mod solve_budget;
