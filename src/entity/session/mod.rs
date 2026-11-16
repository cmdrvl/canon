#![forbid(unsafe_code)]

//! The evidence-session rule table and deterministic solve.
//!
//! Session solve deliberately owns the session-specific tier policy while it
//! reuses the entity signed graph and component evaluator. It does not
//! introduce a second union-find or a second graph implementation.

use crate::{
    InputFormat, InputValues, Refusal, RefusalCode, SpecialReason,
    entity::runtime::{AnchorValue, CannotLinkFact, PendingClusterRecord},
    entity::{
        edge::{EdgeEvidenceHit, build_edge_evidence_record},
        graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
        publication::{
            EntityPublicationFileInput, EntityPublicationRequest, EntityPublicationSnapshot,
            EntityPublicationUpstreamRef, open_current_stream_generation, publish_stream_patch,
        },
        score::{ScoreLane, ScoreUnits},
        solve::{
            SolveBudgetAction, SolveBudgetConfig, SolveComponentBudgetInput,
            evaluate_signed_graph_components, evaluate_solve_component_budget,
        },
    },
    keys::KeyNamespace,
    lookup, registry,
    session::{
        self, SESSION_EVIDENCE_STREAM_ID, SESSION_MANIFEST_STREAM_ID,
        SESSION_OBSERVATIONS_STREAM_ID, SessionCommitAlias, SessionCommitClosure,
        SessionCommitEntry, SessionCommitRegistryExpectation, SessionCommitRelation,
        SessionCommitRelationEndpoint, SessionCommitSkipped, SessionManifest,
        SessionRegistryMemoryWrite, registries,
    },
    temporal::relation::{
        DirectedRelationFact, IntervalBoundary, RelationKindRef, TimeInterval, finalize_relation,
    },
    witness::hash_bytes,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

pub mod rules;
mod status;
use rules::{SessionRules, parse_rules};
pub use status::{
    SESSION_STATUS_VERSION, SessionStatusOutput, SessionStatusRequest, session_status,
};

pub const SESSION_SOLVE_VERSION: &str = "canon.entity.session.solve.v1";
pub const SESSION_SOLVE_STREAM_ID: &str = "entity-session-solve";
const SESSION_PUBLICATION_STAGE: &str = "session";

const TIER_A_TYPES: &[&str] = &["same_key", "asserted_alias"];
const TIER_B_TYPES: &[&str] = &["same_slot", "attribute_agreement", "name_equivalent"];
const RELATION_TYPES: &[&str] = &["related"];
const DISTINCT_TYPES: &[&str] = &["distinct"];
const ROLE_RELATIONS: &[&str] = &[
    "sponsor_of",
    "guarantor_of",
    "borrower_of",
    "servicer_of",
    "trustee_of",
];
const CROSS_ENTITY_TYPE_EVIDENCE_REASON: &str = "cross_entity_type_evidence";

#[derive(Debug, Clone)]
pub struct SessionSolveRequest {
    pub session: PathBuf,
    pub rules: PathBuf,
}

#[derive(Debug, Clone)]
pub struct EntitySessionCommitRequest {
    pub session: PathBuf,
    pub next_version: Option<String>,
    pub bump: Option<registry::RegistryVersionBump>,
    /// `--expect-base`: the registry content_hash the driver synced, or
    /// `kind=hash,kind=hash` for several registries.
    pub expect_base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveInputs {
    pub observations_generation: String,
    pub evidence_generation: Option<String>,
    pub rules_hash: String,
    pub registry: SessionSolveRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveRegistry {
    pub id: String,
    pub version: String,
    pub snapshot_hash: String,
    #[serde(default)]
    pub escrow_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionSolveSummary {
    pub cluster_count: usize,
    pub node_count: usize,
    pub accepted_tier_a: usize,
    pub accepted_tier_b: usize,
    pub review: usize,
    pub contradiction: usize,
    pub standalone: usize,
    pub resolved_existing: usize,
    pub relations: usize,
    #[serde(default)]
    pub suppressed_by_relation: usize,
    #[serde(default)]
    pub suppressed_by_cannot_link: usize,
    #[serde(default)]
    pub review_new: usize,
    #[serde(default)]
    pub review_known: usize,
    pub dangling_evidence: usize,
    pub oversized_cluster: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveCluster {
    pub cluster_id: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    pub surfaces: Vec<String>,
    pub observation_count: usize,
    pub keys: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incumbent_canonical_id: Option<String>,
    pub accepting_evidence_ids: Vec<String>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveReview {
    pub left_surface: String,
    pub right_surface: String,
    pub evidence_types: Vec<String>,
    pub lineage_count: usize,
    pub missing: String,
    #[serde(default)]
    pub known_pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escrow_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveRelation {
    pub left_surface: String,
    pub right_surface: String,
    #[serde(default)]
    pub subject_surface: String,
    #[serde(default)]
    pub object_surface: String,
    pub relation: String,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub subject_entity_type: String,
    #[serde(default)]
    pub object_entity_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_external_id: Option<String>,
    #[serde(default)]
    pub valid_time: TimeInterval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveContradiction {
    pub reason: String,
    pub surfaces: Vec<String>,
    pub obs_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveArtifact {
    pub version: String,
    pub inputs: SessionSolveInputs,
    pub authoritative_keys: Vec<String>,
    /// Non-authoritative namespaces the rule table lists under
    /// `commit.key_namespaces`: their values are written as key aliases too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commit_key_namespaces: Vec<String>,
    pub summary: SessionSolveSummary,
    pub clusters: Vec<SessionSolveCluster>,
    pub review: Vec<SessionSolveReview>,
    pub relations: Vec<SessionSolveRelation>,
    pub contradictions: Vec<SessionSolveContradiction>,
    pub standalone_count: usize,
    pub dangling_evidence_count: usize,
    pub derived_evidence: Vec<crate::entity::evidence_ir::EvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionSolveOutput {
    #[serde(flatten)]
    pub artifact: SessionSolveArtifact,
    pub generation_id: String,
    pub artifact_path: String,
}

impl SessionSolveOutput {
    pub fn render_summary(&self) -> String {
        let summary = &self.artifact.summary;
        format!(
            "clusters={} tier_a={} tier_b={} review={} review_new={} review_known={} contradiction={} standalone={} resolved_existing={} relations={} suppressed_by_relation={} suppressed_by_cannot_link={} dangling_evidence={} oversized_cluster={} generation={}",
            summary.cluster_count,
            summary.accepted_tier_a,
            summary.accepted_tier_b,
            summary.review,
            summary.review_new,
            summary.review_known,
            summary.contradiction,
            summary.standalone,
            summary.resolved_existing,
            summary.relations,
            summary.suppressed_by_relation,
            summary.suppressed_by_cannot_link,
            summary.dangling_evidence,
            summary.oversized_cluster,
            self.generation_id,
        )
    }
}

#[derive(Debug, Clone)]
struct Observation {
    obs_id: String,
    entity_type: String,
    surface: String,
    node_key: String,
    keys: BTreeMap<String, String>,
    lineages: BTreeSet<String>,
    origin_components: BTreeSet<BTreeSet<String>>,
}

#[derive(Debug, Clone)]
struct Node {
    surface: String,
    observation_ids: BTreeSet<String>,
    keys: BTreeMap<String, BTreeSet<String>>,
    observed_keys: BTreeMap<String, BTreeSet<String>>,
    incumbent_key_conflicts: BTreeSet<String>,
    incumbent_canonical_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RegistryEndpoint {
    entity_type: String,
    node_key: String,
    surface: String,
    canonical_id: String,
    keys: BTreeMap<String, BTreeSet<String>>,
    alias_endpoint: bool,
}

#[derive(Debug, Clone)]
struct TargetInfo {
    node_key: String,
    entity_type: String,
    surface: String,
    lineages: BTreeSet<String>,
    origin_components: BTreeSet<BTreeSet<String>>,
    alias_endpoint: bool,
}

#[derive(Debug, Clone, Default)]
struct EdgeFacts {
    left: String,
    right: String,
    evidence_types: BTreeSet<String>,
    lineages: BTreeSet<String>,
    origin_components: BTreeSet<BTreeSet<String>>,
    evidence_ids: BTreeSet<String>,
    obs_ids: BTreeSet<String>,
    hits: Vec<EdgeEvidenceHit>,
    relation: BTreeSet<String>,
    relation_subject: BTreeMap<String, String>,
    positive: bool,
    variant_of_incumbent: bool,
    authoritative_same_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeTier {
    A,
    B,
    C,
}

#[derive(Debug, Clone, Deserialize)]
struct RawObservation {
    obs_id: String,
    entity_type: String,
    surface: String,
    #[serde(default)]
    keys: BTreeMap<String, Value>,
    #[serde(default)]
    attrs: BTreeMap<String, Value>,
    #[serde(default)]
    context: BTreeMap<String, Value>,
    #[serde(default)]
    source: RawObservationSource,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawObservationSource {
    #[serde(default)]
    lineage: Option<String>,
    #[serde(default)]
    ancestry: Option<RawAncestry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawAncestry {
    status: String,
    #[serde(default)]
    origin_source_ids: Vec<String>,
}

pub fn solve(request: SessionSolveRequest) -> Result<SessionSolveOutput, Refusal> {
    let session = absolute_session_path(&request.session)?;
    let manifest = read_manifest(&session)?;
    let (rules, rules_hash) = parse_rules(&request.rules)?;
    let observations_snapshot =
        open_required_stream(&session, SESSION_OBSERVATIONS_STREAM_ID, "observations")?;
    let evidence_snapshot = open_optional_stream(&session, SESSION_EVIDENCE_STREAM_ID)?;
    let role_relations = read_role_relations(&observations_snapshot)?;
    let observations = read_observations(&observations_snapshot)?;
    let evidence = match evidence_snapshot.as_ref() {
        Some(snapshot) => read_evidence(snapshot)?,
        None => Vec::new(),
    };
    if manifest.registries.is_empty() {
        return Err(solve_refusal(
            "The session manifest has no registry binding",
            json!({"field": "registries", "writes_performed": false}),
            "Reinitialize the session with --registry, then rerun canon entity session solve",
        ));
    };
    let mut registry_metadata_parts = Vec::new();
    let mut incumbents = BTreeMap::new();
    let mut incumbent_keys = BTreeMap::new();
    let mut endpoints = BTreeMap::new();
    let mut related_pairs = BTreeSet::new();
    let mut registry_cannot_link_facts = Vec::new();
    let mut registry_pending_clusters = Vec::new();
    for registry_binding in &manifest.registries {
        let scoped_observations = observations
            .iter()
            .filter(|observation| observation.entity_type == registry_binding.kind)
            .cloned()
            .collect::<Vec<_>>();
        let registry_info = load_registry_info(
            Path::new(&registry_binding.path),
            &scoped_observations,
            &evidence,
            &registry_binding.kind,
        )?;
        registry_metadata_parts.push((registry_binding.kind.clone(), registry_info.metadata));
        incumbents.extend(registry_info.incumbents);
        incumbent_keys.extend(registry_info.incumbent_keys);
        endpoints.extend(registry_info.endpoints);
        registry_cannot_link_facts.extend(registry_info.cannot_link_facts);
        registry_pending_clusters.extend(registry_info.pending_clusters);
        related_pairs.extend(read_related_registry_pairs(Path::new(
            &registry_binding.path,
        ))?);
    }
    let registry_metadata = aggregate_registry_metadata(&registry_metadata_parts);
    let (nodes, observations_by_id) =
        build_nodes(observations, incumbents, incumbent_keys, endpoints.clone());
    let mut derived_evidence = Vec::new();
    let mut edge_facts = BTreeMap::new();
    let mut same_node_distinct = BTreeSet::new();

    derive_same_key_edges(
        &observations_by_id,
        &rules,
        &mut edge_facts,
        &mut derived_evidence,
    )?;
    derive_registry_key_edges(
        &observations_by_id,
        &endpoints,
        &rules,
        &mut edge_facts,
        &mut derived_evidence,
    )?;

    let mut registry_cannot_link_pairs = BTreeSet::new();
    for fact in &registry_cannot_link_facts {
        let left_nodes = memory_key_node_keys(&fact.left_key, &nodes, &endpoints);
        let right_nodes = memory_key_node_keys(&fact.right_key, &nodes, &endpoints);
        for left in &left_nodes {
            for right in &right_nodes {
                if left != right {
                    let pair = ordered_pair(left, right);
                    registry_cannot_link_pairs.insert(pair.clone());
                    let facts = edge_facts.entry(pair.clone()).or_insert_with(|| EdgeFacts {
                        left: pair.0.clone(),
                        right: pair.1.clone(),
                        ..EdgeFacts::default()
                    });
                    facts.evidence_types.insert("distinct".to_string());
                    facts.hits.push(EdgeEvidenceHit::new(
                        ScoreLane::AntiMerge,
                        "registry",
                        "distinct",
                        "registry_cannot_link",
                        ScoreUnits::ZERO,
                        true,
                        format!(
                            "registry cannot-link {} vs {} ({})",
                            fact.left_key, fact.right_key, fact.reason
                        ),
                    ));
                }
            }
        }
    }

    let mut dangling_evidence_count = 0;
    for record in evidence {
        let Some((left_target_id, right_target_id)) = pair_target(&record) else {
            dangling_evidence_count += 1;
            continue;
        };
        let Some(left_target) = resolve_target(left_target_id, &observations_by_id, &endpoints)
        else {
            dangling_evidence_count += 1;
            continue;
        };
        let Some(right_target) = resolve_target(right_target_id, &observations_by_id, &endpoints)
        else {
            dangling_evidence_count += 1;
            continue;
        };
        if left_target.entity_type != right_target.entity_type {
            return Err(solve_refusal(
                format!(
                    "Evidence {} crosses entity types {} and {}",
                    record.evidence_id, left_target.entity_type, right_target.entity_type
                ),
                json!({
                    "reason": CROSS_ENTITY_TYPE_EVIDENCE_REASON,
                    "evidence_id": record.evidence_id,
                    "left_target": left_target_id,
                    "left_entity_type": left_target.entity_type,
                    "right_target": right_target_id,
                    "right_entity_type": right_target.entity_type,
                    "writes_performed": false
                }),
                "Split observations by entity_type into separate sessions, then rerun canon entity session solve",
            ));
        }
        let left_node = left_target.node_key.clone();
        let right_node = right_target.node_key.clone();
        let evidence_type = evidence_type(&record);
        if evidence_type.is_empty() {
            continue;
        }
        let mut lineage_ids = BTreeSet::new();
        lineage_ids.extend(left_target.lineages.iter().cloned());
        lineage_ids.extend(right_target.lineages.iter().cloned());
        if let Some(lineage) = record_lineage(&record) {
            lineage_ids.insert(lineage);
        }
        let mut origin_components = BTreeSet::new();
        origin_components.extend(left_target.origin_components.iter().cloned());
        origin_components.extend(right_target.origin_components.iter().cloned());
        origin_components.extend(record_origin_components(&record));
        if left_node == right_node {
            if DISTINCT_TYPES.contains(&evidence_type.as_str()) {
                same_node_distinct.insert(left_node.clone());
            }
            continue;
        }
        let (left, right) = ordered_pair(&left_node, &right_node);
        let facts = edge_facts
            .entry((left.clone(), right.clone()))
            .or_insert_with(|| EdgeFacts {
                left,
                right,
                ..EdgeFacts::default()
            });
        facts.evidence_types.insert(evidence_type.clone());
        facts.lineages.extend(lineage_ids);
        merge_origin_components(&mut facts.origin_components, origin_components);
        facts.evidence_ids.insert(record.evidence_id.clone());
        facts.obs_ids.insert(left_target_id.to_string());
        facts.obs_ids.insert(right_target_id.to_string());
        facts.positive |= is_positive_type(&evidence_type);
        facts.variant_of_incumbent |= is_incumbent_variant(
            &record,
            &left_target,
            &right_target,
            &rules.incumbent_attach_steps,
        );
        if RELATION_TYPES.contains(&evidence_type.as_str()) {
            let relation = relation_value(&record);
            facts.relation.insert(relation.clone());
            facts.relation_subject.entry(relation).or_insert_with(|| {
                relation_subject_node(
                    &record,
                    left_target_id,
                    right_target_id,
                    &left_node,
                    &right_node,
                )
            });
        }
        facts.hits.push(hit_for_type(&evidence_type, &record));
    }

    let mut tiered_edges = BTreeMap::new();
    for (pair, facts) in &edge_facts {
        if let Some(tier) = tier_for(facts, &rules) {
            tiered_edges.insert(pair.clone(), tier);
        }
    }
    let mut graph_records = Vec::new();
    for (pair, facts) in &edge_facts {
        let joining = matches!(tiered_edges.get(pair), Some(EdgeTier::A | EdgeTier::B));
        // A human `different` decision can veto a pair that was still below
        // the acceptance threshold (for example, one `same_slot` observation
        // plus the imported `distinct` evidence). Keep that positive edge in
        // the signed graph so the anti-merge fact is evaluated inside its
        // existing evidence component instead of against two singleton nodes.
        let distinct_positive_edge = has_positive_distinct(facts);
        let incumbent_count = [pair.0.as_str(), pair.1.as_str()]
            .iter()
            .filter(|surface_id| {
                nodes
                    .get(**surface_id)
                    .is_some_and(|node| node.incumbent_canonical_id.is_some())
            })
            .count();
        let hits = facts
            .hits
            .iter()
            .filter(|hit| {
                joining
                    || distinct_positive_edge
                    || (incumbent_count == 1 && facts.positive)
                    || DISTINCT_TYPES.contains(&hit.operator_id.as_str())
                    || RELATION_TYPES.contains(&hit.operator_id.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        if !hits.is_empty() {
            graph_records.push(build_edge_evidence_record(
                facts.left.clone(),
                facts.right.clone(),
                hits,
            ));
        }
    }
    let graph_records = graph_records.into_iter().collect::<Result<Vec<_>, _>>()?;
    let incumbent_ids = nodes
        .iter()
        .filter_map(|(key, node)| {
            node.incumbent_canonical_id
                .as_ref()
                .map(|canonical_id| SurfaceIncumbentId {
                    surface_id: key.clone(),
                    canonical_id: canonical_id.clone(),
                })
        })
        .collect::<Vec<_>>();
    let mut graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: graph_records,
        exact_bucket_assertions: Vec::new(),
        incumbent_ids,
    })?;
    graph.retained_surface_ids.extend(nodes.keys().cloned());
    let constraint_report = evaluate_signed_graph_components(&graph);

    let mut clusters = Vec::new();
    let mut contradictions = Vec::new();
    let max_cluster_surfaces = u64::try_from(rules.max_cluster_surfaces).map_err(|_| {
        solve_refusal(
            "The configured cluster size guard is too large for the solve budget",
            json!({
                "max_cluster_surfaces": rules.max_cluster_surfaces,
                "writes_performed": false
            }),
            "Lower guards.max_cluster_surfaces, then rerun canon entity session solve",
        )
    })?;
    let mut summary = SessionSolveSummary {
        node_count: nodes.len(),
        ..SessionSolveSummary::default()
    };
    let relation_suppressed_nodes = edge_facts
        .values()
        .filter(|facts| facts.positive && tier_for(facts, &rules) == Some(EdgeTier::C))
        .filter(|facts| {
            nodes_are_related(
                nodes.get(&facts.left),
                nodes.get(&facts.right),
                &related_pairs,
            )
        })
        .flat_map(|facts| [facts.left.clone(), facts.right.clone()])
        .collect::<BTreeSet<_>>();
    for component in constraint_report.components {
        let mut member_keys = component.surface_ids.clone();
        member_keys.sort();
        let cluster_id = cluster_id(&member_keys);
        let member_nodes = member_keys
            .iter()
            .filter_map(|key| nodes.get(key))
            .collect::<Vec<_>>();
        let joining = tiered_edges
            .iter()
            .filter(|((left, right), tier)| {
                matches!(tier, EdgeTier::A | EdgeTier::B)
                    && member_keys.binary_search(left).is_ok()
                    && member_keys.binary_search(right).is_ok()
            })
            .collect::<Vec<_>>();
        let has_positive = !joining.is_empty();
        let all_tier_a = has_positive && joining.iter().all(|(_, tier)| **tier == EdgeTier::A);
        let key_conflict = component_has_key_conflict(&member_nodes, &rules);
        let budget_decision = evaluate_solve_component_budget(
            SolveComponentBudgetInput::new(cluster_id.clone(), member_keys.clone()),
            SolveBudgetConfig::bounded_abstention(max_cluster_surfaces),
        )?;
        let oversized = budget_decision.action == SolveBudgetAction::Abstain;
        let incumbent_ids = member_nodes
            .iter()
            .filter_map(|node| node.incumbent_canonical_id.clone())
            .collect::<BTreeSet<_>>();
        let has_registry_endpoint = member_nodes
            .iter()
            .any(|node| node.observation_ids.is_empty());
        let relation_suppressed = member_keys
            .iter()
            .any(|key| relation_suppressed_nodes.contains(key));
        let mut conflict_reasons = Vec::new();
        if incumbent_ids.len() > 1 {
            conflict_reasons.push("incumbent_conflict".to_string());
        }
        if key_conflict {
            conflict_reasons.push("key_conflict".to_string());
        }
        let registry_cannot_link = registry_cannot_link_pairs.iter().any(|pair| {
            member_keys.binary_search(&pair.0).is_ok() && member_keys.binary_search(&pair.1).is_ok()
        });
        if registry_cannot_link {
            conflict_reasons.push("registry_cannot_link".to_string());
        }
        let distinct_conflict = same_node_distinct
            .iter()
            .any(|key| member_keys.contains(key))
            || !component.hard_cannot_link_violations.is_empty();
        let related_not_same = joining.iter().any(|((left, right), _)| {
            nodes_are_related(nodes.get(left), nodes.get(right), &related_pairs)
        });
        if related_not_same {
            conflict_reasons.push("related_not_same".to_string());
        }
        let (outcome, tier, reason) = if oversized {
            (
                "oversized_cluster",
                None,
                format!(
                    "oversized_cluster:surfaces={} bridging_edges={}",
                    member_keys.len(),
                    joining.len()
                ),
            )
        } else if related_not_same {
            ("contradiction", None, "related_not_same".to_string())
        } else if registry_cannot_link {
            ("contradiction", None, "registry_cannot_link".to_string())
        } else if distinct_conflict {
            ("contradiction", None, "distinct".to_string())
        } else if incumbent_ids.len() > 1 {
            ("contradiction", None, "incumbent_conflict".to_string())
        } else if key_conflict {
            ("contradiction", None, "key_conflict".to_string())
        } else if has_positive {
            if all_tier_a {
                ("accepted", Some("A".to_string()), "tier_a".to_string())
            } else {
                ("accepted", Some("B".to_string()), "tier_b".to_string())
            }
        } else if rules.mint_when_authoritative_key
            && member_nodes.len() == 1
            && exactly_one_authoritative_value(member_nodes[0], &rules)
        {
            (
                "accepted",
                Some("A".to_string()),
                "single_keyed".to_string(),
            )
        } else if incumbent_ids.len() == 1
            && member_nodes.iter().any(|node| {
                rules.authoritative_keys.iter().any(|namespace| {
                    node.observed_keys
                        .get(namespace)
                        .is_some_and(|values| !values.is_empty())
                })
            })
        {
            // The row states the existing ID's name and an authoritative key
            // together: the ID gains the key with no new spelling needed.
            (
                "accepted",
                Some("A".to_string()),
                "incumbent_keyed".to_string(),
            )
        } else if incumbent_ids.len() == 1 {
            (
                "resolved_existing",
                None,
                "incumbent_without_positive_evidence".to_string(),
            )
        } else {
            ("standalone", None, "no_positive_evidence".to_string())
        };
        let surfaces = member_nodes
            .iter()
            .map(|node| node.surface.clone())
            .collect::<Vec<_>>();
        let observation_count = member_nodes
            .iter()
            .map(|node| node.observation_ids.len())
            .sum();
        let keys = collect_cluster_keys(&member_nodes);
        let incumbent_canonical_id = (incumbent_ids.len() == 1)
            .then(|| incumbent_ids.iter().next().cloned())
            .flatten();
        let accepting_evidence_ids = if outcome == "accepted" && has_positive {
            joining
                .iter()
                .flat_map(|entry| {
                    edge_facts
                        .get(entry.0)
                        .into_iter()
                        .flat_map(|facts| facts.evidence_ids.iter().cloned())
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if outcome == "contradiction" {
            let mut evidence_ids = BTreeSet::new();
            let mut obs_ids = BTreeSet::new();
            for facts in edge_facts.values().filter(|facts| {
                member_keys.contains(&facts.left) && member_keys.contains(&facts.right)
            }) {
                evidence_ids.extend(facts.evidence_ids.iter().cloned());
                obs_ids.extend(facts.obs_ids.iter().cloned());
            }
            for node in &member_nodes {
                obs_ids.extend(node.observation_ids.iter().cloned());
            }
            contradictions.push(SessionSolveContradiction {
                reason: reason.clone(),
                surfaces: surfaces.clone(),
                obs_ids: obs_ids.into_iter().collect(),
                evidence_ids: evidence_ids.into_iter().collect(),
            });
            summary.contradiction += 1;
        } else if outcome == "accepted" {
            let existing_attachment = has_registry_endpoint && incumbent_ids.len() == 1;
            if existing_attachment {
                summary.resolved_existing += 1;
            }
            if !(relation_suppressed && existing_attachment) {
                if tier.as_deref() == Some("A") {
                    summary.accepted_tier_a += 1;
                } else {
                    summary.accepted_tier_b += 1;
                }
            }
        } else if outcome == "resolved_existing" {
            summary.resolved_existing += 1;
        } else if outcome == "oversized_cluster" {
            summary.oversized_cluster += 1;
        } else {
            summary.standalone += 1;
        }
        clusters.push(SessionSolveCluster {
            cluster_id,
            outcome: outcome.to_string(),
            tier,
            surfaces,
            observation_count,
            keys,
            incumbent_canonical_id,
            accepting_evidence_ids,
            reason,
            conflict_reasons,
        });
    }

    let mut review = Vec::new();
    let mut suppressed_by_relation = 0;
    let mut suppressed_by_cannot_link = 0;
    let mut review_new = 0;
    let mut review_known = 0;
    for facts in edge_facts
        .values()
        .filter(|facts| facts.positive && tier_for(facts, &rules) == Some(EdgeTier::C))
    {
        if nodes_are_related(
            nodes.get(&facts.left),
            nodes.get(&facts.right),
            &related_pairs,
        ) {
            suppressed_by_relation += 1;
            continue;
        }
        let pending_match = pending_for_pair(
            &facts.left,
            &facts.right,
            nodes.get(&facts.left),
            nodes.get(&facts.right),
            &registry_pending_clusters,
        );
        if registry_cannot_link_pairs.contains(&(facts.left.clone(), facts.right.clone())) {
            suppressed_by_cannot_link += 1;
            continue;
        }
        let evidence_types = facts
            .evidence_types
            .iter()
            .filter(|kind| is_positive_type(kind))
            .cloned()
            .collect::<Vec<_>>();
        let missing = if rules
            .required_evidence_types
            .iter()
            .any(|kind| !evidence_types.contains(kind))
        {
            "required_evidence_type"
        } else if evidence_types.len() < rules.min_distinct_evidence_types {
            "second_evidence_type"
        } else {
            "second_lineage"
        };
        if pending_match.is_some() {
            review_known += 1;
        } else {
            review_new += 1;
        }
        let escrow_id = Some(pending_match.clone().unwrap_or_else(|| {
            generated_pending_id(
                nodes
                    .get(&facts.left)
                    .map(|node| node.surface.as_str())
                    .unwrap_or(&facts.left),
                nodes
                    .get(&facts.right)
                    .map(|node| node.surface.as_str())
                    .unwrap_or(&facts.right),
            )
        }));
        review.push(SessionSolveReview {
            left_surface: nodes
                .get(&facts.left)
                .map(|node| node.surface.clone())
                .unwrap_or_default(),
            right_surface: nodes
                .get(&facts.right)
                .map(|node| node.surface.clone())
                .unwrap_or_default(),
            evidence_types,
            lineage_count: independent_origin_count(facts),
            missing: missing.to_string(),
            known_pending: pending_match.is_some(),
            escrow_id,
        });
    }
    review.sort_by(|left, right| {
        left.left_surface
            .cmp(&right.left_surface)
            .then_with(|| left.right_surface.cmp(&right.right_surface))
    });
    summary.review = review.len();
    summary.suppressed_by_relation = suppressed_by_relation;
    summary.suppressed_by_cannot_link = suppressed_by_cannot_link;
    summary.review_new = review_new;
    summary.review_known = review_known;

    let mut relations = edge_facts
        .values()
        .filter(|facts| !facts.relation.is_empty())
        .flat_map(|facts| {
            facts.relation.iter().map(|relation| SessionSolveRelation {
                left_surface: nodes
                    .get(&facts.left)
                    .map(|node| node.surface.clone())
                    .unwrap_or_default(),
                right_surface: nodes
                    .get(&facts.right)
                    .map(|node| node.surface.clone())
                    .unwrap_or_default(),
                subject_surface: nodes
                    .get(facts.relation_subject.get(relation).unwrap_or(&facts.left))
                    .map(|node| node.surface.clone())
                    .unwrap_or_default(),
                object_surface: nodes
                    .get(
                        if facts
                            .relation_subject
                            .get(relation)
                            .is_some_and(|subject| subject == &facts.right)
                        {
                            &facts.left
                        } else {
                            &facts.right
                        },
                    )
                    .map(|node| node.surface.clone())
                    .unwrap_or_default(),
                relation: relation.clone(),
                evidence_ids: facts.evidence_ids.iter().cloned().collect(),
                subject_entity_type: node_entity_type(
                    facts.relation_subject.get(relation).unwrap_or(&facts.left),
                ),
                object_entity_type: node_entity_type(
                    if facts
                        .relation_subject
                        .get(relation)
                        .is_some_and(|subject| subject == &facts.right)
                    {
                        &facts.left
                    } else {
                        &facts.right
                    },
                ),
                object_external_id: None,
                valid_time: TimeInterval::default(),
            })
        })
        .collect::<Vec<_>>();
    relations.extend(role_relations);
    relations.sort_by(|left, right| {
        left.subject_surface
            .cmp(&right.subject_surface)
            .then_with(|| left.object_surface.cmp(&right.object_surface))
            .then_with(|| left.relation.cmp(&right.relation))
            .then_with(|| left.evidence_ids.cmp(&right.evidence_ids))
    });
    relations.dedup();
    summary.relations = relations.len();
    summary.dangling_evidence = dangling_evidence_count;
    summary.cluster_count = clusters.len();
    clusters.sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));
    derived_evidence.sort_by(|left, right| left.evidence_id.cmp(&right.evidence_id));
    contradictions.sort_by(|left, right| {
        left.reason
            .cmp(&right.reason)
            .then_with(|| left.surfaces.cmp(&right.surfaces))
    });
    let standalone_count = summary.standalone;

    let artifact = SessionSolveArtifact {
        version: SESSION_SOLVE_VERSION.to_string(),
        inputs: SessionSolveInputs {
            observations_generation: observations_snapshot.generation_id.clone(),
            evidence_generation: evidence_snapshot
                .as_ref()
                .map(|snapshot| snapshot.generation_id.clone()),
            rules_hash,
            registry: registry_metadata,
        },
        authoritative_keys: rules.authoritative_keys.clone(),
        commit_key_namespaces: rules.commit_key_namespaces.clone(),
        summary,
        clusters,
        review,
        relations,
        contradictions,
        standalone_count,
        dangling_evidence_count,
        derived_evidence,
    };
    let bytes = serde_json::to_vec(&artifact).map_err(|error| {
        solve_refusal(
            format!("Failed to serialize session solve artifact: {error}"),
            json!({"writes_performed": false}),
            "Rerun canon entity session solve after repairing the session inputs",
        )
    })?;
    let mut upstream_artifacts = vec![EntityPublicationUpstreamRef {
        version: SESSION_OBSERVATIONS_STREAM_ID.to_string(),
        content_hash: observations_snapshot.generation_id.clone(),
    }];
    if let Some(snapshot) = evidence_snapshot.as_ref() {
        upstream_artifacts.push(EntityPublicationUpstreamRef {
            version: SESSION_EVIDENCE_STREAM_ID.to_string(),
            content_hash: snapshot.generation_id.clone(),
        });
    }
    let request = EntityPublicationRequest {
        stream_id: SESSION_SOLVE_STREAM_ID.to_string(),
        supersedes_generation_id: open_optional_stream(&session, SESSION_SOLVE_STREAM_ID)?
            .map(|snapshot| snapshot.generation_id),
        request_fingerprint: hash_bytes(SESSION_SOLVE_VERSION.as_bytes()),
        cache_mode: "disabled".to_string(),
        cache_status: "cold_start".to_string(),
        cache_receipt_hash: hash_bytes(b"canon.entity.session.solve.receipt.v1"),
        stage_order: vec![SESSION_PUBLICATION_STAGE.to_string()],
        upstream_artifacts,
        files: vec![EntityPublicationFileInput::new(
            "solve/solve.json",
            SESSION_PUBLICATION_STAGE,
            SESSION_SOLVE_VERSION,
            bytes,
        )],
        omit_logical_paths: Vec::new(),
    };
    let receipt = publish_stream_patch(&session, request).map_err(|error| {
        solve_refusal(
            format!("Unable to publish session solve: {error}"),
            json!({"stream": SESSION_SOLVE_STREAM_ID, "writes_performed": error.writes_performed}),
            "Rerun the same canon entity session solve command",
        )
    })?;
    Ok(SessionSolveOutput {
        artifact,
        generation_id: receipt.generation_id,
        artifact_path: "solve/solve.json".to_string(),
    })
}

pub fn commit(
    request: EntitySessionCommitRequest,
) -> Result<session::SessionCommitOutput, Refusal> {
    let session = absolute_session_path(&request.session)?;
    let manifest = read_manifest(&session)?;
    if manifest.registries.is_empty() {
        return Err(solve_refusal(
            "The session manifest has no registry binding",
            json!({"field": "registries", "writes_performed": false}),
            "Reinitialize the session with --registry, then rerun canon entity session commit",
        ));
    }
    let solve_snapshot = open_required_stream(&session, SESSION_SOLVE_STREAM_ID, "solve")?;
    let solve_bytes = solve_snapshot
        .read_logical_file("solve/solve.json")
        .ok_or_else(|| {
            solve_refusal(
                "Session solve generation is missing solve/solve.json",
                json!({"stream": SESSION_SOLVE_STREAM_ID, "writes_performed": false}),
                "Run canon entity session solve, then rerun canon entity session commit",
            )
        })?;
    let solve_hash = hash_bytes(solve_bytes);
    let artifact =
        serde_json::from_slice::<SessionSolveArtifact>(solve_bytes).map_err(|error| {
            solve_refusal(
                format!("Session solve artifact is invalid: {error}"),
                json!({"stream": SESSION_SOLVE_STREAM_ID, "writes_performed": false}),
                "Run canon entity session solve, then rerun canon entity session commit",
            )
        })?;
    let observations_snapshot =
        open_required_stream(&session, SESSION_OBSERVATIONS_STREAM_ID, "observations")?;
    if artifact.inputs.observations_generation != observations_snapshot.generation_id {
        return Err(stale_solve_refusal(
            "observations_generation",
            &artifact.inputs.observations_generation,
            &observations_snapshot.generation_id,
        ));
    }
    let evidence_snapshot = open_optional_stream(&session, SESSION_EVIDENCE_STREAM_ID)?;
    let evidence_generation = evidence_snapshot
        .as_ref()
        .map(|snapshot| snapshot.generation_id.clone());
    if artifact.inputs.evidence_generation != evidence_generation {
        return Err(stale_solve_refusal(
            "evidence_generation",
            artifact
                .inputs
                .evidence_generation
                .as_deref()
                .unwrap_or("<none>"),
            evidence_generation.as_deref().unwrap_or("<none>"),
        ));
    }
    let evidence_records = evidence_snapshot
        .as_ref()
        .map(read_evidence)
        .transpose()?
        .unwrap_or_default();
    let observation_values = read_observation_values(&observations_snapshot)?;
    let mut current_metadata_parts = Vec::new();
    let mut current_registries = BTreeMap::new();
    for binding in &manifest.registries {
        let registry_path = Path::new(&binding.path);
        let loaded_registry = registry::load_registry(registry_path)
            .map_err(|error| Refusal::bad_registry(&binding.path, &error.to_string()))?;
        let registry_snapshot = registry_snapshot_hash(
            registry_path,
            &loaded_registry.meta.id,
            &loaded_registry.meta.version,
        )?;
        current_metadata_parts.push((
            binding.kind.clone(),
            SessionSolveRegistry {
                id: loaded_registry.meta.id.clone(),
                version: loaded_registry.meta.version.clone(),
                snapshot_hash: registry_snapshot.clone(),
                escrow_snapshot_hash: registry_escrow_snapshot_hash(registry_path)?,
            },
        ));
        current_registries.insert(binding.kind.clone(), (loaded_registry, registry_snapshot));
    }
    let current_metadata = aggregate_registry_metadata(&current_metadata_parts);
    let mut partial_commit = false;
    for binding in &manifest.registries {
        let (loaded, _) = current_registries.get(&binding.kind).ok_or_else(|| {
            solve_refusal(
                format!("Registry state for kind '{}' is missing", binding.kind),
                json!({"kind": binding.kind, "writes_performed": false}),
                "Repair the session registry map, then rerun commit",
            )
        })?;
        if session::already_committed(&session, &solve_hash, &loaded.meta.id, &loaded.meta.version)?
            .is_some()
        {
            partial_commit = true;
            break;
        }
    }
    if current_metadata != artifact.inputs.registry && !partial_commit {
        return Err(stale_solve_refusal(
            "registry",
            &format!(
                "{}@{}:{}",
                artifact.inputs.registry.id,
                artifact.inputs.registry.version,
                artifact.inputs.registry.snapshot_hash
            ),
            &format!(
                "{}@{}:{}",
                current_metadata.id, current_metadata.version, current_metadata.snapshot_hash
            ),
        ));
    }
    let known_kinds = manifest
        .registries
        .iter()
        .map(|binding| binding.kind.as_str())
        .collect::<Vec<_>>();
    // --expect-base is checked for every kind before any kind writes. A kind
    // whose solve is already committed is an exact repeat, not a moved base.
    if let Some(raw) = request.expect_base.as_deref() {
        for binding in &manifest.registries {
            let expected =
                registries::flag_value_for_kind("--expect-base", raw, &binding.kind, &known_kinds)
                    .map_err(|error| {
                        solve_refusal(
                            format!("Invalid --expect-base: {error}"),
                            json!({"field": "expect-base", "writes_performed": false}),
                            "Use --expect-base kind=content_hash for each mapped entity kind",
                        )
                    })?;
            let Some(expected) = expected else {
                continue;
            };
            let Some((loaded_registry, _)) = current_registries.get(&binding.kind) else {
                continue;
            };
            if session::already_committed(
                &session,
                &solve_hash,
                &loaded_registry.meta.id,
                &loaded_registry.meta.version,
            )?
            .is_some()
            {
                continue;
            }
            let actual = registry::registry_identity(Path::new(&binding.path))?;
            if actual.content_hash != expected {
                return Err(Refusal {
                    code: RefusalCode::EEntityRegistrySnapshot,
                    message: format!(
                        "Registry directory for kind '{}' is not the base the driver synced",
                        binding.kind
                    ),
                    detail: json!({
                        "reason": "base_moved",
                        "kind": binding.kind,
                        "expected": expected,
                        "actual": actual.content_hash,
                        "writes_performed": false,
                    }),
                    next_command: Some(format!(
                        "Sync the registry directory again, or rerun with --expect-base {}",
                        actual.content_hash
                    )),
                });
            }
        }
    }
    let mut final_output = None;
    for binding in &manifest.registries {
        let (loaded_registry, registry_snapshot) =
            current_registries.get(&binding.kind).ok_or_else(|| {
                solve_refusal(
                    format!("Registry state for kind '{}' is missing", binding.kind),
                    json!({"kind": binding.kind, "writes_performed": false}),
                    "Repair the session registry map, then rerun commit",
                )
            })?;
        let expected_version =
            expected_registry_version(&artifact.inputs.registry.version, &binding.kind);
        let already = session::already_committed(
            &session,
            &solve_hash,
            &loaded_registry.meta.id,
            &loaded_registry.meta.version,
        )?;
        if let Some(already) = already {
            final_output = Some(merge_commit_outputs(final_output.take(), already));
            continue;
        }
        if !partial_commit
            && expected_version.as_deref() != Some(loaded_registry.meta.version.as_str())
        {
            return Err(stale_solve_refusal(
                "registry",
                &format!("{}@{}", binding.id, expected_version.unwrap_or_default()),
                &format!(
                    "{}@{}",
                    loaded_registry.meta.id, loaded_registry.meta.version
                ),
            ));
        }
        let plan = build_commit_plan(
            &artifact,
            CommitPlanContext {
                entity_kind: &binding.kind,
                alias_file: binding.alias_file.as_deref().unwrap_or("aliases.json"),
                key_file: binding.key_file.as_deref(),
                canonical_type: binding.canonical_type.as_deref().unwrap_or(&binding.kind),
                session_id: &manifest.session_id,
                solve_hash: &solve_hash,
                registry_version: &loaded_registry.meta.version,
                observation_values: &observation_values,
            },
        )?;
        let SessionCommitPlan {
            entries,
            skipped,
            relations,
        } = plan;
        let memory = build_registry_memory_write(
            &artifact,
            &evidence_records,
            &observation_values,
            &binding.kind,
            Path::new(&binding.path),
        )?;
        let next_version = match request.next_version.as_deref() {
            Some(raw) => {
                registries::flag_value_for_kind("--next-version", raw, &binding.kind, &known_kinds)
                    .map_err(|error| {
                        solve_refusal(
                            format!("Invalid --next-version: {error}"),
                            json!({"field": "next-version", "writes_performed": false}),
                            "Use --next-version kind=version for each mapped entity kind",
                        )
                    })?
            }
            None => None,
        };
        let output = session::commit(session::SessionCommitRequest {
            session: session.clone(),
            registry_kind: Some(binding.kind.clone()),
            solve_hash: solve_hash.clone(),
            observations_generation: artifact.inputs.observations_generation.clone(),
            evidence_generation: artifact.inputs.evidence_generation.clone(),
            registry: SessionCommitRegistryExpectation {
                id: loaded_registry.meta.id.clone(),
                version: loaded_registry.meta.version.clone(),
                snapshot_hash: registry_snapshot.clone(),
            },
            entries,
            skipped,
            relations,
            memory: Some(memory),
            next_version,
            bump: request.bump,
        })?;
        final_output = Some(merge_commit_outputs(final_output.take(), output));
    }
    final_output.ok_or_else(|| {
        solve_refusal(
            "Session commit produced no registry result",
            json!({"writes_performed": false}),
            "Rerun canon entity session commit",
        )
    })
}

/// Combine the output of one kind's commit with what earlier kinds produced.
/// A kind with nothing to commit must not mask a kind that did commit; later
/// committed outputs already carry the receipt blocks of the earlier ones.
fn merge_commit_outputs(
    previous: Option<session::SessionCommitOutput>,
    next: session::SessionCommitOutput,
) -> session::SessionCommitOutput {
    match previous {
        Some(previous)
            if next.outcome == "nothing_to_commit" && previous.outcome != "nothing_to_commit" =>
        {
            previous
        }
        _ => next,
    }
}

fn expected_registry_version(version: &str, kind: &str) -> Option<String> {
    if !version.contains('=') {
        return Some(version.to_string());
    }
    version.split(',').find_map(|part| {
        let (candidate, value) = part.split_once('=')?;
        (candidate == kind).then(|| value.to_string())
    })
}

fn cluster_entity_kind(
    cluster: &SessionSolveCluster,
    observation_values: &BTreeMap<String, Vec<Value>>,
) -> Option<String> {
    let kinds = cluster
        .surfaces
        .iter()
        .flat_map(|surface| observation_values.get(surface).into_iter().flatten())
        .filter_map(|value| value.get("entity_type").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut matching = kinds
        .iter()
        .filter_map(|kind| {
            let mut member_keys = cluster
                .surfaces
                .iter()
                .map(|surface| format!("{kind}\u{0}{surface}"))
                .collect::<Vec<_>>();
            member_keys.sort();
            (cluster_id(&member_keys) == cluster.cluster_id).then(|| (*kind).to_string())
        })
        .collect::<Vec<_>>();
    if matching.len() == 1 {
        matching.pop()
    } else if kinds.len() == 1 {
        kinds.iter().next().map(|kind| (*kind).to_string())
    } else {
        None
    }
}

fn build_registry_memory_write(
    artifact: &SessionSolveArtifact,
    evidence: &[crate::entity::evidence_ir::EvidenceRecord],
    observation_values: &BTreeMap<String, Vec<Value>>,
    entity_kind: &str,
    registry_path: &Path,
) -> Result<SessionRegistryMemoryWrite, Refusal> {
    let mut cannot_link = BTreeMap::<(String, String), CannotLinkFact>::new();
    let mut pending = BTreeMap::<String, PendingClusterRecord>::new();
    let mut review_pending = BTreeSet::<String>::new();
    let mut pending_clear = BTreeSet::new();

    for record in evidence {
        if record.operator.operator_id != "distinct" || record.authority_basis.is_none() {
            continue;
        }
        let crate::entity::evidence_ir::EvidenceTarget::Pair { left_id, right_id } = &record.target
        else {
            continue;
        };
        if !targets_match_kind(left_id, right_id, observation_values, entity_kind) {
            continue;
        }
        let left_key = memory_key_for_target(left_id, observation_values);
        let right_key = memory_key_for_target(right_id, observation_values);
        if left_key == right_key {
            continue;
        }
        let (left_key, right_key) = ordered_pair(&left_key, &right_key);
        cannot_link
            .entry((left_key.clone(), right_key.clone()))
            .or_insert(CannotLinkFact {
                left_key,
                right_key,
                reason: record.reason_code.clone(),
            });
        clear_pending_for_surfaces(
            &mut pending_clear,
            registry_path,
            target_surfaces(left_id, right_id, observation_values),
        )?;
    }

    let contradicted_surfaces = artifact
        .contradictions
        .iter()
        .flat_map(|contradiction| contradiction.surfaces.iter().cloned())
        .collect::<BTreeSet<_>>();
    for review in &artifact.review {
        if !surfaces_match_kind(
            [&review.left_surface, &review.right_surface],
            observation_values,
            entity_kind,
        ) {
            continue;
        }
        let escrow_id = review
            .escrow_id
            .clone()
            .unwrap_or_else(|| generated_pending_id(&review.left_surface, &review.right_surface));
        let surfaces =
            ordered_unique_strings([review.left_surface.clone(), review.right_surface.clone()]);
        let record = PendingClusterRecord {
            escrow_id: escrow_id.clone(),
            profile: "canon.entity.session".to_string(),
            doc_ids: observation_ids_for_surfaces(&surfaces, observation_values),
            surfaces: surfaces.clone(),
            anchors: anchors_for_surfaces(&surfaces, observation_values),
            witness_pairs: Vec::new(),
            state: "pending".to_string(),
        };
        // A pair inside a contradicted cluster is parked, but it is not a
        // review question of its own: the contradiction already says why.
        // Nor is a pair with a registry endpoint: the registry already holds
        // that side, so there is no unresolved question between two sessions'
        // worth of rows to remember.
        if !contradicted_surfaces.contains(&review.left_surface)
            && !contradicted_surfaces.contains(&review.right_surface)
            && observation_values.contains_key(&review.left_surface)
            && observation_values.contains_key(&review.right_surface)
        {
            review_pending.insert(escrow_id.clone());
        }
        pending.insert(escrow_id, record);
    }

    for contradiction in &artifact.contradictions {
        if contradiction.reason == "registry_cannot_link" || contradiction.reason == "distinct" {
            clear_pending_for_surfaces(
                &mut pending_clear,
                registry_path,
                contradiction.surfaces.clone(),
            )?;
            continue;
        }
        if !surfaces_match_kind(
            contradiction.surfaces.iter().collect::<Vec<_>>(),
            observation_values,
            entity_kind,
        ) {
            continue;
        }
        let surfaces = ordered_unique_strings(contradiction.surfaces.clone());
        let escrow_id = generated_pending_id_for_many(&surfaces);
        pending.insert(
            escrow_id.clone(),
            PendingClusterRecord {
                escrow_id,
                profile: "canon.entity.session".to_string(),
                doc_ids: contradiction.obs_ids.clone(),
                surfaces: surfaces.clone(),
                anchors: anchors_for_surfaces(&surfaces, observation_values),
                witness_pairs: Vec::new(),
                state: "pending".to_string(),
            },
        );
    }

    for cluster in &artifact.clusters {
        let accepted =
            cluster.outcome == "accepted" && matches!(cluster.tier.as_deref(), Some("A" | "B"));
        if accepted || cluster.outcome == "resolved_existing" {
            clear_pending_for_surfaces(
                &mut pending_clear,
                registry_path,
                cluster.surfaces.clone(),
            )?;
        }
    }
    for relation in &artifact.relations {
        clear_pending_for_surfaces(
            &mut pending_clear,
            registry_path,
            vec![
                relation.left_surface.clone(),
                relation.right_surface.clone(),
            ],
        )?;
    }

    Ok(SessionRegistryMemoryWrite {
        cannot_link: cannot_link.into_values().collect(),
        pending: pending.into_values().collect(),
        pending_clear: pending_clear.into_iter().collect(),
        review_pending: review_pending.into_iter().collect(),
    })
}

fn targets_match_kind(
    left: &str,
    right: &str,
    observation_values: &BTreeMap<String, Vec<Value>>,
    entity_kind: &str,
) -> bool {
    [left, right].iter().all(|target| {
        observation_values
            .values()
            .flatten()
            .find_map(|value| {
                (value.get("obs_id").and_then(Value::as_str) == Some(*target))
                    .then(|| value.get("entity_type").and_then(Value::as_str))
                    .flatten()
            })
            .is_none_or(|kind| kind == entity_kind)
    })
}

fn surfaces_match_kind<'a>(
    surfaces: impl IntoIterator<Item = &'a String>,
    observation_values: &BTreeMap<String, Vec<Value>>,
    entity_kind: &str,
) -> bool {
    surfaces.into_iter().all(|surface| {
        observation_values
            .get(surface.as_str())
            .into_iter()
            .flatten()
            .all(|value| value.get("entity_type").and_then(Value::as_str) == Some(entity_kind))
    })
}

fn memory_key_for_target(
    target: &str,
    observation_values: &BTreeMap<String, Vec<Value>>,
) -> String {
    if target.starts_with("registry:") {
        return target.to_string();
    }
    let Some(value) = observation_values
        .values()
        .flatten()
        .find(|value| value.get("obs_id").and_then(Value::as_str) == Some(target))
    else {
        return target.to_string();
    };
    let mut keys = value
        .get("keys")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|object| object.iter())
        .filter_map(|(namespace, value)| {
            value
                .as_str()
                .and_then(|value| canonical_key_value(namespace, value))
                .map(|value| format!("{}:{value}", canonical_key_namespace(namespace)))
        })
        .collect::<Vec<_>>();
    keys.sort();
    keys.into_iter().next().unwrap_or_else(|| {
        format!(
            "surface:{}",
            value
                .get("surface")
                .and_then(Value::as_str)
                .unwrap_or(target)
        )
    })
}

fn target_surfaces(
    left: &str,
    right: &str,
    observation_values: &BTreeMap<String, Vec<Value>>,
) -> Vec<String> {
    [left, right]
        .iter()
        .filter_map(|target| {
            observation_values.values().flatten().find_map(|value| {
                (value.get("obs_id").and_then(Value::as_str) == Some(*target))
                    .then(|| value.get("surface").and_then(Value::as_str))
                    .flatten()
                    .map(str::to_string)
            })
        })
        .collect()
}

fn observation_ids_for_surfaces(
    surfaces: &[String],
    observation_values: &BTreeMap<String, Vec<Value>>,
) -> Vec<String> {
    let mut ids = observation_values
        .values()
        .flatten()
        .filter(|value| {
            value
                .get("surface")
                .and_then(Value::as_str)
                .is_some_and(|surface| surfaces.iter().any(|candidate| candidate == surface))
        })
        .filter_map(|value| {
            value
                .get("obs_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn anchors_for_surfaces(
    surfaces: &[String],
    observation_values: &BTreeMap<String, Vec<Value>>,
) -> Vec<AnchorValue> {
    let mut anchors = observation_values
        .values()
        .flatten()
        .filter(|value| {
            value
                .get("surface")
                .and_then(Value::as_str)
                .is_some_and(|surface| surfaces.iter().any(|candidate| candidate == surface))
        })
        .flat_map(|value| {
            value
                .get("keys")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|object| object.iter())
                .filter_map(|(namespace, value)| {
                    value.as_str().map(|value| AnchorValue {
                        namespace: canonical_key_namespace(namespace),
                        value: canonical_key_value(namespace, value)
                            .unwrap_or_else(|| value.to_string()),
                    })
                })
        })
        .collect::<Vec<_>>();
    anchors.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then(left.value.cmp(&right.value))
    });
    anchors.dedup();
    anchors
}

fn ordered_unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn generated_pending_id_for_many(surfaces: &[String]) -> String {
    let surfaces = ordered_unique_strings(surfaces.iter().cloned());
    let digest = blake3::hash(surfaces.join("\n").as_bytes())
        .to_hex()
        .to_string();
    format!("OE-{}", &digest[..12])
}

fn clear_pending_for_surfaces(
    pending_clear: &mut BTreeSet<String>,
    registry_path: &Path,
    surfaces: Vec<String>,
) -> Result<(), Refusal> {
    if surfaces.is_empty() {
        return Ok(());
    }
    let memory = crate::entity::runtime::incumbent::load_incumbent_memory(registry_path).map_err(
        |error| {
            solve_refusal(
                format!(
                    "Unable to load registry memory '{}': {error}",
                    registry_path.display()
                ),
                json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
                "Repair registry memory sidecars, then rerun canon entity session commit",
            )
        },
    )?;
    let surfaces = surfaces.into_iter().collect::<BTreeSet<_>>();
    for record in memory.pending_clusters {
        let overlap = record
            .surfaces
            .iter()
            .filter(|surface| surfaces.contains(*surface))
            .count();
        if overlap >= 2 || (overlap == 1 && record.surfaces.len() == 1) {
            pending_clear.insert(record.escrow_id);
        }
    }
    Ok(())
}

fn build_commit_plan(
    artifact: &SessionSolveArtifact,
    context: CommitPlanContext<'_>,
) -> Result<SessionCommitPlan, Refusal> {
    let mut clusters = artifact.clusters.clone();
    clusters.sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    for cluster in clusters {
        if cluster_entity_kind(&cluster, context.observation_values)
            .is_some_and(|kind| kind != context.entity_kind)
        {
            continue;
        }
        let accepted =
            cluster.outcome == "accepted" && matches!(cluster.tier.as_deref(), Some("A" | "B"));
        if !accepted {
            let cluster_id = cluster.cluster_id.clone();
            for reason in commit_skip_reasons(&cluster) {
                skipped.push(SessionCommitSkipped {
                    cluster_id: cluster_id.clone(),
                    reason,
                    detail: None,
                });
            }
            continue;
        }
        let tier = cluster.tier.clone().unwrap_or_default();
        let mut surfaces = cluster.surfaces.clone();
        surfaces.sort();
        let mut aliases = Vec::new();
        let mut seen_inputs = BTreeSet::new();
        for surface in surfaces {
            if surface.starts_with("registry:") {
                continue;
            }
            if seen_inputs.insert(surface.clone()) {
                aliases.push(SessionCommitAlias {
                    alias_file: context.alias_file.to_string(),
                    input: surface,
                    canonical_type: context.canonical_type.to_string(),
                    rule_id: format!("SESSION_TIER_{tier}"),
                    skip_if_existing: cluster.incumbent_canonical_id.is_some(),
                    session_authority: None,
                    closure: None,
                });
            }
        }
        let key_namespaces = artifact
            .authoritative_keys
            .iter()
            .chain(artifact.commit_key_namespaces.iter())
            .map(|namespace| canonical_key_namespace(namespace))
            .collect::<BTreeSet<_>>();
        for namespace in key_namespaces {
            let Some(values) = cluster.keys.get(&namespace) else {
                continue;
            };
            let mut values = values.clone();
            values.sort();
            for value in values {
                let invalid_value = value.clone();
                let value = canonical_key_value(&namespace, &value).ok_or_else(|| {
                    solve_refusal(
                        format!(
                            "Accepted cluster {} contains an invalid {} key",
                            cluster.cluster_id, namespace
                        ),
                        json!({
                            "cluster_id": cluster.cluster_id.clone(),
                            "namespace": namespace.clone(),
                            "value": invalid_value,
                            "writes_performed": false
                        }),
                        "Repair the session observation key, then rerun solve and commit",
                    )
                })?;
                if seen_inputs.insert(value.clone()) {
                    aliases.push(SessionCommitAlias {
                        alias_file: context
                            .key_file
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("keys.{namespace}.json")),
                        input: value,
                        canonical_type: context.canonical_type.to_string(),
                        rule_id: format!("SESSION_KEY_{}", namespace.to_ascii_uppercase()),
                        skip_if_existing: true,
                        session_authority: None,
                        closure: None,
                    });
                }
            }
        }
        let cluster_observations = cluster
            .surfaces
            .iter()
            .flat_map(|surface| {
                context
                    .observation_values
                    .get(surface)
                    .into_iter()
                    .flatten()
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut cluster_observations = cluster_observations;
        cluster_observations.sort_by(|left, right| {
            left.get("obs_id")
                .and_then(Value::as_str)
                .cmp(&right.get("obs_id").and_then(Value::as_str))
        });
        let provenance_context = crate::entity::session_provenance::ProvenanceContext {
            session_id: context.session_id,
            solve_hash: context.solve_hash,
            rules_hash: &artifact.inputs.rules_hash,
            registry_id: &artifact.inputs.registry.id,
            registry_version: context.registry_version,
            cluster: &cluster,
            canonical_id: cluster.incumbent_canonical_id.as_deref(),
            sensitivity: None,
        };
        let cluster_subject = crate::entity::session_provenance::outcome_subject(&cluster);
        let cluster_authority = crate::entity::session_provenance::build_authority_material_for_subject_with_observations(
            provenance_context,
            crate::entity::session_provenance::ProvenanceSubject {
                kind: if cluster.incumbent_canonical_id.is_some() {
                    "attached_alias"
                } else {
                    "minted_id"
                },
                subject: &cluster_subject,
                observation_values: &cluster_observations,
            },
        )
        .map_err(|error| {
            solve_refusal(
                format!(
                    "Unable to build evidence closure for accepted cluster {}: {error}",
                    cluster.cluster_id
                ),
                json!({"cluster_id": cluster.cluster_id, "writes_performed": false}),
                "Rerun canon entity session solve, then rerun commit",
            )
        })?;
        let mut entry_authority = None;
        for alias in &mut aliases {
            let (kind, subject) = if cluster.incumbent_canonical_id.is_some() {
                if let Some(namespace) = alias.rule_id.strip_prefix("SESSION_KEY_") {
                    (
                        "key_alias",
                        format!("{}:{}", namespace.to_ascii_lowercase(), alias.input),
                    )
                } else {
                    ("attached_alias", alias.input.clone())
                }
            } else {
                (
                    "minted_id",
                    crate::entity::session_provenance::outcome_subject(&cluster),
                )
            };
            let material = if kind == "minted_id" {
                cluster_authority.clone()
            } else {
                crate::entity::session_provenance::build_authority_material_for_subject(
                    provenance_context,
                    crate::entity::session_provenance::ProvenanceSubject {
                        kind,
                        subject: &subject,
                        observation_values: &[],
                    },
                )
                .map_err(|error| {
                    solve_refusal(
                        format!(
                            "Unable to build evidence closure for alias {} in cluster {}: {error}",
                            alias.input, cluster.cluster_id
                        ),
                        json!({"cluster_id": cluster.cluster_id, "input": alias.input, "writes_performed": false}),
                        "Rerun canon entity session solve, then rerun commit",
                    )
                })?
            };
            if entry_authority.is_none() {
                entry_authority = Some(material.clone());
            }
            alias.session_authority = Some(material.authority);
            alias.closure = Some(SessionCommitClosure {
                outcome_ref: material.closure.outcome_ref,
                closure_hash: material.closure.closure_hash,
                manifest: material.closure.manifest,
            });
        }
        let authority = entry_authority.unwrap_or(cluster_authority);
        entries.push(SessionCommitEntry {
            cluster_id: cluster.cluster_id.clone(),
            tier,
            outcome_ref: authority.closure.outcome_ref.clone(),
            closure_hash: authority.closure.closure_hash.clone(),
            canonical_id: cluster.incumbent_canonical_id.clone(),
            aliases,
            session_authority: Some(authority.authority),
            closure: Some(SessionCommitClosure {
                outcome_ref: authority.closure.outcome_ref,
                closure_hash: authority.closure.closure_hash,
                manifest: authority.closure.manifest,
            }),
        });
    }
    let relations = artifact
        .relations
        .iter()
        .filter(|relation| relation.subject_entity_type == context.entity_kind)
        .filter_map(|relation| {
            let subject = commit_relation_endpoint(&artifact.clusters, relation, true)?;
            let object = if let Some(external_id) = &relation.object_external_id {
                SessionCommitRelationEndpoint {
                    canonical_id: None,
                    cluster_id: None,
                    external_id: Some(external_id.clone()),
                    entity_type: relation.object_entity_type.clone(),
                }
            } else {
                commit_relation_endpoint(&artifact.clusters, relation, false)?
            };
            Some(SessionCommitRelation {
                subject,
                relation: relation.relation.clone(),
                object,
                valid_time: relation.valid_time.clone(),
                evidence_ids: relation.evidence_ids.clone(),
            })
        })
        .collect();
    Ok(SessionCommitPlan {
        entries,
        skipped,
        relations,
    })
}

fn commit_relation_endpoint(
    clusters: &[SessionSolveCluster],
    relation: &SessionSolveRelation,
    subject: bool,
) -> Option<SessionCommitRelationEndpoint> {
    let surface = if subject {
        &relation.subject_surface
    } else {
        &relation.object_surface
    };
    clusters
        .iter()
        .find(|cluster| {
            cluster
                .surfaces
                .iter()
                .any(|candidate| candidate == surface)
                && ((cluster.outcome == "accepted"
                    && matches!(cluster.tier.as_deref(), Some("A" | "B")))
                    || (cluster.outcome == "resolved_existing"
                        && cluster.incumbent_canonical_id.is_some()))
        })
        .map(|cluster| SessionCommitRelationEndpoint {
            canonical_id: cluster.incumbent_canonical_id.clone(),
            cluster_id: cluster
                .incumbent_canonical_id
                .is_none()
                .then(|| cluster.cluster_id.clone()),
            external_id: None,
            entity_type: if subject {
                relation.subject_entity_type.clone()
            } else {
                relation.object_entity_type.clone()
            },
        })
}

fn commit_skip_reasons(cluster: &SessionSolveCluster) -> Vec<String> {
    let mut reasons = Vec::new();
    if cluster.reason == "key_conflict" {
        reasons.push("key_conflict".to_string());
    } else if cluster.reason == "incumbent_conflict" {
        if cluster
            .conflict_reasons
            .iter()
            .any(|reason| reason == "key_conflict")
        {
            reasons.push("incumbent_conflict".to_string());
            reasons.push("key_conflict".to_string());
        } else {
            reasons.push("contradiction".to_string());
        }
    }
    if reasons.is_empty() {
        let reason = if cluster.incumbent_canonical_id.is_some() {
            "incumbent"
        } else if cluster.outcome == "contradiction" {
            "contradiction"
        } else if cluster.outcome == "standalone" {
            "standalone"
        } else if cluster.outcome == "review" || cluster.tier.is_none() {
            "review"
        } else {
            "not_accepted"
        };
        reasons.push(reason.to_string());
    }
    reasons
}

fn stale_solve_refusal(field: &str, expected: &str, actual: &str) -> Refusal {
    solve_refusal(
        format!("Session solve is stale for {field}"),
        json!({
            "reason": "stale_solve",
            "field": field,
            "expected": expected,
            "actual": actual,
            "writes_performed": false,
        }),
        "Run canon entity session solve, then rerun canon entity session commit",
    )
}

fn read_manifest(work_dir: &Path) -> Result<SessionManifest, Refusal> {
    let snapshot = open_required_stream(work_dir, SESSION_MANIFEST_STREAM_ID, "session manifest")?;
    let bytes = snapshot
        .read_logical_file("session/session.json")
        .ok_or_else(|| {
            solve_refusal(
                "Session manifest generation is missing session/session.json",
                json!({"stream": SESSION_MANIFEST_STREAM_ID, "writes_performed": false}),
                "Initialize the session again in a new work directory",
            )
        })?;
    serde_json::from_slice(bytes).map_err(|error| {
        solve_refusal(
            format!("Session manifest is invalid: {error}"),
            json!({"stream": SESSION_MANIFEST_STREAM_ID, "writes_performed": false}),
            "Initialize the session again in a new work directory",
        )
    })
}

fn open_required_stream(
    work_dir: &Path,
    stream_id: &str,
    label: &str,
) -> Result<EntityPublicationSnapshot, Refusal> {
    open_current_stream_generation(work_dir, stream_id).map_err(|error| {
        solve_refusal(
            format!("No committed {label} generation is available: {error}"),
            json!({"stream": stream_id, "writes_performed": false}),
            format!("Run the session {label} command, then rerun canon entity session solve"),
        )
    })
}

fn open_optional_stream(
    work_dir: &Path,
    stream_id: &str,
) -> Result<Option<EntityPublicationSnapshot>, Refusal> {
    match open_current_stream_generation(work_dir, stream_id) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error)
            if error.kind
                == crate::entity::publication::EntityPublicationErrorKind::UncommittedGeneration =>
        {
            Ok(None)
        }
        Err(error) => Err(solve_refusal(
            format!("Unable to read session stream {stream_id}: {error}"),
            json!({"stream": stream_id, "writes_performed": false}),
            "Repair the session publication, then rerun canon entity session solve",
        )),
    }
}

fn read_observation_values(
    snapshot: &EntityPublicationSnapshot,
) -> Result<BTreeMap<String, Vec<Value>>, Refusal> {
    let mut values = BTreeMap::<String, Vec<Value>>::new();
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(logical_path).ok_or_else(|| {
            solve_refusal(
                format!("Observation shard {logical_path} is missing"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation stream, then rerun canon entity session commit",
            )
        })?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            solve_refusal(
                format!("Observation shard {logical_path} is not UTF-8: {error}"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation shard, then rerun canon entity session commit",
            )
        })?;
        for (line_number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str::<Value>(line).map_err(|error| {
                solve_refusal(
                    format!(
                        "Observation shard {logical_path} line {} is invalid: {error}",
                        line_number + 1
                    ),
                    json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "line": line_number + 1, "writes_performed": false}),
                    "Repair the observation stream, then rerun canon entity session commit",
                )
            })?;
            let Some(surface) = value.get("surface").and_then(Value::as_str) else {
                continue;
            };
            values.entry(surface.to_string()).or_default().push(value);
        }
    }
    for observations in values.values_mut() {
        observations.sort_by(|left, right| {
            left.get("obs_id")
                .and_then(Value::as_str)
                .cmp(&right.get("obs_id").and_then(Value::as_str))
        });
    }
    Ok(values)
}

fn read_observations(snapshot: &EntityPublicationSnapshot) -> Result<Vec<Observation>, Refusal> {
    let mut observations = BTreeMap::new();
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(logical_path).ok_or_else(|| {
            solve_refusal(
                format!("Observation shard {logical_path} is missing"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation stream, then rerun canon entity session solve",
            )
        })?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            solve_refusal(
                format!("Observation shard {logical_path} is not UTF-8: {error}"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation shard, then rerun canon entity session solve",
            )
        })?;
        for (line_index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let raw = serde_json::from_str::<RawObservation>(line).map_err(|error| {
                solve_refusal(
                    format!(
                        "Observation shard {logical_path} line {} is invalid: {error}",
                        line_index + 1
                    ),
                    json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "line": line_index + 1, "writes_performed": false}),
                    "Repair the observation stream, then rerun canon entity session solve",
                )
            })?;
            let obs_id = ascii_trim(&raw.obs_id).to_string();
            let entity_type = ascii_trim(&raw.entity_type).to_string();
            let surface = ascii_trim(&raw.surface).to_string();
            if obs_id.is_empty() || entity_type.is_empty() || surface.is_empty() {
                return Err(solve_refusal(
                    format!(
                        "Observation shard {logical_path} line {} has an empty identity field",
                        line_index + 1
                    ),
                    json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "line": line_index + 1, "writes_performed": false}),
                    "Repair the observation identity fields, then rerun canon entity session solve",
                ));
            }
            let keys = raw
                .keys
                .into_iter()
                .filter_map(|(key, value)| value_to_key_value(&key, value))
                .filter_map(|(namespace, value)| {
                    canonical_key_value(&namespace, &value)
                        .map(|value| (canonical_key_namespace(&namespace), value))
                })
                .collect::<BTreeMap<_, _>>();
            let (lineages, origin_components) = observation_origins(&raw.source);
            let node_key = format!("{entity_type}\u{0}{surface}");
            let observation = Observation {
                obs_id: obs_id.clone(),
                entity_type,
                surface,
                node_key,
                keys,
                lineages,
                origin_components,
            };
            if observations.insert(obs_id.clone(), observation).is_some() {
                return Err(solve_refusal(
                    format!("Observation id {obs_id} occurs more than once in the session"),
                    json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "obs_id": obs_id, "writes_performed": false}),
                    "Use one canonical observation row per obs_id, then rerun solve",
                ));
            }
        }
    }
    Ok(observations.into_values().collect())
}

fn observation_origins(
    source: &RawObservationSource,
) -> (BTreeSet<String>, BTreeSet<BTreeSet<String>>) {
    if let Some(ancestry) = &source.ancestry {
        if ancestry.status == "known" {
            let origins = ancestry
                .origin_source_ids
                .iter()
                .map(|origin| ascii_trim(origin).to_string())
                .filter(|origin| !origin.is_empty())
                .collect::<BTreeSet<_>>();
            if !origins.is_empty() {
                return (origins.clone(), BTreeSet::from([origins]));
            }
        }
        return (BTreeSet::new(), BTreeSet::new());
    }
    source
        .lineage
        .as_deref()
        .map(ascii_trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let lineage = value.to_string();
            (
                BTreeSet::from([lineage.clone()]),
                BTreeSet::from([BTreeSet::from([lineage])]),
            )
        })
        .unwrap_or_default()
}

fn read_evidence(
    snapshot: &EntityPublicationSnapshot,
) -> Result<Vec<crate::entity::evidence_ir::EvidenceRecord>, Refusal> {
    let mut records = BTreeMap::new();
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(logical_path).ok_or_else(|| {
            solve_refusal(
                format!("Evidence shard {logical_path} is missing"),
                json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
                "Repair the evidence stream, then rerun canon entity session solve",
            )
        })?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            solve_refusal(
                format!("Evidence shard {logical_path} is not UTF-8: {error}"),
                json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
                "Repair the evidence shard, then rerun canon entity session solve",
            )
        })?;
        for (line_index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record = serde_json::from_str::<crate::entity::evidence_ir::EvidenceRecord>(line)
                .map_err(|error| {
                    solve_refusal(
                        format!(
                            "Evidence shard {logical_path} line {} is invalid: {error}",
                            line_index + 1
                        ),
                        json!({"stream": SESSION_EVIDENCE_STREAM_ID, "line": line_index + 1, "writes_performed": false}),
                        "Repair the evidence stream, then rerun canon entity session solve",
                    )
                })?;
            let record = crate::entity::evidence_ir::canonicalize_record(record).map_err(|error| {
                solve_refusal(
                    format!(
                        "Evidence shard {logical_path} line {} is not canonical: {error}",
                        line_index + 1
                    ),
                    json!({"stream": SESSION_EVIDENCE_STREAM_ID, "line": line_index + 1, "writes_performed": false}),
                    "Repair the evidence stream, then rerun canon entity session solve",
                )
            })?;
            let id = record.evidence_id.clone();
            let bytes = serde_json::to_vec(&record).map_err(|error| {
                solve_refusal(
                    format!(
                        "Evidence shard {logical_path} line {} cannot be serialized: {error}",
                        line_index + 1
                    ),
                    json!({"stream": SESSION_EVIDENCE_STREAM_ID, "line": line_index + 1, "writes_performed": false}),
                    "Repair the evidence stream, then rerun canon entity session solve",
                )
            })?;
            if let Some(previous) = records.insert(id.clone(), (record, bytes.clone())) {
                let previous_bytes = serde_json::to_vec(&previous.0).map_err(|error| {
                    solve_refusal(
                        format!("Evidence id {id} cannot be compared: {error}"),
                        json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
                        "Repair the evidence stream, then rerun canon entity session solve",
                    )
                })?;
                if previous_bytes != bytes {
                    return Err(solve_refusal(
                        format!("Evidence id {id} occurs with conflicting bytes"),
                        json!({"stream": SESSION_EVIDENCE_STREAM_ID, "evidence_id": id, "writes_performed": false}),
                        "Repair the evidence stream, then rerun canon entity session solve",
                    ));
                }
            }
        }
    }
    Ok(records.into_values().map(|(record, _)| record).collect())
}

