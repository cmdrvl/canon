#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        block::BlockCandidateRecord,
        edge::EdgeEvidenceRecord,
        profile_package::{canonical_package_bytes, load_profile_package_bytes},
        record_link::{AssignmentAlignmentSidecar, canonical_assignment_alignment_bytes},
        run::link::EntityLinkArtifact,
        score::ScoreLane,
        source_mapping::{
            AnchorMapping, AssignmentMapping, CANON_SOURCE_MAPPING_VERSION, CapturePolicy,
            ObservationMapping, RecordLinkComparisonKind, RecordLinkComparisonMapping,
            RecordLinkComparisonPolicies, RecordLinkComparisonSource, RecordLinkInputBuildRequest,
            RoleBinding, SourceFormat, SourceMappingDocumentationRef, SourceMappingPackage,
            SourceMappingProfile, SourceMappingProfileRef, SourceRecord,
            build_record_link_input_sidecar, canonical_record_link_input_bytes, finalize_package,
            map_record, source_mapping_package_digest,
        },
    },
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const SHARED_SURFACE_ROW_COUNT: usize = 12;
const CONTEXT_COLLISION_ROW_COUNT: usize = 1;

#[test]
fn public_subprocess_record_link_stages_link_and_run_publish_sidecars() {
    let fixture = RecordLinkFixture::new();

    assert_success(block_command(&fixture, &fixture.work_dir), "entity block");
    assert_success(
        evidence_command(&fixture, &fixture.work_dir),
        "entity evidence",
    );
    assert_success(solve_command(&fixture, &fixture.work_dir), "entity solve");

    let candidates: Vec<BlockCandidateRecord> =
        read_jsonl(&fixture.work_dir.join("block/candidates.jsonl"));
    assert!(
        candidates.iter().any(|candidate| {
            candidate
                .block_hits
                .iter()
                .any(|hit| hit.operator_id.starts_with("record_link:"))
        }),
        "record-link candidate digest must be present in block candidates"
    );
    assert!(
        fixture
            .work_dir
            .join("block/record_link_candidates.json")
            .exists()
    );
    assert!(
        fixture
            .work_dir
            .join("evidence/record_link_evidence.json")
            .exists()
    );
    assert!(
        fixture
            .work_dir
            .join("evidence/assignment_alignment.json")
            .exists()
    );

    let edge_records: Vec<EdgeEvidenceRecord> =
        read_jsonl(&fixture.work_dir.join("evidence/evidence.jsonl"));
    let support_hits = edge_records
        .iter()
        .flat_map(|record| &record.hits)
        .filter(|hit| hit.reason_code == "record_link_feature_support")
        .collect::<Vec<_>>();
    assert!(
        support_hits.len() >= 3,
        "expected record-grain support across numeric/date/categorical views"
    );
    assert!(support_hits.iter().all(|hit| {
        hit.lane == ScoreLane::Support
            && hit.namespace == "record_link"
            && hit.explanation.contains("record-link derived evidence_id=")
    }));

    let alignment_hits = edge_records
        .iter()
        .flat_map(|record| &record.hits)
        .filter(|hit| hit.reason_code == "record_link_assignment_alignment")
        .collect::<Vec<_>>();
    assert!(
        alignment_hits.len() >= 2,
        "primary and secondary assignments remain record-grain relation hints"
    );
    assert!(alignment_hits.iter().all(|hit| {
        hit.lane == ScoreLane::RelationHint
            && hit.score_units.as_u32() > 0
            && hit.namespace == "record_link"
            && hit.explanation.contains("record-link derived evidence_id=")
    }));

    let veto_hits = edge_records
        .iter()
        .flat_map(|record| &record.hits)
        .filter(|hit| hit.reason_code == "record_link_feature_conflict")
        .collect::<Vec<_>>();
    assert!(
        !veto_hits.is_empty(),
        "mismatched record-link facts must produce hard cannot-link evidence"
    );
    assert!(veto_hits.iter().all(|hit| {
        hit.lane == ScoreLane::AntiMerge
            && hit.hard_cannot_link
            && hit.score_units.as_u32() == 10_000
    }));

    let run_work_dir = fixture.root.join("run-work");
    assert_success(run_command(&fixture, &run_work_dir), "entity run");
    let run_link_evidence: Value =
        read_json(&run_work_dir.join("evidence/record_link_evidence.json"));
    assert_eq!(run_link_evidence["version"], "canon.evidence.v1");

    let link_work_dir = fixture.root.join("link-work");
    let link = link_command(&fixture, &link_work_dir);
    assert_not_refusal(&link, "entity link");
    let link_artifact: Value = read_json(&link_work_dir.join("link/link.json"));
    assert_eq!(link_artifact["version"], "canon_entity_link.v1");
    let profile_source = fixture.profile.display().to_string();
    let profile_hash = hash_bytes(&fs::read(&fixture.profile).expect("profile bytes"));
    assert_eq!(
        link_artifact["profile_source"]["source"].as_str(),
        Some(profile_source.as_str())
    );
    assert_eq!(
        link_artifact["profile_source"]["content_hash"].as_str(),
        Some(profile_hash.as_str())
    );
    assert_eq!(
        link_artifact["assignment_alignment_artifacts"][0]["evidence_semantics"],
        "nonidentity_relation_hint"
    );
    assert_eq!(
        link_artifact["assignment_alignment_artifacts"][0]["path"],
        "assignment_alignment.json"
    );
    assert!(
        link_work_dir
            .join("link/assignment_alignment.json")
            .exists()
    );
    assert_success(
        review_export_command(&link_work_dir.join("link/link.json")),
        "entity review export link",
    );
}

