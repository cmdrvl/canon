//! Entity profile schema and validation contracts.
//!
//! Profiles define identity semantics before any entity stage runs. This file
//! validates profile declarations and strategy/profile compatibility; it does
//! not perform matching, candidate generation, or promotion.

use crate::Refusal;
use crate::entity::{
    EntityContractKind, EntityGovernanceContractSlice, EntityPatchNamespaces,
    EntityProfileReference, EntityTypedReference,
    error::EntityRefusalKind,
    prepare::PrepareFieldMapping,
    profile_package::{self, EntityProfilePackage as ExternalEntityProfilePackage},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_yaml::Value as YamlValue;
use std::collections::{BTreeMap, BTreeSet};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityProfileDocument {
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub entity_type: String,
    #[serde(default)]
    pub identity_semantics: String,
    #[serde(default)]
    pub canonical_type: String,
    #[serde(default)]
    pub required_fields: Vec<String>,
    #[serde(default)]
    pub normalized_views: BTreeMap<String, EntityNormalizedView>,
    #[serde(default)]
    pub evidence: EntityEvidenceLanes,
    #[serde(default)]
    pub patch_namespaces: EntityPatchNamespaces,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityProfilePackageProjection {
    pub document: EntityProfileDocument,
    pub prepare_mapping: PrepareFieldMapping,
    pub package_digest: String,
    pub package: ExternalEntityProfilePackage,
}

impl EntityProfileDocument {
    pub fn from_yaml_str(input: &str) -> Result<Self, EntityProfileError> {
        let value = serde_yaml::from_str::<YamlValue>(input).map_err(|error| {
            EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile YAML is malformed",
                json!({ "error": error.to_string() }),
            )
        })?;
        ensure_entity_profile_kind(&value)?;
        let profile = serde_yaml::from_value::<Self>(value).map_err(|error| {
            EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile YAML is malformed",
                json!({ "error": error.to_string() }),
            )
        })?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), EntityProfileError> {
        let mut missing = Vec::new();
        for (field, value) in [
            ("profile", self.profile.as_str()),
            ("version", self.version.as_str()),
            ("entity_type", self.entity_type.as_str()),
            ("identity_semantics", self.identity_semantics.as_str()),
            ("canonical_type", self.canonical_type.as_str()),
        ] {
            if value.trim().is_empty() {
                missing.push(field);
            }
        }

        if self.required_fields.is_empty() {
            missing.push("required_fields");
        }
        if self.normalized_views.is_empty() {
            missing.push("normalized_views");
        }
        if self.evidence.support.is_empty() {
            missing.push("evidence.support");
        }
        if self.evidence.cannot_link.is_empty() {
            missing.push("evidence.cannot_link");
        }
        if self.evidence.relation_hints.is_empty() {
            missing.push("evidence.relation_hints");
        }
        if !self.patch_namespaces.is_complete() {
            missing.push("patch_namespaces");
        }

        if !missing.is_empty() {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile is missing required identity-semantics fields",
                json!({ "missing": missing }),
            ));
        }

        ensure_unique_non_empty("required_fields", &self.required_fields)?;
        self.validate_patch_namespaces()?;
        self.validate_normalized_views()?;
        self.validate_evidence()?;
        Ok(())
    }

    pub fn to_reference(&self) -> EntityProfileReference {
        EntityProfileReference {
            id: self.profile.clone(),
            version: self.version.clone(),
            entity_type: self.entity_type.clone(),
            identity_semantics: self.identity_semantics.clone(),
            canonical_type: self.canonical_type.clone(),
            patch_namespaces: self.patch_namespaces.clone(),
            content_hash: None,
        }
    }

    pub fn validate_matches(
        &self,
        expected: &EntityProfileReference,
    ) -> Result<(), EntityProfileError> {
        let actual = self.to_reference();
        let mismatches = [
            ("profile", actual.id.as_str(), expected.id.as_str()),
            (
                "version",
                actual.version.as_str(),
                expected.version.as_str(),
            ),
            (
                "entity_type",
                actual.entity_type.as_str(),
                expected.entity_type.as_str(),
            ),
            (
                "identity_semantics",
                actual.identity_semantics.as_str(),
                expected.identity_semantics.as_str(),
            ),
            (
                "canonical_type",
                actual.canonical_type.as_str(),
                expected.canonical_type.as_str(),
            ),
            (
                "patch_namespaces.aliases",
                actual.patch_namespaces.aliases.as_str(),
                expected.patch_namespaces.aliases.as_str(),
            ),
            (
                "patch_namespaces.distinct",
                actual.patch_namespaces.distinct.as_str(),
                expected.patch_namespaces.distinct.as_str(),
            ),
            (
                "patch_namespaces.relations",
                actual.patch_namespaces.relations.as_str(),
                expected.patch_namespaces.relations.as_str(),
            ),
        ]
        .into_iter()
        .filter_map(|(field, actual, expected)| {
            (actual != expected).then_some(json!({
                "field": field,
                "actual": actual,
                "expected": expected
            }))
        })
        .collect::<Vec<_>>();

        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity strategy/profile mismatch",
                json!({ "mismatches": mismatches }),
            ))
        }
    }

    fn validate_patch_namespaces(&self) -> Result<(), EntityProfileError> {
        let expected_prefix = format!("{}.", self.profile);
        for (field, value) in [
            (
                "patch_namespaces.aliases",
                self.patch_namespaces.aliases.as_str(),
            ),
            (
                "patch_namespaces.distinct",
                self.patch_namespaces.distinct.as_str(),
            ),
            (
                "patch_namespaces.relations",
                self.patch_namespaces.relations.as_str(),
            ),
        ] {
            if !value.starts_with(&expected_prefix) {
                return Err(EntityProfileError::new(
                    EntityRefusalKind::Profile,
                    "Entity patch namespace must be scoped to the profile",
                    json!({
                        "field": field,
                        "profile": self.profile,
                        "expected_prefix": expected_prefix,
                        "actual": value
                    }),
                ));
            }
        }
        Ok(())
    }

    fn validate_normalized_views(&self) -> Result<(), EntityProfileError> {
        for (view_name, view) in &self.normalized_views {
            if view_name.trim().is_empty() {
                return Err(EntityProfileError::new(
                    EntityRefusalKind::Profile,
                    "Entity normalized view names must be non-empty",
                    json!({ "view": view_name }),
                ));
            }
            if view.operators.is_empty() {
                return Err(EntityProfileError::new(
                    EntityRefusalKind::Profile,
                    "Entity normalized views must declare at least one operator",
                    json!({ "view": view_name }),
                ));
            }
            ensure_unique_non_empty(
                &format!("normalized_views.{view_name}.operators"),
                &view.operators,
            )?;
            for op in &view.operators {
                if !SUPPORTED_NORMALIZE_OPS.contains(&op.as_str()) {
                    return Err(unsupported_operator("normalize", op));
                }
            }
        }
        Ok(())
    }

    fn validate_evidence(&self) -> Result<(), EntityProfileError> {
        validate_evidence_lane("support", &self.evidence.support, SUPPORTED_SUPPORT_OPS)?;
        validate_evidence_lane(
            "cannot_link",
            &self.evidence.cannot_link,
            SUPPORTED_CANNOT_LINK_OPS,
        )?;
        validate_evidence_lane(
            "relation_hints",
            &self.evidence.relation_hints,
            SUPPORTED_RELATION_HINT_OPS,
        )?;

        for op in &self.evidence.support {
            if CROSS_PROFILE_RELATION_OPS.contains(&op.op.as_str()) {
                return Err(EntityProfileError::new(
                    EntityRefusalKind::Strategy,
                    "Cross-profile alignment cannot be support evidence",
                    json!({
                        "lane": "support",
                        "operator": op.op,
                        "recovery": "Move cross-profile relationship signals to evidence.relation_hints"
                    }),
                ));
            }
        }
        Ok(())
    }
}

