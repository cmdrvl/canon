//! Explain/proof-trace extraction for `canon entity`.

use super::{
    review::ReviewExportOutput,
    types::{
        AbstentionRecord, CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_EXPLAIN_VERSION,
        CANON_ENTITY_RUN_VERSION, CANON_ENTITY_SOLVE_VERSION, ContradictionRecord, EdgeRecord,
        EntityError, EntityErrorCode, EntityResult, EntityState, EvidenceKind, ExplainArtifact,
        ExplainCandidateRecord, ExplainEvidenceRecord, ExplainPromotionProvenanceRecord,
        ExplainQuery, ExplainResult, ExplainReviewDecisionRecord, ExplainSurfaceRecord,
        InheritanceMode, InheritanceRecord, MergeWitness, PendingClusterRecord, PromoteArtifact,
        SolveRunArtifact, SolvedEntity,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const NAME_NAMESPACE: &str = "name";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExplainReconstructionBundle {
    pub result: SolveRunArtifact,
    #[serde(default)]
    pub edges: Vec<EdgeRecord>,
    #[serde(default)]
    pub pending_clusters: Vec<PendingClusterRecord>,
    #[serde(default)]
    pub surfaces: Vec<ExplainSurfaceRecord>,
    #[serde(default)]
    pub review_exports: Vec<ReviewExportOutput>,
    #[serde(default)]
    pub review_decisions: Vec<ExplainReviewDecisionRecord>,
    #[serde(default)]
    pub promotion_artifacts: Vec<PromoteArtifact>,
    #[serde(default)]
    pub promotion_provenance: Vec<ExplainPromotionProvenanceRecord>,
}

pub fn explain(
    query: ExplainQuery,
    result: &SolveRunArtifact,
    edges: &[EdgeRecord],
    pending_clusters: &[PendingClusterRecord],
) -> EntityResult<ExplainArtifact> {
    explain_with_context(query, result, edges, pending_clusters, &[], &[], &[])
}

pub fn explain_from_result(
    query: ExplainQuery,
    result: &SolveRunArtifact,
) -> EntityResult<ExplainArtifact> {
    explain_with_context(query, result, &[], &[], &[], &[], &[])
}

pub fn explain_from_bundle(
    query: ExplainQuery,
    bundle: &ExplainReconstructionBundle,
) -> EntityResult<ExplainArtifact> {
    let mut review_decisions = bundle.review_decisions.clone();
    review_decisions.extend(review_decisions_from_exports(&bundle.review_exports));
    review_decisions.sort_by(explain_review_decision_cmp);
    review_decisions.dedup();

    let mut promotion_provenance = bundle.promotion_provenance.clone();
    promotion_provenance.extend(promotion_provenance_from_artifacts(
        &bundle.promotion_artifacts,
    ));
    promotion_provenance.sort_by(explain_promotion_provenance_cmp);
    promotion_provenance.dedup();

    explain_with_context(
        query,
        &bundle.result,
        &bundle.edges,
        &bundle.pending_clusters,
        &bundle.surfaces,
        &review_decisions,
        &promotion_provenance,
    )
}

pub fn explain_from_artifact_value(
    query: ExplainQuery,
    value: Value,
) -> EntityResult<ExplainArtifact> {
    if value.get("result").is_some() {
        let bundle =
            serde_json::from_value::<ExplainReconstructionBundle>(value).map_err(|error| {
                explain_error(
                    "Explain reconstruction bundle is malformed",
                    json!({ "error": error.to_string() }),
                )
            })?;
        explain_from_bundle(query, &bundle)
    } else {
        let result = serde_json::from_value::<SolveRunArtifact>(value).map_err(|error| {
            explain_error(
                "Explain result artifact is malformed",
                json!({ "error": error.to_string() }),
            )
        })?;
        explain_from_result(query, &result)
    }
}

fn explain_with_context(
    query: ExplainQuery,
    result: &SolveRunArtifact,
    edges: &[EdgeRecord],
    pending_clusters: &[PendingClusterRecord],
    surfaces: &[ExplainSurfaceRecord],
    review_decisions: &[ExplainReviewDecisionRecord],
    promotion_provenance: &[ExplainPromotionProvenanceRecord],
) -> EntityResult<ExplainArtifact> {
    validate_result_artifact(result)?;
    validate_query(&query)?;

    let edge_index = build_edge_index(edges, result)?;
    let pending_index = build_pending_index(pending_clusters)?;
    let surface_index = build_surface_index(surfaces)?;
    let row_surface_index = build_row_surface_index(surfaces);

    let mut explain_result = match resolve_query(&query, result, &surface_index)? {
        QueryMatch::Entity(entity) => build_entity_result(&query, entity, &edge_index),
        QueryMatch::Abstention(abstention) => {
            build_abstention_result(&query, abstention, &edge_index, &pending_index)
        }
        QueryMatch::Contradiction(contradiction) => {
            build_contradiction_result(contradiction, &edge_index)
        }
    };
    enrich_explain_result(
        &query,
        &mut explain_result,
        edges,
        surfaces,
        &row_surface_index,
        review_decisions,
        promotion_provenance,
    );

    Ok(ExplainArtifact {
        version: CANON_ENTITY_EXPLAIN_VERSION.to_string(),
        query,
        result: explain_result,
    })
}

enum QueryMatch<'a> {
    Entity(&'a SolvedEntity),
    Abstention(&'a AbstentionRecord),
    Contradiction(&'a ContradictionRecord),
}

fn validate_result_artifact(result: &SolveRunArtifact) -> EntityResult<()> {
    match result.version.as_str() {
        CANON_ENTITY_SOLVE_VERSION | CANON_ENTITY_RUN_VERSION => Ok(()),
        other => Err(explain_error(
            "Explain requires a canon_entity_solve.v0 or canon_entity_run.v0 artifact",
            json!({
                "expected": [CANON_ENTITY_SOLVE_VERSION, CANON_ENTITY_RUN_VERSION],
                "actual": other,
            }),
        )),
    }
}

fn validate_query(query: &ExplainQuery) -> EntityResult<()> {
    let selector_count = usize::from(query.row_id.is_some())
        + usize::from(query.surface_id.is_some())
        + usize::from(query.canonical_id.is_some())
        + usize::from(query.escrow_id.is_some());

    if selector_count == 1 {
        Ok(())
    } else {
        Err(explain_error(
            "Explain query must set exactly one of row_id, surface_id, canonical_id, or escrow_id",
            json!({
                "query": query,
            }),
        ))
    }
}

fn build_edge_index(
    edges: &[EdgeRecord],
    result: &SolveRunArtifact,
) -> EntityResult<BTreeMap<(String, String), EdgeRecord>> {
    let mut index = BTreeMap::new();

    for edge in edges {
        if edge.version != CANON_ENTITY_EDGE_VERSION {
            return Err(explain_error(
                "Edge artifact version does not match canon_entity_edge.v0",
                json!({
                    "expected": CANON_ENTITY_EDGE_VERSION,
                    "actual": edge.version,
                }),
            ));
        }
        if edge.strategy != result.strategy {
            return Err(explain_error(
                "Edge artifact strategy does not match result artifact strategy",
                json!({
                    "edge_strategy": edge.strategy,
                    "result_strategy": result.strategy,
                }),
            ));
        }
        if edge.registry_snapshot != result.registry {
            return Err(explain_error(
                "Edge artifact registry snapshot does not match result artifact registry",
                json!({
                    "edge_registry": edge.registry_snapshot,
                    "result_registry": result.registry,
                }),
            ));
        }

        let key = canonical_pair(&edge.left_row_id, &edge.right_row_id);
        if index.insert(key.clone(), edge.clone()).is_some() {
            return Err(explain_error(
                "Duplicate edge pair encountered while building explain index",
                json!({
                    "left_row_id": key.0,
                    "right_row_id": key.1,
                }),
            ));
        }
    }

    Ok(index)
}

fn build_pending_index(
    pending_clusters: &[PendingClusterRecord],
) -> EntityResult<BTreeMap<String, PendingClusterRecord>> {
    let mut index = BTreeMap::new();

    for pending in pending_clusters {
        if index
            .insert(pending.escrow_id.clone(), pending.clone())
            .is_some()
        {
            return Err(explain_error(
                "Duplicate escrow_id encountered while building explain index",
                json!({
                    "escrow_id": pending.escrow_id,
                }),
            ));
        }
    }

    Ok(index)
}

fn resolve_query<'a>(
    query: &ExplainQuery,
    result: &'a SolveRunArtifact,
    surface_index: &BTreeMap<String, ExplainSurfaceRecord>,
) -> EntityResult<QueryMatch<'a>> {
    let mut matches = Vec::new();

    if let Some(row_id) = query.row_id.as_deref() {
        matches.extend(
            result
                .entities
                .iter()
                .filter(|entity| entity.all_rows.iter().any(|candidate| candidate == row_id))
                .map(QueryMatch::Entity),
        );
        matches.extend(
            result
                .abstentions
                .iter()
                .filter(|abstention| {
                    abstention
                        .all_rows
                        .iter()
                        .any(|candidate| candidate == row_id)
                })
                .map(QueryMatch::Abstention),
        );
        matches.extend(
            result
                .contradictions
                .iter()
                .filter(|contradiction| {
                    contradiction
                        .row_ids
                        .iter()
                        .any(|candidate| candidate == row_id)
                })
                .map(QueryMatch::Contradiction),
        );
    }

    if let Some(surface_id) = query.surface_id.as_deref() {
        let surface = surface_index.get(surface_id).ok_or_else(|| {
            explain_error(
                "Explain surface_id did not match any reconstructed surface",
                json!({
                    "surface_id": surface_id,
                }),
            )
        })?;
        if surface.row_ids.is_empty() {
            return Err(explain_error(
                "Explain surface_id matched a surface with no row provenance",
                json!({
                    "surface_id": surface_id,
                }),
            ));
        }
        let row_ids = surface.row_ids.iter().collect::<BTreeSet<_>>();
        matches.extend(
            result
                .entities
                .iter()
                .filter(|entity| {
                    entity
                        .all_rows
                        .iter()
                        .any(|row_id| row_ids.contains(row_id))
                })
                .map(QueryMatch::Entity),
        );
        matches.extend(
            result
                .abstentions
                .iter()
                .filter(|abstention| {
                    abstention
                        .all_rows
                        .iter()
                        .any(|row_id| row_ids.contains(row_id))
                })
                .map(QueryMatch::Abstention),
        );
        matches.extend(
            result
                .contradictions
                .iter()
                .filter(|contradiction| {
                    contradiction
                        .row_ids
                        .iter()
                        .any(|row_id| row_ids.contains(row_id))
                })
                .map(QueryMatch::Contradiction),
        );
    }

    if let Some(canonical_id) = query.canonical_id.as_deref() {
        matches.extend(
            result
                .entities
                .iter()
                .filter(|entity| entity.canonical_id.as_deref() == Some(canonical_id))
                .map(QueryMatch::Entity),
        );
    }

    if let Some(escrow_id) = query.escrow_id.as_deref() {
        matches.extend(
            result
                .entities
                .iter()
                .filter(|entity| {
                    entity
                        .escrow
                        .as_ref()
                        .and_then(|escrow| escrow.escrow_id.as_deref())
                        == Some(escrow_id)
                })
                .map(QueryMatch::Entity),
        );
        matches.extend(
            result
                .abstentions
                .iter()
                .filter(|abstention| {
                    abstention
                        .escrow
                        .as_ref()
                        .and_then(|escrow| escrow.escrow_id.as_deref())
                        == Some(escrow_id)
                })
                .map(QueryMatch::Abstention),
        );
    }

    match matches.len() {
        0 => Err(explain_error(
            "Explain query did not match any entity, abstention, or contradiction",
            json!({
                "query": query,
            }),
        )),
        1 => Ok(matches.into_iter().next().expect("single match")),
        _ => Err(explain_error(
            "Explain query matched multiple records",
            json!({
                "query": query,
                "matches": matches.len(),
            }),
        )),
    }
}

fn build_entity_result(
    query: &ExplainQuery,
    entity: &SolvedEntity,
    edge_index: &BTreeMap<(String, String), EdgeRecord>,
) -> ExplainResult {
    let mut witness_chain = entity.merge_witnesses.clone();

    if let Some(row_id) = query.row_id.as_deref() {
        if entity
            .attached_rows
            .iter()
            .any(|candidate| candidate == row_id)
            && let Some(witness) =
                best_attachment_witness(row_id, &entity.backbone_rows, edge_index)
        {
            witness_chain.push(witness);
        }
    } else {
        for row_id in &entity.attached_rows {
            if let Some(witness) =
                best_attachment_witness(row_id, &entity.backbone_rows, edge_index)
            {
                witness_chain.push(witness);
            }
        }
    }

    ExplainResult {
        state: entity.state,
        canonical_id: entity.canonical_id.clone(),
        escrow_id: entity
            .escrow
            .as_ref()
            .and_then(|escrow| escrow.escrow_id.clone()),
        backbone_rows: sorted_rows(entity.backbone_rows.clone()),
        attached_rows: sorted_rows(entity.attached_rows.clone()),
        inheritance: entity.inheritance.clone(),
        witness_chain: sorted_unique_witnesses(witness_chain),
        ..ExplainResult::default()
    }
}

fn build_abstention_result(
    query: &ExplainQuery,
    abstention: &AbstentionRecord,
    edge_index: &BTreeMap<(String, String), EdgeRecord>,
    pending_index: &BTreeMap<String, PendingClusterRecord>,
) -> ExplainResult {
    let escrow_id = abstention
        .escrow
        .as_ref()
        .and_then(|escrow| escrow.escrow_id.clone());
    let pending = escrow_id
        .as_ref()
        .and_then(|escrow_id| pending_index.get(escrow_id));
    let backbone_rows = pending
        .map(|pending| pending_backbone_rows(pending, &abstention.all_rows))
        .unwrap_or_else(|| sorted_rows(abstention.all_rows.clone()));
    let backbone_row_set = backbone_rows.iter().cloned().collect::<BTreeSet<_>>();
    let attached_rows = abstention
        .all_rows
        .iter()
        .filter(|row_id| !backbone_row_set.contains(*row_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut witness_chain = pending
        .map(|pending| pending_witness_chain(pending, edge_index))
        .unwrap_or_default();

    if let Some(row_id) = query.row_id.as_deref() {
        if attached_rows.iter().any(|candidate| candidate == row_id)
            && let Some(witness) = best_attachment_witness(row_id, &backbone_rows, edge_index)
        {
            witness_chain.push(witness);
        }
    } else {
        for row_id in &attached_rows {
            if let Some(witness) = best_attachment_witness(row_id, &backbone_rows, edge_index) {
                witness_chain.push(witness);
            }
        }
    }

    ExplainResult {
        state: abstention.state,
        canonical_id: None,
        escrow_id,
        backbone_rows,
        attached_rows: sorted_rows(attached_rows),
        inheritance: abstention_inheritance(abstention),
        witness_chain: sorted_unique_witnesses(witness_chain),
        ..ExplainResult::default()
    }
}

fn build_contradiction_result(
    contradiction: &ContradictionRecord,
    edge_index: &BTreeMap<(String, String), EdgeRecord>,
) -> ExplainResult {
    let mut witness_chain = Vec::new();

    for (left_index, left_row_id) in contradiction.row_ids.iter().enumerate() {
        for right_row_id in contradiction.row_ids.iter().skip(left_index + 1) {
            let key = canonical_pair(left_row_id, right_row_id);
            if let Some(edge) = edge_index.get(&key)
                && edge.has_cannot_link
            {
                witness_chain.push(witness_from_edge(edge, true));
            }
        }
    }

    ExplainResult {
        state: EntityState::Contradiction,
        canonical_id: None,
        escrow_id: None,
        backbone_rows: sorted_rows(contradiction.row_ids.clone()),
        attached_rows: Vec::new(),
        inheritance: InheritanceRecord {
            mode: InheritanceMode::NoIncumbentOverlap,
            incumbent_ids: Vec::new(),
        },
        witness_chain: sorted_unique_witnesses(witness_chain),
        ..ExplainResult::default()
    }
}

fn build_surface_index(
    surfaces: &[ExplainSurfaceRecord],
) -> EntityResult<BTreeMap<String, ExplainSurfaceRecord>> {
    let mut index = BTreeMap::new();

    for surface in surfaces {
        if surface.surface_id.trim().is_empty() {
            return Err(explain_error(
                "Explain surface reconstruction contains an empty surface_id",
                json!({ "field": "surfaces.surface_id" }),
            ));
        }
        let mut normalized = surface.clone();
        normalized.row_ids = sorted_rows(normalized.row_ids);
        if normalized.row_count == 0 {
            normalized.row_count = normalized.row_ids.len() as u64;
        }
        if index
            .insert(normalized.surface_id.clone(), normalized)
            .is_some()
        {
            return Err(explain_error(
                "Duplicate surface_id encountered while building explain index",
                json!({ "surface_id": surface.surface_id }),
            ));
        }
    }

    Ok(index)
}

fn build_row_surface_index(surfaces: &[ExplainSurfaceRecord]) -> BTreeMap<String, Vec<String>> {
    let mut index = BTreeMap::<String, BTreeSet<String>>::new();

    for surface in surfaces {
        for row_id in &surface.row_ids {
            index
                .entry(row_id.clone())
                .or_default()
                .insert(surface.surface_id.clone());
        }
    }

    index
        .into_iter()
        .map(|(row_id, surface_ids)| (row_id, surface_ids.into_iter().collect()))
        .collect()
}

fn enrich_explain_result(
    query: &ExplainQuery,
    result: &mut ExplainResult,
    edges: &[EdgeRecord],
    surfaces: &[ExplainSurfaceRecord],
    row_surface_index: &BTreeMap<String, Vec<String>>,
    review_decisions: &[ExplainReviewDecisionRecord],
    promotion_provenance: &[ExplainPromotionProvenanceRecord],
) {
    let selected_rows = selected_result_rows(result, query);
    let mut selected_surface_ids = selected_surface_ids_for_rows(&selected_rows, row_surface_index);
    if let Some(surface_id) = &query.surface_id {
        selected_surface_ids.insert(surface_id.clone());
    }
    let provenance_surface_ids = selected_surface_ids.clone();

    let (candidates, positive_evidence, anti_merge_evidence) = explain_edges(
        edges,
        &selected_rows,
        &selected_surface_ids,
        row_surface_index,
    );
    for candidate in &candidates {
        selected_surface_ids.extend(candidate.left_surface_ids.iter().cloned());
        selected_surface_ids.extend(candidate.right_surface_ids.iter().cloned());
    }
    result.surfaces = explain_surfaces(surfaces, &selected_rows, &selected_surface_ids);
    for surface in &result.surfaces {
        selected_surface_ids.insert(surface.surface_id.clone());
    }
    result.candidates = candidates;
    result.positive_evidence = positive_evidence;
    result.anti_merge_evidence = anti_merge_evidence;
    result.review_decisions = explain_review_decisions(
        review_decisions,
        result,
        &selected_rows,
        &provenance_surface_ids,
        query,
    );
    result.promotion_provenance = explain_promotion_provenance(
        promotion_provenance,
        result,
        &selected_rows,
        &provenance_surface_ids,
        query,
    );
}

fn selected_result_rows(result: &ExplainResult, query: &ExplainQuery) -> BTreeSet<String> {
    let mut rows = result
        .backbone_rows
        .iter()
        .chain(result.attached_rows.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    if let Some(row_id) = &query.row_id {
        rows.insert(row_id.clone());
    }
    rows
}

fn selected_surface_ids_for_rows(
    rows: &BTreeSet<String>,
    row_surface_index: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|row_id| row_surface_index.get(row_id))
        .flat_map(|surface_ids| surface_ids.iter().cloned())
        .collect()
}

fn explain_surfaces(
    surfaces: &[ExplainSurfaceRecord],
    selected_rows: &BTreeSet<String>,
    selected_surface_ids: &BTreeSet<String>,
) -> Vec<ExplainSurfaceRecord> {
    let mut selected = surfaces
        .iter()
        .filter(|surface| {
            selected_surface_ids.contains(&surface.surface_id)
                || surface
                    .row_ids
                    .iter()
                    .any(|row_id| selected_rows.contains(row_id))
        })
        .cloned()
        .map(|mut surface| {
            surface.row_ids = sorted_rows(surface.row_ids);
            if surface.row_count == 0 {
                surface.row_count = surface.row_ids.len() as u64;
            }
            surface
        })
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));
    selected.dedup_by(|left, right| left.surface_id == right.surface_id);
    selected
}

fn explain_edges(
    edges: &[EdgeRecord],
    selected_rows: &BTreeSet<String>,
    selected_surface_ids: &BTreeSet<String>,
    row_surface_index: &BTreeMap<String, Vec<String>>,
) -> (
    Vec<ExplainCandidateRecord>,
    Vec<ExplainEvidenceRecord>,
    Vec<ExplainEvidenceRecord>,
) {
    let mut candidates = Vec::new();
    let mut positive_evidence = Vec::new();
    let mut anti_merge_evidence = Vec::new();

    for edge in edges {
        let left_surface_ids = row_surface_index
            .get(&edge.left_row_id)
            .cloned()
            .unwrap_or_default();
        let right_surface_ids = row_surface_index
            .get(&edge.right_row_id)
            .cloned()
            .unwrap_or_default();
        let touches_selected_row =
            selected_rows.contains(&edge.left_row_id) || selected_rows.contains(&edge.right_row_id);
        let touches_selected_surface = left_surface_ids
            .iter()
            .chain(right_surface_ids.iter())
            .any(|surface_id| selected_surface_ids.contains(surface_id));

        if !touches_selected_row && !touches_selected_surface {
            continue;
        }

        candidates.push(ExplainCandidateRecord {
            left_surface_ids: left_surface_ids.clone(),
            right_surface_ids: right_surface_ids.clone(),
            left_row_id: edge.left_row_id.clone(),
            right_row_id: edge.right_row_id.clone(),
            pair_score_total: edge.pair_score_total,
            pair_score_by_namespace: edge.pair_score_by_namespace.clone(),
            operator_ids: edge_operator_ids(edge),
        });

        for hit in &edge.hits {
            let record = ExplainEvidenceRecord {
                kind: hit.kind,
                namespace: hit.namespace.clone(),
                operator_id: hit.operator_id.clone(),
                score: hit.score,
                explanation: hit.explanation.clone(),
                left_surface_ids: left_surface_ids.clone(),
                right_surface_ids: right_surface_ids.clone(),
                left_row_id: edge.left_row_id.clone(),
                right_row_id: edge.right_row_id.clone(),
            };
            if matches!(hit.kind, EvidenceKind::CannotLink) {
                anti_merge_evidence.push(record);
            } else {
                positive_evidence.push(record);
            }
        }
    }

    candidates.sort_by(explain_candidate_cmp);
    positive_evidence.sort_by(explain_evidence_cmp);
    anti_merge_evidence.sort_by(explain_evidence_cmp);
    (candidates, positive_evidence, anti_merge_evidence)
}

fn edge_operator_ids(edge: &EdgeRecord) -> Vec<String> {
    edge.hits
        .iter()
        .map(|hit| hit.operator_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn explain_review_decisions(
    review_decisions: &[ExplainReviewDecisionRecord],
    result: &ExplainResult,
    selected_rows: &BTreeSet<String>,
    selected_surface_ids: &BTreeSet<String>,
    query: &ExplainQuery,
) -> Vec<ExplainReviewDecisionRecord> {
    let mut decisions = review_decisions
        .iter()
        .filter(|decision| {
            selector_matches_optional_ids(
                query,
                result,
                decision.canonical_id.as_deref(),
                decision.escrow_id.as_deref(),
            ) || decision
                .source_row_ids
                .iter()
                .any(|row_id| selected_rows.contains(row_id))
                || decision
                    .surface_ids
                    .iter()
                    .any(|surface_id| selected_surface_ids.contains(surface_id))
        })
        .cloned()
        .map(|mut decision| {
            decision.source_row_ids = sorted_rows(decision.source_row_ids);
            decision.surface_ids = sorted_rows(decision.surface_ids);
            decision
        })
        .collect::<Vec<_>>();
    decisions.sort_by(explain_review_decision_cmp);
    decisions.dedup();
    decisions
}

fn explain_promotion_provenance(
    promotion_provenance: &[ExplainPromotionProvenanceRecord],
    result: &ExplainResult,
    selected_rows: &BTreeSet<String>,
    selected_surface_ids: &BTreeSet<String>,
    query: &ExplainQuery,
) -> Vec<ExplainPromotionProvenanceRecord> {
    let mut provenance = promotion_provenance
        .iter()
        .filter(|record| {
            promotion_record_is_run_level(record)
                || selector_matches_optional_ids(
                    query,
                    result,
                    record.canonical_id.as_deref(),
                    record.escrow_id.as_deref(),
                )
                || record
                    .row_ids
                    .iter()
                    .any(|row_id| selected_rows.contains(row_id))
                || record
                    .surface_ids
                    .iter()
                    .any(|surface_id| selected_surface_ids.contains(surface_id))
        })
        .cloned()
        .map(|mut record| {
            record.row_ids = sorted_rows(record.row_ids);
            record.surface_ids = sorted_rows(record.surface_ids);
            record
        })
        .collect::<Vec<_>>();
    provenance.sort_by(explain_promotion_provenance_cmp);
    provenance.dedup();
    provenance
}

fn promotion_record_is_run_level(record: &ExplainPromotionProvenanceRecord) -> bool {
    record.canonical_id.is_none()
        && record.escrow_id.is_none()
        && record.row_ids.is_empty()
        && record.surface_ids.is_empty()
}

fn selector_matches_optional_ids(
    query: &ExplainQuery,
    result: &ExplainResult,
    canonical_id: Option<&str>,
    escrow_id: Option<&str>,
) -> bool {
    canonical_id.is_some()
        && (canonical_id == query.canonical_id.as_deref()
            || canonical_id == result.canonical_id.as_deref())
        || escrow_id.is_some()
            && (escrow_id == query.escrow_id.as_deref() || escrow_id == result.escrow_id.as_deref())
}

fn review_decisions_from_exports(
    review_exports: &[ReviewExportOutput],
) -> Vec<ExplainReviewDecisionRecord> {
    review_exports
        .iter()
        .flat_map(|export| {
            export.items.iter().map(|item| ExplainReviewDecisionRecord {
                review_id: item.review_id.clone(),
                category: item.category.clone(),
                state: item.state,
                canonical_id: item.canonical_id.clone(),
                escrow_id: None,
                source_row_ids: item.source_row_ids.clone(),
                surface_ids: Vec::new(),
                proposed_action: item.proposed_action.clone(),
                decision: item.decision.clone(),
            })
        })
        .collect()
}

fn promotion_provenance_from_artifacts(
    promotion_artifacts: &[PromoteArtifact],
) -> Vec<ExplainPromotionProvenanceRecord> {
    promotion_artifacts
        .iter()
        .map(|artifact| ExplainPromotionProvenanceRecord {
            artifact_version: Some(artifact.version.clone()),
            canonical_id: None,
            escrow_id: None,
            row_ids: Vec::new(),
            surface_ids: Vec::new(),
            decision: artifact.decision,
            writes: artifact.writes.clone(),
            registry_version_before: Some(artifact.registry.version_before.clone()),
            registry_version_after: Some(artifact.registry.version_after.clone()),
        })
        .collect()
}

fn pending_backbone_rows(pending: &PendingClusterRecord, all_rows: &[String]) -> Vec<String> {
    let all_row_set = all_rows.iter().cloned().collect::<BTreeSet<_>>();
    let mut backbone_rows = pending
        .witness_pairs
        .iter()
        .flat_map(|pair| [pair.left_row_id.clone(), pair.right_row_id.clone()])
        .filter(|row_id| all_row_set.contains(row_id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    if backbone_rows.is_empty() {
        backbone_rows = sorted_rows(all_rows.to_vec());
    }

    backbone_rows
}

fn pending_witness_chain(
    pending: &PendingClusterRecord,
    edge_index: &BTreeMap<(String, String), EdgeRecord>,
) -> Vec<MergeWitness> {
    pending
        .witness_pairs
        .iter()
        .map(|pair| {
            let key = canonical_pair(&pair.left_row_id, &pair.right_row_id);
            edge_index
                .get(&key)
                .map(|edge| witness_from_edge(edge, false))
                .unwrap_or_else(|| empty_witness(&pair.left_row_id, &pair.right_row_id))
        })
        .collect()
}

fn abstention_inheritance(abstention: &AbstentionRecord) -> InheritanceRecord {
    let mode = match abstention.reason.as_str() {
        "multiple_incumbent_overlap" => InheritanceMode::MultiIncumbentOverlap,
        "trusted_anchor_conflict" => InheritanceMode::TrustedAnchorConflict,
        _ if abstention.incumbent_ids.len() == 1 => InheritanceMode::SingleIncumbentOverlap,
        _ => InheritanceMode::NoIncumbentOverlap,
    };

    InheritanceRecord {
        mode,
        incumbent_ids: abstention.incumbent_ids.clone(),
    }
}

fn best_attachment_witness(
    row_id: &str,
    backbone_rows: &[String],
    edge_index: &BTreeMap<(String, String), EdgeRecord>,
) -> Option<MergeWitness> {
    let mut best: Option<MergeWitness> = None;

    for backbone_row_id in backbone_rows {
        if backbone_row_id == row_id {
            continue;
        }

        let key = canonical_pair(row_id, backbone_row_id);
        let Some(edge) = edge_index.get(&key) else {
            continue;
        };
        if edge.has_cannot_link
            || edge
                .pair_score_by_namespace
                .get(NAME_NAMESPACE)
                .copied()
                .unwrap_or(0)
                == 0
        {
            continue;
        }

        let witness = witness_from_edge(edge, false);
        match &best {
            Some(current) if !witness_is_better(&witness, current) => {}
            _ => best = Some(witness),
        }
    }

    best
}

fn witness_from_edge(edge: &EdgeRecord, include_cannot_link: bool) -> MergeWitness {
    let mut operator_ids = edge
        .hits
        .iter()
        .filter(|hit| include_cannot_link || !matches!(hit.kind, EvidenceKind::CannotLink))
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

fn empty_witness(left_row_id: &str, right_row_id: &str) -> MergeWitness {
    let (left_row_id, right_row_id) = canonical_pair(left_row_id, right_row_id);
    MergeWitness {
        left_row_id,
        right_row_id,
        pair_score_total: 0,
        pair_score_by_namespace: BTreeMap::new(),
        operator_ids: Vec::new(),
    }
}

fn sorted_unique_witnesses(witnesses: Vec<MergeWitness>) -> Vec<MergeWitness> {
    let mut deduped = BTreeMap::<(String, String), MergeWitness>::new();

    for witness in witnesses {
        let key = canonical_pair(&witness.left_row_id, &witness.right_row_id);
        let normalized = MergeWitness {
            left_row_id: key.0.clone(),
            right_row_id: key.1.clone(),
            pair_score_total: witness.pair_score_total,
            pair_score_by_namespace: witness.pair_score_by_namespace,
            operator_ids: witness.operator_ids,
        };

        match deduped.get(&key) {
            Some(current) if !witness_is_better(&normalized, current) => {}
            _ => {
                deduped.insert(key, normalized);
            }
        }
    }

    deduped.into_values().collect()
}

fn witness_is_better(candidate: &MergeWitness, current: &MergeWitness) -> bool {
    (
        candidate.pair_score_total,
        candidate.operator_ids.len(),
        &candidate.operator_ids,
    ) > (
        current.pair_score_total,
        current.operator_ids.len(),
        &current.operator_ids,
    )
}

fn sorted_rows(rows: Vec<String>) -> Vec<String> {
    rows.into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_pair(left_row_id: &str, right_row_id: &str) -> (String, String) {
    if left_row_id <= right_row_id {
        (left_row_id.to_string(), right_row_id.to_string())
    } else {
        (right_row_id.to_string(), left_row_id.to_string())
    }
}

fn explain_candidate_cmp(
    left: &ExplainCandidateRecord,
    right: &ExplainCandidateRecord,
) -> std::cmp::Ordering {
    (
        &left.left_surface_ids,
        &left.right_surface_ids,
        &left.left_row_id,
        &left.right_row_id,
        left.pair_score_total,
        &left.operator_ids,
    )
        .cmp(&(
            &right.left_surface_ids,
            &right.right_surface_ids,
            &right.left_row_id,
            &right.right_row_id,
            right.pair_score_total,
            &right.operator_ids,
        ))
}

fn explain_evidence_cmp(
    left: &ExplainEvidenceRecord,
    right: &ExplainEvidenceRecord,
) -> std::cmp::Ordering {
    (
        &left.left_surface_ids,
        &left.right_surface_ids,
        &left.left_row_id,
        &left.right_row_id,
        &left.namespace,
        &left.operator_id,
        left.score,
    )
        .cmp(&(
            &right.left_surface_ids,
            &right.right_surface_ids,
            &right.left_row_id,
            &right.right_row_id,
            &right.namespace,
            &right.operator_id,
            right.score,
        ))
}

fn explain_review_decision_cmp(
    left: &ExplainReviewDecisionRecord,
    right: &ExplainReviewDecisionRecord,
) -> std::cmp::Ordering {
    (
        &left.review_id,
        &left.category,
        &left.canonical_id,
        &left.escrow_id,
        &left.source_row_ids,
        &left.surface_ids,
        &left.decision,
    )
        .cmp(&(
            &right.review_id,
            &right.category,
            &right.canonical_id,
            &right.escrow_id,
            &right.source_row_ids,
            &right.surface_ids,
            &right.decision,
        ))
}

fn explain_promotion_provenance_cmp(
    left: &ExplainPromotionProvenanceRecord,
    right: &ExplainPromotionProvenanceRecord,
) -> std::cmp::Ordering {
    (
        &left.canonical_id,
        &left.escrow_id,
        &left.row_ids,
        &left.surface_ids,
        &left.registry_version_before,
        &left.registry_version_after,
    )
        .cmp(&(
            &right.canonical_id,
            &right.escrow_id,
            &right.row_ids,
            &right.surface_ids,
            &right.registry_version_before,
            &right.registry_version_after,
        ))
}

fn explain_error(message: impl Into<String>, detail: serde_json::Value) -> EntityError {
    EntityError::with_detail(EntityErrorCode::Explain, message, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_runtime::types::{
        CANON_ENTITY_EDGE_VERSION, EscrowActionKind, EscrowActionRecord, EvidenceHit,
        RegistrySnapshot, RowPair, StrategyReference,
    };

    #[test]
    fn explains_resolved_row_queries_from_entity_witnesses() {
        let result = solve_artifact(vec![resolved_entity()], Vec::new(), Vec::new());

        let artifact = explain(
            ExplainQuery {
                row_id: Some("row-9".to_string()),
                surface_id: None,
                canonical_id: None,
                escrow_id: None,
            },
            &result,
            &[],
            &[],
        )
        .expect("row query to resolve");

        assert_eq!(artifact.version, CANON_ENTITY_EXPLAIN_VERSION);
        assert_eq!(artifact.result.state, EntityState::ResolvedExisting);
        assert_eq!(
            artifact.result.canonical_id.as_deref(),
            Some("IC-123abc456def")
        );
        assert_eq!(artifact.result.backbone_rows, vec!["row-1", "row-9"]);
        assert!(artifact.result.attached_rows.is_empty());
        assert_eq!(artifact.result.witness_chain.len(), 1);
        assert_eq!(
            artifact.result.witness_chain[0].operator_ids,
            vec!["exact_view:core_name".to_string()]
        );
    }

    #[test]
    fn explains_canonical_id_queries_with_attachment_witnesses() {
        let mut entity = resolved_entity();
        entity.attached_rows = vec!["row-11".to_string()];
        entity.all_rows = vec![
            "row-1".to_string(),
            "row-9".to_string(),
            "row-11".to_string(),
        ];
        let result = solve_artifact(vec![entity], Vec::new(), Vec::new());
        let edges = vec![edge_record(
            "row-1",
            "row-11",
            21,
            false,
            vec![("name", "exact_view:alias", 21)],
        )];

        let artifact = explain(
            ExplainQuery {
                row_id: None,
                surface_id: None,
                canonical_id: Some("IC-123abc456def".to_string()),
                escrow_id: None,
            },
            &result,
            &edges,
            &[],
        )
        .expect("canonical_id query to resolve");

        assert_eq!(artifact.result.backbone_rows, vec!["row-1", "row-9"]);
        assert_eq!(artifact.result.attached_rows, vec!["row-11"]);
        assert_eq!(artifact.result.witness_chain.len(), 2);
        assert_eq!(
            artifact
                .result
                .witness_chain
                .iter()
                .map(|witness| (witness.left_row_id.as_str(), witness.right_row_id.as_str()))
                .collect::<Vec<_>>(),
            vec![("row-1", "row-11"), ("row-1", "row-9")]
        );
    }

    #[test]
    fn explains_escrow_id_queries_from_pending_cluster_witness_pairs() {
        let result = solve_artifact(
            Vec::new(),
            vec![AbstentionRecord {
                state: EntityState::AbstainLowEvidence,
                all_rows: vec!["row-41".to_string(), "row-58".to_string()],
                reason: "insufficient_distinct_docs".to_string(),
                incumbent_ids: Vec::new(),
                escrow: Some(EscrowActionRecord {
                    action: EscrowActionKind::UpsertPending,
                    escrow_id: Some("OE-8f9b7c1d2a3e".to_string()),
                    cannot_link: None,
                }),
            }],
            Vec::new(),
        );
        let edges = vec![edge_record(
            "row-41",
            "row-58",
            32,
            false,
            vec![
                ("name", "exact_view:core_name", 20),
                ("context", "categorical_equal:industry", 12),
            ],
        )];
        let pending_clusters = vec![PendingClusterRecord {
            escrow_id: "OE-8f9b7c1d2a3e".to_string(),
            profile: "bdc_issuer".to_string(),
            doc_ids: vec!["doc-1".to_string(), "doc-2".to_string()],
            surfaces: vec!["Acme Corp.".to_string()],
            anchors: Vec::new(),
            witness_pairs: vec![RowPair {
                left_row_id: "row-41".to_string(),
                right_row_id: "row-58".to_string(),
            }],
            state: "pending".to_string(),
        }];

        let artifact = explain(
            ExplainQuery {
                row_id: None,
                surface_id: None,
                canonical_id: None,
                escrow_id: Some("OE-8f9b7c1d2a3e".to_string()),
            },
            &result,
            &edges,
            &pending_clusters,
        )
        .expect("escrow_id query to resolve");

        assert_eq!(artifact.result.state, EntityState::AbstainLowEvidence);
        assert_eq!(
            artifact.result.escrow_id.as_deref(),
            Some("OE-8f9b7c1d2a3e")
        );
        assert_eq!(artifact.result.backbone_rows, vec!["row-41", "row-58"]);
        assert!(artifact.result.attached_rows.is_empty());
        assert_eq!(
            artifact.result.inheritance.mode,
            InheritanceMode::NoIncumbentOverlap
        );
        assert_eq!(artifact.result.witness_chain.len(), 1);
        assert_eq!(
            artifact.result.witness_chain[0].operator_ids,
            vec![
                "categorical_equal:industry".to_string(),
                "exact_view:core_name".to_string(),
            ]
        );
    }

    fn solve_artifact(
        entities: Vec<SolvedEntity>,
        abstentions: Vec<AbstentionRecord>,
        contradictions: Vec<ContradictionRecord>,
    ) -> SolveRunArtifact {
        SolveRunArtifact {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            strategy: strategy_reference(),
            registry: registry_snapshot(),
            entities,
            abstentions,
            contradictions,
            ..SolveRunArtifact::default()
        }
    }

    fn resolved_entity() -> SolvedEntity {
        SolvedEntity {
            state: EntityState::ResolvedExisting,
            canonical_id: Some("IC-123abc456def".to_string()),
            backbone_rows: vec!["row-1".to_string(), "row-9".to_string()],
            attached_rows: Vec::new(),
            all_rows: vec!["row-1".to_string(), "row-9".to_string()],
            merge_witnesses: vec![MergeWitness {
                left_row_id: "row-1".to_string(),
                right_row_id: "row-9".to_string(),
                pair_score_total: 32,
                pair_score_by_namespace: BTreeMap::from([("name".to_string(), 32)]),
                operator_ids: vec!["exact_view:core_name".to_string()],
            }],
            inheritance: InheritanceRecord {
                mode: InheritanceMode::SingleIncumbentOverlap,
                incumbent_ids: vec!["IC-123abc456def".to_string()],
            },
            ..SolvedEntity::default()
        }
    }

    fn edge_record(
        left_row_id: &str,
        right_row_id: &str,
        pair_score_total: i64,
        has_cannot_link: bool,
        hits: Vec<(&str, &str, i64)>,
    ) -> EdgeRecord {
        let pair_score_by_namespace = hits
            .iter()
            .filter(|(namespace, _, _)| *namespace != "cannot_link")
            .map(|(namespace, _, score)| ((*namespace).to_string(), *score))
            .collect::<BTreeMap<_, _>>();

        EdgeRecord {
            version: CANON_ENTITY_EDGE_VERSION.to_string(),
            strategy: strategy_reference(),
            registry_snapshot: registry_snapshot(),
            left_row_id: left_row_id.to_string(),
            right_row_id: right_row_id.to_string(),
            hits: hits
                .into_iter()
                .map(|(namespace, operator_id, score)| EvidenceHit {
                    kind: if namespace == "cannot_link" {
                        EvidenceKind::CannotLink
                    } else {
                        EvidenceKind::Support
                    },
                    namespace: namespace.to_string(),
                    operator_id: operator_id.to_string(),
                    score,
                    explanation: operator_id.to_string(),
                })
                .collect(),
            pair_score_by_namespace,
            pair_score_total,
            has_must_link: false,
            has_cannot_link,
        }
    }

    fn strategy_reference() -> StrategyReference {
        StrategyReference {
            id: "bdc_org_graph.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        }
    }

    fn registry_snapshot() -> RegistrySnapshot {
        RegistrySnapshot {
            id: "bdc-issuers".to_string(),
            version: "2026.03.01".to_string(),
            source: "registries/bdc-issuers/".to_string(),
            lookup_snapshot_hash: "blake3:lookup".to_string(),
            escrow_snapshot_hash: "blake3:escrow".to_string(),
        }
    }
}
