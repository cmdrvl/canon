use canon::registry::{
    RegistryMergeChangeKind, RegistryMergeDecision, RegistryPackage,
    RegistryPackageAttachmentDescriptor, RegistryPackageDependencyReference,
    RegistryPackageDescriptor, compile_registry_package, plan_registry_merge,
    plan_registry_package_merge,
};
use serde::Serialize;
use std::{fs, path::Path};
use tempfile::TempDir;

#[derive(Clone, Serialize)]
struct MappingEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

fn entry(input: &str, canonical_id: &str) -> MappingEntry {
    MappingEntry {
        input: input.to_string(),
        canonical_id: canonical_id.to_string(),
        canonical_type: "issuer".to_string(),
        rule_id: "MANUAL".to_string(),
    }
}

fn write_registry(version: &str, entries: &[MappingEntry]) -> TempDir {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join("registry.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "neutral-registry",
            "version": version,
            "description": "neutral merge fixture",
            "updated": "2026-07-11",
            "entry_count": entries.len()
        }))
        .expect("registry json serializes"),
    )
    .expect("registry json writes");
    fs::write(
        temp.path().join("mappings.json"),
        serde_json::to_vec_pretty(entries).expect("mappings serialize"),
    )
    .expect("mappings write");
    temp
}

fn tree_digest(root: &Path) -> String {
    let mut files = fs::read_dir(root)
        .expect("read dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();

    let mut hasher = blake3::Hasher::new();
    for path in files {
        hasher.update(path.file_name().unwrap().to_string_lossy().as_bytes());
        hasher.update(&fs::read(path).expect("read file"));
    }
    hasher.finalize().to_hex().to_string()
}

fn digest(seed: char) -> String {
    format!("blake3:{}", seed.to_string().repeat(64))
}

fn descriptor(path: &str, kind: &str, seed: char) -> RegistryPackageDescriptor {
    RegistryPackageDescriptor {
        path: path.to_string(),
        kind: kind.to_string(),
        content_digest: digest(seed),
        bytes: 10,
        entry_count: None,
    }
}

fn package_fixture() -> RegistryPackage {
    let registry = write_registry("1.0.0", &[entry("A", "CANON-A")]);
    compile_registry_package(registry.path()).expect("package compiles")
}

#[test]
fn non_conflicting_additions_merge_deterministically_without_mutating_inputs() {
    let base = write_registry("1.0.0", &[entry("A", "CANON-A")]);
    let ours = write_registry("1.1.0", &[entry("A", "CANON-A"), entry("B", "CANON-B")]);
    let theirs = write_registry("1.2.0", &[entry("A", "CANON-A"), entry("C", "CANON-C")]);
    let before = [
        tree_digest(base.path()),
        tree_digest(ours.path()),
        tree_digest(theirs.path()),
    ];

    let plan = plan_registry_merge(base.path(), ours.path(), theirs.path()).expect("merge plan");

    assert!(!plan.requires_operator_decision);
    assert_eq!(plan.summary.total_inputs, 3);
    assert_eq!(plan.summary.unchanged, 1);
    assert_eq!(plan.summary.auto_mergeable, 2);
    assert_eq!(plan.summary.operator_decisions, 0);
    assert_eq!(plan.summary.package_changes, 0);
    assert_eq!(
        plan.proposed_write_plan
            .iter()
            .map(|action| action.input.as_str())
            .collect::<Vec<_>>(),
        vec!["B", "C"]
    );
    assert_eq!(
        before,
        [
            tree_digest(base.path()),
            tree_digest(ours.path()),
            tree_digest(theirs.path())
        ]
    );
}

#[test]
fn identical_additions_are_idempotent_but_deletions_require_operator_review() {
    let base = write_registry("1.0.0", &[entry("A", "CANON-A"), entry("D", "CANON-D")]);
    let ours = write_registry("1.1.0", &[entry("A", "CANON-A"), entry("B", "CANON-B")]);
    let theirs = write_registry("1.2.0", &[entry("A", "CANON-A"), entry("B", "CANON-B")]);

    let plan = plan_registry_merge(base.path(), ours.path(), theirs.path()).expect("merge plan");

    assert!(plan.requires_operator_decision);
    assert_eq!(plan.summary.idempotent, 1);
    assert_eq!(plan.summary.deletions, 1);
    assert_eq!(
        plan.changes
            .iter()
            .map(|change| change.kind)
            .collect::<Vec<_>>(),
        vec![
            RegistryMergeChangeKind::Idempotent,
            RegistryMergeChangeKind::Deletion
        ]
    );
    assert_eq!(plan.proposed_write_plan.len(), 1);
    assert_eq!(plan.proposed_write_plan[0].input, "B");
}

#[test]
fn conflicting_identity_assertions_never_auto_resolve() {
    let base = write_registry("1.0.0", &[entry("A", "CANON-A")]);
    let ours = write_registry("1.1.0", &[entry("A", "CANON-A"), entry("X", "CANON-X1")]);
    let theirs = write_registry("1.2.0", &[entry("A", "CANON-A"), entry("X", "CANON-X2")]);

    let plan = plan_registry_merge(base.path(), ours.path(), theirs.path()).expect("merge plan");
    let conflict = plan
        .changes
        .iter()
        .find(|change| change.input.as_deref() == Some("X"))
        .expect("X conflict");

    assert!(plan.requires_operator_decision);
    assert_eq!(plan.summary.conflicts, 1);
    assert_eq!(plan.summary.auto_mergeable, 0);
    assert_eq!(conflict.kind, RegistryMergeChangeKind::AliasTargetConflict);
    assert_eq!(
        conflict.decision,
        RegistryMergeDecision::OperatorDecisionRequired
    );
    assert!(plan.proposed_write_plan.is_empty());
    assert_eq!(
        conflict.blast_radius.affected_canonical_ids,
        vec!["CANON-X1", "CANON-X2"]
    );
}

#[test]
fn package_provenance_sidecar_and_temporal_changes_are_operator_decisions() {
    let base = package_fixture();
    let mut ours = base.clone();
    ours.build_provenance = Some(descriptor("_build.json", "build_provenance", '1'));
    ours.attachments.push(RegistryPackageAttachmentDescriptor {
        path: "_attachments/audit.json".to_string(),
        kind: "audit".to_string(),
        content_digest: digest('a'),
        bytes: 10,
    });
    ours.dependency_references
        .push(RegistryPackageDependencyReference {
            id: "temporal-snapshot".to_string(),
            version: "2026.07.10".to_string(),
            content_digest: digest('b'),
        });
    ours.content_digest = digest('c');

    let mut theirs = base.clone();
    theirs.build_provenance = Some(descriptor("_build.json", "build_provenance", '2'));
    theirs
        .attachments
        .push(RegistryPackageAttachmentDescriptor {
            path: "_attachments/signature.json".to_string(),
            kind: "signature".to_string(),
            content_digest: digest('d'),
            bytes: 10,
        });
    theirs
        .dependency_references
        .push(RegistryPackageDependencyReference {
            id: "temporal-snapshot".to_string(),
            version: "2026.07.11".to_string(),
            content_digest: digest('e'),
        });
    theirs.content_digest = digest('f');

    let plan = plan_registry_package_merge(&base, &ours, &theirs).expect("merge plan");
    let kinds = plan
        .changes
        .iter()
        .map(|change| change.kind)
        .collect::<Vec<_>>();

    assert!(plan.requires_operator_decision);
    assert!(kinds.contains(&RegistryMergeChangeKind::ProvenanceChange));
    assert!(kinds.contains(&RegistryMergeChangeKind::SidecarScope));
    assert!(kinds.contains(&RegistryMergeChangeKind::TemporalOverlap));
    assert_eq!(plan.summary.package_changes, 3);
    assert!(plan.proposed_write_plan.is_empty());
}