pub fn entity_profile_package_projection_from_file(
    path: &Path,
) -> Result<EntityProfilePackageProjection, EntityProfileError> {
    let bytes = fs::read(path).map_err(|error| {
        EntityProfileError::new(
            EntityRefusalKind::Profile,
            "Entity profile package could not be read",
            json!({
                "path": path.display().to_string(),
                "error": error.to_string()
            }),
        )
    })?;
    entity_profile_package_projection_from_bytes(&bytes)
}

pub fn entity_profile_package_projection_from_bytes(
    bytes: &[u8],
) -> Result<EntityProfilePackageProjection, EntityProfileError> {
    let package = profile_package::load_profile_package_bytes(bytes).map_err(|error| {
        EntityProfileError::new(
            EntityRefusalKind::Profile,
            "Entity profile package is invalid",
            json!({
                "code": format!("{:?}", error.code),
                "message": error.message
            }),
        )
    })?;
    let package_digest =
        profile_package::entity_profile_package_digest(&package).map_err(|error| {
            EntityProfileError::new(
                EntityRefusalKind::ArtifactContract,
                "Entity profile package digest could not be computed",
                json!({
                    "code": format!("{:?}", error.code),
                    "message": error.message
                }),
            )
        })?;
    let document = profile_document_from_package(&package)?;
    let prepare_mapping = prepare_mapping_from_package(&package)?;
    Ok(EntityProfilePackageProjection {
        document,
        prepare_mapping,
        package_digest,
        package,
    })
}

