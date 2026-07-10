#![forbid(unsafe_code)]

#[path = "../src/fs_safety.rs"]
mod fs_safety;
#[path = "../src/project/workspace.rs"]
mod workspace;

use fs_safety::{
    FsSafetyErrorCode, diagnose_io_error, plan_atomic_publication, resolve_workspace_path,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tempfile::TempDir;
use workspace::{
    WorkspaceInput, WorkspaceOutput, WorkspacePolicy, allocate_temp_path, plan_workspace,
    validate_declared_mutation_target,
};

#[test]
fn path_traversal_and_owned_root_rules_refuse_before_mutation() {
    let fixture = workspace_fixture();
    let error = plan_workspace(
        &fixture.policy,
        &[],
        &[WorkspaceOutput {
            logical_field: "output.report".to_string(),
            relative_path: PathBuf::from("../escape.json"),
            atomic_publish: true,
        }],
    )
    .expect_err("path traversal must fail");
    assert_eq!(error.code, FsSafetyErrorCode::PathTraversal);
    assert_eq!(error.logical_field, "output.report");

    let error = plan_workspace(
        &fixture.policy,
        &[],
        &[WorkspaceOutput {
            logical_field: "output.report".to_string(),
            relative_path: PathBuf::from("outside/report.json"),
            atomic_publish: true,
        }],
    )
    .expect_err("owned root policy must fail");
    assert_eq!(error.code, FsSafetyErrorCode::OutputRootPolicy);
}

#[test]
fn input_output_overlap_and_undeclared_mutations_are_rejected() {
    let fixture = workspace_fixture();
    let input_path = fixture.workspace.path().join("artifacts/shared.json");
    fs::write(&input_path, b"seed").unwrap();

    let error = plan_workspace(
        &fixture.policy,
        &[WorkspaceInput {
            logical_field: "input.seed".to_string(),
            relative_path: PathBuf::from("artifacts/shared.json"),
        }],
        &[WorkspaceOutput {
            logical_field: "output.shared".to_string(),
            relative_path: PathBuf::from("artifacts/shared.json"),
            atomic_publish: true,
        }],
    )
    .expect_err("input/output overlap must fail");
    assert_eq!(error.code, FsSafetyErrorCode::InputOutputOverlap);

    let plan = plan_workspace(
        &fixture.policy,
        &[WorkspaceInput {
            logical_field: "input.seed".to_string(),
            relative_path: PathBuf::from("inputs/source.json"),
        }],
        &[WorkspaceOutput {
            logical_field: "output.report".to_string(),
            relative_path: PathBuf::from("artifacts/report.json"),
            atomic_publish: true,
        }],
    )
    .expect("workspace plan succeeds");
    let rogue_path = fixture.workspace.path().join("artifacts/rogue.json");
    let error = validate_declared_mutation_target(&plan, "output.rogue", &rogue_path)
        .expect_err("undeclared mutation must fail");
    assert_eq!(error.code, FsSafetyErrorCode::UndeclaredMutation);
}

#[test]
fn read_only_policy_refuses_outputs_without_changing_existing_artifacts() {
    let workspace = TempDir::new().unwrap();
    let inputs_dir = workspace.path().join("inputs");
    let artifacts_dir = workspace.path().join("artifacts");
    fs::create_dir_all(&inputs_dir).unwrap();
    fs::create_dir_all(&artifacts_dir).unwrap();
    let source_path = inputs_dir.join("source.json");
    let report_path = artifacts_dir.join("report.json");
    fs::write(&source_path, b"source").unwrap();
    fs::write(&report_path, b"existing").unwrap();

    let before = metadata_snapshot(&report_path);
    let policy = WorkspacePolicy {
        workspace_root: workspace.path().to_path_buf(),
        owned_output_roots: vec![PathBuf::from("artifacts")],
        temp_root: PathBuf::from("artifacts/.tmp"),
        read_only: true,
    };
    let error = plan_workspace(
        &policy,
        &[WorkspaceInput {
            logical_field: "input.source".to_string(),
            relative_path: PathBuf::from("inputs/source.json"),
        }],
        &[WorkspaceOutput {
            logical_field: "output.report".to_string(),
            relative_path: PathBuf::from("artifacts/report.json"),
            atomic_publish: true,
        }],
    )
    .expect_err("read-only workspace must refuse outputs");
    assert_eq!(error.code, FsSafetyErrorCode::ReadOnlyViolation);
    assert_eq!(metadata_snapshot(&report_path), before);
    assert_eq!(fs::read(&report_path).unwrap(), b"existing");
}

#[test]
fn atomic_publication_collision_preserves_previous_artifact() {
    let fixture = workspace_fixture();
    let report_path = fixture.workspace.path().join("artifacts/report.json");
    fs::write(&report_path, b"old").unwrap();

    let resolution = resolve_workspace_path(
        fixture.workspace.path(),
        "output.report",
        Path::new("artifacts/report.json"),
        fs_safety::PlannedAccess::Write,
    )
    .expect("output resolves");
    let temp_path = fs_safety::atomic_temp_sibling(&report_path, "canon-workspace");
    fs::write(&temp_path, b"busy").unwrap();

    let error = plan_atomic_publication(&resolution, "canon-workspace")
        .expect_err("temp collision must fail");
    assert_eq!(error.code, FsSafetyErrorCode::AtomicPublishConflict);
    assert_eq!(fs::read(&report_path).unwrap(), b"old");
    assert_eq!(fs::read(&temp_path).unwrap(), b"busy");
}

#[test]
fn temp_root_must_be_owned_and_allocations_stay_separate_from_inputs() {
    let workspace = TempDir::new().unwrap();
    let inputs_dir = workspace.path().join("inputs");
    let artifacts_dir = workspace.path().join("artifacts");
    fs::create_dir_all(&inputs_dir).unwrap();
    fs::create_dir_all(&artifacts_dir).unwrap();
    fs::write(inputs_dir.join("source.json"), b"source").unwrap();

    let bad_policy = WorkspacePolicy {
        workspace_root: workspace.path().to_path_buf(),
        owned_output_roots: vec![PathBuf::from("artifacts")],
        temp_root: PathBuf::from("inputs/tmp"),
        read_only: false,
    };
    let error = plan_workspace(
        &bad_policy,
        &[WorkspaceInput {
            logical_field: "input.source".to_string(),
            relative_path: PathBuf::from("inputs/source.json"),
        }],
        &[],
    )
    .expect_err("temp root outside owned outputs must fail");
    assert_eq!(error.code, FsSafetyErrorCode::OutputRootPolicy);

    let good_policy = WorkspacePolicy {
        workspace_root: workspace.path().to_path_buf(),
        owned_output_roots: vec![PathBuf::from("artifacts")],
        temp_root: PathBuf::from("artifacts/.tmp"),
        read_only: false,
    };
    let plan = plan_workspace(
        &good_policy,
        &[WorkspaceInput {
            logical_field: "input.source".to_string(),
            relative_path: PathBuf::from("inputs/source.json"),
        }],
        &[WorkspaceOutput {
            logical_field: "output.report".to_string(),
            relative_path: PathBuf::from("artifacts/report.json"),
            atomic_publish: false,
        }],
    )
    .expect("workspace plan succeeds");
    let temp_path = allocate_temp_path(&plan, "temp.buffer", "session/buffer.json")
        .expect("temp path allocates");
    assert!(temp_path.starts_with(workspace.path().join("artifacts/.tmp")));
}

#[test]
fn permission_and_quota_diagnostics_are_safe_and_actionable() {
    let quota_error = diagnose_io_error(
        "output.snapshot",
        &io::Error::other("quota exceeded"),
        "Free space or increase the workspace quota, then retry.",
    );
    assert_eq!(quota_error.code, FsSafetyErrorCode::QuotaExceeded);
    assert!(quota_error.message.contains("output.snapshot"));
    assert!(!quota_error.message.contains("/secure/"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let fixture = workspace_fixture();
        let locked_dir = fixture.workspace.path().join("artifacts");
        let report_path = locked_dir.join("report.json");
        let plan = fs_safety::AtomicPublicationPlan {
            logical_field: "output.report".to_string(),
            destination: report_path.clone(),
            temp_path: fs_safety::atomic_temp_sibling(&report_path, "canon-workspace"),
        };

        let original = fs::metadata(&locked_dir).unwrap().permissions().mode();
        fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o555)).unwrap();
        let error = workspace::publish_atomic(&plan, b"new bytes")
            .expect_err("permission denied must be surfaced safely");
        assert_eq!(error.code, FsSafetyErrorCode::PermissionDenied);
        assert_eq!(error.logical_field, "output.report");
        fs::set_permissions(&locked_dir, fs::Permissions::from_mode(original)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn symlink_escape_and_hard_link_aliasing_are_rejected() {
    use std::os::unix::fs::symlink;

    let fixture = workspace_fixture();
    let external = TempDir::new().unwrap();
    fs::create_dir_all(external.path().join("outside")).unwrap();
    fs::create_dir_all(fixture.workspace.path().join("artifacts")).unwrap();
    symlink(
        external.path().join("outside"),
        fixture.workspace.path().join("artifacts/escape"),
    )
    .unwrap();

    let error = plan_workspace(
        &fixture.policy,
        &[],
        &[WorkspaceOutput {
            logical_field: "output.escape".to_string(),
            relative_path: PathBuf::from("artifacts/escape/report.json"),
            atomic_publish: true,
        }],
    )
    .expect_err("symlink escape must fail");
    assert_eq!(error.code, FsSafetyErrorCode::WorkspaceEscape);

    let source_path = fixture.workspace.path().join("inputs/source.json");
    let hard_link_path = fixture.workspace.path().join("artifacts/report.json");
    fs::write(&source_path, b"source").unwrap();
    fs::hard_link(&source_path, &hard_link_path).unwrap();

    let error = plan_workspace(
        &fixture.policy,
        &[WorkspaceInput {
            logical_field: "input.source".to_string(),
            relative_path: PathBuf::from("inputs/source.json"),
        }],
        &[WorkspaceOutput {
            logical_field: "output.report".to_string(),
            relative_path: PathBuf::from("artifacts/report.json"),
            atomic_publish: false,
        }],
    )
    .expect_err("hard-link alias must fail");
    assert_eq!(error.code, FsSafetyErrorCode::HardLinkAlias);
}

fn workspace_fixture() -> WorkspaceFixture {
    let workspace = TempDir::new().unwrap();
    fs::create_dir_all(workspace.path().join("inputs")).unwrap();
    fs::create_dir_all(workspace.path().join("artifacts")).unwrap();
    fs::write(workspace.path().join("inputs/source.json"), b"source").unwrap();
    WorkspaceFixture {
        policy: WorkspacePolicy {
            workspace_root: workspace.path().to_path_buf(),
            owned_output_roots: vec![PathBuf::from("artifacts")],
            temp_root: PathBuf::from("artifacts/.tmp"),
            read_only: false,
        },
        workspace,
    }
}

struct WorkspaceFixture {
    policy: WorkspacePolicy,
    workspace: TempDir,
}

fn metadata_snapshot(path: &Path) -> (u64, bool, Option<SystemTime>) {
    let metadata = fs::metadata(path).unwrap();
    (
        metadata.len(),
        metadata.permissions().readonly(),
        metadata.modified().ok(),
    )
}
