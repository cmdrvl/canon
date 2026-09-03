#![forbid(unsafe_code)]

use canon::{
    InputFormat, InputValues, RefusalCode, SpecialReason,
    entity::{
        CANON_ENTITY_REVIEW_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
        audit::{EntityAuditV1Request, run_entity_audit_v1},
        contracts::entity_artifact_v1_contract_for_version,
        review_import::{ReviewImportV1Request, import_review_v1},
        schema::{entity_v1_schema_content_hash, finalize_entity_v1_self_hash},
        solve::CANON_ENTITY_ALIAS_PROPOSAL_VERSION,
    },
    identity_scope::{
        CoreScopeDimension, IdentityScope, ScopeBinding, ScopeDimensionBinding, ScopeDimensionRef,
    },
    lookup::{ExactLookupContext, resolve_values_with_context},
    registry::load_registry,
    witness,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};
use tempfile::TempDir;

#[test]
fn reviewed_scoped_alias_requires_evidence_before_registry_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = PromotionFixture::new("2026.09.01")?;
    let registry_before = registry_snapshot(&fixture.registry);
    let run = run_artifact(fixture.work.path(), &fixture.registry_hash);
    let run_hash = artifact_hash(&run);
    let audit = audit_artifact(fixture.work.path(), run);
    let review = review_artifact(
        fixture.work.path(),
        &fixture.registry_hash,
        &run_hash,
        vec![
            review_item(
                "review:deal-a",
                "surface:deal-a",
                "deal:CGCMT-2016-P6",
                None,
            ),
            review_item(
                "review:deal-b",
                "surface:deal-b",
                "deal:WFCM-2017-C39",
                None,
            ),
        ],
    );
    let review_bytes = serde_json::to_vec_pretty(&review)?;
    let audit_bytes = serde_json::to_vec_pretty(&audit)?;

    let refusal = import_review_v1(ReviewImportV1Request {
        review_path: &fixture.work.path().join("review.json"),
        review_bytes: &review_bytes,
        registry: &fixture.registry,
        next_version: "2026.09.02",
        audit: Some((&audit, &audit_bytes)),
    })
    .expect_err("scoped alias promotion without evidence_ref refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "alias_proposal.evidence_ref");
    assert_eq!(refusal.detail["reason"], "cross_scope_evidence_required");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(registry_snapshot(&fixture.registry), registry_before);
    Ok(())
}

#[test]
fn reviewed_scoped_alias_with_evidence_promotes_and_replays_in_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = PromotionFixture::new("2026.09.01")?;
    let run = run_artifact(fixture.work.path(), &fixture.registry_hash);
    let run_hash = artifact_hash(&run);
    let audit = audit_artifact(fixture.work.path(), run);
    let evidence_ref = artifact_hash(&audit);
    let review = review_artifact(
        fixture.work.path(),
        &fixture.registry_hash,
        &run_hash,
        vec![review_item(
            "review:deal-a",
            "surface:deal-a",
            "deal:CGCMT-2016-P6",
            Some(&evidence_ref),
        )],
    );
    let review_bytes = serde_json::to_vec_pretty(&review)?;
    let audit_bytes = serde_json::to_vec_pretty(&audit)?;

    let receipt = import_review_v1(ReviewImportV1Request {
        review_path: &fixture.work.path().join("review.json"),
        review_bytes: &review_bytes,
        registry: &fixture.registry,
        next_version: "2026.09.02",
        audit: Some((&audit, &audit_bytes)),
    })
    .expect("evidence-backed scoped alias import succeeds");

    assert_eq!(receipt["summary"]["counts"]["accepted_aliases"], 1);
    assert_eq!(
        receipt["decisions"][0]["alias_proposal"]["evidence_ref"],
        evidence_ref
    );

    let aliases: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture.registry.join("aliases.json"))?)?;
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0]["input"], "41-001");
    assert_eq!(aliases[0]["canonical_id"], "PROP-BENTONVILLE-AR");
    assert_eq!(aliases[0]["canonical_type"], "cmbs_property");
    assert_eq!(aliases[0]["provenance"]["evidence_ref"], evidence_ref);
    assert_eq!(aliases[0]["scope"], deal_scope_value("deal:CGCMT-2016-P6"));

    let registry = load_registry(&fixture.registry)?;
    let matching = resolve_values_with_context(
        &registry,
        &input_values(&["41-001"]),
        &ExactLookupContext {
            namespace: None,
            scope: Some(deal_scope("deal:CGCMT-2016-P6")),
        },
    )?;
    assert_eq!(matching.mappings.len(), 1);
    assert_eq!(matching.mappings[0].canonical_id, "PROP-BENTONVILLE-AR");

    let wrong_deal = resolve_values_with_context(
        &registry,
        &input_values(&["41-001"]),
        &ExactLookupContext {
            namespace: None,
            scope: Some(deal_scope("deal:WFCM-2017-C39")),
        },
    )?;
    assert!(wrong_deal.mappings.is_empty());
    assert_eq!(wrong_deal.unresolved.len(), 1);
    Ok(())
}