fn profile_document_from_package(
    package: &ExternalEntityProfilePackage,
) -> Result<EntityProfileDocument, EntityProfileError> {
    let document = EntityProfileDocument {
        profile: package.profile.clone(),
        version: package.version.clone(),
        entity_type: package.entity_type.clone(),
        identity_semantics: package.identity_semantics.clone(),
        canonical_type: package.canonical_type.clone(),
        required_fields: package.required_fields.clone(),
        normalized_views: package
            .normalized_views
            .iter()
            .map(|(name, view)| {
                (
                    name.clone(),
                    EntityNormalizedView {
                        operators: view
                            .operators
                            .iter()
                            .map(|operator| operator.op.as_str().to_string())
                            .collect(),
                    },
                )
            })
            .collect(),
        evidence: EntityEvidenceLanes {
            support: package
                .evidence
                .support
                .iter()
                .map(profile_operator_from_package)
                .collect(),
            cannot_link: package
                .evidence
                .cannot_link
                .iter()
                .map(profile_operator_from_package)
                .collect(),
            relation_hints: package
                .evidence
                .relation_hints
                .iter()
                .map(profile_operator_from_package)
                .collect(),
        },
        patch_namespaces: EntityPatchNamespaces {
            aliases: package.patch_namespaces.aliases.clone(),
            distinct: package.patch_namespaces.distinct.clone(),
            relations: package.patch_namespaces.relations.clone(),
        },
    };
    document.validate()?;
    Ok(document)
}

fn profile_operator_from_package(
    operator: &profile_package::EntityOperatorSpec,
) -> EntityOperatorSpec {
    EntityOperatorSpec {
        op: operator.op.clone(),
        view: operator.view.clone(),
        params: operator.params.clone(),
    }
}

