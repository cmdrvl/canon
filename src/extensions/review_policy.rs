#![forbid(unsafe_code)]

//! Domain-neutral review semantics and presentation policy package contract.
//!
//! Canon owns the safe action vocabulary and typed patch outcomes. Review
//! policy packages own labels, evidence grouping, escalation, and rationale
//! requirements over opaque ontology, namespace, and relation references.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeSet, error::Error, fmt};

pub const CANON_REVIEW_POLICY_VERSION: &str = "canon.review.policy.v1";

pub type ReviewPolicyResult<T> = Result<T, ReviewPolicyError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPolicyErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingPolicy,
    MissingAction,
    ApprovalRequired,
    RelationIdentityBoundary,
    PatchContract,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPolicyError {
    pub code: ReviewPolicyErrorCode,
    pub message: String,
}

impl ReviewPolicyError {
    pub fn new(code: ReviewPolicyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ReviewPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ReviewPolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPolicyCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSafeAction {
    Same,
    Distinct,
    Related,
    Successor,
    AliasScope,
    Assignment,
    NewEntity,
    Defer,
    Reject,
}

impl ReviewSafeAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Distinct => "distinct",
            Self::Related => "related",
            Self::Successor => "successor",
            Self::AliasScope => "alias_scope",
            Self::Assignment => "assignment",
            Self::NewEntity => "new_entity",
            Self::Defer => "defer",
            Self::Reject => "reject",
        }
    }

    const fn asserts_identity(self) -> bool {
        matches!(
            self,
            Self::Same | Self::AliasScope | Self::Assignment | Self::NewEntity
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewEvidenceKind {
    Identity,
    Distinct,
    RelationOnly,
    Succession,
    Context,
    Missing,
    Override,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRiskTier {
    Low,
    Medium,
    High,
    Critical,
}

impl ReviewRiskTier {
    const fn requires_approval(self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRefKind {
    Ontology,
    Namespace,
    Relation,
    EvidenceGroup,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPolicyPatchKind {
    Alias,
    CannotLink,
    Relation,
    SuccessorRelation,
    AliasScope,
    Assignment,
    NewEntity,
    Defer,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPolicyPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<ReviewPolicyDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<ReviewLabelMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<ReviewPolicyDocumentationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewPolicyDocumentationRef {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewLabelMapping {
    pub ref_id: String,
    pub ref_kind: ReviewRefKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPolicyDefinition {
    pub policy_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_groups: Vec<ReviewEvidenceGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_rules: Vec<ReviewActionRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvidenceGroup {
    pub group_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespace_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ontology_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_kinds: Vec<ReviewEvidenceKind>,
    pub risk_tier: ReviewRiskTier,
    pub required_rationale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewActionRule {
    pub action: ReviewSafeAction,
    pub label: String,
    pub patch_kind: ReviewPolicyPatchKind,
    pub risk_tier: ReviewRiskTier,
    pub required_rationale: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_evidence_kinds: Vec<ReviewEvidenceKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub two_person_rule: Option<ReviewTwoPersonRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewTwoPersonRule {
    pub min_approvals: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approval_role_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewPolicyRef {
    pub package_digest: String,
    pub policy_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledReviewPolicy {
    pub package_digest: String,
    pub policy: ReviewPolicyDefinition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<ReviewLabelMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvidenceRef {
    pub evidence_id: String,
    pub evidence_kind: ReviewEvidenceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedOpaqueRef {
    pub ref_id: String,
    pub label: String,
    pub ref_kind: ReviewRefKind,
    pub known: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentedEvidenceRef {
    pub evidence_id: String,
    pub evidence_kind: ReviewEvidenceKind,
    pub group_id: String,
    pub group_label: String,
    pub risk_tier: ReviewRiskTier,
    pub required_rationale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<RenderedOpaqueRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<RenderedOpaqueRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology: Option<RenderedOpaqueRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewApproval {
    pub approval_id: String,
    pub operator_id: String,
    pub role_ref: String,
    pub approved_action: ReviewSafeAction,
    pub policy_digest: String,
    pub decision_binding_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPolicyDecisionInput {
    pub review_id: String,
    pub action: ReviewSafeAction,
    pub operator_id: String,
    pub policy_digest: String,
    pub source_review_artifact_hash: String,
    pub decision_binding_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<ReviewEvidenceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approvals: Vec<ReviewApproval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPolicyPatch {
    pub patch_id: String,
    pub patch_kind: ReviewPolicyPatchKind,
    pub review_id: String,
    pub action: ReviewSafeAction,
    pub operator_id: String,
    pub policy_digest: String,
    pub source_review_artifact_hash: String,
    pub decision_binding_hash: String,
    pub surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub approvals_hash: String,
}

pub fn review_policy_schema_version() -> &'static str {
    CANON_REVIEW_POLICY_VERSION
}

pub fn finalize_package(
    mut package: ReviewPolicyPackage,
) -> ReviewPolicyResult<ReviewPolicyPackage> {
    if package.version.trim().is_empty() {
        package.version = CANON_REVIEW_POLICY_VERSION.to_string();
    }
    if package.version != CANON_REVIEW_POLICY_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported review policy contract version: {}",
            package.version
        )));
    }
    package.package_id = normalized_package_id(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;

    let mut documentation = package
        .documentation
        .into_iter()
        .map(normalize_documentation_ref)
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    documentation.sort();
    documentation.dedup();
    let known_docs = documentation
        .iter()
        .map(|entry| entry.uri.clone())
        .collect::<BTreeSet<_>>();

    let mut labels = package
        .labels
        .into_iter()
        .map(normalize_label_mapping)
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    labels.sort();
    labels.dedup();

    let mut policies = package
        .policies
        .into_iter()
        .map(|policy| normalize_policy(policy, &known_docs))
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    if policies.is_empty() {
        return Err(artifact_contract_error(
            "review policy package must declare at least one policy",
        ));
    }
    policies.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));

    let mut deduped: Vec<ReviewPolicyDefinition> = Vec::with_capacity(policies.len());
    for policy in policies {
        if let Some(previous) = deduped.last()
            && previous.policy_id == policy.policy_id
        {
            if previous != &policy {
                return Err(artifact_contract_error(format!(
                    "policy {} cannot be declared with conflicting content",
                    policy.policy_id
                )));
            }
            continue;
        }
        deduped.push(policy);
    }

    package.documentation = documentation;
    package.labels = labels;
    package.policies = deduped;
    Ok(package)
}

pub fn canonical_package_bytes(package: &ReviewPolicyPackage) -> ReviewPolicyResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize review policy package: {error}"
        ))
    })
}

pub fn review_policy_package_digest(package: &ReviewPolicyPackage) -> ReviewPolicyResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn finalize_policy_ref(mut reference: ReviewPolicyRef) -> ReviewPolicyResult<ReviewPolicyRef> {
    reference.package_digest = normalized_hash(&reference.package_digest, "package_digest")?;
    reference.policy_id = normalized_opaque_ref(&reference.policy_id, "policy_id")?;
    Ok(reference)
}

pub fn resolve_policy_ref(
    package: &ReviewPolicyPackage,
    reference: &ReviewPolicyRef,
) -> ReviewPolicyResult<ReviewPolicyDefinition> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_policy_ref(reference.clone())?;
    let digest = review_policy_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "review policy {} is pinned to {} but package resolved to {}",
            reference.policy_id, reference.package_digest, digest
        )));
    }
    package
        .policies
        .iter()
        .find(|policy| policy.policy_id == reference.policy_id)
        .cloned()
        .ok_or_else(|| {
            missing_policy_error(format!("unknown review policy {}", reference.policy_id))
        })
}

pub fn compile_policy(
    package: &ReviewPolicyPackage,
    reference: &ReviewPolicyRef,
) -> ReviewPolicyResult<CompiledReviewPolicy> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_policy_ref(reference.clone())?;
    let digest = review_policy_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "review policy {} is pinned to {} but package resolved to {}",
            reference.policy_id, reference.package_digest, digest
        )));
    }
    let policy = package
        .policies
        .iter()
        .find(|policy| policy.policy_id == reference.policy_id)
        .cloned()
        .ok_or_else(|| {
            missing_policy_error(format!("unknown review policy {}", reference.policy_id))
        })?;
    Ok(CompiledReviewPolicy {
        package_digest: digest,
        policy,
        labels: package.labels,
    })
}

