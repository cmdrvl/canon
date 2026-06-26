#![forbid(unsafe_code)]

use canon::entity::{
    apply::{
        APPLY_CANONICAL_FIELDS, ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck,
        ApplyStreamRequest, run_apply_streaming,
    },
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    prepare::{
        PrepareInputContract, PrepareRunRequest, PreparedExactLookupStatus, PreparedSurfaceRecord,
        project_prepare_csv_reader, project_prepare_jsonl_reader, run_prepare,
    },
    profile::EntityProfileDocument,
    profiles::regab::{RegabFirmGuardKind, RegabFirmGuardRequest, regab_firm_guard_hit},
    relation::{RelationHintRequest, relation_hint_hit},
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveComponentAction, SolveReconciliationConfig, SolveReconciliationState,
        build_solve_diagnostics, evaluate_signed_graph_components,
        reconcile_signed_graph_components,
    },
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

const REGAB_PROFILE: &str = include_str!("../fixtures/entity/profiles/regab_firm_identity.yaml");

#[derive(Debug, Deserialize)]
struct ExpectedSummary {
    schema_version: String,
    profile_id: String,
    identity_semantics: String,
    source: BTreeMap<String, u64>,
    exact_resolved_surfaces: Vec<ResolvedSurface>,
    unresolved_surfaces: Vec<String>,
    guarded_pairs: Vec<GuardedPair>,
    solve_summary: BTreeMap<String, u64>,
    apply: ApplyExpected,
}