fn prepare_mapping_from_package(
    package: &ExternalEntityProfilePackage,
) -> Result<PrepareFieldMapping, EntityProfileError> {
    let mut mapping = PrepareFieldMapping::default();
    let mut seen_primary = BTreeSet::new();
    let mut seen_context = BTreeSet::new();
    let mut seen_provenance = BTreeSet::new();

    for field in &package.field_mappings {
        if field.object_type != package.entity_type {
            continue;
        }
        let field_path = field.field_path.clone();
        match field.field_role.as_str() {
            "canonical_surface" => {
                let Some(normalized_view) = field.normalized_view.as_ref() else {
                    return Err(profile_package_mapping_refusal(
                        "Entity profile package canonical surface must declare a normalized view",
                        json!({
                            "field_role": field.field_role,
                            "field_path": field.field_path
                        }),
                    ));
                };
                if mapping
                    .canonical_surface_normalized_view
                    .replace(normalized_view.clone())
                    .is_some()
                {
                    return Err(profile_package_mapping_refusal(
                        "Entity profile package declares duplicate canonical surface fields",
                        json!({ "field_role": field.field_role }),
                    ));
                }
                if seen_primary.insert(field_path.clone()) {
                    mapping.primary_surface_fields.push(field_path);
                }
            }
            "context_value" => {
                if seen_context.insert(field_path.clone()) {
                    mapping.context_fields.push(field_path);
                }
            }
            "record_key" | "provenance_value" => {
                if seen_provenance.insert(field_path.clone()) {
                    mapping.provenance_fields.push(field_path);
                }
            }
            "alias_surfaces" => {
                if mapping.alias_surfaces_field.replace(field_path).is_some() {
                    return Err(profile_package_mapping_refusal(
                        "Entity profile package declares duplicate alias surface fields",
                        json!({ "field_role": field.field_role }),
                    ));
                }
            }
            "mention_surfaces" => {
                if mapping.mention_surfaces_field.replace(field_path).is_some() {
                    return Err(profile_package_mapping_refusal(
                        "Entity profile package declares duplicate mention surface fields",
                        json!({ "field_role": field.field_role }),
                    ));
                }
            }
            role if role.starts_with("anchor:") || role.starts_with("anchor.") => {
                let namespace = role
                    .split_once(':')
                    .or_else(|| role.split_once('.'))
                    .map(|(_, namespace)| namespace)
                    .unwrap_or_default()
                    .trim();
                if namespace.is_empty() {
                    return Err(profile_package_mapping_refusal(
                        "Entity profile package declares an empty anchor namespace",
                        json!({ "field_role": field.field_role }),
                    ));
                }
                if mapping
                    .anchor_fields
                    .insert(namespace.to_string(), field_path)
                    .is_some()
                {
                    return Err(profile_package_mapping_refusal(
                        "Entity profile package declares duplicate anchor namespace",
                        json!({ "field_role": field.field_role }),
                    ));
                }
            }
            role => {
                return Err(profile_package_mapping_refusal(
                    "Entity profile package declares an unsupported field role",
                    json!({
                        "field_role": role,
                        "field_path": field.field_path
                    }),
                ));
            }
        }
    }

    if mapping.primary_surface_fields.is_empty() {
        return Err(profile_package_mapping_refusal(
            "Entity profile package must declare at least one canonical surface field",
            json!({
                "profile": package.profile,
                "entity_type": package.entity_type
            }),
        ));
    }
    Ok(mapping)
}