pub fn validate_package_for_execution(
    package: &ReviewPolicyPackage,
    required_policies: &[ReviewPolicyRef],
) -> ReviewPolicyResult<String> {
    let package = finalize_package(package.clone())?;
    let digest = review_policy_package_digest(&package)?;
    for reference in required_policies {
        let _ = compile_policy(&package, reference)?;
    }
    Ok(digest)
}

pub fn package_compatibility(
    locked: &ReviewPolicyPackage,
    candidate: &ReviewPolicyPackage,
    locked_refs: &[ReviewPolicyRef],
) -> ReviewPolicyResult<ReviewPolicyCompatibility> {
    let locked = finalize_package(locked.clone())?;
    let candidate = finalize_package(candidate.clone())?;
    let locked_digest = review_policy_package_digest(&locked)?;
    let candidate_digest = review_policy_package_digest(&candidate)?;
    if locked_digest == candidate_digest {
        return Ok(ReviewPolicyCompatibility::ExactDigest);
    }
    if locked.package_id != candidate.package_id {
        return Err(compatibility_policy_error(format!(
            "review policy package id changed from {} to {}",
            locked.package_id, candidate.package_id
        )));
    }
    if semver_major(&locked.package_version)? != semver_major(&candidate.package_version)? {
        return Err(compatibility_policy_error(format!(
            "review policy package major version changed from {} to {}",
            locked.package_version, candidate.package_version
        )));
    }
    for reference in locked_refs {
        let reference = finalize_policy_ref(reference.clone())?;
        if reference.package_digest != locked_digest {
            return Err(compatibility_policy_error(format!(
                "review policy {} is not pinned to locked package digest {}",
                reference.policy_id, locked_digest
            )));
        }
        let migrated = ReviewPolicyRef {
            package_digest: candidate_digest.clone(),
            policy_id: reference.policy_id,
        };
        let _ = resolve_policy_ref(&candidate, &migrated)?;
    }
    Ok(ReviewPolicyCompatibility::CompatibleSameMajor)
}

