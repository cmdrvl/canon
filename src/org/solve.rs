//! Staged solver and reconciliation for `canon entity`.

use super::types::{
    AbstentionRecord, AnchorValue, CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_RUN_VERSION,
    CANON_ENTITY_SOLVE_VERSION, CannotLinkFact, EdgeRecord, EntityError, EntityErrorCode,
    EntityResult, EntityState, EntityStrategy, EscrowActionKind, EscrowActionRecord,
    EscrowPatchSummary, IncumbentMemory, InheritanceMode, InheritanceRecord, MergeWitness,
    ProjectedObservation, RegistryPatchSummary, SolveRunArtifact, SolvedEntity, StrategyReference,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const NAME_NAMESPACE: &str = "name";

pub fn solve(
    strategy: &EntityStrategy,
    observations: &[ProjectedObservation],
    edges: &[EdgeRecord],
    incumbent: &IncumbentMemory,
) -> EntityResult<SolveRunArtifact> {
    solve_with_version(
        CANON_ENTITY_SOLVE_VERSION,
        strategy,
        observations,
        edges,
        incumbent,
    )
}

pub fn run(
    strategy: &EntityStrategy,
    observations: &[ProjectedObservation],
    edges: &[EdgeRecord],
    incumbent: &IncumbentMemory,
) -> EntityResult<SolveRunArtifact> {
    solve_with_version(
        CANON_ENTITY_RUN_VERSION,
        strategy,
        observations,
        edges,
        incumbent,
    )
}

fn solve_with_version(
    version: &str,
    strategy: &EntityStrategy,
    observations: &[ProjectedObservation],
    edges: &[EdgeRecord],
    incumbent: &IncumbentMemory,
) -> EntityResult<SolveRunArtifact> {
    let observation_index = build_observation_index(observations)?;
    let edge_index = build_edge_index(edges, &strategy.reference(), &incumbent.registry)?;
    let mut contradictions = Vec::new();
    let mut clusters = build_seed_clusters(
        strategy,
        &observation_index,
        &edge_index,
        &mut contradictions,
    )?;

    loop {
        let candidates =
            best_backbone_merge_candidates(strategy, &clusters, &observation_index, &edge_index);
        let Some(candidate) = candidates.into_iter().next() else {
            break;
        };
        merge_clusters(&mut clusters, candidate);
    }

    loop {
        let candidates =
            attachment_candidates(strategy, &clusters, &observation_index, &edge_index);
        let Some(candidate) = candidates.into_iter().next() else {
            break;
        };
        attach_cluster(&mut clusters, candidate);
    }

    let incumbent_aliases = incumbent_alias_index(incumbent);
    let incumbent_anchor_index = incumbent_anchor_index(incumbent);

    let mut entities = Vec::new();
    let mut abstentions = Vec::new();
    let mut proposed_registry_patch = RegistryPatchSummary::default();
    let mut proposed_escrow_patch = EscrowPatchSummary::default();

    for cluster in clusters.into_iter().flatten() {
        let cluster_rows = cluster.all_rows();
        let aliases = collect_cluster_aliases(&cluster_rows, &observation_index);
        let backbone_aliases = collect_cluster_aliases(&cluster.backbone_rows, &observation_index);
        let anchors = collect_cluster_anchors(&cluster_rows, &observation_index);
        let doc_ids = collect_cluster_doc_ids(&cluster_rows, &observation_index);
        let incumbent_ids = cluster_incumbent_ids(&aliases, &incumbent_aliases);
        let unique_high_trust_anchor = unique_high_trust_anchor(strategy, &anchors);

        if incumbent_ids.len() > 1 {
            proposed_escrow_patch.cannot_link_entries += 1;
            abstentions.push(AbstentionRecord {
                state: EntityState::AbstainConflict,
                all_rows: cluster_rows.clone(),
                reason: "multiple_incumbent_overlap".to_string(),
                incumbent_ids: incumbent_ids.clone(),
                escrow: Some(EscrowActionRecord {
                    action: EscrowActionKind::EmitCannotLink,
                    escrow_id: None,
                    cannot_link: Some(CannotLinkFact {
                        left_key: incumbent_ids[0].clone(),
                        right_key: incumbent_ids[1].clone(),
                        reason: "multiple_incumbent_overlap".to_string(),
                    }),
                }),
            });
            continue;
        }

        if let Some(incumbent_id) = incumbent_ids.first() {
            if let Some((namespace, cluster_value, incumbent_value)) =
                incumbent_anchor_conflict(strategy, incumbent_id, &anchors, &incumbent_anchor_index)
            {
                proposed_escrow_patch.cannot_link_entries += 1;
                abstentions.push(AbstentionRecord {
                    state: EntityState::AbstainConflict,
                    all_rows: cluster_rows.clone(),
                    reason: "trusted_anchor_conflict".to_string(),
                    incumbent_ids: incumbent_ids.clone(),
                    escrow: Some(EscrowActionRecord {
                        action: EscrowActionKind::EmitCannotLink,
                        escrow_id: None,
                        cannot_link: Some(CannotLinkFact {
                            left_key: format!("{namespace}:{cluster_value}"),
                            right_key: format!("{namespace}:{incumbent_value}"),
                            reason: "trusted_anchor_conflict".to_string(),
                        }),
                    }),
                });
                continue;
            }

            let existing_aliases = incumbent_aliases_for_id(incumbent_id, &incumbent_aliases);
            let anchored_single_doc = unique_high_trust_anchor.is_some();
            let eligible_writeback_aliases = backbone_aliases
                .iter()
                .filter(|alias| !existing_aliases.contains(alias.as_str()))
                .filter(|_| doc_ids.len() > 1 || anchored_single_doc)
                .cloned()
                .collect::<Vec<_>>();

            proposed_registry_patch.existing_alias_entries +=
                eligible_writeback_aliases.len() as u64;
            entities.push(SolvedEntity {
                state: EntityState::ResolvedExisting,
                canonical_id: Some(incumbent_id.clone()),
                backbone_rows: cluster.backbone_rows.clone(),
                attached_rows: cluster.attached_rows.clone(),
                all_rows: cluster_rows,
                aliases,
                anchors,
                merge_witnesses: sorted_merge_witnesses(cluster.merge_witnesses.clone()),
                inheritance: InheritanceRecord {
                    mode: InheritanceMode::SingleIncumbentOverlap,
                    incumbent_ids: incumbent_ids.clone(),
                },
                eligible_writeback_aliases,
                escrow: None,
            });
            continue;
        }

        let has_backbone_relation = !cluster.merge_witnesses.is_empty();
        let promotable = is_promotable_new(
            strategy,
            doc_ids.len(),
            unique_high_trust_anchor.is_some(),
            has_backbone_relation,
        );

        if promotable {
            let canonical_id = mint_canonical_id(
                strategy,
                unique_high_trust_anchor.as_ref(),
                &backbone_aliases,
                &cluster.backbone_rows,
            );
            proposed_registry_patch.new_entity_entries += aliases.len() as u64;
            entities.push(SolvedEntity {
                state: EntityState::PromotableNew,
                canonical_id: Some(canonical_id),
                backbone_rows: cluster.backbone_rows.clone(),
                attached_rows: cluster.attached_rows.clone(),
                all_rows: cluster_rows,
                aliases: aliases.clone(),
                anchors,
                merge_witnesses: sorted_merge_witnesses(cluster.merge_witnesses.clone()),
                inheritance: InheritanceRecord {
                    mode: InheritanceMode::NoIncumbentOverlap,
                    incumbent_ids: Vec::new(),
                },
                eligible_writeback_aliases: aliases,
                escrow: None,
            });
            continue;
        }

        let pending_escrow = if low_evidence_cluster_is_escrow_eligible(
            strategy,
            &cluster,
            &cluster_rows,
            &anchors,
            &edge_index,
        ) {
            let escrow_id = pending_cluster_escrow_id(
                strategy,
                &cluster,
                &aliases,
                &anchors,
                &doc_ids,
                &incumbent.pending_clusters,
            );
            proposed_escrow_patch.pending_cluster_entries += 1;
            Some(EscrowActionRecord {
                action: EscrowActionKind::UpsertPending,
                escrow_id: Some(escrow_id),
                cannot_link: None,
            })
        } else {
            None
        };

        abstentions.push(AbstentionRecord {
            state: EntityState::AbstainLowEvidence,
            all_rows: cluster_rows,
            reason: low_evidence_reason(
                doc_ids.len(),
                has_backbone_relation,
                unique_high_trust_anchor.is_some(),
            ),
            incumbent_ids: Vec::new(),
            escrow: pending_escrow,
        });
    }

    entities.sort_by_key(entity_sort_key);
    abstentions.sort_by_key(abstention_sort_key);
    contradictions.sort_by_key(contradiction_sort_key);

    let summary = build_summary(
        observations.len() as u64,
        &entities,
        &abstentions,
        &contradictions,
    );

    Ok(SolveRunArtifact {
        version: version.to_string(),
        strategy: strategy.reference(),
        registry: incumbent.registry.clone(),
        summary,
        entities,
        abstentions,
        contradictions,
        proposed_registry_patch,
        proposed_escrow_patch,
    })
}

#[derive(Debug, Clone)]
struct Cluster {
    backbone_rows: Vec<String>,
    attached_rows: Vec<String>,
    merge_witnesses: Vec<MergeWitness>,
    backbone_formed: bool,
}

impl Cluster {
    fn all_rows(&self) -> Vec<String> {
        self.backbone_rows
            .iter()
            .chain(self.attached_rows.iter())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn representative(&self) -> &str {
        self.backbone_rows.first().map(String::as_str).unwrap_or("")
    }
}

#[derive(Debug, Clone)]
struct MergeCandidate {
    left_index: usize,
    right_index: usize,
    score_total: i64,
    witness: MergeWitness,
}

#[derive(Debug, Clone)]
struct AttachmentCandidate {
    source_index: usize,
    target_index: usize,
    score_total: i64,
    target_score: i64,
}

fn build_observation_index(
    observations: &[ProjectedObservation],
) -> EntityResult<BTreeMap<String, ProjectedObservation>> {
    let mut index = BTreeMap::new();

    for observation in observations {
        if index
            .insert(observation.source_row_id.clone(), observation.clone())
            .is_some()
        {
            return Err(artifact_error(
                "Duplicate source_row_id in projected observations",
                json!({ "source_row_id": observation.source_row_id }),
            ));
        }
    }

    Ok(index)
}

fn build_edge_index(
    edges: &[EdgeRecord],
    strategy_reference: &StrategyReference,
    registry_snapshot: &super::types::RegistrySnapshot,
) -> EntityResult<BTreeMap<(String, String), EdgeRecord>> {
    let mut index = BTreeMap::new();

    for edge in edges {
        if edge.version != CANON_ENTITY_EDGE_VERSION {
            return Err(artifact_error(
                "Edge artifact version does not match canon_entity_edge.v0",
                json!({
                    "expected": CANON_ENTITY_EDGE_VERSION,
                    "actual": edge.version,
                }),
            ));
        }
        if &edge.strategy != strategy_reference {
            return Err(artifact_error(
                "Edge artifact strategy does not match the active strategy",
                json!({
                    "expected": strategy_reference,
                    "actual": edge.strategy,
                }),
            ));
        }
        if &edge.registry_snapshot != registry_snapshot {
            return Err(artifact_error(
                "Edge artifact registry snapshot does not match the active registry snapshot",
                json!({
                    "expected": registry_snapshot,
                    "actual": edge.registry_snapshot,
                }),
            ));
        }

        let key = pair_key(&edge.left_row_id, &edge.right_row_id);
        if index.insert(key.clone(), edge.clone()).is_some() {
            return Err(artifact_error(
                "Duplicate edge artifact for the same row pair",
                json!({
                    "left_row_id": key.0,
                    "right_row_id": key.1,
                }),
            ));
        }
    }

    Ok(index)
}

fn build_seed_clusters(
    strategy: &EntityStrategy,
    observations: &BTreeMap<String, ProjectedObservation>,
    edges: &BTreeMap<(String, String), EdgeRecord>,
    contradictions: &mut Vec<super::types::ContradictionRecord>,
) -> EntityResult<Vec<Option<Cluster>>> {
    let row_ids = observations.keys().cloned().collect::<Vec<_>>();
    let mut row_positions = BTreeMap::new();
    for (index, row_id) in row_ids.iter().enumerate() {
        row_positions.insert(row_id.clone(), index);
    }

    let mut union_find = UnionFind::new(row_ids.len());
    for edge in edges.values() {
        if edge.has_must_link {
            let left = row_positions.get(&edge.left_row_id).ok_or_else(|| {
                artifact_error(
                    "Edge references an unknown left_row_id",
                    json!({ "left_row_id": edge.left_row_id }),
                )
            })?;
            let right = row_positions.get(&edge.right_row_id).ok_or_else(|| {
                artifact_error(
                    "Edge references an unknown right_row_id",
                    json!({ "right_row_id": edge.right_row_id }),
                )
            })?;
            union_find.union(*left, *right);
        }
    }

    let mut groups = BTreeMap::<usize, Vec<String>>::new();
    for (index, row_id) in row_ids.iter().enumerate() {
        groups
            .entry(union_find.find(index))
            .or_default()
            .push(row_id.clone());
    }

    let mut clusters = Vec::new();
    for mut rows in groups.into_values() {
        rows.sort();

        if let Some(contradiction) = component_contradiction(strategy, &rows, observations, edges) {
            contradictions.push(contradiction);
            continue;
        }

        let merge_witnesses = seed_merge_witnesses(&rows, &row_positions, edges);
        clusters.push(Some(Cluster {
            backbone_rows: rows,
            attached_rows: Vec::new(),
            merge_witnesses,
            backbone_formed: false,
        }));
    }

    Ok(clusters)
}

fn component_contradiction(
    strategy: &EntityStrategy,
    rows: &[String],
    observations: &BTreeMap<String, ProjectedObservation>,
    edges: &BTreeMap<(String, String), EdgeRecord>,
) -> Option<super::types::ContradictionRecord> {
    if let Some((namespace, left, right)) =
        internal_trusted_anchor_conflict(strategy, rows, observations)
    {
        return Some(super::types::ContradictionRecord {
            reason: "trusted_anchor_conflict".to_string(),
            row_ids: rows.to_vec(),
            left_key: Some(format!("{namespace}:{left}")),
            right_key: Some(format!("{namespace}:{right}")),
        });
    }

    for left in 0..rows.len() {
        for right in left + 1..rows.len() {
            let key = pair_key(&rows[left], &rows[right]);
            let edge = edges.get(&key)?;
            if edge.has_cannot_link {
                return Some(super::types::ContradictionRecord {
                    reason: "seed_component_contains_cannot_link".to_string(),
                    row_ids: rows.to_vec(),
                    left_key: Some(key.0),
                    right_key: Some(key.1),
                });
            }
        }
    }

    None
}

fn seed_merge_witnesses(
    rows: &[String],
    row_positions: &BTreeMap<String, usize>,
    edges: &BTreeMap<(String, String), EdgeRecord>,
) -> Vec<MergeWitness> {
    if rows.len() <= 1 {
        return Vec::new();
    }

    let mut candidate_edges = rows
        .iter()
        .enumerate()
        .flat_map(|(left_index, left_row)| {
            rows.iter().skip(left_index + 1).filter_map(|right_row| {
                let edge = edges.get(&pair_key(left_row, right_row))?;
                edge.has_must_link.then(|| edge.clone())
            })
        })
        .collect::<Vec<_>>();
    candidate_edges.sort_by(|left, right| {
        (&left.left_row_id, &left.right_row_id).cmp(&(&right.left_row_id, &right.right_row_id))
    });

    let mut union_find = UnionFind::new(rows.len());
    let mut witnesses = Vec::new();
    for edge in candidate_edges {
        let left = row_positions[&edge.left_row_id];
        let right = row_positions[&edge.right_row_id];
        let local_left = rows
            .iter()
            .position(|row| row == &edge.left_row_id)
            .unwrap();
        let local_right = rows
            .iter()
            .position(|row| row == &edge.right_row_id)
            .unwrap();
        if union_find.find(local_left) != union_find.find(local_right) {
            union_find.union(local_left, local_right);
            witnesses.push(merge_witness_from_edge(&edge));
        } else if left < right {
            continue;
        }
    }

    sorted_merge_witnesses(witnesses)
}

fn best_backbone_merge_candidates(
    strategy: &EntityStrategy,
    clusters: &[Option<Cluster>],
    observations: &BTreeMap<String, ProjectedObservation>,
    edges: &BTreeMap<(String, String), EdgeRecord>,
) -> Vec<MergeCandidate> {
    let mut best_by_cluster = vec![None; clusters.len()];

    for left_index in 0..clusters.len() {
        let Some(left) = &clusters[left_index] else {
            continue;
        };

        for right_index in 0..clusters.len() {
            if left_index == right_index {
                continue;
            }
            let Some(right) = &clusters[right_index] else {
                continue;
            };

            let Some(witness_edge) =
                best_component_edge(&left.backbone_rows, &right.backbone_rows, edges)
            else {
                continue;
            };
            let score_name = edge_namespace_score(witness_edge, NAME_NAMESPACE);
            if score_name == 0 || witness_edge.pair_score_total < strategy.solver.backbone_score_min
            {
                continue;
            }
            if has_hard_negative(&left.backbone_rows, &right.backbone_rows, edges) {
                continue;
            }

            let merged = merged_cluster(left, right, merge_witness_from_edge(witness_edge));
            if !cluster_invariants_hold(strategy, &merged, observations) {
                continue;
            }

            let candidate = MergeCandidate {
                left_index,
                right_index,
                score_total: witness_edge.pair_score_total,
                witness: merge_witness_from_edge(witness_edge),
            };

            if is_better_merge_candidate(
                candidate.clone(),
                best_by_cluster[left_index].clone(),
                clusters,
            ) {
                best_by_cluster[left_index] = Some(candidate);
            }
        }
    }

    let mut reciprocal = Vec::new();
    for (index, candidate) in best_by_cluster.iter().enumerate() {
        let Some(candidate) = candidate else {
            continue;
        };
        if index > candidate.right_index {
            continue;
        }
        if best_by_cluster
            .get(candidate.right_index)
            .and_then(|other| other.as_ref())
            .map(|other| other.right_index == index)
            .unwrap_or(false)
        {
            reciprocal.push(candidate.clone());
        }
    }

    reciprocal.sort_by(|left, right| {
        right
            .score_total
            .cmp(&left.score_total)
            .then(candidate_sort_key(left, clusters).cmp(&candidate_sort_key(right, clusters)))
    });
    reciprocal
}

fn attachment_candidates(
    strategy: &EntityStrategy,
    clusters: &[Option<Cluster>],
    observations: &BTreeMap<String, ProjectedObservation>,
    edges: &BTreeMap<(String, String), EdgeRecord>,
) -> Vec<AttachmentCandidate> {
    let mut candidates = Vec::new();

    for source_index in 0..clusters.len() {
        let Some(source) = &clusters[source_index] else {
            continue;
        };
        if !source.attached_rows.is_empty() || source.backbone_formed {
            continue;
        }

        let mut best_target: Option<AttachmentCandidate> = None;
        let mut second_best_score = 0;

        for target_index in 0..clusters.len() {
            if source_index == target_index {
                continue;
            }
            let Some(target) = &clusters[target_index] else {
                continue;
            };

            let Some(edge) =
                best_component_edge(&source.backbone_rows, &target.backbone_rows, edges)
            else {
                continue;
            };
            let score_name = edge_namespace_score(edge, NAME_NAMESPACE);
            if score_name == 0 || edge.pair_score_total < strategy.solver.attach_score_min {
                continue;
            }
            if has_hard_negative(&source.all_rows(), &target.all_rows(), edges) {
                continue;
            }

            let attached = attached_cluster(target, source);
            if !cluster_invariants_hold(strategy, &attached, observations) {
                continue;
            }

            let candidate = AttachmentCandidate {
                source_index,
                target_index,
                score_total: edge.pair_score_total,
                target_score: edge.pair_score_total,
            };

            match best_target.as_ref() {
                Some(current)
                    if is_better_attachment(candidate.clone(), current.clone(), clusters) =>
                {
                    second_best_score = current.target_score;
                    best_target = Some(candidate);
                }
                Some(_) => {
                    second_best_score = second_best_score.max(edge.pair_score_total);
                }
                None => {
                    best_target = Some(candidate);
                }
            }
        }

        if let Some(candidate) = best_target
            && candidate.score_total - second_best_score >= strategy.solver.abstain_margin
        {
            candidates.push(candidate);
        }
    }

    candidates.sort_by(|left, right| {
        right
            .score_total
            .cmp(&left.score_total)
            .then(attachment_sort_key(left, clusters).cmp(&attachment_sort_key(right, clusters)))
    });
    candidates
}

fn merge_clusters(clusters: &mut [Option<Cluster>], candidate: MergeCandidate) {
    let left = clusters[candidate.left_index].take().unwrap();
    let right = clusters[candidate.right_index].take().unwrap();
    let merged = merged_cluster(&left, &right, candidate.witness);
    let keep_index = if left.representative() <= right.representative() {
        candidate.left_index
    } else {
        candidate.right_index
    };
    let remove_index = if keep_index == candidate.left_index {
        candidate.right_index
    } else {
        candidate.left_index
    };
    clusters[keep_index] = Some(merged);
    clusters[remove_index] = None;
}

fn attach_cluster(clusters: &mut [Option<Cluster>], candidate: AttachmentCandidate) {
    let source = clusters[candidate.source_index].take().unwrap();
    let target = clusters[candidate.target_index].take().unwrap();
    clusters[candidate.target_index] = Some(attached_cluster(&target, &source));
}

fn merged_cluster(left: &Cluster, right: &Cluster, witness: MergeWitness) -> Cluster {
    let mut backbone_rows = left
        .backbone_rows
        .iter()
        .chain(right.backbone_rows.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    backbone_rows.sort();

    let attached_rows = left
        .attached_rows
        .iter()
        .chain(right.attached_rows.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let merge_witnesses = sorted_merge_witnesses(
        left.merge_witnesses
            .iter()
            .chain(right.merge_witnesses.iter())
            .cloned()
            .chain(std::iter::once(witness))
            .collect(),
    );

    Cluster {
        backbone_rows,
        attached_rows,
        merge_witnesses,
        backbone_formed: true,
    }
}

fn attached_cluster(target: &Cluster, source: &Cluster) -> Cluster {
    let mut attached_rows = target
        .attached_rows
        .iter()
        .chain(source.backbone_rows.iter())
        .chain(source.attached_rows.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    attached_rows.sort();

    Cluster {
        backbone_rows: target.backbone_rows.clone(),
        attached_rows,
        merge_witnesses: target.merge_witnesses.clone(),
        backbone_formed: target.backbone_formed,
    }
}

fn cluster_invariants_hold(
    strategy: &EntityStrategy,
    cluster: &Cluster,
    observations: &BTreeMap<String, ProjectedObservation>,
) -> bool {
    internal_trusted_anchor_conflict(strategy, &cluster.all_rows(), observations).is_none()
        && backbone_diameter(cluster) <= strategy.solver.max_cluster_diameter
}

fn backbone_diameter(cluster: &Cluster) -> u32 {
    if cluster.backbone_rows.len() <= 1 {
        return 0;
    }

    let mut adjacency = BTreeMap::<String, Vec<String>>::new();
    for row in &cluster.backbone_rows {
        adjacency.entry(row.clone()).or_default();
    }
    for witness in &cluster.merge_witnesses {
        adjacency
            .entry(witness.left_row_id.clone())
            .or_default()
            .push(witness.right_row_id.clone());
        adjacency
            .entry(witness.right_row_id.clone())
            .or_default()
            .push(witness.left_row_id.clone());
    }

    let mut diameter = 0u32;
    for start in &cluster.backbone_rows {
        let mut queue = VecDeque::from([(start.clone(), 0u32)]);
        let mut seen = BTreeSet::from([start.clone()]);
        while let Some((node, distance)) = queue.pop_front() {
            diameter = diameter.max(distance);
            for neighbor in adjacency.get(&node).into_iter().flatten() {
                if seen.insert(neighbor.clone()) {
                    queue.push_back((neighbor.clone(), distance + 1));
                }
            }
        }
        if seen.len() != cluster.backbone_rows.len() {
            return u32::MAX;
        }
    }

    diameter
}

fn internal_trusted_anchor_conflict(
    strategy: &EntityStrategy,
    rows: &[String],
    observations: &BTreeMap<String, ProjectedObservation>,
) -> Option<(String, String, String)> {
    let trusted = strategy
        .anchors
        .trusted_for_must_link
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut values_by_namespace = BTreeMap::<String, BTreeSet<String>>::new();

    for row in rows {
        let observation = observations.get(row)?;
        for anchor in &observation.anchors {
            if trusted.contains(anchor.namespace.as_str()) {
                values_by_namespace
                    .entry(anchor.namespace.clone())
                    .or_default()
                    .insert(anchor.value.clone());
            }
        }
    }

    values_by_namespace
        .into_iter()
        .find_map(|(namespace, values)| {
            if values.len() > 1 {
                let mut values = values.into_iter().collect::<Vec<_>>();
                values.sort();
                Some((namespace, values[0].clone(), values[1].clone()))
            } else {
                None
            }
        })
}

fn best_component_edge<'a>(
    left_rows: &[String],
    right_rows: &[String],
    edges: &'a BTreeMap<(String, String), EdgeRecord>,
) -> Option<&'a EdgeRecord> {
    let mut best: Option<&EdgeRecord> = None;

    for left in left_rows {
        for right in right_rows {
            let Some(edge) = edges.get(&pair_key(left, right)) else {
                continue;
            };
            best = match best {
                Some(current)
                    if current.pair_score_total > edge.pair_score_total
                        || (current.pair_score_total == edge.pair_score_total
                            && (&current.left_row_id, &current.right_row_id)
                                <= (&edge.left_row_id, &edge.right_row_id)) =>
                {
                    Some(current)
                }
                _ => Some(edge),
            };
        }
    }

    best
}

fn has_hard_negative(
    left_rows: &[String],
    right_rows: &[String],
    edges: &BTreeMap<(String, String), EdgeRecord>,
) -> bool {
    left_rows.iter().any(|left| {
        right_rows.iter().any(|right| {
            edges
                .get(&pair_key(left, right))
                .map(|edge| edge.has_cannot_link)
                .unwrap_or(false)
        })
    })
}

fn edge_namespace_score(edge: &EdgeRecord, namespace: &str) -> i64 {
    edge.pair_score_by_namespace
        .get(namespace)
        .copied()
        .unwrap_or(0)
}

fn merge_witness_from_edge(edge: &EdgeRecord) -> MergeWitness {
    let mut operator_ids = edge
        .hits
        .iter()
        .filter(|hit| !matches!(hit.kind, super::types::EvidenceKind::CannotLink))
        .map(|hit| hit.operator_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    operator_ids.sort();

    MergeWitness {
        left_row_id: edge.left_row_id.clone(),
        right_row_id: edge.right_row_id.clone(),
        pair_score_total: edge.pair_score_total,
        pair_score_by_namespace: edge.pair_score_by_namespace.clone(),
        operator_ids,
    }
}

fn sorted_merge_witnesses(mut witnesses: Vec<MergeWitness>) -> Vec<MergeWitness> {
    witnesses.sort_by(|left, right| {
        (&left.left_row_id, &left.right_row_id).cmp(&(&right.left_row_id, &right.right_row_id))
    });
    witnesses
}

fn collect_cluster_aliases(
    rows: &[String],
    observations: &BTreeMap<String, ProjectedObservation>,
) -> Vec<String> {
    let mut aliases = BTreeSet::new();

    for row in rows {
        if let Some(observation) = observations.get(row) {
            aliases.insert(observation.primary_surface.value.clone());
            for alias in &observation.alias_surfaces {
                aliases.insert(alias.value.clone());
            }
        }
    }

    aliases.into_iter().collect()
}

fn collect_cluster_anchors(
    rows: &[String],
    observations: &BTreeMap<String, ProjectedObservation>,
) -> Vec<AnchorValue> {
    rows.iter()
        .filter_map(|row| observations.get(row))
        .flat_map(|observation| {
            observation
                .anchors
                .iter()
                .map(|anchor| (anchor.namespace.clone(), anchor.value.clone()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|(namespace, value)| AnchorValue { namespace, value })
        .collect()
}

fn collect_cluster_doc_ids(
    rows: &[String],
    observations: &BTreeMap<String, ProjectedObservation>,
) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|row| observations.get(row))
        .map(|observation| observation.doc_id.clone())
        .collect()
}

fn incumbent_alias_index(incumbent: &IncumbentMemory) -> BTreeMap<String, BTreeSet<String>> {
    let mut index = BTreeMap::<String, BTreeSet<String>>::new();
    for entry in &incumbent.alias_entries {
        index
            .entry(entry.canonical_id.clone())
            .or_default()
            .insert(entry.input.clone());
    }
    index
}

fn cluster_incumbent_ids(
    aliases: &[String],
    incumbent_aliases: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    incumbent_aliases
        .iter()
        .filter(|(_, known_aliases)| aliases.iter().any(|alias| known_aliases.contains(alias)))
        .map(|(canonical_id, _)| canonical_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn incumbent_aliases_for_id<'a>(
    canonical_id: &str,
    incumbent_aliases: &'a BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<&'a str> {
    incumbent_aliases
        .get(canonical_id)
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect()
}

fn incumbent_anchor_index(
    incumbent: &IncumbentMemory,
) -> BTreeMap<String, BTreeMap<String, BTreeSet<String>>> {
    let mut index = BTreeMap::<String, BTreeMap<String, BTreeSet<String>>>::new();
    for record in &incumbent.trusted_anchors {
        index
            .entry(record.canonical_id.clone())
            .or_default()
            .entry(record.namespace.clone())
            .or_default()
            .insert(record.value.clone());
    }
    index
}

fn incumbent_anchor_conflict(
    strategy: &EntityStrategy,
    incumbent_id: &str,
    anchors: &[AnchorValue],
    incumbent_anchor_index: &BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
) -> Option<(String, String, String)> {
    let incumbent_by_namespace = incumbent_anchor_index.get(incumbent_id)?;

    for namespace in &strategy.anchors.trusted_for_must_link {
        let cluster_values = anchors
            .iter()
            .filter(|anchor| anchor.namespace == *namespace)
            .map(|anchor| anchor.value.as_str())
            .collect::<BTreeSet<_>>();
        let incumbent_values = incumbent_by_namespace
            .get(namespace)
            .map(|values| values.iter().map(String::as_str).collect::<BTreeSet<_>>())
            .unwrap_or_default();

        if !cluster_values.is_empty()
            && !incumbent_values.is_empty()
            && cluster_values.is_disjoint(&incumbent_values)
        {
            return Some((
                namespace.clone(),
                cluster_values.iter().next()?.to_string(),
                incumbent_values.iter().next()?.to_string(),
            ));
        }
    }

    None
}

fn unique_high_trust_anchor(
    strategy: &EntityStrategy,
    anchors: &[AnchorValue],
) -> Option<AnchorValue> {
    for namespace in &strategy.anchors.precedence {
        if !strategy
            .anchors
            .trusted_for_single_doc_promotion
            .contains(namespace)
        {
            continue;
        }
        let values = anchors
            .iter()
            .filter(|anchor| anchor.namespace == *namespace)
            .map(|anchor| anchor.value.clone())
            .collect::<BTreeSet<_>>();
        if values.len() == 1 {
            return Some(AnchorValue {
                namespace: namespace.clone(),
                value: values.into_iter().next().unwrap(),
            });
        }
    }

    None
}

fn is_promotable_new(
    strategy: &EntityStrategy,
    distinct_doc_count: usize,
    has_unique_high_trust_anchor: bool,
    has_backbone_relation: bool,
) -> bool {
    has_unique_high_trust_anchor
        || (distinct_doc_count >= strategy.promotion.min_distinct_docs as usize
            && has_backbone_relation)
}

fn mint_canonical_id(
    strategy: &EntityStrategy,
    unique_high_trust_anchor: Option<&AnchorValue>,
    backbone_aliases: &[String],
    backbone_rows: &[String],
) -> String {
    let seed = if let Some(anchor) = unique_high_trust_anchor {
        format!("anchor|{}|{}", anchor.namespace, anchor.value)
    } else if !backbone_aliases.is_empty() {
        format!("aliases|{}", backbone_aliases.join("|"))
    } else {
        format!("rows|{}", backbone_rows.join("|"))
    };

    let digest = blake3::hash(seed.as_bytes()).to_hex().to_string();
    format!(
        "{}-{}",
        strategy.id_prefix.trim_end_matches('-'),
        &digest[..12]
    )
}

fn low_evidence_cluster_is_escrow_eligible(
    strategy: &EntityStrategy,
    cluster: &Cluster,
    rows: &[String],
    anchors: &[AnchorValue],
    edges: &BTreeMap<(String, String), EdgeRecord>,
) -> bool {
    cluster_has_positive_name_evidence(rows, edges)
        && internal_anchor_conflict_for_values(strategy, anchors).is_none()
        && (!cluster.merge_witnesses.is_empty()
            || unique_high_trust_anchor(strategy, anchors).is_some())
}

fn cluster_has_positive_name_evidence(
    rows: &[String],
    edges: &BTreeMap<(String, String), EdgeRecord>,
) -> bool {
    for left in 0..rows.len() {
        for right in left + 1..rows.len() {
            if edges
                .get(&pair_key(&rows[left], &rows[right]))
                .map(|edge| edge_namespace_score(edge, NAME_NAMESPACE) > 0)
                .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

fn internal_anchor_conflict_for_values(
    strategy: &EntityStrategy,
    anchors: &[AnchorValue],
) -> Option<(String, String, String)> {
    for namespace in &strategy.anchors.trusted_for_must_link {
        let values = anchors
            .iter()
            .filter(|anchor| anchor.namespace == *namespace)
            .map(|anchor| anchor.value.clone())
            .collect::<BTreeSet<_>>();
        if values.len() > 1 {
            let mut values = values.into_iter().collect::<Vec<_>>();
            values.sort();
            return Some((namespace.clone(), values[0].clone(), values[1].clone()));
        }
    }
    None
}

fn pending_cluster_escrow_id(
    strategy: &EntityStrategy,
    cluster: &Cluster,
    aliases: &[String],
    anchors: &[AnchorValue],
    doc_ids: &BTreeSet<String>,
    pending_clusters: &[super::types::PendingClusterRecord],
) -> String {
    let best_pending = pending_clusters
        .iter()
        .filter_map(|pending| {
            if pending_cluster_conflicts(strategy, anchors, pending) {
                return None;
            }
            let surface_overlap = pending
                .surfaces
                .iter()
                .filter(|surface| aliases.contains(surface))
                .count() as i64;
            let anchor_overlap = pending
                .anchors
                .iter()
                .filter(|anchor| {
                    anchors.iter().any(|candidate| {
                        candidate.namespace == anchor.namespace && candidate.value == anchor.value
                    })
                })
                .count() as i64;
            if surface_overlap == 0 {
                return None;
            }
            Some((
                pending.escrow_id.clone(),
                surface_overlap * 10 + anchor_overlap * 20,
            ))
        })
        .collect::<Vec<_>>();

    let mut ordered = best_pending;
    ordered.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    if let Some((escrow_id, best_score)) = ordered.first() {
        let second_best = ordered.get(1).map(|candidate| candidate.1).unwrap_or(0);
        if best_score - second_best >= strategy.solver.abstain_margin {
            return escrow_id.clone();
        }
    }

    let witness_seed = cluster
        .merge_witnesses
        .first()
        .map(|witness| format!("{}|{}", witness.left_row_id, witness.right_row_id))
        .unwrap_or_else(|| aliases.join("|"));
    let doc_seed = doc_ids.iter().next().cloned().unwrap_or_default();
    let digest = blake3::hash(format!("{witness_seed}|{doc_seed}").as_bytes())
        .to_hex()
        .to_string();
    format!("OE-{}", &digest[..12])
}

fn pending_cluster_conflicts(
    strategy: &EntityStrategy,
    anchors: &[AnchorValue],
    pending: &super::types::PendingClusterRecord,
) -> bool {
    for namespace in &strategy.anchors.trusted_for_must_link {
        let cluster_values = anchors
            .iter()
            .filter(|anchor| anchor.namespace == *namespace)
            .map(|anchor| anchor.value.as_str())
            .collect::<BTreeSet<_>>();
        let pending_values = pending
            .anchors
            .iter()
            .filter(|anchor| anchor.namespace == *namespace)
            .map(|anchor| anchor.value.as_str())
            .collect::<BTreeSet<_>>();

        if !cluster_values.is_empty()
            && !pending_values.is_empty()
            && cluster_values.is_disjoint(&pending_values)
        {
            return true;
        }
    }

    false
}

fn low_evidence_reason(
    distinct_doc_count: usize,
    has_backbone_relation: bool,
    has_unique_high_trust_anchor: bool,
) -> String {
    if distinct_doc_count <= 1 && !has_unique_high_trust_anchor {
        "insufficient_distinct_docs".to_string()
    } else if !has_backbone_relation && !has_unique_high_trust_anchor {
        "insufficient_backbone_evidence".to_string()
    } else {
        "insufficient_promotion_evidence".to_string()
    }
}

fn entity_sort_key(entity: &SolvedEntity) -> (u8, String, String) {
    (
        entity_state_rank(entity.state),
        entity.canonical_id.clone().unwrap_or_default(),
        entity.backbone_rows.first().cloned().unwrap_or_default(),
    )
}

fn abstention_sort_key(record: &AbstentionRecord) -> (u8, String) {
    (
        entity_state_rank(record.state),
        record.all_rows.first().cloned().unwrap_or_default(),
    )
}

fn contradiction_sort_key(record: &super::types::ContradictionRecord) -> (String, String) {
    (
        record.row_ids.first().cloned().unwrap_or_default(),
        record.reason.clone(),
    )
}

fn entity_state_rank(state: EntityState) -> u8 {
    match state {
        EntityState::ResolvedExisting => 0,
        EntityState::PromotableNew => 1,
        EntityState::AbstainLowEvidence => 2,
        EntityState::AbstainConflict => 3,
        EntityState::Contradiction => 4,
    }
}

fn build_summary(
    observation_count: u64,
    entities: &[SolvedEntity],
    abstentions: &[AbstentionRecord],
    contradictions: &[super::types::ContradictionRecord],
) -> super::types::SolveRunSummary {
    let resolved_existing = entities
        .iter()
        .filter(|entity| entity.state == EntityState::ResolvedExisting)
        .map(|entity| entity.all_rows.len() as u64)
        .sum();
    let promotable_new = entities
        .iter()
        .filter(|entity| entity.state == EntityState::PromotableNew)
        .map(|entity| entity.all_rows.len() as u64)
        .sum();
    let abstain_low_evidence = abstentions
        .iter()
        .filter(|record| record.state == EntityState::AbstainLowEvidence)
        .map(|record| record.all_rows.len() as u64)
        .sum();
    let mut abstain_conflict = abstentions
        .iter()
        .filter(|record| record.state == EntityState::AbstainConflict)
        .map(|record| record.all_rows.len() as u64)
        .sum::<u64>();
    abstain_conflict += contradictions
        .iter()
        .map(|record| record.row_ids.len() as u64)
        .sum::<u64>();

    super::types::SolveRunSummary {
        observations: observation_count,
        resolved_existing,
        promotable_new,
        abstain_low_evidence,
        abstain_conflict,
    }
}

fn pair_key(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn is_better_merge_candidate(
    candidate: MergeCandidate,
    current: Option<MergeCandidate>,
    clusters: &[Option<Cluster>],
) -> bool {
    match current {
        None => true,
        Some(current) => {
            candidate.score_total > current.score_total
                || (candidate.score_total == current.score_total
                    && candidate_sort_key(&candidate, clusters)
                        < candidate_sort_key(&current, clusters))
        }
    }
}

fn candidate_sort_key(
    candidate: &MergeCandidate,
    clusters: &[Option<Cluster>],
) -> (String, String, String, String) {
    (
        clusters[candidate.left_index]
            .as_ref()
            .map(|cluster| cluster.representative().to_string())
            .unwrap_or_default(),
        clusters[candidate.right_index]
            .as_ref()
            .map(|cluster| cluster.representative().to_string())
            .unwrap_or_default(),
        candidate.witness.left_row_id.clone(),
        candidate.witness.right_row_id.clone(),
    )
}

fn is_better_attachment(
    candidate: AttachmentCandidate,
    current: AttachmentCandidate,
    clusters: &[Option<Cluster>],
) -> bool {
    candidate.score_total > current.score_total
        || (candidate.score_total == current.score_total
            && attachment_sort_key(&candidate, clusters) < attachment_sort_key(&current, clusters))
}

fn attachment_sort_key(
    candidate: &AttachmentCandidate,
    clusters: &[Option<Cluster>],
) -> (String, String) {
    (
        clusters[candidate.source_index]
            .as_ref()
            .map(|cluster| cluster.representative().to_string())
            .unwrap_or_default(),
        clusters[candidate.target_index]
            .as_ref()
            .map(|cluster| cluster.representative().to_string())
            .unwrap_or_default(),
    )
}

fn artifact_error(message: impl Into<String>, detail: serde_json::Value) -> EntityError {
    EntityError::with_detail(EntityErrorCode::ArtifactContract, message, detail)
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parent[index] != index {
            let parent = self.parent[index];
            self.parent[index] = self.find(parent);
        }
        self.parent[index]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left_root = self.find(left);
        let right_root = self.find(right);
        if left_root == right_root {
            return;
        }

        match self.rank[left_root].cmp(&self.rank[right_root]) {
            std::cmp::Ordering::Less => self.parent[left_root] = right_root,
            std::cmp::Ordering::Greater => self.parent[right_root] = left_root,
            std::cmp::Ordering::Equal => {
                self.parent[right_root] = left_root;
                self.rank[left_root] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_runtime::types::{
        EdgeRecord, EvidenceHit, EvidenceKind, ProjectedAnchor, ProjectedSurface, StrategyAnchors,
        StrategyPromotion, StrategyReconcile, StrategySolver,
    };

    fn base_strategy() -> EntityStrategy {
        EntityStrategy {
            id: "bdc_org_graph.v1".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "issuer".to_string(),
            id_prefix: "IC".to_string(),
            content_hash: "blake3:strategy".to_string(),
            solver: StrategySolver {
                score_mode: "namespace_max_sum".to_string(),
                component_score_mode: "core_best_pair_sum".to_string(),
                merge_policy: "reciprocal_best".to_string(),
                backbone_score_min: 32,
                backbone_requires_positive_name: true,
                attach_score_min: 28,
                abstain_margin: 6,
                max_cluster_diameter: 2,
                require_positive_name_evidence: true,
                attach_requires_backbone_contact: true,
                score_against_backbone_only: true,
                attachments_do_not_chain: true,
            },
            reconcile: StrategyReconcile {
                single_incumbent_overlap: "inherit".to_string(),
                multi_incumbent_overlap: "abstain_conflict".to_string(),
                allow_incumbent_merge: false,
                allow_alias_writeback_for_resolved_existing: true,
            },
            anchors: StrategyAnchors {
                precedence: vec!["lei".to_string(), "cik".to_string(), "figi".to_string()],
                trusted_for_must_link: vec!["lei".to_string()],
                trusted_for_single_doc_promotion: vec!["lei".to_string()],
                support_only: vec!["cik".to_string(), "figi".to_string()],
                require_unique_for_attachment: true,
            },
            promotion: StrategyPromotion {
                write_states: vec![EntityState::ResolvedExisting, EntityState::PromotableNew],
                require_zero_anchor_conflicts: true,
                require_holdout_non_regression: true,
                require_perturbation_stability_gte: 0.995,
                min_distinct_docs: 2,
                allow_single_doc_if_unique_anchor: true,
            },
            ..EntityStrategy::default()
        }
    }

    fn registry() -> super::super::types::RegistrySnapshot {
        super::super::types::RegistrySnapshot {
            id: "bdc-issuers".to_string(),
            version: "2026.03.01".to_string(),
            source: "registries/bdc-issuers/".to_string(),
            lookup_snapshot_hash: "blake3:lookup".to_string(),
            escrow_snapshot_hash: "blake3:escrow".to_string(),
        }
    }

    fn observation(
        row: &str,
        doc: &str,
        primary: &str,
        aliases: &[&str],
        anchors: &[(&str, &str)],
    ) -> ProjectedObservation {
        ProjectedObservation {
            source_row_id: row.to_string(),
            doc_id: doc.to_string(),
            primary_surface: ProjectedSurface {
                value: primary.to_string(),
                field: "portfolio_company".to_string(),
            },
            alias_surfaces: aliases
                .iter()
                .map(|alias| ProjectedSurface {
                    value: (*alias).to_string(),
                    field: "alias".to_string(),
                })
                .collect(),
            anchors: anchors
                .iter()
                .map(|(namespace, value)| ProjectedAnchor {
                    namespace: (*namespace).to_string(),
                    value: (*value).to_string(),
                    field: (*namespace).to_string(),
                })
                .collect(),
            ..ProjectedObservation::default()
        }
    }

    fn edge(
        strategy: &EntityStrategy,
        left: &str,
        right: &str,
        hits: Vec<EvidenceHit>,
        pair_score_by_namespace: &[(&str, i64)],
        has_must_link: bool,
        has_cannot_link: bool,
    ) -> EdgeRecord {
        let pair_score_by_namespace = pair_score_by_namespace
            .iter()
            .map(|(namespace, score)| ((*namespace).to_string(), *score))
            .collect::<BTreeMap<_, _>>();
        EdgeRecord {
            version: CANON_ENTITY_EDGE_VERSION.to_string(),
            strategy: strategy.reference(),
            registry_snapshot: registry(),
            left_row_id: left.to_string(),
            right_row_id: right.to_string(),
            pair_score_total: pair_score_by_namespace.values().sum(),
            hits,
            pair_score_by_namespace,
            has_must_link,
            has_cannot_link,
        }
    }

    fn support_hit(operator_id: &str, score: i64) -> EvidenceHit {
        EvidenceHit {
            kind: EvidenceKind::Support,
            namespace: NAME_NAMESPACE.to_string(),
            operator_id: operator_id.to_string(),
            score,
            explanation: operator_id.to_string(),
        }
    }

    fn incumbent() -> IncumbentMemory {
        IncumbentMemory {
            registry: registry(),
            ..IncumbentMemory::default()
        }
    }

    #[test]
    fn backbone_merges_reciprocal_best_components() {
        let strategy = base_strategy();
        let observations = vec![
            observation("row-1", "doc-a", "Acme Corp.", &[], &[]),
            observation("row-2", "doc-b", "ACME Corporation", &[], &[]),
            observation("row-3", "doc-c", "Beta Holdings", &[], &[]),
        ];
        let edges = vec![
            edge(
                &strategy,
                "row-1",
                "row-2",
                vec![support_hit("exact_view:core_name", 32)],
                &[("name", 32)],
                false,
                false,
            ),
            edge(
                &strategy,
                "row-1",
                "row-3",
                vec![support_hit("exact_view:core_name", 10)],
                &[("name", 10)],
                false,
                false,
            ),
            edge(
                &strategy,
                "row-2",
                "row-3",
                vec![support_hit("exact_view:core_name", 10)],
                &[("name", 10)],
                false,
                false,
            ),
        ];

        let artifact = solve(&strategy, &observations, &edges, &incumbent()).expect("solve");

        assert_eq!(artifact.entities.len(), 1);
        assert_eq!(artifact.entities[0].state, EntityState::PromotableNew);
        assert_eq!(artifact.entities[0].backbone_rows, vec!["row-1", "row-2"]);
        assert_eq!(artifact.entities[0].merge_witnesses.len(), 1);
        assert_eq!(artifact.summary.promotable_new, 2);
    }

    #[test]
    fn attachments_require_winner_margin() {
        let strategy = base_strategy();
        let observations = vec![
            observation("row-1", "doc-a", "Acme Corp.", &[], &[]),
            observation("row-2", "doc-b", "ACME Corporation", &[], &[]),
            observation("row-3", "doc-c", "Acme Co.", &[], &[]),
            observation("row-4", "doc-d", "Acme Partners", &[], &[]),
        ];
        let edges = vec![
            edge(
                &strategy,
                "row-1",
                "row-2",
                vec![support_hit("exact_view:core_name", 32)],
                &[("name", 32)],
                false,
                false,
            ),
            edge(
                &strategy,
                "row-1",
                "row-3",
                vec![support_hit("exact_view:core_name", 30)],
                &[("name", 30)],
                false,
                false,
            ),
            edge(
                &strategy,
                "row-2",
                "row-3",
                vec![support_hit("exact_view:core_name", 29)],
                &[("name", 29)],
                false,
                false,
            ),
            edge(
                &strategy,
                "row-3",
                "row-4",
                vec![support_hit("exact_view:core_name", 20)],
                &[("name", 20)],
                false,
                false,
            ),
        ];

        let artifact = solve(&strategy, &observations, &edges, &incumbent()).expect("solve");

        assert_eq!(artifact.entities.len(), 1);
        assert_eq!(artifact.entities[0].attached_rows, vec!["row-3"]);
        assert_eq!(
            artifact.entities[0].all_rows,
            vec!["row-1", "row-2", "row-3"]
        );
        assert_eq!(artifact.summary.promotable_new, 3);
        assert_eq!(artifact.summary.abstain_low_evidence, 1);
    }

    #[test]
    fn single_incumbent_overlap_inherits_existing_id() {
        let strategy = base_strategy();
        let observations = vec![
            observation("row-1", "doc-a", "Acme Corp.", &[], &[]),
            observation("row-2", "doc-b", "ACME CO", &["Acme Company"], &[]),
        ];
        let edges = vec![edge(
            &strategy,
            "row-1",
            "row-2",
            vec![support_hit("exact_view:core_name", 32)],
            &[("name", 32)],
            false,
            false,
        )];
        let mut incumbent = incumbent();
        incumbent.alias_entries = vec![super::super::types::AliasMappingEntry {
            input: "Acme Corp.".to_string(),
            canonical_id: "IC-old".to_string(),
            canonical_type: "org_canon_id".to_string(),
            rule_id: "ORG_PROMOTION".to_string(),
        }];

        let artifact = solve(&strategy, &observations, &edges, &incumbent).expect("solve");

        assert_eq!(artifact.entities.len(), 1);
        assert_eq!(artifact.entities[0].state, EntityState::ResolvedExisting);
        assert_eq!(artifact.entities[0].canonical_id.as_deref(), Some("IC-old"));
        assert_eq!(
            artifact.entities[0].eligible_writeback_aliases,
            vec!["ACME CO", "Acme Company"]
        );
    }

    #[test]
    fn multiple_incumbent_overlap_abstains_and_emits_cannot_link() {
        let strategy = base_strategy();
        let observations = vec![
            observation("row-1", "doc-a", "Alias A", &[], &[]),
            observation("row-2", "doc-b", "Alias B", &[], &[]),
        ];
        let edges = vec![edge(
            &strategy,
            "row-1",
            "row-2",
            vec![support_hit("exact_view:core_name", 32)],
            &[("name", 32)],
            false,
            false,
        )];
        let mut incumbent = incumbent();
        incumbent.alias_entries = vec![
            super::super::types::AliasMappingEntry {
                input: "Alias A".to_string(),
                canonical_id: "IC-1".to_string(),
                canonical_type: "org_canon_id".to_string(),
                rule_id: "ORG_PROMOTION".to_string(),
            },
            super::super::types::AliasMappingEntry {
                input: "Alias B".to_string(),
                canonical_id: "IC-2".to_string(),
                canonical_type: "org_canon_id".to_string(),
                rule_id: "ORG_PROMOTION".to_string(),
            },
        ];

        let artifact = solve(&strategy, &observations, &edges, &incumbent).expect("solve");

        assert_eq!(artifact.abstentions.len(), 1);
        assert_eq!(artifact.abstentions[0].state, EntityState::AbstainConflict);
        assert_eq!(artifact.abstentions[0].incumbent_ids, vec!["IC-1", "IC-2"]);
        assert_eq!(
            artifact.abstentions[0].escrow.as_ref().unwrap().action,
            EscrowActionKind::EmitCannotLink
        );
    }

    #[test]
    fn low_evidence_cluster_creates_pending_escrow() {
        let strategy = base_strategy();
        let observations = vec![
            observation("row-1", "doc-a", "Acme Corp.", &[], &[]),
            observation("row-2", "doc-a", "ACME Corporation", &[], &[]),
        ];
        let edges = vec![edge(
            &strategy,
            "row-1",
            "row-2",
            vec![support_hit("exact_view:core_name", 32)],
            &[("name", 32)],
            false,
            false,
        )];
        let artifact = solve(&strategy, &observations, &edges, &incumbent()).expect("solve");

        assert_eq!(artifact.abstentions.len(), 1);
        assert_eq!(
            artifact.abstentions[0].state,
            EntityState::AbstainLowEvidence
        );
        assert_eq!(
            artifact.abstentions[0].escrow.as_ref().unwrap().action,
            EscrowActionKind::UpsertPending
        );
        assert!(
            artifact.abstentions[0]
                .escrow
                .as_ref()
                .unwrap()
                .escrow_id
                .as_deref()
                .unwrap()
                .starts_with("OE-")
        );
        assert_eq!(artifact.proposed_escrow_patch.pending_cluster_entries, 1);
    }
}
