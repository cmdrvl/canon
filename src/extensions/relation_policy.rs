#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
};

pub const CANON_RELATION_POLICY_VERSION: &str = "canon.relation.policy.v1";

pub type RelationPolicyResult<T> = Result<T, RelationPolicyError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationPolicyErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingPolicy,
    ConstraintViolation,
    TemporalBoundary,
    GraphPolicy,
    MergeGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPolicyError {
    pub code: RelationPolicyErrorCode,
    pub message: String,
}

impl RelationPolicyError {
    pub fn new(code: RelationPolicyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for RelationPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for RelationPolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationPolicyCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationOrientation {
    Directed,
    UnorderedPair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityBoundaryPolicy {
    None,
    ExplicitEqualityFactOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuccessionTemporalPolicy {
    Ignore,
    RequireNonOverlappingTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitiveMergeGuard {
    Allow,
    Review,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationCyclePolicy {
    Allow,
    Review,
    Disallow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Same,
    Distinct,
    Related,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeDisposition {
    AllowWithExplicitEqualityFact,
    NeedsExplicitEqualityFact,
    BlockReviewedDistinct,
    BlockRelatedDistinct,
    BlockTransitivePressure,
    TemporalSuccessionOnly,
    ReviewRequired,
    NoOpinion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPolicyPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<RelationPolicyDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<RelationPolicyDocumentationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelationPolicyDocumentationRef {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPolicyDefinition {
    pub policy_id: String,
    pub relation_type_id: String,
    pub orientation: RelationOrientation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_type_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_type_refs: Vec<String>,
    pub related_distinct_veto: bool,
    pub identity_boundary: IdentityBoundaryPolicy,
    pub succession_policy: SuccessionTemporalPolicy,
    pub transitive_merge_guard: TransitiveMergeGuard,
    pub graph: RelationGraphPolicy,
    pub review: RelationReviewContract,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationGraphPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_objects_per_subject: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_subjects_per_object: Option<usize>,
    pub cycle_policy: RelationCyclePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationReviewContract {
    pub decision_artifact_family: String,
    pub cannot_link_artifact_family: String,
    pub distinct_writes_cannot_link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelationPolicyRef {
    pub package_digest: String,
    pub policy_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationObservation {
    pub relation_id: String,
    pub left_id: String,
    pub left_type_ref: String,
    pub right_id: String,
    pub right_type_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_valid_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_valid_from: Option<String>,
    pub review_decision: ReviewDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationMergeRequest {
    pub candidate_left_id: String,
    pub candidate_right_id: String,
    pub explicit_equality_fact: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<RelationObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationDecisionArtifact {
    pub artifact_family: String,
    pub artifact_kind: String,
    pub relation_id: String,
    pub pair_key: String,
    pub review_decision: ReviewDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationCannotLinkPatch {
    pub artifact_family: String,
    pub pair_key: String,
    pub policy_id: String,
    pub relation_type_id: String,
    pub decision_artifact_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationMergeOutcome {
    pub pair_key: String,
    pub pair_ids: [String; 2],
    pub disposition: MergeDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_artifact: Option<RelationDecisionArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cannot_link_patch: Option<RelationCannotLinkPatch>,
}

pub fn relation_policy_schema_version() -> &'static str {
    CANON_RELATION_POLICY_VERSION
}

pub fn finalize_package(
    mut package: RelationPolicyPackage,
) -> RelationPolicyResult<RelationPolicyPackage> {
    if package.version.trim().is_empty() {
        package.version = CANON_RELATION_POLICY_VERSION.to_string();
    }
    if package.version != CANON_RELATION_POLICY_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported relation policy contract version: {}",
            package.version
        )));
    }

    package.package_id = normalized_package_id(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;

    let mut documentation = package
        .documentation
        .into_iter()
        .map(normalize_documentation_ref)
        .collect::<RelationPolicyResult<Vec<_>>>()?;
    documentation.sort();
    documentation.dedup();
    let known_docs = documentation
        .iter()
        .map(|entry| entry.uri.clone())
        .collect::<BTreeSet<_>>();

    let mut policies = package
        .policies
        .into_iter()
        .map(|policy| normalize_policy(policy, &known_docs))
        .collect::<RelationPolicyResult<Vec<_>>>()?;
    if policies.is_empty() {
        return Err(artifact_contract_error(
            "relation policy package must declare at least one policy",
        ));
    }
    policies.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));

    let mut deduped: Vec<RelationPolicyDefinition> = Vec::with_capacity(policies.len());
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

pub fn canonical_package_bytes(package: &RelationPolicyPackage) -> RelationPolicyResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize relation policy package: {error}"
        ))
    })
}

pub fn relation_policy_package_digest(
    package: &RelationPolicyPackage,
) -> RelationPolicyResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn finalize_policy_ref(
    mut reference: RelationPolicyRef,
) -> RelationPolicyResult<RelationPolicyRef> {
    reference.package_digest = normalized_hash(&reference.package_digest, "package_digest")?;
    reference.policy_id = normalized_opaque_ref(&reference.policy_id, "policy_id")?;
    Ok(reference)
}

pub fn resolve_policy_ref(
    package: &RelationPolicyPackage,
    reference: &RelationPolicyRef,
) -> RelationPolicyResult<RelationPolicyDefinition> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_policy_ref(reference.clone())?;
    let digest = relation_policy_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "relation policy {} is pinned to {} but package resolved to {}",
            reference.policy_id, reference.package_digest, digest
        )));
    }

    package
        .policies
        .iter()
        .find(|policy| policy.policy_id == reference.policy_id)
        .cloned()
        .ok_or_else(|| {
            missing_policy_error(format!("unknown relation policy {}", reference.policy_id))
        })
}

pub fn validate_package_for_execution(
    package: &RelationPolicyPackage,
    required_policies: &[RelationPolicyRef],
) -> RelationPolicyResult<String> {
    let package = finalize_package(package.clone())?;
    let digest = relation_policy_package_digest(&package)?;
    for reference in required_policies {
        resolve_policy_ref(&package, reference)?;
    }
    Ok(digest)
}

pub fn package_compatibility(
    locked: &RelationPolicyPackage,
    candidate: &RelationPolicyPackage,
    locked_refs: &[RelationPolicyRef],
) -> RelationPolicyResult<RelationPolicyCompatibility> {
    let locked = finalize_package(locked.clone())?;
    let candidate = finalize_package(candidate.clone())?;

    validate_package_for_execution(&locked, locked_refs)?;
    let locked_digest = relation_policy_package_digest(&locked)?;
    let candidate_digest = relation_policy_package_digest(&candidate)?;
    if locked_digest == candidate_digest {
        return Ok(RelationPolicyCompatibility::ExactDigest);
    }

    if locked.package_id != candidate.package_id {
        return Err(compatibility_policy_error(format!(
            "relation policy package id changed from {} to {}",
            locked.package_id, candidate.package_id
        )));
    }
    if semver_major(&locked.package_version) != semver_major(&candidate.package_version) {
        return Err(compatibility_policy_error(format!(
            "relation policy package major version changed from {} to {}",
            locked.package_version, candidate.package_version
        )));
    }
    for reference in locked_refs {
        let locked_reference = finalize_policy_ref(reference.clone())?;
        let migrated = RelationPolicyRef {
            package_digest: candidate_digest.clone(),
            policy_id: locked_reference.policy_id,
        };
        resolve_policy_ref(&candidate, &migrated)?;
    }
    Ok(RelationPolicyCompatibility::CompatibleSameMajor)
}

pub fn evaluate_merge_request(
    package: &RelationPolicyPackage,
    reference: &RelationPolicyRef,
    request: &RelationMergeRequest,
) -> RelationPolicyResult<RelationMergeOutcome> {
    let package = finalize_package(package.clone())?;
    let policy = resolve_policy_ref(&package, reference)?;
    let request = normalize_merge_request(request.clone())?;

    validate_observations(&policy, &request.observations)?;

    let pair_ids = sorted_pair(
        request.candidate_left_id.as_str(),
        request.candidate_right_id.as_str(),
    );
    let pair_key = pair_key(&pair_ids[0], &pair_ids[1])?;

    let direct = request
        .observations
        .iter()
        .filter(|observation| {
            let observation_pair =
                sorted_pair(observation.left_id.as_str(), observation.right_id.as_str());
            observation_pair == pair_ids
        })
        .collect::<Vec<_>>();

    if let Some(outcome) =
        direct_decision_outcome(&policy, &pair_key, &direct, request.explicit_equality_fact)?
    {
        return Ok(outcome);
    }

    if direct.is_empty() {
        match transitive_pressure_disposition(&policy, &request.observations, &pair_ids)? {
            Some(MergeDisposition::BlockTransitivePressure) => {
                return Ok(RelationMergeOutcome {
                    pair_key,
                    pair_ids,
                    disposition: MergeDisposition::BlockTransitivePressure,
                    decision_artifact: None,
                    cannot_link_patch: None,
                });
            }
            Some(MergeDisposition::ReviewRequired) => {
                return Ok(RelationMergeOutcome {
                    pair_key,
                    pair_ids,
                    disposition: MergeDisposition::ReviewRequired,
                    decision_artifact: None,
                    cannot_link_patch: None,
                });
            }
            Some(_) | None => {}
        }
    }

    Ok(RelationMergeOutcome {
        pair_key,
        pair_ids,
        disposition: MergeDisposition::NoOpinion,
        decision_artifact: None,
        cannot_link_patch: None,
    })
}

fn normalize_policy(
    mut policy: RelationPolicyDefinition,
    known_docs: &BTreeSet<String>,
) -> RelationPolicyResult<RelationPolicyDefinition> {
    policy.policy_id = normalized_opaque_ref(&policy.policy_id, "policy_id")?;
    policy.relation_type_id = normalized_opaque_ref(&policy.relation_type_id, "relation_type_id")?;
    policy.subject_type_refs =
        normalize_opaque_ref_list(policy.subject_type_refs, "subject_type_refs")?;
    policy.object_type_refs =
        normalize_opaque_ref_list(policy.object_type_refs, "object_type_refs")?;
    policy.documentation_refs = normalize_documentation_ref_list(
        policy.documentation_refs,
        known_docs,
        "documentation_refs",
    )?;
    policy.review.decision_artifact_family = normalized_opaque_ref(
        &policy.review.decision_artifact_family,
        "review.decision_artifact_family",
    )?;
    policy.review.cannot_link_artifact_family = normalized_opaque_ref(
        &policy.review.cannot_link_artifact_family,
        "review.cannot_link_artifact_family",
    )?;

    if policy.graph.max_objects_per_subject == Some(0) {
        return Err(artifact_contract_error(
            "graph.max_objects_per_subject must be greater than zero when declared",
        ));
    }
    if policy.graph.max_subjects_per_object == Some(0) {
        return Err(artifact_contract_error(
            "graph.max_subjects_per_object must be greater than zero when declared",
        ));
    }

    Ok(policy)
}

fn normalize_documentation_ref(
    mut reference: RelationPolicyDocumentationRef,
) -> RelationPolicyResult<RelationPolicyDocumentationRef> {
    reference.label = normalized_non_empty(&reference.label, "documentation.label")?;
    reference.uri = normalized_documentation_uri(&reference.uri, "documentation.uri")?;
    Ok(reference)
}

fn normalize_merge_request(
    mut request: RelationMergeRequest,
) -> RelationPolicyResult<RelationMergeRequest> {
    request.candidate_left_id =
        normalized_non_empty(&request.candidate_left_id, "candidate_left_id")?;
    request.candidate_right_id =
        normalized_non_empty(&request.candidate_right_id, "candidate_right_id")?;
    request.observations = request
        .observations
        .into_iter()
        .map(normalize_observation)
        .collect::<RelationPolicyResult<Vec<_>>>()?;
    Ok(request)
}

fn normalize_observation(
    mut observation: RelationObservation,
) -> RelationPolicyResult<RelationObservation> {
    observation.relation_id = normalized_hash(&observation.relation_id, "relation_id")?;
    observation.left_id = normalized_non_empty(&observation.left_id, "left_id")?;
    observation.left_type_ref = normalized_opaque_ref(&observation.left_type_ref, "left_type_ref")?;
    observation.right_id = normalized_non_empty(&observation.right_id, "right_id")?;
    observation.right_type_ref =
        normalized_opaque_ref(&observation.right_type_ref, "right_type_ref")?;
    if let Some(left_valid_to) = observation.left_valid_to.as_mut() {
        *left_valid_to = normalized_non_empty(left_valid_to, "left_valid_to")?;
    }
    if let Some(right_valid_from) = observation.right_valid_from.as_mut() {
        *right_valid_from = normalized_non_empty(right_valid_from, "right_valid_from")?;
    }
    Ok(observation)
}

fn validate_observations(
    policy: &RelationPolicyDefinition,
    observations: &[RelationObservation],
) -> RelationPolicyResult<()> {
    if observations.is_empty() {
        return Ok(());
    }

    for observation in observations {
        validate_observation_types(policy, observation)?;
        validate_temporal_boundary(policy, observation)?;
    }
    validate_graph(policy, observations)?;
    Ok(())
}

fn validate_observation_types(
    policy: &RelationPolicyDefinition,
    observation: &RelationObservation,
) -> RelationPolicyResult<()> {
    let left_matches = policy.subject_type_refs.is_empty()
        || policy
            .subject_type_refs
            .contains(&observation.left_type_ref);
    let right_matches = policy.object_type_refs.is_empty()
        || policy
            .object_type_refs
            .contains(&observation.right_type_ref);

    match policy.orientation {
        RelationOrientation::Directed => {
            if left_matches && right_matches {
                Ok(())
            } else {
                Err(constraint_violation_error(format!(
                    "relation {} does not satisfy the directed subject/object type contract for policy {}",
                    observation.relation_id, policy.policy_id
                )))
            }
        }
        RelationOrientation::UnorderedPair => {
            let swapped_left = policy.subject_type_refs.is_empty()
                || policy
                    .subject_type_refs
                    .contains(&observation.right_type_ref);
            let swapped_right = policy.object_type_refs.is_empty()
                || policy.object_type_refs.contains(&observation.left_type_ref);
            if (left_matches && right_matches) || (swapped_left && swapped_right) {
                Ok(())
            } else {
                Err(constraint_violation_error(format!(
                    "relation {} does not satisfy the unordered pair type contract for policy {}",
                    observation.relation_id, policy.policy_id
                )))
            }
        }
    }
}

fn validate_temporal_boundary(
    policy: &RelationPolicyDefinition,
    observation: &RelationObservation,
) -> RelationPolicyResult<()> {
    if policy.succession_policy != SuccessionTemporalPolicy::RequireNonOverlappingTransition {
        return Ok(());
    }

    let left_valid_to = observation.left_valid_to.as_deref().ok_or_else(|| {
        temporal_boundary_error(format!(
            "policy {} requires left_valid_to for succession relation {}",
            policy.policy_id, observation.relation_id
        ))
    })?;
    let right_valid_from = observation.right_valid_from.as_deref().ok_or_else(|| {
        temporal_boundary_error(format!(
            "policy {} requires right_valid_from for succession relation {}",
            policy.policy_id, observation.relation_id
        ))
    })?;

    if left_valid_to >= right_valid_from {
        return Err(temporal_boundary_error(format!(
            "policy {} requires non-overlapping succession boundaries for relation {}",
            policy.policy_id, observation.relation_id
        )));
    }

    Ok(())
}

fn validate_graph(
    policy: &RelationPolicyDefinition,
    observations: &[RelationObservation],
) -> RelationPolicyResult<()> {
    let mut outbound = BTreeMap::<&str, BTreeSet<&str>>::new();
    let mut inbound = BTreeMap::<&str, BTreeSet<&str>>::new();

    for observation in observations {
        outbound
            .entry(observation.left_id.as_str())
            .or_default()
            .insert(observation.right_id.as_str());
        inbound
            .entry(observation.right_id.as_str())
            .or_default()
            .insert(observation.left_id.as_str());
    }

    if let Some(max_objects_per_subject) = policy.graph.max_objects_per_subject {
        for (subject, objects) in &outbound {
            if objects.len() > max_objects_per_subject {
                return Err(graph_policy_error(format!(
                    "policy {} allows at most {} objects per subject but {} reached {}",
                    policy.policy_id,
                    max_objects_per_subject,
                    subject,
                    objects.len()
                )));
            }
        }
    }

    if let Some(max_subjects_per_object) = policy.graph.max_subjects_per_object {
        for (object, subjects) in &inbound {
            if subjects.len() > max_subjects_per_object {
                return Err(graph_policy_error(format!(
                    "policy {} allows at most {} subjects per object but {} reached {}",
                    policy.policy_id,
                    max_subjects_per_object,
                    object,
                    subjects.len()
                )));
            }
        }
    }

    if policy.orientation == RelationOrientation::Directed
        && policy.graph.cycle_policy == RelationCyclePolicy::Disallow
        && has_directed_cycle(observations)
    {
        return Err(graph_policy_error(format!(
            "policy {} disallows directed cycles for relation graph",
            policy.policy_id
        )));
    }

    Ok(())
}

fn direct_decision_outcome(
    policy: &RelationPolicyDefinition,
    pair_key: &str,
    direct: &[&RelationObservation],
    explicit_equality_fact: bool,
) -> RelationPolicyResult<Option<RelationMergeOutcome>> {
    let Some((decision, chosen)) = strongest_decision(direct) else {
        return Ok(None);
    };
    let decision_artifact = RelationDecisionArtifact {
        artifact_family: policy.review.decision_artifact_family.clone(),
        artifact_kind: decision_artifact_kind(decision).to_string(),
        relation_id: chosen.relation_id.clone(),
        pair_key: pair_key.to_string(),
        review_decision: decision,
    };

    let disposition = match decision {
        ReviewDecision::Distinct => MergeDisposition::BlockReviewedDistinct,
        ReviewDecision::Related => {
            if policy.succession_policy == SuccessionTemporalPolicy::RequireNonOverlappingTransition
            {
                MergeDisposition::TemporalSuccessionOnly
            } else if policy.related_distinct_veto {
                MergeDisposition::BlockRelatedDistinct
            } else {
                MergeDisposition::NoOpinion
            }
        }
        ReviewDecision::Same => match policy.identity_boundary {
            IdentityBoundaryPolicy::None => MergeDisposition::NeedsExplicitEqualityFact,
            IdentityBoundaryPolicy::ExplicitEqualityFactOnly => {
                if explicit_equality_fact {
                    MergeDisposition::AllowWithExplicitEqualityFact
                } else {
                    MergeDisposition::NeedsExplicitEqualityFact
                }
            }
        },
        ReviewDecision::Uncertain => MergeDisposition::ReviewRequired,
    };

    let cannot_link_patch =
        if decision == ReviewDecision::Distinct && policy.review.distinct_writes_cannot_link {
            Some(RelationCannotLinkPatch {
                artifact_family: policy.review.cannot_link_artifact_family.clone(),
                pair_key: pair_key.to_string(),
                policy_id: policy.policy_id.clone(),
                relation_type_id: policy.relation_type_id.clone(),
                decision_artifact_digest: canonical_digest(&decision_artifact)?,
            })
        } else {
            None
        };

    Ok(Some(RelationMergeOutcome {
        pair_key: pair_key.to_string(),
        pair_ids: sorted_pair(chosen.left_id.as_str(), chosen.right_id.as_str()),
        disposition,
        decision_artifact: Some(decision_artifact),
        cannot_link_patch,
    }))
}

fn strongest_decision<'a>(
    direct: &'a [&RelationObservation],
) -> Option<(ReviewDecision, &'a RelationObservation)> {
    let mut candidates = direct.to_vec();
    candidates.sort_by(|left, right| left.relation_id.cmp(&right.relation_id));

    for decision in [
        ReviewDecision::Distinct,
        ReviewDecision::Same,
        ReviewDecision::Related,
        ReviewDecision::Uncertain,
    ] {
        if let Some(observation) = candidates
            .iter()
            .copied()
            .find(|observation| observation.review_decision == decision)
        {
            return Some((decision, observation));
        }
    }

    None
}