pub fn render_opaque_ref(
    policy: &CompiledReviewPolicy,
    ref_kind: ReviewRefKind,
    ref_id: &str,
) -> RenderedOpaqueRef {
    let normalized = normalized_opaque_ref(ref_id, "ref_id").unwrap_or_else(|_| ref_id.to_string());
    if let Some(label) = policy
        .labels
        .iter()
        .find(|label| label.ref_kind == ref_kind && label.ref_id == normalized)
    {
        return RenderedOpaqueRef {
            ref_id: normalized,
            label: label.label.clone(),
            ref_kind,
            known: true,
        };
    }
    RenderedOpaqueRef {
        ref_id: normalized.clone(),
        label: normalized,
        ref_kind,
        known: false,
    }
}

pub fn present_evidence_refs(
    policy: &CompiledReviewPolicy,
    evidence_refs: &[ReviewEvidenceRef],
) -> ReviewPolicyResult<Vec<PresentedEvidenceRef>> {
    let evidence_refs = evidence_refs
        .iter()
        .cloned()
        .map(normalize_evidence_ref)
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    let mut presented = evidence_refs
        .into_iter()
        .map(|evidence| {
            let group = best_group_for_evidence(&policy.policy, &evidence);
            PresentedEvidenceRef {
                evidence_id: evidence.evidence_id,
                evidence_kind: evidence.evidence_kind,
                group_id: group
                    .map(|group| group.group_id.clone())
                    .unwrap_or_else(|| "opaque:ungrouped".to_string()),
                group_label: group
                    .map(|group| group.label.clone())
                    .unwrap_or_else(|| "Ungrouped evidence".to_string()),
                risk_tier: group
                    .map(|group| group.risk_tier)
                    .unwrap_or(ReviewRiskTier::Medium),
                required_rationale: group.map(|group| group.required_rationale).unwrap_or(false),
                namespace: evidence
                    .namespace_ref
                    .as_deref()
                    .map(|ref_id| render_opaque_ref(policy, ReviewRefKind::Namespace, ref_id)),
                relation: evidence
                    .relation_ref
                    .as_deref()
                    .map(|ref_id| render_opaque_ref(policy, ReviewRefKind::Relation, ref_id)),
                ontology: evidence
                    .ontology_ref
                    .as_deref()
                    .map(|ref_id| render_opaque_ref(policy, ReviewRefKind::Ontology, ref_id)),
                reason_code: evidence.reason_code,
            }
        })
        .collect::<Vec<_>>();
    presented.sort_by(|left, right| left.evidence_id.cmp(&right.evidence_id));
    Ok(presented)
}

