#![forbid(unsafe_code)]

//! Sparse TF-IDF support evidence for `canon entity edge`.
//!
//! The edge stage consumes the compact namekit TF-IDF layout: sparse rows,
//! postings, and integer IDF/score units. It does not materialize dense vectors
//! or reinterpret TF-IDF scores as floating-point values.

use crate::{
    entity::{
        edge::EdgeEvidenceHit,
        score::{ScoreLane, ScoreUnits},
    },
    namekit::tfidf::{
        RARE_TOKEN_MIN_IDF_UNITS, SparseTfidfModel, TfidfEvidenceClass, TfidfRowTerm,
        TfidfSparseRow, TfidfTermId, TfidfTopKDiagnostics, TopKConfig,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TfidfCosineSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub model: &'a SparseTfidfModel,
    pub left_surface_id: &'a str,
    pub right_surface_id: &'a str,
    pub min_score_units: ScoreUnits,
    pub top_k: usize,
    pub candidate_cap: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfCosineSupportEvidence {
    pub hit: EdgeEvidenceHit,
    pub evidence_class: TfidfEvidenceClass,
    pub shared_term_count: u32,
    pub max_shared_idf_units: u32,
    pub top_contributors: Vec<TfidfSupportContributor>,
    pub top_k_diagnostics: TfidfTopKDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfSupportContributor {
    pub term_id: u32,
    pub term_key: String,
    pub idf_units: u32,
    pub left_weight_units: u64,
    pub right_weight_units: u64,
    pub contribution_units: u64,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfSparseArchitectureProof {
    pub version: String,
    pub document_count: u32,
    pub dictionary_term_count: usize,
    pub sparse_row_term_count: usize,
    pub posting_count: usize,
    pub dense_cell_count_if_materialized: u64,
    pub uses_dense_vectors: bool,
}

pub fn tfidf_cosine_support_hit(request: TfidfCosineSupportRequest<'_>) -> Option<EdgeEvidenceHit> {
    tfidf_cosine_support_evidence(request).map(|evidence| evidence.hit)
}

pub fn tfidf_cosine_support_evidence(
    request: TfidfCosineSupportRequest<'_>,
) -> Option<TfidfCosineSupportEvidence> {
    let config = top_k_config(request.top_k, request.candidate_cap);
    let top_k = request
        .model
        .top_k_for_surface(request.left_surface_id, config)?;
    let candidate = top_k
        .candidates
        .iter()
        .find(|candidate| candidate.surface_id == request.right_surface_id)?;
    let score_units = ScoreUnits::from_scaled(u32::from(candidate.score_units))?;
    if score_units == ScoreUnits::ZERO || score_units < request.min_score_units {
        return None;
    }

    let top_contributors = shared_contributors(
        request.model,
        request.left_surface_id,
        request.right_surface_id,
    )?;
    let reason_code = evidence_reason_code(candidate.evidence_class);
    let explanation = format!(
        "tfidf cosine score_units={} evidence_class={} shared_terms={} max_shared_idf_units={} top_contributors={}",
        score_units.as_u32(),
        evidence_class_id(candidate.evidence_class),
        candidate.shared_term_count,
        candidate.max_shared_idf_units,
        contributor_keys(&top_contributors),
    );

    Some(TfidfCosineSupportEvidence {
        hit: EdgeEvidenceHit::new(
            ScoreLane::Support,
            request.namespace,
            request.operator_id,
            reason_code,
            score_units,
            false,
            explanation,
        ),
        evidence_class: candidate.evidence_class,
        shared_term_count: candidate.shared_term_count,
        max_shared_idf_units: candidate.max_shared_idf_units,
        top_contributors,
        top_k_diagnostics: top_k.diagnostics,
    })
}

pub fn sparse_architecture_proof(model: &SparseTfidfModel) -> TfidfSparseArchitectureProof {
    let sparse_row_term_count = model.rows.iter().map(|row| row.terms.len()).sum::<usize>();
    let dense_cell_count_if_materialized =
        u64::from(model.document_count) * u64::try_from(model.terms.len()).unwrap_or(u64::MAX);

    TfidfSparseArchitectureProof {
        version: model.version.clone(),
        document_count: model.document_count,
        dictionary_term_count: model.terms.len(),
        sparse_row_term_count,
        posting_count: model.postings.len(),
        dense_cell_count_if_materialized,
        uses_dense_vectors: false,
    }
}

fn top_k_config(k: usize, candidate_cap: Option<usize>) -> TopKConfig {
    let config = TopKConfig::new(k);
    match candidate_cap {
        Some(cap) => config.with_candidate_cap(cap),
        None => config,
    }
}

fn shared_contributors(
    model: &SparseTfidfModel,
    left_surface_id: &str,
    right_surface_id: &str,
) -> Option<Vec<TfidfSupportContributor>> {
    let left = model.row(left_surface_id)?;
    let right = model.row(right_surface_id)?;
    let right_terms = terms_by_id(right);
    let mut contributors = left
        .terms
        .iter()
        .filter_map(|left_term| {
            let right_term = right_terms.get(&left_term.term_id)?;
            let term = model.term(left_term.term_id)?;
            let contribution_units =
                u128::from(left_term.weight_units) * u128::from(right_term.weight_units);
            Some(TfidfSupportContributor {
                term_id: term.id.as_u32(),
                term_key: term.key.key.clone(),
                idf_units: term.idf_units,
                left_weight_units: left_term.weight_units,
                right_weight_units: right_term.weight_units,
                contribution_units: u64::try_from(contribution_units).unwrap_or(u64::MAX),
                reason_code: contributor_reason_code(term.idf_units).to_string(),
            })
        })
        .collect::<Vec<_>>();
    contributors.sort_by(contributor_cmp);
    Some(contributors)
}

fn terms_by_id(row: &TfidfSparseRow) -> BTreeMap<TfidfTermId, &TfidfRowTerm> {
    row.terms.iter().map(|term| (term.term_id, term)).collect()
}

fn contributor_cmp(
    left: &TfidfSupportContributor,
    right: &TfidfSupportContributor,
) -> std::cmp::Ordering {
    right
        .contribution_units
        .cmp(&left.contribution_units)
        .then_with(|| right.idf_units.cmp(&left.idf_units))
        .then_with(|| left.term_key.as_bytes().cmp(right.term_key.as_bytes()))
        .then_with(|| left.term_id.cmp(&right.term_id))
}

fn contributor_reason_code(idf_units: u32) -> &'static str {
    if idf_units >= RARE_TOKEN_MIN_IDF_UNITS {
        "rare_token_support"
    } else {
        "common_token_downweighted"
    }
}

fn evidence_reason_code(evidence_class: TfidfEvidenceClass) -> &'static str {
    match evidence_class {
        TfidfEvidenceClass::RareTokenSupport => "rare_token_support",
        TfidfEvidenceClass::CommonTokenOnly => "common_token_downweighted",
        TfidfEvidenceClass::Diagnostic => "tfidf_diagnostic",
    }
}

fn evidence_class_id(evidence_class: TfidfEvidenceClass) -> &'static str {
    match evidence_class {
        TfidfEvidenceClass::RareTokenSupport => "rare_token_support",
        TfidfEvidenceClass::CommonTokenOnly => "common_token_only",
        TfidfEvidenceClass::Diagnostic => "diagnostic",
    }
}

fn contributor_keys(contributors: &[TfidfSupportContributor]) -> String {
    contributors
        .iter()
        .map(|contributor| contributor.term_key.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