fn transitive_pressure_disposition(
    policy: &RelationPolicyDefinition,
    observations: &[RelationObservation],
    pair_ids: &[String; 2],
) -> RelationPolicyResult<Option<MergeDisposition>> {
    match policy.transitive_merge_guard {
        TransitiveMergeGuard::Allow => Ok(None),
        TransitiveMergeGuard::Review | TransitiveMergeGuard::Block => {
            let pressured = match policy.orientation {
                RelationOrientation::Directed => {
                    path_exists(observations, &pair_ids[0], &pair_ids[1], policy.orientation)
                        || path_exists(observations, &pair_ids[1], &pair_ids[0], policy.orientation)
                }
                RelationOrientation::UnorderedPair => {
                    path_exists(observations, &pair_ids[0], &pair_ids[1], policy.orientation)
                }
            };
            if !pressured {
                return Ok(None);
            }

            Ok(Some(match policy.transitive_merge_guard {
                TransitiveMergeGuard::Allow => MergeDisposition::NoOpinion,
                TransitiveMergeGuard::Review => MergeDisposition::ReviewRequired,
                TransitiveMergeGuard::Block => MergeDisposition::BlockTransitivePressure,
            }))
        }
    }
}

fn path_exists(
    observations: &[RelationObservation],
    start: &str,
    goal: &str,
    orientation: RelationOrientation,
) -> bool {
    let mut adjacency = BTreeMap::<&str, BTreeSet<&str>>::new();
    for observation in observations {
        adjacency
            .entry(observation.left_id.as_str())
            .or_default()
            .insert(observation.right_id.as_str());
        if orientation == RelationOrientation::UnorderedPair {
            adjacency
                .entry(observation.right_id.as_str())
                .or_default()
                .insert(observation.left_id.as_str());
        }
    }

    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        if !visited.insert(node) {
            continue;
        }
        if node == goal && node != start {
            return true;
        }
        if let Some(neighbors) = adjacency.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }
    false
}

