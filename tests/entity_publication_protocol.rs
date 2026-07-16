use canon::entity::publication::{
    ENTITY_PUBLICATION_ROOT, EntityPublicationErrorKind, EntityPublicationFailpoint,
    EntityPublicationFileInput, EntityPublicationOptions, EntityPublicationOutcome,
    EntityPublicationRequest, EntityPublicationUpstreamRef, open_committed_generation,
    open_current_stream_generation, publication_commit_marker_path, publication_object_path,
    publish_generation, publish_generation_with_options, publish_stream_patch,
    publish_stream_patch_with_options,
};
use canon::witness::hash_bytes;
use std::sync::{Arc, Barrier};

fn file(
    logical_path: &str,
    stage: &str,
    version: &str,
    bytes: &[u8],
) -> EntityPublicationFileInput {
    EntityPublicationFileInput::new(logical_path, stage, version, bytes.to_vec())
}

fn request(
    cache_mode: &str,
    cache_status: &str,
    files: Vec<EntityPublicationFileInput>,
) -> EntityPublicationRequest {
    EntityPublicationRequest {
        stream_id: "entity-stage-set".to_string(),
        supersedes_generation_id: None,
        request_fingerprint:
            "blake3:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        cache_mode: cache_mode.to_string(),
        cache_status: cache_status.to_string(),
        cache_receipt_hash:
            "blake3:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        stage_order: vec![
            "prepare".to_string(),
            "index".to_string(),
            "block".to_string(),
            "evidence".to_string(),
            "solve".to_string(),
            "run".to_string(),
            "link".to_string(),
        ],
        upstream_artifacts: vec![EntityPublicationUpstreamRef {
            version: "canon_entity_run.v1".to_string(),
            content_hash: "blake3:3333333333333333333333333333333333333333333333333333333333333333"
                .to_string(),
        }],
        files,
        omit_logical_paths: Vec::new(),
    }
}

fn request_for_stream(
    stream_id: &str,
    cache_mode: &str,
    cache_status: &str,
    files: Vec<EntityPublicationFileInput>,
) -> EntityPublicationRequest {
    let mut request = request(cache_mode, cache_status, files);
    request.stream_id = stream_id.to_string();
    request
}

fn run_stage_set_files(run_bytes: &[u8]) -> Vec<EntityPublicationFileInput> {
    vec![
        file(
            "block/block.json",
            "block",
            "canon_entity_block.v1",
            br#"{"stage":"block"}"#,
        ),
        file(
            "block/candidates.jsonl",
            "block",
            "canon_entity_block.v1",
            br#"{"candidate":"one"}"#,
        ),
        file(
            "block/diagnostics.json",
            "block",
            "canon_entity_block.v1",
            br#"{"diagnostics":"block"}"#,
        ),
        file(
            "block/exact_buckets.jsonl",
            "block",
            "canon_entity_block.v1",
            br#"{"bucket":"exact"}"#,
        ),
        file(
            "evidence/evidence.json",
            "evidence",
            "canon_entity_evidence.v1",
            br#"{"stage":"evidence"}"#,
        ),
        file(
            "evidence/evidence.jsonl",
            "evidence",
            "canon_entity_evidence.v1",
            br#"{"evidence":"one"}"#,
        ),
        file(
            "solve/decision_ledger.jsonl",
            "solve",
            "canon_entity_solve.v1",
            b"",
        ),
        file(
            "solve/solve.json",
            "solve",
            "canon_entity_solve.v1",
            br#"{"stage":"solve"}"#,
        ),
        file(
            "run/manifest.json",
            "run",
            "canon_entity_run.v1",
            br#"{"manifest":"run"}"#,
        ),
        file("run/run.json", "run", "canon_entity_run.v1", run_bytes),
    ]
}

fn child_claim_path(work_dir: &std::path::Path, parent_generation_id: &str) -> std::path::PathBuf {
    work_dir
        .join(ENTITY_PUBLICATION_ROOT)
        .join("claims")
        .join("children")
        .join(parent_generation_id.strip_prefix("blake3:").unwrap())
}

fn write_child_claim(
    work_dir: &std::path::Path,
    parent_generation_id: &str,
    child_generation_id: &str,
) {
    let claim_path = child_claim_path(work_dir, parent_generation_id);
    std::fs::create_dir_all(&claim_path).unwrap();
    std::fs::write(claim_path.join("child"), format!("{child_generation_id}\n")).unwrap();
}