fn profile_package_mapping_refusal(
    message: impl Into<String>,
    detail: serde_json::Value,
) -> EntityProfileError {
    EntityProfileError::new(EntityRefusalKind::Profile, message, detail)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileContractEnvelope {
    pub kind: EntityContractKind,
    #[serde(flatten)]
    pub profile: EntityProfileDocument,
    pub evidence_policy: EntityTypedReference,
    pub review_policy: EntityTypedReference,
    pub promotion_policy: EntityTypedReference,
    pub frozen_executable_strategy: EntityTypedReference,
}

impl EntityProfileContractEnvelope {
    pub fn from_yaml_str(input: &str) -> Result<Self, EntityProfileError> {
        let value = serde_yaml::from_str::<YamlValue>(input).map_err(|error| {
            EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile YAML is malformed",
                json!({ "error": error.to_string() }),
            )
        })?;
        ensure_entity_profile_kind(&value)?;
        let envelope = serde_yaml::from_value::<Self>(value).map_err(|error| {
            EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile YAML is malformed",
                json!({ "error": error.to_string() }),
            )
        })?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), EntityProfileError> {
        if self.kind != EntityContractKind::EntityProfile {
            return Err(wrong_contract_kind(
                EntityRefusalKind::Profile,
                EntityContractKind::EntityProfile,
                self.kind,
            ));
        }
        self.profile.validate()?;
        ensure_complete_reference(
            "evidence_policy",
            &self.evidence_policy,
            EntityContractKind::EvidencePolicy,
        )?;
        ensure_complete_reference(
            "frozen_executable_strategy",
            &self.frozen_executable_strategy,
            EntityContractKind::FrozenExecutableStrategy,
        )?;
        EntityGovernanceContractSlice {
            review_policy: self.review_policy.clone(),
            promotion_policy: self.promotion_policy.clone(),
        }
        .validate()
        .map_err(map_typed_contract_error)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityNormalizedView {
    #[serde(default)]
    pub operators: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityEvidenceLanes {
    #[serde(default)]
    pub support: Vec<EntityOperatorSpec>,
    #[serde(default)]
    pub cannot_link: Vec<EntityOperatorSpec>,
    #[serde(default)]
    pub relation_hints: Vec<EntityOperatorSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityOperatorSpec {
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileFirewall {
    pub profile: EntityProfileReference,
    pub strategy_content_hash: String,
    pub registry_snapshot_hash: String,
    pub patch_namespace: String,
}

impl EntityProfileFirewall {
    pub fn new(
        profile: &EntityProfileDocument,
        strategy_content_hash: impl Into<String>,
        registry_snapshot_hash: impl Into<String>,
        patch_namespace: impl Into<String>,
    ) -> Result<Self, EntityProfileError> {
        let firewall = Self {
            profile: profile.to_reference(),
            strategy_content_hash: strategy_content_hash.into(),
            registry_snapshot_hash: registry_snapshot_hash.into(),
            patch_namespace: patch_namespace.into(),
        };
        firewall.validate()?;
        Ok(firewall)
    }

    pub fn validate(&self) -> Result<(), EntityProfileError> {
        let mut missing = Vec::new();
        if self.profile.id.trim().is_empty() {
            missing.push("profile.id");
        }
        if self.profile.version.trim().is_empty() {
            missing.push("profile.version");
        }
        if self.profile.entity_type.trim().is_empty() {
            missing.push("profile.entity_type");
        }
        if self.profile.identity_semantics.trim().is_empty() {
            missing.push("profile.identity_semantics");
        }
        if self.profile.canonical_type.trim().is_empty() {
            missing.push("profile.canonical_type");
        }
        if self.strategy_content_hash.trim().is_empty() {
            missing.push("strategy_content_hash");
        }
        if self.registry_snapshot_hash.trim().is_empty() {
            missing.push("registry_snapshot_hash");
        }
        if self.patch_namespace.trim().is_empty() {
            missing.push("patch_namespace");
        }
        if !self.profile.patch_namespaces.is_complete() {
            missing.push("profile.patch_namespaces");
        }

        if !missing.is_empty() {
            return Err(EntityProfileError::new(
                EntityRefusalKind::ArtifactContract,
                "Entity artifact firewall metadata is incomplete",
                json!({ "missing": missing }),
            ));
        }

        let expected_prefix = format!("{}.", self.profile.id);
        if !self.patch_namespace.starts_with(&expected_prefix) {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity patch namespace must stay inside the profile firewall",
                json!({
                    "profile": self.profile.id,
                    "patch_namespace": self.patch_namespace,
                    "expected_prefix": expected_prefix
                }),
            ));
        }
        if !self
            .profile
            .patch_namespaces
            .matches_profile_root(&self.profile.id)
        {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile patch namespaces must stay inside the profile firewall",
                json!({
                    "profile": self.profile.id,
                    "patch_namespaces": self.profile.patch_namespaces,
                    "expected_prefix": expected_prefix
                }),
            ));
        }

        Ok(())
    }

    pub fn validate_same_as_reuse(
        &self,
        target: &EntityProfileFirewall,
    ) -> Result<(), EntityProfileError> {
        self.validate()?;
        target.validate()?;

        let mismatches = self.scope_mismatches(target);
        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Cross-profile same-as reuse is not allowed",
                json!({
                    "mode": "same_as",
                    "mismatches": mismatches,
                    "recovery": "Emit a relation hint handoff instead of merging scoped IDs"
                }),
            ))
        }
    }

    pub fn relation_handoff(
        &self,
        target: &EntityProfileFirewall,
        relation: impl Into<String>,
    ) -> Result<EntityProfileRelationHandoff, EntityProfileError> {
        self.validate()?;
        target.validate()?;

        let relation = relation.into().trim().to_string();
        if relation.is_empty() || relation == "same_as" || relation == "same-as" {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity relation handoff must not authorize same-as merge",
                json!({ "relation": relation }),
            ));
        }

        Ok(EntityProfileRelationHandoff {
            relation,
            source_profile: self.profile.clone(),
            target_profile: target.profile.clone(),
            source_patch_namespace: self.patch_namespace.clone(),
            target_patch_namespace: target.patch_namespace.clone(),
            merge_authorized: false,
        })
    }

    fn scope_mismatches(&self, target: &EntityProfileFirewall) -> Vec<serde_json::Value> {
        [
            (
                "profile",
                self.profile.id.as_str(),
                target.profile.id.as_str(),
            ),
            (
                "profile_version",
                self.profile.version.as_str(),
                target.profile.version.as_str(),
            ),
            (
                "entity_type",
                self.profile.entity_type.as_str(),
                target.profile.entity_type.as_str(),
            ),
            (
                "identity_semantics",
                self.profile.identity_semantics.as_str(),
                target.profile.identity_semantics.as_str(),
            ),
            (
                "canonical_type",
                self.profile.canonical_type.as_str(),
                target.profile.canonical_type.as_str(),
            ),
            (
                "patch_namespace",
                self.patch_namespace.as_str(),
                target.patch_namespace.as_str(),
            ),
        ]
        .into_iter()
        .filter_map(|(field, source, target)| {
            (source != target).then_some(json!({
                "field": field,
                "source": source,
                "target": target
            }))
        })
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileRelationHandoff {
    pub relation: String,
    pub source_profile: EntityProfileReference,
    pub target_profile: EntityProfileReference,
    pub source_patch_namespace: String,
    pub target_patch_namespace: String,
    pub merge_authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityProfileError {
    pub kind: EntityRefusalKind,
    pub message: String,
    pub detail: serde_json::Value,
}

impl EntityProfileError {
    pub fn new(
        kind: EntityRefusalKind,
        message: impl Into<String>,
        detail: serde_json::Value,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            detail,
        }
    }

    pub fn to_refusal(&self) -> Refusal {
        self.kind
            .to_refusal(self.message.clone(), self.detail.clone(), None)
    }
}

const SUPPORTED_NORMALIZE_OPS: &[&str] = &[
    "identity",
    "ascii_trim_upper",
    "unicode_fold",
    "lowercase",
    "uppercase",
    "strip_tenant_noise",
    "strip_legal_suffixes",
    "normalize_whitespace",
    "tenant_brand_fingerprint",
    "tokenize",
    "replace_tokens",
    "remove_tokens",
    "strip_suffixes",
    "fingerprint",
    "drop_tenant_stopwords",
    "strip_regab_noise",
    "preserve_legal_form",
    "firm_core",
    "expand_na_abbreviation",
];

const SUPPORTED_SUPPORT_OPS: &[&str] = &[
    "exact_view",
    "token_overlap",
    "string_similarity",
    "tfidf_cosine",
    "alias_patch_match",
    "reviewed_alias",
];

const SUPPORTED_CANNOT_LINK_OPS: &[&str] = &[
    "alias_patch_distinct",
    "protected_token_conflict",
    "related_distinct_phrase",
    "conflicting_anchor",
    "same_property_distinct_rank",
    "role_conflict",
    "protected_anchor_conflict",
    "segment_conflict",
    "platform_label_guard",
    "division_boundary",
];

const SUPPORTED_RELATION_HINT_OPS: &[&str] = &[
    "related_brand_family",
    "possible_successor_predecessor",
    "same_parent_or_sponsor",
    "division_of",
    "parent_subsidiary_context",
    "cross_profile_alignment",
    "context_alignment",
    "segment_alignment",
];

const CROSS_PROFILE_RELATION_OPS: &[&str] = &["cross_profile_alignment"];

fn validate_evidence_lane(
    lane: &str,
    operators: &[EntityOperatorSpec],
    supported: &[&str],
) -> Result<(), EntityProfileError> {
    for operator in operators {
        if operator.op.trim().is_empty() {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Strategy,
                "Entity evidence operators must be non-empty",
                json!({ "lane": lane }),
            ));
        }
        if !supported.contains(&operator.op.as_str()) {
            return Err(unsupported_operator(lane, &operator.op));
        }
    }
    Ok(())
}