fn has_directed_cycle(observations: &[RelationObservation]) -> bool {
    let mut adjacency = BTreeMap::<&str, BTreeSet<&str>>::new();
    for observation in observations {
        adjacency
            .entry(observation.left_id.as_str())
            .or_default()
            .insert(observation.right_id.as_str());
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    adjacency
        .keys()
        .copied()
        .any(|node| has_cycle_from(node, &adjacency, &mut visiting, &mut visited))
}

fn has_cycle_from<'a>(
    node: &'a str,
    adjacency: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node) {
        return true;
    }

    if let Some(neighbors) = adjacency.get(node)
        && neighbors
            .iter()
            .copied()
            .any(|neighbor| has_cycle_from(neighbor, adjacency, visiting, visited))
    {
        return true;
    }

    visiting.remove(node);
    visited.insert(node);
    false
}

fn decision_artifact_kind(decision: ReviewDecision) -> &'static str {
    match decision {
        ReviewDecision::Same => "same",
        ReviewDecision::Distinct => "distinct",
        ReviewDecision::Related => "related",
        ReviewDecision::Uncertain => "uncertain",
    }
}

fn sorted_pair(left: &str, right: &str) -> [String; 2] {
    if left <= right {
        [left.to_string(), right.to_string()]
    } else {
        [right.to_string(), left.to_string()]
    }
}

