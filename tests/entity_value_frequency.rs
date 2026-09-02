#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::evidence::value_frequency::{
        EntityValueFrequencyBand, EntityValueFrequencyBandConfig, EntityValueFrequencyTable,
        FREQUENCY_COMMON_MAX_COUNT_PARAM, FREQUENCY_COMMON_MULTIPLIER_PARAM,
        FREQUENCY_MINIMUM_COUNT_PARAM, FREQUENCY_RARE_MAX_COUNT_PARAM,
        FREQUENCY_RARE_MULTIPLIER_PARAM, FREQUENCY_TABLE_HASH_PARAM,
        FREQUENCY_UNCOMMON_MAX_COUNT_PARAM, FREQUENCY_UNCOMMON_MULTIPLIER_PARAM,
        FREQUENCY_VERY_COMMON_MULTIPLIER_PARAM, scale_score_units_by_frequency,
    },
    entity::{
        edge::{EdgeEvidenceHit, build_edge_evidence_record},
        evidence::{
            ExactViewSupportRequest, FrequencyWeightedExactViewSupportRequest,
            FrequencyWeightedStringSimilaritySupportRequest, StringSimilaritySupportRequest,
            apply_value_frequency_adjustment, exact_view_support_hit,
            frequency_weighted_exact_view_support_hit,
            frequency_weighted_string_similarity_support_hit, string_similarity_support_hit,
            validate_value_frequency_table_for_scoring,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
        score::{
            CandidateScoreDecisionReason, ScoreLane, ScoreThreshold, ScoreUnits, ScoredCandidate,
            evaluate_candidate_score,
        },
    },
    namekit::similarity::SimilarityMetric,
};
use std::collections::BTreeMap;

#[test]
fn absent_frequency_config_is_byte_identical_for_support_hits() {
    let exact = ExactViewSupportRequest {
        namespace: "name",
        operator_id: "exact_view:firm_core",
        reason_code: "exact_firm_core",
        view_name: "firm_core",
        left_value: "acme capital",
        right_value: "acme capital",
        score_units: score(8_000),
    };
    let unweighted_exact = exact_view_support_hit(exact.clone()).expect("exact support");
    let no_config_exact =
        frequency_weighted_exact_view_support_hit(FrequencyWeightedExactViewSupportRequest {
            support: exact,
            adjustment: None,
        })
        .expect("no-config exact support");
    assert_eq!(unweighted_exact, no_config_exact);
    assert_eq!(
        serde_json::to_vec(&unweighted_exact).expect("unweighted exact serializes"),
        serde_json::to_vec(&no_config_exact).expect("no-config exact serializes")
    );

    let fuzzy = StringSimilaritySupportRequest {
        namespace: "name",
        operator_id: "string_similarity:firm_core",
        reason_code: "similar_firm_core",
        metric: SimilarityMetric::JaroWinkler,
        left_value: "martha",
        right_value: "marhta",
        score_cutoff: Some(score(9_500)),
        score_hint: Some(score(9_600)),
    };
    let unweighted_fuzzy = string_similarity_support_hit(fuzzy.clone()).expect("fuzzy support");
    let no_config_fuzzy = frequency_weighted_string_similarity_support_hit(
        FrequencyWeightedStringSimilaritySupportRequest {
            support: fuzzy,
            adjustment: None,
        },
    )
    .expect("no-config fuzzy support");
    assert_eq!(unweighted_fuzzy, no_config_fuzzy);

    let unweighted_record = build_edge_evidence_record(
        "surf:001",
        "surf:002",
        vec![unweighted_exact, unweighted_fuzzy],
    )
    .expect("unweighted record");
    let no_config_record = build_edge_evidence_record(
        "surf:001",
        "surf:002",
        vec![no_config_exact, no_config_fuzzy],
    )
    .expect("no-config record");
    assert_eq!(unweighted_record, no_config_record);
    assert_eq!(
        serde_json::to_vec(&unweighted_record).expect("unweighted record serializes"),
        serde_json::to_vec(&no_config_record).expect("no-config record serializes")
    );
}

