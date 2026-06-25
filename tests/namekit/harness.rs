use canon::namekit::{
    ReasonCode, SimilarityScore,
    ids::{NgramSymbolTable, TokenSymbolTable},
    legal_suffix::{LegalSuffixProfile, analyze_legal_suffixes},
    normalize::{normalize_normality, normalize_openrefine_fingerprint},
    similarity::{
        SimilarityMetric, SimilarityOptions, SimilarityPath, normalized_similarity,
        score_units_from_ratio,
    },
    tfidf::{
        SortedNeighborhoodInput, SparseTfidfModel, TfidfInputSurface, TopKConfig,
        sorted_neighborhood_pairs_with_key,
    },
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;

const HARNESS_FIXTURE: &str = "tests/fixtures/namekit/harness/benchmark_contract.jsonl";

#[test]
fn namekit_harness_manifest_covers_fixture_and_benchmark_primitives() {
    let rows = harness_rows();
    assert!(rows.len() >= 8);

    for row in &rows {
        for key in [
            "case_id",
            "tier",
            "primitive",
            "profile",
            "raw_inputs",
            "expected_probe",
            "expected_reason_codes",
            "protected_tokens",
            "support_features",
            "anti_merge_features",
            "expected_non_equivalent",
            "ci_safe",
            "generated",
            "slow_bench",
        ] {
            assert!(row.get(key).is_some(), "{key} missing from {row}");
        }
        assert_reason_order(row);
    }

    let primitives = rows
        .iter()
        .map(|row| string(row, "primitive"))
        .collect::<BTreeSet<_>>();
    for primitive in [
        "ascii_fast_path",
        "unicode_path",
        "legal_suffix_stripping",
        "token_ngram_generation",
        "sparse_vector_construction",
        "metric_scoring",
        "pair_explosion_guard",
        "operator_stress_manifest",
    ] {
        assert!(
            primitives.contains(primitive),
            "missing primitive {primitive}"
        );
    }

    assert!(
        rows.iter()
            .any(|row| bool_field(row, "expected_non_equivalent"))
    );
    assert!(
        rows.iter()
            .any(|row| !strings(row, "protected_tokens").is_empty())
    );
    assert!(
        rows.iter()
            .any(|row| !array(row, "support_features").is_empty())
    );
    assert!(
        rows.iter()
            .any(|row| !array(row, "anti_merge_features").is_empty())
    );
}

#[test]
fn namekit_harness_ci_probes_are_deterministic() {
    let first = ci_probe_summary();
    let second = ci_probe_summary();
    assert_eq!(first, second);

    let case_ids = first
        .iter()
        .map(|row| string(row, "case_id"))
        .collect::<BTreeSet<_>>();
    for required in [
        "harness_ascii_metric_fast_path",
        "harness_unicode_normality_path",
        "harness_legal_suffix_strip",
        "harness_token_ngram_symbols",
        "harness_sparse_vector_topk",
        "harness_metric_scoring",
        "harness_pair_explosion_guard",
    ] {
        assert!(case_ids.contains(required), "missing CI probe {required}");
    }
}

#[test]
fn namekit_harness_operator_stress_is_opt_in_and_seeded() {
    let rows = harness_rows();
    let stress = rows
        .iter()
        .find(|row| string(row, "primitive") == "operator_stress_manifest")
        .expect("operator stress manifest row");

    assert!(!bool_field(stress, "ci_safe"));
    assert!(bool_field(stress, "generated"));
    assert!(bool_field(stress, "slow_bench"));
    assert_eq!(
        string(stress, "deterministic_seed"),
        "canon-namekit-harness-v0"
    );
    assert!(
        string(stress, "generator_command").contains("--ignored"),
        "operator stress command must be opt-in"
    );
    assert_eq!(number(stress, "row_count"), 10_000);
}

#[test]
fn namekit_harness_pair_explosion_probe_has_cap_diagnostics() {
    let row = harness_row("harness_pair_explosion_guard");
    let probe = probe_row(&row);
    assert_eq!(probe["cap_exceeded"], true);
    assert_eq!(
        probe["emitted_pair_count"],
        row["expected_probe"]["emitted_pair_count"]
    );
    assert_eq!(
        probe["uncapped_pair_count"],
        row["expected_probe"]["uncapped_pair_count"]
    );
    assert!(
        probe["uncapped_pair_count"]
            .as_u64()
            .expect("uncapped count")
            > probe["emitted_pair_count"].as_u64().expect("emitted count")
    );
}

fn ci_probe_summary() -> Vec<Value> {
    harness_rows()
        .into_iter()
        .filter(|row| bool_field(row, "ci_safe"))
        .map(|row| {
            let actual = probe_row(&row);
            assert_eq!(actual, row["expected_probe"], "probe changed for {row}");
            serde_json::json!({
                "case_id": string(&row, "case_id"),
                "primitive": string(&row, "primitive"),
                "probe": actual,
            })
        })
        .collect()
}

fn probe_row(row: &Value) -> Value {
    match string(row, "primitive").as_str() {
        "ascii_fast_path" => {
            let inputs = strings(row, "raw_inputs");
            let result = normalized_similarity(
                SimilarityMetric::LevenshteinNormalized,
                &inputs[0],
                &inputs[1],
                SimilarityOptions::default(),
            );
            serde_json::json!({
                "similarity_path": path_string(result.path),
                "score_units": result.score.map(SimilarityScore::as_scaled),
                "evidence_only": result.evidence_only,
            })
        }
        "unicode_path" => {
            let inputs = strings(row, "raw_inputs");
            let normalized = normalize_normality(&inputs[1]);
            let result = normalized_similarity(
                SimilarityMetric::LevenshteinNormalized,
                &inputs[0],
                &inputs[1],
                SimilarityOptions::default(),
            );
            serde_json::json!({
                "normality_normalized": normalized.normalized,
                "similarity_path": path_string(result.path),
                "score_units": result.score.map(SimilarityScore::as_scaled),
            })
        }
        "legal_suffix_stripping" => {
            let inputs = strings(row, "raw_inputs");
            let analysis = analyze_legal_suffixes(&inputs[0], LegalSuffixProfile::CmbsTenantLabel);
            serde_json::json!({
                "legal_basename": analysis.basename,
                "stripped_terms": analysis.stripped_terms,
            })
        }
        "token_ngram_generation" => {
            let inputs = strings(row, "raw_inputs");
            let normalized = normalize_openrefine_fingerprint(&inputs[0]);
            let tokens = normalized
                .normalized
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>();
            let token_table = TokenSymbolTable::from_tokens(tokens.clone());
            let ngrams = char_ngrams("sears", 3);
            let ngram_table = NgramSymbolTable::from_ngrams(ngrams.clone());
            serde_json::json!({
                "tokens": tokens,
                "token_symbols": token_table.entries.iter().map(|entry| entry.value.as_str()).collect::<Vec<_>>(),
                "ngrams": ngrams,
                "ngram_symbols": ngram_table.entries.iter().map(|entry| entry.value.as_str()).collect::<Vec<_>>(),
            })
        }
        "sparse_vector_construction" => {
            let model = SparseTfidfModel::build(&[
                TfidfInputSurface::tokenized("tenant-001", "sears roebuck", ["sears", "roebuck"]),
                TfidfInputSurface::tokenized("tenant-002", "sears llc", ["sears", "llc"]),
                TfidfInputSurface::tokenized("tenant-003", "sears auto", ["sears", "auto"]),
                TfidfInputSurface::tokenized(
                    "tenant-004",
                    "roebuck holdings",
                    ["roebuck", "holdings"],
                ),
                TfidfInputSurface::tokenized("tenant-005", "kmart", ["kmart"]),
            ]);
            let topk = model
                .top_k_for_surface("tenant-001", TopKConfig::new(3).with_candidate_cap(3))
                .expect("query exists");
            serde_json::json!({
                "document_count": model.document_count,
                "topk": topk.candidates.iter().map(|candidate| candidate.normalized_key.as_str()).collect::<Vec<_>>(),
                "cap_exceeded": topk.diagnostics.cap_exceeded,
            })
        }
        "metric_scoring" => {
            let inputs = strings(row, "raw_inputs");
            let result = normalized_similarity(
                SimilarityMetric::JaroWinkler,
                &inputs[0],
                &inputs[1],
                SimilarityOptions::new(Some(score(9_500)), Some(score(9_600))),
            );
            serde_json::json!({
                "similarity_path": path_string(result.path),
                "score_units": result.score.map(SimilarityScore::as_scaled),
                "passed_cutoff": result.passed_cutoff,
                "rounded_half_up": score_units_from_ratio(0.818_181_818).as_scaled(),
            })
        }
        "pair_explosion_guard" => {
            let inputs = [
                SortedNeighborhoodInput::new("tenant-000", "kmart"),
                SortedNeighborhoodInput::new("tenant-001", "sears"),
                SortedNeighborhoodInput::new("tenant-002", "sears"),
                SortedNeighborhoodInput::new("tenant-003", "sears auto"),
                SortedNeighborhoodInput::new("tenant-004", "sears holdings"),
                SortedNeighborhoodInput::new("tenant-005", "sears roebuck"),
            ];
            let result = sorted_neighborhood_pairs_with_key("tenant_core", &inputs, 4, Some(5));
            serde_json::json!({
                "window_size": result.diagnostics.window_size,
                "uncapped_pair_count": result.diagnostics.uncapped_pair_count,
                "emitted_pair_count": result.emitted_pair_count,
                "cap_exceeded": result.diagnostics.cap_exceeded,
            })
        }
        "operator_stress_manifest" => serde_json::json!({
            "manifest_only": true,
            "row_count": number(row, "row_count"),
        }),
        primitive => panic!("unexpected harness primitive {primitive}"),
    }
}

fn harness_rows() -> Vec<Value> {
    fs::read_to_string(HARNESS_FIXTURE)
        .unwrap_or_else(|error| panic!("read {HARNESS_FIXTURE}: {error}"))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("parse {HARNESS_FIXTURE} row {line}: {error}"))
        })
        .collect()
}