pub fn compile_review_decision(
    package: &ReviewPolicyPackage,
    reference: &ReviewPolicyRef,
    decision: &ReviewPolicyDecisionInput,
) -> ReviewPolicyResult<ReviewPolicyPatch> {
    let compiled = compile_policy(package, reference)?;
    let mut decision = normalize_decision_input(decision.clone())?;
    if decision.policy_digest != compiled.package_digest {
        return Err(compatibility_policy_error(format!(
            "review decision {} is bound to {} but policy resolved to {}",
            decision.review_id, decision.policy_digest, compiled.package_digest
        )));
    }
    let action_rule = compiled
        .policy
        .action_rules
        .iter()
        .find(|rule| rule.action == decision.action)
        .ok_or_else(|| {
            missing_action_error(format!(
                "policy {} does not allow action {}",
                compiled.policy.policy_id,
                decision.action.as_str()
            ))
        })?;

    validate_relation_identity_boundary(&decision, action_rule)?;
    validate_action_evidence(&decision, action_rule)?;
    validate_required_rationale(&decision, action_rule)?;
    validate_approvals(&decision, action_rule)?;
    validate_patch_fields(&mut decision, action_rule)?;

    let patch_seed = json!({
        "review_id": decision.review_id,
        "action": decision.action,
        "policy_digest": decision.policy_digest,
        "decision_binding_hash": decision.decision_binding_hash,
        "source_review_artifact_hash": decision.source_review_artifact_hash,
        "surface_ids": decision.surface_ids
    });
    Ok(ReviewPolicyPatch {
        patch_id: format!(
            "review_patch:{}",
            blake3::hash(canonical_json(&patch_seed).as_bytes()).to_hex()
        ),
        patch_kind: action_rule.patch_kind,
        review_id: decision.review_id,
        action: decision.action,
        operator_id: decision.operator_id,
        policy_digest: decision.policy_digest,
        source_review_artifact_hash: decision.source_review_artifact_hash,
        decision_binding_hash: decision.decision_binding_hash,
        surface_ids: decision.surface_ids,
        relation_ref: decision.relation_ref,
        target_canonical_id: decision.target_canonical_id,
        rationale: decision.rationale,
        approvals_hash: approvals_hash(&decision.approvals)?,
    })
}

fn normalize_policy(
    mut policy: ReviewPolicyDefinition,
    known_docs: &BTreeSet<String>,
) -> ReviewPolicyResult<ReviewPolicyDefinition> {
    policy.policy_id = normalized_opaque_ref(&policy.policy_id, "policy_id")?;
    policy.evidence_groups = policy
        .evidence_groups
        .into_iter()
        .map(normalize_evidence_group)
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    policy
        .evidence_groups
        .sort_by(|left, right| left.group_id.cmp(&right.group_id));
    policy.action_rules = policy
        .action_rules
        .into_iter()
        .map(normalize_action_rule)
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    if policy.action_rules.is_empty() {
        return Err(artifact_contract_error(format!(
            "policy {} must declare at least one action rule",
            policy.policy_id
        )));
    }
    policy.action_rules.sort_by_key(|rule| rule.action);
    policy.documentation_refs =
        normalize_documentation_ref_list(policy.documentation_refs, known_docs)?;
    Ok(policy)
}

fn normalize_evidence_group(
    mut group: ReviewEvidenceGroup,
) -> ReviewPolicyResult<ReviewEvidenceGroup> {
    group.group_id = normalized_opaque_ref(&group.group_id, "evidence_groups.group_id")?;
    group.label = normalized_non_empty(&group.label, "evidence_groups.label")?;
    group.namespace_refs =
        normalize_opaque_ref_list(group.namespace_refs, "evidence_groups.namespace_refs")?;
    group.relation_refs =
        normalize_opaque_ref_list(group.relation_refs, "evidence_groups.relation_refs")?;
    group.ontology_refs =
        normalize_opaque_ref_list(group.ontology_refs, "evidence_groups.ontology_refs")?;
    group.evidence_kinds.sort();
    group.evidence_kinds.dedup();
    Ok(group)
}

