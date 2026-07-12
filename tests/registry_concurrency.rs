#![forbid(unsafe_code)]

use canon::{
    InputFormat, InputValues, Refusal, RefusalCode, SpecialReason,
    entity::{
        CANON_ENTITY_DECISION_LEDGER_VERSION, CANON_ENTITY_SOLVE_VERSION, EntityArtifactHeader,
        EntityArtifactMetadata, EntityArtifactReference, EntityDeterministicSummary,
        EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
        EntityRegistrySnapshot, EntityStrategyReference,
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage,
        },
        audit::{
            EntityAuditArtifact, EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite,
            run_entity_audit,
        },
        promote::{
            EntityPromoteRegistryRequest, EntityPromotedAlias, EntityPromotionAuditExpectation,
            promote_registry_aliases,
        },
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
    },
    lookup::resolve_values,
    registry::{RegistryAddEntryOutput, RegistryAddEntryRequest, add_entry, load_registry, mint},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;

const MUTATION_LOCK_NAME: &str = ".canon-registry-mutation.lock";
const INDEX_LOCK_SUFFIX: &str = ".canon-index.lock";

fn write_registry_metadata(
    dir: &Path,
    version: &str,
    entry_count: usize,
) -> Result<(), Box<dyn Error>> {
    fs::write(
        dir.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "people",
            "version": version,
            "description": "registry concurrency fixture",
            "updated": "2026-07-10",
            "entry_count": entry_count,
            "owner": "test-suite"
        }))?,
    )?;
    Ok(())
}

fn write_aliases(dir: &Path, entries: Value) -> Result<(), Box<dyn Error>> {
    fs::write(
        dir.join("aliases.json"),
        serde_json::to_vec_pretty(&entries)?,
    )?;
    Ok(())
}

fn make_registry(version: &str, entries: Value) -> Result<TempDir, Box<dyn Error>> {
    let temp = TempDir::new()?;
    let entry_count = entries.as_array().ok_or("entries must be an array")?.len();
    write_registry_metadata(temp.path(), version, entry_count)?;
    write_aliases(temp.path(), entries)?;
    Ok(temp)
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}

fn add_entry_request(registry: &Path, canonical_id: &str, input: &str) -> RegistryAddEntryRequest {
    RegistryAddEntryRequest {
        registry: registry.to_path_buf(),
        alias_file: "aliases.json".to_string(),
        canonical_id: canonical_id.to_string(),
        input: input.to_string(),
        rule_id: "MANUAL".to_string(),
        canonical_type: Some("person".to_string()),
        bump: None,
        next_version: None,
        no_lint: true,
    }
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_lease(path: &Path, purpose: &str, created_unix_secs: u64) -> Result<(), Box<dyn Error>> {
    fs::write(
        path,
        serde_json::to_vec(&json!({
            "pid": std::process::id(),
            "created_unix_secs": created_unix_secs,
            "purpose": purpose,
        }))?,
    )?;
    Ok(())
}

fn release_after_plans(lock_path: PathBuf) {
    thread::sleep(Duration::from_millis(500));
    fs::remove_file(lock_path).expect("release precreated lock");
}

fn run_add_entries_together(
    requests: [RegistryAddEntryRequest; 2],
) -> Vec<Result<RegistryAddEntryOutput, Refusal>> {
    let barrier = Arc::new(Barrier::new(3));
    let handles = requests
        .into_iter()
        .map(|request| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                add_entry(request)
            })
        })
        .collect::<Vec<_>>();

    barrier.wait();
    handles
        .into_iter()
        .map(|handle| handle.join().expect("thread completes"))
        .collect::<Vec<_>>()
}

fn assert_stale_write_plan_refusal(refusal: &Refusal) {
    assert_eq!(refusal.code, RefusalCode::EBadRegistry);
    assert_eq!(refusal.detail["field"], "write_plan_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(
        refusal.detail["expected_hash"]
            .as_str()
            .expect("expected hash")
            .starts_with("blake3:")
    );
    assert!(
        refusal.detail["actual_hash"]
            .as_str()
            .expect("actual hash")
            .starts_with("blake3:")
    );
}

fn assert_identical_add_entry_refusal(refusal: &Refusal) {
    if refusal.code == RefusalCode::EBadRegistry && refusal.detail["field"] == "write_plan_hash" {
        assert_stale_write_plan_refusal(refusal);
        return;
    }

    assert_eq!(refusal.code, RefusalCode::EParse);
    assert_eq!(refusal.detail["input"], "Alpha");
    assert_eq!(refusal.detail["existing"]["canonical_id"], "PPL-001");
    assert_eq!(refusal.detail["existing"]["canonical_type"], "person");
    assert_eq!(refusal.detail["existing"]["rule_id"], "MANUAL");
}