#[test]
fn public_subprocess_tampered_record_link_sidecar_refuses_before_block_write() {
    let fixture = RecordLinkFixture::new();
    let mut sidecar: Value = read_json(&fixture.left_sidecar);
    sidecar["summary"]["record_count"] = Value::from(999_u64);
    fs::write(
        &fixture.left_sidecar,
        serde_json::to_vec(&sidecar).expect("tampered sidecar bytes"),
    )
    .expect("tamper sidecar");

    let output = block_command(&fixture, &fixture.work_dir);
    assert_refusal(&output, RefusalCode::EEntityArtifactContract, "block");
    assert!(!fixture.work_dir.join("block/block.json").exists());
}

#[test]
fn public_subprocess_record_link_outputs_are_stable_when_strategy_input_order_changes() {
    let first = RecordLinkFixture::new();
    let second = RecordLinkFixture::new();
    write_strategy(
        &second.strategy,
        &second.right_sidecar,
        &second.left_sidecar,
        25,
        25_000,
    );

    assert_success(block_command(&first, &first.work_dir), "first block");
    assert_success(evidence_command(&first, &first.work_dir), "first evidence");
    assert_success(block_command(&second, &second.work_dir), "second block");
    assert_success(
        evidence_command(&second, &second.work_dir),
        "second evidence",
    );

    assert_eq!(
        fs::read(first.work_dir.join("block/candidates.jsonl")).expect("first candidates"),
        fs::read(second.work_dir.join("block/candidates.jsonl")).expect("second candidates")
    );
    assert_eq!(
        fs::read(first.work_dir.join("evidence/evidence.jsonl")).expect("first evidence"),
        fs::read(second.work_dir.join("evidence/evidence.jsonl")).expect("second evidence")
    );
}

#[test]
fn public_subprocess_record_link_shared_surface_binds_each_source_lane() {
    let first = RecordLinkFixture::new_shared_surface();
    let second = RecordLinkFixture::new_shared_surface();

    assert_success(block_command(&first, &first.work_dir), "first shared block");
    assert_success(
        block_command(&second, &second.work_dir),
        "second shared block",
    );
    assert_success(
        evidence_command(&first, &first.work_dir),
        "first shared evidence",
    );
    assert_success(
        evidence_command(&second, &second.work_dir),
        "second shared evidence",
    );

    let candidates: Value = read_json(&first.work_dir.join("block/record_link_candidates.json"));
    let mut sources_by_surface = BTreeMap::<String, BTreeSet<String>>::new();
    let mut same_surface_records_by_source = BTreeMap::<String, BTreeSet<String>>::new();
    let mut same_surface_candidate_count = 0usize;
    for candidate in candidates["candidates"]
        .as_array()
        .expect("record-link candidates array")
    {
        let left_surface_id = candidate["left"]["surface_id"]
            .as_str()
            .expect("left surface_id");
        let right_surface_id = candidate["right"]["surface_id"]
            .as_str()
            .expect("right surface_id");
        let same_surface_candidate = left_surface_id == right_surface_id;
        if same_surface_candidate {
            same_surface_candidate_count += 1;
        }
        for endpoint in [&candidate["left"], &candidate["right"]] {
            let source_id = endpoint["source_id"]
                .as_str()
                .expect("endpoint source_id")
                .to_string();
            let record_id = endpoint["record_id"]
                .as_str()
                .expect("endpoint record_id")
                .to_string();
            assert!(
                !record_id.trim().is_empty(),
                "dedicated record-link endpoint record_id must be nonempty"
            );
            let surface_id = endpoint["surface_id"]
                .as_str()
                .expect("endpoint surface_id")
                .to_string();
            sources_by_surface
                .entry(surface_id)
                .or_default()
                .insert(source_id.clone());
            if same_surface_candidate {
                same_surface_records_by_source
                    .entry(source_id)
                    .or_default()
                    .insert(record_id);
            }
        }
    }
    assert!(
        same_surface_candidate_count > 0,
        "dedicated record-link candidates must retain record-grain same-surface endpoints"
    );
    assert!(
        sources_by_surface.values().any(|source_ids| {
            source_ids.contains("left_feed") && source_ids.contains("right_feed")
        }),
        "the exact shared surface must bind independently in both source lanes"
    );
    let left_record_ids = same_surface_records_by_source
        .get("left_feed")
        .expect("left source lane record ids");
    let right_record_ids = same_surface_records_by_source
        .get("right_feed")
        .expect("right source lane record ids");
    assert_eq!(
        left_record_ids.len(),
        SHARED_SURFACE_ROW_COUNT * 2,
        "left source rows with two assignments each should emit complete canonical record ids"
    );
    assert_eq!(
        right_record_ids.len(),
        SHARED_SURFACE_ROW_COUNT * 2,
        "right source rows with two assignments each should emit complete canonical record ids"
    );
    let forbidden_fixture_ids = shared_surface_fixture_ids();
    assert!(
        left_record_ids.is_disjoint(&forbidden_fixture_ids),
        "left dedicated record ids must not reuse caller source-row, assignment, or source fixture ids"
    );
    assert!(
        right_record_ids.is_disjoint(&forbidden_fixture_ids),
        "right dedicated record ids must not reuse caller source-row, assignment, or source fixture ids"
    );
    let generic_candidates: Vec<BlockCandidateRecord> =
        read_jsonl(&first.work_dir.join("block/candidates.jsonl"));
    assert!(
        generic_candidates
            .iter()
            .all(|candidate| candidate.left_surface_id != candidate.right_surface_id),
        "generic block candidates must not contain self-pairs"
    );
    let generic_edge_records: Vec<EdgeEvidenceRecord> =
        read_jsonl(&first.work_dir.join("evidence/evidence.jsonl"));
    assert!(
        generic_edge_records
            .iter()
            .all(|record| record.left_surface_id != record.right_surface_id),
        "generic edge records must not contain self-pairs"
    );
    assert_eq!(
        fs::read(first.work_dir.join("block/record_link_candidates.json"))
            .expect("first shared record-link candidates"),
        fs::read(second.work_dir.join("block/record_link_candidates.json"))
            .expect("second shared record-link candidates"),
        "shared-surface record-link candidate artifact should be deterministic"
    );
    assert_eq!(
        fs::read(first.work_dir.join("block/candidates.jsonl")).expect("first shared candidates"),
        fs::read(second.work_dir.join("block/candidates.jsonl")).expect("second shared candidates"),
        "merged block candidates should be deterministic"
    );
    assert_eq!(
        fs::read(first.work_dir.join("evidence/evidence.jsonl")).expect("first shared evidence"),
        fs::read(second.work_dir.join("evidence/evidence.jsonl")).expect("second shared evidence"),
        "generic edge records should be deterministic"
    );

    let link_work_dir = first.root.join("shared-link-work");
    let link = link_command(&first, &link_work_dir);
    assert_not_refusal(&link, "shared-surface entity link");
}