fn normalize_action_rule(mut rule: ReviewActionRule) -> ReviewPolicyResult<ReviewActionRule> {
    rule.label = normalized_non_empty(&rule.label, "action_rules.label")?;
    rule.allowed_evidence_kinds.sort();
    rule.allowed_evidence_kinds.dedup();
    if !rule.allowed_evidence_kinds.is_empty()
        && rule
            .allowed_evidence_kinds
            .iter()
            .all(|kind| *kind == ReviewEvidenceKind::RelationOnly)
        && rule.action.asserts_identity()
    {
        return Err(relation_identity_boundary_error(format!(
            "action {} cannot be authorized by relation-only evidence",
            rule.action.as_str()
        )));
    }
    if rule.risk_tier.requires_approval() && rule.two_person_rule.is_none() {
        return Err(artifact_contract_error(format!(
            "high-risk action {} must declare a two-person approval rule",
            rule.action.as_str()
        )));
    }
    if let Some(rule) = rule.two_person_rule.as_mut() {
        if rule.min_approvals == 0 {
            return Err(artifact_contract_error(
                "two_person_rule.min_approvals must be greater than zero",
            ));
        }
        rule.approval_role_refs =
            normalize_opaque_ref_list(rule.approval_role_refs.clone(), "approval_role_refs")?;
    }
    Ok(rule)
}

fn normalize_label_mapping(
    mut label: ReviewLabelMapping,
) -> ReviewPolicyResult<ReviewLabelMapping> {
    label.ref_id = normalized_opaque_ref(&label.ref_id, "labels.ref_id")?;
    label.label = normalized_non_empty(&label.label, "labels.label")?;
    if let Some(help_text) = label.help_text.as_mut() {
        *help_text = normalized_non_empty(help_text, "labels.help_text")?;
    }
    Ok(label)
}

fn normalize_documentation_ref(
    mut reference: ReviewPolicyDocumentationRef,
) -> ReviewPolicyResult<ReviewPolicyDocumentationRef> {
    reference.label = normalized_non_empty(&reference.label, "documentation.label")?;
    reference.uri = normalized_documentation_uri(&reference.uri, "documentation.uri")?;
    Ok(reference)
}

fn normalize_evidence_ref(
    mut evidence: ReviewEvidenceRef,
) -> ReviewPolicyResult<ReviewEvidenceRef> {
    evidence.evidence_id = normalized_non_empty(&evidence.evidence_id, "evidence_id")?;
    if let Some(namespace_ref) = evidence.namespace_ref.as_mut() {
        *namespace_ref = normalized_opaque_ref(namespace_ref, "namespace_ref")?;
    }
    if let Some(relation_ref) = evidence.relation_ref.as_mut() {
        *relation_ref = normalized_opaque_ref(relation_ref, "relation_ref")?;
    }
    if let Some(ontology_ref) = evidence.ontology_ref.as_mut() {
        *ontology_ref = normalized_opaque_ref(ontology_ref, "ontology_ref")?;
    }
    if let Some(reason_code) = evidence.reason_code.as_mut() {
        *reason_code = normalized_non_empty(reason_code, "reason_code")?;
    }
    Ok(evidence)
}

fn normalize_decision_input(
    mut decision: ReviewPolicyDecisionInput,
) -> ReviewPolicyResult<ReviewPolicyDecisionInput> {
    decision.review_id = normalized_non_empty(&decision.review_id, "review_id")?;
    decision.operator_id = normalized_non_empty(&decision.operator_id, "operator_id")?;
    decision.policy_digest = normalized_hash(&decision.policy_digest, "policy_digest")?;
    decision.source_review_artifact_hash = normalized_hash(
        &decision.source_review_artifact_hash,
        "source_review_artifact_hash",
    )?;
    decision.decision_binding_hash =
        normalized_hash(&decision.decision_binding_hash, "decision_binding_hash")?;
    decision.surface_ids = normalize_non_empty_list(decision.surface_ids, "surface_ids")?;
    if let Some(relation_ref) = decision.relation_ref.as_mut() {
        *relation_ref = normalized_opaque_ref(relation_ref, "relation_ref")?;
    }
    if let Some(target_canonical_id) = decision.target_canonical_id.as_mut() {
        *target_canonical_id = normalized_non_empty(target_canonical_id, "target_canonical_id")?;
    }
    if let Some(rationale) = decision.rationale.as_mut() {
        *rationale = normalized_non_empty(rationale, "rationale")?;
    }
    decision.evidence_refs = decision
        .evidence_refs
        .into_iter()
        .map(normalize_evidence_ref)
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    decision.approvals = decision
        .approvals
        .into_iter()
        .map(normalize_approval)
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    Ok(decision)
}

