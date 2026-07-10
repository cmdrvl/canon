#![forbid(unsafe_code)]

//! Domain-neutral evidence policy package contract.
//!
//! Evidence policy packages compose candidate, evidence, decision, abstention,
//! and review rules over opaque profile field, view, namespace, and relation
//! references. Hard veto and auto-decision authority are explicit clauses, not
//! implicit weights, and every evidence rule declares the typed evidence IR it
//! emits.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

pub const CANON_EVIDENCE_POLICY_VERSION: &str = "canon.evidence.policy.v1";

pub type EvidencePolicyResult<T> = Result<T, EvidencePolicyError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePolicyErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingPolicy,
    MissingRule,
    UnsupportedCapability,
    DecisionConflict,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencePolicyError {
    pub code: EvidencePolicyErrorCode,
    pub message: String,
}

impl EvidencePolicyError {
    pub fn new(code: EvidencePolicyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for EvidencePolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for EvidencePolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePolicyCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencePolicyPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<EvidencePolicyDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<EvidencePolicyDocumentationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidencePolicyDocumentationRef {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePrimitive {
    Exact,
    Token,
    Vector,
    Structural,
    Temporal,
    Anchor,
    Context,
    ProtectedConflict,
    CannotLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateTargetKind {
    Pair,
    Hyperedge,
    RecordLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRuleLane {
    Positive,
    Negative,
    Contextual,
    Missing,
    HardVeto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceIrKind {
    PairSupport,
    HyperedgeSupport,
    RecordLinkSupport,
    ContextOnly,
    ContextualNegative,
    Missingness,
    AntiMergeVeto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceSelectorScope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespace_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRule {
    pub rule_id: String,
    pub primitive: EvidencePrimitive,
    pub target_kind: CandidateTargetKind,
    pub selectors: EvidenceSelectorScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRule {
    pub rule_id: String,
    pub primitive: EvidencePrimitive,
    pub lane: EvidenceRuleLane,
    pub emits_kind: EvidenceIrKind,
    pub selectors: EvidenceSelectorScope,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionGate {
    pub gate_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub require_rule_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbid_rule_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_trigger_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoDecisionOutcome {
    Merge,
    Distinct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoDecisionRule {
    pub decision_id: String,
    pub gate_id: String,
    pub outcome: AutoDecisionOutcome,
    pub authority_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbstentionDisposition {
    InsufficientEvidence,
    Conflict,
    ResourceLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbstentionRule {
    pub abstention_id: String,
    pub gate_id: String,
    pub disposition: AbstentionDisposition,
    pub reason_code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDisposition {
    Same,
    Distinct,
    Related,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRule {
    pub review_id: String,
    pub gate_id: String,
    pub artifact_family: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_dispositions: Vec<ReviewDisposition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencePolicyDefinition {
    pub policy_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_rules: Vec<CandidateRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_rules: Vec<EvidenceRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_gates: Vec<DecisionGate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_decision_rules: Vec<AutoDecisionRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abstention_rules: Vec<AbstentionRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_rules: Vec<ReviewRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidencePolicyRef {
    pub package_digest: String,
    pub policy_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceCapabilityCatalog {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespace_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_primitives: Vec<EvidencePrimitive>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_candidate_targets: Vec<CandidateTargetKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_evidence_kinds: Vec<EvidenceIrKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledEvidencePolicy {
    pub package_digest: String,
    pub policy: EvidencePolicyDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcome {
    NoDecision,
    HardVeto,
    AutoMerge,
    AutoDistinct,
    AbstainInsufficientEvidence,
    AbstainConflict,
    AbstainResourceLimit,
    ReviewRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub outcome: DecisionOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_rule_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_artifact_family: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_review_dispositions: Vec<ReviewDisposition>,
}

pub fn evidence_policy_schema_version() -> &'static str {
    CANON_EVIDENCE_POLICY_VERSION
}

pub fn finalize_package(
    mut package: EvidencePolicyPackage,
) -> EvidencePolicyResult<EvidencePolicyPackage> {
    if package.version.trim().is_empty() {
        package.version = CANON_EVIDENCE_POLICY_VERSION.to_string();
    }
    if package.version != CANON_EVIDENCE_POLICY_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported evidence policy contract version: {}",
            package.version
        )));
    }

    package.package_id = normalized_package_id(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;

    let mut documentation = package
        .documentation
        .into_iter()
        .map(normalize_documentation_ref)
        .collect::<EvidencePolicyResult<Vec<_>>>()?;
    documentation.sort();
    documentation.dedup();
    let known_docs = documentation
        .iter()
        .map(|entry| entry.uri.clone())
        .collect::<BTreeSet<_>>();

    let mut policies = package
        .policies
        .into_iter()
        .map(|policy| normalize_policy(policy, &package.package_id, &known_docs))
        .collect::<EvidencePolicyResult<Vec<_>>>()?;
    if policies.is_empty() {
        return Err(artifact_contract_error(
            "evidence policy package must declare at least one policy",
        ));
    }
    policies.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));

    let mut deduped: Vec<EvidencePolicyDefinition> = Vec::with_capacity(policies.len());
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
    package.policies = deduped;
    Ok(package)
}

pub fn canonical_package_bytes(package: &EvidencePolicyPackage) -> EvidencePolicyResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize evidence policy package: {error}"
        ))
    })
}

pub fn evidence_policy_package_digest(
    package: &EvidencePolicyPackage,
) -> EvidencePolicyResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn finalize_policy_ref(
    mut reference: EvidencePolicyRef,
) -> EvidencePolicyResult<EvidencePolicyRef> {
    reference.package_digest = normalized_hash(&reference.package_digest, "package_digest")?;
    reference.policy_id = normalized_opaque_ref(&reference.policy_id, "policy_id")?;
    Ok(reference)
}

pub fn resolve_policy_ref(
    package: &EvidencePolicyPackage,
    reference: &EvidencePolicyRef,
) -> EvidencePolicyResult<EvidencePolicyDefinition> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_policy_ref(reference.clone())?;
    let digest = evidence_policy_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "evidence policy {} is pinned to {} but package resolved to {}",
            reference.policy_id, reference.package_digest, digest
        )));
    }

    package
        .policies
        .iter()
        .find(|policy| policy.policy_id == reference.policy_id)
        .cloned()
        .ok_or_else(|| {
            missing_policy_error(format!("unknown evidence policy {}", reference.policy_id))
        })
}

pub fn validate_package_for_execution(
    package: &EvidencePolicyPackage,
    required_policies: &[EvidencePolicyRef],
    capabilities: &EvidenceCapabilityCatalog,
) -> EvidencePolicyResult<String> {
    let package = finalize_package(package.clone())?;
    let digest = evidence_policy_package_digest(&package)?;
    for reference in required_policies {
        let reference = finalize_policy_ref(reference.clone())?;
        if reference.package_digest != digest {
            return Err(compatibility_policy_error(format!(
                "evidence policy {} is pinned to {} but package resolved to {}",
                reference.policy_id, reference.package_digest, digest
            )));
        }
        let _ = compile_policy(&package, &reference, capabilities)?;
    }
    Ok(digest)
}

pub fn package_compatibility(
    locked: &EvidencePolicyPackage,
    candidate: &EvidencePolicyPackage,
    used_policies: &[EvidencePolicyRef],
) -> EvidencePolicyResult<EvidencePolicyCompatibility> {
    let locked = finalize_package(locked.clone())?;
    let candidate = finalize_package(candidate.clone())?;

    if locked.package_id != candidate.package_id {
        return Err(compatibility_policy_error(format!(
            "evidence policy package ids differ: {} vs {}",
            locked.package_id, candidate.package_id
        )));
    }

    let locked_major = semver_major(&locked.package_version)?;
    let candidate_major = semver_major(&candidate.package_version)?;
    if locked_major != candidate_major {
        return Err(compatibility_policy_error(format!(
            "evidence policy package {} changed major version from {} to {}",
            locked.package_id, locked_major, candidate_major
        )));
    }

    let locked_digest = evidence_policy_package_digest(&locked)?;
    let candidate_digest = evidence_policy_package_digest(&candidate)?;
    for reference in used_policies {
        let reference = finalize_policy_ref(reference.clone())?;
        if reference.package_digest != locked_digest {
            return Err(compatibility_policy_error(format!(
                "evidence policy {} is not pinned to locked package digest {}",
                reference.policy_id, locked_digest
            )));
        }
        if candidate
            .policies
            .iter()
            .all(|policy| policy.policy_id != reference.policy_id)
        {
            return Err(missing_policy_error(format!(
                "candidate evidence policy package {} no longer defines {}",
                candidate.package_id, reference.policy_id
            )));
        }
    }

    Ok(if locked_digest == candidate_digest {
        EvidencePolicyCompatibility::ExactDigest
    } else {
        EvidencePolicyCompatibility::CompatibleSameMajor
    })
}

pub fn compile_policy(
    package: &EvidencePolicyPackage,
    reference: &EvidencePolicyRef,
    capabilities: &EvidenceCapabilityCatalog,
) -> EvidencePolicyResult<CompiledEvidencePolicy> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_policy_ref(reference.clone())?;
    let capabilities = normalize_capability_catalog(capabilities.clone())?;
    let digest = evidence_policy_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "evidence policy {} is pinned to {} but package resolved to {}",
            reference.policy_id, reference.package_digest, digest
        )));
    }

    let policy = package
        .policies
        .iter()
        .find(|policy| policy.policy_id == reference.policy_id)
        .cloned()
        .ok_or_else(|| {
            missing_policy_error(format!("unknown evidence policy {}", reference.policy_id))
        })?;

    for rule in &policy.candidate_rules {
        require_supported_primitive(
            &capabilities.supported_primitives,
            rule.primitive,
            &rule.rule_id,
            "candidate",
        )?;
        require_supported_candidate_target(
            &capabilities.supported_candidate_targets,
            rule.target_kind,
            &rule.rule_id,
        )?;
        require_selector_capabilities(&capabilities, &rule.selectors, &rule.rule_id)?;
    }

    for rule in &policy.evidence_rules {
        require_supported_primitive(
            &capabilities.supported_primitives,
            rule.primitive,
            &rule.rule_id,
            "evidence",
        )?;
        require_supported_evidence_kind(
            &capabilities.supported_evidence_kinds,
            rule.emits_kind,
            &rule.rule_id,
        )?;
        require_selector_capabilities(&capabilities, &rule.selectors, &rule.rule_id)?;
    }

    Ok(CompiledEvidencePolicy {
        package_digest: digest,
        policy,
    })
}