#[test]
fn committed_generation_reads_complete_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let receipt = publish_generation(
        temp.path(),
        request(
            "enabled",
            "warm_hit",
            vec![
                file(
                    "run/run.json",
                    "run",
                    "canon_entity_run.v1",
                    br#"{"stage":"run"}"#,
                ),
                file(
                    "solve/solve.json",
                    "solve",
                    "canon_entity_solve.v1",
                    br#"{"stage":"solve"}"#,
                ),
            ],
        ),
    )
    .unwrap();

    assert_eq!(receipt.outcome, EntityPublicationOutcome::Committed);
    assert!(receipt.writes_performed);
    assert_eq!(receipt.committed, Some(true));

    let snapshot = open_committed_generation(temp.path(), &receipt.generation_id).unwrap();
    assert_eq!(
        snapshot.read_logical_file("run/run.json"),
        Some(br#"{"stage":"run"}"#.as_slice())
    );
    assert_eq!(
        snapshot.read_logical_file("solve/solve.json"),
        Some(br#"{"stage":"solve"}"#.as_slice())
    );
    assert_eq!(
        snapshot.logical_paths().collect::<Vec<_>>(),
        vec!["run/run.json", "solve/solve.json"]
    );
}

#[test]
fn warm_rerun_same_generation_is_no_write() {
    let temp = tempfile::tempdir().unwrap();
    let input = request(
        "enabled",
        "warm_hit",
        vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
    );
    let first = publish_generation(temp.path(), input.clone()).unwrap();
    let second = publish_generation(temp.path(), input).unwrap();

    assert_eq!(first.generation_id, second.generation_id);
    assert_eq!(second.outcome, EntityPublicationOutcome::AlreadyCommitted);
    assert!(!second.writes_performed);
    assert_eq!(second.committed, Some(true));
}

#[test]
fn cache_mode_changes_generation_key() {
    let temp = tempfile::tempdir().unwrap();
    let enabled = publish_generation(
        temp.path(),
        request(
            "enabled",
            "warm_hit",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"same")],
        ),
    )
    .unwrap();
    let disabled = publish_generation(
        temp.path(),
        request(
            "disabled",
            "bypass",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"same")],
        ),
    )
    .unwrap();

    assert_ne!(enabled.generation_id, disabled.generation_id);
    assert_eq!(enabled.committed, Some(true));
    assert_eq!(disabled.committed, Some(true));
}

#[test]
fn invalid_requests_refuse_before_writes() {
    let cases = [
        {
            let mut input = request(
                "enabled",
                "cold_miss",
                vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
            );
            input.request_fingerprint = "not-a-hash".to_string();
            input
        },
        request(
            "relabel",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
        request(
            "enabled",
            "cold_miss",
            vec![file("../run.json", "run", "canon_entity_run.v1", b"run")],
        ),
        request(
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "ghost", "canon_entity_run.v1", b"run")],
        ),
    ];

    for input in cases {
        let temp = tempfile::tempdir().unwrap();
        let err = publish_generation(temp.path(), input).unwrap_err();
        assert!(!err.writes_performed);
        assert_eq!(err.committed, Some(false));
        assert!(!temp.path().join(ENTITY_PUBLICATION_ROOT).exists());
    }
}

#[test]
fn manifest_records_bind_object_paths_to_content_hashes() {
    let temp = tempfile::tempdir().unwrap();
    let receipt = publish_generation(
        temp.path(),
        request(
            "enabled",
            "cold_miss",
            vec![
                file("run/run.json", "run", "canon_entity_run.v1", b"run"),
                file(
                    "solve/solve.json",
                    "solve",
                    "canon_entity_solve.v1",
                    b"solve",
                ),
            ],
        ),
    )
    .unwrap();
    let snapshot = open_committed_generation(temp.path(), &receipt.generation_id).unwrap();

    for record in snapshot.manifest.files {
        let expected = format!(
            "{ENTITY_PUBLICATION_ROOT}/objects/blake3/{}",
            record.content_hash.strip_prefix("blake3:").unwrap()
        );
        assert_eq!(record.object_path, expected);
        assert!(
            publication_object_path(temp.path(), &record.content_hash)
                .unwrap()
                .is_file()
        );
    }
}