fn normalize_approval(mut approval: ReviewApproval) -> ReviewPolicyResult<ReviewApproval> {
    approval.approval_id = normalized_non_empty(&approval.approval_id, "approval_id")?;
    approval.operator_id = normalized_non_empty(&approval.operator_id, "approval.operator_id")?;
    approval.role_ref = normalized_opaque_ref(&approval.role_ref, "approval.role_ref")?;
    approval.policy_digest = normalized_hash(&approval.policy_digest, "approval.policy_digest")?;
    approval.decision_binding_hash = normalized_hash(
        &approval.decision_binding_hash,
        "approval.decision_binding_hash",
    )?;
    Ok(approval)
}

fn best_group_for_evidence<'a>(
    policy: &'a ReviewPolicyDefinition,
    evidence: &ReviewEvidenceRef,
) -> Option<&'a ReviewEvidenceGroup> {
    policy
        .evidence_groups
        .iter()
        .find(|group| group.evidence_kinds.contains(&evidence.evidence_kind))
        .or_else(|| {
            policy
                .evidence_groups
                .iter()
                .find(|group| group_matches_ref(group, evidence))
        })
}

fn group_matches_ref(group: &ReviewEvidenceGroup, evidence: &ReviewEvidenceRef) -> bool {
    evidence
        .namespace_ref
        .as_ref()
        .is_some_and(|ref_id| group.namespace_refs.contains(ref_id))
        || evidence
            .relation_ref
            .as_ref()
            .is_some_and(|ref_id| group.relation_refs.contains(ref_id))
        || evidence
            .ontology_ref
            .as_ref()
            .is_some_and(|ref_id| group.ontology_refs.contains(ref_id))
}

fn validate_relation_identity_boundary(
    decision: &ReviewPolicyDecisionInput,
    action_rule: &ReviewActionRule,
) -> ReviewPolicyResult<()> {
    if !action_rule.action.asserts_identity() || decision.evidence_refs.is_empty() {
        return Ok(());
    }
    if decision
        .evidence_refs
        .iter()
        .all(|evidence| evidence.evidence_kind == ReviewEvidenceKind::RelationOnly)
    {
        return Err(relation_identity_boundary_error(format!(
            "decision {} cannot compile action {} from relation-only evidence",
            decision.review_id,
            action_rule.action.as_str()
        )));
    }
    Ok(())
}

fn validate_action_evidence(
    decision: &ReviewPolicyDecisionInput,
    action_rule: &ReviewActionRule,
) -> ReviewPolicyResult<()> {
    if action_rule.allowed_evidence_kinds.is_empty() || decision.evidence_refs.is_empty() {
        return Ok(());
    }
    if decision.evidence_refs.iter().any(|evidence| {
        action_rule
            .allowed_evidence_kinds
            .contains(&evidence.evidence_kind)
    }) {
        Ok(())
    } else {
        Err(missing_action_error(format!(
            "decision {} has no evidence kind allowed for action {}",
            decision.review_id,
            action_rule.action.as_str()
        )))
    }
}

fn validate_required_rationale(
    decision: &ReviewPolicyDecisionInput,
    action_rule: &ReviewActionRule,
) -> ReviewPolicyResult<()> {
    if action_rule.required_rationale
        && decision
            .rationale
            .as_deref()
            .is_none_or(|rationale| rationale.trim().is_empty())
    {
        return Err(patch_contract_error(format!(
            "decision {} requires rationale for action {}",
            decision.review_id,
            action_rule.action.as_str()
        )));
    }
    Ok(())
}

