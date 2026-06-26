#![forbid(unsafe_code)]

//! Built-in `canon entity profile` templates.
//!
//! These templates are operator starting points, not hidden strategy logic.
//! They are validated through the normal profile schema before being listed or
//! written so `profile init` cannot ship stale YAML.

use crate::{
    CanonOutput, RefusalCode,
    entity::{error::EntityRefusalKind, profile::EntityProfileDocument},
    refusal,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::Path};

pub const ENTITY_PROFILE_TEMPLATE_CATALOG_VERSION: &str = "canon_entity_profile_templates.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileTemplateCatalog {
    pub version: String,
    pub profiles: Vec<EntityProfileTemplateSummary>,
}

impl EntityProfileTemplateCatalog {
    pub fn render_summary(&self) -> String {
        let mut lines = Vec::with_capacity(self.profiles.len() + 1);
        lines.push("Built-in entity profile templates:".to_string());
        for profile in &self.profiles {
            lines.push(format!(
                "- {} ({}) -> {} [{}]",
                profile.profile,
                profile.identity_semantics,
                profile.canonical_type,
                profile.init_command
            ));
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileTemplateSummary {
    pub profile: String,
    pub version: String,
    pub entity_type: String,
    pub identity_semantics: String,
    pub canonical_type: String,
    pub required_fields: Vec<String>,
    pub patch_namespaces: Vec<String>,
    pub non_goals: Vec<String>,
    pub init_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileTemplateInitOutput {
    pub version: String,
    pub profile: String,
    pub output: String,
    pub bytes_written: u64,
    pub template_valid: bool,
    pub next_command: String,
}

#[allow(clippy::result_large_err)]
pub fn list_profile_templates() -> Result<EntityProfileTemplateCatalog, CanonOutput> {
    let profiles = profile_templates()
        .iter()
        .map(|template| template.summary())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EntityProfileTemplateCatalog {
        version: ENTITY_PROFILE_TEMPLATE_CATALOG_VERSION.to_string(),
        profiles,
    })
}

#[allow(clippy::result_large_err)]
pub fn init_profile_template(
    profile_id: &str,
    output: &Path,
) -> Result<EntityProfileTemplateInitOutput, CanonOutput> {
    let template = find_profile_template(profile_id)?;
    template.validate()?;
    fs::write(output, template.yaml).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EIo,
            "Failed to write entity profile template".to_string(),
            json!({
                "profile": template.profile,
                "output": output.display().to_string(),
                "error": error.to_string()
            }),
            Some("Choose a writable --output path and rerun canon entity profile init".to_string()),
        )
    })?;

    Ok(EntityProfileTemplateInitOutput {
        version: ENTITY_PROFILE_TEMPLATE_CATALOG_VERSION.to_string(),
        profile: template.profile.to_string(),
        output: output.display().to_string(),
        bytes_written: template.yaml.len() as u64,
        template_valid: true,
        next_command: format!(
            "canon entity prepare <ROWS> --profile {} --registry <REGISTRY> --work-dir <WORK_DIR>",
            template.profile
        ),
    })
}

#[allow(clippy::result_large_err)]
pub fn profile_template_yaml(profile_id: &str) -> Result<&'static str, CanonOutput> {
    let template = find_profile_template(profile_id)?;
    template.validate()?;
    Ok(template.yaml)
}

#[allow(clippy::result_large_err)]
fn find_profile_template(profile_id: &str) -> Result<&'static ProfileTemplate, CanonOutput> {
    profile_templates()
        .iter()
        .find(|template| template.profile == profile_id)
        .ok_or_else(|| unknown_profile_template_refusal(profile_id))
}

fn profile_templates() -> &'static [ProfileTemplate] {
    &[CMBS_TENANT_LABEL_TEMPLATE, REGAB_FIRM_IDENTITY_TEMPLATE]
}

#[derive(Debug, Clone, Copy)]
struct ProfileTemplate {
    profile: &'static str,
    non_goals: &'static [&'static str],
    yaml: &'static str,
}

impl ProfileTemplate {
    #[allow(clippy::result_large_err)]
    fn validate(self) -> Result<EntityProfileDocument, CanonOutput> {
        EntityProfileDocument::from_yaml_str(self.yaml).map_err(|error| {
            EntityRefusalKind::Profile
                .to_refusal(
                    "Built-in entity profile template is invalid",
                    json!({
                        "profile": self.profile,
                        "error": error.message,
                        "detail": error.detail
                    }),
                    Some(
                        "Report the built-in template bug and avoid hand-editing around it"
                            .to_string(),
                    ),
                )
                .to_canon_output()
        })
    }

