use canon::entity::{
    block::{
        BlockCandidateBudgetConfig,
        declared::{Blocking, generate},
    },
    prepare::{PreparedSurfaceRecord, load_prepare_profile},
    profile::EntityProfileDocument,
};
use serde_json::json;

fn profile(operators: serde_json::Value) -> EntityProfileDocument {
    let mut profile = load_prepare_profile("instrument_identity").unwrap();
    profile.blocking = Some(serde_json::from_value(json!({"operators": operators})).unwrap());
    profile.validate().unwrap();
    profile
}

fn surface(
    id: &str,
    title: &str,
    cusip: &str,
    maturity: &str,
    rate: &str,
) -> PreparedSurfaceRecord {
    serde_json::from_value(json!({
        "surface_id": id, "profile_id": "instrument_identity", "surface_key": id,
        "primary_surface": title,
        "normalized_views": {
            "core": {"value": title, "reason_codes": ["surface_id_view"]},
            "cusip": {"value": cusip, "reason_codes": []},
            "instrument_maturity": {"value": maturity, "reason_codes": []},
            "annualized_rate": {"value": rate, "reason_codes": []}
        },
        "exact_lookup": {"status": "unresolved", "canonical_id": null, "canonical_type": null,
            "rule_id": null, "matched_input": null},
        "raw_variants": [title], "alias_surfaces": [], "mention_surfaces": [],
        "anchors": [], "row_count": 1, "deal_count": 0, "provenance_samples": []
    }))
    .unwrap()
}

fn budget() -> BlockCandidateBudgetConfig {
    BlockCandidateBudgetConfig::new(100, 1000, 1000)
}

#[test]
fn declared_exact_and_composite_keys_retrieve_without_claiming_support() {
    let profile = profile(json!([
        {"op": "exact_view", "view": "cusip"},
        {"op": "composite_key", "views": ["instrument_maturity", "annualized_rate"]}
    ]));
    let mut surfaces = vec![
        surface("a", "unrelated first", "ID1", "2030", "5"),
        surface("b", "unrelated second", "ID1", "2040", "7"),
        surface("c", "unrelated third", "ID2", "2030", "5"),
        surface("d", "missing first", "N/A", "", ""),
        surface("e", "missing second", "N/A", "", ""),
    ];
    let first = generate(&profile, &surfaces, budget()).unwrap();
    let mut pairs = first
        .candidates
        .iter()
        .map(|pair| {
            (
                pair.left_surface_id.as_str(),
                pair.right_surface_id.as_str(),
            )
        })
        .collect::<Vec<_>>();
    pairs.sort();
    assert_eq!(pairs, [("a", "b"), ("a", "c")]);
    assert!(
        first
            .candidates
            .iter()
            .all(|pair| pair.candidate_score_hint == 0)
    );
    surfaces.reverse();
    assert_eq!(first, generate(&profile, &surfaces, budget()).unwrap());
}

#[test]
fn partition_is_applied_before_top_k_and_missing_keys_do_not_match() {
    let profile = profile(json!([{"op": "ngram_topk", "view": "core", "k": 1,
        "candidate_cap": 1, "partition_by": ["instrument_maturity"]}]));
    let surfaces = vec![
        surface("a", "alphabet soup", "a", "2030", "5"),
        surface("b", "alphabet soup", "b", "2040", "5"),
        surface("c", "alphabet stew", "c", "2030", "5"),
        surface("d", "alphabet soup", "d", "", "5"),
        surface("e", "alphabet soup", "e", "", "5"),
    ];
    let result = generate(&profile, &surfaces, budget()).unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(
        (
            &*result.candidates[0].left_surface_id,
            &*result.candidates[0].right_surface_id
        ),
        ("a", "c")
    );
}

#[test]
fn declared_anchor_and_tuple_boundaries_are_exact() {
    let profile = profile(json!([
        {"op": "exact_anchor", "field": "figi"},
        {"op": "composite_key", "views": ["instrument_maturity", "annualized_rate"]}
    ]));
    let mut a = surface("a", "first", "a", "x+y", "z");
    let mut b = surface("b", "second", "b", "x", "y+z");
    let c = surface("c", "third", "c", "x", "y+z");
    a.anchors = vec![canon::entity::prepare::PreparedAnchor {
        namespace: "figi".into(),
        value: "F1".into(),
        field: "figi".into(),
    }];
    b.anchors = a.anchors.clone();
    let result = generate(&profile, &[a, b, c], budget()).unwrap();
    assert_eq!(result.candidates.len(), 2);
    assert_eq!(
        result
            .candidates
            .iter()
            .find(|pair| pair.left_surface_id == "a")
            .unwrap()
            .block_hits[0]
            .operator_id,
        "exact_anchor:figi"
    );
    assert_eq!(
        result
            .candidates
            .iter()
            .find(|pair| pair.left_surface_id == "b")
            .unwrap()
            .block_hits[0]
            .operator_id,
        "composite_key:instrument_maturity+annualized_rate"
    );
}

