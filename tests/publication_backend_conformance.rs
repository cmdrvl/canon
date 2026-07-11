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
    BackendCapabilities, ChannelCompareAndSwapRequest, FILESYSTEM_BACKEND_KIND,
    FILESYSTEM_BACKEND_SCHEMA_VERSION, FilesystemPublicationBackend,
    PROVIDER_URI_HANDLING_DEFERRED, PublicationBackend, PublicationErrorKind, PublicationOutcome,
    PublicationRequest, PublishedPackageRef,
};
use registry_transaction::{RegistryPublicationTransaction, publish_registry_transaction};
use serde_json::{Value, json};
use std::{
    env,
    error::Error,
    fs,
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

const WORKER_ENV: &str = "CANON_PUBLICATION_BACKEND_WORKER";
const ROOT_ENV: &str = "CANON_PUBLICATION_BACKEND_ROOT";
const CHANNEL_ENV: &str = "CANON_PUBLICATION_BACKEND_CHANNEL";
const BASE_ID_ENV: &str = "CANON_PUBLICATION_BACKEND_BASE_ID";
const BASE_VERSION_ENV: &str = "CANON_PUBLICATION_BACKEND_BASE_VERSION";
const BASE_DIGEST_ENV: &str = "CANON_PUBLICATION_BACKEND_BASE_DIGEST";
const EXPECTED_DIGEST_ENV: &str = "CANON_PUBLICATION_BACKEND_EXPECTED_DIGEST";
const WORKER_INDEX_ENV: &str = "CANON_PUBLICATION_BACKEND_WORKER_INDEX";

fn digest(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}

fn digest_hex(digest: &str) -> &str {
    digest.strip_prefix("blake3:").unwrap()
}

fn genesis(package_id: &str) -> PublishedPackageRef {
    PublishedPackageRef {
        package_id: package_id.to_string(),
        package_version: "0.0.0".to_string(),
        content_digest: digest("genesis"),
    }
}

fn package_bytes(package_id: &str, version: &str, payload: &str) -> Vec<u8> {
    let mut value = json!({
        "schema_version": "canon.test.package.v1",
        "package_id": package_id,
        "package_version": version,
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
        package_id: value["package_id"].as_str().unwrap().to_string(),
        package_version: value["package_version"].as_str().unwrap().to_string(),
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
            "description": "publication backend conformance fixture",
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
fn filesystem_backend_capabilities_are_cloud_free_and_defer_provider_uri_handling() {
    let temp = TempDir::new().unwrap();
    let backend = FilesystemPublicationBackend::new(temp.path());
    assert_eq!(backend.root(), temp.path());
    let capabilities = <FilesystemPublicationBackend as PublicationBackend>::capabilities(&backend);
    assert_eq!(backend.capabilities(), capabilities);

    assert_eq!(
        capabilities.schema_version,
        FILESYSTEM_BACKEND_SCHEMA_VERSION
    );
    assert_eq!(capabilities.backend_kind, FILESYSTEM_BACKEND_KIND);
    assert!(capabilities.content_addressed_objects);
    assert!(capabilities.read_by_digest);
    assert!(capabilities.create_if_absent);
    assert!(capabilities.compare_and_swap_tags);
    assert!(capabilities.immutable_package_history);
    assert!(capabilities.list_declared_ancestry);
    assert!(capabilities.deterministic_conflict_receipts);
    assert!(!capabilities.requires_network);
    assert_eq!(
        capabilities.provider_specific_uri_handling,
        PROVIDER_URI_HANDLING_DEFERRED
    );

    let cargo = fs::read_to_string("Cargo.toml").unwrap();
    for forbidden in [
        "aws-sdk",
        "aws_config",
        "aws-config",
        "rusoto",
        "object_store",
    ] {
        assert!(
            !cargo.contains(forbidden),
            "default canon binary must not depend on {forbidden}"
        );
    }
}

#[test]
fn filesystem_backend_conforms_to_low_level_seam() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let backend = FilesystemPublicationBackend::new(temp.path());
    let bytes = package_bytes("people", "1.0.0", "alpha");
    let candidate = candidate_ref(&bytes);

    assert!(backend.current_head("stable")?.is_none());
    assert!(backend.read_by_digest(&candidate.content_digest)?.is_none());

    let wrong_digest = digest("wrong-object");
    let error = backend
        .create_immutable_object_if_absent(&wrong_digest, &bytes)
        .unwrap_err();
    assert_eq!(error.kind, PublicationErrorKind::InvalidPackage);
    assert!(backend.read_by_digest(&wrong_digest)?.is_none());

    let first_write =
        backend.create_immutable_object_if_absent(&candidate.content_digest, &bytes)?;
    assert!(first_write.created);
    assert!(temp.path().join(&first_write.path).exists());

    let replay_write =
        backend.create_immutable_object_if_absent(&candidate.content_digest, &bytes)?;
    assert!(!replay_write.created);
    assert_eq!(replay_write.path, first_write.path);
    assert_eq!(
        backend
            .read_by_digest(&candidate.content_digest)?
            .expect("object bytes"),
        bytes
    );

    let history_path = format!(
        "packages/{}/{}/{}.json",
        candidate.package_id,
        candidate.package_version,
        digest_hex(&candidate.content_digest)
    );
    let absolute_history_path = temp.path().join(&history_path);
    fs::create_dir_all(absolute_history_path.parent().unwrap())?;
    fs::write(&absolute_history_path, b"{}")?;

    let base = genesis("people");
    let cas = backend.compare_and_swap_channel(ChannelCompareAndSwapRequest {
        channel: "stable".to_string(),
        expected_base: base.clone(),
        expected_channel_digest: None,
        candidate: candidate.clone(),
        object_path: first_write.path.clone(),
        history_path: history_path.clone(),
        candidate_immutably_stored: true,
    })?;
    assert_eq!(cas.outcome, PublicationOutcome::Published);
    assert_eq!(cas.current_head, candidate);
    assert_eq!(backend.current_head("stable")?.unwrap(), candidate);

    let replay = backend.compare_and_swap_channel(ChannelCompareAndSwapRequest {
        channel: "stable".to_string(),
        expected_base: base.clone(),
        expected_channel_digest: Some(base.content_digest.clone()),
        candidate: candidate.clone(),
        object_path: first_write.path,
        history_path,
        candidate_immutably_stored: true,
    })?;
    assert_eq!(replay.outcome, PublicationOutcome::AlreadyPublished);

    let divergent_bytes = package_bytes("people", "1.0.1", "divergent");
    let divergent = candidate_ref(&divergent_bytes);
    let error = backend
        .compare_and_swap_channel(ChannelCompareAndSwapRequest {
            channel: "stable".to_string(),
            expected_base: base.clone(),
            expected_channel_digest: Some(base.content_digest),
            candidate: divergent,
            object_path: "objects/blake3/not-written.json".to_string(),
            history_path: "packages/people/1.0.1/not-written.json".to_string(),
            candidate_immutably_stored: false,
        })
        .unwrap_err();

    assert_eq!(error.kind, PublicationErrorKind::Conflict);
    let conflict = error.conflict.expect("conflict receipt");
    assert_eq!(conflict.conflict_kind, "tag_conflict");
    assert_eq!(conflict.actual_head.unwrap(), candidate);
    assert!(!conflict.candidate_immutably_stored);
    assert!(
        conflict
            .recovery_plan
            .join("\n")
            .contains("retry publication")
    );
    Ok(())
}

#[test]
fn publish_records_ancestry_and_refuses_divergent_stale_heads() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let backend = FilesystemPublicationBackend::new(temp.path());
    let base_bytes = package_bytes("people", "1.0.0", "base");
    let base_receipt = backend.publish(request(
        "stable",
        genesis("people"),
        None,
        base_bytes.clone(),
    ))?;
    let base_ref = candidate_ref(&base_bytes);

    let winner_bytes = package_bytes("people", "1.0.1", "winner");
    let winner = backend.publish(request(
        "stable",
        base_ref.clone(),
        Some(base_receipt.current_channel_digest.clone()),
        winner_bytes,
    ))?;
    assert_eq!(winner.outcome, PublicationOutcome::Published);

    let ancestry = backend.list_ancestry(&winner.package)?;
    assert_eq!(ancestry.len(), 1);
    assert_eq!(ancestry[0].parent, base_ref.clone());
    assert_eq!(ancestry[0].child, winner.package);

    let stale_bytes = package_bytes("people", "1.0.1", "loser");
    let stale = candidate_ref(&stale_bytes);
    let error = backend
        .publish(request(
            "stable",
            base_ref,
            Some(base_receipt.current_channel_digest),
            stale_bytes,
        ))
        .unwrap_err();
    assert_eq!(error.kind, PublicationErrorKind::Conflict);
    let conflict = error.conflict.expect("conflict receipt");
    assert_eq!(conflict.conflict_kind, "tag_conflict");
    assert_eq!(conflict.candidate, stale);
    assert!(conflict.candidate_immutably_stored);
    assert!(conflict.actual_head.is_some());
    Ok(())
}

#[test]
fn unsafe_backend_capabilities_refuse_mutation_instead_of_emulating_cas() {
    let temp = TempDir::new().unwrap();
    let mut capabilities = BackendCapabilities::filesystem();
    capabilities.compare_and_swap_tags = false;
    let backend =
        FilesystemPublicationBackend::with_capabilities_for_test(temp.path(), capabilities);
    let bytes = package_bytes("people", "1.0.0", "alpha");

    let error = backend
        .publish(request("stable", genesis("people"), None, bytes))
        .unwrap_err();
    assert_eq!(error.kind, PublicationErrorKind::UnsafeBackend);
    assert!(backend.current_head("stable").unwrap().is_none());

    let mut capabilities = BackendCapabilities::filesystem();
    capabilities.requires_network = true;
    let backend =
        FilesystemPublicationBackend::with_capabilities_for_test(temp.path(), capabilities);
    let bytes = package_bytes("people", "1.0.0", "alpha");
    let error = backend
        .publish(request("stable", genesis("people"), None, bytes))
        .unwrap_err();
    assert_eq!(error.kind, PublicationErrorKind::UnsafeBackend);
}

#[test]
fn registry_transaction_uses_publication_backend_trait() -> Result<(), Box<dyn Error>> {
    fn publish_with_backend<B: PublicationBackend>(
        backend: &B,
        registry_dir: &Path,
    ) -> Result<registry_transaction::RegistryPublicationOutput, Box<dyn Error>> {
        Ok(publish_registry_transaction(
            backend,
            RegistryPublicationTransaction {
                registry_dir: registry_dir.to_path_buf(),
                channel: "stable".to_string(),
                expected_base: genesis("people"),
                expected_channel_digest: None,
            },
        )?)
    }

    let registry = TempDir::new()?;
    write_registry(registry.path(), "1.0.0", "Jane Doe")?;
    let backend_dir = TempDir::new()?;
    let backend = FilesystemPublicationBackend::new(backend_dir.path());

    let output = publish_with_backend(&backend, registry.path())?;

    assert_eq!(output.receipt.outcome, PublicationOutcome::Published);
    assert_eq!(output.package.package_id, "people");
    assert_eq!(output.package.package_version, "1.0.0");
    assert!(
        backend_dir
            .path()
            .join(&output.receipt.object_path)
            .exists()
    );
    assert!(
        backend_dir
            .path()
            .join(&output.receipt.history_path)
            .exists()
    );
    assert!(backend_dir.path().join(&output.receipt.tag_path).exists());
    Ok(())
}

#[test]
fn subprocess_workers_get_one_winner_from_filesystem_cas() -> Result<(), Box<dyn Error>> {
    if env::var_os(WORKER_ENV).is_some() {
        return Ok(());
    }

    let temp = TempDir::new()?;
    let backend = FilesystemPublicationBackend::new(temp.path());
    let base_bytes = package_bytes("people", "1.0.0", "base");
    let base_receipt = backend.publish(request(
        "stable",
        genesis("people"),
        None,
        base_bytes.clone(),
    ))?;
    let base_ref = candidate_ref(&base_bytes);

    let worker_count = 8;
    let exe = env::current_exe()?;
    let children = (0..worker_count)
        .map(|index| {
            Command::new(&exe)
                .arg("--exact")
                .arg("publication_backend_conformance_subprocess_worker")
                .arg("--nocapture")
                .env(WORKER_ENV, "1")
                .env(ROOT_ENV, temp.path())
                .env(CHANNEL_ENV, "stable")
                .env(BASE_ID_ENV, &base_ref.package_id)
                .env(BASE_VERSION_ENV, &base_ref.package_version)
                .env(BASE_DIGEST_ENV, &base_ref.content_digest)
                .env(EXPECTED_DIGEST_ENV, &base_receipt.current_channel_digest)
                .env(WORKER_INDEX_ENV, index.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut published = 0;
    let mut conflicts = 0;
    for child in children {
        let output = child.wait_with_output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "worker failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        if stdout.contains("worker_outcome=published") {
            published += 1;
        } else if stdout.contains("worker_outcome=conflict") {
            conflicts += 1;
        } else {
            panic!("worker did not report an outcome\nstdout:\n{stdout}\nstderr:\n{stderr}");
        }
    }

    assert_eq!(published, 1);
    assert_eq!(conflicts, worker_count - 1);
    Ok(())
}

#[test]
fn publication_backend_conformance_subprocess_worker() {
    if env::var_os(WORKER_ENV).is_none() {
        return;
    }

    let backend = FilesystemPublicationBackend::new(env::var(ROOT_ENV).unwrap());
    let base = PublishedPackageRef {
        package_id: env::var(BASE_ID_ENV).unwrap(),
        package_version: env::var(BASE_VERSION_ENV).unwrap(),
        content_digest: env::var(BASE_DIGEST_ENV).unwrap(),
    };
    let channel = env::var(CHANNEL_ENV).unwrap();
    let expected = env::var(EXPECTED_DIGEST_ENV).ok();
    let worker_index = env::var(WORKER_INDEX_ENV).unwrap();
    let bytes = package_bytes("people", "1.0.1", &format!("worker-{worker_index}"));

    match backend.publish(request(&channel, base, expected, bytes)) {
        Ok(receipt) => println!(
            "worker_outcome=published digest={}",
            receipt.current_channel_digest
        ),
        Err(error) if error.kind == PublicationErrorKind::Conflict => {
            let actual = error
                .conflict
                .and_then(|receipt| receipt.actual_head)
                .map(|head| head.content_digest)
                .unwrap_or_else(|| "none".to_string());
            println!("worker_outcome=conflict actual={actual}");
        }
        Err(error) => panic!("worker failed with unexpected publication error: {error:?}"),
    }
}

#[test]
fn oci_artifacts_doc_states_filesystem_first_boundary() {
    let doc = fs::read_to_string("docs/OCI_ARTIFACTS.md").unwrap();
    for phrase in [
        "filesystem-first",
        "cloud-free by default",
        "No AWS SDK",
        "expected base digest",
        "create-if-absent",
        "compare-and-swap",
        "refuse mutation rather than emulate CAS",
        "provider-specific URI handling remains deferred",
    ] {
        assert!(
            doc.contains(phrase),
            "missing documentation phrase: {phrase}"
        );
    }
}