struct RegistryInfo {
    metadata: SessionSolveRegistry,
    incumbents: BTreeMap<String, String>,
    incumbent_keys: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    endpoints: BTreeMap<String, RegistryEndpoint>,
    cannot_link_facts: Vec<CannotLinkFact>,
    pending_clusters: Vec<PendingClusterRecord>,
}

struct CommitPlanContext<'a> {
    entity_kind: &'a str,
    alias_file: &'a str,
    key_file: Option<&'a str>,
    canonical_type: &'a str,
    session_id: &'a str,
    solve_hash: &'a str,
    registry_version: &'a str,
    observation_values: &'a BTreeMap<String, Vec<Value>>,
}

struct SessionCommitPlan {
    entries: Vec<SessionCommitEntry>,
    skipped: Vec<SessionCommitSkipped>,
    relations: Vec<SessionCommitRelation>,
}

fn aggregate_registry_metadata(parts: &[(String, SessionSolveRegistry)]) -> SessionSolveRegistry {
    if let Some((_, only)) = parts.first().filter(|_| parts.len() == 1) {
        return only.clone();
    }
    let mut bytes = Vec::new();
    for (kind, metadata) in parts {
        bytes.extend_from_slice(kind.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(metadata.id.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(metadata.version.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(metadata.snapshot_hash.as_bytes());
        bytes.push(0xff);
    }
    let mut escrow_bytes = Vec::new();
    for (kind, metadata) in parts {
        escrow_bytes.extend_from_slice(kind.as_bytes());
        escrow_bytes.push(0);
        escrow_bytes.extend_from_slice(metadata.escrow_snapshot_hash.as_bytes());
        escrow_bytes.push(0xff);
    }
    SessionSolveRegistry {
        id: parts
            .iter()
            .map(|(kind, metadata)| format!("{kind}={}", metadata.id))
            .collect::<Vec<_>>()
            .join(","),
        version: parts
            .iter()
            .map(|(kind, metadata)| format!("{kind}={}", metadata.version))
            .collect::<Vec<_>>()
            .join(","),
        snapshot_hash: hash_bytes(&bytes),
        escrow_snapshot_hash: hash_bytes(&escrow_bytes),
    }
}

fn load_registry_info(
    registry_path: &Path,
    observations: &[Observation],
    evidence: &[crate::entity::evidence_ir::EvidenceRecord],
    entity_type: &str,
) -> Result<RegistryInfo, Refusal> {
    let loaded = registry::load_registry(registry_path).map_err(|error| Refusal {
        code: RefusalCode::EBadRegistry,
        message: format!(
            "Unable to load session registry '{}': {error}",
            registry_path.display()
        ),
        detail: json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
        next_command: Some(
            "Repair the registry, then rerun canon entity session solve".to_string(),
        ),
    })?;
    let memory = crate::entity::runtime::incumbent::load_incumbent_memory(registry_path).map_err(
        |error| {
            solve_refusal(
                format!(
                    "Unable to load registry memory '{}': {error}",
                    registry_path.display()
                ),
                json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
                "Repair registry memory sidecars, then rerun canon entity session solve",
            )
        },
    )?;
    let observation_kinds = observations
        .iter()
        .map(|observation| {
            (
                observation.obs_id.as_str(),
                observation.entity_type.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut endpoint_targets = BTreeMap::new();
    for record in evidence {
        let Some((left, right)) = pair_target(record) else {
            continue;
        };
        let left_kind = observation_kinds.get(left).copied();
        let right_kind = observation_kinds.get(right).copied();
        if left_kind
            .or(right_kind)
            .is_some_and(|kind| kind != entity_type)
        {
            continue;
        }
        for target in [left, right] {
            if let Some((target, spec)) = registry_endpoint_spec(target) {
                endpoint_targets.insert(target, spec);
            }
        }
    }
    for observation in observations {
        for (namespace, value) in &observation.keys {
            let target = format!("registry:key:{namespace}:{value}");
            endpoint_targets
                .entry(target)
                .or_insert_with(|| RegistryEndpointSpec::Key {
                    namespace: namespace.clone(),
                    value: value.clone(),
                });
        }
    }
    for fact in &memory.cannot_link_facts {
        for key in [&fact.left_key, &fact.right_key] {
            if let Some((target, spec)) = registry_endpoint_spec(key) {
                endpoint_targets.entry(target).or_insert(spec);
            } else if !key.contains(':') && !key.starts_with("surface:") {
                endpoint_targets
                    .entry((*key).clone())
                    .or_insert_with(|| RegistryEndpointSpec::CanonicalId((*key).clone()));
            }
        }
    }
    let mut values = observations
        .iter()
        .map(|observation| (observation.surface.clone(), ()))
        .collect::<HashMap<_, _>>();
    for spec in endpoint_targets.values() {
        match spec {
            RegistryEndpointSpec::Alias(surface)
            | RegistryEndpointSpec::Key { value: surface, .. } => {
                values.insert(surface.clone(), ());
            }
            RegistryEndpointSpec::CanonicalId(_) => {}
        }
    }
    for observation in observations {
        for (namespace, value) in &observation.keys {
            values.extend(
                key_lookup_values(namespace, value)
                    .into_iter()
                    .map(|value| (value, ())),
            );
        }
    }
    for spec in endpoint_targets.values() {
        if let RegistryEndpointSpec::Key { namespace, value } = spec {
            values.extend(
                key_lookup_values(namespace, value)
                    .into_iter()
                    .map(|value| (value, ())),
            );
        }
    }
    let resolved = lookup::resolve_values(
        &loaded,
        &InputValues {
            values,
            special: HashMap::<SpecialReason, usize>::new(),
            format: InputFormat::Jsonl,
            delimiter: None,
            source_hash: None,
            source_bytes: None,
        },
    )
    .map_err(|error| Refusal {
        code: RefusalCode::EBadRegistry,
        message: format!("Unable to perform exact registry lookup: {error}"),
        detail: json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
        next_command: Some(
            "Repair the registry index, then rerun canon entity session solve".to_string(),
        ),
    })?;
    let mappings = resolved
        .mappings
        .into_iter()
        .map(|mapping| (mapping.input.clone(), mapping))
        .collect::<BTreeMap<_, _>>();
    let incumbents = observations
        .iter()
        .filter_map(|observation| {
            mappings
                .get(&observation.surface)
                .map(|mapping| (observation.node_key.clone(), mapping.canonical_id.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut canonical_ids = incumbents.values().cloned().collect::<BTreeSet<_>>();
    canonical_ids.extend(
        mappings
            .values()
            .map(|mapping| mapping.canonical_id.clone()),
    );
    canonical_ids.extend(endpoint_targets.values().filter_map(|spec| match spec {
        RegistryEndpointSpec::CanonicalId(canonical_id) => Some(canonical_id.clone()),
        RegistryEndpointSpec::Alias(_) | RegistryEndpointSpec::Key { .. } => None,
    }));
    let connection = Connection::open(&loaded.db_path).map_err(|error| {
        solve_refusal(
            format!("Unable to open session registry index: {error}"),
            json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
            "Repair the registry index, then rerun canon entity session solve",
        )
    })?;
    let mut rows_by_id = BTreeMap::new();
    for canonical_id in canonical_ids {
        rows_by_id.insert(
            canonical_id.clone(),
            registry_rows_for_id(&connection, registry_path, &canonical_id)?,
        );
    }
    let incumbent_keys = rows_by_id
        .iter()
        .map(|(canonical_id, rows)| {
            (
                format!("{}\u{0}{canonical_id}", entity_type),
                key_values(rows),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut endpoints = BTreeMap::new();
    for (target, spec) in endpoint_targets {
        let endpoint = match spec {
            RegistryEndpointSpec::Alias(surface) => {
                mappings.get(&surface).map(|mapping| RegistryEndpoint {
                    entity_type: entity_type.to_string(),
                    node_key: format!("{entity_type}\u{0}{surface}"),
                    surface,
                    canonical_id: mapping.canonical_id.clone(),
                    keys: incumbent_keys
                        .get(&format!("{}\u{0}{}", entity_type, mapping.canonical_id))
                        .cloned()
                        .unwrap_or_default(),
                    alias_endpoint: true,
                })
            }
            RegistryEndpointSpec::Key { namespace, value } => {
                let namespace = canonical_key_namespace(&namespace);
                let Some(value) = canonical_key_value(&namespace, &value) else {
                    continue;
                };
                let expected_rule = format!("SESSION_KEY_{}", namespace.to_ascii_uppercase());
                mappings
                    .values()
                    .find(|mapping| {
                        mapping.rule_id == expected_rule
                            && canonical_key_value(&namespace, &mapping.input).as_deref()
                                == Some(value.as_str())
                    })
                    .map(|mapping| {
                        let mut keys = incumbent_keys
                            .get(&format!("{}\u{0}{}", entity_type, mapping.canonical_id))
                            .cloned()
                            .unwrap_or_default();
                        keys.entry(namespace.clone())
                            .or_default()
                            .insert(value.clone());
                        RegistryEndpoint {
                            entity_type: entity_type.to_string(),
                            node_key: format!("{entity_type}\u{0}{value}"),
                            surface: value,
                            canonical_id: mapping.canonical_id.clone(),
                            keys,
                            alias_endpoint: false,
                        }
                    })
            }
            RegistryEndpointSpec::CanonicalId(canonical_id) => {
                rows_by_id.get(&canonical_id).and_then(|rows| {
                    (!rows.is_empty()).then(|| RegistryEndpoint {
                        entity_type: entity_type.to_string(),
                        node_key: format!("registry\u{0}{canonical_id}"),
                        surface: target.clone(),
                        canonical_id,
                        keys: key_values(rows),
                        alias_endpoint: false,
                    })
                })
            }
        };
        if let Some(endpoint) = endpoint {
            endpoints.insert(target, endpoint);
        }
    }
    let snapshot_hash =
        registry_snapshot_hash(registry_path, &loaded.meta.id, &loaded.meta.version)?;
    Ok(RegistryInfo {
        metadata: SessionSolveRegistry {
            id: loaded.meta.id,
            version: loaded.meta.version,
            snapshot_hash,
            escrow_snapshot_hash: memory.registry.escrow_snapshot_hash,
        },
        incumbents,
        incumbent_keys,
        endpoints,
        cannot_link_facts: memory.cannot_link_facts,
        pending_clusters: memory.pending_clusters,
    })
}

fn memory_key_node_keys(
    key: &str,
    nodes: &BTreeMap<String, Node>,
    endpoints: &BTreeMap<String, RegistryEndpoint>,
) -> BTreeSet<String> {
    let mut matches = BTreeSet::new();
    if let Some(endpoint) = endpoints.get(key) {
        matches.insert(endpoint.node_key.clone());
    }
    if let Some(surface) = key.strip_prefix("surface:") {
        matches.extend(
            nodes
                .iter()
                .filter(|(_, node)| node.surface == surface)
                .map(|(node_key, _)| node_key.clone()),
        );
    }
    matches.extend(
        nodes
            .iter()
            .filter(|(_, node)| node.surface == key)
            .map(|(node_key, _)| node_key.clone()),
    );
    matches.extend(
        nodes
            .iter()
            .filter(|(_, node)| node.incumbent_canonical_id.as_deref() == Some(key))
            .map(|(node_key, _)| node_key.clone()),
    );
    let key_spec = key
        .strip_prefix("registry:key:")
        .unwrap_or(key)
        .split_once(':');
    if let Some((namespace, value)) = key_spec {
        let namespace = canonical_key_namespace(namespace);
        let value = canonical_key_value(&namespace, value).unwrap_or_else(|| value.to_string());
        matches.extend(
            nodes
                .iter()
                .filter(|(_, node)| {
                    node.keys
                        .get(&namespace)
                        .is_some_and(|values| values.contains(&value))
                        || node
                            .observed_keys
                            .get(&namespace)
                            .is_some_and(|values| values.contains(&value))
                })
                .map(|(node_key, _)| node_key.clone()),
        );
        matches.extend(
            endpoints
                .values()
                .filter(|endpoint| {
                    endpoint
                        .keys
                        .get(&namespace)
                        .is_some_and(|values| values.contains(&value))
                })
                .map(|endpoint| endpoint.node_key.clone()),
        );
    }
    matches
}

fn pending_for_pair(
    left_key: &str,
    right_key: &str,
    left: Option<&Node>,
    right: Option<&Node>,
    pending: &[PendingClusterRecord],
) -> Option<String> {
    let surfaces = [
        left.map(|node| node.surface.as_str()),
        right.map(|node| node.surface.as_str()),
    ];
    pending
        .iter()
        .filter(|record| {
            surfaces.iter().all(|surface| {
                surface
                    .is_some_and(|value| record.surfaces.iter().any(|candidate| candidate == value))
            }) || record.surfaces.iter().any(|surface| surface == left_key)
                && record.surfaces.iter().any(|surface| surface == right_key)
        })
        .map(|record| record.escrow_id.clone())
        .min()
}

fn generated_pending_id(left_surface: &str, right_surface: &str) -> String {
    let (left, right) = ordered_pair(left_surface, right_surface);
    let digest = blake3::hash(format!("{left}\n{right}").as_bytes())
        .to_hex()
        .to_string();
    format!("OE-{}", &digest[..12])
}

fn registry_escrow_snapshot_hash(path: &Path) -> Result<String, Refusal> {
    crate::entity::runtime::incumbent::escrow_snapshot_hash(path).map_err(|error| {
        solve_refusal(
            format!(
                "Unable to hash registry memory '{}': {error}",
                path.display()
            ),
            json!({"registry": path.display().to_string(), "writes_performed": false}),
            "Repair registry memory sidecars, then rerun canon entity session solve",
        )
    })
}

#[derive(Debug, Clone)]
enum RegistryEndpointSpec {
    CanonicalId(String),
    Alias(String),
    Key { namespace: String, value: String },
}

fn registry_endpoint_spec(target: &str) -> Option<(String, RegistryEndpointSpec)> {
    if let Some(alias) = target.strip_prefix("registry:alias:") {
        return (!alias.is_empty()).then(|| {
            (
                target.to_string(),
                RegistryEndpointSpec::Alias(alias.to_string()),
            )
        });
    }
    if let Some(key) = target.strip_prefix("registry:key:") {
        let (namespace, value) = key.split_once(':')?;
        if namespace.is_empty() || value.is_empty() {
            return None;
        }
        return Some((
            target.to_string(),
            RegistryEndpointSpec::Key {
                namespace: namespace.to_string(),
                value: value.to_string(),
            },
        ));
    }
    target
        .strip_prefix("registry:")
        .filter(|canonical_id| !canonical_id.is_empty())
        .map(|canonical_id| {
            (
                target.to_string(),
                RegistryEndpointSpec::CanonicalId(canonical_id.to_string()),
            )
        })
}

#[derive(Debug, Clone)]
struct RegistryRow {
    rule_id: String,
    input: String,
}

fn registry_rows_for_id(
    connection: &Connection,
    registry_path: &Path,
    canonical_id: &str,
) -> Result<Vec<RegistryRow>, Refusal> {
    let mut statement = connection
        .prepare(
            "SELECT input, rule_id FROM entries WHERE canonical_id = ?1 ORDER BY source_file, entry_order",
        )
        .map_err(|error| {
            solve_refusal(
                format!("Unable to query registry entries for {canonical_id}: {error}"),
                json!({"registry": registry_path.display().to_string(), "canonical_id": canonical_id, "writes_performed": false}),
                "Repair the registry index, then rerun canon entity session solve",
            )
        })?;
    statement
        .query_map(params![canonical_id], |row| {
            Ok(RegistryRow {
                input: row.get(0)?,
                rule_id: row.get(1)?,
            })
        })
        .map_err(|error| {
            solve_refusal(
                format!("Unable to read registry entries for {canonical_id}: {error}"),
                json!({"registry": registry_path.display().to_string(), "canonical_id": canonical_id, "writes_performed": false}),
                "Repair the registry index, then rerun canon entity session solve",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            solve_refusal(
                format!("Unable to decode registry entries for {canonical_id}: {error}"),
                json!({"registry": registry_path.display().to_string(), "canonical_id": canonical_id, "writes_performed": false}),
                "Repair the registry index, then rerun canon entity session solve",
            )
        })
}

fn key_values(rows: &[RegistryRow]) -> BTreeMap<String, BTreeSet<String>> {
    let mut keys = BTreeMap::new();
    for row in rows {
        let Some(namespace) = row.rule_id.strip_prefix("SESSION_KEY_") else {
            continue;
        };
        let namespace = canonical_key_namespace(namespace);
        let Some(value) = canonical_key_value(&namespace, &row.input) else {
            continue;
        };
        keys.entry(namespace)
            .or_insert_with(BTreeSet::new)
            .insert(value);
    }
    keys
}

fn canonical_key_namespace(namespace: &str) -> String {
    match KeyNamespace::from_name(namespace) {
        KeyNamespace::Opaque => namespace.trim().to_ascii_lowercase(),
        known => known.name().to_string(),
    }
}

fn canonical_key_value(namespace: &str, value: &str) -> Option<String> {
    KeyNamespace::from_name(namespace).canonicalise(value).ok()
}

fn key_lookup_values(namespace: &str, value: &str) -> Vec<String> {
    let Some(canonical) = canonical_key_value(namespace, value) else {
        return Vec::new();
    };
    if KeyNamespace::from_name(namespace) != KeyNamespace::Cik {
        return vec![canonical];
    }
    let mut values = vec![canonical.clone()];
    for width in (canonical.len() + 1)..=10 {
        values.push(format!("{:0>width$}", canonical, width = width));
    }
    values
}

fn registry_snapshot_hash(path: &Path, id: &str, version: &str) -> Result<String, Refusal> {
    let mut files = Vec::new();
    collect_registry_files(path, path, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut bytes = Vec::new();
    bytes.extend_from_slice(id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(version.as_bytes());
    bytes.push(0xff);
    for (relative, digest) in files {
        bytes.extend_from_slice(relative.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(digest.as_bytes());
        bytes.push(0xff);
    }
    Ok(hash_bytes(&bytes))
}

fn collect_registry_files(
    root: &Path,
    path: &Path,
    output: &mut Vec<(String, String)>,
) -> Result<(), Refusal> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| {
            solve_refusal(
                format!(
                    "Unable to read registry directory '{}': {error}",
                    path.display()
                ),
                json!({"registry": root.display().to_string(), "writes_performed": false}),
                "Repair the registry directory, then rerun canon entity session solve",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            solve_refusal(
                format!(
                    "Unable to enumerate registry directory '{}': {error}",
                    path.display()
                ),
                json!({"registry": root.display().to_string(), "writes_performed": false}),
                "Repair the registry directory, then rerun canon entity session solve",
            )
        })?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let child = entry.path();
        if child.is_dir() {
            collect_registry_files(root, &child, output)?;
        } else if child.is_file()
            && child.file_name().and_then(|name| name.to_str()) != Some("registry.json")
        {
            let relative = child
                .strip_prefix(root)
                .map_err(|error| {
                    solve_refusal(
                        format!(
                            "Unable to relativize registry path '{}': {error}",
                            child.display()
                        ),
                        json!({"registry": root.display().to_string(), "writes_performed": false}),
                        "Repair the registry directory, then rerun canon entity session solve",
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read(&child).map_err(|error| {
                solve_refusal(
                    format!("Unable to read registry sidecar '{}': {error}", child.display()),
                    json!({"registry": root.display().to_string(), "path": relative, "writes_performed": false}),
                    "Repair the registry directory, then rerun canon entity session solve",
                )
            })?;
            output.push((relative, hash_bytes(&content)));
        }
    }
    Ok(())
}

fn build_nodes(
    observations: Vec<Observation>,
    incumbents: BTreeMap<String, String>,
    incumbent_keys: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    endpoints: BTreeMap<String, RegistryEndpoint>,
) -> (BTreeMap<String, Node>, BTreeMap<String, Observation>) {
    let mut nodes = BTreeMap::new();
    let mut by_id = BTreeMap::new();
    for observation in observations {
        let incumbent_canonical_id = incumbents.get(&observation.node_key).cloned();
        let node = nodes
            .entry(observation.node_key.clone())
            .or_insert_with(|| Node {
                surface: observation.surface.clone(),
                observation_ids: BTreeSet::new(),
                keys: BTreeMap::new(),
                observed_keys: BTreeMap::new(),
                incumbent_key_conflicts: BTreeSet::new(),
                incumbent_canonical_id: incumbent_canonical_id.clone(),
            });
        if node.incumbent_canonical_id.is_none() {
            node.incumbent_canonical_id = incumbent_canonical_id.clone();
        }
        node.observation_ids.insert(observation.obs_id.clone());
        for (key, value) in &observation.keys {
            node.keys
                .entry(key.clone())
                .or_default()
                .insert(value.clone());
            node.observed_keys
                .entry(key.clone())
                .or_default()
                .insert(value.clone());
        }
        if let Some(canonical_id) = incumbent_canonical_id
            && let Some(existing_keys) =
                incumbent_keys.get(&format!("{}\u{0}{canonical_id}", observation.entity_type))
        {
            for (namespace, value) in &observation.keys {
                if existing_keys
                    .get(namespace)
                    .is_some_and(|values| !values.is_empty() && !values.contains(value))
                {
                    node.incumbent_key_conflicts.insert(namespace.clone());
                }
            }
            merge_node_keys(&mut node.keys, existing_keys);
        }
        by_id.insert(observation.obs_id.clone(), observation);
    }
    for endpoint in endpoints.values() {
        let node = nodes
            .entry(endpoint.node_key.clone())
            .or_insert_with(|| Node {
                surface: endpoint.surface.clone(),
                observation_ids: BTreeSet::new(),
                keys: BTreeMap::new(),
                observed_keys: BTreeMap::new(),
                incumbent_key_conflicts: BTreeSet::new(),
                incumbent_canonical_id: Some(endpoint.canonical_id.clone()),
            });
        if node.incumbent_canonical_id.is_none() {
            node.incumbent_canonical_id = Some(endpoint.canonical_id.clone());
        }
        merge_node_keys(&mut node.keys, &endpoint.keys);
    }
    (nodes, by_id)
}

fn merge_node_keys(
    destination: &mut BTreeMap<String, BTreeSet<String>>,
    source: &BTreeMap<String, BTreeSet<String>>,
) {
    for (namespace, values) in source {
        destination
            .entry(namespace.clone())
            .or_default()
            .extend(values.iter().cloned());
    }
}

fn derive_same_key_edges(
    observations: &BTreeMap<String, Observation>,
    rules: &SessionRules,
    edge_facts: &mut BTreeMap<(String, String), EdgeFacts>,
    derived_evidence: &mut Vec<crate::entity::evidence_ir::EvidenceRecord>,
) -> Result<(), Refusal> {
    for (namespace, key_rule) in &rules.key_rules {
        let mut values = BTreeMap::<(String, String), Vec<&Observation>>::new();
        for observation in observations.values() {
            if let Some(value) = observation.keys.get(namespace) {
                values
                    .entry((observation.entity_type.clone(), value.clone()))
                    .or_default()
                    .push(observation);
            }
        }
        for ((_, value), mut members) in values {
            members.sort_by(|left, right| left.obs_id.cmp(&right.obs_id));
            let Some(anchor) = members.first() else {
                continue;
            };
            for member in members.iter().skip(1) {
                if anchor.node_key == member.node_key {
                    continue;
                }
                let (left, right) = ordered_pair(&anchor.node_key, &member.node_key);
                let record =
                    derived_same_key_record(&anchor.obs_id, &member.obs_id, namespace, &value)?;
                let evidence_id = record.evidence_id.clone();
                let facts = edge_facts
                    .entry((left.clone(), right.clone()))
                    .or_insert_with(|| EdgeFacts {
                        left,
                        right,
                        ..EdgeFacts::default()
                    });
                facts.evidence_types.insert("same_key".to_string());
                facts.lineages.extend(anchor.lineages.iter().cloned());
                facts.lineages.extend(member.lineages.iter().cloned());
                merge_origin_components(
                    &mut facts.origin_components,
                    anchor
                        .origin_components
                        .iter()
                        .chain(member.origin_components.iter())
                        .cloned()
                        .collect(),
                );
                facts.authoritative_same_key |= key_rule.authoritative;
                facts.evidence_ids.insert(evidence_id);
                facts.obs_ids.insert(anchor.obs_id.clone());
                facts.obs_ids.insert(member.obs_id.clone());
                facts.positive = true;
                facts.hits.push(hit_for_type("same_key", &record));
                derived_evidence.push(record);
            }
        }
    }
    Ok(())
}

fn derive_registry_key_edges(
    observations: &BTreeMap<String, Observation>,
    endpoints: &BTreeMap<String, RegistryEndpoint>,
    rules: &SessionRules,
    edge_facts: &mut BTreeMap<(String, String), EdgeFacts>,
    derived_evidence: &mut Vec<crate::entity::evidence_ir::EvidenceRecord>,
) -> Result<(), Refusal> {
    for (target, endpoint) in endpoints {
        let Some(key_spec) = target.strip_prefix("registry:key:") else {
            continue;
        };
        let Some((namespace, value)) = key_spec.split_once(':') else {
            continue;
        };
        let namespace = canonical_key_namespace(namespace);
        let Some(value) = canonical_key_value(&namespace, value) else {
            continue;
        };
        if !rules.is_authoritative_key(&namespace) {
            continue;
        }
        for observation in observations.values().filter(|observation| {
            observation.entity_type.as_str() == endpoint.entity_type.as_str()
                && observation.keys.get(&namespace).is_some_and(|key| {
                    canonical_key_value(&namespace, key).as_deref() == Some(value.as_str())
                })
        }) {
            if observation.node_key == endpoint.node_key {
                continue;
            }
            let (left, right) = ordered_pair(&observation.node_key, &endpoint.node_key);
            let record = derived_same_key_record(&observation.obs_id, target, &namespace, &value)?;
            let evidence_id = record.evidence_id.clone();
            let facts = edge_facts
                .entry((left.clone(), right.clone()))
                .or_insert_with(|| EdgeFacts {
                    left,
                    right,
                    ..EdgeFacts::default()
                });
            facts.evidence_types.insert("same_key".to_string());
            facts.lineages.extend(observation.lineages.iter().cloned());
            merge_origin_components(
                &mut facts.origin_components,
                observation.origin_components.clone(),
            );
            facts.authoritative_same_key = true;
            facts.evidence_ids.insert(evidence_id);
            facts.obs_ids.insert(observation.obs_id.clone());
            facts.obs_ids.insert(target.clone());
            facts.positive = true;
            facts.hits.push(hit_for_type("same_key", &record));
            derived_evidence.push(record);
        }
    }
    Ok(())
}

fn derived_same_key_record(
    left_id: &str,
    right_id: &str,
    namespace: &str,
    value: &str,
) -> Result<crate::entity::evidence_ir::EvidenceRecord, Refusal> {
    use crate::entity::evidence_ir::{
        CANON_EVIDENCE_VERSION, EvidenceKind, EvidenceOperatorRef, EvidencePolicyRef,
        EvidenceProvenanceRef, EvidenceRecord, EvidenceTarget, canonicalize_record,
    };
    let source_id = format!("derived:same_key:{namespace}:{value}");
    let record = EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind: EvidenceKind::PairSupport,
        target: EvidenceTarget::Pair {
            left_id: left_id.to_string(),
            right_id: right_id.to_string(),
        },
        operator: EvidenceOperatorRef {
            namespace: "session".to_string(),
            operator_id: "same_key".to_string(),
            operator_version: "v1".to_string(),
            adapter_id: None,
        },
        reason_code: "same_key".to_string(),
        policy: EvidencePolicyRef {
            policy_id: "canon.entity.session.solve".to_string(),
            policy_version: "v1".to_string(),
            content_hash: hash_bytes(namespace.as_bytes()),
        },
        authority_basis: None,
        scope: None,
        temporal_scope: None,
        provenance: vec![EvidenceProvenanceRef {
            source_type: "session".to_string(),
            source_id,
            locator: "derived".to_string(),
            content_hash: hash_bytes(value.as_bytes()),
            observed_at: None,
        }],
        measurements: Vec::new(),
        extensions: Vec::new(),
    };
    canonicalize_record(record).map_err(|error| {
        solve_refusal(
            format!("Derived same_key evidence is invalid: {error}"),
            json!({"namespace": namespace, "writes_performed": false}),
            "Repair the session observation keys, then rerun solve",
        )
    })
}

fn pair_target(record: &crate::entity::evidence_ir::EvidenceRecord) -> Option<(&str, &str)> {
    match &record.target {
        crate::entity::evidence_ir::EvidenceTarget::Pair { left_id, right_id } => {
            Some((left_id, right_id))
        }
        _ => None,
    }
}

fn resolve_target(
    target: &str,
    observations: &BTreeMap<String, Observation>,
    endpoints: &BTreeMap<String, RegistryEndpoint>,
) -> Option<TargetInfo> {
    if let Some(observation) = observations.get(target) {
        return Some(TargetInfo {
            node_key: observation.node_key.clone(),
            entity_type: observation.entity_type.clone(),
            surface: observation.surface.clone(),
            lineages: observation.lineages.clone(),
            origin_components: observation.origin_components.clone(),
            alias_endpoint: false,
        });
    }
    endpoints.get(target).map(|endpoint| TargetInfo {
        node_key: endpoint.node_key.clone(),
        entity_type: endpoint.entity_type.clone(),
        surface: endpoint.surface.clone(),
        lineages: BTreeSet::new(),
        origin_components: BTreeSet::new(),
        alias_endpoint: endpoint.alias_endpoint,
    })
}

fn is_incumbent_variant(
    record: &crate::entity::evidence_ir::EvidenceRecord,
    left: &TargetInfo,
    right: &TargetInfo,
    steps: &[String],
) -> bool {
    if record.operator.operator_id != "name_equivalent"
        || steps.is_empty()
        || left.alias_endpoint == right.alias_endpoint
    {
        return false;
    }
    let Some(left_value) = basis_string(record, "left_value") else {
        return false;
    };
    let Some(right_value) = basis_string(record, "right_value") else {
        return false;
    };
    normalize_name(left_value, steps) == normalize_name(right_value, steps)
        && normalize_name(&left.surface, steps) == normalize_name(&right.surface, steps)
}

fn basis_string<'a>(
    record: &'a crate::entity::evidence_ir::EvidenceRecord,
    key: &str,
) -> Option<&'a str> {
    record.extensions.iter().find_map(|extension| {
        let crate::entity::evidence_ir::EvidenceExtensionValue::Object(basis) =
            extension.payload.get("basis")?
        else {
            return None;
        };
        match basis.get(key)? {
            crate::entity::evidence_ir::EvidenceExtensionValue::String(value) => {
                Some(value.as_str())
            }
            _ => None,
        }
    })
}

fn normalize_name(value: &str, steps: &[String]) -> String {
    steps
        .iter()
        .fold(value.to_string(), |value, step| match step.as_str() {
            "casefold" => value.to_lowercase(),
            "strip_punctuation" => value
                .chars()
                .filter(|character| character.is_alphanumeric() || character.is_whitespace())
                .collect(),
            "collapse_whitespace" => value.split_whitespace().collect::<Vec<_>>().join(" "),
            _ => value,
        })
}

fn evidence_type(record: &crate::entity::evidence_ir::EvidenceRecord) -> String {
    let operator_id = record.operator.operator_id.trim();
    if TIER_A_TYPES.contains(&operator_id)
        || TIER_B_TYPES.contains(&operator_id)
        || RELATION_TYPES.contains(&operator_id)
        || DISTINCT_TYPES.contains(&operator_id)
    {
        operator_id.to_string()
    } else {
        record.reason_code.trim().to_string()
    }
}

fn relation_value(record: &crate::entity::evidence_ir::EvidenceRecord) -> String {
    record
        .extensions
        .iter()
        .flat_map(|extension| extension.payload.iter())
        .find_map(|(key, value)| {
            (key == "relation").then(|| match value {
                crate::entity::evidence_ir::EvidenceExtensionValue::String(value) => value.clone(),
                _ => record.reason_code.clone(),
            })
        })
        .unwrap_or_else(|| record.reason_code.clone())
}

fn relation_subject_node(
    record: &crate::entity::evidence_ir::EvidenceRecord,
    left_target_id: &str,
    right_target_id: &str,
    left_node: &str,
    right_node: &str,
) -> String {
    let subject = record.extensions.iter().find_map(|extension| {
        extension
            .payload
            .get("subject")
            .and_then(|value| match value {
                crate::entity::evidence_ir::EvidenceExtensionValue::String(value) => {
                    Some(value.as_str())
                }
                _ => None,
            })
    });
    match subject {
        Some(subject) if subject == right_target_id => right_node.to_string(),
        Some(subject) if subject == left_target_id => left_node.to_string(),
        _ => left_node.to_string(),
    }
}

fn read_related_registry_pairs(
    registry_path: &Path,
) -> Result<BTreeSet<(String, String)>, Refusal> {
    let path = registry_path.join("relations.jsonl");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => {
            return Err(solve_refusal(
                format!(
                    "Unable to read registry relations '{}': {error}",
                    path.display()
                ),
                json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
                "Repair relations.jsonl, then rerun canon entity session solve",
            ));
        }
    };
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        solve_refusal(
            format!(
                "Registry relations '{}' are not UTF-8: {error}",
                path.display()
            ),
            json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
            "Repair relations.jsonl, then rerun canon entity session solve",
        )
    })?;
    let mut pairs = BTreeSet::new();
    for (line, value) in text.lines().enumerate() {
        if value.trim().is_empty() {
            continue;
        }
        let fact = serde_json::from_str::<DirectedRelationFact>(value).map_err(|error| {
            solve_refusal(
                format!("Registry relation line {} is invalid: {error}", line + 1),
                json!({"registry": registry_path.display().to_string(), "line": line + 1, "writes_performed": false}),
                "Repair relations.jsonl, then rerun canon entity session solve",
            )
        })?;
        let fact = finalize_relation(fact).map_err(|error| {
            solve_refusal(
                format!("Registry relation line {} is not canonical: {error}", line + 1),
                json!({"registry": registry_path.display().to_string(), "line": line + 1, "writes_performed": false}),
                "Repair relations.jsonl, then rerun canon entity session solve",
            )
        })?;
        let relation_name = match &fact.relation {
            RelationKindRef::Extension { value, .. } => value.as_str(),
            RelationKindRef::Core { .. } => continue,
        };
        if ROLE_RELATIONS.contains(&relation_name) {
            continue;
        }
        pairs.insert(ordered_pair(
            &fact.subject.identity_id,
            &fact.object.identity_id,
        ));
    }
    Ok(pairs)
}

fn nodes_are_related(
    left: Option<&Node>,
    right: Option<&Node>,
    related_pairs: &BTreeSet<(String, String)>,
) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    let (Some(left_id), Some(right_id)) = (
        left.incumbent_canonical_id.as_deref(),
        right.incumbent_canonical_id.as_deref(),
    ) else {
        return false;
    };
    related_pairs.contains(&ordered_pair(left_id, right_id))
}

fn node_entity_type(node_key: &str) -> String {
    node_key
        .split_once('\0')
        .map(|(entity_type, _)| entity_type.to_string())
        .unwrap_or_else(|| "organisation".to_string())
}

fn read_role_relations(
    snapshot: &EntityPublicationSnapshot,
) -> Result<Vec<SessionSolveRelation>, Refusal> {
    let mut relations = BTreeMap::<(String, String, String), SessionSolveRelation>::new();
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(logical_path).ok_or_else(|| {
            solve_refusal(
                format!("Observation shard {logical_path} is missing"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation stream, then rerun canon entity session solve",
            )
        })?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            solve_refusal(
                format!("Observation shard {logical_path} is not UTF-8: {error}"),
                json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
                "Repair the observation stream, then rerun canon entity session solve",
            )
        })?;
        for (line, value) in text.lines().enumerate() {
            if value.trim().is_empty() {
                continue;
            }
            let raw = serde_json::from_str::<RawObservation>(value).map_err(|error| {
                solve_refusal(
                    format!(
                        "Observation shard {logical_path} line {} is invalid: {error}",
                        line + 1
                    ),
                    json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "line": line + 1, "writes_performed": false}),
                    "Repair the observation stream, then rerun canon entity session solve",
                )
            })?;
            let Some(role) = map_role(raw.context.get("role").or_else(|| raw.attrs.get("role")))
            else {
                continue;
            };
            let Some((object, object_type)) = role_object(&raw.context) else {
                continue;
            };
            let subject = ascii_trim(&raw.surface).to_string();
            let key = (subject.clone(), role.clone(), object.clone());
            let timestamp = role_timestamp(&raw.context, &raw.attrs);
            let relation = relations
                .entry(key)
                .or_insert_with(|| SessionSolveRelation {
                    left_surface: subject.clone(),
                    right_surface: object.clone(),
                    subject_surface: subject.clone(),
                    object_surface: object.clone(),
                    relation: role.clone(),
                    evidence_ids: Vec::new(),
                    subject_entity_type: ascii_trim(&raw.entity_type).to_string(),
                    object_entity_type: object_type.to_string(),
                    object_external_id: Some(object.clone()),
                    valid_time: TimeInterval::default(),
                });
            if let Some(timestamp) = timestamp {
                relation.valid_time.start_at = match relation.valid_time.start_at.take() {
                    Some(existing) => Some(existing.min(timestamp.clone())),
                    None => Some(timestamp.clone()),
                };
                relation.valid_time.end_at = match relation.valid_time.end_at.take() {
                    Some(existing) => Some(existing.max(timestamp)),
                    None => relation.valid_time.start_at.clone(),
                };
                relation.valid_time.start_bound = IntervalBoundary::Inclusive;
                relation.valid_time.end_bound = IntervalBoundary::Inclusive;
            }
        }
    }
    Ok(relations.into_values().collect())
}

fn map_role(value: Option<&Value>) -> Option<String> {
    let role = value?.as_str()?.trim().to_ascii_lowercase();
    let relation = match role.as_str() {
        "sponsor" => "sponsor_of",
        "guarantor" => "guarantor_of",
        "borrower" => "borrower_of",
        "servicer" => "servicer_of",
        "trustee" => "trustee_of",
        _ => return None,
    };
    Some(relation.to_string())
}

fn role_object(context: &BTreeMap<String, Value>) -> Option<(String, &'static str)> {
    [
        ("deal_id", "deal"),
        ("deal_cik", "deal"),
        ("deal", "deal"),
        ("loan_id", "loan"),
        ("loan", "loan"),
    ]
    .into_iter()
    .find_map(|(key, entity_type)| {
        context
            .get(key)
            .and_then(Value::as_str)
            .map(|value| (value.trim().to_string(), entity_type))
            .filter(|(value, _)| !value.is_empty())
    })
}

fn role_timestamp(
    context: &BTreeMap<String, Value>,
    attrs: &BTreeMap<String, Value>,
) -> Option<String> {
    context
        .get("as_of_date")
        .or_else(|| context.get("as_of"))
        .or_else(|| attrs.get("as_of_date"))
        .or_else(|| attrs.get("as_of"))
        .and_then(Value::as_str)
        .map(|value| {
            let value = value.trim();
            if value.len() == 10 && value.as_bytes().get(4) == Some(&b'-') {
                format!("{value}T00:00:00Z")
            } else {
                value.to_string()
            }
        })
        .filter(|value| !value.is_empty())
}

fn record_lineage(record: &crate::entity::evidence_ir::EvidenceRecord) -> Option<String> {
    record.extensions.iter().find_map(|extension| {
        extension
            .payload
            .get("lineage")
            .and_then(|value| match value {
                crate::entity::evidence_ir::EvidenceExtensionValue::String(value) => {
                    let value = ascii_trim(value);
                    (!value.is_empty()).then(|| value.to_string())
                }
                _ => None,
            })
    })
}

fn record_origin_components(
    record: &crate::entity::evidence_ir::EvidenceRecord,
) -> BTreeSet<BTreeSet<String>> {
    for extension in &record.extensions {
        let Some(crate::entity::evidence_ir::EvidenceExtensionValue::Object(ancestry)) =
            extension.payload.get("ancestry")
        else {
            continue;
        };
        if !matches!(
            ancestry.get("status"),
            Some(crate::entity::evidence_ir::EvidenceExtensionValue::String(status))
                if status == "known"
        ) {
            return BTreeSet::new();
        }
        let Some(crate::entity::evidence_ir::EvidenceExtensionValue::List(ids)) =
            ancestry.get("origin_source_ids")
        else {
            return BTreeSet::new();
        };
        let origins = ids
            .iter()
            .filter_map(|id| match id {
                crate::entity::evidence_ir::EvidenceExtensionValue::String(id) => Some(id.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        return if origins.is_empty() {
            BTreeSet::new()
        } else {
            BTreeSet::from([origins])
        };
    }
    record_lineage(record)
        .map(|lineage| BTreeSet::from([BTreeSet::from([lineage])]))
        .unwrap_or_default()
}

fn merge_origin_components(
    destination: &mut BTreeSet<BTreeSet<String>>,
    incoming: BTreeSet<BTreeSet<String>>,
) {
    for incoming_component in incoming {
        let mut merged = incoming_component;
        let overlapping = destination
            .iter()
            .filter(|existing| existing.iter().any(|origin| merged.contains(origin)))
            .cloned()
            .collect::<Vec<_>>();
        for existing in overlapping {
            destination.remove(&existing);
            merged.extend(existing);
        }
        destination.insert(merged);
    }
}

fn independent_origin_count(facts: &EdgeFacts) -> usize {
    if facts.origin_components.is_empty() {
        facts.lineages.len()
    } else {
        facts.origin_components.len()
    }
}

fn hit_for_type(
    evidence_type: &str,
    record: &crate::entity::evidence_ir::EvidenceRecord,
) -> EdgeEvidenceHit {
    let lane = if DISTINCT_TYPES.contains(&evidence_type) {
        ScoreLane::AntiMerge
    } else if RELATION_TYPES.contains(&evidence_type) {
        ScoreLane::RelationHint
    } else {
        ScoreLane::Support
    };
    EdgeEvidenceHit::new(
        lane,
        "session",
        evidence_type,
        evidence_type,
        ScoreUnits::ZERO,
        DISTINCT_TYPES.contains(&evidence_type),
        format!(
            "session evidence {} ({})",
            evidence_type, record.evidence_id
        ),
    )
}

fn is_positive_type(evidence_type: &str) -> bool {
    TIER_A_TYPES.contains(&evidence_type) || TIER_B_TYPES.contains(&evidence_type)
}

fn tier_for(facts: &EdgeFacts, rules: &SessionRules) -> Option<EdgeTier> {
    if !facts.positive
        || !facts
            .evidence_types
            .iter()
            .any(|kind| is_positive_type(kind))
    {
        return None;
    }
    if facts.variant_of_incumbent {
        return Some(EdgeTier::A);
    }
    if (facts.authoritative_same_key && facts.evidence_types.contains("same_key"))
        || facts
            .evidence_types
            .iter()
            .any(|kind| kind != "same_key" && rules.tier_a_types.contains(kind))
    {
        Some(EdgeTier::A)
    } else {
        let evidence_types = facts
            .evidence_types
            .iter()
            .filter(|kind| rules.tier_b_types.contains(kind))
            .collect::<BTreeSet<_>>();
        let independent_origins = independent_origin_count(facts);
        let tier_b = rules.tier_b_alternatives.iter().any(|alternative| {
            alternative
                .required_evidence_types
                .iter()
                .all(|kind| evidence_types.contains(kind))
                && evidence_types.len() >= alternative.min_evidence_types
                && independent_origins >= alternative.min_independent_origins
        });
        if tier_b {
            Some(EdgeTier::B)
        } else {
            Some(EdgeTier::C)
        }
    }
}

fn has_positive_distinct(facts: &EdgeFacts) -> bool {
    facts
        .evidence_types
        .iter()
        .any(|evidence_type| DISTINCT_TYPES.contains(&evidence_type.as_str()))
        && facts
            .evidence_types
            .iter()
            .any(|evidence_type| is_positive_type(evidence_type))
}

fn component_has_key_conflict(nodes: &[&Node], rules: &SessionRules) -> bool {
    rules.key_rules.iter().any(|(namespace, rule)| {
        rule.single_valued
            && (nodes
                .iter()
                .any(|node| node.incumbent_key_conflicts.contains(namespace))
                || nodes
                    .iter()
                    .filter_map(|node| node.observed_keys.get(namespace))
                    .flat_map(|values| values.iter())
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 1)
    })
}

fn exactly_one_authoritative_value(node: &Node, rules: &SessionRules) -> bool {
    rules
        .key_rules
        .iter()
        .filter(|(_, rule)| rule.authoritative && rule.single_valued)
        .filter_map(|(key, _)| node.keys.get(key))
        .filter(|values| values.len() == 1)
        .count()
        == 1
        && rules
            .key_rules
            .iter()
            .filter(|(_, rule)| rule.authoritative && rule.single_valued)
            .all(|(key, _)| node.keys.get(key).is_none_or(|values| values.len() <= 1))
}

fn collect_cluster_keys(nodes: &[&Node]) -> BTreeMap<String, Vec<String>> {
    let mut keys = BTreeMap::<String, BTreeSet<String>>::new();
    for node in nodes {
        for (namespace, values) in &node.keys {
            keys.entry(namespace.clone())
                .or_default()
                .extend(values.iter().cloned());
        }
    }
    keys.into_iter()
        .map(|(namespace, values)| (namespace, values.into_iter().collect()))
        .collect()
}

fn value_to_key_value(key: &str, value: Value) -> Option<(String, String)> {
    match value {
        Value::Null => None,
        Value::String(value) => {
            let value = ascii_trim(&value);
            (!value.is_empty()).then(|| (key.to_string(), value.to_string()))
        }
        value => serde_json::to_string(&value)
            .ok()
            .map(|serialized| (key.to_string(), serialized)),
    }
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn cluster_id(member_keys: &[String]) -> String {
    let mut bytes = Vec::new();
    for key in member_keys {
        bytes.extend_from_slice(key.as_bytes());
        bytes.push(0xff);
    }
    format!("cluster:{}", hash_bytes(&bytes))
}

fn absolute_session_path(path: &Path) -> Result<PathBuf, Refusal> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| {
                solve_refusal(
                    format!(
                        "Unable to resolve session path '{}': {error}",
                        path.display()
                    ),
                    json!({"path": path.display().to_string(), "writes_performed": false}),
                    "Use an accessible session work directory, then rerun solve",
                )
            })
    }
}

fn solve_refusal(
    message: impl Into<String>,
    detail: Value,
    next_command: impl Into<String>,
) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.into(),
        detail,
        next_command: Some(next_command.into()),
    }
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::{
        CommitPlanContext, EdgeFacts, EdgeTier, RegistryRow, SessionCommitPlan, SessionRules,
        SessionSolveArtifact, SessionSolveCluster, SessionSolveInputs, SessionSolveRegistry,
        SessionSolveSummary, TIER_A_TYPES, TIER_B_TYPES, build_commit_plan, canonical_key_value,
        commit_skip_reasons, component_has_key_conflict, exactly_one_authoritative_value,
        generated_pending_id_for_many, has_positive_distinct, key_values, memory_key_for_target,
        nodes_are_related, ordered_pair, tier_for,
    };
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};

    fn rules() -> SessionRules {
        SessionRules {
            version: "canon.entity.session.rules.v1".to_string(),
            key_rules: BTreeMap::from([
                (
                    "cik".to_string(),
                    super::rules::KeyRule {
                        authoritative: true,
                        single_valued: true,
                    },
                ),
                (
                    "lei".to_string(),
                    super::rules::KeyRule {
                        authoritative: true,
                        single_valued: true,
                    },
                ),
            ]),
            authoritative_keys: vec!["cik".to_string(), "lei".to_string()],
            commit_key_namespaces: Vec::new(),
            tier_a_types: TIER_A_TYPES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            tier_b_types: TIER_B_TYPES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            min_distinct_evidence_types: 2,
            min_distinct_lineages: 2,
            tier_b_alternatives: vec![super::rules::TierBAlternative {
                required_evidence_types: Vec::new(),
                min_evidence_types: 2,
                min_independent_origins: 2,
            }],
            required_evidence_types: Vec::new(),
            mint_when_authoritative_key: true,
            max_cluster_surfaces: 50,
            incumbent_attach_steps: Vec::new(),
        }
    }

    fn facts(types: &[&str], lineages: &[&str]) -> EdgeFacts {
        EdgeFacts {
            positive: true,
            evidence_types: types.iter().map(|value| (*value).to_string()).collect(),
            lineages: lineages.iter().map(|value| (*value).to_string()).collect(),
            authoritative_same_key: types.contains(&"same_key"),
            ..EdgeFacts::default()
        }
    }

    #[test]
    fn rule_tiering_requires_distinct_types_and_lineages() {
        let rules = rules();
        assert_eq!(
            tier_for(&facts(&["same_key"], &["a"]), &rules),
            Some(EdgeTier::A)
        );
        assert_eq!(
            tier_for(
                &facts(&["same_slot", "name_equivalent"], &["a", "b"]),
                &rules
            ),
            Some(EdgeTier::B)
        );
        assert_eq!(
            tier_for(&facts(&["same_slot", "name_equivalent"], &["a"]), &rules),
            Some(EdgeTier::C)
        );
        assert_eq!(
            tier_for(&facts(&["same_slot"], &["a", "b"]), &rules),
            Some(EdgeTier::C)
        );
        assert_eq!(tier_for(&facts(&[], &[]), &rules), None);
    }

    #[test]
    fn positive_distinct_edges_are_retained_for_contradiction_evaluation() {
        assert!(has_positive_distinct(&facts(
            &["same_slot", "distinct"],
            &["lineage-a"]
        )));
        assert!(!has_positive_distinct(&facts(
            &["distinct"],
            &["lineage-a"]
        )));
        assert!(!has_positive_distinct(&facts(
            &["same_slot"],
            &["lineage-a"]
        )));
    }

    #[test]
    fn registry_relation_pair_suppresses_only_matching_incumbents() {
        let left = super::Node {
            surface: "Alpha".to_string(),
            observation_ids: BTreeSet::new(),
            keys: BTreeMap::new(),
            observed_keys: BTreeMap::new(),
            incumbent_key_conflicts: BTreeSet::new(),
            incumbent_canonical_id: Some("ORG-001".to_string()),
        };
        let right = super::Node {
            surface: "Beta".to_string(),
            observation_ids: BTreeSet::new(),
            keys: BTreeMap::new(),
            observed_keys: BTreeMap::new(),
            incumbent_key_conflicts: BTreeSet::new(),
            incumbent_canonical_id: Some("ORG-002".to_string()),
        };
        let related = BTreeSet::from([(String::from("ORG-001"), String::from("ORG-002"))]);

        assert!(nodes_are_related(Some(&left), Some(&right), &related));
        assert!(!nodes_are_related(
            Some(&left),
            Some(&right),
            &BTreeSet::new()
        ));
        assert!(!nodes_are_related(None, Some(&right), &related));
    }

    #[test]
    fn exactly_one_authoritative_value_is_required_for_single_keyed_acceptance() {
        let mut node = super::Node {
            surface: "Solo".to_string(),
            observation_ids: BTreeSet::from(["obs".to_string()]),
            keys: std::collections::BTreeMap::from([(
                "cik".to_string(),
                BTreeSet::from(["555".to_string()]),
            )]),
            observed_keys: std::collections::BTreeMap::from([(
                "cik".to_string(),
                BTreeSet::from(["555".to_string()]),
            )]),
            incumbent_key_conflicts: BTreeSet::new(),
            incumbent_canonical_id: None,
        };
        assert!(exactly_one_authoritative_value(&node, &rules()));
        node.keys
            .insert("lei".to_string(), BTreeSet::from(["LEI".to_string()]));
        assert!(!exactly_one_authoritative_value(&node, &rules()));
    }

    #[test]
    fn incumbent_endpoint_keys_do_not_create_a_key_conflict() {
        let endpoint_a = super::Node {
            surface: "Medline Borrower, LP".to_string(),
            observation_ids: BTreeSet::new(),
            keys: BTreeMap::from([("cik".to_string(), BTreeSet::from(["111".to_string()]))]),
            observed_keys: BTreeMap::new(),
            incumbent_key_conflicts: BTreeSet::new(),
            incumbent_canonical_id: Some("ORG-001".to_string()),
        };
        let endpoint_b = super::Node {
            surface: "Other Holdings, Inc".to_string(),
            observation_ids: BTreeSet::new(),
            keys: BTreeMap::from([("cik".to_string(), BTreeSet::from(["222".to_string()]))]),
            observed_keys: BTreeMap::new(),
            incumbent_key_conflicts: BTreeSet::new(),
            incumbent_canonical_id: Some("ORG-002".to_string()),
        };

        assert!(!component_has_key_conflict(
            &[&endpoint_a, &endpoint_b],
            &rules()
        ));
    }

    #[test]
    fn registry_and_observation_cik_forms_share_canonical_key() {
        let registry = key_values(&[RegistryRow {
            rule_id: "SESSION_KEY_CIK".to_string(),
            input: "222".to_string(),
        }]);
        let observation = canonical_key_value("cik", "0000000222").expect("valid CIK");

        assert_eq!(observation, "222");
        assert!(
            registry
                .get("cik")
                .is_some_and(|values| values.contains(&observation))
        );
    }

    #[test]
    fn registry_memory_key_forms_are_canonical_and_distinct() {
        let observations = BTreeMap::from([
            (
                "Alpha Holdings".to_string(),
                vec![json!({
                    "obs_id": "alpha-1",
                    "surface": "Alpha Holdings",
                    "keys": {"cik": "0000000222"}
                })],
            ),
            (
                "Beta Holdings".to_string(),
                vec![json!({
                    "obs_id": "beta-1",
                    "surface": "Beta Holdings",
                    "keys": {}
                })],
            ),
        ]);

        assert_eq!(memory_key_for_target("alpha-1", &observations), "cik:222");
        assert_eq!(
            memory_key_for_target("beta-1", &observations),
            "surface:Beta Holdings"
        );
        assert_eq!(
            memory_key_for_target("registry:ORG-001", &observations),
            "registry:ORG-001"
        );
    }

    #[test]
    fn registry_memory_pair_and_pending_ids_are_order_independent() {
        assert_eq!(
            ordered_pair("surface:Beta", "surface:Alpha"),
            ("surface:Alpha".to_string(), "surface:Beta".to_string())
        );
        assert_eq!(
            generated_pending_id_for_many(&["Beta".to_string(), "Alpha".to_string()]),
            generated_pending_id_for_many(&["Alpha".to_string(), "Beta".to_string()])
        );
    }

    #[test]
    fn commit_plan_sorts_clusters_and_skips_non_accepted_outcomes() {
        let artifact = SessionSolveArtifact {
            version: "canon.entity.session.solve.v1".to_string(),
            inputs: SessionSolveInputs {
                observations_generation: "obs-generation".to_string(),
                evidence_generation: None,
                rules_hash: "rules-hash".to_string(),
                registry: SessionSolveRegistry {
                    id: "organisations".to_string(),
                    version: "0.0.0".to_string(),
                    snapshot_hash: "registry-hash".to_string(),
                    escrow_snapshot_hash: String::new(),
                },
            },
            authoritative_keys: vec!["cik".to_string()],
            commit_key_namespaces: Vec::new(),
            summary: SessionSolveSummary::default(),
            clusters: vec![
                SessionSolveCluster {
                    cluster_id: "cluster-z".to_string(),
                    outcome: "accepted".to_string(),
                    tier: Some("B".to_string()),
                    surfaces: vec!["Zeta Corporation".to_string(), "Zeta Corp".to_string()],
                    observation_count: 2,
                    keys: BTreeMap::from([(String::from("cik"), vec![String::from("222")])]),
                    incumbent_canonical_id: None,
                    accepting_evidence_ids: vec!["evidence-z".to_string()],
                    reason: "tier B".to_string(),
                    conflict_reasons: Vec::new(),
                },
                SessionSolveCluster {
                    cluster_id: "cluster-a".to_string(),
                    outcome: "accepted".to_string(),
                    tier: Some("A".to_string()),
                    surfaces: vec!["Acme Holdings".to_string()],
                    observation_count: 1,
                    keys: BTreeMap::from([(String::from("cik"), vec![String::from("111")])]),
                    incumbent_canonical_id: None,
                    accepting_evidence_ids: vec!["evidence-a".to_string()],
                    reason: "tier A".to_string(),
                    conflict_reasons: Vec::new(),
                },
                SessionSolveCluster {
                    cluster_id: "cluster-r".to_string(),
                    outcome: "review".to_string(),
                    tier: None,
                    surfaces: vec!["Review Only".to_string()],
                    observation_count: 1,
                    keys: BTreeMap::new(),
                    incumbent_canonical_id: None,
                    accepting_evidence_ids: Vec::new(),
                    reason: "insufficient evidence".to_string(),
                    conflict_reasons: Vec::new(),
                },
            ],
            review: Vec::new(),
            relations: Vec::new(),
            contradictions: Vec::new(),
            standalone_count: 0,
            dangling_evidence_count: 0,
            derived_evidence: Vec::new(),
        };

        let plan = build_commit_plan(
            &artifact,
            CommitPlanContext {
                entity_kind: "organisation",
                alias_file: "aliases.json",
                key_file: None,
                canonical_type: "organisation",
                session_id: "session-id",
                solve_hash: "solve-hash",
                registry_version: "0.0.0",
                observation_values: &BTreeMap::new(),
            },
        )
        .expect("commit plan");
        let SessionCommitPlan {
            entries,
            skipped,
            relations,
        } = plan;
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.cluster_id.as_str())
                .collect::<Vec<_>>(),
            vec!["cluster-a", "cluster-z"]
        );
        assert_eq!(entries[0].tier, "A");
        assert_eq!(entries[0].aliases[0].input, "Acme Holdings");
        assert_eq!(entries[0].aliases[0].rule_id, "SESSION_TIER_A");
        assert_eq!(entries[0].aliases[1].input, "111");
        assert_eq!(entries[0].aliases[1].rule_id, "SESSION_KEY_CIK");
        assert_eq!(entries[0].aliases[1].alias_file, "keys.cik.json");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].cluster_id, "cluster-r");
        assert_eq!(skipped[0].reason, "review");
        assert!(relations.is_empty());
        assert!(entries[0].session_authority.is_some());
        assert!(entries[0].closure.is_some());
    }

    #[test]
    fn commit_plan_reports_both_incumbent_and_key_conflicts() {
        let cluster = SessionSolveCluster {
            cluster_id: "cluster-conflict".to_string(),
            outcome: "contradiction".to_string(),
            tier: None,
            surfaces: vec![
                "Medline Borrower, LP".to_string(),
                "Medline Wrong".to_string(),
            ],
            observation_count: 2,
            keys: BTreeMap::from([(
                String::from("cik"),
                vec![String::from("111"), String::from("222")],
            )]),
            incumbent_canonical_id: None,
            accepting_evidence_ids: Vec::new(),
            reason: "incumbent_conflict".to_string(),
            conflict_reasons: vec!["incumbent_conflict".to_string(), "key_conflict".to_string()],
        };

        assert_eq!(
            commit_skip_reasons(&cluster),
            vec!["incumbent_conflict", "key_conflict"]
        );
    }

    #[test]
    fn incumbent_bridge_remains_a_generic_contradiction() {
        let cluster = SessionSolveCluster {
            cluster_id: "cluster-bridge".to_string(),
            outcome: "contradiction".to_string(),
            tier: None,
            surfaces: vec!["Shared Alias".to_string()],
            observation_count: 1,
            keys: BTreeMap::from([(
                String::from("cik"),
                vec![String::from("111"), String::from("222")],
            )]),
            incumbent_canonical_id: None,
            accepting_evidence_ids: Vec::new(),
            reason: "incumbent_conflict".to_string(),
            conflict_reasons: vec!["incumbent_conflict".to_string()],
        };

        assert_eq!(
            commit_skip_reasons(&cluster),
            vec!["contradiction"],
            "stored keys from two incumbent endpoints must not become a key conflict"
        );
    }
}