#[test]
fn posting_index_builds_hash_bound_value_frequency_table_deterministically() {
    let index = sample_index();
    let shuffled = sample_index_shuffled();
    let table = EntityValueFrequencyTable::from_posting_index(&index).expect("frequency table");
    let shuffled_table =
        EntityValueFrequencyTable::from_posting_index(&shuffled).expect("shuffled frequency table");

    assert_eq!(table.version, "canon_entity_value_frequency.v0");
    assert_eq!(
        table.source_posting_index_hash,
        index.content_hash().unwrap()
    );
    assert_eq!(table.surface_count, 11);
    assert_eq!(table.records, shuffled_table.records);
    assert_eq!(
        serde_json::to_vec(&table).expect("table serializes"),
        serde_json::to_vec(&shuffled_table).expect("shuffled table serializes")
    );
    validate_value_frequency_table_for_scoring(&table, &index).expect("table matches index");

    assert_eq!(table.count_for("firm_core", "rareco").unwrap(), 3);
    assert_eq!(table.count_for("firm_core", "wells fargo").unwrap(), 6);
    assert_eq!(table.count_for("firm_core", "wels fargo").unwrap(), 1);
}

#[test]
fn rare_floor_and_fuzzy_greater_side_choose_conservative_band() {
    let table = EntityValueFrequencyTable::from_posting_index(&sample_index()).unwrap();
    let config = band_config();

    let rare = table
        .adjustment_for_exact_value(&config, "firm_core", "rareco")
        .expect("rare exact adjustment");
    assert_eq!(rare.count, 3);
    assert_eq!(rare.band, EntityValueFrequencyBand::Rare);
    assert!(!rare.floor_applied);
    assert_eq!(rare.multiplier_basis_points, 15_000);

    let singleton_typo = table
        .adjustment_for_exact_value(&config, "firm_core", "siohban")
        .expect("singleton exact adjustment");
    assert_eq!(singleton_typo.count, 1);
    assert_eq!(singleton_typo.band, EntityValueFrequencyBand::Uncommon);
    assert!(singleton_typo.floor_applied);
    assert_eq!(singleton_typo.multiplier_basis_points, 10_000);

    let fuzzy = table
        .adjustment_for_fuzzy_values(&config, "firm_core", "wells fargo", "wels fargo")
        .expect("fuzzy adjustment uses more frequent side");
    assert_eq!(fuzzy.value, "wells fargo");
    assert_eq!(fuzzy.count, 6);
    assert_eq!(fuzzy.band, EntityValueFrequencyBand::Common);
    assert_eq!(fuzzy.multiplier_basis_points, 5_000);

    let unweighted = string_similarity_support_hit(StringSimilaritySupportRequest {
        namespace: "name",
        operator_id: "string_similarity:firm_core",
        reason_code: "similar_firm_core",
        metric: SimilarityMetric::JaroWinkler,
        left_value: "wells fargo",
        right_value: "wels fargo",
        score_cutoff: Some(score(1)),
        score_hint: None,
    })
    .expect("fuzzy support");
    let weighted = frequency_weighted_string_similarity_support_hit(
        FrequencyWeightedStringSimilaritySupportRequest {
            support: StringSimilaritySupportRequest {
                namespace: "name",
                operator_id: "string_similarity:firm_core",
                reason_code: "similar_firm_core",
                metric: SimilarityMetric::JaroWinkler,
                left_value: "wells fargo",
                right_value: "wels fargo",
                score_cutoff: Some(score(1)),
                score_hint: None,
            },
            adjustment: Some(fuzzy.clone()),
        },
    )
    .expect("weighted fuzzy support");
    assert_eq!(
        weighted.score_units,
        scale_score_units_by_frequency(unweighted.score_units, &fuzzy)
    );
    assert!(weighted.explanation.contains("band=common"));
    assert!(weighted.explanation.contains("count=6"));
}

