use canon::namekit::tfidf::{
    COMMON_TOKEN_MAX_IDF_UNITS, RARE_TOKEN_MIN_IDF_UNITS, SortedNeighborhoodInput,
    SparseTfidfModel, TfidfEvidenceClass, TfidfTermKey, TopKConfig, idf_units,
    sorted_neighborhood_pairs_with_key, tf_units,
};

fn sears_model() -> SparseTfidfModel {
    SparseTfidfModel::build(&[
        canon::namekit::tfidf::TfidfInputSurface::tokenized(
            "tenant-001",
            "sears roebuck",
            ["sears", "roebuck"],
        ),
        canon::namekit::tfidf::TfidfInputSurface::tokenized(
            "tenant-002",
            "sears llc",
            ["sears", "llc"],
        ),
        canon::namekit::tfidf::TfidfInputSurface::tokenized(
            "tenant-003",
            "sears auto",
            ["sears", "auto"],
        ),
        canon::namekit::tfidf::TfidfInputSurface::tokenized(
            "tenant-004",
            "roebuck holdings",
            ["roebuck", "holdings"],
        ),
        canon::namekit::tfidf::TfidfInputSurface::tokenized(
            "tenant-005",
            "pnc bank",
            ["pnc", "bank"],
        ),
    ])
}

#[test]
fn namekit_tfidf_sparse() {
    let model = sears_model();

    let term_keys = model
        .terms
        .iter()
        .map(|term| (&term.key.key, term.id.as_u32()))
        .collect::<Vec<_>>();
    assert_eq!(
        term_keys,
        [
            (&"auto".to_string(), 0),
            (&"bank".to_string(), 1),
            (&"holdings".to_string(), 2),
            (&"llc".to_string(), 3),
            (&"pnc".to_string(), 4),
            (&"roebuck".to_string(), 5),
            (&"sears".to_string(), 6),
        ]
    );

    let roebuck = model.term_by_key(&TfidfTermKey::token("roebuck")).unwrap();
    let sears = model.term_by_key(&TfidfTermKey::token("sears")).unwrap();
    assert!(roebuck.idf_units > sears.idf_units);

    let row = model.row("tenant-001").unwrap();
    assert_eq!(
        row.terms
            .iter()
            .map(|term| term.term_id.as_u32())
            .collect::<Vec<_>>(),
        [roebuck.id.as_u32(), sears.id.as_u32()]
    );
    assert!(row.norm_units > 0);

    let topk = model
        .top_k_for_surface("tenant-001", TopKConfig::new(3))
        .unwrap();
    assert_eq!(
        topk.candidates
            .iter()
            .map(|candidate| candidate.surface_id.as_str())
            .collect::<Vec<_>>(),
        ["tenant-004", "tenant-003", "tenant-002"]
    );
    assert_eq!(
        topk.candidates[0].evidence_class,
        TfidfEvidenceClass::RareTokenSupport
    );
    assert!(topk.candidates[0].score_units > topk.candidates[1].score_units);
    assert_eq!(topk.diagnostics.uncapped_candidate_count, 3);
    assert!(!topk.diagnostics.cap_exceeded);
}

#[test]
fn tfidf_rare_token_weighting() {
    assert_eq!(tf_units(1), 1_000);
    assert_eq!(tf_units(3), 3_000);
    assert_eq!(tf_units(99), 3_000);

    let common = idf_units(8, 8);
    let repeated_rare = idf_units(8, 2);
    let singleton = idf_units(8, 1);
    assert!(common <= COMMON_TOKEN_MAX_IDF_UNITS);
    assert!(repeated_rare >= RARE_TOKEN_MIN_IDF_UNITS);
    assert!(singleton > repeated_rare);

    let model = sears_model();
    let topk = model
        .top_k_for_surface("tenant-001", TopKConfig::new(3))
        .unwrap();
    let rare = topk
        .candidates
        .iter()
        .find(|candidate| candidate.surface_id == "tenant-004")
        .unwrap();
    let common_only = topk
        .candidates
        .iter()
        .find(|candidate| candidate.surface_id == "tenant-002")
        .unwrap();
    assert_eq!(rare.evidence_class, TfidfEvidenceClass::RareTokenSupport);
    assert_eq!(
        common_only.evidence_class,
        TfidfEvidenceClass::CommonTokenOnly
    );
    assert!(rare.score_units > common_only.score_units);
    assert_eq!(rare.reasons()[0].code.as_str(), "rare_token_support");
    assert_eq!(
        common_only.reasons()[0].code.as_str(),
        "common_token_downweighted"
    );
}

#[test]
fn tfidf_topk_cap_diagnostics_are_deterministic() {
    let model = sears_model();
    let capped = model
        .top_k_for_surface("tenant-001", TopKConfig::new(5).with_candidate_cap(2))
        .unwrap();

    assert_eq!(capped.candidates.len(), 2);
    assert_eq!(capped.diagnostics.uncapped_candidate_count, 3);
    assert_eq!(capped.diagnostics.capped_candidate_count, 1);
    assert!(capped.diagnostics.cap_exceeded);
    assert_eq!(
        capped
            .candidates
            .iter()
            .map(|candidate| candidate.surface_id.as_str())
            .collect::<Vec<_>>(),
        ["tenant-004", "tenant-003"]
    );
}

#[test]
fn sorted_neighborhood_contract_has_cap_diagnostics() {
    let inputs = [
        SortedNeighborhoodInput::new("tenant-003", "sears auto"),
        SortedNeighborhoodInput::new("tenant-001", "sears roebuck"),
        SortedNeighborhoodInput::new("tenant-004", "sears roebuck outlet"),
        SortedNeighborhoodInput::new("tenant-002", "sears roebuck"),
    ];

    let result = sorted_neighborhood_pairs_with_key("normalized_key", &inputs, 3, Some(3));
    assert_eq!(result.diagnostics.key_name, "normalized_key");
    assert_eq!(result.diagnostics.window_size, 3);
    assert_eq!(result.diagnostics.uncapped_pair_count, 5);
    assert_eq!(result.diagnostics.capped_pair_count, 2);
    assert!(result.diagnostics.cap_exceeded);
    assert_eq!(
        result
            .pairs
            .iter()
            .map(|pair| (
                pair.left_surface_id.as_str(),
                pair.right_surface_id.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("tenant-001", "tenant-003"),
            ("tenant-002", "tenant-003"),
            ("tenant-001", "tenant-002"),
        ]
    );
}