#[derive(Debug, Deserialize)]
struct ResolvedSurface {
    org_name: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

#[derive(Debug, Deserialize)]
struct GuardedPair {
    benchmark_id: String,
    left: String,
    right: String,
    left_role: String,
    right_role: String,
    guard: String,
    relation: String,
    support_reason: String,
    support_score_units: u32,
    guard_score_units: u32,
    expected_review_priority: String,
}

#[derive(Debug, Deserialize)]
struct ApplyExpected {
    registry: ApplyRegistryReference,
    rows: u64,
    resolved: u64,
    unresolved: u64,
    canonical_fields: Vec<String>,
    downstream_org_fields: Vec<String>,
}

#[test]
#[allow(non_snake_case)]
fn REGAB_I001_org_mentions_shape_is_accepted_directly() {
    let expected = expected_summary();
    let profile =
        EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("regab profile validates");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");

    assert_eq!(
        expected.schema_version,
        "canon.entity.regab.org_mentions.integration.v0"
    );
    assert_eq!(expected.profile_id, "regab_firm_identity");
    assert_eq!(expected.identity_semantics, "same_firm_or_reviewed_alias");

    let csv_input = fs::read_to_string(org_mentions_csv()).expect("org_mentions csv opens");
    let csv_observations = project_prepare_csv_reader(Cursor::new(csv_input), b',', &contract)
        .expect("csv org_mentions shape projects");
    let jsonl_input = fs::read_to_string(org_mentions_jsonl()).expect("org_mentions jsonl opens");
    let jsonl_observations =
        project_prepare_jsonl_reader(Cursor::new(jsonl_input.into_bytes()), &contract)
            .expect("jsonl org_mentions shape projects");

    assert_eq!(csv_observations.len() as u64, expected.source["row_count"]);
    assert_eq!(jsonl_observations.len(), csv_observations.len());

    let pnc = csv_observations
        .iter()
        .find(|observation| observation.primary_surface.value == "PNC Bank, National Association")
        .expect("PNC observation is present");
    assert_eq!(pnc.profile_id, "regab_firm_identity");
    assert_eq!(pnc.primary_surface.field, "org_name");
    assert_eq!(pnc.alias_surfaces[0].value, "PNC Bank N.A.");
    assert_eq!(pnc.context["dataset"], "regab_servicer_schedules");
    assert_eq!(pnc.context["role_context"], "servicer_name:master_servicer");
    assert_eq!(pnc.context["capacity"], "Master Servicer");
    assert_eq!(pnc.context["subject_role"], "servicer");
    assert_eq!(pnc.provenance["source_row_id"], "regab-fixture-001");
    assert_eq!(pnc.provenance["accession"], "0001234567-26-000001");
    assert_eq!(
        pnc.anchors
            .iter()
            .map(|anchor| (anchor.namespace.as_str(), anchor.field.as_str()))
            .collect::<Vec<_>>(),
        [("accession", "accession"), ("cik", "filing_cik")]
    );
    assert!(
        pnc.mention_surfaces
            .iter()
            .any(|surface| surface.value == "period=2025-12-31")
    );

    let (artifact, surfaces) = run_prepare_fixture();
    assert_prepare_summary(&artifact.summary, &expected.source);
    assert_surface_expectations(&surfaces, &expected);
}

#[test]
#[allow(non_snake_case)]
fn REGAB_I002_I003_edge_solve_guards_keep_review_and_escrow_cases_out_of_auto_merge() {
    let expected = expected_summary();
    let (_, surfaces) = run_prepare_fixture();
    let surface_ids = surface_ids_by_name(&surfaces);
    let edge_records = guarded_edge_records(&expected, &surface_ids);

    assert_eq!(
        edge_records.len() as u64,
        expected.solve_summary["hard_cannot_link_count"]
    );
    for record in &edge_records {
        assert!(record.has_hard_cannot_link);
        assert!(
            record
                .hits
                .iter()
                .any(|hit| hit.lane == ScoreLane::RelationHint)
        );
    }

    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: Vec::new(),
        incumbent_ids: Vec::new(),
    })
    .expect("Reg AB signed graph builds");
    assert_eq!(
        graph.diagnostics.support_edge_count,
        expected.solve_summary["hard_cannot_link_count"]
    );
    assert_eq!(
        graph.diagnostics.hard_cannot_link_edge_count,
        expected.solve_summary["hard_cannot_link_count"]
    );

    let constraints = evaluate_signed_graph_components(&graph);
    assert_eq!(
        constraints.summary["component_count"],
        expected.solve_summary["component_count"]
    );
    assert_eq!(
        constraints.summary["auto_merge_candidate_count"],
        expected.solve_summary["auto_merge_candidate_count"]
    );
    assert_eq!(
        constraints.summary["contradiction_count"],
        expected.solve_summary["contradiction_count"]
    );
    assert_eq!(
        constraints.summary["hard_cannot_link_count"],
        expected.solve_summary["hard_cannot_link_count"]
    );
    for component in &constraints.components {
        assert_eq!(component.action, SolveComponentAction::Contradiction);
        assert_eq!(
            component.reason,
            "hard_cannot_link_inside_positive_component"
        );
        assert_eq!(component.review_priority_reasons, ["hard_cannot_link"]);
    }

    let reconciliation =
        reconcile_signed_graph_components(&graph, SolveReconciliationConfig::escrow_only(score(1)));
    assert_eq!(
        reconciliation.summary["contradiction_count"],
        expected.solve_summary["contradiction_count"]
    );
    assert_eq!(reconciliation.summary["promotable_new_count"], 0);
    assert_eq!(reconciliation.summary["resolved_existing_count"], 0);
    assert!(
        reconciliation
            .decisions
            .iter()
            .all(|decision| decision.state == SolveReconciliationState::Contradiction)
    );

    let diagnostics = build_solve_diagnostics(
        &graph,
        SolveReconciliationConfig::escrow_only(score(1)),
        &[],
    );
    assert_eq!(
        diagnostics.summary["review_group_count"],
        expected.solve_summary["review_group_count"]
    );
    assert!(
        diagnostics
            .review_group_seeds
            .iter()
            .all(|seed| seed.state == SolveReconciliationState::Contradiction)
    );
}