#[test]
fn public_subprocess_record_link_candidate_stable_tamper_uses_committed_block_payload() {
    let fixture = RecordLinkFixture::new();
    assert_success(block_command(&fixture, &fixture.work_dir), "entity block");
    let candidate_path = fixture.work_dir.join("block/record_link_candidates.json");
    let mut candidates: Value = read_json(&candidate_path);
    candidates["content_hash"] = Value::String("blake3:tampered".to_string());
    fs::write(
        &candidate_path,
        serde_json::to_vec(&candidates).expect("tampered candidates"),
    )
    .expect("write tampered candidates");

    let output = evidence_command(&fixture, &fixture.work_dir);
    assert_success(output, "entity evidence with stale candidate mirror");
    assert_eq!(
        read_json::<Value>(&candidate_path)["content_hash"],
        "blake3:tampered"
    );
    assert!(fixture.work_dir.join("evidence/evidence.json").exists());
    assert!(
        fixture
            .work_dir
            .join("evidence/assignment_alignment.json")
            .exists()
    );
}

#[test]
fn public_subprocess_record_link_budget_refusal_writes_no_block_artifact() {
    let fixture = RecordLinkFixture::new();
    write_strategy(
        &fixture.strategy,
        &fixture.left_sidecar,
        &fixture.right_sidecar,
        25,
        1,
    );
    let output = block_command(&fixture, &fixture.work_dir);
    assert_refusal(&output, RefusalCode::EEntityCandidateBudget, "block");
    assert!(!fixture.work_dir.join("block/block.json").exists());
    assert!(
        !fixture
            .work_dir
            .join("block/record_link_candidates.json")
            .exists()
    );
}

#[test]
fn public_subprocess_link_stale_assignment_and_evidence_mirrors_use_committed_payloads() {
    let fixture = RecordLinkFixture::new();
    let link_work_dir = fixture.root.join("link-tamper-work");
    let link = link_command(&fixture, &link_work_dir);
    assert_not_refusal(&link, "entity link");
    let original_link: EntityLinkArtifact = read_json(&link_work_dir.join("link/link.json"));
    let original_link_hash = original_link.artifact_content_hash.clone();

    let alignment_path = link_work_dir.join("link/assignment_alignment.json");
    let mut alignment: AssignmentAlignmentSidecar = read_json(&alignment_path);
    alignment.record_link_evidence_hash = hash_bytes(b"stale evidence");
    reseal_assignment_alignment_sidecar(&mut alignment);
    let alignment_bytes =
        canonical_assignment_alignment_bytes(&alignment).expect("alignment remains canonical");
    fs::write(&alignment_path, &alignment_bytes).expect("write tampered alignment");

    let link_path = link_work_dir.join("link/link.json");
    let mut link_artifact: EntityLinkArtifact = read_json(&link_path);
    link_artifact.assignment_alignment_artifacts[0].content_hash = hash_bytes(&alignment_bytes);
    reseal_link_artifact(&mut link_artifact);
    fs::write(
        &link_path,
        serde_json::to_vec(&link_artifact).expect("link artifact bytes"),
    )
    .expect("write tampered link");
    fs::write(
        link_work_dir.join("evidence/record_link_evidence.json"),
        br#"{"version":"tampered-stable-record-link-evidence"}"#,
    )
    .expect("write stale stable evidence mirror");

    let output = review_export_command(&link_path);
    assert_review_export_uses_committed_link(output, &original_link_hash);
}

#[test]
fn public_subprocess_link_stale_profile_source_mirror_uses_committed_link() {
    let fixture = RecordLinkFixture::new();
    let link_work_dir = fixture.root.join("link-profile-tamper-work");
    let link = link_command(&fixture, &link_work_dir);
    assert_not_refusal(&link, "entity link");

    let link_path = link_work_dir.join("link/link.json");
    let mut link_artifact: EntityLinkArtifact = read_json(&link_path);
    let original_link_hash = link_artifact.artifact_content_hash.clone();
    link_artifact.profile_source.content_hash = hash_bytes(b"stale profile");
    reseal_link_artifact(&mut link_artifact);
    fs::write(
        &link_path,
        serde_json::to_vec(&link_artifact).expect("link artifact bytes"),
    )
    .expect("write tampered link");

    let output = review_export_command(&link_path);
    assert_review_export_uses_committed_link(output, &original_link_hash);
}

