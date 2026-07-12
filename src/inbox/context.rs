#![forbid(unsafe_code)]

//! Privacy-safe usage and blast-radius context for unresolved groups.
//!
//! This artifact is decision support for review queues only. It is derived from
//! declared projects, artifacts, lineage, and consumers; it never promotes an
//! identity, picks a candidate, or mutates unresolved evidence.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
};

pub const CANON_USAGE_CONTEXT_VERSION: &str = "canon.usage_context.v1";
pub const USAGE_CONTEXT_IDENTITY_STATUS: &str = "context_only_no_identity_assertion";
pub const USAGE_CONTEXT_PRIVACY_MODEL: &str = "declared_inputs_banded_no_raw_values_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageContextInput {
    pub source_unresolved_groups_artifact_hash: String,
    pub policy: UsageContextPolicy,
    #[serde(default)]
    pub groups: Vec<UnresolvedGroupInput>,
    #[serde(default)]
    pub projects: Vec<DeclaredProject>,
    #[serde(default)]
    pub artifacts: Vec<DeclaredArtifact>,
    #[serde(default)]
    pub lineage_edges: Vec<LineageEdge>,
    #[serde(default)]
    pub consumer_bindings: Vec<ConsumerBinding>,
    #[serde(default)]
    pub group_occurrences: Vec<GroupOccurrence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageContextPolicy {
    pub policy_id: String,
    pub revision: String,
    pub weights: UsageContextWeights,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageContextWeights {
    pub frequency: i64,
    pub project_spread: i64,
    pub role_criticality: i64,
    pub source_criticality: i64,
    pub exposure: i64,
    pub row_count: i64,
    pub downstream_dependencies: i64,
    pub consumer_count: i64,
    pub lineage_depth: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedGroupInput {
    pub group_id: String,
    pub unresolved_group_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredProject {
    pub project_id: String,
    pub sensitivity: Sensitivity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criticality: Option<Criticality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure: Option<Exposure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredArtifact {
    pub artifact_id: String,
    pub project_id: String,
    pub artifact_kind: String,
    pub sensitivity: Sensitivity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criticality: Option<Criticality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure: Option<Exposure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageEdge {
    pub upstream_artifact_id: String,
    pub downstream_artifact_id: String,
    pub relation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumerBinding {
    pub consumer_id: String,
    pub artifact_id: String,
    pub consumer_kind: ConsumerKind,
    pub sensitivity: Sensitivity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downstream_dependency_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupOccurrence {
    pub group_id: String,
    pub artifact_id: String,
    pub typed_role: TypedRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageContextArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub identity_status: String,
    pub privacy_model: String,
    pub source_unresolved_groups_artifact_hash: String,
    pub policy: UsageContextPolicy,
    pub policy_content_hash: String,
    pub summary: UsageContextSummary,
    pub groups: Vec<GroupUsageContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UsageContextSummary {
    pub total_groups: u64,
    pub groups_with_unknown_context: u64,
    pub groups_with_sensitive_context: u64,
    pub declared_project_count: u64,
    pub declared_artifact_count: u64,
    pub declared_lineage_edge_count: u64,
    pub declared_consumer_binding_count: u64,
    pub group_occurrence_count: u64,
    #[serde(default)]
    pub by_source_criticality: BTreeMap<String, u64>,
    #[serde(default)]
    pub by_exposure: BTreeMap<String, u64>,
    #[serde(default)]
    pub by_consumer_kind: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupUsageContext {
    pub group_id: String,
    pub unresolved_group_digest: String,
    pub identity_status: String,
    pub context_only: bool,
    pub impact_units: i64,
    pub bands: UsageBands,
    pub typed_role_counts: BTreeMap<TypedRole, CountBand>,
    pub consumer_kinds: Vec<ConsumerKind>,
    pub privacy_safe_refs: PrivacySafeRefs,
    pub contributions: Vec<UsageContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncertainty_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageBands {
    pub frequency: CountBand,
    pub declared_project_count: CountBand,
    pub declared_artifact_count: CountBand,
    pub source_criticality: CriticalityBand,
    pub exposure: ExposureBand,
    pub row_count: CountBand,
    pub consumer_count: CountBand,
    pub downstream_dependency_count: CountBand,
    pub downstream_artifact_count: CountBand,
    pub lineage_depth: CountBand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PrivacySafeRefs {
    #[serde(default)]
    pub project_refs: Vec<String>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub consumer_refs: Vec<String>,
    pub redacted_project_count: CountBand,
    pub redacted_artifact_count: CountBand,
    pub redacted_consumer_count: CountBand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageContribution {
    pub component: String,
    pub signal_band: String,
    pub weight_units: i64,
    pub contribution_units: i64,
    pub unknown_or_redacted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CountBand {
    #[default]
    Unknown,
    KnownZero,
    One,
    TwoToFive,
    SixToTen,
    ElevenToFifty,
    FiftyOneToHundred,
    HundredOneToThousand,
    OverThousand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criticality {
    Low,
    Medium,
    High,
    MissionCritical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CriticalityBand {
    #[default]
    Unknown,
    Low,
    Medium,
    High,
    MissionCritical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exposure {
    Internal,
    Partner,
    Public,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExposureBand {
    #[default]
    Unknown,
    Internal,
    Partner,
    Public,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypedRole {
    LookupInput,
    NameField,
    AnchorField,
    ContextField,
    RelationSubject,
    RelationObject,
    AssignmentSubject,
    AssignmentAssignee,
    SourceOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerKind {
    DbtSeed,
    DbtModel,
    SearchIndex,
    Api,
    Dashboard,
    Notebook,
    ExportBundle,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageContextError {
    pub code: UsageContextErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageContextErrorCode {
    ArtifactContract,
}

pub type UsageContextResult<T> = Result<T, UsageContextError>;

#[derive(Debug)]
struct NormalizedInput {
    source_hash: String,
    policy: UsageContextPolicy,
    groups: Vec<UnresolvedGroupInput>,
    projects: BTreeMap<String, DeclaredProject>,
    artifacts: BTreeMap<String, DeclaredArtifact>,
    lineage_edges: Vec<LineageEdge>,
    consumers: BTreeMap<String, ConsumerBinding>,
    occurrences: Vec<GroupOccurrence>,
}

impl UsageContextPolicy {
    pub fn baseline(policy_id: impl Into<String>, revision: impl Into<String>) -> Self {
        Self {
            policy_id: policy_id.into(),
            revision: revision.into(),
            weights: UsageContextWeights {
                frequency: 8,
                project_spread: 12,
                role_criticality: 10,
                source_criticality: 12,
                exposure: 10,
                row_count: 7,
                downstream_dependencies: 16,
                consumer_count: 10,
                lineage_depth: 5,
            },
        }
    }
}

pub fn build_usage_context(input: UsageContextInput) -> UsageContextResult<UsageContextArtifact> {
    let input = normalize_input(input)?;
    let policy_content_hash = hash_serialized(&input.policy, "usage context policy")?;

    let mut groups = input
        .groups
        .iter()
        .map(|group| context_for_group(group, &input))
        .collect::<UsageContextResult<Vec<_>>>()?;
    groups.sort_by(|left, right| {
        left.group_id.cmp(&right.group_id).then_with(|| {
            left.unresolved_group_digest
                .cmp(&right.unresolved_group_digest)
        })
    });

    let mut artifact = UsageContextArtifact {
        version: CANON_USAGE_CONTEXT_VERSION.to_string(),
        artifact_content_hash: String::new(),
        identity_status: USAGE_CONTEXT_IDENTITY_STATUS.to_string(),
        privacy_model: USAGE_CONTEXT_PRIVACY_MODEL.to_string(),
        source_unresolved_groups_artifact_hash: input.source_hash.clone(),
        policy: input.policy.clone(),
        policy_content_hash,
        summary: UsageContextSummary::default(),
        groups,
    };
    artifact.summary = build_summary(&artifact, &input);
    artifact.artifact_content_hash = hash_without_self(&artifact)?;
    Ok(artifact)
}

pub fn canonical_usage_context_json_bytes(
    artifact: &UsageContextArtifact,
) -> UsageContextResult<Vec<u8>> {
    serde_json::to_vec(artifact).map_err(|error| {
        contract_error(format!(
            "failed to serialize usage context artifact: {error}"
        ))
    })
}

fn normalize_input(input: UsageContextInput) -> UsageContextResult<NormalizedInput> {
    let source_hash = normalized_blake3(
        &input.source_unresolved_groups_artifact_hash,
        "source_unresolved_groups_artifact_hash",
    )?;
    let policy = normalize_policy(input.policy)?;
    let groups = normalize_groups(input.groups)?;
    let projects = normalize_projects(input.projects)?;
    let artifacts = normalize_artifacts(input.artifacts, &projects)?;
    let lineage_edges = normalize_lineage_edges(input.lineage_edges, &artifacts)?;
    let consumers = normalize_consumer_bindings(input.consumer_bindings, &artifacts)?;
    let occurrences = normalize_group_occurrences(input.group_occurrences, &groups, &artifacts)?;

    Ok(NormalizedInput {
        source_hash,
        policy,
        groups,
        projects,
        artifacts,
        lineage_edges,
        consumers,
        occurrences,
    })
}

fn normalize_policy(mut policy: UsageContextPolicy) -> UsageContextResult<UsageContextPolicy> {
    policy.policy_id = policy.policy_id.trim().to_string();
    policy.revision = policy.revision.trim().to_string();
    if policy.policy_id.is_empty() || policy.revision.is_empty() {
        return Err(contract_error(
            "usage context policy requires non-empty policy_id and revision",
        ));
    }
    Ok(policy)
}

fn normalize_groups(
    groups: Vec<UnresolvedGroupInput>,
) -> UsageContextResult<Vec<UnresolvedGroupInput>> {
    let mut normalized = Vec::with_capacity(groups.len());
    let mut seen = BTreeSet::new();

    for mut group in groups {
        group.group_id = group.group_id.trim().to_string();
        group.unresolved_group_digest = normalized_blake3(
            &group.unresolved_group_digest,
            "groups.unresolved_group_digest",
        )?;
        if group.group_id.is_empty() {
            return Err(contract_error("usage context groups require group_id"));
        }
        if !seen.insert(group.group_id.clone()) {
            return Err(contract_error(format!(
                "duplicate unresolved group id: {}",
                group.group_id
            )));
        }
        normalized.push(group);
    }
    normalized.sort_by(|left, right| left.group_id.cmp(&right.group_id));
    Ok(normalized)
}

fn normalize_projects(
    projects: Vec<DeclaredProject>,
) -> UsageContextResult<BTreeMap<String, DeclaredProject>> {
    let mut normalized = BTreeMap::new();
    for mut project in projects {
        project.project_id = project.project_id.trim().to_string();
        if project.project_id.is_empty() {
            return Err(contract_error("declared projects require project_id"));
        }
        if normalized
            .insert(project.project_id.clone(), project)
            .is_some()
        {
            return Err(contract_error("duplicate declared project id"));
        }
    }
    Ok(normalized)
}

fn normalize_artifacts(
    artifacts: Vec<DeclaredArtifact>,
    projects: &BTreeMap<String, DeclaredProject>,
) -> UsageContextResult<BTreeMap<String, DeclaredArtifact>> {
    let mut normalized = BTreeMap::new();
    for mut artifact in artifacts {
        artifact.artifact_id = artifact.artifact_id.trim().to_string();
        artifact.project_id = artifact.project_id.trim().to_string();
        artifact.artifact_kind = artifact.artifact_kind.trim().to_string();
        if artifact.artifact_id.is_empty()
            || artifact.project_id.is_empty()
            || artifact.artifact_kind.is_empty()
        {
            return Err(contract_error(
                "declared artifacts require artifact_id, project_id, and artifact_kind",
            ));
        }
        if !projects.contains_key(&artifact.project_id) {
            return Err(contract_error(format!(
                "artifact {} references unknown project {}",
                artifact.artifact_id, artifact.project_id
            )));
        }
        if normalized
            .insert(artifact.artifact_id.clone(), artifact)
            .is_some()
        {
            return Err(contract_error("duplicate declared artifact id"));
        }
    }
    Ok(normalized)
}

fn normalize_lineage_edges(
    edges: Vec<LineageEdge>,
    artifacts: &BTreeMap<String, DeclaredArtifact>,
) -> UsageContextResult<Vec<LineageEdge>> {
    let mut normalized = Vec::with_capacity(edges.len());
    let mut seen = BTreeSet::new();
    for mut edge in edges {
        edge.upstream_artifact_id = edge.upstream_artifact_id.trim().to_string();
        edge.downstream_artifact_id = edge.downstream_artifact_id.trim().to_string();
        edge.relation = edge.relation.trim().to_string();
        if edge.upstream_artifact_id.is_empty()
            || edge.downstream_artifact_id.is_empty()
            || edge.relation.is_empty()
        {
            return Err(contract_error(
                "lineage edges require upstream_artifact_id, downstream_artifact_id, and relation",
            ));
        }
        if !artifacts.contains_key(&edge.upstream_artifact_id) {
            return Err(contract_error(format!(
                "lineage edge references unknown upstream artifact {}",
                edge.upstream_artifact_id
            )));
        }
        if !artifacts.contains_key(&edge.downstream_artifact_id) {
            return Err(contract_error(format!(
                "lineage edge references unknown downstream artifact {}",
                edge.downstream_artifact_id
            )));
        }
        let key = (
            edge.upstream_artifact_id.clone(),
            edge.downstream_artifact_id.clone(),
            edge.relation.clone(),
        );
        if seen.insert(key) {
            normalized.push(edge);
        }
    }
    normalized.sort_by(|left, right| {
        left.upstream_artifact_id
            .cmp(&right.upstream_artifact_id)
            .then_with(|| {
                left.downstream_artifact_id
                    .cmp(&right.downstream_artifact_id)
            })
            .then_with(|| left.relation.cmp(&right.relation))
    });
    Ok(normalized)
}

fn normalize_consumer_bindings(
    bindings: Vec<ConsumerBinding>,
    artifacts: &BTreeMap<String, DeclaredArtifact>,
) -> UsageContextResult<BTreeMap<String, ConsumerBinding>> {
    let mut normalized = BTreeMap::new();
    for mut binding in bindings {
        binding.consumer_id = binding.consumer_id.trim().to_string();
        binding.artifact_id = binding.artifact_id.trim().to_string();
        if binding.consumer_id.is_empty() || binding.artifact_id.is_empty() {
            return Err(contract_error(
                "consumer bindings require consumer_id and artifact_id",
            ));
        }
        if !artifacts.contains_key(&binding.artifact_id) {
            return Err(contract_error(format!(
                "consumer {} references unknown artifact {}",
                binding.consumer_id, binding.artifact_id
            )));
        }
        if normalized
            .insert(binding.consumer_id.clone(), binding)
            .is_some()
        {
            return Err(contract_error("duplicate consumer id"));
        }
    }
    Ok(normalized)
}

fn normalize_group_occurrences(
    occurrences: Vec<GroupOccurrence>,
    groups: &[UnresolvedGroupInput],
    artifacts: &BTreeMap<String, DeclaredArtifact>,
) -> UsageContextResult<Vec<GroupOccurrence>> {
    let group_ids = groups
        .iter()
        .map(|group| group.group_id.clone())
        .collect::<BTreeSet<_>>();
    let mut normalized = Vec::with_capacity(occurrences.len());
    for mut occurrence in occurrences {
        occurrence.group_id = occurrence.group_id.trim().to_string();
        occurrence.artifact_id = occurrence.artifact_id.trim().to_string();
        if occurrence.group_id.is_empty() || occurrence.artifact_id.is_empty() {
            return Err(contract_error(
                "group occurrences require group_id and artifact_id",
            ));
        }
        if !group_ids.contains(&occurrence.group_id) {
            return Err(contract_error(format!(
                "occurrence references unknown unresolved group {}",
                occurrence.group_id
            )));
        }
        if !artifacts.contains_key(&occurrence.artifact_id) {
            return Err(contract_error(format!(
                "occurrence references unknown artifact {}",
                occurrence.artifact_id
            )));
        }
        normalized.push(occurrence);
    }
    normalized.sort_by(|left, right| {
        left.group_id
            .cmp(&right.group_id)
            .then_with(|| left.artifact_id.cmp(&right.artifact_id))
            .then_with(|| left.typed_role.cmp(&right.typed_role))
            .then_with(|| left.occurrence_count.cmp(&right.occurrence_count))
    });
    Ok(normalized)
}

fn context_for_group(
    group: &UnresolvedGroupInput,
    input: &NormalizedInput,
) -> UsageContextResult<GroupUsageContext> {
    let occurrences = input
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.group_id == group.group_id)
        .collect::<Vec<_>>();

    let artifact_ids = occurrences
        .iter()
        .map(|occurrence| occurrence.artifact_id.clone())
        .collect::<BTreeSet<_>>();
    let mut project_ids = BTreeSet::new();
    for artifact_id in &artifact_ids {
        let artifact = input
            .artifacts
            .get(artifact_id)
            .ok_or_else(|| contract_error("normalized occurrence references missing artifact"))?;
        project_ids.insert(artifact.project_id.clone());
    }

    let consumers = input
        .consumers
        .values()
        .filter(|consumer| artifact_ids.contains(&consumer.artifact_id))
        .collect::<Vec<_>>();

    let consumer_kinds = consumers
        .iter()
        .map(|consumer| consumer.consumer_kind)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let downstream = downstream_lineage(&artifact_ids, &input.lineage_edges);
    let privacy_safe_refs = privacy_safe_refs(&project_ids, &artifact_ids, &consumers, input)?;

    let mut uncertainty_flags = Vec::new();
    let frequency = band_optional_sum(
        occurrences
            .iter()
            .map(|occurrence| occurrence.occurrence_count)
            .collect(),
        &mut uncertainty_flags,
        "missing_occurrence_count",
    );
    let row_count = band_optional_sum(
        artifact_ids
            .iter()
            .map(|artifact_id| input.artifacts[artifact_id].row_count)
            .collect(),
        &mut uncertainty_flags,
        "missing_row_count",
    );
    let downstream_dependency_count = if input.consumers.is_empty() {
        uncertainty_flags.push("missing_consumer_manifest".to_string());
        CountBand::Unknown
    } else {
        band_optional_sum(
            consumers
                .iter()
                .map(|consumer| consumer.downstream_dependency_count)
                .collect(),
            &mut uncertainty_flags,
            "missing_downstream_dependency_count",
        )
    };
    let lineage_depth = if input.lineage_edges.is_empty() {
        uncertainty_flags.push("missing_lineage_manifest".to_string());
        CountBand::Unknown
    } else {
        CountBand::from_count(downstream.max_depth)
    };
    let downstream_artifact_count = if input.lineage_edges.is_empty() {
        CountBand::Unknown
    } else {
        CountBand::from_count(downstream.artifact_ids.len() as u64)
    };

    let source_criticality =
        source_criticality_band(&project_ids, &artifact_ids, input, &mut uncertainty_flags);
    let exposure = exposure_band(&project_ids, &artifact_ids, input, &mut uncertainty_flags);
    let typed_role_counts = typed_role_counts(&occurrences, &mut uncertainty_flags);
    if occurrences.is_empty() {
        uncertainty_flags.push("missing_group_occurrences".to_string());
    }
    if privacy_safe_refs.redacted_project_count != CountBand::KnownZero
        || privacy_safe_refs.redacted_artifact_count != CountBand::KnownZero
        || privacy_safe_refs.redacted_consumer_count != CountBand::KnownZero
    {
        uncertainty_flags.push("sensitive_context_redacted".to_string());
    }
    normalize_string_list(&mut uncertainty_flags);

    let bands = UsageBands {
        frequency,
        declared_project_count: CountBand::from_count(project_ids.len() as u64),
        declared_artifact_count: CountBand::from_count(artifact_ids.len() as u64),
        source_criticality,
        exposure,
        row_count,
        consumer_count: if input.consumers.is_empty() {
            CountBand::Unknown
        } else {
            CountBand::from_count(consumers.len() as u64)
        },
        downstream_dependency_count,
        downstream_artifact_count,
        lineage_depth,
    };

    let contributions = contributions(&bands, &typed_role_counts, &input.policy.weights);
    let impact_units = contributions
        .iter()
        .map(|contribution| contribution.contribution_units)
        .sum();

    Ok(GroupUsageContext {
        group_id: group.group_id.clone(),
        unresolved_group_digest: group.unresolved_group_digest.clone(),
        identity_status: USAGE_CONTEXT_IDENTITY_STATUS.to_string(),
        context_only: true,
        impact_units,
        bands,
        typed_role_counts,
        consumer_kinds,
        privacy_safe_refs,
        contributions,
        uncertainty_flags,
    })
}

fn typed_role_counts(
    occurrences: &[&GroupOccurrence],
    uncertainty_flags: &mut Vec<String>,
) -> BTreeMap<TypedRole, CountBand> {
    let mut counts: BTreeMap<TypedRole, Vec<Option<u64>>> = BTreeMap::new();
    for occurrence in occurrences {
        counts
            .entry(occurrence.typed_role)
            .or_default()
            .push(occurrence.occurrence_count);
    }

    let mut bands = BTreeMap::new();
    for (role, values) in counts {
        let flag = format!("missing_{}_count", role.as_str());
        bands.insert(role, band_optional_sum(values, uncertainty_flags, &flag));
    }
    bands
}

fn source_criticality_band(
    project_ids: &BTreeSet<String>,
    artifact_ids: &BTreeSet<String>,
    input: &NormalizedInput,
    uncertainty_flags: &mut Vec<String>,
) -> CriticalityBand {
    let mut known = Vec::new();
    for project_id in project_ids {
        match input.projects[project_id].criticality {
            Some(criticality) => known.push(criticality.into()),
            None => uncertainty_flags.push("missing_project_criticality".to_string()),
        }
    }
    for artifact_id in artifact_ids {
        match input.artifacts[artifact_id].criticality {
            Some(criticality) => known.push(criticality.into()),
            None => uncertainty_flags.push("missing_artifact_criticality".to_string()),
        }
    }
    known.into_iter().max().unwrap_or(CriticalityBand::Unknown)
}

fn exposure_band(
    project_ids: &BTreeSet<String>,
    artifact_ids: &BTreeSet<String>,
    input: &NormalizedInput,
    uncertainty_flags: &mut Vec<String>,
) -> ExposureBand {
    let mut known = Vec::new();
    for project_id in project_ids {
        match input.projects[project_id].exposure {
            Some(exposure) => known.push(exposure.into()),
            None => uncertainty_flags.push("missing_project_exposure".to_string()),
        }
    }
    for artifact_id in artifact_ids {
        match input.artifacts[artifact_id].exposure {
            Some(exposure) => known.push(exposure.into()),
            None => uncertainty_flags.push("missing_artifact_exposure".to_string()),
        }
    }
    known.into_iter().max().unwrap_or(ExposureBand::Unknown)
}

fn privacy_safe_refs(
    project_ids: &BTreeSet<String>,
    artifact_ids: &BTreeSet<String>,
    consumers: &[&ConsumerBinding],
    input: &NormalizedInput,
) -> UsageContextResult<PrivacySafeRefs> {
    let mut refs = PrivacySafeRefs::default();
    let mut redacted_projects = 0;
    let mut redacted_artifacts = 0;
    let mut redacted_consumers = 0;

    for project_id in project_ids {
        let project = input
            .projects
            .get(project_id)
            .ok_or_else(|| contract_error("normalized group references missing project"))?;
        if project.sensitivity.is_redacted() {
            redacted_projects += 1;
        } else {
            refs.project_refs.push(project_id.clone());
        }
    }

    for artifact_id in artifact_ids {
        let artifact = input
            .artifacts
            .get(artifact_id)
            .ok_or_else(|| contract_error("normalized group references missing artifact"))?;
        let project = input
            .projects
            .get(&artifact.project_id)
            .ok_or_else(|| contract_error("normalized artifact references missing project"))?;
        if artifact.sensitivity.is_redacted() || project.sensitivity.is_redacted() {
            redacted_artifacts += 1;
        } else {
            refs.artifact_refs.push(artifact_id.clone());
        }
    }

    for consumer in consumers {
        let artifact = input
            .artifacts
            .get(&consumer.artifact_id)
            .ok_or_else(|| contract_error("normalized consumer references missing artifact"))?;
        let project = input
            .projects
            .get(&artifact.project_id)
            .ok_or_else(|| contract_error("normalized artifact references missing project"))?;
        if consumer.sensitivity.is_redacted()
            || artifact.sensitivity.is_redacted()
            || project.sensitivity.is_redacted()
        {
            redacted_consumers += 1;
        } else {
            refs.consumer_refs.push(consumer.consumer_id.clone());
        }
    }

    refs.redacted_project_count = CountBand::from_count(redacted_projects);
    refs.redacted_artifact_count = CountBand::from_count(redacted_artifacts);
    refs.redacted_consumer_count = CountBand::from_count(redacted_consumers);
    Ok(refs)
}

fn downstream_lineage(
    roots: &BTreeSet<String>,
    lineage_edges: &[LineageEdge],
) -> DownstreamLineage {
    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in lineage_edges {
        adjacency
            .entry(edge.upstream_artifact_id.as_str())
            .or_default()
            .push(edge.downstream_artifact_id.as_str());
    }

    let mut queue = roots
        .iter()
        .map(|root| (root.as_str(), 0_u64))
        .collect::<VecDeque<_>>();
    let mut downstream = BTreeSet::new();
    let mut max_depth = 0;

    while let Some((artifact_id, depth)) = queue.pop_front() {
        if let Some(children) = adjacency.get(artifact_id) {
            for child in children {
                if roots.contains(*child) {
                    continue;
                }
                if !downstream.insert((*child).to_string()) {
                    continue;
                }
                let child_depth = depth + 1;
                max_depth = max_depth.max(child_depth);
                queue.push_back((*child, child_depth));
            }
        }
    }

    DownstreamLineage {
        artifact_ids: downstream,
        max_depth,
    }
}

fn contributions(
    bands: &UsageBands,
    typed_role_counts: &BTreeMap<TypedRole, CountBand>,
    weights: &UsageContextWeights,
) -> Vec<UsageContribution> {
    let role_score = typed_role_counts
        .iter()
        .map(|(role, band)| role.weight_units() * band.score_units())
        .max()
        .unwrap_or(0);
    let role_unknown = typed_role_counts.values().any(|band| band.is_unknown());

    let mut contributions = vec![
        count_contribution("frequency", bands.frequency, weights.frequency),
        count_contribution(
            "project_spread",
            bands.declared_project_count,
            weights.project_spread,
        ),
        UsageContribution {
            component: "role_criticality".to_string(),
            signal_band: "max_typed_role_weight".to_string(),
            weight_units: weights.role_criticality,
            contribution_units: role_score * weights.role_criticality,
            unknown_or_redacted: role_unknown || typed_role_counts.is_empty(),
        },
        criticality_contribution(
            "source_criticality",
            bands.source_criticality,
            weights.source_criticality,
        ),
        exposure_contribution("exposure", bands.exposure, weights.exposure),
        count_contribution("row_count", bands.row_count, weights.row_count),
        count_contribution(
            "downstream_dependencies",
            bands.downstream_dependency_count,
            weights.downstream_dependencies,
        ),
        count_contribution(
            "consumer_count",
            bands.consumer_count,
            weights.consumer_count,
        ),
        count_contribution("lineage_depth", bands.lineage_depth, weights.lineage_depth),
    ];
    contributions.sort_by(|left, right| left.component.cmp(&right.component));
    contributions
}

fn count_contribution(component: &str, band: CountBand, weight_units: i64) -> UsageContribution {
    UsageContribution {
        component: component.to_string(),
        signal_band: band.as_str().to_string(),
        weight_units,
        contribution_units: band.score_units() * weight_units,
        unknown_or_redacted: band.is_unknown(),
    }
}

fn criticality_contribution(
    component: &str,
    band: CriticalityBand,
    weight_units: i64,
) -> UsageContribution {
    UsageContribution {
        component: component.to_string(),
        signal_band: band.as_str().to_string(),
        weight_units,
        contribution_units: band.score_units() * weight_units,
        unknown_or_redacted: band == CriticalityBand::Unknown,
    }
}

fn exposure_contribution(
    component: &str,
    band: ExposureBand,
    weight_units: i64,
) -> UsageContribution {
    UsageContribution {
        component: component.to_string(),
        signal_band: band.as_str().to_string(),
        weight_units,
        contribution_units: band.score_units() * weight_units,
        unknown_or_redacted: band == ExposureBand::Unknown,
    }
}

fn build_summary(artifact: &UsageContextArtifact, input: &NormalizedInput) -> UsageContextSummary {
    let mut summary = UsageContextSummary {
        total_groups: artifact.groups.len() as u64,
        groups_with_unknown_context: 0,
        groups_with_sensitive_context: 0,
        declared_project_count: input.projects.len() as u64,
        declared_artifact_count: input.artifacts.len() as u64,
        declared_lineage_edge_count: input.lineage_edges.len() as u64,
        declared_consumer_binding_count: input.consumers.len() as u64,
        group_occurrence_count: input.occurrences.len() as u64,
        by_source_criticality: BTreeMap::new(),
        by_exposure: BTreeMap::new(),
        by_consumer_kind: BTreeMap::new(),
    };

    for group in &artifact.groups {
        if !group.uncertainty_flags.is_empty() {
            summary.groups_with_unknown_context += 1;
        }
        if group
            .uncertainty_flags
            .iter()
            .any(|flag| flag == "sensitive_context_redacted")
        {
            summary.groups_with_sensitive_context += 1;
        }
        *summary
            .by_source_criticality
            .entry(group.bands.source_criticality.as_str().to_string())
            .or_insert(0) += 1;
        *summary
            .by_exposure
            .entry(group.bands.exposure.as_str().to_string())
            .or_insert(0) += 1;
        for consumer_kind in &group.consumer_kinds {
            *summary
                .by_consumer_kind
                .entry(consumer_kind.as_str().to_string())
                .or_insert(0) += 1;
        }
    }
    summary
}

fn band_optional_sum(
    values: Vec<Option<u64>>,
    uncertainty_flags: &mut Vec<String>,
    missing_flag: &str,
) -> CountBand {
    if values.is_empty() {
        return CountBand::KnownZero;
    }
    let mut sum = 0_u64;
    for value in values {
        match value {
            Some(value) => sum = sum.saturating_add(value),
            None => {
                uncertainty_flags.push(missing_flag.to_string());
                return CountBand::Unknown;
            }
        }
    }
    CountBand::from_count(sum)
}

fn normalize_string_list(values: &mut Vec<String>) {
    for value in values.iter_mut() {
        *value = value.trim().to_string();
    }
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
}

fn hash_without_self(artifact: &UsageContextArtifact) -> UsageContextResult<String> {
    let mut clone = artifact.clone();
    clone.artifact_content_hash.clear();
    hash_serialized(&clone, "usage context artifact")
}

fn hash_serialized<T: Serialize>(value: &T, label: &str) -> UsageContextResult<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| contract_error(format!("failed to serialize {label}: {error}")))?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn normalized_blake3(value: &str, field: &str) -> UsageContextResult<String> {
    let trimmed = value.trim();
    let digest = trimmed.strip_prefix("blake3:").ok_or_else(|| {
        contract_error(format!(
            "{field} must be a blake3 digest with blake3: prefix"
        ))
    })?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(contract_error(format!(
            "{field} must be a 64-character lowercase hex blake3 digest"
        )));
    }
    Ok(format!("blake3:{}", digest.to_ascii_lowercase()))
}

fn contract_error(message: impl Into<String>) -> UsageContextError {
    UsageContextError {
        code: UsageContextErrorCode::ArtifactContract,
        message: message.into(),
    }
}

impl CountBand {
    pub fn from_count(count: u64) -> Self {
        match count {
            0 => CountBand::KnownZero,
            1 => CountBand::One,
            2..=5 => CountBand::TwoToFive,
            6..=10 => CountBand::SixToTen,
            11..=50 => CountBand::ElevenToFifty,
            51..=100 => CountBand::FiftyOneToHundred,
            101..=1_000 => CountBand::HundredOneToThousand,
            _ => CountBand::OverThousand,
        }
    }

    pub fn is_unknown(self) -> bool {
        self == CountBand::Unknown
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CountBand::Unknown => "unknown",
            CountBand::KnownZero => "known_zero",
            CountBand::One => "one",
            CountBand::TwoToFive => "two_to_five",
            CountBand::SixToTen => "six_to_ten",
            CountBand::ElevenToFifty => "eleven_to_fifty",
            CountBand::FiftyOneToHundred => "fifty_one_to_hundred",
            CountBand::HundredOneToThousand => "hundred_one_to_thousand",
            CountBand::OverThousand => "over_thousand",
        }
    }

    fn score_units(self) -> i64 {
        match self {
            CountBand::Unknown | CountBand::KnownZero => 0,
            CountBand::One => 1,
            CountBand::TwoToFive => 3,
            CountBand::SixToTen => 8,
            CountBand::ElevenToFifty => 25,
            CountBand::FiftyOneToHundred => 75,
            CountBand::HundredOneToThousand => 250,
            CountBand::OverThousand => 1_000,
        }
    }
}

impl Sensitivity {
    fn is_redacted(self) -> bool {
        matches!(self, Sensitivity::Confidential | Sensitivity::Restricted)
    }
}

impl From<Criticality> for CriticalityBand {
    fn from(value: Criticality) -> Self {
        match value {
            Criticality::Low => CriticalityBand::Low,
            Criticality::Medium => CriticalityBand::Medium,
            Criticality::High => CriticalityBand::High,
            Criticality::MissionCritical => CriticalityBand::MissionCritical,
        }
    }
}

impl CriticalityBand {
    fn as_str(self) -> &'static str {
        match self {
            CriticalityBand::Unknown => "unknown",
            CriticalityBand::Low => "low",
            CriticalityBand::Medium => "medium",
            CriticalityBand::High => "high",
            CriticalityBand::MissionCritical => "mission_critical",
        }
    }

    fn score_units(self) -> i64 {
        match self {
            CriticalityBand::Unknown => 0,
            CriticalityBand::Low => 1,
            CriticalityBand::Medium => 3,
            CriticalityBand::High => 7,
            CriticalityBand::MissionCritical => 12,
        }
    }
}

impl From<Exposure> for ExposureBand {
    fn from(value: Exposure) -> Self {
        match value {
            Exposure::Internal => ExposureBand::Internal,
            Exposure::Partner => ExposureBand::Partner,
            Exposure::Public => ExposureBand::Public,
            Exposure::Restricted => ExposureBand::Restricted,
        }
    }
}

impl ExposureBand {
    fn as_str(self) -> &'static str {
        match self {
            ExposureBand::Unknown => "unknown",
            ExposureBand::Internal => "internal",
            ExposureBand::Partner => "partner",
            ExposureBand::Public => "public",
            ExposureBand::Restricted => "restricted",
        }
    }

    fn score_units(self) -> i64 {
        match self {
            ExposureBand::Unknown => 0,
            ExposureBand::Internal => 1,
            ExposureBand::Partner => 4,
            ExposureBand::Public => 8,
            ExposureBand::Restricted => 10,
        }
    }
}

impl TypedRole {
    fn as_str(self) -> &'static str {
        match self {
            TypedRole::LookupInput => "lookup_input",
            TypedRole::NameField => "name_field",
            TypedRole::AnchorField => "anchor_field",
            TypedRole::ContextField => "context_field",
            TypedRole::RelationSubject => "relation_subject",
            TypedRole::RelationObject => "relation_object",
            TypedRole::AssignmentSubject => "assignment_subject",
            TypedRole::AssignmentAssignee => "assignment_assignee",
            TypedRole::SourceOnly => "source_only",
            TypedRole::Unknown => "unknown",
        }
    }

    fn weight_units(self) -> i64 {
        match self {
            TypedRole::AnchorField => 14,
            TypedRole::LookupInput | TypedRole::NameField => 10,
            TypedRole::RelationSubject
            | TypedRole::RelationObject
            | TypedRole::AssignmentSubject
            | TypedRole::AssignmentAssignee => 8,
            TypedRole::ContextField => 3,
            TypedRole::SourceOnly => 1,
            TypedRole::Unknown => 0,
        }
    }
}

impl ConsumerKind {
    fn as_str(self) -> &'static str {
        match self {
            ConsumerKind::DbtSeed => "dbt_seed",
            ConsumerKind::DbtModel => "dbt_model",
            ConsumerKind::SearchIndex => "search_index",
            ConsumerKind::Api => "api",
            ConsumerKind::Dashboard => "dashboard",
            ConsumerKind::Notebook => "notebook",
            ConsumerKind::ExportBundle => "export_bundle",
            ConsumerKind::Other => "other",
        }
    }
}

impl fmt::Display for UsageContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for UsageContextError {}

#[derive(Debug)]
struct DownstreamLineage {
    artifact_ids: BTreeSet<String>,
    max_depth: u64,
}
