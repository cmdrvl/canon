//! Profile-specific entity workbench helpers.

pub mod cmbs;
pub mod regab;

pub const BUILTIN_PROFILES: &[(&str, &str)] = &[
    (
        "cmbs_tenant_label",
        include_str!("../../../tests/fixtures/entity/profiles/cmbs_tenant_label.yaml"),
    ),
    (
        "regab_firm_identity",
        include_str!("../../../tests/fixtures/entity/profiles/regab_firm_identity.yaml"),
    ),
    (
        "instrument_identity",
        include_str!("../../../tests/fixtures/entity/profiles/instrument_identity.yaml"),
    ),
];

pub fn builtin_source(id: &str) -> Option<&'static str> {
    BUILTIN_PROFILES
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, source)| *source)
}

/// A single declared input field can be used as an apply lookup column.
/// Composite primary surfaces require an explicit column from the caller.
pub fn builtin_primary_surface_field(id: &str) -> Option<String> {
    let profile =
        crate::entity::profile::EntityProfileDocument::from_yaml_str(builtin_source(id)?).ok()?;
    let mut fields = profile.prepare?.primary_surface_fields;
    (fields.len() == 1).then(|| fields.remove(0))
}

use crate::entity::profile_cli::ProfileTemplate;
pub(crate) const TEMPLATES: &[ProfileTemplate] = &[
    ProfileTemplate {
        profile: BUILTIN_PROFILES[0].0,
        yaml: BUILTIN_PROFILES[0].1,
        non_goals: &[
            "does_not_claim_legal_entity_identity",
            "does_not_merge_brand_family_or_successor_relationships",
        ],
    },
    ProfileTemplate {
        profile: BUILTIN_PROFILES[1].0,
        yaml: BUILTIN_PROFILES[1].1,
        non_goals: &[
            "does_not_merge_parent_subsidiary_or_division_boundaries",
            "does_not_mutate_sec10d_parser_fields",
        ],
    },
    ProfileTemplate {
        profile: BUILTIN_PROFILES[2].0,
        yaml: BUILTIN_PROFILES[2].1,
        non_goals: &[
            "does_not_claim_issuer_legal_entity_identity",
            "does_not_merge_on_shared_issuer_lei",
            "does_not_collapse_share_classes",
            "does_not_claim_live_source_coverage",
        ],
    },
];