fn registry_metadata(path: &Path) -> Value {
    read_json(&path.join("registry.json"))
}

fn alias_entries(path: &Path) -> Vec<Value> {
    read_json(&path.join("aliases.json"))
        .as_array()
        .expect("aliases array")
        .clone()
}

fn entries_by_input(entries: &[Value]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|entry| {
            (
                entry["input"].as_str().expect("entry input").to_string(),
                entry["canonical_id"]
                    .as_str()
                    .expect("entry canonical id")
                    .to_string(),
            )
        })
        .collect()
}

fn input_values(values: &[&str]) -> InputValues {
    let mut deduped = HashMap::new();
    for value in values {
        deduped.insert((*value).to_string(), ());
    }

    InputValues {
        values: deduped,
        special: HashMap::<SpecialReason, usize>::new(),
        format: InputFormat::Csv,
        delimiter: Some(b','),
        source_hash: None,
        source_bytes: None,
    }
}

fn passing_audit() -> EntityAuditArtifact {
    run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&solve_header()),
        ),
        certified_artifacts: certified_artifacts(),
        result: solve_header(),
        suite: passing_suite(),
    })
    .expect("audit passes")
}

fn solve_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        metadata: EntityArtifactMetadata {
            profile: EntityProfileReference {
                id: "cmbs_tenant_label".to_string(),
                version: "0.1.0".to_string(),
                entity_type: "tenant_label".to_string(),
                identity_semantics: "canonical_display_label".to_string(),
                canonical_type: "tenant_label".to_string(),
                patch_namespaces: EntityPatchNamespaces {
                    aliases: "cmbs_tenant_label.aliases".to_string(),
                    distinct: "cmbs_tenant_label.distinct".to_string(),
                    relations: "cmbs_tenant_label.relations".to_string(),
                },
                content_hash: Some("blake3:profile".to_string()),
            },
            strategy: EntityStrategyReference {
                id: "cmbs_tenant_label.v1".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:strategy".to_string(),
            },
            registry_snapshot: EntityRegistrySnapshot {
                id: "cmbs-tenants".to_string(),
                version: "2026.06.25".to_string(),
                source: "registries/cmbs-tenants".to_string(),
                lookup_snapshot_hash: "blake3:registry".to_string(),
                sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
            },
            patch_namespace: "cmbs_tenant_label.aliases".to_string(),
            input: Some(EntityInputReference {
                row_count: 1,
                content_hash: "blake3:input".to_string(),
            }),
            upstream_artifacts: vec![],
            patch_set: None,
            namekit: None,
            artifact_content_hash: "blake3:solve".to_string(),
        },
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("entity_count".to_string(), 1)]),
            labels: BTreeMap::new(),
        },
    }
}

fn certified_artifacts() -> Vec<EntityArtifactReference> {
    vec![
        EntityArtifactReference {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            content_hash: "blake3:solve".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
            content_hash: "blake3:review-queue".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_DECISION_LEDGER_VERSION.to_string(),
            content_hash: "blake3:decision-ledger".to_string(),
        },
    ]
}

fn passing_suite() -> EntityAuditSuite {
    EntityAuditSuite {
        id: "promotion_smoke".to_string(),
        version: "2026.06.26".to_string(),
        gates: vec![
            EntityAuditGateCheck {
                gate_id: "G01".to_string(),
                label: "artifact continuity".to_string(),
                passed: true,
                expected: "all_hashes_match".to_string(),
                actual: "all_hashes_match".to_string(),
                evidence: BTreeMap::new(),
            },
            EntityAuditGateCheck {
                gate_id: "G09".to_string(),
                label: "decision ledger continuity".to_string(),
                passed: true,
                expected: "continuous_jsonl".to_string(),
                actual: "continuous_jsonl".to_string(),
                evidence: BTreeMap::new(),
            },
            EntityAuditGateCheck {
                gate_id: "G14".to_string(),
                label: "promotion gate".to_string(),
                passed: true,
                expected: "audit_status=passed".to_string(),
                actual: "audit_status=passed".to_string(),
                evidence: BTreeMap::new(),
            },
        ],
    }
}

fn audit_expectation(audit: &EntityAuditArtifact) -> EntityPromotionAuditExpectation {
    EntityPromotionAuditExpectation {
        audit_artifact_hash: audit.artifact_content_hash.clone(),
        audited_artifact_hash: "blake3:solve".to_string(),
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        required_gate_ids: vec!["G14".to_string()],
    }
}

fn promoted_alias(input: &str, canonical_id: &str) -> EntityPromotedAlias {
    EntityPromotedAlias {
        input: input.to_string(),
        canonical_id: canonical_id.to_string(),
        canonical_type: "tenant_label".to_string(),
        rule_id: "ENTITY_REVIEW_PROMOTE".to_string(),
    }
}