#[test]
#[allow(non_snake_case)]
fn REGAB_I004_apply_appends_org_fields_and_preserves_raw_input() {
    let temp = tempfile::tempdir().expect("tempdir");
    let expected = expected_summary();
    let output = temp.path().join("org_mentions.canon.csv");
    let resolutions = resolution_table(&expected);

    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &org_mentions_csv(),
        output: &output,
        lookup_column: "org_name",
        registry: expected.apply.registry.clone(),
        resolutions: &resolutions,
        safety: ApplySafetyCheck::default(),
        require_full_resolution: false,
        target_rows_per_chunk: 3,
    })
    .expect("Reg AB apply fixture runs");

    assert_eq!(artifact.version, "canon_entity_apply.v0");
    assert_eq!(artifact.registry, expected.apply.registry);
    assert_eq!(artifact.summary["rows"], expected.apply.rows);
    assert_eq!(artifact.summary["resolved"], expected.apply.resolved);
    assert_eq!(artifact.summary["unresolved"], expected.apply.unresolved);
    assert_eq!(
        expected.apply.canonical_fields,
        APPLY_CANONICAL_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>()
    );

    let (input_headers, input_rows) = csv_maps(org_mentions_csv());
    let (output_headers, output_rows) = csv_maps(&output);
    let mut expected_headers = input_headers.clone();
    expected_headers.extend(expected.apply.canonical_fields.clone());
    assert_eq!(output_headers, expected_headers);
    assert_eq!(output_rows.len(), input_rows.len());

    let input_by_row = input_rows
        .into_iter()
        .map(|row| (row["source_row_id"].clone(), row))
        .collect::<BTreeMap<_, _>>();
    let resolved_by_surface = expected
        .exact_resolved_surfaces
        .iter()
        .map(|surface| (surface.org_name.as_str(), surface))
        .collect::<BTreeMap<_, _>>();
    let unresolved = expected
        .unresolved_surfaces
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    for row in &output_rows {
        let original = input_by_row
            .get(&row["source_row_id"])
            .expect("output row came from input fixture");
        for header in &input_headers {
            assert_eq!(
                row.get(header),
                original.get(header),
                "raw field {header} changed for {}",
                row["source_row_id"]
            );
        }

        let org_name = row["org_name"].as_str();
        if let Some(expected_surface) = resolved_by_surface.get(org_name) {
            assert_eq!(row["canonical_id"], expected_surface.canonical_id);
            assert_eq!(row["canonical_type"], expected_surface.canonical_type);
            assert_eq!(row["canonical_status"], "resolved");
            assert_eq!(row["canonical_registry_id"], expected.apply.registry.id);
            assert_eq!(
                row["canonical_registry_version"],
                expected.apply.registry.version
            );
            assert_eq!(row["canonical_rule_id"], expected_surface.rule_id);
        } else {
            assert!(
                unresolved.contains(org_name),
                "unexpected unresolved {org_name}"
            );
            assert_eq!(row["canonical_id"], "");
            assert_eq!(row["canonical_type"], "");
            assert_eq!(row["canonical_status"], "unresolved");
            assert_eq!(row["canonical_rule_id"], "");
        }
    }

    assert_downstream_org_enrichment_fixture_is_append_only(&expected);
}

fn guarded_edge_records(
    expected: &ExpectedSummary,
    surface_ids: &BTreeMap<String, String>,
) -> Vec<EdgeEvidenceRecord> {
    expected
        .guarded_pairs
        .iter()
        .map(|pair| guarded_edge_record(pair, surface_ids))
        .collect()
}

