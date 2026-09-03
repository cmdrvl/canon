#![forbid(unsafe_code)]

//! Read-only two-record entity scoring.
//!
//! This module prepares exactly two JSON records in memory and delegates the
//! candidate evidence calculation to the same candidate-to-edge scorer used by
//! `canon entity run`.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1,
        block::{BlockCandidateHit, BlockCandidateRecord},
        edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
        error::EntityRefusalKind,
        prepare::{
            assign_exact_lookups, load_prepare_profile_with_hash, load_prepare_registry_snapshot,
            prepare_contract_for_loaded_profile, prepare_surface_records,
            project_prepare_jsonl_reader,
        },
        review_export::{
            NativeEvidenceWaterfall, NativeEvidenceWaterfallContribution,
            NativeEvidenceWaterfallThresholdLine,
        },
        run::score_edge_candidate_for_prepared_surfaces,
        score::{ENTITY_SCORE_SCALE, ScoreLane, ScoreUnits},
    },
    witness,
};
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    fs,
    io::{BufReader, Cursor},
    path::Path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScorePairVerdict {
    CannotLink,
    WouldMerge,
    WouldAttach,
    WouldEscrow,
    BelowFloor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScorePairThresholds {
    pub backbone_score_min: u32,
    pub attach_score_min: u32,
    pub abstain_margin: u32,
}

impl Default for ScorePairThresholds {
    fn default() -> Self {
        Self {
            backbone_score_min: ENTITY_SCORE_SCALE,
            attach_score_min: ENTITY_SCORE_SCALE,
            abstain_margin: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScorePairEvaluation {
    pub profile_id: String,
    pub profile_version: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: Option<String>,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: u32,
    pub verdict: ScorePairVerdict,
    pub thresholds: ScorePairThresholds,
    pub evidence_record: EdgeEvidenceRecord,
    pub evidence_waterfall: NativeEvidenceWaterfall,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScorePairRequest<'a> {
    pub left: &'a Value,
    pub right: &'a Value,
    pub profile: &'a str,
    pub strategy: &'a Path,
    pub registry: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScorePairStrategy {
    content_hash: String,
    thresholds: ScorePairThresholds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WaterfallSource {
    lane: String,
    operator: String,
    view_field: String,
    reason_codes: Vec<String>,
    source_score_units: u32,
}

pub(crate) fn score_pair(request: ScorePairRequest<'_>) -> Result<ScorePairEvaluation, Refusal> {
    let strategy = load_score_pair_strategy(request.strategy)?;
    let loaded_profile = load_prepare_profile_with_hash(request.profile)?;
    if loaded_profile.package.is_some() {
        return Err(score_pair_refusal(
            EntityRefusalKind::Profile,
            "Entity score-pair requires a built-in or YAML prepare profile",
            json!({
                "profile": request.profile,
                "reason": "profile_package_requires_row_source"
            }),
            Some("Run canon entity run with an explicit work directory for profile-package prepare execution".to_string()),
        ));
    }
    let contract = prepare_contract_for_loaded_profile(&loaded_profile)?;
    let observations = prepare_pair_observations(request.left, request.right, &contract)?;
    let mut surfaces = prepare_surface_records(&observations)?;
    let registry_snapshot_hash = if let Some(registry) = request.registry {
        let registry_snapshot = load_prepare_registry_snapshot(registry)?;
        let snapshot_hash = registry_snapshot.lookup_snapshot_hash.clone();
        assign_exact_lookups(&mut surfaces, registry, &registry_snapshot)?;
        Some(snapshot_hash)
    } else {
        None
    };
    surfaces.sort_by(|left, right| {
        left.surface_id
            .cmp(&right.surface_id)
            .then_with(|| left.surface_key.cmp(&right.surface_key))
    });
    if surfaces.len() != 2 {
        return Err(score_pair_refusal(
            EntityRefusalKind::InputContract,
            "Entity score-pair requires two distinct prepared surfaces",
            json!({
                "prepared_surface_count": surfaces.len(),
                "prepared_surface_ids": surfaces.iter().map(|surface| surface.surface_id.clone()).collect::<Vec<_>>()
            }),
            Some("Use two records whose profile-normalized canonical surfaces are distinct, or treat identical prepared surfaces as already coalesced".to_string()),
        ));
    }

    let candidate = BlockCandidateRecord {
        version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
        left_surface_id: surfaces[0].surface_id.clone(),
        right_surface_id: surfaces[1].surface_id.clone(),
        block_hits: vec![BlockCandidateHit {
            operator_id: "score_pair:direct_pair".to_string(),
            rank: Some(1),
            score_units: 0,
        }],
        candidate_score_hint: 0,
    };
    let profile = &loaded_profile.document;
    let mut evidence_record = score_edge_candidate_for_prepared_surfaces(
        profile,
        &profile.patch_namespaces.aliases,
        &profile.patch_namespaces.relations,
        &surfaces,
        &candidate,
    )?;
    evidence_record.version = CANON_ENTITY_EVIDENCE_VERSION_V1.to_string();
    evidence_record = apply_registry_pair_evidence(
        evidence_record,
        &profile.patch_namespaces.aliases,
        &surfaces[0],
        &surfaces[1],
    )?;

    let score_units = evidence_record.pair_score_total.as_u32();
    let verdict = verdict_for_record(&evidence_record, strategy.thresholds);
    let evidence_waterfall = evidence_waterfall_for_record(&evidence_record, strategy.thresholds);

    Ok(ScorePairEvaluation {
        profile_id: profile.profile.clone(),
        profile_version: profile.version.clone(),
        strategy_hash: strategy.content_hash,
        registry_snapshot_hash,
        left_surface_id: evidence_record.left_surface_id.clone(),
        right_surface_id: evidence_record.right_surface_id.clone(),
        score_units,
        verdict,
        thresholds: strategy.thresholds,
        evidence_record,
        evidence_waterfall,
    })
}

fn prepare_pair_observations(
    left: &Value,
    right: &Value,
    contract: &crate::entity::prepare::PrepareInputContract,
) -> Result<Vec<crate::entity::prepare::PreparedInputObservation>, Refusal> {
    let mut bytes = Vec::new();
    serde_json::to_writer(&mut bytes, left).map_err(score_pair_json_refusal)?;
    bytes.push(b'\n');
    serde_json::to_writer(&mut bytes, right).map_err(score_pair_json_refusal)?;
    bytes.push(b'\n');
    project_prepare_jsonl_reader(BufReader::new(Cursor::new(bytes)), contract)
}

fn apply_registry_pair_evidence(
    record: EdgeEvidenceRecord,
    namespace: &str,
    left: &crate::entity::prepare::PreparedSurfaceRecord,
    right: &crate::entity::prepare::PreparedSurfaceRecord,
) -> Result<EdgeEvidenceRecord, Refusal> {
    let Some(hit) = registry_pair_hit(namespace, left, right) else {
        return Ok(record);
    };
    let mut hits = record
        .hits
        .into_iter()
        .filter(|hit| !is_fallback_relation_hint(hit))
        .collect::<Vec<_>>();
    hits.push(hit);
    let mut rebuilt =
        build_edge_evidence_record(record.left_surface_id, record.right_surface_id, hits)?;
    rebuilt.version = CANON_ENTITY_EVIDENCE_VERSION_V1.to_string();
    Ok(rebuilt)
}

fn registry_pair_hit(
    namespace: &str,
    left: &crate::entity::prepare::PreparedSurfaceRecord,
    right: &crate::entity::prepare::PreparedSurfaceRecord,
) -> Option<EdgeEvidenceHit> {
    let left_lookup = resolved_lookup(left)?;
    let right_lookup = resolved_lookup(right)?;
    if left_lookup.0 == right_lookup.0 && left_lookup.1 == right_lookup.1 {
        return Some(EdgeEvidenceHit::new(
            ScoreLane::Support,
            namespace,
            "registry_alias_match",
            "registry_alias_support",
            ScoreUnits::MAX,
            false,
            format!(
                "registry alias support canonical_id={} canonical_type={} left_matched_input={} right_matched_input={}",
                left_lookup.0, left_lookup.1, left_lookup.2, right_lookup.2
            ),
        ));
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        namespace,
        "registry_alias_conflict",
        "registry_alias_cannot_link",
        ScoreUnits::MAX,
        true,
        format!(
            "registry alias conflict left_canonical_id={} left_canonical_type={} right_canonical_id={} right_canonical_type={}",
            left_lookup.0, left_lookup.1, right_lookup.0, right_lookup.1
        ),
    ))
}

fn resolved_lookup(
    surface: &crate::entity::prepare::PreparedSurfaceRecord,
) -> Option<(&str, &str, &str)> {
    if surface.exact_lookup.status != crate::entity::prepare::PreparedExactLookupStatus::Resolved {
        return None;
    }
    Some((
        surface.exact_lookup.canonical_id.as_deref()?,
        surface.exact_lookup.canonical_type.as_deref()?,
        surface
            .exact_lookup
            .matched_input
            .as_deref()
            .unwrap_or("unknown"),
    ))
}

fn is_fallback_relation_hint(hit: &EdgeEvidenceHit) -> bool {
    hit.lane == ScoreLane::RelationHint
        && hit.operator_id == "run_candidate_review"
        && hit.reason_code == "candidate_requires_review"
}

fn verdict_for_record(
    record: &EdgeEvidenceRecord,
    thresholds: ScorePairThresholds,
) -> ScorePairVerdict {
    if record.has_hard_cannot_link {
        return ScorePairVerdict::CannotLink;
    }
    let score = record.pair_score_total.as_u32();
    if score >= thresholds.backbone_score_min {
        ScorePairVerdict::WouldMerge
    } else if score >= thresholds.attach_score_min {
        ScorePairVerdict::WouldAttach
    } else if score.saturating_add(thresholds.abstain_margin) >= thresholds.attach_score_min {
        ScorePairVerdict::WouldEscrow
    } else {
        ScorePairVerdict::BelowFloor
    }
}

fn evidence_waterfall_for_record(
    record: &EdgeEvidenceRecord,
    thresholds: ScorePairThresholds,
) -> NativeEvidenceWaterfall {
    let evidence_ref_id = pair_evidence_ref_id(record);
    let mut sources = record
        .hits
        .iter()
        .filter(|hit| hit.lane != ScoreLane::RelationHint)
        .map(|hit| WaterfallSource {
            lane: score_lane_string(hit.lane),
            operator: hit.operator_id.clone(),
            view_field: hit.namespace.clone(),
            reason_codes: vec![hit.reason_code.clone()],
            source_score_units: hit.score_units.as_u32(),
        })
        .collect::<Vec<_>>();
    sources.sort_by(waterfall_source_cmp);

    let mut remaining_support_units = u64::from(ENTITY_SCORE_SCALE);
    let mut contributions = Vec::new();
    for source in sources {
        let score_units = if source.lane == "support" {
            let score_units = u64::from(source.source_score_units).min(remaining_support_units);
            remaining_support_units -= score_units;
            score_units as u32
        } else {
            0
        };
        contributions.push(NativeEvidenceWaterfallContribution {
            evidence_ref_id: evidence_ref_id.clone(),
            lane: source.lane,
            operator: source.operator,
            view_field: source.view_field,
            left_surface_id: record.left_surface_id.clone(),
            right_surface_id: record.right_surface_id.clone(),
            evidence_count: 1,
            reason_codes: source.reason_codes,
            source_score_units: source.source_score_units,
            score_units,
            running_total_units: 0,
            value_frequency: None,
        });
    }
    contributions.sort_by(waterfall_contribution_cmp);

    let mut running_total_units = 0u64;
    let mut raw_support_score_units = 0u64;
    for contribution in &mut contributions {
        if contribution.lane == "support" {
            raw_support_score_units =
                raw_support_score_units.saturating_add(u64::from(contribution.source_score_units));
        }
        running_total_units =
            running_total_units.saturating_add(u64::from(contribution.score_units));
        contribution.running_total_units = running_total_units as u32;
    }

    NativeEvidenceWaterfall {
        score_total_units: record.pair_score_total.as_u32(),
        raw_support_score_units,
        threshold_lines: threshold_lines(record.pair_score_total.as_u32(), thresholds),
        contributions,
    }
}

fn pair_evidence_ref_id(record: &EdgeEvidenceRecord) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(record.left_surface_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(record.right_surface_id.as_bytes());
    let digest = witness::hash_bytes(&bytes);
    let hex = digest.strip_prefix("blake3:").unwrap_or(digest.as_str());
    format!("evidence:score_pair:{}", &hex[..16])
}

fn threshold_lines(
    score_units: u32,
    thresholds: ScorePairThresholds,
) -> Vec<NativeEvidenceWaterfallThresholdLine> {
    [
        (
            "backbone_score_min",
            thresholds.backbone_score_min,
            "strategy.solver.backbone_score_min".to_string(),
        ),
        (
            "attach_score_min",
            thresholds.attach_score_min,
            "strategy.solver.attach_score_min".to_string(),
        ),
        (
            "abstain_margin",
            thresholds
                .attach_score_min
                .saturating_sub(thresholds.abstain_margin),
            format!(
                "strategy.solver.abstain_margin; margin_units={}",
                thresholds.abstain_margin
            ),
        ),
    ]
    .into_iter()
    .map(
        |(threshold_id, threshold_units, source)| NativeEvidenceWaterfallThresholdLine {
            threshold_id: threshold_id.to_string(),
            score_units: Some(threshold_units),
            delta_units: Some(i64::from(score_units) - i64::from(threshold_units)),
            source,
        },
    )
    .collect()
}

fn score_lane_string(lane: ScoreLane) -> String {
    match lane {
        ScoreLane::Support => "support",
        ScoreLane::AntiMerge => "anti_merge",
        ScoreLane::RelationHint => "relation_hint",
    }
    .to_string()
}

fn waterfall_source_cmp(left: &WaterfallSource, right: &WaterfallSource) -> Ordering {
    right
        .source_score_units
        .cmp(&left.source_score_units)
        .then_with(|| left.operator.cmp(&right.operator))
        .then_with(|| left.view_field.cmp(&right.view_field))
        .then_with(|| left.lane.cmp(&right.lane))
        .then_with(|| left.reason_codes.cmp(&right.reason_codes))
}

fn waterfall_contribution_cmp(
    left: &NativeEvidenceWaterfallContribution,
    right: &NativeEvidenceWaterfallContribution,
) -> Ordering {
    right
        .score_units
        .cmp(&left.score_units)
        .then_with(|| left.operator.cmp(&right.operator))
        .then_with(|| left.view_field.cmp(&right.view_field))
        .then_with(|| left.lane.cmp(&right.lane))
        .then_with(|| left.reason_codes.cmp(&right.reason_codes))
        .then_with(|| left.evidence_ref_id.cmp(&right.evidence_ref_id))
        .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}

fn load_score_pair_strategy(strategy: &Path) -> Result<ScorePairStrategy, Refusal> {
    let bytes = fs::read(strategy).map_err(|error| {
        score_pair_refusal(
            EntityRefusalKind::Strategy,
            "Failed to read entity score-pair strategy",
            json!({
                "path": strategy.display().to_string(),
                "error": error.to_string()
            }),
            Some("Provide a readable entity strategy YAML file".to_string()),
        )
    })?;
    let value = serde_yaml::from_slice::<serde_yaml::Value>(&bytes).map_err(|error| {
        score_pair_refusal(
            EntityRefusalKind::Strategy,
            "Invalid entity score-pair strategy YAML",
            json!({
                "path": strategy.display().to_string(),
                "error": error.to_string()
            }),
            Some("Fix the strategy YAML before running canon entity score-pair".to_string()),
        )
    })?;
    let thresholds = score_pair_thresholds(&value)?;
    Ok(ScorePairStrategy {
        content_hash: witness::hash_bytes(&bytes),
        thresholds,
    })
}

fn score_pair_thresholds(value: &serde_yaml::Value) -> Result<ScorePairThresholds, Refusal> {
    let Some(solver) = value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String("solver".to_string())))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return Ok(ScorePairThresholds::default());
    };

    let defaults = ScorePairThresholds::default();
    let backbone = optional_strategy_score_units(solver, "backbone_score_min")?
        .unwrap_or(defaults.backbone_score_min);
    let attach = optional_strategy_score_units(solver, "attach_score_min")?.unwrap_or(backbone);
    let abstain_margin =
        optional_strategy_score_units(solver, "abstain_margin")?.unwrap_or(defaults.abstain_margin);
    if attach > backbone {
        return Err(score_pair_refusal(
            EntityRefusalKind::Strategy,
            "Entity score-pair strategy attach threshold cannot exceed backbone threshold",
            json!({
                "attach_score_min": attach,
                "backbone_score_min": backbone
            }),
            Some(
                "Set solver.attach_score_min less than or equal to solver.backbone_score_min"
                    .to_string(),
            ),
        ));
    }
    if abstain_margin > attach {
        return Err(score_pair_refusal(
            EntityRefusalKind::Strategy,
            "Entity score-pair strategy abstain margin cannot exceed attach threshold",
            json!({
                "abstain_margin": abstain_margin,
                "attach_score_min": attach
            }),
            Some(
                "Set solver.abstain_margin less than or equal to solver.attach_score_min"
                    .to_string(),
            ),
        ));
    }
    Ok(ScorePairThresholds {
        backbone_score_min: backbone,
        attach_score_min: attach,
        abstain_margin,
    })
}

fn optional_strategy_score_units(
    solver: &serde_yaml::Mapping,
    field: &'static str,
) -> Result<Option<u32>, Refusal> {
    let Some(value) = solver.get(serde_yaml::Value::String(field.to_string())) else {
        return Ok(None);
    };
    let parsed = match value {
        serde_yaml::Value::Number(number) => number.as_u64(),
        serde_yaml::Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
    .ok_or_else(|| {
        score_pair_refusal(
            EntityRefusalKind::Strategy,
            "Entity score-pair strategy thresholds must be integer score units",
            json!({
                "field": field,
                "value": value
            }),
            Some("Use integer solver threshold units in the strategy YAML".to_string()),
        )
    })?;
    if parsed > u64::from(ENTITY_SCORE_SCALE) {
        return Err(score_pair_refusal(
            EntityRefusalKind::Strategy,
            "Entity score-pair strategy threshold is outside the entity score scale",
            json!({
                "field": field,
                "value": parsed,
                "max": ENTITY_SCORE_SCALE
            }),
            Some("Use threshold units between 0 and 10000".to_string()),
        ));
    }
    Ok(Some(parsed as u32))
}

fn score_pair_json_refusal(error: serde_json::Error) -> Refusal {
    score_pair_refusal(
        EntityRefusalKind::InputContract,
        "Failed to serialize entity score-pair input record",
        json!({ "error": error.to_string() }),
        None,
    )
}

fn score_pair_refusal(
    kind: EntityRefusalKind,
    message: impl Into<String>,
    detail: Value,
    next_command: Option<String>,
) -> Refusal {
    kind.to_refusal(
        message,
        json!({
            "stage": "score_pair",
            "detail": detail,
            "writes_performed": false
        }),
        next_command,
    )
}
