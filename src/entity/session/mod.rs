#![forbid(unsafe_code)]

//! The evidence-session rule table and deterministic solve.
//!
//! Session solve deliberately owns the session-specific tier policy while it
//! reuses the entity signed graph and component evaluator. It does not
//! introduce a second union-find or a second graph implementation.

use crate::{
    InputFormat, InputValues, Refusal, RefusalCode, SpecialReason,
    entity::{
        edge::{EdgeEvidenceHit, build_edge_evidence_record},
        graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
        publication::{
            EntityPublicationFileInput, EntityPublicationRequest, EntityPublicationSnapshot,
            EntityPublicationUpstreamRef, open_current_stream_generation, publish_stream_patch,
        },
        score::{ScoreLane, ScoreUnits},
        solve::evaluate_signed_graph_components,
    },
    lookup, registry,
    session::{
        self, SESSION_EVIDENCE_STREAM_ID, SESSION_MANIFEST_STREAM_ID,
        SESSION_OBSERVATIONS_STREAM_ID, SessionCommitAlias, SessionCommitEntry,
        SessionCommitRegistryExpectation, SessionCommitSkipped, SessionManifest,
    },
    witness::hash_bytes,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

mod status;
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
const CROSS_ENTITY_TYPE_EVIDENCE_REASON: &str = "cross_entity_type_evidence";
const INCUMBENT_ATTACH_STEPS: &[&str] = &["casefold", "strip_punctuation", "collapse_whitespace"];

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
    pub dangling_evidence: usize,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveReview {
    pub left_surface: String,
    pub right_surface: String,
    pub evidence_types: Vec<String>,
    pub lineage_count: usize,
    pub missing: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSolveRelation {
    pub left_surface: String,
    pub right_surface: String,
    pub relation: String,
    pub evidence_ids: Vec<String>,
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
            "clusters={} tier_a={} tier_b={} review={} contradiction={} standalone={} resolved_existing={} relations={} dangling_evidence={} generation={}",
            summary.cluster_count,
            summary.accepted_tier_a,
            summary.accepted_tier_b,
            summary.review,
            summary.contradiction,
            summary.standalone,
            summary.resolved_existing,
            summary.relations,
            summary.dangling_evidence,
            self.generation_id,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionRules {
    version: String,
    authoritative_keys: Vec<String>,
    tier_a_types: Vec<String>,
    tier_b_types: Vec<String>,
    min_distinct_evidence_types: usize,
    min_distinct_lineages: usize,
    mint_when_authoritative_key: bool,
    incumbent_attach_steps: Vec<String>,
}

#[derive(Debug, Clone)]
struct Observation {
    obs_id: String,
    entity_type: String,
    surface: String,
    node_key: String,
    keys: BTreeMap<String, String>,
    lineages: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct Node {
    surface: String,
    observation_ids: BTreeSet<String>,
    keys: BTreeMap<String, BTreeSet<String>>,
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
    alias_endpoint: bool,
}

#[derive(Debug, Clone, Default)]
struct EdgeFacts {
    left: String,
    right: String,
    evidence_types: BTreeSet<String>,
    lineages: BTreeSet<String>,
    evidence_ids: BTreeSet<String>,
    obs_ids: BTreeSet<String>,
    hits: Vec<EdgeEvidenceHit>,
    relation: BTreeSet<String>,
    positive: bool,
    variant_of_incumbent: bool,
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
    source: RawObservationSource,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawObservationSource {
    #[serde(default)]
    lineage: Option<String>,
}

pub fn solve(request: SessionSolveRequest) -> Result<SessionSolveOutput, Refusal> {
    let session = absolute_session_path(&request.session)?;
    let manifest = read_manifest(&session)?;
    let (rules, rules_hash) = parse_rules(&request.rules)?;
    let observations_snapshot =
        open_required_stream(&session, SESSION_OBSERVATIONS_STREAM_ID, "observations")?;
    let evidence_snapshot = open_optional_stream(&session, SESSION_EVIDENCE_STREAM_ID)?;
    let observations = read_observations(&observations_snapshot)?;
    let evidence = match evidence_snapshot.as_ref() {
        Some(snapshot) => read_evidence(snapshot)?,
        None => Vec::new(),
    };
    let registry_binding = manifest.registries.first().ok_or_else(|| {
        solve_refusal(
            "The session manifest has no registry binding",
            json!({"field": "registries", "writes_performed": false}),
            "Reinitialize the session with --registry, then rerun canon entity session solve",
        )
    })?;
    let registry_info = load_registry_info(
        Path::new(&registry_binding.path),
        &observations,
        &evidence,
        &registry_binding.kind,
    )?;
    let RegistryInfo {
        metadata: registry_metadata,
        incumbents,
        incumbent_keys,
        endpoints,
    } = registry_info;
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
            facts.relation.insert(relation_value(&record));
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
        let hits = facts
            .hits
            .iter()
            .filter(|hit| {
                joining
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
    let mut summary = SessionSolveSummary {
        node_count: nodes.len(),
        ..SessionSolveSummary::default()
    };
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
        let key_conflict = rules.authoritative_keys.iter().any(|namespace| {
            member_nodes
                .iter()
                .filter_map(|node| node.keys.get(namespace))
                .flat_map(|values| values.iter())
                .collect::<BTreeSet<_>>()
                .len()
                > 1
        });
        let incumbent_ids = member_nodes
            .iter()
            .filter_map(|node| node.incumbent_canonical_id.clone())
            .collect::<BTreeSet<_>>();
        let distinct_conflict = same_node_distinct
            .iter()
            .any(|key| member_keys.contains(key))
            || !component.hard_cannot_link_violations.is_empty();
        let (outcome, tier, reason) = if distinct_conflict {
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
            if tier.as_deref() == Some("A") {
                summary.accepted_tier_a += 1;
            } else {
                summary.accepted_tier_b += 1;
            }
        } else if outcome == "resolved_existing" {
            summary.resolved_existing += 1;
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
        });
    }

    let mut review = edge_facts
        .values()
        .filter(|facts| facts.positive && tier_for(facts, &rules) == Some(EdgeTier::C))
        .map(|facts| {
            let evidence_types = facts
                .evidence_types
                .iter()
                .filter(|kind| is_positive_type(kind))
                .cloned()
                .collect::<Vec<_>>();
            let missing = if evidence_types.len() < rules.min_distinct_evidence_types {
                "second_evidence_type"
            } else {
                "second_lineage"
            };
            SessionSolveReview {
                left_surface: nodes
                    .get(&facts.left)
                    .map(|node| node.surface.clone())
                    .unwrap_or_default(),
                right_surface: nodes
                    .get(&facts.right)
                    .map(|node| node.surface.clone())
                    .unwrap_or_default(),
                evidence_types,
                lineage_count: facts.lineages.len(),
                missing: missing.to_string(),
            }
        })
        .collect::<Vec<_>>();
    review.sort_by(|left, right| {
        left.left_surface
            .cmp(&right.left_surface)
            .then_with(|| left.right_surface.cmp(&right.right_surface))
    });
    summary.review = review.len();

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
                relation: relation.clone(),
                evidence_ids: facts.evidence_ids.iter().cloned().collect(),
            })
        })
        .collect::<Vec<_>>();
    relations.sort_by(|left, right| {
        left.left_surface
            .cmp(&right.left_surface)
            .then_with(|| left.right_surface.cmp(&right.right_surface))
            .then_with(|| left.relation.cmp(&right.relation))
    });
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
    let binding = manifest.registries.first().ok_or_else(|| {
        solve_refusal(
            "The session manifest has no registry binding",
            json!({"field": "registries", "writes_performed": false}),
            "Reinitialize the session with --registry, then rerun canon entity session commit",
        )
    })?;
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
    let registry_path = Path::new(&binding.path);
    let loaded_registry = registry::load_registry(registry_path)
        .map_err(|error| Refusal::bad_registry(&binding.path, &error.to_string()))?;
    let registry_snapshot = registry_snapshot_hash(
        registry_path,
        &loaded_registry.meta.id,
        &loaded_registry.meta.version,
    )?;
    let expected_registry = artifact.inputs.registry.clone();
    if expected_registry.id != loaded_registry.meta.id
        || expected_registry.version != loaded_registry.meta.version
        || expected_registry.snapshot_hash != registry_snapshot
    {
        if let Some(output) =
            session::already_committed(&session, &solve_hash, &loaded_registry.meta.version)?
        {
            return Ok(output);
        }
        return Err(stale_solve_refusal(
            "registry",
            &format!(
                "{}@{}:{}",
                expected_registry.id, expected_registry.version, expected_registry.snapshot_hash
            ),
            &format!(
                "{}@{}:{}",
                loaded_registry.meta.id, loaded_registry.meta.version, registry_snapshot
            ),
        ));
    }
    let (entries, skipped) = build_commit_plan(&artifact, &binding.kind)?;
    session::commit(session::SessionCommitRequest {
        session,
        solve_hash,
        observations_generation: artifact.inputs.observations_generation,
        evidence_generation: artifact.inputs.evidence_generation,
        registry: SessionCommitRegistryExpectation {
            id: expected_registry.id,
            version: expected_registry.version,
            snapshot_hash: expected_registry.snapshot_hash,
        },
        entries,
        skipped,
        next_version: request.next_version,
        bump: request.bump,
    })
}

fn build_commit_plan(
    artifact: &SessionSolveArtifact,
    entity_kind: &str,
) -> Result<(Vec<SessionCommitEntry>, Vec<SessionCommitSkipped>), Refusal> {
    let mut clusters = artifact.clusters.clone();
    clusters.sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    for cluster in clusters {
        let accepted =
            cluster.outcome == "accepted" && matches!(cluster.tier.as_deref(), Some("A" | "B"));
        if !accepted {
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
            skipped.push(SessionCommitSkipped {
                cluster_id: cluster.cluster_id,
                reason: reason.to_string(),
            });
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
                    alias_file: "aliases.json".to_string(),
                    input: surface,
                    canonical_type: entity_kind.to_string(),
                    rule_id: format!("SESSION_TIER_{tier}"),
                    skip_if_existing: cluster.incumbent_canonical_id.is_some(),
                });
            }
        }
        for namespace in &artifact.authoritative_keys {
            let Some(values) = cluster.keys.get(namespace) else {
                continue;
            };
            let mut values = values.clone();
            values.sort();
            for value in values {
                if seen_inputs.insert(value.clone()) {
                    aliases.push(SessionCommitAlias {
                        alias_file: "aliases.json".to_string(),
                        input: value,
                        canonical_type: entity_kind.to_string(),
                        rule_id: format!("SESSION_KEY_{}", namespace.to_ascii_uppercase()),
                        skip_if_existing: true,
                    });
                }
            }
        }
        let closure_bytes = serde_json::to_vec(&cluster).map_err(|error| {
            solve_refusal(
                format!(
                    "Unable to hash accepted cluster {}: {error}",
                    cluster.cluster_id
                ),
                json!({"cluster_id": cluster.cluster_id, "writes_performed": false}),
                "Rerun canon entity session solve, then rerun commit",
            )
        })?;
        entries.push(SessionCommitEntry {
            cluster_id: cluster.cluster_id.clone(),
            tier,
            outcome_ref: format!("entity-session:{}", cluster.cluster_id),
            closure_hash: hash_bytes(&closure_bytes),
            canonical_id: cluster.incumbent_canonical_id.clone(),
            aliases,
        });
    }
    Ok((entries, skipped))
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