    #[allow(clippy::result_large_err)]
    fn summary(self) -> Result<EntityProfileTemplateSummary, CanonOutput> {
        let profile = self.validate()?;
        Ok(EntityProfileTemplateSummary {
            profile: profile.profile.clone(),
            version: profile.version,
            entity_type: profile.entity_type,
            identity_semantics: profile.identity_semantics,
            canonical_type: profile.canonical_type,
            required_fields: profile.required_fields,
            patch_namespaces: vec![
                profile.patch_namespaces.aliases,
                profile.patch_namespaces.distinct,
                profile.patch_namespaces.relations,
            ],
            non_goals: self
                .non_goals
                .iter()
                .map(|goal| (*goal).to_string())
                .collect(),
            init_command: format!(
                "canon entity profile init {} --output {}.yaml",
                self.profile, self.profile
            ),
        })
    }
}

fn unknown_profile_template_refusal(profile_id: &str) -> CanonOutput {
    let available_profiles = profile_templates()
        .iter()
        .map(|template| template.profile)
        .collect::<Vec<_>>();
    EntityRefusalKind::Profile
        .to_refusal(
            "Unknown entity profile template",
            json!({
                "profile": profile_id,
                "available_profiles": available_profiles
            }),
            Some("canon entity profile list --emit json".to_string()),
        )
        .to_canon_output()
}

const CMBS_TENANT_LABEL_TEMPLATE: ProfileTemplate = ProfileTemplate {
    profile: "cmbs_tenant_label",
    non_goals: &[
        "does_not_claim_legal_entity_identity",
        "does_not_merge_brand_family_or_successor_relationships",
    ],
    yaml: r#"# canon entity profile template: cmbs_tenant_label
# Identity semantics: canonical display label for CMBS tenant strings.
# Non-goal: this does not claim legal-entity, obligor, investor, owner,
# brand-hierarchy, or successor identity.
profile: cmbs_tenant_label
version: 0.1.0
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
required_fields:
  - source_row_id
  - deal_id
  - loan_id
  - property_id
  - raw_tenant_name
normalized_views:
  tenant_core:
    operators:
      - unicode_fold
      - lowercase
      - strip_tenant_noise
      - strip_legal_suffixes
      - normalize_whitespace
  tenant_tokens:
    operators:
      - unicode_fold
      - lowercase
      - tokenize
      - drop_tenant_stopwords
  tenant_brand:
    operators:
      - unicode_fold
      - lowercase
      - tenant_brand_fingerprint
      - normalize_whitespace
evidence:
  support:
    - op: exact_view
      view: tenant_core
    - op: string_similarity
      view: tenant_core
    - op: tfidf_cosine
      view: tenant_tokens
    - op: alias_patch_match
      view: tenant_core
  cannot_link:
    - op: protected_token_conflict
      view: tenant_tokens
    - op: related_distinct_phrase
      view: tenant_core
    - op: same_property_distinct_rank
  relation_hints:
    - op: related_brand_family
      view: tenant_brand
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
    - op: possible_successor_predecessor
      view: tenant_brand
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
    - op: cross_profile_alignment
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: cmbs_tenant_label.aliases
  distinct: cmbs_tenant_label.distinct
  relations: cmbs_tenant_label.relations
"#,
};

const REGAB_FIRM_IDENTITY_TEMPLATE: ProfileTemplate = ProfileTemplate {
    profile: "regab_firm_identity",
    non_goals: &[
        "does_not_merge_parent_subsidiary_or_division_boundaries",
        "does_not_mutate_sec10d_parser_fields",
    ],
    yaml: r#"# canon entity profile template: regab_firm_identity
# Identity semantics: reviewed firm identity / firm alias canonicalization.
# Non-goal: this does not collapse parent/subsidiary, bank/division,
# platform/category labels, or person/certifying-party fields by default.
profile: regab_firm_identity
version: 0.1.0
entity_type: organization
identity_semantics: same_firm_or_reviewed_alias
canonical_type: org
required_fields:
  - source_row_id
  - field_name
  - org_name
  - dataset
normalized_views:
  firm_core:
    operators:
      - unicode_fold
      - lowercase
      - expand_na_abbreviation
      - preserve_legal_form
      - normalize_whitespace
  firm_tokens:
    operators:
      - unicode_fold
      - lowercase
      - tokenize
evidence:
  support:
    - op: exact_view
      view: firm_core
    - op: reviewed_alias
      view: firm_core
  cannot_link:
    - op: role_conflict
    - op: platform_label_guard
    - op: division_boundary
  relation_hints:
    - op: division_of
    - op: parent_subsidiary_context
patch_namespaces:
  aliases: regab_firm_identity.aliases
  distinct: regab_firm_identity.distinct
  relations: regab_firm_identity.relations
"#,
};
