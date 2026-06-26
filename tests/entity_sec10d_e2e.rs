#![forbid(unsafe_code)]

use canon::entity::{
    EntityArtifactHeader, EntityArtifactReference,
    apply::{
        APPLY_CANONICAL_FIELDS, ApplyRegistryReference, ApplySafetyCheck,
        SEC10D_ORG_FIELD_SUFFIXES, Sec10dOrgApplyResolution, Sec10dOrgApplyStreamRequest,
        run_sec10d_org_apply_streaming,
    },
    artifact_chain::{EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage},
    audit::{EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite, run_entity_audit},
    review::{
        ReviewExportInclude, ReviewQueueRequest, build_review_queue_artifact,
        render_review_queue_csv,
    },
    run::{EntityRunRequest, run_entity_workbench},
    solve::SolveArtifact,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const MANIFEST_PATH: &str = "tests/fixtures/entity/e2e/sec10d_regab/manifest.json";

#[derive(Debug, Deserialize)]
struct E2eManifest {
    schema_version: String,
    profile_id: String,
    fixture_paths: FixturePaths,
    expected_counts: BTreeMap<String, u64>,
    selected_apply_source_row_ids: Vec<String>,
    boundary_expectations: Vec<BoundaryExpectation>,
    must_remain_distinct: Vec<DistinctPair>,
    assertions: Vec<BehaviorAssertion>,
    runtime_forbidden: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FixturePaths {
    org_mentions_csv: String,
    org_mentions_jsonl: String,
    profile_strategy_yaml: String,
    registry_snapshot: String,
    expected_summary: String,
    selected_snowflake_enrichment_jsonl: String,
}

#[derive(Debug, Deserialize)]
struct BoundaryExpectation {
    id: String,
    surface: String,
    prepare_status: String,
    canonical_id: Option<String>,
    apply_status: String,
}

#[derive(Debug, Deserialize)]
struct DistinctPair {
    left: String,
    right: String,
}

#[derive(Debug, Deserialize)]
struct BehaviorAssertion {
    id: String,
    behavior: String,
}

#[derive(Debug, Deserialize)]
struct ExpectedSummary {
    exact_resolved_surfaces: Vec<ResolvedSurface>,
    unresolved_surfaces: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ResolvedSurface {
    org_name: String,
    canonical_id: String,
    rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SurfaceLookup {
    status: String,
    canonical_id: Option<String>,
}

#[test]
fn entity_sec10d_e2e_manifest_names_behavioral_regressions() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, "canon.entity.sec10d_regab_e2e.v0");
    assert_eq!(manifest.profile_id, "regab_firm_identity");

    for path in [
        &manifest.fixture_paths.org_mentions_csv,
        &manifest.fixture_paths.org_mentions_jsonl,
        &manifest.fixture_paths.profile_strategy_yaml,
        &manifest.fixture_paths.registry_snapshot,
        &manifest.fixture_paths.expected_summary,
        &manifest.fixture_paths.selected_snowflake_enrichment_jsonl,
    ] {
        assert!(
            repo_path(path).exists(),
            "fixture path should exist: {path}"
        );
    }

    let assertion_ids = manifest
        .assertions
        .iter()
        .map(|assertion| assertion.id.as_str())
        .collect::<BTreeSet<_>>();
    for id in ["REGAB-I001", "REGAB-I002", "REGAB-I003", "REGAB-I004"] {
        assert!(assertion_ids.contains(id), "missing assertion {id}");
    }
    for assertion in &manifest.assertions {
        assert!(
            !assertion.behavior.trim().is_empty() && !assertion.behavior.contains("exists"),
            "{} must describe behavior, not file presence",
            assertion.id
        );
    }

    let forbidden = manifest
        .runtime_forbidden
        .iter()
        .map(|term| term.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for term in [
        "frontier model call",
        "network call",
        "runtime model download",
        "python ml runtime",
        "unsafe rust",
    ] {
        assert!(forbidden.contains(term), "missing forbidden runtime {term}");
    }
}

#[test]
fn entity_sec10d_e2e_small_runs_workbench_audit_review_and_apply() {
    let manifest = manifest();
    let expected = expected_summary(&manifest);
    let temp = tempfile::tempdir().expect("tempdir");
    let work_dir = temp.path().join("work");

    let run = run_entity_workbench(EntityRunRequest {
        rows: &repo_path(&manifest.fixture_paths.org_mentions_csv),
        profile: &manifest.profile_id,
        strategy: &repo_path(&manifest.fixture_paths.profile_strategy_yaml),
        registry: &repo_path(&manifest.fixture_paths.registry_snapshot),
        work_dir: &work_dir,
    })
    .expect("sec10d e2e workbench run succeeds")
    .artifact;

    assert_count(&run.summary.counts, &manifest, "row_count");
    assert_count(&run.summary.counts, &manifest, "prepared_surfaces");
    assert_count(&run.summary.counts, &manifest, "exact_resolved_surfaces");
    assert_count(&run.summary.counts, &manifest, "candidate_pairs");
    assert_count(&run.summary.counts, &manifest, "edge_records");
    assert_count(&run.summary.counts, &manifest, "solved_entities");
    assert_eq!(run.summary.labels["profile_id"], manifest.profile_id);
    assert_eq!(run.summary.labels["registry_id"], "firms");
    assert_eq!(run.summary.labels["registry_version"], "1.0.12");

    for artifact in &run.stage_artifacts {
        assert!(
            work_dir.join(&artifact.path).exists(),
            "{} stage artifact is written",
            artifact.stage
        );
        assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    }

    let surfaces = surface_lookup_by_name(&work_dir.join("prepare/surfaces.jsonl"));
    assert_eq!(
        surfaces
            .values()
            .filter(|surface| surface.status == "resolved")
            .count() as u64,
        manifest.expected_counts["exact_resolved_surfaces"]
    );
    assert_eq!(
        surfaces
            .values()
            .filter(|surface| surface.status == "unresolved")
            .count() as u64,
        manifest.expected_counts["unresolved_surfaces"]
    );
    assert_boundary_expectations(&manifest, &surfaces);
    assert_distinct_pairs(&manifest, &surfaces);

    let solve: SolveArtifact = read_json(&work_dir.join("solve/solve.json"));
    let review = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve.clone(),
        include: ReviewExportInclude::Escrow,
        provenance_samples: vec![],
        relation_hints: vec![],
    })
    .expect("review export builds for e2e solve artifact");
    assert_eq!(
        review.review_items.len() as u64,
        manifest.expected_counts["review_items"]
    );
    let review_csv = render_review_queue_csv(&review).expect("review csv renders");
    assert_eq!(
        csv::Reader::from_reader(review_csv.as_bytes())
            .records()
            .collect::<Result<Vec<_>, _>>()
            .expect("review csv rows parse")
            .len(),
        0
    );

    let audit = run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&solve_header(&solve)),
        ),
        certified_artifacts: vec![
            EntityArtifactReference {
                version: solve.version.clone(),
                content_hash: solve.artifact_content_hash.clone(),
            },
            EntityArtifactReference {
                version: review.version.clone(),
                content_hash: review.artifact_content_hash.clone(),
            },
        ],
        result: solve_header(&solve),
        suite: sec10d_e2e_audit_suite(&manifest),
    })
    .expect("sec10d e2e audit passes");
    assert_eq!(audit.summary.labels["status"], "passed");
    assert_eq!(
        audit.summary.counts["gate_count"],
        manifest.expected_counts["audit_gates"]
    );

    let selected_input = temp.path().join("selected-org-mentions.jsonl");
    let selected_output = temp.path().join("selected-org-mentions.enriched.jsonl");
    fs::write(&selected_input, selected_source_rows(&manifest)).expect("selected rows");
    run_sec10d_org_apply_streaming(Sec10dOrgApplyStreamRequest {
        rows: &selected_input,
        output: &selected_output,
        lookup_column: "org_name",
        field_name_column: "field_name",
        registry: registry_ref(),
        resolutions: &resolution_table(&expected),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: false,
        target_rows_per_chunk: 2,
    })
    .expect("selected sec10d apply succeeds");
    assert_eq!(
        fs::read_to_string(&selected_output).expect("selected output"),
        fs::read_to_string(repo_path(
            &manifest.fixture_paths.selected_snowflake_enrichment_jsonl
        ))
        .expect("selected expected output")
    );

    let full_output = temp.path().join("org_mentions.enriched.jsonl");
    let apply = run_sec10d_org_apply_streaming(Sec10dOrgApplyStreamRequest {
        rows: &repo_path(&manifest.fixture_paths.org_mentions_jsonl),
        output: &full_output,
        lookup_column: "org_name",
        field_name_column: "field_name",
        registry: registry_ref(),
        resolutions: &resolution_table(&expected),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: false,
        target_rows_per_chunk: 3,
    })
    .expect("full sec10d apply succeeds");
    assert_eq!(
        apply.summary["rows"],
        manifest.expected_counts["apply_rows"]
    );
    assert_eq!(
        apply.summary["resolved"],
        manifest.expected_counts["apply_resolved"]
    );
    assert_eq!(
        apply.summary["unresolved"],
        manifest.expected_counts["apply_unresolved"]
    );
    assert_full_apply_output(&manifest, &expected, &full_output);

    assert_no_forbidden_runtime_terms(&manifest, &work_dir);
}

