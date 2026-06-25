//! Entity profile schema and validation contracts.
//!
//! Profiles define identity semantics before any entity stage runs. This file
//! validates profile declarations and strategy/profile compatibility; it does
//! not perform matching, candidate generation, or promotion.

use crate::Refusal;
use crate::entity::{EntityProfileReference, error::EntityRefusalKind};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

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

impl EntityProfileDocument {
    pub fn from_yaml_str(input: &str) -> Result<Self, EntityProfileError> {
        let profile = serde_yaml::from_str::<Self>(input).map_err(|error| {
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
        if self.patch_namespaces.is_incomplete() {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityPatchNamespaces {
    #[serde(default)]
    pub aliases: String,
    #[serde(default)]
    pub distinct: String,
    #[serde(default)]
    pub relations: String,
}

impl EntityPatchNamespaces {
    fn is_incomplete(&self) -> bool {
        self.aliases.trim().is_empty()
            || self.distinct.trim().is_empty()
            || self.relations.trim().is_empty()
    }
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
    "unicode_fold",
    "lowercase",
    "strip_tenant_noise",
    "strip_legal_suffixes",
    "normalize_whitespace",
    "tenant_brand_fingerprint",
    "tokenize",
    "drop_tenant_stopwords",
    "strip_regab_noise",
    "preserve_legal_form",
    "firm_core",
    "expand_na_abbreviation",
];

const SUPPORTED_SUPPORT_OPS: &[&str] = &[
    "exact_view",
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