pub fn evaluate_triggered_rules(
    policy: &CompiledEvidencePolicy,
    triggered_rule_ids: &[String],
) -> EvidencePolicyResult<PolicyDecision> {
    let mut triggered = triggered_rule_ids
        .iter()
        .map(|rule_id| normalized_opaque_ref(rule_id, "triggered_rule_id"))
        .collect::<EvidencePolicyResult<Vec<_>>>()?;
    triggered.sort();
    triggered.dedup();

    let mut rule_index = std::collections::BTreeMap::new();
    for rule in &policy.policy.evidence_rules {
        rule_index.insert(rule.rule_id.clone(), rule);
    }
    for rule_id in &triggered {
        if !rule_index.contains_key(rule_id) {
            return Err(missing_rule_error(format!(
                "triggered rule {} is not defined by policy {}",
                rule_id, policy.policy.policy_id
            )));
        }
    }

    let hard_veto_ids = triggered
        .iter()
        .filter(|rule_id| {
            matches!(
                rule_index.get(*rule_id).map(|rule| rule.lane),
                Some(EvidenceRuleLane::HardVeto)
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if !hard_veto_ids.is_empty() {
        return Ok(PolicyDecision {
            outcome: DecisionOutcome::HardVeto,
            matched_rule_ids: hard_veto_ids,
            gate_id: None,
            authority_id: None,
            review_artifact_family: None,
            allowed_review_dispositions: vec![],
        });
    }

    let auto_matches = policy
        .policy
        .auto_decision_rules
        .iter()
        .filter(|decision| gate_is_satisfied(&policy.policy, &decision.gate_id, &triggered))
        .collect::<Vec<_>>();
    if !auto_matches.is_empty() {
        let first = auto_matches[0];
        if auto_matches.len() > 1
            || auto_matches
                .iter()
                .any(|decision| decision.outcome != first.outcome)
        {
            return Err(decision_conflict_error(
                "multiple auto-decision clauses matched the same trigger set",
            ));
        }
        let gate = find_gate(&policy.policy, &first.gate_id)?;
        return Ok(PolicyDecision {
            outcome: match first.outcome {
                AutoDecisionOutcome::Merge => DecisionOutcome::AutoMerge,
                AutoDecisionOutcome::Distinct => DecisionOutcome::AutoDistinct,
            },
            matched_rule_ids: matched_gate_rule_ids(gate, &triggered),
            gate_id: Some(first.gate_id.clone()),
            authority_id: Some(first.decision_id.clone()),
            review_artifact_family: None,
            allowed_review_dispositions: vec![],
        });
    }

    let abstention_matches = policy
        .policy
        .abstention_rules
        .iter()
        .filter(|rule| gate_is_satisfied(&policy.policy, &rule.gate_id, &triggered))
        .collect::<Vec<_>>();
    if !abstention_matches.is_empty() {
        if abstention_matches.len() > 1 {
            return Err(decision_conflict_error(
                "multiple abstention clauses matched the same trigger set",
            ));
        }
        let rule = abstention_matches[0];
        let gate = find_gate(&policy.policy, &rule.gate_id)?;
        return Ok(PolicyDecision {
            outcome: match rule.disposition {
                AbstentionDisposition::InsufficientEvidence => {
                    DecisionOutcome::AbstainInsufficientEvidence
                }
                AbstentionDisposition::Conflict => DecisionOutcome::AbstainConflict,
                AbstentionDisposition::ResourceLimit => DecisionOutcome::AbstainResourceLimit,
            },
            matched_rule_ids: matched_gate_rule_ids(gate, &triggered),
            gate_id: Some(rule.gate_id.clone()),
            authority_id: Some(rule.abstention_id.clone()),
            review_artifact_family: None,
            allowed_review_dispositions: vec![],
        });
    }

    let review_matches = policy
        .policy
        .review_rules
        .iter()
        .filter(|rule| gate_is_satisfied(&policy.policy, &rule.gate_id, &triggered))
        .collect::<Vec<_>>();
    if !review_matches.is_empty() {
        if review_matches.len() > 1 {
            return Err(decision_conflict_error(
                "multiple review clauses matched the same trigger set",
            ));
        }
        let rule = review_matches[0];
        let gate = find_gate(&policy.policy, &rule.gate_id)?;
        return Ok(PolicyDecision {
            outcome: DecisionOutcome::ReviewRequired,
            matched_rule_ids: matched_gate_rule_ids(gate, &triggered),
            gate_id: Some(rule.gate_id.clone()),
            authority_id: Some(rule.review_id.clone()),
            review_artifact_family: Some(rule.artifact_family.clone()),
            allowed_review_dispositions: rule.allowed_dispositions.clone(),
        });
    }

    Ok(PolicyDecision {
        outcome: DecisionOutcome::NoDecision,
        matched_rule_ids: triggered,
        gate_id: None,
        authority_id: None,
        review_artifact_family: None,
        allowed_review_dispositions: vec![],
    })
}

fn normalize_policy(
    mut policy: EvidencePolicyDefinition,
    package_id: &str,
    known_docs: &BTreeSet<String>,
) -> EvidencePolicyResult<EvidencePolicyDefinition> {
    policy.policy_id = normalized_opaque_ref(&policy.policy_id, "policy_id")?;
    if !policy.policy_id.starts_with(&format!("{package_id}:")) {
        return Err(artifact_contract_error(format!(
            "policy {} must be namespaced under package {}",
            policy.policy_id, package_id
        )));
    }

    policy.candidate_rules = dedupe_components(
        policy
            .candidate_rules
            .into_iter()
            .map(|rule| normalize_candidate_rule(rule, package_id))
            .collect::<EvidencePolicyResult<Vec<_>>>()?,
        |rule| rule.rule_id.clone(),
        "candidate rule",
    )?;
    policy.evidence_rules = dedupe_components(
        policy
            .evidence_rules
            .into_iter()
            .map(|rule| normalize_evidence_rule(rule, package_id))
            .collect::<EvidencePolicyResult<Vec<_>>>()?,
        |rule| rule.rule_id.clone(),
        "evidence rule",
    )?;
    if policy.evidence_rules.is_empty() {
        return Err(artifact_contract_error(format!(
            "policy {} must declare at least one evidence rule",
            policy.policy_id
        )));
    }
    let evidence_rule_ids = policy
        .evidence_rules
        .iter()
        .map(|rule| rule.rule_id.clone())
        .collect::<BTreeSet<_>>();

    policy.decision_gates = dedupe_components(
        policy
            .decision_gates
            .into_iter()
            .map(|gate| normalize_decision_gate(gate, package_id, &evidence_rule_ids))
            .collect::<EvidencePolicyResult<Vec<_>>>()?,
        |gate| gate.gate_id.clone(),
        "decision gate",
    )?;
    let gate_ids = policy
        .decision_gates
        .iter()
        .map(|gate| gate.gate_id.clone())
        .collect::<BTreeSet<_>>();

    policy.auto_decision_rules = dedupe_components(
        policy
            .auto_decision_rules
            .into_iter()
            .map(|rule| normalize_auto_decision_rule(rule, package_id, &gate_ids))
            .collect::<EvidencePolicyResult<Vec<_>>>()?,
        |rule| rule.decision_id.clone(),
        "auto-decision rule",
    )?;
    policy.abstention_rules = dedupe_components(
        policy
            .abstention_rules
            .into_iter()
            .map(|rule| normalize_abstention_rule(rule, package_id, &gate_ids))
            .collect::<EvidencePolicyResult<Vec<_>>>()?,
        |rule| rule.abstention_id.clone(),
        "abstention rule",
    )?;
    policy.review_rules = dedupe_components(
        policy
            .review_rules
            .into_iter()
            .map(|rule| normalize_review_rule(rule, package_id, &gate_ids))
            .collect::<EvidencePolicyResult<Vec<_>>>()?,
        |rule| rule.review_id.clone(),
        "review rule",
    )?;
    policy.documentation_refs = normalize_documentation_ref_list(
        policy.documentation_refs,
        known_docs,
        "documentation_refs",
    )?;

    Ok(policy)
}

fn normalize_documentation_ref(
    mut reference: EvidencePolicyDocumentationRef,
) -> EvidencePolicyResult<EvidencePolicyDocumentationRef> {
    reference.label = normalized_non_empty(&reference.label, "documentation.label")?;
    reference.uri = normalized_documentation_uri(&reference.uri, "documentation.uri")?;
    Ok(reference)
}

fn normalize_candidate_rule(
    mut rule: CandidateRule,
    package_id: &str,
) -> EvidencePolicyResult<CandidateRule> {
    rule.rule_id = normalized_opaque_ref(&rule.rule_id, "candidate_rule.rule_id")?;
    require_package_prefix(&rule.rule_id, package_id, "candidate rule")?;
    rule.selectors = normalize_selector_scope(rule.selectors, "candidate_rule.selectors", false)?;
    Ok(rule)
}

fn normalize_evidence_rule(
    mut rule: EvidenceRule,
    package_id: &str,
) -> EvidencePolicyResult<EvidenceRule> {
    rule.rule_id = normalized_opaque_ref(&rule.rule_id, "evidence_rule.rule_id")?;
    require_package_prefix(&rule.rule_id, package_id, "evidence rule")?;
    rule.reason_code = normalized_non_empty(&rule.reason_code, "evidence_rule.reason_code")?;
    rule.selectors = normalize_selector_scope(rule.selectors, "evidence_rule.selectors", false)?;
    validate_lane_output_contract(rule.lane, rule.emits_kind, &rule.rule_id)?;
    Ok(rule)
}

fn validate_lane_output_contract(
    lane: EvidenceRuleLane,
    emits_kind: EvidenceIrKind,
    rule_id: &str,
) -> EvidencePolicyResult<()> {
    let valid = match lane {
        EvidenceRuleLane::Positive => matches!(
            emits_kind,
            EvidenceIrKind::PairSupport
                | EvidenceIrKind::HyperedgeSupport
                | EvidenceIrKind::RecordLinkSupport
        ),
        EvidenceRuleLane::Negative => matches!(emits_kind, EvidenceIrKind::ContextualNegative),
        EvidenceRuleLane::Contextual => matches!(emits_kind, EvidenceIrKind::ContextOnly),
        EvidenceRuleLane::Missing => matches!(emits_kind, EvidenceIrKind::Missingness),
        EvidenceRuleLane::HardVeto => matches!(emits_kind, EvidenceIrKind::AntiMergeVeto),
    };
    if valid {
        Ok(())
    } else {
        Err(artifact_contract_error(format!(
            "rule {} lane {:?} is incompatible with evidence IR kind {:?}",
            rule_id, lane, emits_kind
        )))
    }
}

fn normalize_decision_gate(
    mut gate: DecisionGate,
    package_id: &str,
    evidence_rule_ids: &BTreeSet<String>,
) -> EvidencePolicyResult<DecisionGate> {
    gate.gate_id = normalized_opaque_ref(&gate.gate_id, "decision_gate.gate_id")?;
    require_package_prefix(&gate.gate_id, package_id, "decision gate")?;
    gate.require_rule_ids =
        normalize_rule_id_list(gate.require_rule_ids, "decision_gate.require_rule_ids")?;
    gate.forbid_rule_ids =
        normalize_rule_id_list(gate.forbid_rule_ids, "decision_gate.forbid_rule_ids")?;
    if gate.minimum_trigger_count == Some(0) {
        return Err(artifact_contract_error(
            "decision_gate.minimum_trigger_count must be absent or >= 1",
        ));
    }
    if gate.require_rule_ids.is_empty()
        && gate.forbid_rule_ids.is_empty()
        && gate.minimum_trigger_count.is_none()
    {
        return Err(artifact_contract_error(format!(
            "decision gate {} must declare at least one trigger condition",
            gate.gate_id
        )));
    }
    for rule_id in gate
        .require_rule_ids
        .iter()
        .chain(gate.forbid_rule_ids.iter())
    {
        if !evidence_rule_ids.contains(rule_id) {
            return Err(missing_rule_error(format!(
                "decision gate {} references unknown evidence rule {}",
                gate.gate_id, rule_id
            )));
        }
    }
    if gate
        .require_rule_ids
        .iter()
        .any(|rule_id| gate.forbid_rule_ids.contains(rule_id))
    {
        return Err(artifact_contract_error(format!(
            "decision gate {} cannot require and forbid the same rule",
            gate.gate_id
        )));
    }
    Ok(gate)
}

fn normalize_auto_decision_rule(
    mut rule: AutoDecisionRule,
    package_id: &str,
    gate_ids: &BTreeSet<String>,
) -> EvidencePolicyResult<AutoDecisionRule> {
    rule.decision_id = normalized_opaque_ref(&rule.decision_id, "auto_decision.decision_id")?;
    require_package_prefix(&rule.decision_id, package_id, "auto-decision rule")?;
    rule.gate_id = normalized_opaque_ref(&rule.gate_id, "auto_decision.gate_id")?;
    rule.authority_label =
        normalized_non_empty(&rule.authority_label, "auto_decision.authority_label")?;
    if !gate_ids.contains(&rule.gate_id) {
        return Err(missing_rule_error(format!(
            "auto-decision rule {} references unknown decision gate {}",
            rule.decision_id, rule.gate_id
        )));
    }
    Ok(rule)
}

fn normalize_abstention_rule(
    mut rule: AbstentionRule,
    package_id: &str,
    gate_ids: &BTreeSet<String>,
) -> EvidencePolicyResult<AbstentionRule> {
    rule.abstention_id =
        normalized_opaque_ref(&rule.abstention_id, "abstention_rule.abstention_id")?;
    require_package_prefix(&rule.abstention_id, package_id, "abstention rule")?;
    rule.gate_id = normalized_opaque_ref(&rule.gate_id, "abstention_rule.gate_id")?;
    rule.reason_code = normalized_non_empty(&rule.reason_code, "abstention_rule.reason_code")?;
    if !gate_ids.contains(&rule.gate_id) {
        return Err(missing_rule_error(format!(
            "abstention rule {} references unknown decision gate {}",
            rule.abstention_id, rule.gate_id
        )));
    }
    Ok(rule)
}

fn normalize_review_rule(
    mut rule: ReviewRule,
    package_id: &str,
    gate_ids: &BTreeSet<String>,
) -> EvidencePolicyResult<ReviewRule> {
    rule.review_id = normalized_opaque_ref(&rule.review_id, "review_rule.review_id")?;
    require_package_prefix(&rule.review_id, package_id, "review rule")?;
    rule.gate_id = normalized_opaque_ref(&rule.gate_id, "review_rule.gate_id")?;
    rule.artifact_family =
        normalized_non_empty(&rule.artifact_family, "review_rule.artifact_family")?;
    if !gate_ids.contains(&rule.gate_id) {
        return Err(missing_rule_error(format!(
            "review rule {} references unknown decision gate {}",
            rule.review_id, rule.gate_id
        )));
    }
    let mut allowed = rule.allowed_dispositions;
    allowed.sort();
    allowed.dedup();
    if allowed.is_empty() {
        return Err(artifact_contract_error(format!(
            "review rule {} must declare at least one review disposition",
            rule.review_id
        )));
    }
    rule.allowed_dispositions = allowed;
    Ok(rule)
}

fn normalize_selector_scope(
    mut selectors: EvidenceSelectorScope,
    field: &str,
    allow_empty: bool,
) -> EvidencePolicyResult<EvidenceSelectorScope> {
    selectors.field_refs =
        normalize_opaque_ref_list(selectors.field_refs, &format!("{field}.field_refs"))?;
    selectors.view_refs =
        normalize_opaque_ref_list(selectors.view_refs, &format!("{field}.view_refs"))?;
    selectors.namespace_refs =
        normalize_opaque_ref_list(selectors.namespace_refs, &format!("{field}.namespace_refs"))?;
    selectors.relation_refs =
        normalize_opaque_ref_list(selectors.relation_refs, &format!("{field}.relation_refs"))?;
    if !allow_empty
        && selectors.field_refs.is_empty()
        && selectors.view_refs.is_empty()
        && selectors.namespace_refs.is_empty()
        && selectors.relation_refs.is_empty()
    {
        return Err(artifact_contract_error(format!(
            "{field} must reference at least one field, view, namespace, or relation"
        )));
    }
    Ok(selectors)
}

fn normalize_rule_id_list(values: Vec<String>, field: &str) -> EvidencePolicyResult<Vec<String>> {
    normalize_opaque_ref_list(values, field)
}

fn normalize_capability_catalog(
    mut capabilities: EvidenceCapabilityCatalog,
) -> EvidencePolicyResult<EvidenceCapabilityCatalog> {
    capabilities.field_refs =
        normalize_opaque_ref_list(capabilities.field_refs, "capabilities.field_refs")?;
    capabilities.view_refs =
        normalize_opaque_ref_list(capabilities.view_refs, "capabilities.view_refs")?;
    capabilities.namespace_refs =
        normalize_opaque_ref_list(capabilities.namespace_refs, "capabilities.namespace_refs")?;
    capabilities.relation_refs =
        normalize_opaque_ref_list(capabilities.relation_refs, "capabilities.relation_refs")?;
    sort_and_dedup(&mut capabilities.supported_primitives);
    sort_and_dedup(&mut capabilities.supported_candidate_targets);
    sort_and_dedup(&mut capabilities.supported_evidence_kinds);
    Ok(capabilities)
}

fn require_supported_primitive(
    supported: &[EvidencePrimitive],
    primitive: EvidencePrimitive,
    rule_id: &str,
    lane: &str,
) -> EvidencePolicyResult<()> {
    if supported.contains(&primitive) {
        Ok(())
    } else {
        Err(unsupported_capability_error(format!(
            "{lane} rule {rule_id} requires unsupported primitive {:?}",
            primitive
        )))
    }
}

fn require_supported_candidate_target(
    supported: &[CandidateTargetKind],
    target_kind: CandidateTargetKind,
    rule_id: &str,
) -> EvidencePolicyResult<()> {
    if supported.contains(&target_kind) {
        Ok(())
    } else {
        Err(unsupported_capability_error(format!(
            "candidate rule {rule_id} requires unsupported target kind {:?}",
            target_kind
        )))
    }
}

fn require_supported_evidence_kind(
    supported: &[EvidenceIrKind],
    emits_kind: EvidenceIrKind,
    rule_id: &str,
) -> EvidencePolicyResult<()> {
    if supported.contains(&emits_kind) {
        Ok(())
    } else {
        Err(unsupported_capability_error(format!(
            "evidence rule {rule_id} requires unsupported evidence IR kind {:?}",
            emits_kind
        )))
    }
}

fn require_selector_capabilities(
    capabilities: &EvidenceCapabilityCatalog,
    selectors: &EvidenceSelectorScope,
    rule_id: &str,
) -> EvidencePolicyResult<()> {
    for (refs, supported, label) in [
        (&selectors.field_refs, &capabilities.field_refs, "field"),
        (&selectors.view_refs, &capabilities.view_refs, "view"),
        (
            &selectors.namespace_refs,
            &capabilities.namespace_refs,
            "namespace",
        ),
        (
            &selectors.relation_refs,
            &capabilities.relation_refs,
            "relation",
        ),
    ] {
        for reference in refs {
            if !supported.contains(reference) {
                return Err(unsupported_capability_error(format!(
                    "rule {rule_id} references unsupported {label} {reference}"
                )));
            }
        }
    }
    Ok(())
}

fn gate_is_satisfied(
    policy: &EvidencePolicyDefinition,
    gate_id: &str,
    triggered_rule_ids: &[String],
) -> bool {
    let Ok(gate) = find_gate(policy, gate_id) else {
        return false;
    };

    if gate
        .require_rule_ids
        .iter()
        .any(|rule_id| !triggered_rule_ids.contains(rule_id))
    {
        return false;
    }
    if gate
        .forbid_rule_ids
        .iter()
        .any(|rule_id| triggered_rule_ids.contains(rule_id))
    {
        return false;
    }
    if let Some(minimum) = gate.minimum_trigger_count {
        let counted = if gate.require_rule_ids.is_empty() {
            triggered_rule_ids.len()
        } else {
            gate.require_rule_ids.len()
        };
        if counted < minimum {
            return false;
        }
    }
    true
}

fn matched_gate_rule_ids(gate: &DecisionGate, triggered_rule_ids: &[String]) -> Vec<String> {
    if gate.require_rule_ids.is_empty() {
        triggered_rule_ids.to_vec()
    } else {
        gate.require_rule_ids
            .iter()
            .filter(|rule_id| triggered_rule_ids.contains(*rule_id))
            .cloned()
            .collect()
    }
}

fn find_gate<'a>(
    policy: &'a EvidencePolicyDefinition,
    gate_id: &str,
) -> EvidencePolicyResult<&'a DecisionGate> {
    policy
        .decision_gates
        .iter()
        .find(|gate| gate.gate_id == gate_id)
        .ok_or_else(|| missing_rule_error(format!("unknown decision gate {gate_id}")))
}

fn normalize_documentation_ref_list(
    values: Vec<String>,
    known_docs: &BTreeSet<String>,
    field: &str,
) -> EvidencePolicyResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_documentation_uri(&value, field))
        .collect::<EvidencePolicyResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    for value in &normalized {
        if !known_docs.contains(value) {
            return Err(artifact_contract_error(format!(
                "documentation ref {} is not present in package documentation",
                value
            )));
        }
    }
    Ok(normalized)
}