fn assert_count(counts: &BTreeMap<String, u64>, manifest: &E2eManifest, key: &str) {
    assert_eq!(
        counts.get(key).copied().unwrap_or_default(),
        manifest.expected_counts[key],
        "{key}"
    );
}

fn assert_boundary_expectations(
    manifest: &E2eManifest,
    surfaces: &BTreeMap<String, SurfaceLookup>,
) {
    for expected in &manifest.boundary_expectations {
        let actual = surfaces
            .get(&expected.surface)
            .unwrap_or_else(|| panic!("missing boundary surface {}", expected.surface));
        assert_eq!(actual.status, expected.prepare_status, "{}", expected.id);
        assert_eq!(
            actual.canonical_id, expected.canonical_id,
            "{} canonical id",
            expected.id
        );
    }
}

fn assert_distinct_pairs(manifest: &E2eManifest, surfaces: &BTreeMap<String, SurfaceLookup>) {
    for pair in &manifest.must_remain_distinct {
        let left = surfaces
            .get(&pair.left)
            .and_then(|surface| surface.canonical_id.as_ref())
            .unwrap_or_else(|| panic!("missing canonical id for {}", pair.left));
        let right = surfaces
            .get(&pair.right)
            .and_then(|surface| surface.canonical_id.as_ref())
            .unwrap_or_else(|| panic!("missing canonical id for {}", pair.right));
        assert_ne!(
            left, right,
            "{} and {} must stay distinct",
            pair.left, pair.right
        );
    }
}

