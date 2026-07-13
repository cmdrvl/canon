#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_INDEX_VERSION_V1,
        index::{
            EntityIndexBuildRequest, EntityIndexCacheStatus, index_build_v1_report,
            run_index_build_v1,
        },
        prepare::{PrepareRunRequest, run_prepare_v1},
        schema::{validate_artifact_v1_core_contract, validate_entity_v1_self_hash},
    },
};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn entity_index_build_api_writes_artifacts_and_reuses_verified_cache() {
    let fixture = IndexCliFixture::new();
    fixture.prepare();

    let first = run_index_build_v1(fixture.index_request(None)).expect("index build succeeds");
    assert_eq!(first.cache_status, EntityIndexCacheStatus::Rebuilt);
    assert_eq!(first.artifact["version"], CANON_ENTITY_INDEX_VERSION_V1);
    assert_eq!(
        validate_artifact_v1_core_contract(&first.artifact)
            .expect("index v1 core contract")
            .artifact_version,
        CANON_ENTITY_INDEX_VERSION_V1
    );
    assert_eq!(
        validate_entity_v1_self_hash(&first.artifact).expect("index v1 self hash"),
        first.artifact["artifact_content_hash"]
            .as_str()
            .expect("hash")
    );
    assert!(first.paths.artifact_path.exists());
    assert!(first.paths.cache_key_path.exists());
    assert!(first.paths.postings_path.exists());
    assert!(first.paths.diagnostics_path.exists());

    let first_bytes = fs::read(&first.paths.artifact_path).expect("index artifact bytes");
    let first_report = index_build_v1_report(&first);
    assert_eq!(first_report.version, "canon_entity_index_build.v1");
    assert_eq!(first_report.cache_status, EntityIndexCacheStatus::Rebuilt);
    assert!(first_report.next_command.contains("canon entity block"));

    let second = run_index_build_v1(fixture.index_request(None)).expect("index cache hit succeeds");
    assert_eq!(second.cache_status, EntityIndexCacheStatus::Hit);
    assert_eq!(second.artifact, first.artifact);
    assert_eq!(
        fs::read(&second.paths.artifact_path).expect("index artifact bytes"),
        first_bytes,
        "verified cache hit keeps the artifact byte-identical"
    );

    let second_report = index_build_v1_report(&second);
    assert_eq!(second_report.version, "canon_entity_index_build.v1");
    assert_eq!(
        second_report.artifact["version"],
        CANON_ENTITY_INDEX_VERSION_V1
    );
    assert_eq!(second_report.cache_status, EntityIndexCacheStatus::Hit);
    assert!(second_report.next_command.contains("canon entity block"));
}