fn normalize_opaque_ref_list(
    values: Vec<String>,
    field: &str,
) -> EvidencePolicyResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_opaque_ref(&value, field))
        .collect::<EvidencePolicyResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn dedupe_components<T, F>(
    mut values: Vec<T>,
    key_fn: F,
    label: &str,
) -> EvidencePolicyResult<Vec<T>>
where
    T: Clone + PartialEq,
    F: Fn(&T) -> String,
{
    values.sort_by_key(|value| key_fn(value));

    let mut deduped = Vec::with_capacity(values.len());
    for value in values {
        if let Some(previous) = deduped.last()
            && key_fn(previous) == key_fn(&value)
        {
            if previous != &value {
                return Err(artifact_contract_error(format!(
                    "{label} {} cannot be declared with conflicting content",
                    key_fn(&value)
                )));
            }
            continue;
        }
        deduped.push(value);
    }
    Ok(deduped)
}

fn require_package_prefix(value: &str, package_id: &str, label: &str) -> EvidencePolicyResult<()> {
    if value.starts_with(&format!("{package_id}:")) {
        Ok(())
    } else {
        Err(artifact_contract_error(format!(
            "{label} {value} must be namespaced under package {package_id}"
        )))
    }
}

fn normalized_package_id(value: &str, field: &str) -> EvidencePolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    if !normalized.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err(artifact_contract_error(format!(
            "{field} must use lowercase [a-z0-9._-] characters"
        )));
    }
    Ok(normalized)
}

