#![forbid(unsafe_code)]

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const CI_TIERS: &str = include_str!("../fixtures/entity/perf/ci_tiers.json");

#[derive(Debug, Deserialize)]
struct PerfCiTiers {
    schema_version: String,
    doc: String,
    normal_ci: Vec<PerfTierCase>,
    ignored_stress: Vec<StressTierCase>,
    operator_benchmarks: Vec<OperatorBenchmarkCase>,
    release_claims: ReleaseClaimRules,
    prohibited_runtime_dependencies: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PerfTierCase {
    id: String,
    tier: String,
    command: String,
    gate_ids: Vec<String>,
    assertions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct StressTierCase {
    id: String,
    tier: String,
    command: String,
    seed: u64,
    telemetry_out: String,
    assertions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OperatorBenchmarkCase {
    id: String,
    tier: String,
    command: String,
    telemetry_out: String,
    requires_recorded_artifact: bool,
}

#[derive(Debug, Deserialize)]
struct ReleaseClaimRules {
    g11_requires_artifacts: Vec<String>,
    g12_requires_artifacts: Vec<String>,
    waiver_required_fields: Vec<String>,
}

#[test]
fn entity_perf_ci_small_contract_defines_cheap_structural_gates() {
    let tiers = tiers();
    assert_eq!(tiers.schema_version, "canon.entity.perf_ci_tiers.v0");
    assert_eq!(tiers.doc, "docs/ENTITY_EVALS_AND_PERFORMANCE.md");

    let cases = tiers
        .normal_ci
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    for id in [
        "entity_perf_ci_small_exact_bucket_compactness",
        "entity_perf_ci_small_cache_invalidation",
        "entity_perf_ci_small_topk_ordering",
        "entity_perf_ci_small_budget_refusal",
    ] {
        let case = cases.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(case.tier, "small-ci");
        assert!(case.command.starts_with("cargo test --test "));
        assert!(!case.command.contains("--ignored"));
        assert!(!case.command.contains("bench"));
        assert!(!case.gate_ids.is_empty(), "{id} must name gates");
        assert!(
            case.assertions
                .iter()
                .all(|assertion| !assertion.trim().is_empty() && !assertion.contains("exists")),
            "{id} must assert behavior, not artifact existence"
        );
    }

    let covered_gates = tiers
        .normal_ci
        .iter()
        .flat_map(|case| case.gate_ids.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    for gate in ["G05", "G10", "G11", "G12"] {
        assert!(covered_gates.contains(gate), "small CI omits {gate}");
    }
}

#[test]
fn entity_perf_ci_stress_tiers_are_ignored_seeded_and_telemetry_backed() {
    let tiers = tiers();
    let stress = tiers
        .ignored_stress
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();

    for id in ["cmbs_500k_stress", "entity_500k_unique_stress"] {
        let case = stress.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(case.tier, "stress");
        assert!(case.command.contains("--ignored"));
        assert!(case.command.contains(id));
        assert!(case.seed > 0);
        assert!(case.telemetry_out.ends_with("/telemetry.json"));
        assert!(
            case.assertions
                .iter()
                .any(|assertion| assertion.contains("deterministic")
                    || assertion.contains("candidate")
                    || assertion.contains("exact_bucket")),
            "{id} needs structural assertions"
        );
    }
}

#[test]
fn entity_perf_operator_commands_emit_telemetry_without_network_or_model_runtime() {
    let tiers = tiers();
    let prohibited = tiers
        .prohibited_runtime_dependencies
        .iter()
        .map(|item| item.as_str())
        .collect::<BTreeSet<_>>();
    for required in [
        "network",
        "frontier_model_call",
        "runtime_model_download",
        "python_ml_runtime",
        "general_ml_framework",
        "dense_embedding_service",
    ] {
        assert!(prohibited.contains(required), "missing {required}");
    }

    for case in &tiers.operator_benchmarks {
        assert_eq!(case.tier, "stress");
        assert!(case.command.starts_with("canon entity eval "));
        assert!(case.command.contains("--telemetry-out "));
        assert!(case.command.contains(&case.telemetry_out));
        assert!(case.telemetry_out.ends_with("/telemetry.json"));
        assert!(case.requires_recorded_artifact);
        assert_forbidden_runtime_words_absent(&case.command);
    }
}

#[test]
fn entity_perf_release_claims_require_recorded_artifacts_or_waivers() {
    let tiers = tiers();
    let operators = tiers
        .operator_benchmarks
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();

    for artifact in &tiers.release_claims.g11_requires_artifacts {
        assert!(
            operators.contains(artifact.as_str()),
            "G11 references missing operator artifact {artifact}"
        );
    }
    for artifact in &tiers.release_claims.g12_requires_artifacts {
        assert!(
            operators.contains(artifact.as_str()),
            "G12 references missing operator artifact {artifact}"
        );
    }
    for field in [
        "gate_id",
        "reason",
        "operator",
        "expires_at",
        "telemetry_run_id",
        "next_command",
    ] {
        assert!(
            tiers
                .release_claims
                .waiver_required_fields
                .iter()
                .any(|candidate| candidate == field),
            "waiver must require {field}"
        );
    }
}

#[test]
#[ignore = "operator stress command contract only; generator lands in later ENT-P14 beads"]
fn cmbs_500k_stress_command_is_documented() {
    let tiers = tiers();
    let case = tiers
        .ignored_stress
        .iter()
        .find(|case| case.id == "cmbs_500k_stress")
        .expect("cmbs stress command");
    assert!(case.command.contains("--ignored cmbs_500k_stress"));
    assert!(case.telemetry_out.contains("stress-cmbs-500k"));
}

#[test]
#[ignore = "operator stress command contract only; generator lands in later ENT-P14 beads"]
fn entity_500k_unique_stress_command_is_documented() {
    let tiers = tiers();
    let case = tiers
        .ignored_stress
        .iter()
        .find(|case| case.id == "entity_500k_unique_stress")
        .expect("unique stress command");
    assert!(case.command.contains("--ignored entity_500k_unique_stress"));
    assert!(case.telemetry_out.contains("stress-unique-500k"));
}

fn tiers() -> PerfCiTiers {
    serde_json::from_str(CI_TIERS).expect("perf CI tier fixture parses")
}

fn assert_forbidden_runtime_words_absent(command: &str) {
    for forbidden in [
        "http://",
        "https://",
        "curl ",
        "pip ",
        "python ",
        "openai",
        "embedding",
        "model-download",
    ] {
        assert!(
            !command.contains(forbidden),
            "operator command must not include {forbidden}: {command}"
        );
    }
}