fn validate_approvals(
    decision: &ReviewPolicyDecisionInput,
    action_rule: &ReviewActionRule,
) -> ReviewPolicyResult<()> {
    if !action_rule.risk_tier.requires_approval() {
        return Ok(());
    }
    let rule = action_rule.two_person_rule.as_ref().ok_or_else(|| {
        approval_required_error(format!(
            "high-risk action {} requires an approval rule",
            action_rule.action.as_str()
        ))
    })?;
    let approved = decision
        .approvals
        .iter()
        .filter(|approval| {
            approval.approved_action == action_rule.action
                && approval.policy_digest == decision.policy_digest
                && approval.decision_binding_hash == decision.decision_binding_hash
                && approval.operator_id != decision.operator_id
                && (rule.approval_role_refs.is_empty()
                    || rule.approval_role_refs.contains(&approval.role_ref))
        })
        .map(|approval| approval.operator_id.clone())
        .collect::<BTreeSet<_>>();
    if approved.len() < usize::from(rule.min_approvals) {
        return Err(approval_required_error(format!(
            "decision {} requires {} hash-bound approval(s) for action {}",
            decision.review_id,
            rule.min_approvals,
            action_rule.action.as_str()
        )));
    }
    Ok(())
}

fn validate_patch_fields(
    decision: &mut ReviewPolicyDecisionInput,
    action_rule: &ReviewActionRule,
) -> ReviewPolicyResult<()> {
    match action_rule.patch_kind {
        ReviewPolicyPatchKind::Alias | ReviewPolicyPatchKind::CannotLink => {
            require_surface_count(decision, 2)?
        }
        ReviewPolicyPatchKind::Relation | ReviewPolicyPatchKind::SuccessorRelation => {
            require_surface_count(decision, 2)?;
            if decision.relation_ref.is_none() {
                decision.relation_ref = decision
                    .evidence_refs
                    .iter()
                    .find_map(|evidence| evidence.relation_ref.clone());
            }
            if decision.relation_ref.is_none() {
                return Err(patch_contract_error(format!(
                    "decision {} requires relation_ref for action {}",
                    decision.review_id,
                    decision.action.as_str()
                )));
            }
        }
        ReviewPolicyPatchKind::AliasScope
        | ReviewPolicyPatchKind::NewEntity
        | ReviewPolicyPatchKind::Defer
        | ReviewPolicyPatchKind::Reject => require_surface_count(decision, 1)?,
        ReviewPolicyPatchKind::Assignment => {
            require_surface_count(decision, 1)?;
            if decision.target_canonical_id.is_none() {
                return Err(patch_contract_error(format!(
                    "decision {} requires target_canonical_id for assignment",
                    decision.review_id
                )));
            }
        }
    }
    Ok(())
}

fn require_surface_count(
    decision: &ReviewPolicyDecisionInput,
    minimum: usize,
) -> ReviewPolicyResult<()> {
    if decision.surface_ids.len() >= minimum {
        Ok(())
    } else {
        Err(patch_contract_error(format!(
            "decision {} requires at least {} surface reference(s)",
            decision.review_id, minimum
        )))
    }
}

fn approvals_hash(approvals: &[ReviewApproval]) -> ReviewPolicyResult<String> {
    let mut approvals = approvals.to_vec();
    approvals.sort_by(|left, right| left.approval_id.cmp(&right.approval_id));
    let bytes = serde_json::to_vec(&approvals).map_err(|error| {
        patch_contract_error(format!(
            "failed to serialize approvals for hashing: {error}"
        ))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(boolean) => boolean.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => serde_json::to_string(text).expect("string serializes"),
        serde_json::Value::Array(array) => {
            let mut rendered = String::from("[");
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                rendered.push_str(&canonical_json(item));
            }
            rendered.push(']');
            rendered
        }
        serde_json::Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let mut rendered = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                rendered.push_str(&serde_json::to_string(key).expect("key serializes"));
                rendered.push(':');
                rendered.push_str(&canonical_json(&object[*key]));
            }
            rendered.push('}');
            rendered
        }
    }
}

