#![forbid(unsafe_code)]

#[path = "entity/apply_streaming.rs"]
mod apply_streaming;
#[path = "entity/postings_layout.rs"]
mod postings_layout;
#[path = "entity/prepare_artifact.rs"]
mod prepare_artifact;
#[path = "entity/prepare_dedupe.rs"]
mod prepare_dedupe;
#[path = "entity/prepare_exact_lookup.rs"]
mod prepare_exact_lookup;
#[path = "entity/prepare_streaming.rs"]
mod prepare_streaming;
#[path = "entity/surface_id.rs"]
mod surface_id;

#[path = "entity/profile_regab.rs"]
mod profile_regab;

#[path = "entity/cmbs_normalization.rs"]
mod cmbs_normalization;
#[path = "entity/cmbs_tenant_id_allocator.rs"]
mod cmbs_tenant_id_allocator;

#[path = "entity/regab_normalization.rs"]
mod regab_normalization;
