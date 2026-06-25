use canon::namekit::{
    legal_suffix::{LegalSuffixProfile, analyze_legal_suffixes},
    normalize::normalize_normality,
    similarity::{SimilarityMetric, SimilarityOptions, normalized_similarity},
    tfidf::{SparseTfidfModel, TfidfInputSurface, TopKConfig},
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, fs};

const HARNESS_MANIFEST: &str = "tests/fixtures/namekit/harness/benchmark_manifest.json";

#[derive(Debug, Deserialize)]
struct HarnessManifest {
    version: String,
    ci_safe: bool,
    slow_benches_opt_in: bool,
    generated_fixtures: Vec<GeneratedFixture>,
    required_fixture_paths: Vec<String>,
    benchmark_cases: Vec<BenchmarkCase>,
}

#[derive(Debug, Deserialize)]
struct GeneratedFixture {
    path: String,
    seed: String,
    command: String,
}

#[derive(Debug, Deserialize)]
struct BenchmarkCase {
    id: String,
    primitive: String,
    fixture_path: String,
    category: String,
    max_rows: usize,
    deterministic_output: bool,
    ci_safe: bool,
    slow_opt_in: bool,
}

#[test]
fn namekit_harness_manifest_is_ci_safe_and_hand_auditable() {
    let manifest = harness_manifest();
    assert_eq!(manifest.version, "canon_namekit_harness.v0");
    assert!(manifest.ci_safe);
    assert!(manifest.slow_benches_opt_in);

    let ids = manifest
        .benchmark_cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), manifest.benchmark_cases.len());

    for generated in &manifest.generated_fixtures {
        assert!(!generated.path.trim().is_empty());
        assert!(!generated.seed.trim().is_empty());
        assert!(!generated.command.trim().is_empty());
    }

    for path in &manifest.required_fixture_paths {
        let metadata = fs::metadata(path).unwrap_or_else(|error| {
            panic!("required fixture path {path} must exist: {error}");
        });
        assert!(
            metadata.len() <= 16 * 1024,
            "{path} must stay small enough for review"
        );
    }
}

#[test]
fn namekit_benchmark_harness_covers_required_primitive_categories() {
    let manifest = harness_manifest();
    let categories = manifest
        .benchmark_cases
        .iter()
        .map(|case| case.category.as_str())
        .collect::<BTreeSet<_>>();

    for required in [
        "ascii_fast_path",
        "unicode_path",
        "legal_suffix_stripping",
        "token_ngram_generation",
        "sparse_vector_construction",
        "metric_scoring",
        "common_token_stress",
    ] {
        assert!(categories.contains(required), "missing category {required}");
    }

    for case in &manifest.benchmark_cases {
        assert!(
            case.deterministic_output,
            "{} must be deterministic",
            case.id
        );
        assert!(
            case.ci_safe || case.slow_opt_in,
            "{} needs explicit opt-in",
            case.id
        );
        assert!(
            case.max_rows <= 8,
            "{} must stay a small CI fixture",
            case.id
        );
        assert!(
            fs::metadata(&case.fixture_path).is_ok(),
            "{} fixture path must exist",
            case.id
        );
        assert!(!case.primitive.trim().is_empty());
    }
}

#[test]
fn namekit_source_parity_fixtures_expose_review_and_antimerge_fields() {
    let manifest = harness_manifest();
    let mut saw_non_equivalent = false;
    let mut saw_protected_tokens = false;
    let mut saw_reason_codes = false;

    for path in manifest
        .required_fixture_paths
        .iter()
        .filter(|path| path.ends_with(".jsonl"))
    {
        for row in jsonl_rows(path) {
            if row.get("expected_non_equivalent").is_some() {
                saw_non_equivalent = true;
            }
            if row
                .get("protected_tokens")
                .and_then(Value::as_array)
                .is_some_and(|tokens| !tokens.is_empty())
            {
                saw_protected_tokens = true;
            }
            if row
                .get("expected_reason_codes")
                .and_then(Value::as_array)
                .is_some_and(|codes| !codes.is_empty())
            {
                saw_reason_codes = true;
            }
        }
    }

    assert!(saw_non_equivalent);
    assert!(saw_protected_tokens);
    assert!(saw_reason_codes);
}