#[test]
fn rare_exact_can_auto_link_while_common_exact_escrows_under_same_strategy() {
    let table = EntityValueFrequencyTable::from_posting_index(&sample_index()).unwrap();
    let config = band_config();
    let threshold = ScoreThreshold::new(score(8_500));

    let rare_adjustment = table
        .adjustment_for_exact_value(&config, "firm_core", "rareco")
        .expect("rare adjustment");
    let rare_hit = weighted_exact_hit("rareco", rare_adjustment);
    assert_eq!(rare_hit.score_units, score(9_000));
    let rare_decision = evaluate_candidate_score(
        &ScoredCandidate::new(
            "candidate:rare",
            "surf:rare-a",
            "surf:rare-b",
            rare_hit.score_units,
            false,
        ),
        threshold,
    );
    assert!(rare_decision.accepted);
    assert_eq!(rare_decision.reason, CandidateScoreDecisionReason::Accepted);

    let common_adjustment = table
        .adjustment_for_exact_value(&config, "firm_core", "wells fargo")
        .expect("common adjustment");
    let common_hit = weighted_exact_hit("wells fargo", common_adjustment);
    assert_eq!(common_hit.score_units, score(3_000));
    let common_decision = evaluate_candidate_score(
        &ScoredCandidate::new(
            "candidate:common",
            "surf:common-a",
            "surf:common-b",
            common_hit.score_units,
            false,
        ),
        threshold,
    );
    assert!(!common_decision.accepted);
    assert_eq!(
        common_decision.reason,
        CandidateScoreDecisionReason::BelowThreshold
    );
}

#[test]
fn frequency_weighting_cannot_elevate_past_hard_negative_evidence() {
    let table = EntityValueFrequencyTable::from_posting_index(&sample_index()).unwrap();
    let adjustment = table
        .adjustment_for_exact_value(&band_config(), "firm_core", "rareco")
        .expect("rare adjustment");
    let support = weighted_exact_hit("rareco", adjustment.clone());
    assert_eq!(support.score_units, score(9_000));

    let protected_token = EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        "guard",
        "protected_token_conflict",
        "protected_token_conflict",
        score(10_000),
        true,
        "protected token cannot-link",
    );
    assert_eq!(
        apply_value_frequency_adjustment(protected_token.clone(), &adjustment),
        protected_token,
        "frequency adjustment must ignore anti-merge hits"
    );
    let anchor_conflict = EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        "anchor",
        "anchor_conflict",
        "anchor_conflict",
        score(10_000),
        true,
        "trusted anchor conflict",
    );

    let record = build_edge_evidence_record(
        "surf:negative-a",
        "surf:negative-b",
        vec![support, protected_token, anchor_conflict],
    )
    .expect("record with support and hard negatives");
    assert!(record.has_hard_cannot_link);
    assert_eq!(record.pair_score_total, score(9_000));
    assert!(
        record
            .hits
            .iter()
            .any(|hit| hit.reason_code == "anchor_conflict")
    );
    assert!(
        record
            .hits
            .iter()
            .any(|hit| hit.reason_code == "protected_token_conflict")
    );

    let decision = evaluate_candidate_score(
        &ScoredCandidate::new(
            "candidate:blocked",
            "surf:negative-a",
            "surf:negative-b",
            record.pair_score_total,
            record.has_hard_cannot_link,
        ),
        ScoreThreshold::new(score(8_500)),
    );
    assert!(!decision.accepted);
    assert_eq!(
        decision.reason,
        CandidateScoreDecisionReason::HardCannotLink
    );
}