#[test]
fn pre_commit_failure_leaves_no_readable_generation() {
    let temp = tempfile::tempdir().unwrap();
    let err = publish_generation_with_options(
        temp.path(),
        request(
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
        EntityPublicationOptions {
            failpoint: EntityPublicationFailpoint::BeforeCommitMarker,
        },
    )
    .unwrap_err();

    assert!(err.writes_performed);
    assert_eq!(err.committed, Some(false));
    let generation_id = err.generation_id.unwrap();
    let open_err = open_committed_generation(temp.path(), &generation_id).unwrap_err();
    assert_eq!(open_err.committed, Some(false));
}

#[test]
fn fault_injection_boundaries_report_truthful_state() {
    let cases = [
        (
            EntityPublicationFailpoint::BeforeObjectDirectoryCreate,
            false,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterObjectDirectoryCreate,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::BeforeObjectAttemptCreate,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterObjectAttemptCreate,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterObjectWrite,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterObjectFlush,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterObjectSync,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::BeforeObjectRename,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterObjectRenameBeforeDirectorySync,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::BeforeCommitDirectoryCreate,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterCommitDirectoryCreate,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::BeforeCommitMarker,
            true,
            Some(false),
            false,
        ),
        (
            EntityPublicationFailpoint::AfterCommitMarkerBeforeParentSync,
            true,
            None,
            true,
        ),
    ];

    for (failpoint, writes_performed, committed, readable) in cases {
        let temp = tempfile::tempdir().unwrap();
        let err = publish_generation_with_options(
            temp.path(),
            request(
                "enabled",
                "cold_miss",
                vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
            ),
            EntityPublicationOptions { failpoint },
        )
        .unwrap_err();

        assert_eq!(err.writes_performed, writes_performed, "{failpoint:?}");
        assert_eq!(err.committed, committed, "{failpoint:?}");
        let generation_id = err.generation_id.unwrap();
        assert_eq!(
            publication_commit_marker_path(temp.path(), &generation_id).is_dir(),
            readable,
            "{failpoint:?}"
        );
        assert_eq!(
            open_committed_generation(temp.path(), &generation_id).is_ok(),
            readable,
            "{failpoint:?}"
        );
    }
}

#[test]
fn post_marker_sync_failure_reports_unknown_without_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let err = publish_generation_with_options(
        temp.path(),
        request(
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
        EntityPublicationOptions {
            failpoint: EntityPublicationFailpoint::AfterCommitMarkerBeforeParentSync,
        },
    )
    .unwrap_err();

    assert!(err.writes_performed);
    assert_eq!(err.committed, None);
    let generation_id = err.generation_id.unwrap();
    let snapshot = open_committed_generation(temp.path(), &generation_id).unwrap();
    assert_eq!(
        snapshot.read_logical_file("run/run.json"),
        Some(b"run".as_slice())
    );
}

#[test]
fn successful_publish_leaves_no_attempt_alias_to_committed_object() {
    let temp = tempfile::tempdir().unwrap();
    publish_generation(
        temp.path(),
        request(
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
    )
    .unwrap();

    let attempts = temp
        .path()
        .join(ENTITY_PUBLICATION_ROOT)
        .join("attempts")
        .read_dir()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(attempts.is_empty());
}

#[test]
fn reader_snapshot_ignores_later_generation() {
    let temp = tempfile::tempdir().unwrap();
    let first = publish_generation(
        temp.path(),
        request(
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"first")],
        ),
    )
    .unwrap();
    let first_snapshot = open_committed_generation(temp.path(), &first.generation_id).unwrap();
    let second = publish_generation(
        temp.path(),
        request(
            "disabled",
            "bypass",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"second",
            )],
        ),
    )
    .unwrap();

    assert_ne!(first.generation_id, second.generation_id);
    assert_eq!(
        first_snapshot.read_logical_file("run/run.json"),
        Some(b"first".as_slice())
    );
    assert_eq!(
        open_committed_generation(temp.path(), &second.generation_id)
            .unwrap()
            .read_logical_file("run/run.json"),
        Some(b"second".as_slice())
    );
}

#[test]
fn tampered_object_or_manifest_refuses() {
    let object_temp = tempfile::tempdir().unwrap();
    let object_receipt = publish_generation(
        object_temp.path(),
        request(
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
    )
    .unwrap();
    let object_snapshot =
        open_committed_generation(object_temp.path(), &object_receipt.generation_id).unwrap();
    let object_hash = &object_snapshot.manifest.files[0].content_hash;
    std::fs::write(
        publication_object_path(object_temp.path(), object_hash).unwrap(),
        b"bad",
    )
    .unwrap();
    assert!(open_committed_generation(object_temp.path(), &object_receipt.generation_id).is_err());

    let manifest_temp = tempfile::tempdir().unwrap();
    let manifest_receipt = publish_generation(
        manifest_temp.path(),
        request(
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
    )
    .unwrap();
    let manifest_path =
        publication_object_path(manifest_temp.path(), &manifest_receipt.generation_id).unwrap();
    let mut manifest_bytes = std::fs::read(&manifest_path).unwrap();
    manifest_bytes.push(b'\n');
    std::fs::write(manifest_path, manifest_bytes).unwrap();
    assert!(
        open_committed_generation(manifest_temp.path(), &manifest_receipt.generation_id).is_err()
    );
}

#[test]
fn racing_same_generation_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = Arc::new(temp.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(8));
    let input = request(
        "enabled",
        "cold_miss",
        vec![
            file("run/run.json", "run", "canon_entity_run.v1", b"run"),
            file(
                "solve/solve.json",
                "solve",
                "canon_entity_solve.v1",
                hash_bytes(b"solve").as_bytes(),
            ),
        ],
    );

    let mut handles = Vec::new();
    for _ in 0..8 {
        let work_dir = Arc::clone(&work_dir);
        let barrier = Arc::clone(&barrier);
        let input = input.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            publish_generation(&work_dir, input).unwrap()
        }));
    }

    let receipts = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    let generation_id = receipts[0].generation_id.clone();
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.generation_id == generation_id)
    );
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.committed == Some(true))
    );
    assert!(
        receipts
            .iter()
            .any(|receipt| matches!(receipt.outcome, EntityPublicationOutcome::Committed))
    );
    let snapshot = open_committed_generation(temp.path(), &generation_id).unwrap();
    assert_eq!(
        snapshot.read_logical_file("run/run.json"),
        Some(b"run".as_slice())
    );
}