#[test]
fn namekit_harness_replays_implemented_primitives_deterministically() {
    let first = primitive_replay_signature();
    let second = primitive_replay_signature();
    assert_eq!(first, second);
    assert_eq!(
        first,
        [
            "normalize:cafe societe:cafe societe",
            "legal_suffix:some big pharma:ltd,llc",
            "metric:9611:ascii=true",
            "tfidf:sears-rare:2:cap=true"
        ]
    );
}

#[test]
fn common_token_mini_stress_is_bounded_without_large_fixtures() {
    let surfaces = [
        TfidfInputSurface::tokenized("s0", "sears roebuck", ["sears", "roebuck"]),
        TfidfInputSurface::tokenized("s1", "sears auto", ["sears", "auto"]),
        TfidfInputSurface::tokenized("s2", "sears center", ["sears", "center"]),
        TfidfInputSurface::tokenized("s3", "sears outlet", ["sears", "outlet"]),
        TfidfInputSurface::tokenized("s4", "sears holdings", ["sears", "holdings"]),
        TfidfInputSurface::tokenized("s5", "kmart store", ["kmart", "store"]),
        TfidfInputSurface::tokenized("s6", "pnc bank", ["pnc", "bank"]),
        TfidfInputSurface::tokenized("s7", "pnc capital", ["pnc", "capital"]),
    ];
    let model = SparseTfidfModel::build(&surfaces);
    let topk = model
        .top_k_for_surface("s0", TopKConfig::new(3).with_candidate_cap(3))
        .expect("query row exists");

    assert_eq!(model.document_count, 8);
    assert_eq!(topk.diagnostics.uncapped_candidate_count, 4);
    assert_eq!(topk.diagnostics.capped_candidate_count, 1);
    assert!(topk.diagnostics.cap_exceeded);
    assert_eq!(topk.candidates.len(), 3);
}

fn primitive_replay_signature() -> Vec<String> {
    let normalized = normalize_normality("Café   Société");
    let suffix = analyze_legal_suffixes(
        "Some Big Pharma LLC Ltd",
        LegalSuffixProfile::CmbsTenantLabel,
    );
    let metric = normalized_similarity(
        SimilarityMetric::JaroWinkler,
        "martha",
        "marhta",
        SimilarityOptions::default(),
    );
    let model = SparseTfidfModel::build(&[
        TfidfInputSurface::tokenized("sears-rare", "sears roebuck", ["sears", "roebuck"]),
        TfidfInputSurface::tokenized("sears-common", "sears llc", ["sears", "llc"]),
        TfidfInputSurface::tokenized(
            "sears-auto",
            "sears auto center",
            ["sears", "auto", "center"],
        ),
        TfidfInputSurface::tokenized(
            "roebuck-holdings",
            "roebuck holdings",
            ["roebuck", "holdings"],
        ),
    ]);
    let topk = model
        .top_k_for_surface("sears-rare", TopKConfig::new(2).with_candidate_cap(2))
        .expect("query row exists");

    vec![
        format!(
            "normalize:{}:{}",
            normalized.normalized, normalized.fingerprint
        ),
        format!(
            "legal_suffix:{}:{}",
            suffix.basename,
            suffix.stripped_terms.join(",")
        ),
        format!(
            "metric:{}:ascii={}",
            metric.score.expect("metric score").as_scaled(),
            matches!(
                metric.path,
                canon::namekit::similarity::SimilarityPath::AsciiBytes
            )
        ),
        format!(
            "tfidf:{}:{}:cap={}",
            topk.query_surface_id,
            topk.candidates.len(),
            topk.diagnostics.cap_exceeded
        ),
    ]
}

fn harness_manifest() -> HarnessManifest {
    let raw = fs::read_to_string(HARNESS_MANIFEST).expect("namekit harness manifest");
    serde_json::from_str(&raw).expect("namekit harness manifest JSON")
}

fn jsonl_rows(path: &str) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("fixture {path} must be readable: {error}"))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("fixture row JSON"))
        .collect()
}
