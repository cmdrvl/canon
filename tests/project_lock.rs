#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/project/lock.rs"]
mod lock;

use lock::{
    CANON_PROJECT_LOCK_VERSION, ProjectLockDiffKind, ProjectLockErrorCode,
    ProjectLockManifestProjection, ProjectLockRefKind, ProjectLockResolvedRef,
    ProjectLockVerificationStatus, canonical_project_lock_bytes, digest_bytes, project_lock_digest,
    project_lock_schema_version, refresh_project_lock, refresh_project_lock_receipt,
    verify_project_lock,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.project.lock.v1.schema.json");

#[test]
fn schema_declares_deterministic_read_only_refresh_contract() {
    let schema = serde_json::from_str::<Value>(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_PROJECT_LOCK_VERSION);
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        CANON_PROJECT_LOCK_VERSION
    );
    assert_eq!(schema["x-canon-contract"]["deterministic_sort_order"], true);
    assert_eq!(schema["x-canon-contract"]["read_only_verify"], true);
    assert_eq!(schema["x-canon-contract"]["explicit_refresh_only"], true);
    assert_eq!(schema["x-canon-contract"]["reject_absolute_paths"], true);
    assert_eq!(
        schema["x-canon-contract"]["reject_secret_like_values"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["reject_timestamps_in_hash"],
        true
    );
    assert_eq!(project_lock_schema_version(), CANON_PROJECT_LOCK_VERSION);
}

#[test]
fn canonical_bytes_are_identical_under_shuffled_projection_order() {
    let left = refresh_project_lock(&projection(
        vec![
            ("source.beta", "feeds/right.jsonl", b"{\"id\":2}\n"),
            ("source.alpha", "feeds/left.csv", b"id\n1\n"),
        ],
        vec![
            (
                "tool.operator",
                ProjectLockRefKind::ToolContract,
                b"operator-v1",
            ),
            (
                "policy.review",
                ProjectLockRefKind::Policy,
                b"review-policy-v1",
            ),
            (
                "strategy.default",
                ProjectLockRefKind::Strategy,
                b"strategy-v1",
            ),
            (
                "package.registry",
                ProjectLockRefKind::Package,
                b"registry-v1",
            ),
        ],
    ))
    .expect("left lock builds");
    let right = refresh_project_lock(&projection(
        vec![
            ("source.alpha", "feeds/left.csv", b"id\n1\n"),
            ("source.beta", "feeds/right.jsonl", b"{\"id\":2}\n"),
        ],
        vec![
            (
                "package.registry",
                ProjectLockRefKind::Package,
                b"registry-v1",
            ),
            (
                "strategy.default",
                ProjectLockRefKind::Strategy,
                b"strategy-v1",
            ),
            (
                "policy.review",
                ProjectLockRefKind::Policy,
                b"review-policy-v1",
            ),
            (
                "tool.operator",
                ProjectLockRefKind::ToolContract,
                b"operator-v1",
            ),
        ],
    ))
    .expect("right lock builds");

    assert_eq!(
        canonical_project_lock_bytes(&left).expect("left bytes"),
        canonical_project_lock_bytes(&right).expect("right bytes")
    );
    assert_eq!(
        project_lock_digest(&left).expect("left digest"),
        project_lock_digest(&right).expect("right digest")
    );
}

#[test]
fn changed_input_bytes_produce_actionable_stale_diff() {
    let baseline = refresh_project_lock(&projection(
        vec![("source.alpha", "feeds/left.csv", b"id\n1\n")],
        vec![
            (
                "package.registry",
                ProjectLockRefKind::Package,
                b"registry-v1",
            ),
            (
                "strategy.default",
                ProjectLockRefKind::Strategy,
                b"strategy-v1",
            ),
            (
                "policy.review",
                ProjectLockRefKind::Policy,
                b"review-policy-v1",
            ),
            (
                "tool.operator",
                ProjectLockRefKind::ToolContract,
                b"operator-v1",
            ),
        ],
    ))
    .expect("baseline lock builds");

    let verification = verify_project_lock(
        &baseline,
        &projection(
            vec![("source.alpha", "feeds/left.csv", b"id\n2\n")],
            vec![
                (
                    "package.registry",
                    ProjectLockRefKind::Package,
                    b"registry-v1",
                ),
                (
                    "strategy.default",
                    ProjectLockRefKind::Strategy,
                    b"strategy-v1",
                ),
                (
                    "policy.review",
                    ProjectLockRefKind::Policy,
                    b"review-policy-v1",
                ),
                (
                    "tool.operator",
                    ProjectLockRefKind::ToolContract,
                    b"operator-v1",
                ),
            ],
        ),
    )
    .expect("verification succeeds");

    assert_eq!(verification.status, ProjectLockVerificationStatus::Stale);
    let diff = verification
        .stale_diffs
        .iter()
        .find(|diff| diff.subject == "source.alpha" && diff.field == "content_digest")
        .expect("input digest diff exists");
    assert_eq!(diff.kind, ProjectLockDiffKind::InputDrift);
    assert!(diff.message.contains("explicit lock refresh"));
}

#[test]
fn changed_resolved_digest_and_tool_contract_drift_are_reported() {
    let baseline = refresh_project_lock(&projection(
        vec![("source.alpha", "feeds/left.csv", b"id\n1\n")],
        vec![
            (
                "package.registry",
                ProjectLockRefKind::Package,
                b"registry-v1",
            ),
            (
                "strategy.default",
                ProjectLockRefKind::Strategy,
                b"strategy-v1",
            ),
            (
                "policy.review",
                ProjectLockRefKind::Policy,
                b"review-policy-v1",
            ),
            (
                "tool.operator",
                ProjectLockRefKind::ToolContract,
                b"operator-v1",
            ),
        ],
    ))
    .expect("baseline lock builds");

    let verification = verify_project_lock(
        &baseline,
        &projection(
            vec![("source.alpha", "feeds/left.csv", b"id\n1\n")],
            vec![
                (
                    "package.registry",
                    ProjectLockRefKind::Package,
                    b"registry-v2",
                ),
                (
                    "strategy.default",
                    ProjectLockRefKind::Strategy,
                    b"strategy-v1",
                ),
                (
                    "policy.review",
                    ProjectLockRefKind::Policy,
                    b"review-policy-v1",
                ),
                (
                    "tool.operator",
                    ProjectLockRefKind::ToolContract,
                    b"operator-v2",
                ),
            ],
        ),
    )
    .expect("verification succeeds");

    assert_eq!(verification.status, ProjectLockVerificationStatus::Stale);
    assert!(
        verification
            .stale_diffs
            .iter()
            .any(|diff| diff.kind == ProjectLockDiffKind::ResolvedDigestDrift
                && diff.subject == "package.registry")
    );
    assert!(
        verification
            .stale_diffs
            .iter()
            .any(|diff| diff.kind == ProjectLockDiffKind::ToolContractDrift
                && diff.subject == "tool.operator")
    );
}

#[test]
fn explicit_refresh_changes_lock_digest() {
    let baseline_projection = projection(
        vec![("source.alpha", "feeds/left.csv", b"id\n1\n")],
        vec![
            (
                "package.registry",
                ProjectLockRefKind::Package,
                b"registry-v1",
            ),
            (
                "strategy.default",
                ProjectLockRefKind::Strategy,
                b"strategy-v1",
            ),
            (
                "policy.review",
                ProjectLockRefKind::Policy,
                b"review-policy-v1",
            ),
            (
                "tool.operator",
                ProjectLockRefKind::ToolContract,
                b"operator-v1",
            ),
        ],
    );
    let baseline = refresh_project_lock(&baseline_projection).expect("baseline lock builds");

    let receipt = refresh_project_lock_receipt(
        &baseline,
        &projection(
            vec![("source.alpha", "feeds/left.csv", b"id\n2\n")],
            vec![
                (
                    "package.registry",
                    ProjectLockRefKind::Package,
                    b"registry-v1",
                ),
                (
                    "strategy.default",
                    ProjectLockRefKind::Strategy,
                    b"strategy-v1",
                ),
                (
                    "policy.review",
                    ProjectLockRefKind::Policy,
                    b"review-policy-v1",
                ),
                (
                    "tool.operator",
                    ProjectLockRefKind::ToolContract,
                    b"operator-v1",
                ),
            ],
        ),
    )
    .expect("refresh receipt builds");

    assert_eq!(receipt.schema_version, CANON_PROJECT_LOCK_VERSION);
    assert_eq!(receipt.previous_lock_digest, baseline.lock_digest);
    assert_ne!(receipt.previous_lock_digest, receipt.refreshed_lock_digest);
    assert_eq!(
        receipt.refreshed_lock.lock_digest,
        receipt.refreshed_lock_digest
    );
}

#[test]
fn absolute_paths_secret_like_values_and_timestamps_are_rejected() {
    let absolute_path_error = refresh_project_lock(&projection(
        vec![("source.alpha", "/tmp/private.csv", b"id\n1\n")],
        vec![(
            "tool.operator",
            ProjectLockRefKind::ToolContract,
            b"operator-v1",
        )],
    ))
    .expect_err("absolute path refuses");
    assert_eq!(absolute_path_error.code, ProjectLockErrorCode::PathPolicy);

    let secret_error = refresh_project_lock(&projection(
        vec![("source.alpha", "feeds/left.csv", b"id\n1\n")],
        vec![(
            "env:TOP_SECRET",
            ProjectLockRefKind::ToolContract,
            b"operator-v1",
        )],
    ))
    .expect_err("secret-like ref refuses");
    assert_eq!(secret_error.code, ProjectLockErrorCode::SecretPolicy);

    let timestamp_error = refresh_project_lock(&ProjectLockManifestProjection {
        project_id: "2026-07-10T00:00:00Z".to_string(),
        project_digest: digest_bytes(b"project-alpha"),
        inputs: vec![lock::ProjectLockInput {
            input_id: "source.alpha".to_string(),
            relative_path: "feeds/left.csv".to_string(),
            content_digest: digest_bytes(b"id\n1\n"),
        }],
        resolved_refs: vec![ProjectLockResolvedRef {
            ref_id: "tool.operator".to_string(),
            kind: ProjectLockRefKind::ToolContract,
            resolved_digest: digest_bytes(b"operator-v1"),
        }],
    })
    .expect_err("timestamp-like value refuses");
    assert_eq!(timestamp_error.code, ProjectLockErrorCode::ArtifactContract);
}

fn projection(
    inputs: Vec<(&str, &str, &[u8])>,
    refs: Vec<(&str, ProjectLockRefKind, &[u8])>,
) -> ProjectLockManifestProjection {
    ProjectLockManifestProjection {
        project_id: "project.synthetic.alpha".to_string(),
        project_digest: digest_bytes(b"project-alpha"),
        inputs: inputs
            .into_iter()
            .map(|(input_id, relative_path, bytes)| lock::ProjectLockInput {
                input_id: input_id.to_string(),
                relative_path: relative_path.to_string(),
                content_digest: digest_bytes(bytes),
            })
            .collect(),
        resolved_refs: refs
            .into_iter()
            .map(|(ref_id, kind, bytes)| ProjectLockResolvedRef {
                ref_id: ref_id.to_string(),
                kind,
                resolved_digest: digest_bytes(bytes),
            })
            .collect(),
    }
}