#[test]
fn unscoped_review_import_keeps_legacy_alias_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = PromotionFixture::new("2026.09.01")?;
    let run = run_artifact(fixture.work.path(), &fixture.registry_hash);
    let run_hash = artifact_hash(&run);
    let audit = audit_artifact(fixture.work.path(), run);
    let review = review_artifact(
        fixture.work.path(),
        &fixture.registry_hash,
        &run_hash,
        vec![unscoped_review_item("review:legacy", "surface:legacy")],
    );
    let review_bytes = serde_json::to_vec_pretty(&review)?;
    let audit_bytes = serde_json::to_vec_pretty(&audit)?;

    import_review_v1(ReviewImportV1Request {
        review_path: &fixture.work.path().join("review.json"),
        review_bytes: &review_bytes,
        registry: &fixture.registry,
        next_version: "2026.09.02",
        audit: Some((&audit, &audit_bytes)),
    })
    .expect("unscoped alias import succeeds");

    let expected = legacy_alias_bytes()?;
    assert_eq!(fs::read(fixture.registry.join("aliases.json"))?, expected);
    Ok(())
}

struct PromotionFixture {
    work: TempDir,
    registry: std::path::PathBuf,
    registry_hash: String,
}

impl PromotionFixture {
    fn new(version: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let work = tempfile::tempdir()?;
        let registry = work.path().join("registry");
        fs::create_dir_all(&registry)?;
        fs::write(
            registry.join("registry.json"),
            format!(
                r#"{{
  "id": "cmbs-properties",
  "version": "{version}",
  "description": "identity scope promotion fixture",
  "updated": "2026-09-03",
  "entry_count": 0
}}"#
            ),
        )?;
        fs::write(registry.join("aliases.json"), b"[]\n")?;
        let registry_hash = registry_snapshot_hash(&registry)?;
        Ok(Self {
            work,
            registry,
            registry_hash,
        })
    }
}

fn run_artifact(work: &Path, registry_hash: &str) -> Value {
    let contract = entity_artifact_v1_contract_for_version(CANON_ENTITY_RUN_VERSION_V1)
        .expect("run v1 contract");
    let mut artifact = json!({
        "version": CANON_ENTITY_RUN_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata(work, contract, registry_hash),
        "summary": {
            "counts": {
                "rows": 2
            },
            "labels": {
                "stage": "run"
            }
        },
        "run_manifest_path": "run/manifest.json"
    });
    finalize_entity_v1_self_hash(&mut artifact).expect("run self hash finalizes");
    artifact
}