fn make_entity_registry() -> Result<TempDir, Box<dyn Error>> {
    let temp = TempDir::new()?;
    fs::write(
        temp.path().join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "cmbs-tenants",
            "version": "1.0.0",
            "description": "entity promote concurrency fixture",
            "updated": "2026-07-10",
            "entry_count": 0,
            "owner": "test-suite"
        }))?,
    )?;
    fs::write(temp.path().join("aliases.json"), b"[]\n")?;
    Ok(temp)
}

#[test]
fn concurrent_load_registry_recovers_stale_builder_lease_and_cleans_it_up()
-> Result<(), Box<dyn Error>> {
    let registry = make_registry(
        "1.0.0",
        json!([
            {"input": "Jane Doe", "canonical_id": "PPL-001", "canonical_type": "person", "rule_id": "MANUAL"}
        ]),
    )?;

    let first = load_registry(registry.path())?;
    let db_path = first.db_path.clone();
    let lock_path = db_path.with_file_name(format!(
        "{}{}",
        db_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("db file name")?,
        INDEX_LOCK_SUFFIX
    ));

    let _ = fs::remove_file(&db_path);
    write_lease(&lock_path, "registry-index-builder", 0)?;

    let barrier = Arc::new(Barrier::new(3));
    let handles = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let registry_dir = registry.path().to_path_buf();
            thread::spawn(move || {
                barrier.wait();
                let registry = load_registry(&registry_dir).expect("registry loads");
                let resolved = resolve_values(&registry, &input_values(&["Jane Doe"]))
                    .expect("lookup succeeds");
                (registry.db_path, resolved.mappings[0].canonical_id.clone())
            })
        })
        .collect::<Vec<_>>();

    barrier.wait();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread completes"))
        .collect::<Vec<_>>();

    assert_eq!(results[0].0, results[1].0);
    assert_eq!(results[0].1, "PPL-001");
    assert_eq!(results[1].1, "PPL-001");
    assert!(results[0].0.exists());
    assert!(!lock_path.exists());

    let _ = fs::remove_file(&results[0].0);
    Ok(())
}

#[test]
fn concurrent_conflicting_add_entry_never_loses_committed_write() -> Result<(), Box<dyn Error>> {
    let registry = make_registry("1.0.0", json!([]))?;
    let results = run_add_entries_together([
        add_entry_request(registry.path(), "PPL-001", "Alpha"),
        add_entry_request(registry.path(), "PPL-002", "Beta"),
    ]);
    let successes = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect::<Vec<_>>();
    let refusals = results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect::<Vec<_>>();
    let entries = alias_entries(registry.path());
    let entries_by_input = entries_by_input(&entries);
    let registry_json = registry_metadata(registry.path());

    match (successes.len(), refusals.len()) {
        (1, 1) => {
            assert_stale_write_plan_refusal(refusals[0]);
            assert_eq!(entries.len(), 1);
            assert_eq!(registry_json["version"], "1.0.1");
            assert_eq!(registry_json["entry_count"], 1);
            assert_eq!(
                entries_by_input.get(&successes[0].alias_entry.input),
                Some(&successes[0].alias_entry.canonical_id)
            );
            match entries_by_input.iter().next().expect("one entry") {
                (input, canonical_id)
                    if (input.as_str(), canonical_id.as_str()) == ("Alpha", "PPL-001")
                        || (input.as_str(), canonical_id.as_str()) == ("Beta", "PPL-002") => {}
                other => panic!("unexpected surviving entry: {other:?}"),
            }
        }
        (2, 0) => {
            assert_eq!(
                entries.len(),
                2,
                "two successful concurrent add-entry calls lost an update"
            );
            assert_eq!(registry_json["version"], "1.0.2");
            assert_eq!(registry_json["entry_count"], 2);
            assert_eq!(entries_by_input.get("Alpha"), Some(&"PPL-001".to_string()));
            assert_eq!(entries_by_input.get("Beta"), Some(&"PPL-002".to_string()));
        }
        other => panic!("unexpected concurrent add-entry outcome: {other:?}"),
    }
    Ok(())
}

#[test]
fn concurrent_identical_add_entry_replays_without_duplicate_entries() -> Result<(), Box<dyn Error>>
{
    let registry = make_registry("1.0.0", json!([]))?;
    let results = run_add_entries_together([
        add_entry_request(registry.path(), "PPL-001", "Alpha"),
        add_entry_request(registry.path(), "PPL-001", "Alpha"),
    ]);
    let successes = results.iter().filter(|result| result.is_ok()).count();
    let refusals = results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect::<Vec<_>>();
    match (successes, refusals.len()) {
        (2, 0) => {}
        (1, 1) => assert_identical_add_entry_refusal(refusals[0]),
        other => panic!("unexpected concurrent identical add-entry outcome: {other:?}"),
    }

    let entries = alias_entries(registry.path());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["input"], "Alpha");
    assert_eq!(entries[0]["canonical_id"], "PPL-001");

    let registry_json = registry_metadata(registry.path());
    assert_eq!(registry_json["version"], "1.0.1");
    assert_eq!(registry_json["entry_count"], 1);
    Ok(())
}