#[test]
fn stream_initial_publish_reads_current_head() {
    let temp = tempfile::tempdir().unwrap();
    let receipt = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "primary-stream",
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
    )
    .unwrap();

    assert_eq!(receipt.outcome, EntityPublicationOutcome::Committed);
    let current = open_current_stream_generation(temp.path(), "primary-stream").unwrap();
    assert_eq!(current.generation_id, receipt.generation_id);
    assert_eq!(
        current.read_logical_file("run/run.json"),
        Some(b"run".as_slice())
    );
}

#[test]
fn stream_patch_carries_forward_unchanged_logical_objects() {
    let temp = tempfile::tempdir().unwrap();
    let first = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "carry-forward",
            "enabled",
            "cold_miss",
            vec![
                file("run/run.json", "run", "canon_entity_run.v1", b"run-v1"),
                file(
                    "solve/solve.json",
                    "solve",
                    "canon_entity_solve.v1",
                    b"solve-v1",
                ),
            ],
        ),
    )
    .unwrap();
    let second = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "carry-forward",
            "enabled",
            "cold_miss",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"run-v2",
            )],
        ),
    )
    .unwrap();

    assert_ne!(first.generation_id, second.generation_id);
    let current = open_current_stream_generation(temp.path(), "carry-forward").unwrap();
    assert_eq!(current.generation_id, second.generation_id);
    assert_eq!(
        current.manifest.supersedes_generation_id.as_deref(),
        Some(first.generation_id.as_str())
    );
    assert_eq!(
        current.read_logical_file("run/run.json"),
        Some(b"run-v2".as_slice())
    );
    assert_eq!(
        current.read_logical_file("solve/solve.json"),
        Some(b"solve-v1".as_slice())
    );
}