struct RecordLinkFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    rows: PathBuf,
    reference_rows: PathBuf,
    target_rows: PathBuf,
    profile: PathBuf,
    strategy: PathBuf,
    registry: PathBuf,
    work_dir: PathBuf,
    left_sidecar: PathBuf,
    right_sidecar: PathBuf,
}

impl RecordLinkFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let rows = root.join("rows.csv");
        let reference_rows = root.join("reference.csv");
        let target_rows = root.join("target.csv");
        let profile = root.join("profile.json");
        let strategy = root.join("strategy.yaml");
        let registry = root.join("registry");
        let work_dir = root.join("work");
        let left_sidecar = root.join("left.record_link.json");
        let right_sidecar = root.join("right.record_link.json");
        write_rows(&rows);
        write_reference_rows(&reference_rows);
        write_target_rows(&target_rows);
        write_profile(&profile);
        write_registry(&registry);
        let profile_digest = hash_bytes(&fs::read(&profile).expect("profile bytes"));
        write_sidecar(&left_sidecar, "left_feed", &profile_digest, left_rows());
        write_sidecar(&right_sidecar, "right_feed", &profile_digest, right_rows());
        write_strategy(&strategy, &left_sidecar, &right_sidecar, 25, 25_000);
        Self {
            _temp: temp,
            root,
            rows,
            reference_rows,
            target_rows,
            profile,
            strategy,
            registry,
            work_dir,
            left_sidecar,
            right_sidecar,
        }
    }

    fn new_shared_surface() -> Self {
        let fixture = Self::new();
        write_shared_surface_rows(&fixture.rows);
        write_shared_surface_reference_rows(&fixture.reference_rows);
        write_shared_surface_target_rows(&fixture.target_rows);
        let profile_digest = hash_bytes(&fs::read(&fixture.profile).expect("profile bytes"));
        write_sidecar(
            &fixture.left_sidecar,
            "left_feed",
            &profile_digest,
            left_shared_surface_rows(),
        );
        write_sidecar(
            &fixture.right_sidecar,
            "right_feed",
            &profile_digest,
            right_shared_surface_rows(),
        );
        let left_records = (SHARED_SURFACE_ROW_COUNT + CONTEXT_COLLISION_ROW_COUNT) * 2;
        let right_records = SHARED_SURFACE_ROW_COUNT * 2;
        let expected_candidate_pairs = left_records * right_records;
        write_strategy(
            &fixture.strategy,
            &fixture.left_sidecar,
            &fixture.right_sidecar,
            expected_candidate_pairs,
            expected_candidate_pairs,
        );
        fixture
    }
}

fn write_rows(path: &Path) {
    fs::write(
        path,
        "source_row_id,name,source_system\n\
left-1,Alpha Holdings,left_feed\n\
left-2,Gamma Holdings,left_feed\n\
right-1,Beta Holdings,right_feed\n\
right-2,Delta Holdings,right_feed\n",
    )
    .expect("rows");
}

fn write_reference_rows(path: &Path) {
    fs::write(
        path,
        "source_row_id,name,source_system\n\
left-1,Alpha Holdings,left_feed\n\
left-2,Gamma Holdings,left_feed\n",
    )
    .expect("reference rows");
}

fn write_target_rows(path: &Path) {
    fs::write(
        path,
        "source_row_id,name,source_system\n\
right-1,Beta Holdings,right_feed\n\
right-2,Delta Holdings,right_feed\n",
    )
    .expect("target rows");
}

fn write_shared_surface_rows(path: &Path) {
    let mut rows = String::from("source_row_id,name,source_system\n");
    for row_id in shared_surface_row_ids("left") {
        rows.push_str(&format!("{row_id},Shared Holdings,left_feed\n"));
    }
    for row_id in context_collision_row_ids("left") {
        rows.push_str(&format!("{row_id},Context Echo,left_feed\n"));
    }
    for row_id in shared_surface_row_ids("right") {
        rows.push_str(&format!("{row_id},Shared Holdings,right_feed\n"));
    }
    fs::write(path, rows).expect("shared rows");
}

fn write_shared_surface_reference_rows(path: &Path) {
    let mut rows = String::from("source_row_id,name,source_system\n");
    for row_id in shared_surface_row_ids("left") {
        rows.push_str(&format!("{row_id},Shared Holdings,left_feed\n"));
    }
    for row_id in context_collision_row_ids("left") {
        rows.push_str(&format!("{row_id},Context Echo,left_feed\n"));
    }
    fs::write(path, rows).expect("shared reference rows");
}

fn write_shared_surface_target_rows(path: &Path) {
    let mut rows = String::from("source_row_id,name,source_system\n");
    for row_id in shared_surface_row_ids("right") {
        rows.push_str(&format!("{row_id},Shared Holdings,right_feed\n"));
    }
    fs::write(path, rows).expect("shared target rows");
}