fn normalized_semver(value: &str, field: &str) -> EvidencePolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let parts = normalized.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(artifact_contract_error(format!(
            "{field} must use MAJOR.MINOR.PATCH numeric semver"
        )));
    }
    Ok(normalized)
}

fn semver_major(value: &str) -> EvidencePolicyResult<u64> {
    value
        .split('.')
        .next()
        .ok_or_else(|| artifact_contract_error("semver missing major version"))?
        .parse::<u64>()
        .map_err(|error| artifact_contract_error(format!("invalid semver major version: {error}")))
}

fn normalized_opaque_ref(value: &str, field: &str) -> EvidencePolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let Some((namespace, local)) = normalized.split_once(':') else {
        return Err(artifact_contract_error(format!(
            "{field} must use namespaced <namespace>:<local> format"
        )));
    };
    if namespace.is_empty()
        || local.is_empty()
        || !namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        || !local.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(artifact_contract_error(format!(
            "{field} must use namespaced <namespace>:<local> format"
        )));
    }
    Ok(normalized)
}

fn normalized_documentation_uri(value: &str, field: &str) -> EvidencePolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    if normalized.starts_with('/')
        || normalized.contains('\\')
        || normalized.split('/').any(|part| part == "..")
    {
        return Err(artifact_contract_error(format!(
            "{field} cannot be absolute or contain traversal segments"
        )));
    }
    if normalized.contains(':')
        && !normalized.starts_with("http://")
        && !normalized.starts_with("https://")
    {
        return Err(artifact_contract_error(format!(
            "{field} must be relative or http(s)"
        )));
    }
    Ok(normalized)
}