fn normalize_documentation_ref_list(
    refs: Vec<String>,
    known_docs: &BTreeSet<String>,
) -> ReviewPolicyResult<Vec<String>> {
    let mut refs = refs
        .into_iter()
        .map(|reference| normalized_documentation_uri(&reference, "documentation_refs"))
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    refs.sort();
    refs.dedup();
    for reference in &refs {
        if !known_docs.contains(reference) {
            return Err(artifact_contract_error(format!(
                "documentation reference {} is not declared in package documentation",
                reference
            )));
        }
    }
    Ok(refs)
}

fn normalize_opaque_ref_list(values: Vec<String>, field: &str) -> ReviewPolicyResult<Vec<String>> {
    let mut values = values
        .into_iter()
        .map(|value| normalized_opaque_ref(&value, field))
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn normalize_non_empty_list(values: Vec<String>, field: &str) -> ReviewPolicyResult<Vec<String>> {
    let mut values = values
        .into_iter()
        .map(|value| normalized_non_empty(&value, field))
        .collect::<ReviewPolicyResult<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn normalized_package_id(value: &str, field: &str) -> ReviewPolicyResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    {
        Ok(value)
    } else {
        Err(artifact_contract_error(format!(
            "{field} must match package id syntax"
        )))
    }
}

fn normalized_opaque_ref(value: &str, field: &str) -> ReviewPolicyResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some((left, right)) = value.split_once(':') else {
        return Err(artifact_contract_error(format!(
            "{field} must be an opaque namespaced ref"
        )));
    };
    normalized_package_id(left, field)?;
    normalized_package_id(right, field)?;
    Ok(value)
}

fn normalized_hash(value: &str, field: &str) -> ReviewPolicyResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.starts_with("blake3:")
        && value.len() == "blake3:".len() + 64
        && value["blake3:".len()..]
            .chars()
            .all(|ch| ch.is_ascii_hexdigit())
    {
        Ok(value)
    } else {
        Err(artifact_contract_error(format!(
            "{field} must be a blake3 content hash"
        )))
    }
}

fn normalized_semver(value: &str, field: &str) -> ReviewPolicyResult<String> {
    let value = normalized_non_empty(value, field)?;
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    {
        Ok(value)
    } else {
        Err(artifact_contract_error(format!(
            "{field} must be a major.minor.patch semver"
        )))
    }
}

fn normalized_documentation_uri(value: &str, field: &str) -> ReviewPolicyResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.starts_with('/')
        || value.contains('\\')
        || value.split('/').any(|segment| segment == "..")
    {
        return Err(artifact_contract_error(format!(
            "{field} must be a relative path or http(s) URI without traversal"
        )));
    }
    Ok(value)
}

fn normalized_non_empty(value: &str, field: &str) -> ReviewPolicyResult<String> {
    if value.trim().is_empty() || value.trim() != value {
        Err(artifact_contract_error(format!(
            "{field} must be non-empty and already trimmed"
        )))
    } else {
        Ok(value.to_string())
    }
}

fn semver_major(value: &str) -> ReviewPolicyResult<u64> {
    normalized_semver(value, "package_version")?
        .split('.')
        .next()
        .expect("semver has major")
        .parse::<u64>()
        .map_err(|error| artifact_contract_error(format!("invalid semver major: {error}")))
}

fn artifact_contract_error(message: impl Into<String>) -> ReviewPolicyError {
    ReviewPolicyError::new(ReviewPolicyErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> ReviewPolicyError {
    ReviewPolicyError::new(ReviewPolicyErrorCode::CompatibilityPolicy, message)
}

fn missing_policy_error(message: impl Into<String>) -> ReviewPolicyError {
    ReviewPolicyError::new(ReviewPolicyErrorCode::MissingPolicy, message)
}

fn missing_action_error(message: impl Into<String>) -> ReviewPolicyError {
    ReviewPolicyError::new(ReviewPolicyErrorCode::MissingAction, message)
}

fn approval_required_error(message: impl Into<String>) -> ReviewPolicyError {
    ReviewPolicyError::new(ReviewPolicyErrorCode::ApprovalRequired, message)
}

fn relation_identity_boundary_error(message: impl Into<String>) -> ReviewPolicyError {
    ReviewPolicyError::new(ReviewPolicyErrorCode::RelationIdentityBoundary, message)
}

fn patch_contract_error(message: impl Into<String>) -> ReviewPolicyError {
    ReviewPolicyError::new(ReviewPolicyErrorCode::PatchContract, message)
}
