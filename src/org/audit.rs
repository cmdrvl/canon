//! Audit engine for `canon org`.

use super::types::{
    AuditArtifact, AuditMetrics, AuditSummary, CANON_ORG_AUDIT_VERSION, CANON_ORG_RUN_VERSION,
    CANON_ORG_SOLVE_VERSION, OrgEntityState, OrgError, OrgErrorCode, OrgResult,
    ProjectedObservation, PromotionDecision, ResultReference, SolveRunArtifact, SuiteReference,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuditBudgetUsage {
    pub runtime_seconds: u64,
    pub candidate_pairs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuditBaseline {
    pub holdout_score: f64,
    pub anchor_consistency_holdout: f64,
    pub gold_pair_f1_holdout: Option<f64>,
    pub continuity_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuditContext<'a> {
    pub suite_dir: &'a Path,
    pub profile: &'a str,
    pub budget_usage: AuditBudgetUsage,
    pub baseline: Option<AuditBaseline>,
    pub promoted_with_prior_escrow_count: u64,
}

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    suite_id: String,
    profile: String,
    thresholds: SuiteThresholds,
    budget: SuiteBudget,
}

#[derive(Debug, Deserialize)]
struct SuiteThresholds {
    max_contradiction_rate: f64,
    min_perturbation_stability: f64,
    non_regression_epsilon: f64,
}

#[derive(Debug, Deserialize)]
struct SuiteBudget {
    max_runtime_seconds: u64,
    max_candidate_pairs: u64,
}

#[derive(Debug, Deserialize)]
struct SilverAnchorFixture {
    source_row_id: String,
    namespace: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct PerturbationFixture {
    #[serde(rename = "set_id")]
    _set_id: String,
    member_row_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ContradictionFixture {
    #[serde(rename = "fixture_id")]
    _fixture_id: String,
    row_ids: Vec<String>,
    #[serde(rename = "reason")]
    _reason: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LabeledPair {
    left_row_id: String,
    right_row_id: String,
    label: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CorpusPartition {
    Tune,
    Holdout,
}

#[derive(Debug, Clone)]
struct SuiteRowInfo {
    raw_name_surface: String,
    partition: CorpusPartition,
}

#[derive(Debug)]
struct SuiteData {
    manifest: SuiteManifest,
    row_catalog: BTreeMap<String, SuiteRowInfo>,
    silver_anchors: Vec<SilverAnchorFixture>,
    perturbations: Vec<PerturbationFixture>,
    contradictions: Vec<ContradictionFixture>,
    continuity_pairs: Vec<LabeledPair>,
    gold_pairs: Option<Vec<LabeledPair>>,
}

#[derive(Debug, Clone)]
struct RowResolution {
    component_label: String,
    canonical_label: Option<String>,
    comparable_incumbent_id: Option<String>,
    abstain_conflict: bool,
}

pub fn audit(
    result: &SolveRunArtifact,
    result_bytes: &[u8],
    context: AuditContext<'_>,
) -> OrgResult<AuditArtifact> {
    validate_result_artifact(result)?;
    let suite = load_suite(context.suite_dir, context.profile)?;
    let row_resolutions = build_row_resolutions(result)?;
    validate_suite_row_references(&suite, &row_resolutions)?;

    let gold_pair_f1 = suite.gold_pairs.as_ref().map(|pairs| {
        pair_f1(
            pairs,
            &row_resolutions,
            predicts_same_component_or_canonical,
        )
    });
    let anchor_consistency_metric = anchor_consistency(
        &suite.silver_anchors,
        &row_resolutions,
        &suite.row_catalog,
        None,
    )?;
    let anchor_consistency_holdout = anchor_consistency(
        &suite.silver_anchors,
        &row_resolutions,
        &suite.row_catalog,
        Some(CorpusPartition::Holdout),
    )?;
    let contradiction_rate_metric = contradiction_rate(
        &suite.contradictions,
        &row_resolutions,
        &suite.row_catalog,
        None,
    )?;
    let contradiction_rate_holdout = contradiction_rate(
        &suite.contradictions,
        &row_resolutions,
        &suite.row_catalog,
        Some(CorpusPartition::Holdout),
    )?;
    let perturbation_stability_metric = perturbation_stability(
        &suite.perturbations,
        &row_resolutions,
        &suite.row_catalog,
        None,
    )?;
    let perturbation_stability_holdout = perturbation_stability(
        &suite.perturbations,
        &row_resolutions,
        &suite.row_catalog,
        Some(CorpusPartition::Holdout),
    )?;
    let continuity_score = pair_f1(
        &suite.continuity_pairs,
        &row_resolutions,
        predicts_same_continuity_identity,
    );
    let continuity_gain = continuity_score
        - context
            .baseline
            .map(|baseline| baseline.continuity_score)
            .unwrap_or(0.0);
    let anchor_conflicts =
        anchor_conflicts(&suite.silver_anchors, &row_resolutions, &suite.row_catalog)?;
    let compression_gain = compression_gain(&suite.row_catalog, &row_resolutions);
    let registry_churn = registry_churn(&row_resolutions);
    let escrow_reuse_rate = escrow_reuse_rate(result, context.promoted_with_prior_escrow_count)?;
    let holdout_gold_pair_f1 = suite.gold_pairs.as_ref().map(|pairs| {
        filtered_pair_f1(
            pairs,
            &suite.row_catalog,
            CorpusPartition::Holdout,
            &row_resolutions,
            predicts_same_component_or_canonical,
        )
    });
    let holdout_score = geometric_mean(&holdout_terms(
        anchor_consistency_holdout,
        perturbation_stability_holdout,
        contradiction_rate_holdout,
        holdout_gold_pair_f1,
    ))?;

    let metrics = AuditMetrics {
        gold_pair_f1,
        anchor_consistency: anchor_consistency_metric,
        anchor_conflicts,
        holdout_score,
        contradiction_rate: contradiction_rate_metric,
        perturbation_stability: perturbation_stability_metric,
        continuity_gain,
        compression_gain,
        registry_churn,
        escrow_reuse_rate,
    };

    let gate_failures = gate_failures(
        &suite.manifest,
        &metrics,
        anchor_consistency_holdout,
        holdout_gold_pair_f1,
        context,
    );
    let hard_gates_passed = gate_failures.is_empty();

    Ok(AuditArtifact {
        version: CANON_ORG_AUDIT_VERSION.to_string(),
        result: ResultReference {
            version: result.version.clone(),
            content_hash: format!("blake3:{}", blake3::hash(result_bytes).to_hex()),
            strategy_content_hash: result.strategy.content_hash.clone(),
            lookup_snapshot_hash: result.registry.lookup_snapshot_hash.clone(),
            escrow_snapshot_hash: result.registry.escrow_snapshot_hash.clone(),
        },
        suite: SuiteReference {
            id: suite.manifest.suite_id,
        },
        summary: AuditSummary {
            decision: if hard_gates_passed {
                PromotionDecision::Promote
            } else {
                PromotionDecision::Reject
            },
            hard_gates_passed,
        },
        metrics,
        gate_failures,
    })
}

fn validate_result_artifact(result: &SolveRunArtifact) -> OrgResult<()> {
    match result.version.as_str() {
        CANON_ORG_SOLVE_VERSION | CANON_ORG_RUN_VERSION => Ok(()),
        other => Err(audit_error(
            "Audit requires a canon_org_solve.v0 or canon_org_run.v0 artifact",
            json!({
                "expected": [CANON_ORG_SOLVE_VERSION, CANON_ORG_RUN_VERSION],
                "actual": other,
            }),
        )),
    }
}

fn load_suite(suite_dir: &Path, profile: &str) -> OrgResult<SuiteData> {
    let manifest: SuiteManifest =
        read_json_file(&suite_dir.join("manifest.json"), "suite manifest")?;
    if manifest.profile != profile {
        return Err(audit_error(
            "Suite profile does not match the requested profile",
            json!({
                "suite_profile": manifest.profile,
                "requested_profile": profile,
            }),
        ));
    }

    let row_catalog = load_row_catalog(suite_dir)?;
    let silver_anchors =
        read_jsonl_file(&suite_dir.join("silver_anchors.jsonl"), "silver anchors")?;
    let perturbations = read_jsonl_file(&suite_dir.join("perturbations.jsonl"), "perturbations")?;
    let contradictions: Vec<ContradictionFixture> =
        read_yaml_file(&suite_dir.join("contradictions.yaml"), "contradictions")?;
    let continuity_pairs = load_jsonl_directory(&suite_dir.join("continuity"), "continuity")?;
    let gold_path = suite_dir.join("optional_gold_pairs.jsonl");
    let gold_pairs = if gold_path.exists() {
        Some(read_jsonl_file(&gold_path, "gold pairs")?)
    } else {
        None
    };

    Ok(SuiteData {
        manifest,
        row_catalog,
        silver_anchors,
        perturbations,
        contradictions,
        continuity_pairs,
        gold_pairs,
    })
}

fn load_row_catalog(suite_dir: &Path) -> OrgResult<BTreeMap<String, SuiteRowInfo>> {
    let mut row_catalog = BTreeMap::new();

    for (partition, subdir) in [
        (CorpusPartition::Tune, suite_dir.join("tune")),
        (CorpusPartition::Holdout, suite_dir.join("holdout")),
    ] {
        for observation in load_jsonl_directory::<ProjectedObservation>(&subdir, "suite corpus")? {
            if row_catalog
                .insert(
                    observation.source_row_id.clone(),
                    SuiteRowInfo {
                        raw_name_surface: observation.primary_surface.value,
                        partition,
                    },
                )
                .is_some()
            {
                return Err(audit_error(
                    "Suite row catalog contains duplicate source_row_id values",
                    json!({
                        "source_row_id": observation.source_row_id,
                    }),
                ));
            }
        }
    }

    Ok(row_catalog)
}

fn build_row_resolutions(result: &SolveRunArtifact) -> OrgResult<BTreeMap<String, RowResolution>> {
    let mut rows = BTreeMap::new();

    for entity in &result.entities {
        let component_label = component_label("entity", &entity.all_rows);
        let comparable_incumbent_id = match entity.state {
            OrgEntityState::ResolvedExisting => entity.inheritance.incumbent_ids.first().cloned(),
            _ => None,
        };

        for row_id in &entity.all_rows {
            insert_row_resolution(
                &mut rows,
                row_id,
                RowResolution {
                    component_label: component_label.clone(),
                    canonical_label: entity.canonical_id.clone(),
                    comparable_incumbent_id: comparable_incumbent_id.clone(),
                    abstain_conflict: false,
                },
            )?;
        }
    }

    for abstention in &result.abstentions {
        let component_label = component_label("abstention", &abstention.all_rows);
        let comparable_incumbent_id =
            (abstention.incumbent_ids.len() == 1).then(|| abstention.incumbent_ids[0].clone());

        for row_id in &abstention.all_rows {
            insert_row_resolution(
                &mut rows,
                row_id,
                RowResolution {
                    component_label: component_label.clone(),
                    canonical_label: None,
                    comparable_incumbent_id: comparable_incumbent_id.clone(),
                    abstain_conflict: matches!(abstention.state, OrgEntityState::AbstainConflict),
                },
            )?;
        }
    }

    for contradiction in &result.contradictions {
        let component_label = component_label("contradiction", &contradiction.row_ids);

        for row_id in &contradiction.row_ids {
            insert_row_resolution(
                &mut rows,
                row_id,
                RowResolution {
                    component_label: component_label.clone(),
                    canonical_label: None,
                    comparable_incumbent_id: None,
                    abstain_conflict: false,
                },
            )?;
        }
    }

    Ok(rows)
}

fn insert_row_resolution(
    rows: &mut BTreeMap<String, RowResolution>,
    row_id: &str,
    resolution: RowResolution,
) -> OrgResult<()> {
    if rows.insert(row_id.to_string(), resolution).is_some() {
        return Err(audit_error(
            "Result artifact maps one source_row_id into multiple outcomes",
            json!({
                "source_row_id": row_id,
            }),
        ));
    }
    Ok(())
}

fn validate_suite_row_references(
    suite: &SuiteData,
    row_resolutions: &BTreeMap<String, RowResolution>,
) -> OrgResult<()> {
    for row_id in suite.row_catalog.keys() {
        if !row_resolutions.contains_key(row_id) {
            return Err(audit_error(
                "Suite row catalog references a source_row_id missing from the result artifact",
                json!({
                    "source_row_id": row_id,
                }),
            ));
        }
    }

    let verify_row = |row_id: &str, origin: &str| -> OrgResult<()> {
        if !suite.row_catalog.contains_key(row_id) || !row_resolutions.contains_key(row_id) {
            return Err(audit_error(
                "Suite fixture references an unknown source_row_id",
                json!({
                    "source_row_id": row_id,
                    "origin": origin,
                }),
            ));
        }
        Ok(())
    };

    for fixture in &suite.silver_anchors {
        verify_row(&fixture.source_row_id, "silver_anchors.jsonl")?;
    }
    for fixture in &suite.perturbations {
        for row_id in &fixture.member_row_ids {
            verify_row(row_id, "perturbations.jsonl")?;
        }
    }
    for fixture in &suite.contradictions {
        for row_id in &fixture.row_ids {
            verify_row(row_id, "contradictions.yaml")?;
        }
    }
    for pair in &suite.continuity_pairs {
        verify_row(&pair.left_row_id, "continuity/*.jsonl")?;
        verify_row(&pair.right_row_id, "continuity/*.jsonl")?;
    }
    if let Some(gold_pairs) = &suite.gold_pairs {
        for pair in gold_pairs {
            verify_row(&pair.left_row_id, "optional_gold_pairs.jsonl")?;
            verify_row(&pair.right_row_id, "optional_gold_pairs.jsonl")?;
        }
    }

    Ok(())
}

fn pair_f1(
    pairs: &[LabeledPair],
    row_resolutions: &BTreeMap<String, RowResolution>,
    predictor: fn(&RowResolution, &RowResolution) -> bool,
) -> f64 {
    if pairs.is_empty() {
        return 1.0;
    }

    let mut true_positive = 0_u64;
    let mut false_positive = 0_u64;
    let mut false_negative = 0_u64;

    for pair in pairs {
        let prediction = predictor(
            row_resolutions
                .get(&pair.left_row_id)
                .expect("pair row validated"),
            row_resolutions
                .get(&pair.right_row_id)
                .expect("pair row validated"),
        );
        match (prediction, pair.label) {
            (true, 1) => true_positive += 1,
            (true, 0) => false_positive += 1,
            (false, 1) => false_negative += 1,
            (false, 0) => {}
            (_, other) => panic!("unexpected binary label {other}"),
        }
    }

    f1_from_counts(true_positive, false_positive, false_negative)
}

fn filtered_pair_f1(
    pairs: &[LabeledPair],
    row_catalog: &BTreeMap<String, SuiteRowInfo>,
    partition: CorpusPartition,
    row_resolutions: &BTreeMap<String, RowResolution>,
    predictor: fn(&RowResolution, &RowResolution) -> bool,
) -> f64 {
    let filtered = pairs
        .iter()
        .filter(|pair| {
            row_catalog
                .get(&pair.left_row_id)
                .is_some_and(|row| row.partition == partition)
                && row_catalog
                    .get(&pair.right_row_id)
                    .is_some_and(|row| row.partition == partition)
        })
        .cloned()
        .collect::<Vec<_>>();

    pair_f1(&filtered, row_resolutions, predictor)
}

fn anchor_consistency(
    fixtures: &[SilverAnchorFixture],
    row_resolutions: &BTreeMap<String, RowResolution>,
    row_catalog: &BTreeMap<String, SuiteRowInfo>,
    partition: Option<CorpusPartition>,
) -> OrgResult<f64> {
    let mut grouped = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();

    for fixture in fixtures {
        if let Some(partition) = partition
            && row_catalog
                .get(&fixture.source_row_id)
                .is_some_and(|row| row.partition != partition)
        {
            continue;
        }

        grouped
            .entry(fixture.namespace.clone())
            .or_default()
            .entry(fixture.value.clone())
            .or_default()
            .push(fixture.source_row_id.clone());
    }

    let mut positive_correct = 0_u64;
    let mut positive_total = 0_u64;
    let mut negative_correct = 0_u64;
    let mut negative_total = 0_u64;

    for values in grouped.into_values() {
        let value_entries = values.into_iter().collect::<Vec<_>>();

        for (_, row_ids) in &value_entries {
            for (index, left_row_id) in row_ids.iter().enumerate() {
                for right_row_id in row_ids.iter().skip(index + 1) {
                    positive_total += 1;
                    if predicts_same_component_or_canonical(
                        row_resolutions
                            .get(left_row_id)
                            .expect("silver row validated"),
                        row_resolutions
                            .get(right_row_id)
                            .expect("silver row validated"),
                    ) {
                        positive_correct += 1;
                    }
                }
            }
        }

        for (left_index, (_, left_rows)) in value_entries.iter().enumerate() {
            for (_, right_rows) in value_entries.iter().skip(left_index + 1) {
                for left_row_id in left_rows {
                    for right_row_id in right_rows {
                        negative_total += 1;
                        if !predicts_same_component_or_canonical(
                            row_resolutions
                                .get(left_row_id)
                                .expect("silver row validated"),
                            row_resolutions
                                .get(right_row_id)
                                .expect("silver row validated"),
                        ) {
                            negative_correct += 1;
                        }
                    }
                }
            }
        }
    }

    let total = positive_total + negative_total;
    if total == 0 {
        Ok(1.0)
    } else {
        Ok((positive_correct + negative_correct) as f64 / total as f64)
    }
}

fn perturbation_stability(
    fixtures: &[PerturbationFixture],
    row_resolutions: &BTreeMap<String, RowResolution>,
    row_catalog: &BTreeMap<String, SuiteRowInfo>,
    partition: Option<CorpusPartition>,
) -> OrgResult<f64> {
    let filtered = fixtures
        .iter()
        .filter(|fixture| {
            fixture_matches_partition(&fixture.member_row_ids, row_catalog, partition)
        })
        .collect::<Vec<_>>();

    if filtered.is_empty() {
        return Ok(1.0);
    }

    let stable = filtered
        .iter()
        .filter(|fixture| {
            let rows = fixture
                .member_row_ids
                .iter()
                .map(|row_id| {
                    row_resolutions
                        .get(row_id)
                        .expect("perturbation row validated")
                })
                .collect::<Vec<_>>();
            perturbation_fixture_is_stable(&rows)
        })
        .count();

    Ok(stable as f64 / filtered.len() as f64)
}

fn perturbation_fixture_is_stable(rows: &[&RowResolution]) -> bool {
    let component_labels = rows
        .iter()
        .map(|row| row.component_label.clone())
        .collect::<BTreeSet<_>>();
    if component_labels.len() == 1 {
        return true;
    }

    let canonical_labels = rows
        .iter()
        .filter_map(|row| row.canonical_label.clone())
        .collect::<BTreeSet<_>>();
    canonical_labels.len() == 1
        && canonical_labels.iter().next().is_some()
        && rows.iter().all(|row| row.canonical_label.is_some())
}

fn contradiction_rate(
    fixtures: &[ContradictionFixture],
    row_resolutions: &BTreeMap<String, RowResolution>,
    row_catalog: &BTreeMap<String, SuiteRowInfo>,
    partition: Option<CorpusPartition>,
) -> OrgResult<f64> {
    let filtered = fixtures
        .iter()
        .filter(|fixture| fixture_matches_partition(&fixture.row_ids, row_catalog, partition))
        .collect::<Vec<_>>();

    if filtered.is_empty() {
        return Ok(0.0);
    }

    let violated = filtered
        .iter()
        .filter(|fixture| {
            let rows = fixture
                .row_ids
                .iter()
                .map(|row_id| {
                    row_resolutions
                        .get(row_id)
                        .expect("contradiction row validated")
                })
                .collect::<Vec<_>>();
            contradiction_fixture_is_violated(&rows)
        })
        .count();

    Ok(violated as f64 / filtered.len() as f64)
}

fn contradiction_fixture_is_violated(rows: &[&RowResolution]) -> bool {
    let component_labels = rows
        .iter()
        .map(|row| row.component_label.clone())
        .collect::<BTreeSet<_>>();
    if component_labels.len() == 1 {
        return true;
    }

    let canonical_labels = rows
        .iter()
        .filter_map(|row| row.canonical_label.clone())
        .collect::<BTreeSet<_>>();
    canonical_labels.len() == 1
        && canonical_labels.iter().next().is_some()
        && rows.iter().all(|row| row.canonical_label.is_some())
}

fn anchor_conflicts(
    fixtures: &[SilverAnchorFixture],
    row_resolutions: &BTreeMap<String, RowResolution>,
    row_catalog: &BTreeMap<String, SuiteRowInfo>,
) -> OrgResult<u64> {
    let mut by_component = BTreeMap::<String, BTreeMap<String, BTreeSet<String>>>::new();

    for fixture in fixtures {
        if !row_catalog.contains_key(&fixture.source_row_id) {
            continue;
        }

        let component_label = row_resolutions
            .get(&fixture.source_row_id)
            .expect("silver row validated")
            .component_label
            .clone();
        by_component
            .entry(component_label)
            .or_default()
            .entry(fixture.namespace.clone())
            .or_default()
            .insert(fixture.value.clone());
    }

    Ok(by_component
        .into_values()
        .filter(|namespace_values| namespace_values.values().any(|values| values.len() > 1))
        .count() as u64)
}

fn compression_gain(
    row_catalog: &BTreeMap<String, SuiteRowInfo>,
    row_resolutions: &BTreeMap<String, RowResolution>,
) -> f64 {
    let included_rows = row_resolutions
        .iter()
        .filter(|(_, resolution)| !resolution.abstain_conflict)
        .collect::<Vec<_>>();

    let distinct_raw_surfaces = included_rows
        .iter()
        .filter_map(|(row_id, _)| row_catalog.get(*row_id))
        .map(|row| row.raw_name_surface.clone())
        .collect::<BTreeSet<_>>();
    if distinct_raw_surfaces.is_empty() {
        return 0.0;
    }

    let distinct_identity_labels = included_rows
        .iter()
        .filter_map(|(_, resolution)| resolution.canonical_label.clone())
        .collect::<BTreeSet<_>>();

    1.0 - distinct_identity_labels.len() as f64 / distinct_raw_surfaces.len() as f64
}

fn registry_churn(row_resolutions: &BTreeMap<String, RowResolution>) -> f64 {
    let mut comparable = 0_u64;
    let mut changed = 0_u64;

    for resolution in row_resolutions.values() {
        if let Some(incumbent_id) = resolution.comparable_incumbent_id.as_deref() {
            comparable += 1;
            if resolution.canonical_label.as_deref() != Some(incumbent_id) {
                changed += 1;
            }
        }
    }

    if comparable == 0 {
        0.0
    } else {
        changed as f64 / comparable as f64
    }
}

fn escrow_reuse_rate(
    result: &SolveRunArtifact,
    promoted_with_prior_escrow_count: u64,
) -> OrgResult<f64> {
    let promotable_new_clusters = result
        .entities
        .iter()
        .filter(|entity| matches!(entity.state, OrgEntityState::PromotableNew))
        .count() as u64;

    if promotable_new_clusters == 0 {
        return Ok(0.0);
    }
    if promoted_with_prior_escrow_count > promotable_new_clusters {
        return Err(audit_error(
            "Promoted-with-prior-escrow count exceeds promotable_new cluster count",
            json!({
                "promoted_with_prior_escrow_count": promoted_with_prior_escrow_count,
                "promotable_new_clusters": promotable_new_clusters,
            }),
        ));
    }

    Ok(promoted_with_prior_escrow_count as f64 / promotable_new_clusters as f64)
}

fn holdout_terms(
    anchor_consistency_holdout: f64,
    perturbation_stability_holdout: f64,
    contradiction_rate_holdout: f64,
    gold_pair_f1_holdout: Option<f64>,
) -> Vec<f64> {
    let mut terms = vec![
        anchor_consistency_holdout,
        perturbation_stability_holdout,
        1.0 - contradiction_rate_holdout,
    ];
    if let Some(gold_pair_f1_holdout) = gold_pair_f1_holdout {
        terms.push(gold_pair_f1_holdout);
    }
    terms
}

fn geometric_mean(values: &[f64]) -> OrgResult<f64> {
    if values.is_empty() {
        return Err(audit_error(
            "Holdout score requires at least one holdout term",
            json!({}),
        ));
    }

    let mut log_sum = 0.0;
    for value in values {
        if *value <= 0.0 {
            return Ok(0.0);
        }
        log_sum += value.ln();
    }
    Ok((log_sum / values.len() as f64).exp())
}

fn gate_failures(
    manifest: &SuiteManifest,
    metrics: &AuditMetrics,
    anchor_consistency_holdout: f64,
    gold_pair_f1_holdout: Option<f64>,
    context: AuditContext<'_>,
) -> Vec<String> {
    let mut failures = Vec::new();

    if metrics.anchor_conflicts > 0 {
        failures.push("anchor_conflicts".to_string());
    }
    if metrics.contradiction_rate > manifest.thresholds.max_contradiction_rate {
        failures.push("max_contradiction_rate".to_string());
    }
    if metrics.perturbation_stability < manifest.thresholds.min_perturbation_stability {
        failures.push("min_perturbation_stability".to_string());
    }
    if context.budget_usage.runtime_seconds > manifest.budget.max_runtime_seconds {
        failures.push("budget_runtime_seconds".to_string());
    }
    if context.budget_usage.candidate_pairs > manifest.budget.max_candidate_pairs {
        failures.push("budget_candidate_pairs".to_string());
    }

    if let Some(baseline) = context.baseline {
        let epsilon = manifest.thresholds.non_regression_epsilon;
        if metrics.holdout_score + epsilon < baseline.holdout_score {
            failures.push("holdout_non_regression".to_string());
        }
        if anchor_consistency_holdout + epsilon < baseline.anchor_consistency_holdout {
            failures.push("anchor_consistency_holdout_non_regression".to_string());
        }
        match (gold_pair_f1_holdout, baseline.gold_pair_f1_holdout) {
            (Some(candidate), Some(incumbent)) if candidate + epsilon < incumbent => {
                failures.push("gold_pair_f1_holdout_non_regression".to_string());
            }
            (None, Some(_)) => {
                failures.push("gold_pair_f1_holdout_non_regression".to_string());
            }
            _ => {}
        }
    }

    failures
}

fn fixture_matches_partition(
    row_ids: &[String],
    row_catalog: &BTreeMap<String, SuiteRowInfo>,
    partition: Option<CorpusPartition>,
) -> bool {
    match partition {
        Some(partition) => row_ids.iter().all(|row_id| {
            row_catalog
                .get(row_id)
                .is_some_and(|row| row.partition == partition)
        }),
        None => true,
    }
}

fn predicts_same_component_or_canonical(left: &RowResolution, right: &RowResolution) -> bool {
    left.component_label == right.component_label
        || left
            .canonical_label
            .as_ref()
            .zip(right.canonical_label.as_ref())
            .is_some_and(|(left, right)| left == right)
}

fn predicts_same_continuity_identity(left: &RowResolution, right: &RowResolution) -> bool {
    left.canonical_label
        .as_ref()
        .zip(right.canonical_label.as_ref())
        .is_some_and(|(left, right)| left == right)
}

fn f1_from_counts(true_positive: u64, false_positive: u64, false_negative: u64) -> f64 {
    let precision_denominator = true_positive + false_positive;
    let recall_denominator = true_positive + false_negative;

    if precision_denominator == 0 && recall_denominator == 0 {
        return 1.0;
    }
    if precision_denominator == 0 || recall_denominator == 0 {
        return 0.0;
    }

    let precision = true_positive as f64 / precision_denominator as f64;
    let recall = true_positive as f64 / recall_denominator as f64;
    if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

fn component_label(prefix: &str, row_ids: &[String]) -> String {
    format!("{prefix}:{}", row_ids.join("|"))
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> OrgResult<T> {
    let text = fs::read_to_string(path).map_err(|error| {
        audit_error(
            format!("Failed to read {label}"),
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    serde_json::from_str(&text).map_err(|error| {
        audit_error(
            format!("Failed to parse {label}"),
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })
}

fn read_yaml_file<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> OrgResult<T> {
    let text = fs::read_to_string(path).map_err(|error| {
        audit_error(
            format!("Failed to read {label}"),
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    serde_yaml::from_str(&text).map_err(|error| {
        audit_error(
            format!("Failed to parse {label}"),
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })
}

fn read_jsonl_file<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> OrgResult<Vec<T>> {
    let text = fs::read_to_string(path).map_err(|error| {
        audit_error(
            format!("Failed to read {label}"),
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    let mut records = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str(line).map_err(|error| {
            audit_error(
                format!("Failed to parse {label} JSONL"),
                json!({
                    "path": path.display().to_string(),
                    "line_number": line_number + 1,
                    "error": error.to_string(),
                }),
            )
        })?;
        records.push(record);
    }

    Ok(records)
}

fn load_jsonl_directory<T: for<'de> Deserialize<'de>>(
    directory: &Path,
    label: &str,
) -> OrgResult<Vec<T>> {
    let mut files = fs::read_dir(directory)
        .map_err(|error| {
            audit_error(
                format!("Failed to read {label} directory"),
                json!({
                    "path": directory.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            audit_error(
                format!("Failed to enumerate {label} directory"),
                json!({
                    "path": directory.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;
    files.retain(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"));
    files.sort();

    let mut records = Vec::new();
    for path in files {
        records.extend(read_jsonl_file(&path, label)?);
    }

    Ok(records)
}

fn audit_error(message: impl Into<String>, detail: serde_json::Value) -> OrgError {
    OrgError::with_detail(OrgErrorCode::Audit, message, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::types::{
        AbstentionRecord, CANON_ORG_SOLVE_VERSION, InheritanceMode, InheritanceRecord,
        RegistrySnapshot, SolveRunSummary, SolvedEntity, StrategyReference,
    };
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn audit_positive_path_promotes_when_hard_gates_pass() {
        let suite_dir = build_suite();
        let result = positive_result();
        let result_bytes = serde_json::to_vec(&result).expect("result bytes");

        let artifact = audit(
            &result,
            &result_bytes,
            AuditContext {
                suite_dir: suite_dir.path(),
                profile: "bdc_issuer",
                budget_usage: AuditBudgetUsage {
                    runtime_seconds: 10,
                    candidate_pairs: 4,
                },
                baseline: Some(AuditBaseline {
                    holdout_score: 0.9,
                    anchor_consistency_holdout: 0.9,
                    gold_pair_f1_holdout: Some(0.9),
                    continuity_score: 0.5,
                }),
                promoted_with_prior_escrow_count: 1,
            },
        )
        .expect("audit to succeed");

        assert_eq!(artifact.version, CANON_ORG_AUDIT_VERSION);
        assert_eq!(artifact.summary.decision, PromotionDecision::Promote);
        assert!(artifact.summary.hard_gates_passed);
        assert!(artifact.gate_failures.is_empty());
        assert_eq!(artifact.suite.id, "bdc_org_eval.v1");
        assert_eq!(artifact.result.version, CANON_ORG_SOLVE_VERSION);
        assert!(artifact.result.content_hash.starts_with("blake3:"));
        assert_eq!(artifact.metrics.anchor_conflicts, 0);
        assert_eq!(artifact.metrics.escrow_reuse_rate, 1.0);
        assert!(artifact.metrics.holdout_score > 0.9);
    }

    #[test]
    fn audit_failing_path_rejects_when_perturbation_and_holdout_regress() {
        let suite_dir = build_suite();
        let result = failing_result();
        let result_bytes = serde_json::to_vec(&result).expect("result bytes");

        let artifact = audit(
            &result,
            &result_bytes,
            AuditContext {
                suite_dir: suite_dir.path(),
                profile: "bdc_issuer",
                budget_usage: AuditBudgetUsage {
                    runtime_seconds: 10,
                    candidate_pairs: 4,
                },
                baseline: Some(AuditBaseline {
                    holdout_score: 0.95,
                    anchor_consistency_holdout: 0.95,
                    gold_pair_f1_holdout: Some(0.95),
                    continuity_score: 0.5,
                }),
                promoted_with_prior_escrow_count: 0,
            },
        )
        .expect("audit to succeed");

        assert_eq!(artifact.summary.decision, PromotionDecision::Reject);
        assert!(!artifact.summary.hard_gates_passed);
        assert!(
            artifact
                .gate_failures
                .contains(&"min_perturbation_stability".to_string())
        );
        assert!(
            artifact
                .gate_failures
                .contains(&"holdout_non_regression".to_string())
        );
        assert!(
            artifact
                .gate_failures
                .contains(&"anchor_consistency_holdout_non_regression".to_string())
        );
    }

    fn build_suite() -> TempDir {
        let temp_dir = TempDir::new().expect("temp dir");
        fs::create_dir_all(temp_dir.path().join("tune")).expect("tune dir");
        fs::create_dir_all(temp_dir.path().join("holdout")).expect("holdout dir");
        fs::create_dir_all(temp_dir.path().join("continuity")).expect("continuity dir");

        fs::write(
            temp_dir.path().join("manifest.json"),
            serde_json::to_string(&json!({
                "suite_id": "bdc_org_eval.v1",
                "profile": "bdc_issuer",
                "thresholds": {
                    "max_contradiction_rate": 0.0,
                    "min_perturbation_stability": 0.995,
                    "non_regression_epsilon": 0.0005
                },
                "budget": {
                    "max_runtime_seconds": 120,
                    "max_candidate_pairs": 1000
                }
            }))
            .expect("manifest"),
        )
        .expect("write manifest");

        write_jsonl(
            &temp_dir.path().join("holdout").join("rows.jsonl"),
            &[
                projected_observation("row-1", "Acme Corp."),
                projected_observation("row-9", "ACME Corporation"),
                projected_observation("row-2", "Beacon Capital"),
                projected_observation("row-11", "Beacon Advisors"),
            ],
        );
        write_jsonl(
            &temp_dir.path().join("silver_anchors.jsonl"),
            &[
                json!({"source_row_id":"row-1","namespace":"lei","value":"LEI-1"}),
                json!({"source_row_id":"row-9","namespace":"lei","value":"LEI-1"}),
                json!({"source_row_id":"row-2","namespace":"lei","value":"LEI-2"}),
                json!({"source_row_id":"row-11","namespace":"lei","value":"LEI-3"}),
            ],
        );
        write_jsonl(
            &temp_dir.path().join("perturbations.jsonl"),
            &[json!({"set_id":"p-001","member_row_ids":["row-1","row-9"]})],
        );
        fs::write(
            temp_dir.path().join("contradictions.yaml"),
            "- fixture_id: c-001\n  row_ids: [\"row-2\", \"row-11\"]\n  reason: conflicting_trusted_anchor\n",
        )
        .expect("write contradictions");
        write_jsonl(
            &temp_dir.path().join("continuity").join("pairs.jsonl"),
            &[
                json!({"left_row_id":"row-1","right_row_id":"row-9","label":1}),
                json!({"left_row_id":"row-2","right_row_id":"row-11","label":0}),
            ],
        );
        write_jsonl(
            &temp_dir.path().join("optional_gold_pairs.jsonl"),
            &[
                json!({"left_row_id":"row-1","right_row_id":"row-9","label":1}),
                json!({"left_row_id":"row-2","right_row_id":"row-11","label":0}),
            ],
        );

        temp_dir
    }

    fn projected_observation(row_id: &str, surface: &str) -> serde_json::Value {
        json!({
            "version": "canon_org_projection.v0",
            "source_row_id": row_id,
            "doc_id": format!("doc-{row_id}"),
            "primary_surface": {
                "value": surface,
                "field": "portfolio_company"
            },
            "alias_surfaces": [],
            "mention_surfaces": [],
            "anchors": [],
            "context": {},
            "provenance": {}
        })
    }

    fn positive_result() -> SolveRunArtifact {
        SolveRunArtifact {
            version: CANON_ORG_SOLVE_VERSION.to_string(),
            strategy: strategy_reference(),
            registry: registry_snapshot(),
            summary: SolveRunSummary {
                observations: 4,
                resolved_existing: 1,
                promotable_new: 1,
                abstain_low_evidence: 1,
                abstain_conflict: 0,
            },
            entities: vec![
                SolvedEntity {
                    state: OrgEntityState::ResolvedExisting,
                    canonical_id: Some("IC-1".to_string()),
                    all_rows: vec!["row-1".to_string(), "row-9".to_string()],
                    inheritance: InheritanceRecord {
                        mode: InheritanceMode::SingleIncumbentOverlap,
                        incumbent_ids: vec!["IC-1".to_string()],
                    },
                    ..SolvedEntity::default()
                },
                SolvedEntity {
                    state: OrgEntityState::PromotableNew,
                    canonical_id: Some("IC-NEW".to_string()),
                    all_rows: vec!["row-2".to_string()],
                    inheritance: InheritanceRecord {
                        mode: InheritanceMode::NoIncumbentOverlap,
                        incumbent_ids: Vec::new(),
                    },
                    ..SolvedEntity::default()
                },
            ],
            abstentions: vec![AbstentionRecord {
                state: OrgEntityState::AbstainLowEvidence,
                all_rows: vec!["row-11".to_string()],
                reason: "insufficient_distinct_docs".to_string(),
                incumbent_ids: Vec::new(),
                escrow: None,
            }],
            contradictions: Vec::new(),
            ..SolveRunArtifact::default()
        }
    }

    fn failing_result() -> SolveRunArtifact {
        SolveRunArtifact {
            version: CANON_ORG_RUN_VERSION.to_string(),
            strategy: strategy_reference(),
            registry: registry_snapshot(),
            summary: SolveRunSummary {
                observations: 4,
                resolved_existing: 1,
                promotable_new: 0,
                abstain_low_evidence: 2,
                abstain_conflict: 0,
            },
            entities: vec![SolvedEntity {
                state: OrgEntityState::ResolvedExisting,
                canonical_id: Some("IC-1".to_string()),
                all_rows: vec!["row-1".to_string()],
                inheritance: InheritanceRecord {
                    mode: InheritanceMode::SingleIncumbentOverlap,
                    incumbent_ids: vec!["IC-1".to_string()],
                },
                ..SolvedEntity::default()
            }],
            abstentions: vec![
                AbstentionRecord {
                    state: OrgEntityState::AbstainLowEvidence,
                    all_rows: vec!["row-9".to_string()],
                    reason: "insufficient_backbone_evidence".to_string(),
                    incumbent_ids: Vec::new(),
                    escrow: None,
                },
                AbstentionRecord {
                    state: OrgEntityState::AbstainLowEvidence,
                    all_rows: vec!["row-2".to_string()],
                    reason: "insufficient_backbone_evidence".to_string(),
                    incumbent_ids: Vec::new(),
                    escrow: None,
                },
                AbstentionRecord {
                    state: OrgEntityState::AbstainLowEvidence,
                    all_rows: vec!["row-11".to_string()],
                    reason: "insufficient_backbone_evidence".to_string(),
                    incumbent_ids: Vec::new(),
                    escrow: None,
                },
            ],
            contradictions: Vec::new(),
            ..SolveRunArtifact::default()
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

    fn write_jsonl(path: &Path, values: &[serde_json::Value]) {
        let mut file = fs::File::create(path).expect("create jsonl file");
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                writeln!(file).expect("newline");
            }
            write!(file, "{value}").expect("write json line");
        }
    }
}