#[test]
fn run_stage_set_publishes_transactional_stream_and_rejects_stale_parent() {
    let temp = tempfile::tempdir().unwrap();
    let first = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "public-run-stage-set",
            "enabled",
            "cold_miss",
            run_stage_set_files(br#"{"stage":"run","generation":1}"#),
        ),
    )
    .unwrap();

    assert_eq!(first.outcome, EntityPublicationOutcome::Committed);
    assert!(first.writes_performed);
    let first_snapshot =
        open_current_stream_generation(temp.path(), "public-run-stage-set").unwrap();
    assert_eq!(first_snapshot.generation_id, first.generation_id);
    assert_eq!(
        first_snapshot.logical_paths().collect::<Vec<_>>(),
        vec![
            "block/block.json",
            "block/candidates.jsonl",
            "block/diagnostics.json",
            "block/exact_buckets.jsonl",
            "evidence/evidence.json",
            "evidence/evidence.jsonl",
            "run/manifest.json",
            "run/run.json",
            "solve/decision_ledger.jsonl",
            "solve/solve.json",
        ]
    );
    assert_eq!(
        first_snapshot.read_logical_file("run/run.json"),
        Some(br#"{"stage":"run","generation":1}"#.as_slice())
    );

    let mut run_patch = request_for_stream(
        "public-run-stage-set",
        "enabled",
        "cold_miss",
        vec![file(
            "run/run.json",
            "run",
            "canon_entity_run.v1",
            br#"{"stage":"run","generation":2}"#,
        )],
    );
    run_patch.request_fingerprint = hash_bytes(b"public-run-stage-set-rerun");
    let second = publish_stream_patch(temp.path(), run_patch).unwrap();
    let second_snapshot =
        open_current_stream_generation(temp.path(), "public-run-stage-set").unwrap();
    assert_eq!(second_snapshot.generation_id, second.generation_id);
    assert_eq!(
        second_snapshot.manifest.supersedes_generation_id.as_deref(),
        Some(first.generation_id.as_str())
    );
    assert_eq!(
        second_snapshot.read_logical_file("run/run.json"),
        Some(br#"{"stage":"run","generation":2}"#.as_slice())
    );
    assert_eq!(
        second_snapshot.read_logical_file("solve/solve.json"),
        Some(br#"{"stage":"solve"}"#.as_slice())
    );
    assert_eq!(
        second_snapshot.read_logical_file("block/block.json"),
        Some(br#"{"stage":"block"}"#.as_slice())
    );
    assert_eq!(
        second_snapshot.read_logical_file("evidence/evidence.json"),
        Some(br#"{"stage":"evidence"}"#.as_slice())
    );

    let mut stale_parent = request_for_stream(
        "public-run-stage-set",
        "enabled",
        "cold_miss",
        vec![file(
            "run/run.json",
            "run",
            "canon_entity_run.v1",
            br#"{"stage":"run","generation":3}"#,
        )],
    );
    stale_parent.request_fingerprint = hash_bytes(b"public-run-stage-set-stale-parent");
    stale_parent.supersedes_generation_id = Some(first.generation_id.clone());
    let err = publish_stream_patch(temp.path(), stale_parent).unwrap_err();
    assert!(matches!(
        err.kind,
        EntityPublicationErrorKind::ForkedGeneration | EntityPublicationErrorKind::InvalidRequest
    ));
    assert_eq!(err.committed, Some(true));

    let current = open_current_stream_generation(temp.path(), "public-run-stage-set").unwrap();
    assert_eq!(current.generation_id, second.generation_id);
    assert_eq!(
        current.read_logical_file("run/run.json"),
        Some(br#"{"stage":"run","generation":2}"#.as_slice())
    );
    assert_eq!(
        current.read_logical_file("solve/solve.json"),
        Some(br#"{"stage":"solve"}"#.as_slice())
    );
}

#[test]
fn stream_patch_can_append_only_omit_stale_downstream_logical_files() {
    let temp = tempfile::tempdir().unwrap();
    let run_head = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "entity-run-stage-set",
            "enabled",
            "rebuilt",
            run_stage_set_files(br#"{"run":"one"}"#),
        ),
    )
    .unwrap();
    let link_head = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "entity-run-stage-set",
            "enabled",
            "rebuilt",
            vec![file(
                "link/link.json",
                "link",
                "canon_entity_link.v1",
                br#"{"link":"one"}"#,
            )],
        ),
    )
    .unwrap();
    assert_ne!(run_head.generation_id, link_head.generation_id);

    let mut replacement = request_for_stream(
        "entity-run-stage-set",
        "enabled",
        "rebuilt",
        run_stage_set_files(br#"{"run":"two"}"#),
    );
    replacement.omit_logical_paths = vec!["link/link.json".to_string()];
    let replaced = publish_stream_patch(temp.path(), replacement).unwrap();
    let snapshot = open_current_stream_generation(temp.path(), "entity-run-stage-set").unwrap();

    assert_eq!(snapshot.generation_id, replaced.generation_id);
    assert_eq!(
        snapshot.read_logical_file("run/run.json"),
        Some(br#"{"run":"two"}"#.as_slice())
    );
    assert_eq!(snapshot.read_logical_file("link/link.json"), None);
    assert!(
        publication_object_path(temp.path(), &link_head.generation_id)
            .unwrap()
            .is_file(),
        "omission removes only the manifest reference and leaves old objects intact"
    );
}

#[test]
fn stream_patch_rejects_omit_without_current_head() {
    let temp = tempfile::tempdir().unwrap();
    let mut request = request_for_stream(
        "entity-run-stage-set",
        "enabled",
        "rebuilt",
        run_stage_set_files(br#"{"run":"one"}"#),
    );
    request.omit_logical_paths = vec!["link/link.json".to_string()];

    let err = publish_stream_patch(temp.path(), request).unwrap_err();

    assert_eq!(err.kind, EntityPublicationErrorKind::InvalidRequest);
    assert!(
        err.message
            .contains("cannot omit logical path link/link.json without a current head")
    );
    assert!(!err.writes_performed);
    assert_eq!(err.committed, Some(false));
}

#[test]
fn stream_patch_rejects_omit_for_missing_current_logical_path() {
    let temp = tempfile::tempdir().unwrap();
    publish_stream_patch(
        temp.path(),
        request_for_stream(
            "entity-run-stage-set",
            "enabled",
            "rebuilt",
            run_stage_set_files(br#"{"run":"one"}"#),
        ),
    )
    .unwrap();
    let mut request = request_for_stream(
        "entity-run-stage-set",
        "enabled",
        "rebuilt",
        vec![file(
            "run/run.json",
            "run",
            "canon_entity_run.v1",
            br#"{"run":"two"}"#,
        )],
    );
    request.omit_logical_paths = vec!["link/link.json".to_string()];

    let err = publish_stream_patch(temp.path(), request).unwrap_err();

    assert_eq!(err.kind, EntityPublicationErrorKind::InvalidRequest);
    assert!(
        err.message
            .contains("cannot omit missing logical path link/link.json")
    );
    assert!(!err.writes_performed);
    assert_eq!(err.committed, Some(false));
}

#[test]
fn stream_patch_rejects_link_child_when_run_head_advanced() {
    let temp = tempfile::tempdir().unwrap();
    let run_head = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "entity-run-stage-set",
            "enabled",
            "rebuilt",
            run_stage_set_files(br#"{"run":"one"}"#),
        ),
    )
    .unwrap();
    let advanced_head = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "entity-run-stage-set",
            "enabled",
            "rebuilt",
            run_stage_set_files(br#"{"run":"two"}"#),
        ),
    )
    .unwrap();
    let mut link_child = request_for_stream(
        "entity-run-stage-set",
        "enabled",
        "rebuilt",
        vec![file(
            "link/link.json",
            "link",
            "canon_entity_link.v1",
            br#"{"link":"one"}"#,
        )],
    );
    link_child.supersedes_generation_id = Some(run_head.generation_id.clone());

    let err = publish_stream_patch(temp.path(), link_child).unwrap_err();

    assert_eq!(err.kind, EntityPublicationErrorKind::ForkedGeneration);
    assert!(
        err.message.contains("already claimed by generation")
            && err.message.contains(&advanced_head.generation_id)
    );
    let current = open_current_stream_generation(temp.path(), "entity-run-stage-set").unwrap();
    assert_eq!(current.generation_id, advanced_head.generation_id);
    assert_eq!(current.read_logical_file("link/link.json"), None);
}

#[test]
fn stream_identical_patch_is_no_write_replay() {
    let temp = tempfile::tempdir().unwrap();
    let first = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "identical-replay",
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
    )
    .unwrap();
    let second = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "identical-replay",
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
        ),
    )
    .unwrap();

    assert_eq!(second.generation_id, first.generation_id);
    assert_eq!(second.outcome, EntityPublicationOutcome::AlreadyCommitted);
    assert!(!second.writes_performed);
    assert_eq!(second.committed, Some(true));
}

#[test]
fn stream_patch_failure_leaves_previous_head_current() {
    let temp = tempfile::tempdir().unwrap();
    let first = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "patch-failure",
            "enabled",
            "cold_miss",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"run-v1",
            )],
        ),
    )
    .unwrap();
    let err = publish_stream_patch_with_options(
        temp.path(),
        request_for_stream(
            "patch-failure",
            "enabled",
            "cold_miss",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"run-v2",
            )],
        ),
        EntityPublicationOptions {
            failpoint: EntityPublicationFailpoint::BeforeCommitMarker,
        },
    )
    .unwrap_err();

    assert!(err.writes_performed);
    assert_eq!(err.committed, Some(false));
    let current = open_current_stream_generation(temp.path(), "patch-failure").unwrap();
    assert_eq!(current.generation_id, first.generation_id);
    assert_eq!(
        current.read_logical_file("run/run.json"),
        Some(b"run-v1".as_slice())
    );
}