fn pair_key(left: &str, right: &str) -> RelationPolicyResult<String> {
    canonical_digest(&serde_json::json!([left, right]))
}

fn canonical_digest<T: Serialize>(value: &T) -> RelationPolicyResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize relation policy digest input: {error}"
        ))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn normalized_non_empty(value: &str, field: &str) -> RelationPolicyResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_package_id(value: &str, field: &str) -> RelationPolicyResult<String> {
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

fn normalized_semver(value: &str, field: &str) -> RelationPolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let parts = normalized.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(|char| char.is_ascii_digit()))
    {
        return Err(artifact_contract_error(format!(
            "{field} must use MAJOR.MINOR.PATCH numeric semver"
        )));
    }
    Ok(normalized)
}

fn semver_major(value: &str) -> &str {
    value.split('.').next().unwrap_or_default()
}

fn normalized_hash(value: &str, field: &str) -> RelationPolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let hex = normalized
        .strip_prefix("blake3:")
        .ok_or_else(|| artifact_contract_error(format!("{field} must start with blake3:")))?;
    if hex.len() != 64 || !hex.chars().all(|char| char.is_ascii_hexdigit()) {
        return Err(artifact_contract_error(format!(
            "{field} must be a blake3: hash with 64 lowercase hex characters"
        )));
    }
    Ok(format!("blake3:{}", hex.to_ascii_lowercase()))
}