#[test]
fn entity_index_build_api_refuses_wrong_profile_before_writes() {
    let fixture = IndexCliFixture::new();
    fixture.prepare();

    let mut request = fixture.index_request(None);
    request.profile = "regab_firm_identity";
    let refusal = run_index_build_v1(request).expect_err("wrong profile refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityProfile);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!fixture.index_artifact_path().exists());
}

#[test]
fn entity_index_build_api_refuses_tampered_prepare_before_writes() {
    let fixture = IndexCliFixture::new();
    fixture.prepare();

    let prepare_path = fixture.work_dir().join("prepare/prepare.json");
    let mut prepare: Value =
        serde_json::from_slice(&fs::read(&prepare_path).expect("prepare artifact"))
            .expect("prepare json");
    prepare["metadata"]["input"]["content_hash"] = Value::String("blake3:tampered".to_string());
    fs::write(
        &prepare_path,
        serde_json::to_vec_pretty(&prepare).expect("tampered prepare json"),
    )
    .expect("write tampered prepare");

    let refusal =
        run_index_build_v1(fixture.index_request(None)).expect_err("tampered prepare refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["field"], "artifact_content_hash");
    assert!(
        refusal
            .detail
            .get("expected")
            .and_then(Value::as_str)
            .is_some_and(|hash| hash.starts_with("blake3:"))
    );
    assert!(
        refusal
            .detail
            .get("actual")
            .and_then(Value::as_str)
            .is_some_and(|hash| hash.starts_with("blake3:"))
    );
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!fixture.index_artifact_path().exists());
}

#[test]
fn entity_index_build_api_refuses_over_budget_before_writes() {
    let fixture = IndexCliFixture::new();
    fixture.prepare();

    let refusal =
        run_index_build_v1(fixture.index_request(Some(1))).expect_err("small byte budget refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityIoBudget);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!fixture.index_artifact_path().exists());
}

#[test]
fn entity_index_build_cli_executes_after_dispatch_wiring() {
    let fixture = IndexCliFixture::new();
    fixture.prepare();

    let first = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "index",
            "build",
            fixture.rows().to_str().expect("rows path"),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            fixture.strategy().to_str().expect("strategy path"),
            "--registry",
            fixture.registry().to_str().expect("registry path"),
            "--work-dir",
            fixture.work_dir().to_str().expect("work dir path"),
        ])
        .output()
        .expect("run canon entity index build");

    assert!(
        first.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_report: Value =
        serde_json::from_slice(&first.stdout).expect("first index build report");
    assert_eq!(first_report["version"], "canon_entity_index_build.v1");
    assert_eq!(
        first_report["artifact"]["version"],
        CANON_ENTITY_INDEX_VERSION_V1
    );
    assert_eq!(first_report["cache_status"], "rebuilt");
    assert_eq!(
        validate_entity_v1_self_hash(&first_report["artifact"]).expect("first cli self hash"),
        first_report["artifact"]["artifact_content_hash"]
            .as_str()
            .expect("hash")
    );
    let index_artifact_path = fixture.index_artifact_path();
    assert_eq!(
        first_report["paths"]["artifact"]
            .as_str()
            .expect("artifact path"),
        index_artifact_path.display().to_string()
    );
    let first_bytes = fs::read(&index_artifact_path).expect("first index artifact bytes");

    let second = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "index",
            "build",
            fixture.rows().to_str().expect("rows path"),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            fixture.strategy().to_str().expect("strategy path"),
            "--registry",
            fixture.registry().to_str().expect("registry path"),
            "--work-dir",
            fixture.work_dir().to_str().expect("work dir path"),
        ])
        .output()
        .expect("rerun canon entity index build");
    assert!(
        second.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_report: Value =
        serde_json::from_slice(&second.stdout).expect("second index build report");
    assert_eq!(second_report["version"], "canon_entity_index_build.v1");
    assert_eq!(second_report["artifact"], first_report["artifact"]);
    assert_eq!(second_report["cache_status"], "hit");
    assert_eq!(
        second_report["paths"]["artifact"]
            .as_str()
            .expect("artifact path"),
        index_artifact_path.display().to_string()
    );
    assert_eq!(
        fs::read(index_artifact_path).expect("second index artifact bytes"),
        first_bytes,
        "public verified warm hit must preserve index artifact bytes"
    );
}

struct IndexCliFixture {
    _temp: tempfile::TempDir,
    rows: PathBuf,
    registry: PathBuf,
    strategy: PathBuf,
    work_dir: PathBuf,
}

impl IndexCliFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let strategy = temp.path().join("strategy.yaml");
        fs::write(
            &strategy,
            "strategy_id: cmbs_tenant_label\nstrategy_version: 1.0.0\n",
        )
        .expect("write strategy");
        let work_dir = temp.path().join("work");

        Self {
            _temp: temp,
            rows: root.join("tests/fixtures/entity/prepare/en_p001_rows.csv"),
            registry: root.join("tests/fixtures/registries/empty"),
            strategy,
            work_dir,
        }
    }

    fn prepare(&self) {
        run_prepare_v1(PrepareRunRequest {
            rows: &self.rows,
            profile: "cmbs_tenant_label",
            registry: &self.registry,
            work_dir: &self.work_dir,
        })
        .expect("prepare succeeds");
    }

    fn index_request(&self, max_artifact_bytes: Option<u64>) -> EntityIndexBuildRequest<'_> {
        EntityIndexBuildRequest {
            rows: &self.rows,
            profile: "cmbs_tenant_label",
            strategy: &self.strategy,
            registry: &self.registry,
            work_dir: &self.work_dir,
            max_artifact_bytes,
        }
    }

    fn rows(&self) -> &PathBuf {
        &self.rows
    }

    fn registry(&self) -> &PathBuf {
        &self.registry
    }

    fn strategy(&self) -> &PathBuf {
        &self.strategy
    }

    fn work_dir(&self) -> &PathBuf {
        &self.work_dir
    }

    fn index_artifact_path(&self) -> PathBuf {
        self.work_dir.join("index").join("index.json")
    }
}