#[test]
fn stream_claim_failpoints_recover_without_partial_visibility() {
    let cases = [
        (
            EntityPublicationFailpoint::BeforeClaimDirectoryCreate,
            false,
            false,
        ),
        (
            EntityPublicationFailpoint::AfterClaimDirectoryCreate,
            true,
            false,
        ),
        (EntityPublicationFailpoint::BeforeClaimCreate, true, false),
        (EntityPublicationFailpoint::AfterClaimCreate, true, false),
        (EntityPublicationFailpoint::AfterClaimWrite, true, false),
        (EntityPublicationFailpoint::AfterClaimFlush, true, false),
        (EntityPublicationFailpoint::AfterClaimSync, true, false),
        (EntityPublicationFailpoint::BeforeClaimPublish, true, false),
        (
            EntityPublicationFailpoint::AfterClaimPublishBeforeParentSync,
            true,
            true,
        ),
    ];

    for (failpoint, writes_performed, stable_claim_visible) in cases {
        let temp = tempfile::tempdir().unwrap();
        let first = publish_stream_patch(
            temp.path(),
            request_for_stream(
                "claim-failpoint",
                "enabled",
                "cold_miss",
                vec![file(
                    "run/run.json",
                    "run",
                    "canon_entity_run.v1",
                    b"run-v1",
                )],
            ),
        )
        .unwrap();
        let claim_path = child_claim_path(temp.path(), &first.generation_id);
        let err = publish_stream_patch_with_options(
            temp.path(),
            request_for_stream(
                "claim-failpoint",
                "enabled",
                "cold_miss",
                vec![file(
                    "run/run.json",
                    "run",
                    "canon_entity_run.v1",
                    b"run-v2",
                )],
            ),
            EntityPublicationOptions { failpoint },
        )
        .unwrap_err();

        assert_eq!(err.writes_performed, writes_performed, "{failpoint:?}");
        assert_eq!(err.committed, Some(false), "{failpoint:?}");
        assert_eq!(claim_path.is_dir(), stable_claim_visible, "{failpoint:?}");
        if stable_claim_visible {
            let claimed_child = std::fs::read_to_string(claim_path.join("child")).unwrap();
            assert_eq!(claimed_child.trim(), err.generation_id.as_deref().unwrap());
        }
        let current = open_current_stream_generation(temp.path(), "claim-failpoint").unwrap();
        assert_eq!(current.generation_id, first.generation_id, "{failpoint:?}");
        assert_eq!(
            current.read_logical_file("run/run.json"),
            Some(b"run-v1".as_slice()),
            "{failpoint:?}"
        );

        let recovered = publish_stream_patch(
            temp.path(),
            request_for_stream(
                "claim-failpoint",
                "enabled",
                "cold_miss",
                vec![file(
                    "run/run.json",
                    "run",
                    "canon_entity_run.v1",
                    b"run-v2",
                )],
            ),
        )
        .unwrap();
        let current = open_current_stream_generation(temp.path(), "claim-failpoint").unwrap();
        assert_eq!(
            current.generation_id, recovered.generation_id,
            "{failpoint:?}"
        );
        assert_eq!(
            current.manifest.supersedes_generation_id.as_deref(),
            Some(first.generation_id.as_str()),
            "{failpoint:?}"
        );
        assert_eq!(
            current.read_logical_file("run/run.json"),
            Some(b"run-v2".as_slice()),
            "{failpoint:?}"
        );
    }
}