fn ensure_unique_non_empty(field: &str, values: &[String]) -> Result<(), EntityProfileError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile list fields must not contain empty values",
                json!({ "field": field }),
            ));
        }
        if !seen.insert(value) {
            return Err(EntityProfileError::new(
                EntityRefusalKind::Profile,
                "Entity profile list fields must not contain duplicate values",
                json!({ "field": field, "value": value }),
            ));
        }
    }
    Ok(())
}

fn unsupported_operator(lane: &str, op: &str) -> EntityProfileError {
    EntityProfileError::new(
        EntityRefusalKind::Strategy,
        "Entity strategy references an unsupported operator",
        json!({
            "lane": lane,
            "operator": op,
            "next_command": "Run strategy lint/doctor and replace the unsupported operator"
        }),
    )
}

fn ensure_entity_profile_kind(value: &YamlValue) -> Result<(), EntityProfileError> {
    if let Some(actual_kind) = detect_profile_input_kind(value)
        && actual_kind != EntityContractKind::EntityProfile
    {
        return Err(wrong_contract_kind(
            EntityRefusalKind::Profile,
            EntityContractKind::EntityProfile,
            actual_kind,
        ));
    }
    Ok(())
}

fn detect_profile_input_kind(value: &YamlValue) -> Option<EntityContractKind> {
    let mapping = value.as_mapping()?;
    if let Some(kind) = mapping
        .get(YamlValue::String("kind".to_string()))
        .and_then(YamlValue::as_str)
    {
        return parse_contract_kind(kind);
    }

    let has_profile_shape = mapping.contains_key(YamlValue::String("profile".to_string()))
        || mapping.contains_key(YamlValue::String("normalized_views".to_string()))
        || mapping.contains_key(YamlValue::String("patch_namespaces".to_string()));
    if has_profile_shape {
        return Some(EntityContractKind::EntityProfile);
    }

    let looks_like_legacy_linkage = mapping.contains_key(YamlValue::String("strategy_id".into()))
        || mapping.contains_key(YamlValue::String("identity".into()))
        || mapping.contains_key(YamlValue::String("assertions".into()));
    if looks_like_legacy_linkage {
        return Some(EntityContractKind::LinkageMap);
    }

    if mapping.contains_key(YamlValue::String("language".into()))
        || mapping.contains_key(YamlValue::String("script_id".into()))
        || mapping.contains_key(YamlValue::String("entrypoint".into()))
    {
        return Some(EntityContractKind::FrozenExecutableStrategy);
    }

    None
}