fn guarded_edge_record(
    pair: &GuardedPair,
    surface_ids: &BTreeMap<String, String>,
) -> EdgeEvidenceRecord {
    assert!(
        matches!(pair.benchmark_id.as_str(), "REGAB-I002" | "REGAB-I003"),
        "unexpected Reg AB benchmark {}",
        pair.benchmark_id
    );
    let guard = RegabFirmGuardKind::from_code(&pair.guard)
        .unwrap_or_else(|| panic!("unknown Reg AB guard {}", pair.guard));
    assert_eq!(guard.review_priority(), pair.expected_review_priority);

    let support = EdgeEvidenceHit::new(
        ScoreLane::Support,
        "regab_firm_identity.fixture_support",
        "high_recall_regab_candidate",
        &pair.support_reason,
        score(pair.support_score_units),
        false,
        format!(
            "fixture support reason={} benchmark_id={}",
            pair.support_reason, pair.benchmark_id
        ),
    );
    let anti_merge = regab_firm_guard_hit(RegabFirmGuardRequest {
        namespace: "regab_firm_identity.guards",
        guard,
        left_name: &pair.left,
        right_name: &pair.right,
        left_role: Some(&pair.left_role),
        right_role: Some(&pair.right_role),
        score_units: score(pair.guard_score_units),
    })
    .expect("Reg AB guard emits cannot-link evidence");
    let relation = relation_hint_hit(RelationHintRequest {
        namespace: "regab_firm_identity.relations",
        operator_id: "relation_hint:regab_fixture",
        reason_code: "regab_relation_context",
        relation: &pair.relation,
        left_value: &pair.left,
        right_value: &pair.right,
        score_units: score(1),
    })
    .expect("Reg AB relation hint emits");

    let mut ids = [
        surface_ids[&pair.left].clone(),
        surface_ids[&pair.right].clone(),
    ];
    ids.sort();
    build_edge_evidence_record(
        ids[0].clone(),
        ids[1].clone(),
        vec![support, anti_merge, relation],
    )
    .expect("guarded edge record builds")
}

fn assert_downstream_org_enrichment_fixture_is_append_only(expected: &ExpectedSummary) {
    let source_rows = read_jsonl_objects(org_mentions_jsonl())
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
    let allowed_suffixes = expected
        .apply
        .downstream_org_fields
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    for object in read_jsonl_objects(fixture_root().join("applied_org_enrichment.jsonl")) {
        let source_row_id = object["source_row_id"].as_str().expect("source_row_id");
        let source = source_rows
            .get(source_row_id)
            .unwrap_or_else(|| panic!("unknown enriched source row {source_row_id}"));
        for (key, value) in source {
            assert_eq!(
                object.get(key),
                Some(value),
                "downstream enrichment changed raw field {key}"
            );
        }

        let org_fields = object
            .keys()
            .filter(|key| key.contains("_org_"))
            .collect::<Vec<_>>();
        assert!(
            !org_fields.is_empty(),
            "downstream enrichment should append org fields"
        );
        for field in org_fields {
            assert!(
                allowed_suffixes
                    .iter()
                    .any(|suffix| field.ends_with(suffix)),
                "unexpected downstream org enrichment field {field}"
            );
        }
    }
}

fn assert_prepare_summary(actual: &BTreeMap<String, u64>, expected: &BTreeMap<String, u64>) {
    for key in [
        "row_count",
        "raw_unique_surfaces",
        "prepared_surfaces",
        "exact_resolved_surfaces",
        "unresolved_surfaces",
        "alias_surface_count",
        "mention_surface_count",
        "anchor_count",
    ] {
        assert_eq!(actual[key], expected[key], "prepare counter {key}");
    }
}