#[test]
fn racing_same_stream_generation_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = Arc::new(temp.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(8));
    let input = request_for_stream(
        "same-stream-race",
        "enabled",
        "cold_miss",
        vec![file("run/run.json", "run", "canon_entity_run.v1", b"run")],
    );

    let mut handles = Vec::new();
    for _ in 0..8 {
        let work_dir = Arc::clone(&work_dir);
        let barrier = Arc::clone(&barrier);
        let input = input.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            publish_stream_patch(&work_dir, input).unwrap()
        }));
    }

    let receipts = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    let generation_id = receipts[0].generation_id.clone();
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.generation_id == generation_id)
    );
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.committed == Some(true))
    );
    let current = open_current_stream_generation(temp.path(), "same-stream-race").unwrap();
    assert_eq!(current.generation_id, generation_id);
}

#[test]
fn divergent_racing_stream_children_leave_one_usable_head() {
    let temp = tempfile::tempdir().unwrap();
    let base = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "divergent-race",
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"base")],
        ),
    )
    .unwrap();
    let mut alpha = request_for_stream(
        "divergent-race",
        "enabled",
        "cold_miss",
        vec![file("run/run.json", "run", "canon_entity_run.v1", b"alpha")],
    );
    alpha.request_fingerprint = hash_bytes(b"alpha-request");
    alpha.supersedes_generation_id = Some(base.generation_id.clone());
    let mut beta = request_for_stream(
        "divergent-race",
        "enabled",
        "cold_miss",
        vec![file("run/run.json", "run", "canon_entity_run.v1", b"beta")],
    );
    beta.request_fingerprint = hash_bytes(b"beta-request");
    beta.supersedes_generation_id = Some(base.generation_id.clone());

    let work_dir = Arc::new(temp.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(2));
    let handles = [alpha, beta]
        .into_iter()
        .map(|input| {
            let work_dir = Arc::clone(&work_dir);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                publish_stream_patch(&work_dir, input)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    let successes = results.iter().filter(|result| result.is_ok()).count();
    let refusals = results.iter().filter(|result| result.is_err()).count();
    assert_eq!(successes, 1, "{results:?}");
    assert_eq!(refusals, 1, "{results:?}");
    let refusal = results.into_iter().find_map(Result::err).unwrap();
    assert!(matches!(
        refusal.kind,
        EntityPublicationErrorKind::ForkedGeneration | EntityPublicationErrorKind::InvalidRequest
    ));
    assert!(
        refusal.writes_performed,
        "divergent loser should report durable claim/object writes: {refusal:?}"
    );

    let current = open_current_stream_generation(temp.path(), "divergent-race").unwrap();
    assert_eq!(
        current.manifest.supersedes_generation_id,
        Some(base.generation_id)
    );
    let current_bytes = current.read_logical_file("run/run.json").unwrap();
    assert!(current_bytes == b"alpha" || current_bytes == b"beta");
}

#[test]
fn direct_unclaimed_generations_do_not_become_stream_heads() {
    let temp = tempfile::tempdir().unwrap();
    let unclaimed = publish_generation(
        temp.path(),
        request_for_stream(
            "external-fork",
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"alpha")],
        ),
    )
    .unwrap();
    let err = open_current_stream_generation(temp.path(), "external-fork").unwrap_err();
    assert_eq!(err.kind, EntityPublicationErrorKind::UncommittedGeneration);

    let claimed = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "external-fork",
            "enabled",
            "cold_miss",
            vec![file(
                "solve/solve.json",
                "solve",
                "canon_entity_solve.v1",
                b"claimed",
            )],
        ),
    )
    .unwrap();
    let mut bypass = request_for_stream(
        "external-fork",
        "disabled",
        "bypass",
        vec![file(
            "run/run.json",
            "run",
            "canon_entity_run.v1",
            b"bypass",
        )],
    );
    bypass.supersedes_generation_id = Some(claimed.generation_id.clone());
    let bypass = publish_generation(temp.path(), bypass).unwrap();

    assert_ne!(unclaimed.generation_id, claimed.generation_id);
    assert_ne!(bypass.generation_id, claimed.generation_id);
    let current = open_current_stream_generation(temp.path(), "external-fork").unwrap();
    assert_eq!(current.generation_id, claimed.generation_id);
    assert_eq!(
        current.read_logical_file("solve/solve.json"),
        Some(b"claimed".as_slice())
    );
}