#[test]
fn declared_validation_rejects_unknown_fields_operators_and_duplicate_ids() {
    assert!(serde_json::from_value::<Blocking>(json!({"operators": [{"op": "guess"}]})).is_err());
    assert!(
        serde_json::from_value::<Blocking>(
            json!({"operators": [{"op": "exact_view", "view": "cusip", "typo": true}]})
        )
        .is_err()
    );
    let mut profile = load_prepare_profile("instrument_identity").unwrap();
    for operators in [
        json!([{"op": "exact_view", "view": "unknown"}]),
        json!([{"op": "exact_anchor", "field": "unknown"}]),
        json!([{"op": "exact_view", "view": "cusip"}, {"op": "exact_view", "view": "cusip"}]),
        json!([{"op": "ngram_topk", "view": "core", "k": 1, "candidate_cap": 1, "partition_by": ["unknown"]}]),
        json!([{"op": "composite_key", "views": []}]),
    ] {
        profile.blocking = Some(serde_json::from_value(json!({"operators": operators})).unwrap());
        assert!(profile.validate().is_err());
    }
}

#[test]
fn exact_retrieval_stops_at_first_budget_crossing() {
    let profile = profile(json!([{"op": "exact_view", "view": "cusip"}]));
    let surfaces = (0..100)
        .map(|id| surface(&format!("{id:03}"), "same", "ID", "2030", "5"))
        .collect::<Vec<_>>();
    let refusal = generate(
        &profile,
        &surfaces,
        BlockCandidateBudgetConfig::new(100, 3, 3),
    )
    .unwrap_err();
    assert_eq!(refusal.detail["stopped_early"], true);
    assert_eq!(refusal.detail["observed_at_stop"], 4);
    assert_eq!(refusal.detail["surfaces_visited"], 1);
    assert_eq!(refusal.detail["partial_candidate_artifact_written"], false);
}

#[test]
fn optional_df_filter_is_opt_in_and_reports_real_candidate_counts() {
    let surfaces = vec![
        surface("a", "same", "1", "2030", "5"),
        surface("b", "same", "2", "2030", "5"),
    ];
    let unfiltered = profile(json!([{"op":"ngram_topk", "view":"core", "k":1, "candidate_cap":1}]));
    assert_eq!(
        generate(&unfiltered, &surfaces, budget())
            .unwrap()
            .candidates
            .len(),
        1
    );
    let filtered = profile(
        json!([{"op":"ngram_topk", "view":"core", "k":1, "candidate_cap":1, "max_ngram_df":1}]),
    );
    assert!(
        generate(&filtered, &surfaces, budget())
            .unwrap()
            .candidates
            .is_empty()
    );
}

#[test]
fn profile_name_does_not_select_retrieval_behavior() {
    let original = profile(json!([{"op":"exact_view", "view":"cusip"}]));
    let mut renamed = original.clone();
    renamed.profile = "unrelated_domain".into();
    let surfaces = vec![
        surface("a", "one", "ID", "2030", "5"),
        surface("b", "two", "ID", "2030", "5"),
    ];
    assert_eq!(
        generate(&original, &surfaces, budget()).unwrap(),
        generate(&renamed, &surfaces, budget()).unwrap()
    );
}

#[test]
fn declared_alias_patch_requires_known_surface_and_retains_provenance() {
    let surfaces = vec![
        surface("a", "one", "1", "2030", "5"),
        surface("b", "two", "2", "2030", "5"),
    ];
    let declared = profile(
        json!([{"op":"alias_patch_match", "pairs":[{"patch_id":"review-1", "left_surface_id":"a", "right_surface_id":"b"}]}]),
    );
    let result = generate(&declared, &surfaces, budget()).unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(
        result.candidates[0].block_hits[0].operator_id,
        "alias_patch_match"
    );
    assert!(generate(&declared, &surfaces[..1], budget()).is_err());
}

#[test]
fn strategy_tunes_only_declared_similarity_limits() {
    use canon::entity::block::{BlockOperatorTuning, default_block_runtime_config};
    let original = profile(json!([{"op":"ngram_topk", "view":"core", "k":5, "candidate_cap":5}]));
    let mut runtime = default_block_runtime_config();
    runtime.operator_overrides.insert(
        "ngram_topk:core".into(),
        BlockOperatorTuning {
            k: Some(1),
            candidate_cap: Some(1),
        },
    );
    let tuned = runtime.tuned_profile(&original).unwrap();
    assert_eq!(
        serde_json::to_value(tuned.blocking).unwrap()["operators"][0]["k"],
        1
    );
    runtime
        .operator_overrides
        .insert("missing".into(), BlockOperatorTuning::default());
    assert!(runtime.tuned_profile(&original).is_err());
}