fn assert_surface_expectations(surfaces: &[PreparedSurfaceRecord], expected: &ExpectedSummary) {
    let by_name = surfaces_by_name(surfaces);
    for expectation in &expected.exact_resolved_surfaces {
        let surface = by_name
            .get(&expectation.org_name)
            .unwrap_or_else(|| panic!("missing prepared surface {}", expectation.org_name));
        assert_eq!(
            surface.exact_lookup.status,
            PreparedExactLookupStatus::Resolved
        );
        assert_eq!(
            surface.exact_lookup.canonical_id.as_deref(),
            Some(expectation.canonical_id.as_str())
        );
        assert_eq!(
            surface.exact_lookup.canonical_type.as_deref(),
            Some(expectation.canonical_type.as_str())
        );
        assert_eq!(
            surface.exact_lookup.rule_id.as_deref(),
            Some(expectation.rule_id.as_str())
        );
        assert!(
            surface
                .exact_lookup
                .registry_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.id == "firms" && snapshot.version == "1.0.12")
        );
    }

    for org_name in &expected.unresolved_surfaces {
        let surface = by_name
            .get(org_name)
            .unwrap_or_else(|| panic!("missing unresolved surface {org_name}"));
        assert_eq!(
            surface.exact_lookup.status,
            PreparedExactLookupStatus::Unresolved
        );
        assert!(surface.exact_lookup.canonical_id.is_none());
    }
}

fn run_prepare_fixture() -> (
    canon::entity::prepare::PrepareRunArtifact,
    Vec<PreparedSurfaceRecord>,
) {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact = run_prepare(PrepareRunRequest {
        rows: &org_mentions_csv(),
        profile: "regab_firm_identity",
        registry: &registry_path(),
        work_dir: temp.path(),
    })
    .expect("prepare fixture runs");
    let surfaces = read_surfaces(temp.path().join(&artifact.surfaces_path));
    (artifact, surfaces)
}

fn resolution_table(expected: &ExpectedSummary) -> BTreeMap<String, ApplyCanonicalResolution> {
    expected
        .exact_resolved_surfaces
        .iter()
        .map(|surface| {
            (
                surface.org_name.clone(),
                ApplyCanonicalResolution {
                    canonical_id: surface.canonical_id.clone(),
                    canonical_type: surface.canonical_type.clone(),
                    rule_id: surface.rule_id.clone(),
                },
            )
        })
        .collect()
}

fn surfaces_by_name(surfaces: &[PreparedSurfaceRecord]) -> BTreeMap<String, PreparedSurfaceRecord> {
    let mut by_name = BTreeMap::new();
    for surface in surfaces {
        for raw in &surface.raw_variants {
            by_name.insert(raw.clone(), surface.clone());
        }
    }
    by_name
}

fn surface_ids_by_name(surfaces: &[PreparedSurfaceRecord]) -> BTreeMap<String, String> {
    surfaces_by_name(surfaces)
        .into_iter()
        .map(|(name, surface)| (name, surface.surface_id))
        .collect()
}

fn read_surfaces(path: impl AsRef<Path>) -> Vec<PreparedSurfaceRecord> {
    fs::read_to_string(path)
        .expect("surfaces jsonl opens")
        .lines()
        .map(|line| serde_json::from_str(line).expect("surface record parses"))
        .collect()
}

fn csv_maps(path: impl AsRef<Path>) -> (Vec<String>, Vec<BTreeMap<String, String>>) {
    let mut reader = csv::Reader::from_path(path).expect("csv opens");
    let headers = reader
        .headers()
        .expect("csv headers")
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let rows = reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv rows parse");
    (headers, rows)
}

fn read_jsonl_objects(path: impl AsRef<Path>) -> Vec<Map<String, Value>> {
    fs::read_to_string(path)
        .expect("jsonl opens")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .expect("jsonl line parses")
                .as_object()
                .expect("jsonl line object")
                .clone()
        })
        .collect()
}

fn expected_summary() -> ExpectedSummary {
    serde_json::from_str(
        &fs::read_to_string(fixture_root().join("expected_summary.json"))
            .expect("expected summary opens"),
    )
    .expect("expected summary parses")
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("fixture score is in range")
}

fn org_mentions_csv() -> PathBuf {
    fixture_root().join("org_mentions.csv")
}

fn org_mentions_jsonl() -> PathBuf {
    fixture_root().join("org_mentions.jsonl")
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab/org_mentions")
}

fn registry_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/regab/sec10d_baseline_public/registry_snapshot/firms")
}