fn write_profile(path: &Path) {
    let package = load_profile_package_bytes(
        br#"{
  "kind": "entity-profile",
  "profile": "pkg.synthetic:record_link",
  "version": "0.1.0",
  "entity_type": "organization",
  "identity_semantics": "same_entity_or_reviewed_alias",
  "canonical_type": "org",
  "required_fields": [
    "source_row_id",
    "name",
    "source_system"
  ],
  "normalized_views": {
    "core": {
      "operators": [
        { "op": "lowercase" },
        { "op": "normalize_whitespace" }
      ]
    }
  },
  "evidence": {
    "support": [
      { "op": "exact_view", "view": "core" }
    ],
    "cannot_link": [
      { "op": "role_conflict", "view": "core" }
    ],
    "relation_hints": [
      { "op": "cross_profile_alignment", "view": "core" }
    ]
  },
  "patch_namespaces": {
    "aliases": "pkg.synthetic:record_link.aliases",
    "distinct": "pkg.synthetic:record_link.distinct",
    "relations": "pkg.synthetic:record_link.relations"
  },
  "evidence_policy": {
    "kind": "evidence-policy",
    "id": "pkg.synthetic:record_link.evidence_policy",
    "version": "2026.07.14",
    "content_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "review_policy": {
    "kind": "review-policy",
    "id": "pkg.synthetic:record_link.review_policy",
    "version": "2026.07.14",
    "content_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  },
  "promotion_policy": {
    "kind": "promotion-policy",
    "id": "pkg.synthetic:record_link.promotion_policy",
    "version": "2026.07.14",
    "content_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  },
  "frozen_executable_strategy": {
    "kind": "frozen-executable-strategy",
    "id": "pkg.synthetic:record_link.strategy",
    "version": "2026.07.14",
    "content_hash": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
  },
  "ontology_package": {
    "kind": "ontology-package",
    "id": "pkg.synthetic:record_link.ontology",
    "version": "2026.07.14",
    "content_hash": "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
  },
  "identifier_package": {
    "kind": "identifier-package",
    "id": "pkg.synthetic:record_link.identifier",
    "version": "2026.07.14",
    "content_hash": "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
  },
  "vocabulary_package": {
    "kind": "vocabulary-package",
    "id": "pkg.synthetic:record_link.vocabulary",
    "version": "2026.07.14",
    "content_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111"
  },
  "evidence_package": {
    "kind": "evidence-package",
    "id": "pkg.synthetic:record_link.evidence",
    "version": "2026.07.14",
    "content_hash": "blake3:2222222222222222222222222222222222222222222222222222222222222222"
  },
  "normalization_packages": [
    {
      "kind": "normalization-package",
      "id": "pkg.synthetic:record_link.normalization",
      "version": "2026.07.14",
      "content_hash": "blake3:3333333333333333333333333333333333333333333333333333333333333333"
    }
  ],
  "available_capabilities": [
    "prepare",
    "index",
    "block",
    "evidence",
    "solve_cluster",
    "solve_link",
    "review",
    "promote",
    "apply"
  ],
  "field_mappings": [
    {
      "field_path": "source_row_id",
      "object_type": "organization",
      "field_role": "record_key",
      "required": true
    },
    {
      "field_path": "name",
      "object_type": "organization",
      "field_role": "canonical_surface",
      "normalized_view": "core",
      "required": true
    },
    {
      "field_path": "source_system",
      "object_type": "organization",
      "field_role": "provenance_value",
      "required": true
    }
  ],
  "execution_modes": [
    {
      "mode": "cluster",
      "source_object_type": "organization",
      "required_capabilities": [
        "prepare",
        "index",
        "block",
        "evidence",
        "solve_cluster",
        "review",
        "promote",
        "apply"
      ],
      "field_paths": [
        "source_row_id",
        "name",
        "source_system"
      ],
      "outputs": [
        "prepare_bundle",
        "cluster_assignments",
        "review_queue"
      ]
    },
    {
      "mode": "link",
      "source_object_type": "organization",
      "target_object_type": "organization",
      "link_direction": "bidirectional",
      "required_capabilities": [
        "prepare",
        "index",
        "block",
        "evidence",
        "solve_link",
        "review",
        "promote",
        "apply"
      ],
      "field_paths": [
        "source_row_id",
        "name",
        "source_system"
      ],
      "outputs": [
        "prepare_bundle",
        "link_candidates",
        "link_decisions"
      ]
    }
  ],
  "limits": {
    "max_observation_fields": 8,
    "max_candidate_pairs": 25000,
    "max_outputs": 500
  },
  "expected_outputs": [
    "prepare_bundle",
    "cluster_assignments",
    "review_queue",
    "link_candidates",
    "link_decisions"
  ],
  "project_overrides": []
}"#,
    )
    .expect("profile package");
    let bytes = canonical_package_bytes(&package).expect("canonical profile package bytes");
    fs::write(path, bytes).expect("profile");
}

fn write_registry(path: &Path) {
    fs::create_dir_all(path).expect("registry dir");
    fs::write(
        path.join("registry.json"),
        r#"{"id":"record-link-registry","version":"2026.07.14","description":"record-link bridge test registry","updated":"2026-07-14","entry_count":0}"#,
    )
    .expect("registry metadata");
    fs::write(path.join("aliases.json"), b"[]").expect("aliases");
}

fn write_strategy(
    path: &Path,
    left_sidecar: &Path,
    right_sidecar: &Path,
    max_candidates: usize,
    max_pair_comparisons: usize,
) {
    let left_path = left_sidecar
        .file_name()
        .expect("left sidecar file name")
        .to_string_lossy();
    let right_path = right_sidecar
        .file_name()
        .expect("right sidecar file name")
        .to_string_lossy();
    fs::write(
        path,
        format!(
            r#"strategy_id: pkg.synthetic:record_link_strategy
strategy_version: 0.1.0
entity_type: organization
identity:
  reference:
    id_columns: [source_row_id]
  target:
    id_columns: [source_row_id]
candidate_filter: []
assertions:
  - field_ref: source_system
    field_tgt: source_system
    op: exact
    weight: 1.0
    required: false
match_threshold: 1.0
ambiguity_gap: 0.10
max_candidates: 25
record_link:
  inputs:
    - path: {}
    - path: {}
  operator_id: record_link:synthetic:v1
  max_candidates_per_record: {}
  max_candidate_pairs: {}
  max_pair_comparisons: {}
  require_unique_best_per_record: false
  assignment_hint_score_units: 1000
  assignment_alignment:
    policy_id: record_link.assignment_alignment
    policy_version: "1"
    cardinality: many_to_many
  feature_policies:
    - feature_id: pkg.synthetic:amount
      kind: numeric
      support:
        kind: numeric_tolerance
        tolerance_scaled_units: 0
      score_units: 10000
      hard_conflict_on_mismatch: true
    - feature_id: pkg.synthetic:effective_date
      kind: date
      support:
        kind: date_near
        max_days: 0
      score_units: 10000
      hard_conflict_on_mismatch: true
    - feature_id: pkg.synthetic:category
      kind: categorical
      support:
        kind: categorical_exact
      score_units: 10000
      hard_conflict_on_mismatch: true
"#,
            left_path, right_path, max_candidates, max_candidates, max_pair_comparisons
        ),
    )
    .expect("strategy");
}

fn write_sidecar(path: &Path, source_id: &str, profile_digest: &str, rows: Vec<Value>) {
    let package = finalize_package(sample_package(source_id)).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = SourceMappingProfileRef {
        package_digest: digest.clone(),
        profile_id: "pkg.synthetic:record_link".to_string(),
    };
    let mapped = rows
        .into_iter()
        .map(|payload| {
            map_record(
                &package,
                &reference,
                &SourceRecord {
                    format: SourceFormat::Jsonl,
                    payload,
                },
            )
            .expect("record maps")
        })
        .collect::<Vec<_>>();
    let sidecar =
        build_record_link_input_sidecar(&request(source_id, profile_digest, &digest), &mapped)
            .expect("record-link input sidecar builds");
    let bytes = canonical_record_link_input_bytes(&sidecar).expect("canonical sidecar bytes");
    fs::write(path, bytes).expect("sidecar write");
}

fn request(
    source_id: &str,
    profile_digest: &str,
    source_mapping_digest: &str,
) -> RecordLinkInputBuildRequest {
    RecordLinkInputBuildRequest {
        source_id: source_id.to_string(),
        scope_id: "public_fixture".to_string(),
        profile_id: "pkg.synthetic:record_link".to_string(),
        profile_digest: profile_digest.to_string(),
        input_digest: hash_bytes(source_id.as_bytes()),
        source_mapping_digest: source_mapping_digest.to_string(),
        subject_observation_mapping_id: "pkg.synthetic:subject_surface".to_string(),
        assignment_mapping_ids: vec![
            "pkg.synthetic:primary_assignment".to_string(),
            "pkg.synthetic:secondary_assignment".to_string(),
        ],
        missing_assignment_policy: CapturePolicy::Reject,
        comparison_mappings: vec![
            RecordLinkComparisonMapping {
                feature_id: "pkg.synthetic:amount".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                path: "context.amount".to_string(),
                value_kind: RecordLinkComparisonKind::Numeric,
                units: Some("basis_points".to_string()),
                scale: Some(2),
                policies: RecordLinkComparisonPolicies::default(),
            },
            RecordLinkComparisonMapping {
                feature_id: "pkg.synthetic:effective_date".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                path: "context.effective_date".to_string(),
                value_kind: RecordLinkComparisonKind::Date,
                units: None,
                scale: None,
                policies: RecordLinkComparisonPolicies::default(),
            },
            RecordLinkComparisonMapping {
                feature_id: "pkg.synthetic:category".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                path: "context.category".to_string(),
                value_kind: RecordLinkComparisonKind::Categorical,
                units: None,
                scale: None,
                policies: RecordLinkComparisonPolicies::default(),
            },
        ],
        duplicate_record_policy: CapturePolicy::Reject,
    }
}

fn left_rows() -> Vec<Value> {
    vec![
        source_row(
            "left-1",
            "Alpha Holdings",
            "100.25",
            "2026-03-31",
            "baseline",
        ),
        source_row(
            "left-2",
            "Gamma Holdings",
            "205.00",
            "2026-04-30",
            "control",
        ),
    ]
}

fn right_rows() -> Vec<Value> {
    vec![
        source_row(
            "right-1",
            "Beta Holdings",
            "100.25",
            "2026-03-31",
            "baseline",
        ),
        source_row(
            "right-2",
            "Delta Holdings",
            "199.00",
            "2026-04-30",
            "control",
        ),
    ]
}

fn left_shared_surface_rows() -> Vec<Value> {
    let mut rows = shared_surface_rows("left");
    rows.extend(context_collision_rows("left"));
    rows
}

fn right_shared_surface_rows() -> Vec<Value> {
    shared_surface_rows("right")
}

fn shared_surface_rows(prefix: &str) -> Vec<Value> {
    shared_surface_row_ids(prefix)
        .into_iter()
        .map(|row_id| {
            source_row(
                &row_id,
                "Shared Holdings",
                "100.25",
                "2026-03-31",
                "Context Echo",
            )
        })
        .collect()
}

fn shared_surface_row_ids(prefix: &str) -> Vec<String> {
    (1..=SHARED_SURFACE_ROW_COUNT)
        .map(|row| format!("{prefix}-shared-{row:02}"))
        .collect()
}

fn context_collision_rows(prefix: &str) -> Vec<Value> {
    context_collision_row_ids(prefix)
        .into_iter()
        .map(|row_id| source_row(&row_id, "Context Echo", "999.00", "2026-12-31", "control"))
        .collect()
}

fn context_collision_row_ids(prefix: &str) -> Vec<String> {
    (1..=CONTEXT_COLLISION_ROW_COUNT)
        .map(|row| format!("{prefix}-context-collision-{row:02}"))
        .collect()
}

fn shared_surface_fixture_ids() -> BTreeSet<String> {
    let mut ids = ["left_feed", "right_feed", "public_fixture"]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>();
    for row_id in shared_surface_row_ids("left")
        .into_iter()
        .chain(shared_surface_row_ids("right"))
        .chain(context_collision_row_ids("left"))
        .chain(context_collision_row_ids("right"))
    {
        ids.insert(row_id.clone());
        ids.insert(format!("{row_id}-P"));
        ids.insert(format!("{row_id}-S"));
    }
    ids
}

fn source_row(
    record_id: &str,
    display_name: &str,
    amount: &str,
    effective_date: &str,
    category: &str,
) -> Value {
    json!({
        "meta": { "record_id": record_id, "row": record_id, "as_of": "2026-03-31" },
        "subject": { "display_name": display_name, "public_anchor": record_id },
        "context": {
            "amount": amount,
            "effective_date": effective_date,
            "category": category
        },
        "assignments": {
            "primary": { "name": format!("{display_name} Primary"), "public_ref": format!("{record_id}-P") },
            "secondary": { "name": format!("{display_name} Secondary"), "public_ref": format!("{record_id}-S") }
        }
    })
}

fn sample_package(source_system: &str) -> SourceMappingPackage {
    SourceMappingPackage {
        version: CANON_SOURCE_MAPPING_VERSION.to_string(),
        package_id: "pkg.synthetic".to_string(),
        package_version: "1.0.0".to_string(),
        profiles: vec![SourceMappingProfile {
            profile_id: "pkg.synthetic:record_link".to_string(),
            source_system: source_system.to_string(),
            source_formats: vec![SourceFormat::Jsonl],
            object_id_path: "meta.record_id".to_string(),
            locator_path: "meta.row".to_string(),
            fragment_path: None,
            as_of_path: Some("meta.as_of".to_string()),
            valid_from_path: None,
            valid_to_path: None,
            observations: vec![ObservationMapping {
                mapping_id: "pkg.synthetic:subject_surface".to_string(),
                subject_type_id: "types.synthetic:subject".to_string(),
                surface_path: "subject.display_name".to_string(),
                anchor_mappings: vec![AnchorMapping {
                    namespace: "public_anchor".to_string(),
                    path: "subject.public_anchor".to_string(),
                }],
                context_paths: vec![
                    "context.amount".to_string(),
                    "context.effective_date".to_string(),
                    "context.category".to_string(),
                ],
            }],
            assignments: vec![
                AssignmentMapping {
                    mapping_id: "pkg.synthetic:primary_assignment".to_string(),
                    subject_type_id: "types.synthetic:subject".to_string(),
                    assignee_type_id: "types.synthetic:assignment".to_string(),
                    role_binding: RoleBinding::Literal {
                        role_id: "pkg.synthetic:primary".to_string(),
                    },
                    assignee_surface_path: "assignments.primary.name".to_string(),
                    assignee_anchor_mappings: vec![AnchorMapping {
                        namespace: "assignment_ref".to_string(),
                        path: "assignments.primary.public_ref".to_string(),
                    }],
                    context_paths: Vec::new(),
                },
                AssignmentMapping {
                    mapping_id: "pkg.synthetic:secondary_assignment".to_string(),
                    subject_type_id: "types.synthetic:subject".to_string(),
                    assignee_type_id: "types.synthetic:assignment".to_string(),
                    role_binding: RoleBinding::Literal {
                        role_id: "pkg.synthetic:secondary".to_string(),
                    },
                    assignee_surface_path: "assignments.secondary.name".to_string(),
                    assignee_anchor_mappings: vec![AnchorMapping {
                        namespace: "assignment_ref".to_string(),
                        path: "assignments.secondary.public_ref".to_string(),
                    }],
                    context_paths: Vec::new(),
                },
            ],
            relationships: Vec::new(),
            policies: Default::default(),
            documentation_refs: vec!["docs/record_link.md".to_string()],
        }],
        documentation: vec![SourceMappingDocumentationRef {
            label: "record-link contract".to_string(),
            uri: "docs/record_link.md".to_string(),
        }],
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .expect("jsonl bytes")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl row parses"))
        .collect()
}

fn block_command(fixture: &RecordLinkFixture, work_dir: &Path) -> Output {
    canon([
        OsString::from("entity"),
        OsString::from("block"),
        fixture.rows.as_os_str().to_owned(),
        OsString::from("--profile"),
        fixture.profile.as_os_str().to_owned(),
        OsString::from("--strategy"),
        fixture.strategy.as_os_str().to_owned(),
        OsString::from("--registry"),
        fixture.registry.as_os_str().to_owned(),
        OsString::from("--work-dir"),
        work_dir.as_os_str().to_owned(),
        OsString::from("--emit"),
        OsString::from("summary"),
    ])
}

fn evidence_command(fixture: &RecordLinkFixture, work_dir: &Path) -> Output {
    canon([
        OsString::from("entity"),
        OsString::from("evidence"),
        fixture.rows.as_os_str().to_owned(),
        OsString::from("--profile"),
        fixture.profile.as_os_str().to_owned(),
        OsString::from("--strategy"),
        fixture.strategy.as_os_str().to_owned(),
        OsString::from("--candidates"),
        work_dir.join("block/block.json").as_os_str().to_owned(),
        OsString::from("--registry"),
        fixture.registry.as_os_str().to_owned(),
        OsString::from("--work-dir"),
        work_dir.as_os_str().to_owned(),
        OsString::from("--emit"),
        OsString::from("summary"),
    ])
}

fn solve_command(fixture: &RecordLinkFixture, work_dir: &Path) -> Output {
    canon([
        OsString::from("entity"),
        OsString::from("solve"),
        fixture.rows.as_os_str().to_owned(),
        OsString::from("--profile"),
        fixture.profile.as_os_str().to_owned(),
        OsString::from("--strategy"),
        fixture.strategy.as_os_str().to_owned(),
        OsString::from("--evidence"),
        work_dir
            .join("evidence/evidence.json")
            .as_os_str()
            .to_owned(),
        OsString::from("--registry"),
        fixture.registry.as_os_str().to_owned(),
        OsString::from("--work-dir"),
        work_dir.as_os_str().to_owned(),
        OsString::from("--emit"),
        OsString::from("summary"),
    ])
}

fn run_command(fixture: &RecordLinkFixture, work_dir: &Path) -> Output {
    canon([
        OsString::from("entity"),
        OsString::from("run"),
        fixture.rows.as_os_str().to_owned(),
        OsString::from("--profile"),
        fixture.profile.as_os_str().to_owned(),
        OsString::from("--strategy"),
        fixture.strategy.as_os_str().to_owned(),
        OsString::from("--registry"),
        fixture.registry.as_os_str().to_owned(),
        OsString::from("--work-dir"),
        work_dir.as_os_str().to_owned(),
        OsString::from("--emit"),
        OsString::from("summary"),
        OsString::from("--no-witness"),
    ])
}

fn link_command(fixture: &RecordLinkFixture, work_dir: &Path) -> Output {
    canon([
        OsString::from("entity"),
        OsString::from("link"),
        fixture.reference_rows.as_os_str().to_owned(),
        fixture.target_rows.as_os_str().to_owned(),
        OsString::from("--profile"),
        fixture.profile.as_os_str().to_owned(),
        OsString::from("--strategy"),
        fixture.strategy.as_os_str().to_owned(),
        OsString::from("--registry"),
        fixture.registry.as_os_str().to_owned(),
        OsString::from("--work-dir"),
        work_dir.as_os_str().to_owned(),
        OsString::from("--emit"),
        OsString::from("json"),
        OsString::from("--no-witness"),
    ])
}

fn review_export_command(link_artifact: &Path) -> Output {
    canon([
        OsString::from("entity"),
        OsString::from("review"),
        OsString::from("export"),
        link_artifact.as_os_str().to_owned(),
        OsString::from("--include"),
        OsString::from("escrow"),
        OsString::from("--emit"),
        OsString::from("json"),
    ])
}

fn canon<const N: usize>(args: [OsString; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .expect("canon subprocess runs")
}

fn assert_success(output: Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn assert_review_export_uses_committed_link(output: Output, expected_link_hash: &str) {
    assert!(
        output.status.success(),
        "entity review export link failed\nstdout:\n{}\nstderr:\n{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    let review: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "failed to parse review export JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            text(&output.stdout),
            text(&output.stderr)
        )
    });
    assert_eq!(review["version"], "canon_entity_review_queue.v0");
    assert_eq!(review["source_link_hash"], expected_link_hash);
}

fn assert_not_refusal(output: &Output, label: &str) {
    assert!(
        output.status.code() != Some(2),
        "{label} refused\nstdout:\n{}\nstderr:\n{}",
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn assert_refusal(output: &Output, code: RefusalCode, stage: &str) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected refusal\nstdout:\n{}\nstderr:\n{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    let envelope = parse_refusal_envelope(output);
    let refusal = envelope.get("refusal").unwrap_or(&envelope);
    assert_eq!(refusal["code"], refusal_code_string(code));
    assert_eq!(refusal["detail"]["stage"], stage);
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

fn parse_refusal_envelope(output: &Output) -> Value {
    if output.stdout.is_empty() {
        let stderr = text(&output.stderr);
        let Some(final_line) = stderr.lines().rev().find(|line| !line.trim().is_empty()) else {
            panic!(
                "expected refusal JSON on final stderr line\nstdout:\n{}\nstderr:\n{}",
                text(&output.stdout),
                stderr
            );
        };
        serde_json::from_str(final_line).unwrap_or_else(|error| {
            panic!(
                "failed to parse final stderr line as refusal JSON: {error}\nstdout:\n{}\nstderr:\n{}",
                text(&output.stdout),
                stderr
            )
        })
    } else {
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "failed to parse stdout as refusal JSON: {error}\nstdout:\n{}\nstderr:\n{}",
                text(&output.stdout),
                text(&output.stderr)
            )
        })
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn refusal_code_string(code: RefusalCode) -> &'static str {
    match code {
        RefusalCode::EEntityArtifactContract => "E_ENTITY_ARTIFACT_CONTRACT",
        RefusalCode::EEntityCandidateBudget => "E_ENTITY_CANDIDATE_BUDGET",
        _ => panic!("unexpected refusal code in record-link pipeline test"),
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn reseal_assignment_alignment_sidecar(sidecar: &mut AssignmentAlignmentSidecar) {
    sidecar.artifact_content_hash.clear();
    sidecar.artifact_content_hash =
        hash_bytes(&serde_json::to_vec(sidecar).expect("hashable assignment sidecar"));
}

fn reseal_link_artifact(artifact: &mut EntityLinkArtifact) {
    artifact.artifact_content_hash.clear();
    artifact.metadata.artifact_content_hash.clear();
    let hash = hash_bytes(&serde_json::to_vec(artifact).expect("hashable link artifact"));
    artifact.artifact_content_hash = hash.clone();
    artifact.metadata.artifact_content_hash = hash;
}