#[test]
fn stale_frequency_table_refuses_before_scoring() {
    let table = EntityValueFrequencyTable::from_posting_index(&sample_index()).unwrap();
    let changed_index = sample_index_with_extra_common_value();

    let refusal = validate_value_frequency_table_for_scoring(&table, &changed_index)
        .expect_err("stale frequency table refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "evidence");
    assert_eq!(refusal.detail["artifact"], "value_frequency_table");
    assert_eq!(refusal.detail["reason"], "stale_frequency_table");
    assert_eq!(refusal.detail["field"], "source_posting_index_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn strategy_frequency_band_params_are_integer_only_and_explicit() {
    assert!(
        EntityValueFrequencyBandConfig::from_operator_params(&BTreeMap::new())
            .unwrap()
            .is_none()
    );

    let mut params = band_params("blake3:table");
    let parsed = EntityValueFrequencyBandConfig::from_operator_params(&params)
        .expect("integer params parse")
        .expect("frequency config present");
    assert_eq!(parsed, band_config());

    params.insert(
        FREQUENCY_COMMON_MULTIPLIER_PARAM.to_string(),
        "0.5".to_string(),
    );
    let error = EntityValueFrequencyBandConfig::from_operator_params(&params)
        .expect_err("decimal multiplier refuses");
    assert_eq!(error.reason(), "invalid_frequency_config");
    assert_eq!(error.field(), FREQUENCY_COMMON_MULTIPLIER_PARAM);

    let mut partial = BTreeMap::new();
    partial.insert(
        FREQUENCY_TABLE_HASH_PARAM.to_string(),
        "blake3:table".to_string(),
    );
    let missing = EntityValueFrequencyBandConfig::from_operator_params(&partial)
        .expect_err("partial config refuses");
    assert_eq!(missing.reason(), "invalid_frequency_config");
    assert_eq!(missing.field(), FREQUENCY_MINIMUM_COUNT_PARAM);
}

fn weighted_exact_hit(
    value: &str,
    adjustment: canon::entity::evidence::value_frequency::EntityValueFrequencyAdjustment,
) -> EdgeEvidenceHit {
    frequency_weighted_exact_view_support_hit(FrequencyWeightedExactViewSupportRequest {
        support: ExactViewSupportRequest {
            namespace: "name",
            operator_id: "exact_view:firm_core",
            reason_code: "exact_firm_core",
            view_name: "firm_core",
            left_value: value,
            right_value: value,
            score_units: score(6_000),
        },
        adjustment: Some(adjustment),
    })
    .expect("weighted exact support")
}

fn band_config() -> EntityValueFrequencyBandConfig {
    EntityValueFrequencyBandConfig {
        minimum_count: 2,
        rare_max_count: 3,
        uncommon_max_count: 5,
        common_max_count: 8,
        rare_multiplier_basis_points: 15_000,
        uncommon_multiplier_basis_points: 10_000,
        common_multiplier_basis_points: 5_000,
        very_common_multiplier_basis_points: 2_500,
    }
}

fn band_params(table_hash: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            FREQUENCY_TABLE_HASH_PARAM.to_string(),
            table_hash.to_string(),
        ),
        (FREQUENCY_MINIMUM_COUNT_PARAM.to_string(), "2".to_string()),
        (FREQUENCY_RARE_MAX_COUNT_PARAM.to_string(), "3".to_string()),
        (
            FREQUENCY_UNCOMMON_MAX_COUNT_PARAM.to_string(),
            "5".to_string(),
        ),
        (
            FREQUENCY_COMMON_MAX_COUNT_PARAM.to_string(),
            "8".to_string(),
        ),
        (
            FREQUENCY_RARE_MULTIPLIER_PARAM.to_string(),
            "15000".to_string(),
        ),
        (
            FREQUENCY_UNCOMMON_MULTIPLIER_PARAM.to_string(),
            "10000".to_string(),
        ),
        (
            FREQUENCY_COMMON_MULTIPLIER_PARAM.to_string(),
            "5000".to_string(),
        ),
        (
            FREQUENCY_VERY_COMMON_MULTIPLIER_PARAM.to_string(),
            "2500".to_string(),
        ),
    ])
}

fn sample_index() -> EntityPostingIndex {
    EntityPostingIndex::build(&sample_surfaces(), EntityPostingBuildConfig::default())
        .expect("posting index builds")
}

fn sample_index_shuffled() -> EntityPostingIndex {
    let mut surfaces = sample_surfaces();
    surfaces.reverse();
    EntityPostingIndex::build(&surfaces, EntityPostingBuildConfig::default())
        .expect("posting index builds after row shuffle")
}

fn sample_index_with_extra_common_value() -> EntityPostingIndex {
    let mut surfaces = sample_surfaces();
    surfaces.push(surface("surf:012", "wells fargo"));
    EntityPostingIndex::build(&surfaces, EntityPostingBuildConfig::default())
        .expect("changed posting index builds")
}

fn sample_surfaces() -> Vec<EntityPostingSurface> {
    vec![
        surface("surf:001", "rareco"),
        surface("surf:002", "rareco"),
        surface("surf:003", "rareco"),
        surface("surf:004", "wells fargo"),
        surface("surf:005", "wells fargo"),
        surface("surf:006", "wells fargo"),
        surface("surf:007", "wells fargo"),
        surface("surf:008", "wells fargo"),
        surface("surf:009", "wells fargo"),
        surface("surf:010", "wels fargo"),
        surface("surf:011", "siohban"),
    ]
}

fn surface(surface_id: &str, firm_core: &str) -> EntityPostingSurface {
    EntityPostingSurface::new(surface_id).with_exact_view("firm_core", firm_core)
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
