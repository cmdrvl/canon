#![forbid(unsafe_code)]

mod distribution {
    pub mod backend {
        include!("../src/distribution/backend.rs");
    }
}

mod registry {
    pub use canon::registry::{
        canonical_package_bytes, compile_registry_package, parse_registry_package,
    };
}

mod registry_transaction {
    include!("../src/registry/transaction.rs");
}

use distribution::backend::{
    FilesystemPublicationBackend, PublicationErrorKind, PublicationOutcome, PublicationRequest,
    PublishedPackageRef,
};
use registry_transaction::{RegistryPublicationTransaction, publish_registry_transaction};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::Path,
    sync::{Arc, Barrier},
    thread,
};
use tempfile::TempDir;

fn digest(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}

fn genesis() -> PublishedPackageRef {
    PublishedPackageRef {
        package_id: "people".to_string(),
        package_version: "0.0.0".to_string(),
        content_digest: digest("genesis"),
    }
}

fn package_bytes(package_id: &str, version: &str, payload: &str) -> Vec<u8> {
    let mut value = json!({
        "schema_version": "canon.registry.package.v1",
        "registry": {
            "id": package_id,
            "version": version
        },
        "content_digest": "",
        "payload": payload
    });
    let content_digest = {
        let mut digest_view = value.clone();
        digest_view["content_digest"] = Value::String(String::new());
        format!(
            "blake3:{}",
            blake3::hash(&serde_json::to_vec(&digest_view).unwrap()).to_hex()
        )
    };
    value["content_digest"] = Value::String(content_digest);
    serde_json::to_vec(&value).unwrap()
}

fn candidate_ref(bytes: &[u8]) -> PublishedPackageRef {
    let value: Value = serde_json::from_slice(bytes).unwrap();
    PublishedPackageRef {
        package_id: value["registry"]["id"].as_str().unwrap().to_string(),
        package_version: value["registry"]["version"].as_str().unwrap().to_string(),
        content_digest: value["content_digest"].as_str().unwrap().to_string(),
    }
}

fn request(
    channel: &str,
    base: PublishedPackageRef,
    expected_channel_digest: Option<String>,
    bytes: Vec<u8>,
) -> PublicationRequest {
    PublicationRequest {
        channel: channel.to_string(),
        expected_base: base,
        expected_channel_digest,
        candidate_package_bytes: bytes,
    }
}

fn write_registry(dir: &Path, version: &str, alias: &str) -> Result<(), Box<dyn Error>> {
    fs::write(
        dir.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "people",
            "version": version,
            "description": "publication transaction fixture",
            "updated": "2026-07-10",
            "entry_count": 1
        }))?,
    )?;
    fs::write(
        dir.join("aliases.json"),
        serde_json::to_vec_pretty(&json!([
            {
                "input": alias,
                "canonical_id": "PPL-001",
                "canonical_type": "person",
                "rule_id": "MANUAL"
            }
        ]))?,
    )?;
    Ok(())
}

#[test]
fn publication_requires_expected_base_digest_and_version() {
    let temp = TempDir::new().unwrap();
    let backend = FilesystemPublicationBackend::new(temp.path());
    assert_eq!(backend.root(), temp.path());
    assert!(backend.capabilities().content_addressed_objects);
    let bytes = package_bytes("people", "1.0.0", "alpha");
    let error = backend
        .publish(request(
            "stable",
            PublishedPackageRef {
                package_id: "people".to_string(),
                package_version: String::new(),
                content_digest: String::new(),
            },
            None,
            bytes,
        ))
        .unwrap_err();

    assert_eq!(error.kind, PublicationErrorKind::MissingExpectedBase);
    assert!(error.message.contains("expected base digest and version"));
}

#[test]
fn publication_builds_immutable_objects_before_channel_cas() {
    let temp = TempDir::new().unwrap();
    let backend = FilesystemPublicationBackend::new(temp.path());
    let base = genesis();
    let bytes = package_bytes("people", "1.0.0", "alpha");
    let expected = candidate_ref(&bytes);

    let first = backend
        .publish(request("stable", base.clone(), None, bytes.clone()))
        .unwrap();
    assert_eq!(first.outcome, PublicationOutcome::Published);
    assert_eq!(first.package, expected);
    assert!(temp.path().join(&first.object_path).exists());
    assert!(temp.path().join(&first.history_path).exists());
    assert!(temp.path().join(&first.tag_path).exists());
    assert_eq!(first.ancestry.parent, base);

    let replay = backend
        .publish(request(
            "stable",
            genesis(),
            Some(genesis().content_digest),
            bytes,
        ))
        .unwrap();
    assert_eq!(replay.outcome, PublicationOutcome::AlreadyPublished);
    assert_eq!(replay.current_channel_digest, first.current_channel_digest);
}