fn parse_rules(path: &Path) -> Result<(SessionRules, String), Refusal> {
    let text = fs::read_to_string(path).map_err(|error| {
        solve_refusal(
            format!("Unable to read rules file '{}': {error}", path.display()),
            json!({"path": path.display().to_string(), "writes_performed": false}),
            "Provide a readable canon.entity.session.rules.v1 YAML file",
        )
    })?;
    let value = serde_yaml::from_str::<Value>(&text).map_err(|error| {
        rules_path_refusal(path, "<document>", format!("invalid YAML: {error}"))
    })?;
    let root = value
        .as_object()
        .ok_or_else(|| rules_path_refusal(path, "<document>", "rules must be a YAML mapping"))?;
    reject_unknown_keys(
        root,
        &[
            "version",
            "keys",
            "tier_a",
            "tier_b",
            "standalone",
            "incumbent_attach",
        ],
        path,
        "",
    )?;
    let version = required_string(root, "version", path, "version")?;
    if version != SESSION_RULES_VERSION {
        return Err(rules_path_refusal(
            path,
            "version",
            format!("expected {SESSION_RULES_VERSION}, got {version}"),
        ));
    }
    let keys = required_mapping(root, "keys", path, "keys")?;
    reject_unknown_keys(keys, &["authoritative"], path, "keys")?;
    let authoritative_keys = string_list(
        keys,
        "authoritative",
        path,
        "keys.authoritative",
        &["cik", "lei"],
    )?;
    let tier_a = required_mapping(root, "tier_a", path, "tier_a")?;
    reject_unknown_keys(tier_a, &["evidence_types"], path, "tier_a")?;
    let tier_a_types = string_list(
        tier_a,
        "evidence_types",
        path,
        "tier_a.evidence_types",
        TIER_A_TYPES,
    )?;
    let tier_b = required_mapping(root, "tier_b", path, "tier_b")?;
    reject_unknown_keys(
        tier_b,
        &[
            "eligible_evidence_types",
            "min_distinct_evidence_types",
            "min_distinct_lineages",
        ],
        path,
        "tier_b",
    )?;
    let tier_b_types = string_list(
        tier_b,
        "eligible_evidence_types",
        path,
        "tier_b.eligible_evidence_types",
        TIER_B_TYPES,
    )?;
    let min_distinct_evidence_types = positive_usize(
        tier_b,
        "min_distinct_evidence_types",
        path,
        "tier_b.min_distinct_evidence_types",
    )?;
    let min_distinct_lineages = positive_usize(
        tier_b,
        "min_distinct_lineages",
        path,
        "tier_b.min_distinct_lineages",
    )?;
    let mint_when_authoritative_key = match root.get("standalone") {
        None => false,
        Some(value) => {
            let standalone = value.as_object().ok_or_else(|| {
                rules_path_refusal(path, "standalone", "standalone must be a YAML mapping")
            })?;
            reject_unknown_keys(
                standalone,
                &["mint_when_authoritative_key"],
                path,
                "standalone",
            )?;
            match standalone.get("mint_when_authoritative_key") {
                None => false,
                Some(value) => value.as_bool().ok_or_else(|| {
                    rules_path_refusal(
                        path,
                        "standalone.mint_when_authoritative_key",
                        "value must be true or false",
                    )
                })?,
            }
        }
    };
    let incumbent_attach_steps = match root.get("incumbent_attach") {
        None => Vec::new(),
        Some(value) => {
            let incumbent_attach = value.as_object().ok_or_else(|| {
                rules_path_refusal(path, "incumbent_attach", "value must be a YAML mapping")
            })?;
            reject_unknown_keys(
                incumbent_attach,
                &["name_equivalent_steps"],
                path,
                "incumbent_attach",
            )?;
            string_list(
                incumbent_attach,
                "name_equivalent_steps",
                path,
                "incumbent_attach.name_equivalent_steps",
                INCUMBENT_ATTACH_STEPS,
            )?
        }
    };
    let rules = SessionRules {
        version,
        authoritative_keys,
        tier_a_types,
        tier_b_types,
        min_distinct_evidence_types,
        min_distinct_lineages,
        mint_when_authoritative_key,
        incumbent_attach_steps,
    };
    let canonical = serde_json::to_vec(&rules).map_err(|error| {
        rules_path_refusal(path, "<document>", format!("cannot hash rules: {error}"))
    })?;
    Ok((rules, hash_bytes(&canonical)))
}

