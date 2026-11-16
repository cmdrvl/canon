//! `canon entity session status` (bd-vdcw.5): where an evidence session stands,
//! computed from the published streams and never stored.
//!
//! The entity-free part (stream heads, staleness reasons, the commit receipt
//! and the next command) lives in `crate::session::status`. This module adds
//! what needs entity types: tier counts restated from the recorded solve and
//! the top unresolved surfaces ranked by how often they were observed.

use super::{
    SESSION_SOLVE_STREAM_ID, SessionSolveArtifact, absolute_session_path, evidence_type,
    parse_rules, read_evidence, read_observations, registry_snapshot_hash, solve_refusal,
};
use crate::{
    Refusal,
    session::{
        SESSION_COMMIT_STREAM_ID, SESSION_EVIDENCE_STREAM_ID, SESSION_OBSERVATIONS_STREAM_ID,
        status::{
            self as base, CommitHead, CurrentInputs, NextCommandInputs, RecordedInputs, StaleReason,
        },
    },
    witness::hash_bytes,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const SESSION_STATUS_VERSION: &str = "canon.entity.session.status.v1";
const TOP_UNRESOLVED_LIMIT: usize = 20;
const SOLVE_ARTIFACT_PATH: &str = "solve/solve.json";

#[derive(Debug, Clone)]
pub struct SessionStatusRequest {
    pub session: PathBuf,
    /// Optional rules file. Only used to detect rule drift against the hash
    /// the solve recorded; status never solves.
    pub rules: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusRegistry {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusSession {
    pub id: String,
    pub goal: String,
    pub registry: StatusRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusEvidence {
    pub total: usize,
    pub by_type: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusLog {
    pub observations: usize,
    pub observation_shards: usize,
    pub evidence: StatusEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusCounts {
    pub accepted_tier_a: usize,
    pub accepted_tier_b: usize,
    pub resolved_existing: usize,
    pub review: usize,
    pub standalone: usize,
    pub contradictions: usize,
    pub contradictions_by_reason: BTreeMap<String, usize>,
    pub dangling_evidence: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusSolve {
    pub present: bool,
    pub stale: bool,
    pub stale_reasons: Vec<StaleReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solve_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<StatusCounts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusCommit {
    pub present: bool,
    pub last_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
    pub minted: usize,
    pub aliases_attached: usize,
    pub uncommitted_accepted_clusters: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopUnresolved {
    /// `review` (a pair one evidence type or lineage short of acceptance) or
    /// `standalone` (no positive evidence).
    pub kind: String,
    pub surface: String,
    /// The other side of a review pair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_surface: Option<String>,
    /// Observations of the surface (both surfaces for a review pair).
    pub observation_count: usize,
    pub lineage_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionStatusOutput {
    pub version: String,
    pub session: StatusSession,
    pub log: StatusLog,
    pub solve: StatusSolve,
    pub commit: StatusCommit,
    pub top_unresolved: Vec<TopUnresolved>,
    /// `null` when nothing is pending. Only commands that exist are emitted.
    pub next_command: Option<String>,
}

impl SessionStatusOutput {
    pub fn render_summary(&self) -> String {
        let mut lines = vec![
            format!(
                "session {} registry {}@{}",
                self.session.id, self.session.registry.id, self.session.registry.version
            ),
            format!("goal: {}", self.session.goal),
            format!(
                "log: observations={} shards={} evidence={}{}",
                self.log.observations,
                self.log.observation_shards,
                self.log.evidence.total,
                render_by_type(&self.log.evidence.by_type),
            ),
        ];
        match &self.solve.counts {
            Some(counts) => lines.push(format!(
                "solve: stale={}{} tier_a={} tier_b={} resolved_existing={} review={} standalone={} contradictions={}{} dangling_evidence={}",
                self.solve.stale,
                render_reasons(&self.solve.stale_reasons),
                counts.accepted_tier_a,
                counts.accepted_tier_b,
                counts.resolved_existing,
                counts.review,
                counts.standalone,
                counts.contradictions,
                render_by_type(&counts.contradictions_by_reason),
                counts.dangling_evidence,
            )),
            None => lines.push("solve: none".to_string()),
        }
        lines.push(format!(
            "commit: version={} minted={} aliases_attached={} uncommitted_accepted={}",
            self.commit.last_version.as_deref().unwrap_or("<none>"),
            self.commit.minted,
            self.commit.aliases_attached,
            self.commit.uncommitted_accepted_clusters,
        ));
        if !self.top_unresolved.is_empty() {
            lines.push("top unresolved (kind observations lineages surface):".to_string());
            for item in &self.top_unresolved {
                let surface = match &item.other_surface {
                    Some(other) => format!("{} <> {}", item.surface, other),
                    None => item.surface.clone(),
                };
                let missing = item
                    .missing
                    .as_deref()
                    .map(|missing| format!(" (missing {missing})"))
                    .unwrap_or_default();
                lines.push(format!(
                    "  {:<10} {:>5} {:>3}  {}{}",
                    item.kind, item.observation_count, item.lineage_count, surface, missing
                ));
            }
        }
        lines.push(format!(
            "next: {}",
            self.next_command.as_deref().unwrap_or("none")
        ));
        lines.join("\n")
    }
}

fn render_by_type(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return String::new();
    }
    let parts = counts
        .iter()
        .map(|(name, count)| format!("{name}:{count}"))
        .collect::<Vec<_>>();
    format!(" ({})", parts.join(","))
}

fn render_reasons(reasons: &[StaleReason]) -> String {
    if reasons.is_empty() {
        return String::new();
    }
    let parts = reasons
        .iter()
        .map(|reason| {
            serde_json::to_value(reason)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    format!(" ({})", parts.join(","))
}

/// Compute the status of one session. Reads only: no file in the session or
/// its registry is created, changed or touched.
pub fn session_status(request: SessionStatusRequest) -> Result<SessionStatusOutput, Refusal> {
    let session_dir = absolute_session_path(&request.session)?;
    let session_arg = request.session.display().to_string();
    let manifest = base::read_manifest(&session_dir)?;
    let binding = manifest.registries.first().ok_or_else(|| {
        solve_refusal(
            "The session manifest has no registry binding",
            json!({"field": "registries", "writes_performed": false}),
            "Reinitialize the session with --registry, then rerun status",
        )
    })?;

    let observations_head = base::open_stream_head(&session_dir, SESSION_OBSERVATIONS_STREAM_ID)?;
    let evidence_head = base::open_stream_head(&session_dir, SESSION_EVIDENCE_STREAM_ID)?;
    let solve_head = base::open_stream_head(&session_dir, SESSION_SOLVE_STREAM_ID)?;
    let commit_head_snapshot = base::open_stream_head(&session_dir, SESSION_COMMIT_STREAM_ID)?;

    let observations = match observations_head.as_ref() {
        Some(snapshot) => read_observations(snapshot)?,
        None => Vec::new(),
    };
    let observation_shards = observations_head.as_ref().map_or(0, |snapshot| {
        snapshot
            .logical_paths()
            .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
            .count()
    });
    let evidence_records = match evidence_head.as_ref() {
        Some(snapshot) => read_evidence(snapshot)?,
        None => Vec::new(),
    };
    let mut evidence_by_type = BTreeMap::new();
    for record in &evidence_records {
        *evidence_by_type.entry(evidence_type(record)).or_insert(0) += 1;
    }

    let registry_path = Path::new(&binding.path);
    let (registry_id, registry_version) = read_registry_identity(registry_path)?;
    let registry_snapshot = registry_snapshot_hash(registry_path, &registry_id, &registry_version)?;
    let current_registry = registry_key(&registry_id, &registry_version, &registry_snapshot);

    let commit_head = commit_head_snapshot
        .as_ref()
        .map(base::read_commit_head)
        .transpose()?;

    let current_rules_hash = match request.rules.as_deref() {
        Some(path) => Some(parse_rules(path)?.1),
        None => None,
    };

    let mut solve = StatusSolve {
        present: false,
        stale: false,
        stale_reasons: Vec::new(),
        generation_id: None,
        solve_hash: None,
        rules_hash: None,
        counts: None,
    };
    let mut top_unresolved = Vec::new();
    let mut accepted_new_clusters = BTreeSet::new();
    if let Some(snapshot) = solve_head.as_ref() {
        let bytes = snapshot
            .read_logical_file(SOLVE_ARTIFACT_PATH)
            .ok_or_else(|| {
                solve_refusal(
                    "Session solve generation is missing solve/solve.json",
                    json!({"stream": SESSION_SOLVE_STREAM_ID, "writes_performed": false}),
                    "Rerun canon entity session solve",
                )
            })?;
        let artifact = serde_json::from_slice::<SessionSolveArtifact>(bytes).map_err(|error| {
            solve_refusal(
                format!("Session solve artifact is invalid: {error}"),
                json!({"stream": SESSION_SOLVE_STREAM_ID, "writes_performed": false}),
                "Rerun canon entity session solve",
            )
        })?;
        let solve_hash = hash_bytes(bytes);
        let recorded = RecordedInputs {
            observations_generation: artifact.inputs.observations_generation.clone(),
            evidence_generation: artifact.inputs.evidence_generation.clone(),
            rules_hash: artifact.inputs.rules_hash.clone(),
            registry: registry_key(
                &artifact.inputs.registry.id,
                &artifact.inputs.registry.version,
                &artifact.inputs.registry.snapshot_hash,
            ),
        };
        let current = CurrentInputs {
            observations_generation: observations_head
                .as_ref()
                .map(|head| head.generation_id.clone()),
            evidence_generation: evidence_head
                .as_ref()
                .map(|head| head.generation_id.clone()),
            rules_hash: current_rules_hash.clone(),
            registry: current_registry.clone(),
        };
        let own_commit = commit_head.as_ref().is_some_and(|head| {
            head.solve_hash == solve_hash && head.registry_version_after == registry_version
        });
        let reasons = base::stale_reasons(&recorded, &current, own_commit);

        accepted_new_clusters = accepted_new_cluster_ids(&artifact);
        top_unresolved = rank_top_unresolved(&artifact, &observations);
        solve = StatusSolve {
            present: true,
            stale: !reasons.is_empty(),
            stale_reasons: reasons,
            generation_id: Some(snapshot.generation_id.clone()),
            solve_hash: Some(solve_hash),
            rules_hash: Some(artifact.inputs.rules_hash.clone()),
            counts: Some(counts_from(&artifact)),
        };
    }

    let uncommitted = uncommitted_accepted_clusters(
        &accepted_new_clusters,
        commit_head.as_ref(),
        solve.solve_hash.as_deref(),
    );
    let commit = StatusCommit {
        present: commit_head.is_some(),
        last_version: commit_head
            .as_ref()
            .map(|head| head.registry_version_after.clone()),
        generation_id: commit_head.as_ref().map(|head| head.generation_id.clone()),
        minted: commit_head.as_ref().map_or(0, |head| head.minted),
        aliases_attached: commit_head.as_ref().map_or(0, |head| head.aliases_attached),
        uncommitted_accepted_clusters: uncommitted,
    };
    let rules_arg = request
        .rules
        .as_ref()
        .map(|path| path.display().to_string());
    let next_command = base::next_command(&NextCommandInputs {
        session: &session_arg,
        observations_present: observations_head.is_some(),
        solve_present: solve.present,
        stale: solve.stale,
        uncommitted_accepted_clusters: uncommitted,
        rules: rules_arg.as_deref(),
    });

    Ok(SessionStatusOutput {
        version: SESSION_STATUS_VERSION.to_string(),
        session: StatusSession {
            id: manifest.session_id.clone(),
            goal: manifest.goal.clone(),
            registry: StatusRegistry {
                id: registry_id,
                version: registry_version,
            },
        },
        log: StatusLog {
            observations: observations.len(),
            observation_shards,
            evidence: StatusEvidence {
                total: evidence_records.len(),
                by_type: evidence_by_type,
            },
        },
        solve,
        commit,
        top_unresolved,
        next_command,
    })
}

fn registry_key(id: &str, version: &str, snapshot_hash: &str) -> String {
    format!("{id}@{version}:{snapshot_hash}")
}

/// The bound registry's id and version, read straight from `registry.json`.
/// Deliberately not `registry::load_registry`, which may build an index.
fn read_registry_identity(registry: &Path) -> Result<(String, String), Refusal> {
    let path = registry.join("registry.json");
    let bytes = fs::read(&path).map_err(|error| {
        Refusal::bad_registry(&registry.display().to_string(), &error.to_string())
    })?;
    let value = serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|error| {
        Refusal::bad_registry(&registry.display().to_string(), &error.to_string())
    })?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                Refusal::bad_registry(
                    &registry.display().to_string(),
                    &format!("registry.json has no string field {name}"),
                )
            })
    };
    Ok((field("id")?, field("version")?))
}

fn counts_from(artifact: &SessionSolveArtifact) -> StatusCounts {
    let mut by_reason = BTreeMap::new();
    for contradiction in &artifact.contradictions {
        *by_reason
            .entry(contradiction.reason.clone())
            .or_insert(0usize) += 1;
    }
    StatusCounts {
        accepted_tier_a: artifact.summary.accepted_tier_a,
        accepted_tier_b: artifact.summary.accepted_tier_b,
        resolved_existing: artifact.summary.resolved_existing,
        review: artifact.summary.review,
        standalone: artifact.summary.standalone,
        contradictions: artifact.summary.contradiction,
        contradictions_by_reason: by_reason,
        dangling_evidence: artifact.summary.dangling_evidence,
    }
}

/// Accepted tier A/B clusters that would mint a new ID: the same rule as
/// `build_commit_plan`, so status and commit agree on what is committable.
fn accepted_new_cluster_ids(artifact: &SessionSolveArtifact) -> BTreeSet<String> {
    artifact
        .clusters
        .iter()
        .filter(|cluster| {
            cluster.outcome == "accepted"
                && matches!(cluster.tier.as_deref(), Some("A" | "B"))
                && cluster.incumbent_canonical_id.is_none()
        })
        .map(|cluster| cluster.cluster_id.clone())
        .collect()
}

fn uncommitted_accepted_clusters(
    accepted: &BTreeSet<String>,
    commit: Option<&CommitHead>,
    solve_hash: Option<&str>,
) -> usize {
    match (commit, solve_hash) {
        (Some(head), Some(solve_hash)) if head.solve_hash == solve_hash => accepted
            .iter()
            .filter(|id| {
                !head.minted_cluster_ids.contains(*id) && !head.skipped_cluster_ids.contains(*id)
            })
            .count(),
        _ => accepted.len(),
    }
}

#[derive(Default)]
struct SurfaceStats {
    observations: usize,
    lineages: BTreeSet<String>,
}

fn rank_top_unresolved(
    artifact: &SessionSolveArtifact,
    observations: &[super::Observation],
) -> Vec<TopUnresolved> {
    let mut stats = BTreeMap::<&str, SurfaceStats>::new();
    for observation in observations {
        let entry = stats.entry(observation.surface.as_str()).or_default();
        entry.observations += 1;
        entry.lineages.extend(observation.lineages.iter().cloned());
    }
    let observed = |surface: &str| stats.get(surface).map_or(0, |stats| stats.observations);

    let mut items = Vec::new();
    for review in &artifact.review {
        items.push(TopUnresolved {
            kind: "review".to_string(),
            surface: review.left_surface.clone(),
            other_surface: Some(review.right_surface.clone()),
            observation_count: observed(&review.left_surface) + observed(&review.right_surface),
            lineage_count: review.lineage_count,
            evidence_types: Some(review.evidence_types.clone()),
            missing: Some(review.missing.clone()),
        });
    }
    for cluster in artifact
        .clusters
        .iter()
        .filter(|cluster| cluster.outcome == "standalone")
    {
        let lineages = cluster
            .surfaces
            .iter()
            .filter_map(|surface| stats.get(surface.as_str()))
            .flat_map(|stats| stats.lineages.iter())
            .collect::<BTreeSet<_>>();
        items.push(TopUnresolved {
            kind: "standalone".to_string(),
            surface: cluster.surfaces.join(" | "),
            other_surface: None,
            observation_count: cluster.observation_count,
            lineage_count: lineages.len(),
            evidence_types: None,
            missing: None,
        });
    }
    // Frequency first. The name only breaks exact ties, so a surface seen 40
    // times always outranks one seen twice whatever it is called.
    items.sort_by(|left, right| {
        right
            .observation_count
            .cmp(&left.observation_count)
            .then_with(|| right.lineage_count.cmp(&left.lineage_count))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.surface.cmp(&right.surface))
            .then_with(|| left.other_surface.cmp(&right.other_surface))
    });
    items.truncate(TOP_UNRESOLVED_LIMIT);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::session::{
        Observation, SessionSolveCluster, SessionSolveInputs, SessionSolveRegistry,
        SessionSolveReview, SessionSolveSummary,
    };

    /// Every `next_command` status can emit, over every input combination,
    /// must parse against the real clap tree (placeholders filled in), so a
    /// command that does not exist can never be advertised.
    #[test]
    fn every_emittable_next_command_parses_against_the_cli() {
        use clap::Parser;
        let mut emitted = 0;
        for observations_present in [false, true] {
            for solve_present in [false, true] {
                for stale in [false, true] {
                    for uncommitted in [0, 3] {
                        for rules in [None, Some("rules.yaml")] {
                            let inputs = NextCommandInputs {
                                session: "work/s",
                                observations_present,
                                solve_present,
                                stale,
                                uncommitted_accepted_clusters: uncommitted,
                                rules,
                            };
                            let Some(command) = base::next_command(&inputs) else {
                                continue;
                            };
                            emitted += 1;
                            let args: Vec<String> = command
                                .split_whitespace()
                                .map(|word| {
                                    if word.starts_with('<') && word.ends_with('>') {
                                        "placeholder.file".to_string()
                                    } else {
                                        word.to_string()
                                    }
                                })
                                .collect();
                            assert_eq!(args.first().map(String::as_str), Some("canon"));
                            if let Err(error) = crate::cli::Cli::try_parse_from(&args) {
                                panic!(
                                    "status advertised a command clap rejects: {command}\n{error}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(
            emitted >= 3,
            "ingest, solve and commit must all be reachable"
        );
    }

    fn observation(id: &str, surface: &str, lineage: &str) -> Observation {
        Observation {
            obs_id: id.to_string(),
            entity_type: "organisation".to_string(),
            surface: surface.to_string(),
            node_key: format!("organisation\u{0}{surface}"),
            keys: BTreeMap::new(),
            lineages: BTreeSet::from([lineage.to_string()]),
            origin_components: BTreeSet::from([BTreeSet::from([lineage.to_string()])]),
        }
    }

    fn standalone(surface: &str, observation_count: usize) -> SessionSolveCluster {
        SessionSolveCluster {
            cluster_id: format!("c-{surface}"),
            outcome: "standalone".to_string(),
            tier: None,
            surfaces: vec![surface.to_string()],
            observation_count,
            keys: BTreeMap::new(),
            incumbent_canonical_id: None,
            accepting_evidence_ids: Vec::new(),
            reason: "no_positive_evidence".to_string(),
            conflict_reasons: Vec::new(),
        }
    }

    fn accepted(id: &str, tier: &str, incumbent: Option<&str>) -> SessionSolveCluster {
        SessionSolveCluster {
            cluster_id: id.to_string(),
            outcome: "accepted".to_string(),
            tier: Some(tier.to_string()),
            surfaces: vec![id.to_string()],
            observation_count: 1,
            keys: BTreeMap::new(),
            incumbent_canonical_id: incumbent.map(str::to_string),
            accepting_evidence_ids: Vec::new(),
            reason: "tier_a".to_string(),
            conflict_reasons: Vec::new(),
        }
    }

    fn artifact(
        clusters: Vec<SessionSolveCluster>,
        review: Vec<SessionSolveReview>,
    ) -> SessionSolveArtifact {
        SessionSolveArtifact {
            version: super::super::SESSION_SOLVE_VERSION.to_string(),
            inputs: SessionSolveInputs {
                observations_generation: "blake3:o".to_string(),
                evidence_generation: None,
                rules_hash: "blake3:r".to_string(),
                registry: SessionSolveRegistry {
                    id: "orgs".to_string(),
                    version: "0.0.0".to_string(),
                    snapshot_hash: "blake3:s".to_string(),
                },
            },
            authoritative_keys: Vec::new(),
            summary: SessionSolveSummary::default(),
            clusters,
            review,
            relations: Vec::new(),
            contradictions: Vec::new(),
            standalone_count: 0,
            dangling_evidence_count: 0,
            derived_evidence: Vec::new(),
        }
    }

    #[test]
    fn frequency_outranks_name_in_the_ranking() {
        // "Zulu" sorts last by name but was observed 40 times; "Alpha" twice.
        let mut observations = Vec::new();
        for index in 0..40 {
            observations.push(observation(&format!("z{index}"), "Zulu", "fundA"));
        }
        for index in 0..2 {
            observations.push(observation(&format!("a{index}"), "Alpha", "fundA"));
        }
        let artifact = artifact(vec![standalone("Alpha", 2), standalone("Zulu", 40)], vec![]);
        let ranked = rank_top_unresolved(&artifact, &observations);
        let surfaces = ranked
            .iter()
            .map(|item| item.surface.as_str())
            .collect::<Vec<_>>();
        assert_eq!(surfaces, vec!["Zulu", "Alpha"]);
        assert_eq!(ranked[0].observation_count, 40);
        assert_eq!(ranked[1].observation_count, 2);
    }

    #[test]
    fn exact_ties_fall_back_to_lineages_then_name() {
        let observations = vec![
            observation("a1", "Alpha", "fundA"),
            observation("b1", "Beta", "fundA"),
            observation("b2", "Beta", "fundB"),
            observation("c1", "Carbon", "fundA"),
            observation("c2", "Carbon", "fundA"),
        ];
        // Alpha 1 obs; Beta 2 obs / 2 lineages; Carbon 2 obs / 1 lineage.
        let artifact = artifact(
            vec![
                standalone("Carbon", 2),
                standalone("Alpha", 1),
                standalone("Beta", 2),
            ],
            vec![],
        );
        let surfaces = rank_top_unresolved(&artifact, &observations)
            .into_iter()
            .map(|item| item.surface)
            .collect::<Vec<_>>();
        assert_eq!(surfaces, vec!["Beta", "Carbon", "Alpha"]);
    }

    #[test]
    fn review_pair_counts_both_surfaces_and_names_what_is_missing() {
        let observations = vec![
            observation("r1", "Falcon Buyer", "fundA"),
            observation("r2", "Falcon Buyer", "fundA"),
            observation("r3", "Falcon Parent", "fundB"),
        ];
        let artifact = artifact(
            vec![],
            vec![SessionSolveReview {
                left_surface: "Falcon Buyer".to_string(),
                right_surface: "Falcon Parent".to_string(),
                evidence_types: vec!["same_slot".to_string()],
                lineage_count: 2,
                missing: "second_evidence_type".to_string(),
            }],
        );
        let ranked = rank_top_unresolved(&artifact, &observations);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].kind, "review");
        assert_eq!(ranked[0].observation_count, 3);
        assert_eq!(ranked[0].other_surface.as_deref(), Some("Falcon Parent"));
        assert_eq!(ranked[0].missing.as_deref(), Some("second_evidence_type"));
        assert_eq!(
            ranked[0].evidence_types.as_deref(),
            Some(&["same_slot".to_string()][..])
        );
    }

    #[test]
    fn ranking_is_capped_at_twenty() {
        let clusters = (0..30)
            .map(|index| standalone(&format!("S{index:02}"), 1))
            .collect();
        let ranked = rank_top_unresolved(&artifact(clusters, vec![]), &[]);
        assert_eq!(ranked.len(), TOP_UNRESOLVED_LIMIT);
    }

    #[test]
    fn committable_clusters_exclude_incumbents_and_other_outcomes() {
        let artifact = artifact(
            vec![
                accepted("new-a", "A", None),
                accepted("new-b", "B", None),
                accepted("known", "A", Some("ORG-001")),
                standalone("lone", 1),
            ],
            vec![],
        );
        let ids = accepted_new_cluster_ids(&artifact);
        assert_eq!(
            ids.into_iter().collect::<Vec<_>>(),
            vec!["new-a".to_string(), "new-b".to_string()]
        );
    }

    fn head(solve_hash: &str, minted: &[&str], skipped: &[&str]) -> CommitHead {
        CommitHead {
            generation_id: "blake3:g".to_string(),
            solve_hash: solve_hash.to_string(),
            registry_version_after: "0.1.0".to_string(),
            minted: minted.len(),
            aliases_attached: minted.len(),
            minted_cluster_ids: minted.iter().map(|id| (*id).to_string()).collect(),
            skipped_cluster_ids: skipped.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    #[test]
    fn uncommitted_counts_depend_on_the_commit_matching_this_solve() {
        let accepted = BTreeSet::from(["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(uncommitted_accepted_clusters(&accepted, None, Some("h")), 3);
        // A commit of an older solve commits nothing of this one.
        let older = head("old", &["a", "b", "c"], &[]);
        assert_eq!(
            uncommitted_accepted_clusters(&accepted, Some(&older), Some("h")),
            3
        );
        let same = head("h", &["a", "b"], &[]);
        assert_eq!(
            uncommitted_accepted_clusters(&accepted, Some(&same), Some("h")),
            1
        );
        let all = head("h", &["a", "b"], &["c"]);
        assert_eq!(
            uncommitted_accepted_clusters(&accepted, Some(&all), Some("h")),
            0
        );
        assert_eq!(
            uncommitted_accepted_clusters(&BTreeSet::new(), None, None),
            0
        );
    }

    #[test]
    fn counts_restate_the_solve_summary_and_group_contradictions() {
        let mut solved = artifact(vec![], vec![]);
        solved.summary.accepted_tier_a = 3;
        solved.summary.contradiction = 2;
        solved.contradictions = vec![
            crate::entity::session::SessionSolveContradiction {
                reason: "key_conflict".to_string(),
                surfaces: vec!["Apex".to_string()],
                obs_ids: Vec::new(),
                evidence_ids: Vec::new(),
            },
            crate::entity::session::SessionSolveContradiction {
                reason: "key_conflict".to_string(),
                surfaces: vec!["Other".to_string()],
                obs_ids: Vec::new(),
                evidence_ids: Vec::new(),
            },
        ];
        let counts = counts_from(&solved);
        assert_eq!(counts.accepted_tier_a, 3);
        assert_eq!(counts.contradictions, 2);
        assert_eq!(counts.contradictions_by_reason["key_conflict"], 2);
    }
}