fn assert_full_apply_output(manifest: &E2eManifest, expected: &ExpectedSummary, output: &Path) {
    let source_rows = read_jsonl_objects(repo_path(&manifest.fixture_paths.org_mentions_jsonl))
        .into_iter()
        .map(|object| {
            (
                object["source_row_id"]
                    .as_str()
                    .expect("source_row_id")
                    .to_string(),
                object,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let resolutions = resolution_table(expected);
    let expected_apply_status = manifest
        .boundary_expectations
        .iter()
        .map(|boundary| (boundary.surface.as_str(), boundary.apply_status.as_str()))
        .collect::<BTreeMap<_, _>>();
    let unresolved = expected
        .unresolved_surfaces
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    for enriched in read_jsonl_objects(output) {
        let source_row_id = enriched["source_row_id"].as_str().expect("source_row_id");
        let source = source_rows
            .get(source_row_id)
            .unwrap_or_else(|| panic!("unknown enriched source row {source_row_id}"));
        for (field, value) in source {
            assert_eq!(
                enriched.get(field),
                Some(value),
                "raw parser field {field} changed for {source_row_id}"
            );
        }
        for field in APPLY_CANONICAL_FIELDS {
            assert!(
                !enriched.contains_key(*field),
                "sec10d JSONL apply must not append generic {field}"
            );
        }

        let org_name = enriched["org_name"].as_str().expect("org_name");
        let expected_status = expected_apply_status
            .get(org_name)
            .copied()
            .unwrap_or_else(|| panic!("missing apply expectation for {org_name}"));
        let org_fields = org_fields_for_row(&enriched);
        assert_eq!(enriched[org_fields[3].as_str()], registry_ref().id);
        assert_eq!(enriched[org_fields[4].as_str()], registry_ref().version);

        if let Some(resolution) = resolutions.get(org_name) {
            assert_eq!(expected_status, "resolved_exact");
            assert_eq!(enriched[org_fields[0].as_str()], resolution.canonical_id);
            assert_eq!(enriched[org_fields[1].as_str()], resolution.canonical_name);
            assert_eq!(enriched[org_fields[2].as_str()], "resolved_exact");
            assert_eq!(enriched[org_fields[5].as_str()], resolution.rule_id);
        } else {
            assert!(
                unresolved.contains(org_name),
                "unexpected unresolved {org_name}"
            );
            assert_eq!(expected_status, "review_required");
            assert!(enriched[org_fields[0].as_str()].is_null());
            assert!(enriched[org_fields[1].as_str()].is_null());
            assert_eq!(enriched[org_fields[2].as_str()], "review_required");
            assert!(enriched[org_fields[5].as_str()].is_null());
        }
    }
}

fn org_fields_for_row(row: &Map<String, Value>) -> Vec<String> {
    let field_name = row["field_name"].as_str().expect("field_name");
    let prefix = field_name.strip_suffix("_name").unwrap_or(field_name);
    let fields = SEC10D_ORG_FIELD_SUFFIXES
        .iter()
        .map(|suffix| format!("{prefix}{suffix}"))
        .collect::<Vec<_>>();
    assert_eq!(
        row.keys()
            .filter(|field| field.contains("_org_"))
            .cloned()
            .collect::<BTreeSet<_>>(),
        fields.iter().cloned().collect::<BTreeSet<_>>()
    );
    fields
}

fn assert_no_forbidden_runtime_terms(manifest: &E2eManifest, work_dir: &Path) {
    let mut artifact_text = String::new();
    for relative in [
        "run.json",
        "prepare/prepare.json",
        "index.json",
        "block/block.json",
        "edge/edge.json",
        "solve/solve.json",
    ] {
        artifact_text.push_str(
            &fs::read_to_string(work_dir.join(relative))
                .unwrap_or_else(|error| panic!("read {relative}: {error}")),
        );
    }
    let artifact_text = artifact_text.to_ascii_lowercase();
    for term in &manifest.runtime_forbidden {
        assert!(
            !artifact_text.contains(&term.to_ascii_lowercase()),
            "artifact logs should not mention forbidden runtime dependency: {term}"
        );
    }
}

fn surface_lookup_by_name(path: &Path) -> BTreeMap<String, SurfaceLookup> {
    read_jsonl_values(path)
        .into_iter()
        .map(|surface| {
            let primary = surface["primary_surface"]
                .as_str()
                .expect("primary_surface")
                .to_string();
            let exact = &surface["exact_lookup"];
            (
                primary,
                SurfaceLookup {
                    status: exact["status"].as_str().expect("status").to_string(),
                    canonical_id: exact["canonical_id"].as_str().map(str::to_string),
                },
            )
        })
        .collect()
}

fn selected_source_rows(manifest: &E2eManifest) -> String {
    let selected = manifest
        .selected_apply_source_row_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let rows = fs::read_to_string(repo_path(&manifest.fixture_paths.org_mentions_jsonl))
        .expect("org_mentions jsonl opens")
        .lines()
        .filter(|line| {
            selected.iter().any(|source_row_id| {
                line.contains(&format!("\"source_row_id\":\"{source_row_id}\""))
            })
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), selected.len());
    rows.join("\n") + "\n"
}

fn resolution_table(expected: &ExpectedSummary) -> BTreeMap<String, Sec10dOrgApplyResolution> {
    expected
        .exact_resolved_surfaces
        .iter()
        .map(|surface| {
            (
                surface.org_name.clone(),
                Sec10dOrgApplyResolution {
                    canonical_id: surface.canonical_id.clone(),
                    canonical_name: surface.org_name.clone(),
                    resolution_status: "resolved_exact".to_string(),
                    rule_id: surface.rule_id.clone(),
                },
            )
        })
        .collect()
}

fn sec10d_e2e_audit_suite(manifest: &E2eManifest) -> EntityAuditSuite {
    EntityAuditSuite {
        id: "sec10d_regab_e2e".to_string(),
        version: "2026.06.26".to_string(),
        gates: vec![
            gate("G01", "core lookup unchanged", "exact baseline preserved"),
            gate(
                "G13",
                "Reg AB parser boundary",
                "raw parser fields preserved",
            ),
            gate(
                "G15",
                "no network or model runtime",
                &manifest.runtime_forbidden.join("; "),
            ),
        ],
    }
}

fn gate(gate_id: &str, label: &str, actual: &str) -> EntityAuditGateCheck {
    EntityAuditGateCheck {
        gate_id: gate_id.to_string(),
        label: label.to_string(),
        passed: true,
        expected: "passed".to_string(),
        actual: actual.to_string(),
        evidence: BTreeMap::new(),
    }
}

fn solve_header(solve: &SolveArtifact) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: solve.version.clone(),
        metadata: solve.metadata.clone(),
        summary: solve.summary.clone(),
    }
}

fn registry_ref() -> ApplyRegistryReference {
    ApplyRegistryReference {
        id: "firms".to_string(),
        version: "1.0.12".to_string(),
    }
}

fn expected_summary(manifest: &E2eManifest) -> ExpectedSummary {
    read_json(&repo_path(&manifest.fixture_paths.expected_summary))
}

fn manifest() -> E2eManifest {
    read_json(&repo_path(MANIFEST_PATH))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn read_jsonl_values(path: impl AsRef<Path>) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("jsonl opens")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("jsonl parses"))
        .collect()
}

fn read_jsonl_objects(path: impl AsRef<Path>) -> Vec<Map<String, Value>> {
    read_jsonl_values(path)
        .into_iter()
        .map(|value| value.as_object().expect("jsonl object").clone())
        .collect()
}

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}
