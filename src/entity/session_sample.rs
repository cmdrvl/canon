#![forbid(unsafe_code)]

//! Deterministic, without-replacement samples of a session solve and the
//! small arithmetic report used to score operator labels.

use crate::{
    Refusal, RefusalCode,
    entity::{
        publication::open_current_stream_generation,
        session::{SESSION_SOLVE_STREAM_ID, SessionSolveArtifact},
    },
    session::SESSION_OBSERVATIONS_STREAM_ID,
    witness::hash_bytes,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const SESSION_SAMPLE_VERSION: &str = "canon.entity.session.sample.v1";
pub const SESSION_SCORE_VERSION: &str = "canon.entity.session.sample.score.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleStratum {
    TierA,
    TierB,
    TierC,
    Standalone,
    Neighbourhood,
}

impl SampleStratum {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TierA => "tier_a",
            Self::TierB => "tier_b",
            Self::TierC => "tier_c",
            Self::Standalone => "standalone",
            Self::Neighbourhood => "neighbourhood",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tier_a" => Some(Self::TierA),
            "tier_b" => Some(Self::TierB),
            "tier_c" => Some(Self::TierC),
            "standalone" => Some(Self::Standalone),
            "neighbourhood" => Some(Self::Neighbourhood),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionSampleRequest {
    pub session: PathBuf,
    pub stratum: SampleStratum,
    pub count: usize,
    pub seed: String,
    pub entity_type: Option<String>,
    pub blind: bool,
    pub seed_entity: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSampleOutput {
    pub header: Value,
    pub rows: Vec<Value>,
}

impl SessionSampleOutput {
    pub fn render_jsonl(&self) -> Result<String, Refusal> {
        let mut lines = Vec::with_capacity(self.rows.len() + 1);
        lines.push(serde_json::to_string(&self.header).map_err(|error| {
            sample_refusal(
                format!("Unable to serialize sample header: {error}"),
                json!({"writes_performed": false}),
            )
        })?);
        for row in &self.rows {
            lines.push(serde_json::to_string(row).map_err(|error| {
                sample_refusal(
                    format!("Unable to serialize sample row: {error}"),
                    json!({"writes_performed": false}),
                )
            })?);
        }
        Ok(format!("{}\n", lines.join("\n")))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionScoreStratum {
    pub stratum: String,
    pub n: usize,
    pub same: usize,
    pub different: usize,
    pub related: usize,
    pub unsure: usize,
    pub precision: Option<f64>,
    pub share_same: Option<f64>,
    pub population_size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionScoreOutput {
    pub version: String,
    pub solve_hash: String,
    pub strata: Vec<SessionScoreStratum>,
}

#[derive(Debug, Clone)]
struct SampleCandidate {
    sample_id: String,
    left_surface: String,
    right_surface: String,
    stratum: String,
    evidence_types: Vec<String>,
}

pub fn sample_session(request: SessionSampleRequest) -> Result<SessionSampleOutput, Refusal> {
    if request.count == 0 {
        return Err(sample_refusal(
            "Session sample count must be greater than zero",
            json!({"field": "count", "actual": request.count}),
        ));
    }
    if request.stratum == SampleStratum::Neighbourhood && request.seed_entity.is_none() {
        return Err(sample_refusal(
            "Neighbourhood sampling requires --seed-entity",
            json!({"field": "seed_entity", "writes_performed": false}),
        ));
    }
    let (artifact, solve_hash) = read_solve_artifact(&request.session)?;
    let contexts = read_observation_contexts(&request.session)?;
    let mut candidates = candidates_for(&artifact, request.stratum, request.seed_entity.as_deref());
    if let Some(entity_type) = request.entity_type.as_deref() {
        candidates.retain(|candidate| {
            contexts
                .get(&candidate.left_surface)
                .is_some_and(|context| context.entity_type == entity_type)
                && contexts
                    .get(&candidate.right_surface)
                    .is_some_and(|context| context.entity_type == entity_type)
        });
    }
    let population_size = candidates.len();
    let mut ranked = candidates;
    ranked.sort_by(|left, right| {
        sample_rank(&request.seed, left)
            .cmp(&sample_rank(&request.seed, right))
            .then_with(|| left.sample_id.cmp(&right.sample_id))
    });
    ranked.truncate(request.count);

    let mut header = json!({
        "kind": "canon.entity.session.sample.header",
        "version": SESSION_SAMPLE_VERSION,
        "solve_hash": solve_hash,
        "seed": request.seed,
        "population_size": population_size,
        "requested_count": request.count,
        "returned_count": ranked.len(),
        "without_replacement": true,
        "exhausted": ranked.len() < request.count,
    });
    if !request.blind {
        header["stratum"] = Value::String(request.stratum.as_str().to_string());
        if let Some(entity_type) = request.entity_type.as_deref() {
            header["entity_type"] = Value::String(entity_type.to_string());
        }
        if let Some(seed_entity) = request.seed_entity.as_deref() {
            header["seed_entity"] = Value::String(seed_entity.to_string());
        }
    }
    let rows = ranked
        .into_iter()
        .map(|candidate| {
            let mut row = json!({
                "sample_id": candidate.sample_id,
                "left_surface": candidate.left_surface,
                "right_surface": candidate.right_surface,
                "context": {
                    "left": observation_context(&contexts, &candidate.left_surface),
                    "right": observation_context(&contexts, &candidate.right_surface),
                },
            });
            if !request.blind {
                row["stratum"] = Value::String(candidate.stratum);
                row["evidence_types"] = json!(candidate.evidence_types);
            }
            row
        })
        .collect();
    Ok(SessionSampleOutput { header, rows })
}

pub fn score_session(session: &Path, labels_path: &Path) -> Result<SessionScoreOutput, Refusal> {
    let (artifact, solve_hash) = read_solve_artifact(session)?;
    let all_candidates = all_candidates(&artifact);
    let candidate_by_id = all_candidates
        .iter()
        .map(|candidate| (candidate.sample_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let text = fs::read_to_string(labels_path).map_err(|error| {
        sample_refusal(
            format!(
                "Unable to read labels file {}: {error}",
                labels_path.display()
            ),
            json!({"path": labels_path.display().to_string()}),
        )
    })?;
    let mut expected_solve_hash = None;
    let mut labels = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|error| {
            sample_refusal(
                format!("Labels line {} is not valid JSON: {error}", line_number + 1),
                json!({"line": line_number + 1}),
            )
        })?;
        if value.get("kind").and_then(Value::as_str) == Some("canon.entity.session.sample.header") {
            expected_solve_hash = value
                .get("solve_hash")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            continue;
        }
        let solve_hash_on_label = value.get("solve_hash").and_then(Value::as_str);
        if let Some(value) = solve_hash_on_label {
            if expected_solve_hash
                .as_deref()
                .is_some_and(|expected| expected != value)
            {
                return Err(sample_refusal(
                    "Labels file contains more than one solve hash",
                    json!({"line": line_number + 1}),
                ));
            }
            expected_solve_hash = Some(value.to_string());
        }
        labels.push((line_number + 1, value));
    }
    if expected_solve_hash.as_deref() != Some(solve_hash.as_str()) {
        return Err(sample_refusal(
            "Labels solve hash does not match the current session solve",
            json!({
                "expected": solve_hash,
                "actual": expected_solve_hash,
                "writes_performed": false,
            }),
        ));
    }

    let mut seen = BTreeSet::new();
    let mut counts = BTreeMap::<String, (usize, usize, usize, usize)>::new();
    let mut populations = BTreeMap::<String, usize>::new();
    for candidate in &all_candidates {
        *populations.entry(candidate.stratum.clone()).or_default() += 1;
    }
    for (line_number, value) in labels {
        let sample_id = value
            .get("sample_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                sample_refusal(
                    format!("Labels line {line_number} is missing sample_id"),
                    json!({"line": line_number}),
                )
            })?;
        let candidate = candidate_by_id.get(sample_id).ok_or_else(|| {
            sample_refusal(
                format!("Labels line {line_number} names an unknown sample_id"),
                json!({"line": line_number, "sample_id": sample_id}),
            )
        })?;
        if !seen.insert(sample_id.to_string()) {
            return Err(sample_refusal(
                format!("Labels repeat sample_id on line {line_number}"),
                json!({"line": line_number, "sample_id": sample_id}),
            ));
        }
        let label = value.get("label").and_then(Value::as_str).ok_or_else(|| {
            sample_refusal(
                format!("Labels line {line_number} is missing label"),
                json!({"line": line_number}),
            )
        })?;
        if !matches!(label, "same" | "different" | "related" | "unsure") {
            return Err(sample_refusal(
                format!("Labels line {line_number} has unsupported label {label}"),
                json!({"line": line_number, "label": label}),
            ));
        }
        let labeller = value.get("labeller").and_then(Value::as_str).unwrap_or("");
        if labeller.trim().is_empty() {
            return Err(sample_refusal(
                format!("Labels line {line_number} is missing labeller"),
                json!({"line": line_number}),
            ));
        }
        let entry = counts.entry(candidate.stratum.clone()).or_default();
        match label {
            "same" => entry.0 += 1,
            "different" => entry.1 += 1,
            "related" => entry.2 += 1,
            "unsure" => entry.3 += 1,
            _ => {
                return Err(sample_refusal(
                    format!("Labels line {line_number} has unsupported label {label}"),
                    json!({"line": line_number, "label": label}),
                ));
            }
        }
    }
    let strata = counts
        .into_iter()
        .map(|(stratum, (same, different, related, unsure))| {
            let n = same + different + related + unsure;
            let accepted_denominator = same + different + related;
            let accepted = matches!(
                SampleStratum::parse(&stratum),
                Some(SampleStratum::TierA | SampleStratum::TierB)
            );
            SessionScoreStratum {
                population_size: populations.get(&stratum).copied().unwrap_or_default(),
                stratum,
                n,
                same,
                different,
                related,
                unsure,
                precision: accepted
                    .then(|| {
                        (accepted_denominator > 0)
                            .then_some(same as f64 / accepted_denominator as f64)
                    })
                    .flatten(),
                share_same: (!accepted).then(|| same as f64 / n.max(1) as f64),
            }
        })
        .collect();
    Ok(SessionScoreOutput {
        version: SESSION_SCORE_VERSION.to_string(),
        solve_hash,
        strata,
    })
}

fn candidates_for(
    artifact: &SessionSolveArtifact,
    stratum: SampleStratum,
    seed_entity: Option<&str>,
) -> Vec<SampleCandidate> {
    match stratum {
        SampleStratum::TierA | SampleStratum::TierB => {
            let tier = if stratum == SampleStratum::TierA {
                "A"
            } else {
                "B"
            };
            artifact
                .clusters
                .iter()
                .filter(|cluster| {
                    cluster.outcome == "accepted" && cluster.tier.as_deref() == Some(tier)
                })
                .flat_map(|cluster| {
                    pair_candidates(cluster.surfaces.clone(), stratum.as_str(), Vec::new())
                })
                .collect()
        }
        SampleStratum::TierC => artifact
            .review
            .iter()
            .map(|review| {
                candidate(
                    review.left_surface.clone(),
                    review.right_surface.clone(),
                    stratum.as_str(),
                    review.evidence_types.clone(),
                )
            })
            .collect(),
        SampleStratum::Standalone => {
            let mut surfaces = artifact
                .clusters
                .iter()
                .filter(|cluster| cluster.outcome == "standalone")
                .flat_map(|cluster| cluster.surfaces.iter().cloned())
                .collect::<Vec<_>>();
            surfaces.sort();
            let all_unaccepted = artifact
                .clusters
                .iter()
                .filter(|cluster| cluster.outcome != "accepted")
                .flat_map(|cluster| cluster.surfaces.iter().cloned())
                .collect::<BTreeSet<_>>();
            surfaces
                .into_iter()
                .filter_map(|surface| {
                    all_unaccepted
                        .iter()
                        .filter(|other| *other != &surface)
                        .min_by(|left, right| left.cmp(right))
                        .map(|other| {
                            candidate(surface.clone(), other.clone(), stratum.as_str(), Vec::new())
                        })
                })
                .collect()
        }
        SampleStratum::Neighbourhood => {
            neighbourhood_candidates(artifact, seed_entity.unwrap_or_default())
        }
    }
}

fn all_candidates(artifact: &SessionSolveArtifact) -> Vec<SampleCandidate> {
    let mut candidates = [
        SampleStratum::TierA,
        SampleStratum::TierB,
        SampleStratum::TierC,
        SampleStratum::Standalone,
    ]
    .into_iter()
    .flat_map(|stratum| candidates_for(artifact, stratum, None))
    .collect::<Vec<_>>();
    candidates.extend(all_neighbourhood_candidates(artifact));
    candidates.sort_by(|left, right| left.sample_id.cmp(&right.sample_id));
    candidates.dedup_by(|left, right| left.sample_id == right.sample_id);
    candidates
}

fn pair_candidates(
    mut surfaces: Vec<String>,
    stratum: &str,
    evidence_types: Vec<String>,
) -> Vec<SampleCandidate> {
    surfaces.sort();
    surfaces.dedup();
    let mut output = Vec::new();
    for (index, left) in surfaces.iter().enumerate() {
        for right in surfaces.iter().skip(index + 1) {
            output.push(candidate(
                left.clone(),
                right.clone(),
                stratum,
                evidence_types.clone(),
            ));
        }
    }
    output
}

fn neighbourhood_candidates(artifact: &SessionSolveArtifact, seed: &str) -> Vec<SampleCandidate> {
    let seed_surfaces = artifact
        .clusters
        .iter()
        .filter(|cluster| {
            cluster.incumbent_canonical_id.as_deref() == Some(seed)
                || cluster.surfaces.iter().any(|surface| surface == seed)
        })
        .flat_map(|cluster| cluster.surfaces.iter().cloned())
        .collect::<BTreeSet<_>>();
    if seed_surfaces.is_empty() {
        return Vec::new();
    }
    let mut output = neighbourhood_candidates_from_surfaces(artifact, &seed_surfaces);
    output.sort_by(|left, right| left.sample_id.cmp(&right.sample_id));
    output.dedup_by(|left, right| left.sample_id == right.sample_id);
    output
}

fn all_neighbourhood_candidates(artifact: &SessionSolveArtifact) -> Vec<SampleCandidate> {
    let surfaces = artifact
        .clusters
        .iter()
        .flat_map(|cluster| cluster.surfaces.iter().cloned())
        .collect::<BTreeSet<_>>();
    neighbourhood_candidates_from_surfaces(artifact, &surfaces)
}

fn neighbourhood_candidates_from_surfaces(
    artifact: &SessionSolveArtifact,
    seed_surfaces: &BTreeSet<String>,
) -> Vec<SampleCandidate> {
    let mut output = Vec::new();
    for review in &artifact.review {
        if seed_surfaces.contains(&review.left_surface)
            || seed_surfaces.contains(&review.right_surface)
        {
            output.push(candidate(
                review.left_surface.clone(),
                review.right_surface.clone(),
                "neighbourhood",
                review.evidence_types.clone(),
            ));
        }
    }
    for relation in &artifact.relations {
        if seed_surfaces.contains(&relation.left_surface)
            || seed_surfaces.contains(&relation.right_surface)
        {
            output.push(candidate(
                relation.left_surface.clone(),
                relation.right_surface.clone(),
                "neighbourhood",
                Vec::new(),
            ));
        }
    }
    for contradiction in &artifact.contradictions {
        if contradiction
            .surfaces
            .iter()
            .any(|surface| seed_surfaces.contains(surface))
        {
            output.extend(pair_candidates(
                contradiction.surfaces.clone(),
                "neighbourhood",
                Vec::new(),
            ));
        }
    }
    output
}

fn candidate(
    left_surface: String,
    right_surface: String,
    stratum: &str,
    evidence_types: Vec<String>,
) -> SampleCandidate {
    let (left_surface, right_surface) = if left_surface <= right_surface {
        (left_surface, right_surface)
    } else {
        (right_surface, left_surface)
    };
    let sample_id = format!(
        "sample:{}",
        hash_bytes(format!("{stratum}\0{left_surface}\0{right_surface}").as_bytes())
    );
    SampleCandidate {
        sample_id,
        left_surface,
        right_surface,
        stratum: stratum.to_string(),
        evidence_types,
    }
}

fn sample_rank(seed: &str, candidate: &SampleCandidate) -> String {
    hash_bytes(format!("{seed}\0{}", candidate.sample_id).as_bytes())
}

#[derive(Debug, Clone)]
struct ObservationContext {
    entity_type: String,
    context: Value,
}

fn observation_context(contexts: &BTreeMap<String, ObservationContext>, surface: &str) -> Value {
    contexts
        .get(surface)
        .map(|context| context.context.clone())
        .unwrap_or_else(|| json!({"surface": surface}))
}

fn read_observation_contexts(
    session: &Path,
) -> Result<BTreeMap<String, ObservationContext>, Refusal> {
    let snapshot = open_current_stream_generation(session, SESSION_OBSERVATIONS_STREAM_ID)
        .map_err(|error| {
            sample_refusal(
                format!("Unable to open session observations: {error}"),
                json!({"session": session.display().to_string()}),
            )
        })?;
    let mut contexts = BTreeMap::new();
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot.read_logical_file(logical_path).ok_or_else(|| {
            sample_refusal(
                format!("Observation shard {logical_path} is missing"),
                json!({"session": session.display().to_string()}),
            )
        })?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            sample_refusal(
                format!("Observation shard {logical_path} is not UTF-8: {error}"),
                json!({"session": session.display().to_string()}),
            )
        })?;
        for (line_number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(line).map_err(|error| {
                sample_refusal(
                    format!("Observation line {} is invalid: {error}", line_number + 1),
                    json!({"line": line_number + 1}),
                )
            })?;
            let Some(surface) = value.get("surface").and_then(Value::as_str) else {
                continue;
            };
            let surface = surface.trim().to_string();
            if surface.is_empty() {
                continue;
            }
            let entity_type = value
                .get("entity_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let context = value.get("context").cloned().unwrap_or_else(|| json!({}));
            contexts.insert(
                surface,
                ObservationContext {
                    entity_type,
                    context,
                },
            );
        }
    }
    Ok(contexts)
}

fn read_solve_artifact(session: &Path) -> Result<(SessionSolveArtifact, String), Refusal> {
    let session = if session.is_absolute() {
        session.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                sample_refusal(
                    format!("Unable to resolve session path: {error}"),
                    json!({}),
                )
            })?
            .join(session)
    };
    let snapshot =
        open_current_stream_generation(&session, SESSION_SOLVE_STREAM_ID).map_err(|error| {
            sample_refusal(
                format!("Unable to open session solve: {error}"),
                json!({"session": session.display().to_string()}),
            )
        })?;
    let bytes = snapshot
        .read_logical_file("solve/solve.json")
        .ok_or_else(|| {
            sample_refusal(
                "Session solve generation is missing solve/solve.json",
                json!({"session": session.display().to_string()}),
            )
        })?;
    let solve_hash = hash_bytes(bytes);
    let artifact = serde_json::from_slice(bytes).map_err(|error| {
        sample_refusal(
            format!("Session solve artifact is invalid: {error}"),
            json!({"session": session.display().to_string()}),
        )
    })?;
    Ok((artifact, solve_hash))
}