#[test]
fn racing_divergent_workers_accepts_one_linear_successor() {
    let temp = TempDir::new().unwrap();
    let backend = FilesystemPublicationBackend::new(temp.path());
    let base_bytes = package_bytes("people", "1.0.0", "base");
    let base_receipt = backend
        .publish(request("stable", genesis(), None, base_bytes.clone()))
        .unwrap();
    let base_ref = candidate_ref(&base_bytes);

    let worker_count = 16;
    let barrier = Arc::new(Barrier::new(worker_count + 1));
    let handles = (0..worker_count)
        .map(|index| {
            let barrier = Arc::clone(&barrier);
            let backend = backend.clone();
            let base_ref = base_ref.clone();
            let base_digest = base_receipt.current_channel_digest.clone();
            thread::spawn(move || {
                let bytes = package_bytes("people", "1.0.1", &format!("candidate-{index}"));
                barrier.wait();
                backend.publish(request("stable", base_ref, Some(base_digest), bytes))
            })
        })
        .collect::<Vec<_>>();

    barrier.wait();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    let accepted = results.iter().filter(|result| result.is_ok()).count();
    let conflicts = results.iter().filter(|result| result.is_err()).count();
    assert_eq!(accepted, 1);
    assert_eq!(conflicts, worker_count - 1);

    let head = backend.current_head("stable").unwrap().unwrap();
    let accepted_digest = results
        .iter()
        .find_map(|result| result.as_ref().ok())
        .unwrap()
        .current_channel_digest
        .clone();
    assert_eq!(head.content_digest, accepted_digest);

    for conflict in results.iter().filter_map(|result| result.as_ref().err()) {
        let receipt = conflict.conflict.as_ref().expect("conflict receipt");
        assert_eq!(receipt.conflict_kind, "tag_conflict");
        assert_eq!(
            receipt.actual_head.as_ref().unwrap().content_digest,
            accepted_digest
        );
        assert!(receipt.candidate_immutably_stored);
        assert!(
            receipt
                .recovery_plan
                .iter()
                .any(|step| step.contains("retry publication"))
        );
    }
}

#[test]
fn stale_divergent_candidate_gets_actual_head_and_rebase_plan() {
    let temp = TempDir::new().unwrap();
    let backend = FilesystemPublicationBackend::new(temp.path());
    let base_bytes = package_bytes("people", "1.0.0", "base");
    let base_receipt = backend
        .publish(request("stable", genesis(), None, base_bytes.clone()))
        .unwrap();
    let base_ref = candidate_ref(&base_bytes);

    let winner_bytes = package_bytes("people", "1.0.1", "winner");
    let winner = backend
        .publish(request(
            "stable",
            base_ref.clone(),
            Some(base_receipt.current_channel_digest.clone()),
            winner_bytes,
        ))
        .unwrap();

    let stale_bytes = package_bytes("people", "1.0.1", "loser");
    let error = backend
        .publish(request(
            "stable",
            base_ref,
            Some(base_receipt.current_channel_digest),
            stale_bytes,
        ))
        .unwrap_err();

    let conflict = error.conflict.expect("conflict receipt");
    assert_eq!(conflict.conflict_kind, "tag_conflict");
    assert_eq!(
        conflict.actual_head.unwrap().content_digest,
        winner.current_channel_digest
    );
    assert!(
        conflict
            .recovery_plan
            .join("\n")
            .contains("rebuild candidate")
    );
}

#[test]
fn transaction_builds_and_verifies_registry_package_before_publish() -> Result<(), Box<dyn Error>> {
    let registry = TempDir::new()?;
    write_registry(registry.path(), "1.0.0", "Jane Doe")?;
    let backend_dir = TempDir::new()?;
    let backend = FilesystemPublicationBackend::new(backend_dir.path());

    let output = publish_registry_transaction(
        &backend,
        RegistryPublicationTransaction {
            registry_dir: registry.path().to_path_buf(),
            channel: "stable".to_string(),
            expected_base: genesis(),
            expected_channel_digest: None,
        },
    )?;

    assert_eq!(output.receipt.outcome, PublicationOutcome::Published);
    assert_eq!(output.package.package_id, "people");
    assert_eq!(output.package.package_version, "1.0.0");
    assert!(backend_dir.path().join(output.receipt.object_path).exists());
    assert!(
        backend_dir
            .path()
            .join(output.receipt.history_path)
            .exists()
    );
    Ok(())
}

#[test]
fn interrupted_after_immutable_publication_can_recover_by_updating_tag() {
    let temp = TempDir::new().unwrap();
    let backend = FilesystemPublicationBackend::new(temp.path());
    let bytes = package_bytes("people", "1.0.0", "alpha");

    let first = backend
        .publish(request("stable", genesis(), None, bytes.clone()))
        .unwrap();
    let tag = temp.path().join(&first.tag_path);
    fs::remove_file(&tag).unwrap();
    assert!(temp.path().join(&first.object_path).exists());
    assert!(temp.path().join(&first.history_path).exists());

    let recovered = backend
        .publish(request("stable", genesis(), None, bytes))
        .unwrap();
    assert_eq!(recovered.outcome, PublicationOutcome::Published);
    assert_eq!(
        recovered.current_channel_digest,
        first.current_channel_digest
    );
    assert!(tag.exists());
}