fn parse_contract_kind(kind: &str) -> Option<EntityContractKind> {
    serde_json::from_value(json!(kind)).ok()
}

fn wrong_contract_kind(
    refusal_kind: EntityRefusalKind,
    expected_kind: EntityContractKind,
    actual_kind: EntityContractKind,
) -> EntityProfileError {
    EntityProfileError::new(
        refusal_kind,
        "Entity profile loader received the wrong contract kind",
        json!({
            "expected_kind": expected_kind,
            "actual_kind": actual_kind,
            "recovery": "Pass an entity profile document here and keep linkage maps, policies, and frozen executable strategies on their own typed paths"
        }),
    )
}

fn ensure_complete_reference(
    field: &str,
    reference: &EntityTypedReference,
    expected_kind: EntityContractKind,
) -> Result<(), EntityProfileError> {
    if reference.kind != Some(expected_kind) {
        return Err(EntityProfileError::new(
            EntityRefusalKind::Profile,
            "Entity profile contract envelope contains the wrong reference kind",
            json!({
                "field": field,
                "expected_kind": expected_kind,
                "actual_kind": reference.kind
            }),
        ));
    }
    if !reference.is_complete_as(expected_kind) {
        return Err(EntityProfileError::new(
            EntityRefusalKind::Profile,
            "Entity profile contract envelope contains an incomplete typed reference",
            json!({
                "field": field,
                "expected_kind": expected_kind,
                "actual_kind": reference.kind
            }),
        ));
    }
    Ok(())
}

fn map_typed_contract_error(error: crate::entity::EntityTypedContractError) -> EntityProfileError {
    let message = match error.code {
        crate::entity::EntityTypedContractErrorCode::WrongKind => {
            "Entity profile contract envelope contains the wrong reference kind"
        }
        crate::entity::EntityTypedContractErrorCode::IncompleteReference => {
            "Entity profile contract envelope contains an incomplete typed reference"
        }
    };

    EntityProfileError::new(
        EntityRefusalKind::Profile,
        message,
        json!({
            "field": error.field,
            "expected_kind": error.expected_kind,
            "actual_kind": error.actual_kind
        }),
    )
}
