#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        apply::{
            APPLY_CANONICAL_FIELDS, ApplyCanonicalResolution, ApplyRegistryReference,
            ApplySafetyCheck, ApplyStreamRequest, run_apply_streaming,
        },
        prepare::{PrepareInputContract, project_prepare_csv_reader},
        profile::EntityProfileDocument,
        run::{EntityRunArtifact, EntityRunRequest, EntityRunStageArtifact, run_entity_workbench},
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

const REGAB_PROFILE: &str = include_str!("fixtures/entity/profiles/regab_firm_identity.yaml");
const ORG_MENTIONS_COLUMNS: &[&str] = &[
    "source_row_id",
    "record_id",
    "dataset",
    "record_version",
    "field_name",
    "org_name",
    "doc_id",
    "as_of_date",
    "filing_cik",
    "accession",
    "filing_form",
    "filed_date",
    "period",
    "source_exhibit_document_name",
    "source_exhibit_type",
    "source_item",
    "role_context",
    "capacity",
    "capacity_normalized",
    "reporting_party_capacity",
    "platform_capacity",
    "platform_capacity_normalized",
    "subject_role",
    "deal_key",
    "transaction_name",
    "alias_surfaces_json",
    "mention_surfaces_json",
];

#[derive(Debug, Deserialize)]
struct ExpectedSummary {
    source: BTreeMap<String, u64>,
    apply: ExpectedApply,
}

#[derive(Debug, Deserialize)]
struct ExpectedApply {
    registry: ApplyRegistryReference,
    canonical_fields: Vec<String>,
    downstream_org_fields: Vec<String>,
}

#[test]
fn sec10d_entity_contract_runs_workbench_and_advertises_handoff_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    let work_dir = temp.path().join("work");
    let expected = expected_summary();

    let result = run_entity_workbench(EntityRunRequest {
        rows: &org_mentions_csv(),
        profile: "regab_firm_identity",
        strategy: &regab_strategy(),
        registry: &registry_path(),
        work_dir: &work_dir,
    })
    .expect("sec10d-shaped Reg AB run succeeds");

    let artifact = result.artifact;
    assert_eq!(artifact.version, "canon_entity_run.v0");
    assert_eq!(artifact.summary.labels["profile_id"], "regab_firm_identity");
    assert_eq!(artifact.summary.labels["registry_id"], "firms");
    assert_eq!(artifact.summary.labels["registry_version"], "1.0.12");
    assert_eq!(
        artifact.summary.counts["row_count"],
        expected.source["row_count"]
    );
    assert_eq!(
        artifact.summary.counts["prepared_surfaces"],
        expected.source["prepared_surfaces"]
    );
    assert_eq!(
        artifact.summary.counts["exact_resolved_surfaces"],
        expected.source["exact_resolved_surfaces"]
    );

    for stage_name in ["prepare", "index", "block", "edge", "solve"] {
        let stage = stage(&artifact, stage_name);
        assert!(
            Path::new(&stage.path).is_relative(),
            "{stage_name} path stays relative"
        );
        assert!(
            work_dir.join(&stage.path).exists(),
            "{stage_name} artifact exists at {}",
            stage.path
        );
        assert!(stage.artifact_content_hash.starts_with("blake3:"));
    }
    assert!(work_dir.join("prepare/surfaces.jsonl").exists());
    assert!(work_dir.join("block/candidates.jsonl").exists());
    assert!(work_dir.join("edge/edges.jsonl").exists());
    assert!(work_dir.join("solve/solve.json").exists());
    assert!(work_dir.join("run.json").exists());

    assert!(artifact.next_commands.resume.contains("canon entity run"));
    assert!(
        artifact
            .next_commands
            .resume
            .contains("--profile regab_firm_identity")
    );
    assert!(
        artifact
            .next_commands
            .review_export
            .contains("canon entity review export")
    );
    assert!(
        artifact
            .next_commands
            .review_export
            .contains("--include escrow")
    );
    assert!(artifact.next_commands.audit.contains("canon entity audit"));
    assert!(
        artifact
            .next_commands
            .promote
            .contains("canon entity promote")
    );
    assert!(
        artifact
            .next_commands
            .promote
            .contains("--next-version <VERSION>")
    );
    assert!(artifact.next_commands.apply.contains("canon entity apply"));
    assert!(artifact.next_commands.apply.contains("--column <COLUMN>"));

    let persisted: EntityRunArtifact = read_json(&work_dir.join("run.json"));
    assert_eq!(persisted, artifact);
}