fn harness_row(case_id: &str) -> Value {
    harness_rows()
        .into_iter()
        .find(|row| string(row, "case_id") == case_id)
        .unwrap_or_else(|| panic!("missing harness case {case_id}"))
}

fn assert_reason_order(row: &Value) {
    let mut previous = None;
    for code in strings(row, "expected_reason_codes") {
        let order = ReasonCode::try_from(code.as_str())
            .unwrap_or_else(|error| panic!("{}: {error}", string(row, "case_id")))
            .order();
        if let Some(previous) = previous {
            assert!(
                previous <= order,
                "{} reason codes are not canonical",
                string(row, "case_id")
            );
        }
        previous = Some(order);
    }
}

fn char_ngrams(input: &str, width: usize) -> Vec<String> {
    let chars = input.chars().collect::<Vec<_>>();
    chars
        .windows(width)
        .map(|window| window.iter().collect::<String>())
        .collect()
}

fn score(units: u16) -> SimilarityScore {
    SimilarityScore::from_scaled(units).expect("fixture score units are in range")
}

fn path_string(path: SimilarityPath) -> &'static str {
    match path {
        SimilarityPath::AsciiBytes => "ascii_bytes",
        SimilarityPath::UnicodeChars => "unicode_chars",
    }
}

fn strings(row: &Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} must be array in {row}"))
        .iter()
        .map(|value| value.as_str().expect("string array entry").to_string())
        .collect()
}

fn array<'a>(row: &'a Value, key: &str) -> &'a [Value] {
    row[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} must be array in {row}"))
}

fn string(row: &Value, key: &str) -> String {
    row[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be string in {row}"))
        .to_string()
}

fn bool_field(row: &Value, key: &str) -> bool {
    row[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key} must be bool in {row}"))
}

fn number(row: &Value, key: &str) -> u64 {
    row[key]
        .as_u64()
        .unwrap_or_else(|| panic!("{key} must be unsigned integer in {row}"))
}