fn normalized_opaque_ref(value: &str, field: &str) -> RelationPolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let (namespace, local) = normalized.split_once(':').ok_or_else(|| {
        artifact_contract_error(format!(
            "{field} must use namespaced <namespace>:<local> format"
        ))
    })?;
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

fn normalize_opaque_ref_list(
    values: Vec<String>,
    field: &str,
) -> RelationPolicyResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_opaque_ref(&value, field))
        .collect::<RelationPolicyResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_documentation_ref_list(
    values: Vec<String>,
    known_docs: &BTreeSet<String>,
    field: &str,
) -> RelationPolicyResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_documentation_uri(&value, field))
        .collect::<RelationPolicyResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    for value in &normalized {
        if !known_docs.contains(value) {
            return Err(artifact_contract_error(format!(
                "{field} references undeclared documentation uri {value}"
            )));
        }
    }
    Ok(normalized)
}

fn normalized_documentation_uri(value: &str, field: &str) -> RelationPolicyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    if normalized.starts_with('/')
        || normalized.contains('\\')
        || normalized.split('/').any(|segment| segment == "..")
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

fn artifact_contract_error(message: impl Into<String>) -> RelationPolicyError {
    RelationPolicyError::new(RelationPolicyErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> RelationPolicyError {
    RelationPolicyError::new(RelationPolicyErrorCode::CompatibilityPolicy, message)
}

fn missing_policy_error(message: impl Into<String>) -> RelationPolicyError {
    RelationPolicyError::new(RelationPolicyErrorCode::MissingPolicy, message)
}

fn constraint_violation_error(message: impl Into<String>) -> RelationPolicyError {
    RelationPolicyError::new(RelationPolicyErrorCode::ConstraintViolation, message)
}

fn temporal_boundary_error(message: impl Into<String>) -> RelationPolicyError {
    RelationPolicyError::new(RelationPolicyErrorCode::TemporalBoundary, message)
}

fn graph_policy_error(message: impl Into<String>) -> RelationPolicyError {
    RelationPolicyError::new(RelationPolicyErrorCode::GraphPolicy, message)
}