#[test]
fn abandoned_add_entry_temp_artifacts_do_not_block_new_writes() -> Result<(), Box<dyn Error>> {
    let registry = make_registry("1.0.0", json!([]))?;
    let legacy_temp = registry.path().join("aliases.json.canon-add-entry.tmp");
    let user_note = registry.path().join("aliases.json.canon-add-entry.note");
    fs::write(&legacy_temp, b"stale temp artifact")?;
    fs::write(&user_note, b"user-authored note")?;

    add_entry(add_entry_request(registry.path(), "PPL-001", "Alpha"))
        .expect("add-entry succeeds despite abandoned temp artifacts");

    assert!(legacy_temp.exists());
    assert!(user_note.exists());
    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases.as_array().expect("aliases array").len(), 1);
    Ok(())
}

#[test]
fn concurrent_conflicting_promotions_report_stale_registry_snapshot() -> Result<(), Box<dyn Error>>
{
    let registry = make_entity_registry()?;
    let lock_path = registry.path().join(MUTATION_LOCK_NAME);
    write_lease(&lock_path, "registry-mutation", current_unix_secs())?;
    let audit = passing_audit();
    let expectation = audit_expectation(&audit);
    let barrier = Arc::new(Barrier::new(3));
    let requests = [
        EntityPromoteRegistryRequest {
            registry: registry.path().to_path_buf(),
            alias_file: "aliases.json".to_string(),
            next_version: "1.0.1".to_string(),
            audit: audit.clone(),
            audit_expectation: expectation.clone(),
            aliases: vec![promoted_alias("Sears", "TNT-SEARS")],
            no_lint: true,
        },
        EntityPromoteRegistryRequest {
            registry: registry.path().to_path_buf(),
            alias_file: "aliases.json".to_string(),
            next_version: "1.0.1".to_string(),
            audit,
            audit_expectation: expectation,
            aliases: vec![promoted_alias("Kmart", "TNT-KMART")],
            no_lint: true,
        },
    ];
    let handles = requests
        .into_iter()
        .map(|request| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                promote_registry_aliases(request)
            })
        })
        .collect::<Vec<_>>();

    barrier.wait();
    release_after_plans(lock_path);
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread completes"))
        .collect::<Vec<_>>();

    let successes = results.iter().filter(|result| result.is_ok()).count();
    let refusals = results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect::<Vec<_>>();

    assert_eq!(successes, 1);
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].code, RefusalCode::EEntityRegistrySnapshot);
    assert_eq!(refusals[0].detail["field"], "registry_snapshot_hash");
    assert_eq!(refusals[0].detail["writes_performed"], false);
    assert!(
        refusals[0].detail["expected_registry_snapshot_hash"]
            .as_str()
            .expect("expected snapshot hash")
            .starts_with("blake3:")
    );
    assert!(
        refusals[0].detail["actual_registry_snapshot_hash"]
            .as_str()
            .expect("actual snapshot hash")
            .starts_with("blake3:")
    );

    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases.as_array().expect("aliases array").len(), 1);
    assert_eq!(
        read_json(&registry.path().join("registry.json"))["version"],
        "1.0.1"
    );
    Ok(())
}

#[test]
fn registry_mint_still_round_trips_after_atomicity_changes() -> Result<(), Box<dyn Error>> {
    let registry = make_registry(
        "1.0.0",
        json!([
            {"input": "Jane Doe", "canonical_id": "PPL-001", "canonical_type": "person", "rule_id": "MANUAL"}
        ]),
    )?;
    fs::write(registry.path().join("nicknames.json"), b"[]\n")?;

    let output = mint(canon::registry::RegistryMintRequest {
        registry: registry.path().to_path_buf(),
        canonical_id: Some("PPL-002".to_string()),
        prefix: None,
        canonical_type: "person".to_string(),
        with_alias: vec!["nicknames.json=John Doe:MANUAL".to_string()],
        bump: None,
        next_version: None,
        no_lint: true,
    })
    .expect("mint succeeds");

    assert_eq!(output.canonical_id, "PPL-002");
    assert_eq!(
        read_json(&registry.path().join("registry.json"))["version"],
        "1.0.1"
    );
    let nicknames = read_json(&registry.path().join("nicknames.json"));
    assert_eq!(nicknames.as_array().expect("nicknames array").len(), 1);
    Ok(())
}