const SESSION_RULES_VERSION: &str = "canon.entity.session.rules.v1";

fn reject_unknown_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    path: &Path,
    prefix: &str,
) -> Result<(), Refusal> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            let yaml_path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            return Err(rules_path_refusal(path, &yaml_path, "unknown key"));
        }
    }
    Ok(())
}

fn required_mapping<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
) -> Result<&'a Map<String, Value>, Refusal> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| rules_path_refusal(path, yaml_path, "value must be a YAML mapping"))
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
) -> Result<String, Refusal> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| rules_path_refusal(path, yaml_path, "value must be a non-empty string"))
}

fn string_list(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
    allowed: &[&str],
) -> Result<Vec<String>, Refusal> {
    let values = object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| rules_path_refusal(path, yaml_path, "value must be a YAML sequence"))?;
    if values.is_empty() {
        return Err(rules_path_refusal(
            path,
            yaml_path,
            "sequence must not be empty",
        ));
    }
    let mut parsed = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let item_path = format!("{yaml_path}[{index}]");
        let item = value
            .as_str()
            .ok_or_else(|| rules_path_refusal(path, &item_path, "value must be a string"))?;
        if !allowed.contains(&item) {
            return Err(rules_path_refusal(
                path,
                &item_path,
                "unknown evidence or key value",
            ));
        }
        parsed.insert(item.to_string());
    }
    Ok(parsed.into_iter().collect())
}