#[test]
fn unrelated_stream_corruption_does_not_poison_healthy_stream() {
    let temp = tempfile::tempdir().unwrap();
    let healthy = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "healthy-stream",
            "enabled",
            "cold_miss",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"healthy",
            )],
        ),
    )
    .unwrap();
    let corrupted = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "corrupted-stream",
            "disabled",
            "bypass",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"corrupt",
            )],
        ),
    )
    .unwrap();
    let manifest_path = publication_object_path(temp.path(), &corrupted.generation_id).unwrap();
    let mut bytes = std::fs::read(&manifest_path).unwrap();
    bytes.push(b'\n');
    std::fs::write(manifest_path, bytes).unwrap();

    let current = open_current_stream_generation(temp.path(), "healthy-stream").unwrap();
    assert_eq!(current.generation_id, healthy.generation_id);
    assert_eq!(
        current.read_logical_file("run/run.json"),
        Some(b"healthy".as_slice())
    );
    assert!(open_current_stream_generation(temp.path(), "corrupted-stream").is_err());
}

#[test]
fn corrupt_supersession_refuses_current_stream_reader() {
    let temp = tempfile::tempdir().unwrap();
    let first = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "corrupt-supersession",
            "enabled",
            "cold_miss",
            vec![file("run/run.json", "run", "canon_entity_run.v1", b"base")],
        ),
    )
    .unwrap();
    let mut child = request_for_stream(
        "corrupt-supersession",
        "disabled",
        "bypass",
        vec![file("run/run.json", "run", "canon_entity_run.v1", b"child")],
    );
    child.supersedes_generation_id =
        Some("blake3:9999999999999999999999999999999999999999999999999999999999999999".to_string());
    let child = publish_generation(temp.path(), child).unwrap();
    write_child_claim(temp.path(), &first.generation_id, &child.generation_id);

    let err = open_current_stream_generation(temp.path(), "corrupt-supersession").unwrap_err();
    assert_eq!(err.kind, EntityPublicationErrorKind::CorruptGeneration);
}

#[test]
fn stream_reader_snapshot_never_mixes_generations() {
    let temp = tempfile::tempdir().unwrap();
    let first = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "snapshot-isolation",
            "enabled",
            "cold_miss",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"run-v1",
            )],
        ),
    )
    .unwrap();
    let first_snapshot = open_current_stream_generation(temp.path(), "snapshot-isolation").unwrap();
    let second = publish_stream_patch(
        temp.path(),
        request_for_stream(
            "snapshot-isolation",
            "enabled",
            "cold_miss",
            vec![file(
                "run/run.json",
                "run",
                "canon_entity_run.v1",
                b"run-v2",
            )],
        ),
    )
    .unwrap();

    assert_ne!(first.generation_id, second.generation_id);
    assert_eq!(
        first_snapshot.read_logical_file("run/run.json"),
        Some(b"run-v1".as_slice())
    );
    let current = open_current_stream_generation(temp.path(), "snapshot-isolation").unwrap();
    assert_eq!(current.generation_id, second.generation_id);
    assert_eq!(
        current.read_logical_file("run/run.json"),
        Some(b"run-v2".as_slice())
    );
}