fn audit_artifact(work: &Path, run: Value) -> Value {
    let suite = work.join("audit-suite");
    fs::create_dir_all(&suite).expect("audit suite dir");
    run_entity_audit_v1(EntityAuditV1Request {
        result_artifact: run,
        suite_dir: &suite,
    })
    .expect("audit v1 succeeds")
}

fn review_artifact(
    work: &Path,
    registry_hash: &str,
    source_hash: &str,
    review_items: Vec<Value>,
) -> Value {
    let contract = entity_artifact_v1_contract_for_version(CANON_ENTITY_REVIEW_VERSION_V1)
        .expect("review v1 contract");
    let mut artifact = json!({
        "version": CANON_ENTITY_REVIEW_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata(work, contract, registry_hash),
        "summary": {
            "counts": {
                "review_items": review_items.len() as u64,
                "review_group_count": review_items.len() as u64,
                "review_rows_covered": review_items.len() as u64
            },
            "labels": {
                "stage": "review",
                "include": "all"
            }
        },
        "review_queue_path": "review/queue.jsonl",
        "source_result": {
            "version": CANON_ENTITY_RUN_VERSION_V1,
            "content_hash": source_hash
        },
        "include": "all",
        "review_items": review_items,
        "next_commands": {
            "audit": "canon entity audit <RESULT.json> --suite <SUITE_DIR>",
            "review_import": "canon entity review import <REVIEW.json|csv> --registry <REGISTRY> --next-version <VER>",
            "promote": "canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <REGISTRY> --next-version <VER>"
        }
    });
    finalize_entity_v1_self_hash(&mut artifact).expect("review self hash finalizes");
    artifact
}

fn metadata(
    work: &Path,
    contract: &canon::entity::contracts::EntityArtifactContractDescriptor,
    registry_hash: &str,
) -> Value {
    json!({
        "profile": {
            "id": "cmbs_property",
            "version": "0.1.0",
            "entity_type": "property",
            "identity_semantics": "global_physical_property",
            "canonical_type": "cmbs_property",
            "patch_namespaces": {
                "aliases": "cmbs_property.aliases",
                "distinct": "cmbs_property.distinct",
                "relations": "cmbs_property.relations"
            },
            "content_hash": "blake3:profile"
        },
        "strategy": {
            "id": "cmbs_property.v1",
            "version": "0.1.0",
            "content_hash": "blake3:strategy"
        },
        "registry_snapshot": {
            "id": "cmbs-properties",
            "version": "2026.09.01",
            "source": "registry",
            "lookup_snapshot_hash": registry_hash
        },
        "input": {
            "row_count": 2,
            "content_hash": "blake3:input"
        },
        "patch_namespace": "cmbs_property.aliases",
        "schema": {
            "key": contract.schema_key,
            "content_hash": entity_v1_schema_content_hash(contract).expect("schema hash")
        },
        "workdir": {
            "root_dir": work.display().to_string(),
            "stage_dir": contract.stage_dir,
            "artifact_relpath": contract.artifact_relpath,
            "payload_relpath": contract.payload_relpath
        },
        "upstream_artifacts": [],
        "patch_set": {
            "content_hash": "blake3:patch",
            "paths": []
        },
        "namekit": {
            "version": "namekit.v0",
            "content_hash": "blake3:namekit"
        },
        "artifact_content_hash": ""
    })
}

fn review_item(review_id: &str, surface_id: &str, deal: &str, evidence_ref: Option<&str>) -> Value {
    let mut proposal = alias_proposal("41-001", "PROP-BENTONVILLE-AR", surface_id);
    proposal["scope"] = deal_scope_value(deal);
    if let Some(evidence_ref) = evidence_ref {
        proposal["evidence_ref"] = Value::String(evidence_ref.to_string());
    }
    resign_alias_proposal(&mut proposal);
    json!({
        "review_id": review_id,
        "state": "resolved_existing",
        "surface_ids": [surface_id],
        "decision": "accept_alias",
        "operator_id": "operator-1",
        "reason_code": "reviewed_cross_scope_property_evidence",
        "alias_proposal": proposal
    })
}