fn sample_refusal(message: impl Into<String>, detail: Value) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.into(),
        detail,
        next_command: Some("Run canon entity session solve, then rerun session sample".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::session::{SessionSolveCluster, SessionSolveReview};
    use std::collections::BTreeMap;

    fn artifact() -> SessionSolveArtifact {
        SessionSolveArtifact {
            version: "v1".to_string(),
            inputs: crate::entity::session::SessionSolveInputs {
                observations_generation: "o".to_string(),
                evidence_generation: None,
                rules_hash: "r".to_string(),
                registry: crate::entity::session::SessionSolveRegistry {
                    id: "registry".to_string(),
                    version: "0.1.0".to_string(),
                    snapshot_hash: "s".to_string(),
                    escrow_snapshot_hash: String::new(),
                },
            },
            authoritative_keys: Vec::new(),
            commit_key_namespaces: Vec::new(),
            summary: Default::default(),
            clusters: vec![SessionSolveCluster {
                cluster_id: "a".to_string(),
                outcome: "accepted".to_string(),
                tier: Some("B".to_string()),
                surfaces: vec![
                    "Alpha".to_string(),
                    "Beta".to_string(),
                    "Gamma".to_string(),
                    "Delta".to_string(),
                ],
                observation_count: 3,
                keys: BTreeMap::new(),
                incumbent_canonical_id: None,
                accepting_evidence_ids: vec!["e1".to_string()],
                reason: "tier_b".to_string(),
                conflict_reasons: Vec::new(),
            }],
            review: vec![SessionSolveReview {
                left_surface: "Alpha".to_string(),
                right_surface: "Delta".to_string(),
                evidence_types: vec!["name_equivalent".to_string()],
                lineage_count: 1,
                missing: "second_lineage".to_string(),
                known_pending: false,
                escrow_id: None,
            }],
            relations: Vec::new(),
            contradictions: Vec::new(),
            standalone_count: 0,
            dangling_evidence_count: 0,
            derived_evidence: Vec::new(),
            scoped_aliases: Vec::new(),
            units: Vec::new(),
            shared_names: Vec::new(),
        }
    }

    #[test]
    fn seeded_sample_is_deterministic_and_without_replacement() {
        let artifact = artifact();
        let candidates = candidates_for(&artifact, SampleStratum::TierB, None);
        assert_eq!(candidates.len(), 6);
        let mut ranked_once = candidates.clone();
        ranked_once.sort_by_key(|candidate| sample_rank("7", candidate));
        let mut ranked_twice = candidates.clone();
        ranked_twice.sort_by_key(|candidate| sample_rank("7", candidate));
        assert_eq!(
            ranked_once
                .iter()
                .map(|candidate| candidate.sample_id.as_str())
                .collect::<Vec<_>>(),
            ranked_twice
                .iter()
                .map(|candidate| candidate.sample_id.as_str())
                .collect::<Vec<_>>()
        );
        ranked_once.truncate(2);
        assert_ne!(ranked_once[0].sample_id, ranked_once[1].sample_id);
        let mut ranked_other_seed = candidates;
        ranked_other_seed.sort_by_key(|candidate| sample_rank("8", candidate));
        assert_ne!(
            ranked_once
                .iter()
                .map(|candidate| candidate.sample_id.as_str())
                .collect::<Vec<_>>(),
            ranked_other_seed
                .iter()
                .take(2)
                .map(|candidate| candidate.sample_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn score_denominator_excludes_unsure_for_accepted_precision() {
        let same = 18usize;
        let different = 2usize;
        let unsure = 3usize;
        let accepted = same as f64 / (same + different) as f64;
        let all_labels = same as f64 / (same + different + unsure) as f64;
        assert_eq!(accepted, 0.9);
        assert_ne!(accepted, all_labels);
    }

    #[test]
    fn neighbourhood_sampling_stays_on_the_seed_frontier() {
        let mut artifact = artifact();
        artifact.review.extend([
            SessionSolveReview {
                left_surface: "Beta".to_string(),
                right_surface: "Epsilon".to_string(),
                evidence_types: vec!["same_slot".to_string()],
                lineage_count: 1,
                missing: "second_lineage".to_string(),
                known_pending: false,
                escrow_id: None,
            },
            SessionSolveReview {
                left_surface: "Alpha".to_string(),
                right_surface: "Zeta".to_string(),
                evidence_types: vec!["name_equivalent".to_string()],
                lineage_count: 1,
                missing: "second_lineage".to_string(),
                known_pending: false,
                escrow_id: None,
            },
            SessionSolveReview {
                left_surface: "Unrelated".to_string(),
                right_surface: "Far Away".to_string(),
                evidence_types: vec!["name_equivalent".to_string()],
                lineage_count: 1,
                missing: "second_lineage".to_string(),
                known_pending: false,
                escrow_id: None,
            },
        ]);
        let candidates = candidates_for(&artifact, SampleStratum::Neighbourhood, Some("Alpha"));
        assert_eq!(candidates.len(), 3);
        assert!(candidates.iter().all(|candidate| {
            candidate.left_surface == "Alpha"
                || candidate.right_surface == "Alpha"
                || candidate.left_surface == "Beta"
                || candidate.right_surface == "Beta"
        }));
        assert!(!candidates.iter().any(|candidate| {
            candidate.left_surface == "Unrelated" || candidate.right_surface == "Far Away"
        }));
    }
}