fn normalized_non_empty(value: &str, field: &str) -> EvidencePolicyResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty after trimming"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_hash(value: &str, field: &str) -> EvidencePolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let Some(hex) = normalized.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must use blake3:<lower-hex-64> format"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(artifact_contract_error(format!(
            "{field} must use blake3:<lower-hex-64> format"
        )));
    }
    Ok(normalized)
}

fn sort_and_dedup<T>(values: &mut Vec<T>)
where
    T: Ord,
{
    values.sort();
    values.dedup();
}

fn artifact_contract_error(message: impl Into<String>) -> EvidencePolicyError {
    EvidencePolicyError::new(EvidencePolicyErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> EvidencePolicyError {
    EvidencePolicyError::new(EvidencePolicyErrorCode::CompatibilityPolicy, message)
}

fn missing_policy_error(message: impl Into<String>) -> EvidencePolicyError {
    EvidencePolicyError::new(EvidencePolicyErrorCode::MissingPolicy, message)
}

fn missing_rule_error(message: impl Into<String>) -> EvidencePolicyError {
    EvidencePolicyError::new(EvidencePolicyErrorCode::MissingRule, message)
}

fn unsupported_capability_error(message: impl Into<String>) -> EvidencePolicyError {
    EvidencePolicyError::new(EvidencePolicyErrorCode::UnsupportedCapability, message)
}

fn decision_conflict_error(message: impl Into<String>) -> EvidencePolicyError {
    EvidencePolicyError::new(EvidencePolicyErrorCode::DecisionConflict, message)
}