fn unscoped_review_item(review_id: &str, surface_id: &str) -> Value {
    let proposal = alias_proposal(
        "Bentonville Self Storage",
        "PROP-BENTONVILLE-AR",
        surface_id,
    );
    json!({
        "review_id": review_id,
        "state": "resolved_existing",
        "surface_ids": [surface_id],
        "decision": "accept_alias",
        "operator_id": "operator-1",
        "reason_code": "legacy_unscoped_alias",
        "alias_proposal": proposal
    })
}

fn alias_proposal(input: &str, canonical_id: &str, surface_id: &str) -> Value {
    let mut proposal = json!({
        "version": CANON_ENTITY_ALIAS_PROPOSAL_VERSION,
        "proposal_id": "",
        "content_hash": "",
        "allowed_actions": ["accept_alias", "reject_alias"],
        "input": input,
        "canonical_id": canonical_id,
        "canonical_type": "cmbs_property",
        "rule_id": "ENTITY_REVIEW_IMPORT",
        "component_id": format!("component:{surface_id}"),
        "source_surface_ids": [surface_id]
    });
    resign_alias_proposal(&mut proposal);
    proposal
}

fn resign_alias_proposal(proposal: &mut Value) {
    let hash = alias_proposal_hash(proposal);
    proposal["proposal_id"] = Value::String(format!("alias_proposal:{hash}"));
    proposal["content_hash"] = Value::String(hash);
}

fn alias_proposal_hash(proposal: &Value) -> String {
    let mut hashable = json!({
        "version": proposal["version"],
        "input": proposal["input"],
        "canonical_id": proposal["canonical_id"],
        "canonical_type": proposal["canonical_type"],
        "rule_id": proposal["rule_id"],
        "component_id": proposal["component_id"],
        "source_surface_ids": proposal["source_surface_ids"],
        "allowed_actions": proposal["allowed_actions"]
    });
    if let Some(object) = hashable.as_object_mut() {
        for field in ["namespace", "scope", "evidence_ref"] {
            if let Some(value) = proposal.get(field) {
                object.insert(field.to_string(), value.clone());
            }
        }
    }
    witness::hash_bytes(&serde_json::to_vec(&hashable).expect("proposal hash bytes"))
}

fn deal_scope_value(deal: &str) -> Value {
    json!({
        "dimensions": [
            {
                "dimension": {
                    "kind": "core",
                    "dimension": "dataset"
                },
                "binding": {
                    "binding": "exact",
                    "value": deal
                }
            }
        ]
    })
}

fn deal_scope(deal: &str) -> IdentityScope {
    IdentityScope {
        dimensions: vec![ScopeDimensionBinding {
            dimension: ScopeDimensionRef::Core {
                dimension: CoreScopeDimension::Dataset,
            },
            binding: ScopeBinding::Exact {
                value: deal.to_string(),
            },
        }],
    }
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

fn legacy_alias_bytes() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = serde_json::to_vec_pretty(&json!([
        {
            "input": "Bentonville Self Storage",
            "canonical_id": "PROP-BENTONVILLE-AR",
            "canonical_type": "cmbs_property",
            "rule_id": "ENTITY_REVIEW_IMPORT"
        }
    ]))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn registry_snapshot(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    for name in ["registry.json", "aliases.json"] {
        snapshot.insert(
            name.to_string(),
            fs::read(path.join(name)).expect("registry file reads"),
        );
    }
    snapshot
}

fn registry_snapshot_hash(registry: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(registry)? {
        let path = entry?.path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            files.push(path);
        }
    }
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("registry file name utf-8");
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&fs::read(path)?);
        hasher.update(&[0]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn artifact_hash(artifact: &Value) -> String {
    artifact["artifact_content_hash"]
        .as_str()
        .expect("artifact hash")
        .to_string()
}