fn positive_usize(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
) -> Result<usize, Refusal> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| rules_path_refusal(path, yaml_path, "value must be a positive integer"))
}

fn rules_path_refusal(path: &Path, yaml_path: &str, message: impl Into<String>) -> Refusal {
    solve_refusal(
        format!("Rules YAML path {yaml_path}: {}", message.into()),
        json!({
            "path": path.display().to_string(),
            "yaml_path": yaml_path,
            "writes_performed": false
        }),
        "Fix the named rules YAML path, then rerun canon entity session solve",
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
                .collect::<BTreeMap<_, _>>();
            let lineages = raw
                .source
                .lineage
                .as_deref()
                .map(ascii_trim)
                .filter(|value| !value.is_empty())
                .map(|value| BTreeSet::from([value.to_string()]))
                .unwrap_or_default();
            let node_key = format!("{entity_type}\u{0}{surface}");
            let observation = Observation {
                obs_id: obs_id.clone(),
                entity_type,
                surface,
                node_key,
                keys,
                lineages,
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
    let endpoint_targets = evidence
        .iter()
        .filter_map(pair_target)
        .flat_map(|(left, right)| [left, right])
        .filter_map(registry_endpoint_spec)
        .collect::<BTreeMap<_, _>>();
    let mut endpoint_targets = endpoint_targets;
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
                .map(|mapping| (observation.surface.clone(), mapping.canonical_id.clone()))
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
        .map(|(canonical_id, rows)| (canonical_id.clone(), key_values(rows)))
        .collect::<BTreeMap<_, _>>();
    let mut endpoints = BTreeMap::new();
    for (target, spec) in endpoint_targets {
        let endpoint = match spec {
            RegistryEndpointSpec::Alias(surface) => mappings.get(&surface).and_then(|mapping| {
                Some(RegistryEndpoint {
                    entity_type: entity_type.to_string(),
                    node_key: format!("{entity_type}\u{0}{surface}"),
                    surface,
                    canonical_id: mapping.canonical_id.clone(),
                    keys: incumbent_keys
                        .get(&mapping.canonical_id)
                        .cloned()
                        .unwrap_or_default(),
                    alias_endpoint: true,
                })
            }),
            RegistryEndpointSpec::Key { namespace, value } => {
                let expected_rule = format!("SESSION_KEY_{}", namespace.to_ascii_uppercase());
                mappings.get(&value).and_then(|mapping| {
                    (mapping.rule_id == expected_rule).then(|| {
                        let mut keys = incumbent_keys
                            .get(&mapping.canonical_id)
                            .cloned()
                            .unwrap_or_default();
                        keys.entry(namespace).or_default().insert(value.clone());
                        RegistryEndpoint {
                            entity_type: entity_type.to_string(),
                            node_key: format!("{entity_type}\u{0}{value}"),
                            surface: value,
                            canonical_id: mapping.canonical_id.clone(),
                            keys,
                            alias_endpoint: false,
                        }
                    })
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
        },
        incumbents,
        incumbent_keys,
        endpoints,
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
        keys.entry(namespace.to_ascii_lowercase())
            .or_insert_with(BTreeSet::new)
            .insert(row.input.clone());
    }
    keys
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
        let incumbent_canonical_id = incumbents.get(&observation.surface).cloned();
        let node = nodes
            .entry(observation.node_key.clone())
            .or_insert_with(|| Node {
                surface: observation.surface.clone(),
                observation_ids: BTreeSet::new(),
                keys: BTreeMap::new(),
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
        }
        if let Some(canonical_id) = incumbent_canonical_id
            && let Some(existing_keys) = incumbent_keys.get(&canonical_id)
        {
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
    for namespace in &rules.authoritative_keys {
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
        if !rules.authoritative_keys.iter().any(|key| key == namespace) {
            continue;
        }
        for observation in observations.values().filter(|observation| {
            observation.entity_type.as_str() == endpoint.entity_type.as_str()
                && observation
                    .keys
                    .get(namespace)
                    .is_some_and(|key| key == value)
        }) {
            if observation.node_key == endpoint.node_key {
                continue;
            }
            let (left, right) = ordered_pair(&observation.node_key, &endpoint.node_key);
            let record = derived_same_key_record(&observation.obs_id, target, namespace, value)?;
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
            alias_endpoint: false,
        });
    }
    endpoints.get(target).map(|endpoint| TargetInfo {
        node_key: endpoint.node_key.clone(),
        entity_type: endpoint.entity_type.clone(),
        surface: endpoint.surface.clone(),
        lineages: BTreeSet::new(),
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
    if facts
        .evidence_types
        .iter()
        .any(|kind| rules.tier_a_types.contains(kind))
    {
        Some(EdgeTier::A)
    } else if facts
        .evidence_types
        .iter()
        .filter(|kind| rules.tier_b_types.contains(kind))
        .count()
        >= rules.min_distinct_evidence_types
        && facts.lineages.len() >= rules.min_distinct_lineages
    {
        Some(EdgeTier::B)
    } else {
        Some(EdgeTier::C)
    }
}

fn exactly_one_authoritative_value(node: &Node, rules: &SessionRules) -> bool {
    rules
        .authoritative_keys
        .iter()
        .filter_map(|key| node.keys.get(key))
        .filter(|values| values.len() == 1)
        .count()
        == 1
        && rules
            .authoritative_keys
            .iter()
            .all(|key| node.keys.get(key).is_none_or(|values| values.len() <= 1))
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
        EdgeFacts, EdgeTier, SessionRules, SessionSolveArtifact, SessionSolveCluster,
        SessionSolveInputs, SessionSolveRegistry, SessionSolveSummary, TIER_A_TYPES, TIER_B_TYPES,
        build_commit_plan, exactly_one_authoritative_value, tier_for,
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn rules() -> SessionRules {
        SessionRules {
            version: "canon.entity.session.rules.v1".to_string(),
            authoritative_keys: vec!["cik".to_string(), "lei".to_string()],
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
            mint_when_authoritative_key: true,
            incumbent_attach_steps: Vec::new(),
        }
    }

    fn facts(types: &[&str], lineages: &[&str]) -> EdgeFacts {
        EdgeFacts {
            positive: true,
            evidence_types: types.iter().map(|value| (*value).to_string()).collect(),
            lineages: lineages.iter().map(|value| (*value).to_string()).collect(),
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
    fn exactly_one_authoritative_value_is_required_for_single_keyed_acceptance() {
        let mut node = super::Node {
            surface: "Solo".to_string(),
            observation_ids: BTreeSet::from(["obs".to_string()]),
            keys: std::collections::BTreeMap::from([(
                "cik".to_string(),
                BTreeSet::from(["555".to_string()]),
            )]),
            incumbent_canonical_id: None,
        };
        assert!(exactly_one_authoritative_value(&node, &rules()));
        node.keys
            .insert("lei".to_string(), BTreeSet::from(["LEI".to_string()]));
        assert!(!exactly_one_authoritative_value(&node, &rules()));
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
                },
            },
            authoritative_keys: vec!["cik".to_string()],
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
                },
            ],
            review: Vec::new(),
            relations: Vec::new(),
            contradictions: Vec::new(),
            standalone_count: 0,
            dangling_evidence_count: 0,
            derived_evidence: Vec::new(),
        };

        let (entries, skipped) = build_commit_plan(&artifact, "organisation").expect("commit plan");
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
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].cluster_id, "cluster-r");
        assert_eq!(skipped[0].reason, "review");
    }
}