#[test]
fn sec10d_entity_contract_freezes_input_columns_and_append_only_org_fields() {
    let expected = expected_summary();
    let (headers, _) = csv_maps(org_mentions_csv());
    assert_eq!(headers, ORG_MENTIONS_COLUMNS);
    assert_eq!(
        expected.apply.canonical_fields,
        APPLY_CANONICAL_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>()
    );

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
    let allowed_org_suffixes = expected
        .apply
        .downstream_org_fields
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    for enriched in read_jsonl_objects(fixture_root().join("applied_org_enrichment.jsonl")) {
        let source_row_id = enriched["source_row_id"].as_str().expect("source_row_id");
        let source = source_rows
            .get(source_row_id)
            .unwrap_or_else(|| panic!("unknown enriched source row {source_row_id}"));
        for (field, value) in source {
            assert_eq!(
                enriched.get(field),
                Some(value),
                "sec10d raw parser field {field} changed for {source_row_id}"
            );
        }

        let appended_org_fields = enriched
            .keys()
            .filter(|field| field.contains("_org_"))
            .collect::<BTreeSet<_>>();
        assert!(
            !appended_org_fields.is_empty(),
            "enrichment must append downstream *_org_* fields"
        );
        for field in appended_org_fields {
            assert!(
                allowed_org_suffixes
                    .iter()
                    .any(|suffix| field.ends_with(suffix)),
                "unexpected downstream org field {field}"
            );
        }
    }
}

#[test]
fn sec10d_entity_contract_refuses_missing_required_input_and_unresolved_apply() {
    let profile = EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("valid profile");
    let contract = PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract");
    let missing_org_name =
        "source_row_id,field_name,dataset\nrow-1,servicer_name,regab_servicer_schedules\n";

    let refusal = project_prepare_csv_reader(Cursor::new(missing_org_name), b',', &contract)
        .expect_err("missing org_name refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityInputContract);
    assert_eq!(refusal.detail["field"], "org_name");

    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("org_mentions.canon.csv");
    let refusal = run_apply_streaming(ApplyStreamRequest {
        rows: &org_mentions_csv(),
        output: &output,
        lookup_column: "org_name",
        registry: expected_summary().apply.registry,
        resolutions: &BTreeMap::<String, ApplyCanonicalResolution>::new(),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: true,
        target_rows_per_chunk: 4,
    })
    .expect_err("full-resolution apply refuses unresolved rows");

    assert_eq!(refusal.code, RefusalCode::EEntityApplyUnresolved);
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["rows"], 8);
    assert_eq!(refusal.detail["resolved"], 0);
    assert_eq!(refusal.detail["unresolved"], 8);
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!output.exists(), "refusal must not write apply output");
}

fn stage<'a>(artifact: &'a EntityRunArtifact, stage_name: &str) -> &'a EntityRunStageArtifact {
    artifact
        .stage_artifacts
        .iter()
        .find(|stage| stage.stage == stage_name)
        .unwrap_or_else(|| panic!("{stage_name} stage exists"))
}

fn expected_summary() -> ExpectedSummary {
    read_json(&fixture_root().join("expected_summary.json"))
}

fn csv_maps(path: impl AsRef<Path>) -> (Vec<String>, Vec<BTreeMap<String, String>>) {
    let mut reader = csv::Reader::from_path(path).expect("csv opens");
    let headers = reader
        .headers()
        .expect("headers")
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let rows = reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv rows parse");
    (headers, rows)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
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

fn org_mentions_csv() -> PathBuf {
    fixture_root().join("org_mentions.csv")
}

fn org_mentions_jsonl() -> PathBuf {
    fixture_root().join("org_mentions.jsonl")
}

fn regab_strategy() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/profiles/regab_firm_identity.yaml")
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab/org_mentions")
}

fn registry_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/regab/sec10d_baseline_public/registry_snapshot/firms")
}
