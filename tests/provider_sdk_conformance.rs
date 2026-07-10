#![forbid(unsafe_code)]

#[path = "../src/provider_sdk.rs"]
mod provider_sdk;

use provider_sdk::{
    CheckpointUnit, DeclaredSourceFile, DuplicateFactPolicy, FrozenSourceBundle, FrozenSourceFile,
    FrozenSourceManifest, FrozenSourceProvider, ProviderBuildLimits, ProviderBuildPolicies,
    ProviderCapability, ProviderCheckpoint, ProviderDiagnostic, ProviderDiagnosticSeverity,
    ProviderFactRecord, ProviderLicenseContract, ProviderManifest, ProviderMappingContract,
    ProviderMaterializationDraft, ProviderMaterializationPackage, ProviderParserContract,
    ProviderQuarantineRow, ProviderSdkErrorCode, ProviderSdkResult, SourceFormat,
    SourceRecordLocator, UndeclaredFilePolicy, provider_manifest_schema_version,
    run_provider_conformance, semantic_diff,
};
use serde_json::Value;
use std::collections::BTreeMap;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.provider.manifest.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/provider_sdk.rs");

#[test]
fn schema_declares_frozen_source_offline_materializer_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], provider_manifest_schema_version());
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        provider_manifest_schema_version()
    );
    assert_eq!(schema["x-canon-contract"]["offline_build_only"], true);
    assert_eq!(schema["x-canon-contract"]["no_undeclared_file_reads"], true);
    assert_eq!(schema["x-canon-contract"]["semantic_diff_required"], true);
}

#[test]
fn external_fixture_provider_passes_conformance_without_core_changes() {
    let provider = FixtureProvider;
    let bundle = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nr2|fact.synthetic|beta|amber|ok\n",
        "rev-a",
    );

    let package = run_provider_conformance(&provider, &bundle, None).expect("provider conforms");
    assert_eq!(package.provider_id, "pkg.synthetic.provider");
    assert_eq!(package.facts.len(), 2);
    assert!(package.quarantined_rows.is_empty());
    assert_eq!(package.facts[0].locator.source_path, "records.pipe");
}

#[test]
fn malformed_records_and_schema_drift_become_quarantine_and_diagnostics() {
    let provider = FixtureProvider;
    let malformed = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nbroken|row\n",
        "rev-b",
    );
    let malformed_package =
        run_provider_conformance(&provider, &malformed, None).expect("malformed build completes");
    assert_eq!(malformed_package.facts.len(), 1);
    assert_eq!(malformed_package.quarantined_rows.len(), 1);
    assert_eq!(
        malformed_package.quarantined_rows[0].reason_code,
        "malformed_record"
    );

    let drifted = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|mode\nr1|fact.synthetic|alpha|north|ok\n",
        "rev-c",
    );
    let drifted_package =
        run_provider_conformance(&provider, &drifted, None).expect("schema drift build completes");
    assert!(drifted_package.facts.is_empty());
    assert_eq!(drifted_package.diagnostics.len(), 1);
    assert_eq!(drifted_package.diagnostics[0].code, "schema_drift");
}

#[test]
fn duplicate_facts_are_quarantined_deterministically() {
    let provider = FixtureProvider;
    let bundle = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nr2|fact.synthetic|alpha|north|ok\n",
        "rev-d",
    );
    let package =
        run_provider_conformance(&provider, &bundle, None).expect("duplicate build completes");
    assert_eq!(package.facts.len(), 1);
    assert_eq!(package.quarantined_rows.len(), 1);
    assert_eq!(
        package.quarantined_rows[0].reason_code,
        "duplicate_fact_key"
    );
    assert_eq!(
        package.quarantined_rows[0].quarantine_key,
        "duplicate:fact.synthetic:alpha:north"
    );
}

#[test]
fn interruption_and_resume_use_checkpointed_record_ordinals() {
    let provider = FixtureProvider;
    let bundle = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nr2|fact.synthetic|beta|amber|ok\nr3|fact.synthetic|gamma|clear|ok\n",
        "rev-e",
    );
    let checkpoint = ProviderCheckpoint {
        source_path: "records.pipe".to_string(),
        source_digest: bundle.manifest.files[0].content_digest.clone(),
        next_record_ordinal: 3,
        emitted_facts: 1,
        quarantined_rows: 0,
    };

    let resumed = run_provider_conformance(&provider, &bundle, Some(&checkpoint))
        .expect("resumed build conforms");
    assert_eq!(resumed.facts.len(), 2);
    assert_eq!(resumed.facts[0].fact_key, "fact.synthetic:beta:amber");
    assert_eq!(
        resumed
            .checkpoint
            .as_ref()
            .expect("checkpoint")
            .next_record_ordinal,
        5
    );
}

#[test]
fn resource_limits_and_undeclared_file_reads_are_rejected() {
    let provider = FixtureProvider;
    let oversized = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nr2|fact.synthetic|beta|amber|ok\nr3|fact.synthetic|gamma|clear|ok\nr4|fact.synthetic|delta|green|ok\n",
        "rev-f",
    );
    let error =
        run_provider_conformance(&provider, &oversized, None).expect_err("row limit should fail");
    assert_eq!(error.code, ProviderSdkErrorCode::ResourceLimitExceeded);

    let undeclared_provider = UndeclaredFileProvider;
    let bundle = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\n",
        "rev-g",
    );
    let error = run_provider_conformance(&undeclared_provider, &bundle, None)
        .expect_err("undeclared file usage should fail");
    assert_eq!(error.code, ProviderSdkErrorCode::UndeclaredFile);
}

#[test]
fn offline_network_attempts_are_rejected() {
    let bundle = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\n",
        "rev-h",
    );
    let error = run_provider_conformance(&NetworkAttemptProvider, &bundle, None)
        .expect_err("offline policy should fail");
    assert_eq!(error.code, ProviderSdkErrorCode::OfflinePolicy);
}

#[test]
fn package_determinism_and_source_revision_semantic_diff_are_stable() {
    let provider = FixtureProvider;
    let left_bundle = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nr2|fact.synthetic|beta|amber|ok\n",
        "rev-left",
    );
    let right_bundle_same = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nr2|fact.synthetic|beta|amber|ok\n",
        "rev-left",
    );
    let right_bundle_changed = fixture_bundle(
        "records.pipe",
        "record_id|fact_type|subject|value|status\nr1|fact.synthetic|alpha|north|ok\nr2|fact.synthetic|beta|amber|quarantine\nr3|fact.synthetic|gamma|clear|ok\n",
        "rev-right",
    );

    let left = run_provider_conformance(&provider, &left_bundle, None).expect("left package");
    let same = run_provider_conformance(&provider, &right_bundle_same, None).expect("same package");
    let changed =
        run_provider_conformance(&provider, &right_bundle_changed, None).expect("changed package");

    assert_eq!(canonical_json(&left), canonical_json(&same));

    let diff = semantic_diff(&left, &changed).expect("semantic diff");
    assert!(diff.source_manifest_digest_changed);
    assert_eq!(diff.added_fact_keys, vec!["fact.synthetic:gamma:clear"]);
    assert_eq!(diff.added_quarantine_keys, vec!["row:r2:quarantine_status"]);
}

#[test]
fn source_scan_keeps_real_provider_clients_and_unsafe_execution_out_of_sdk_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["openfigi", "sec", "vendor", "agency"] {
        assert!(
            !contains_forbidden_word(&lower_source, banned),
            "provider sdk should not embed concrete provider client term {banned}"
        );
        assert!(
            !contains_forbidden_word(&lower_schema, banned),
            "provider schema should not embed concrete provider client term {banned}"
        );
    }
    for banned in ["std::process::command", "command::new", "spawn(", "reqwest"] {
        assert!(
            !lower_source.contains(banned),
            "provider sdk should not execute or call live network via {banned}"
        );
    }
}

struct FixtureProvider;

impl FrozenSourceProvider for FixtureProvider {
    fn manifest(&self) -> ProviderManifest {
        manifest()
    }

    fn materialize(
        &self,
        bundle: &FrozenSourceBundle,
        checkpoint: Option<&ProviderCheckpoint>,
    ) -> ProviderSdkResult<ProviderMaterializationDraft> {
        let file = &bundle.files[0];
        let source_digest = bundle.manifest.files[0].content_digest.clone();
        let text = std::str::from_utf8(&file.content).expect("fixture source is UTF-8");
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default();

        let mut diagnostics = Vec::new();
        if header != "record_id|fact_type|subject|value|status" {
            diagnostics.push(ProviderDiagnostic {
                severity: ProviderDiagnosticSeverity::Error,
                code: "schema_drift".to_string(),
                message: format!("unexpected header {header}"),
                source_path: Some(file.path.clone()),
                locator: Some(SourceRecordLocator {
                    source_path: file.path.clone(),
                    record_ordinal: 1,
                    line_number: 1,
                    field_path: Some("header".to_string()),
                }),
            });
            return Ok(ProviderMaterializationDraft {
                used_source_paths: vec![file.path.clone()],
                attempted_network_access: false,
                facts: Vec::new(),
                quarantined_rows: Vec::new(),
                diagnostics,
                checkpoint: checkpoint.cloned(),
            });
        }

        let start_record = checkpoint
            .map(|checkpoint| checkpoint.next_record_ordinal)
            .unwrap_or(2);
        let mut facts = Vec::new();
        let mut quarantined_rows = Vec::new();
        let mut last_record_ordinal = start_record.saturating_sub(1);

        for (index, row) in lines.enumerate() {
            let record_ordinal = u64::try_from(index + 2).expect("record ordinal fits u64");
            if record_ordinal < start_record {
                continue;
            }
            last_record_ordinal = record_ordinal;
            let locator = SourceRecordLocator {
                source_path: file.path.clone(),
                record_ordinal,
                line_number: record_ordinal,
                field_path: None,
            };
            let parts = row.split('|').collect::<Vec<_>>();
            if parts.len() != 5 {
                quarantined_rows.push(ProviderQuarantineRow {
                    quarantine_key: format!("row:{record_ordinal}:malformed"),
                    reason_code: "malformed_record".to_string(),
                    raw_record_digest: digest_string(row),
                    source_digest: source_digest.clone(),
                    locator,
                    message: "row must contain five pipe-delimited fields".to_string(),
                });
                continue;
            }

            let [record_id, fact_type, subject, value, status] =
                <[_; 5]>::try_from(parts).expect("parts length checked");
            if status == "quarantine" {
                quarantined_rows.push(ProviderQuarantineRow {
                    quarantine_key: format!("row:{record_id}:quarantine_status"),
                    reason_code: "quarantine_status".to_string(),
                    raw_record_digest: digest_string(row),
                    source_digest: source_digest.clone(),
                    locator,
                    message: "row requested quarantine".to_string(),
                });
                continue;
            }
            if status != "ok" {
                quarantined_rows.push(ProviderQuarantineRow {
                    quarantine_key: format!("row:{record_id}:unknown_status"),
                    reason_code: "unknown_status".to_string(),
                    raw_record_digest: digest_string(row),
                    source_digest: source_digest.clone(),
                    locator,
                    message: format!("unknown status {status}"),
                });
                continue;
            }

            facts.push(ProviderFactRecord {
                fact_key: format!("{fact_type}:{subject}:{value}"),
                fact_schema: "fact.synthetic.v1".to_string(),
                fields: BTreeMap::from([
                    ("fact_type".to_string(), fact_type.to_string()),
                    ("subject".to_string(), subject.to_string()),
                    ("value".to_string(), value.to_string()),
                ]),
                source_digest: source_digest.clone(),
                locator,
            });
        }

        let checkpoint = Some(ProviderCheckpoint {
            source_path: file.path.clone(),
            source_digest,
            next_record_ordinal: last_record_ordinal.saturating_add(1),
            emitted_facts: facts.len(),
            quarantined_rows: quarantined_rows.len(),
        });

        Ok(ProviderMaterializationDraft {
            used_source_paths: vec![file.path.clone()],
            attempted_network_access: false,
            facts,
            quarantined_rows,
            diagnostics,
            checkpoint,
        })
    }
}

struct UndeclaredFileProvider;

impl FrozenSourceProvider for UndeclaredFileProvider {
    fn manifest(&self) -> ProviderManifest {
        manifest()
    }

    fn materialize(
        &self,
        bundle: &FrozenSourceBundle,
        _checkpoint: Option<&ProviderCheckpoint>,
    ) -> ProviderSdkResult<ProviderMaterializationDraft> {
        let file = &bundle.files[0];
        Ok(ProviderMaterializationDraft {
            used_source_paths: vec![file.path.clone(), "extra.pipe".to_string()],
            attempted_network_access: false,
            facts: Vec::new(),
            quarantined_rows: Vec::new(),
            diagnostics: Vec::new(),
            checkpoint: None,
        })
    }
}

struct NetworkAttemptProvider;

impl FrozenSourceProvider for NetworkAttemptProvider {
    fn manifest(&self) -> ProviderManifest {
        manifest()
    }

    fn materialize(
        &self,
        _bundle: &FrozenSourceBundle,
        _checkpoint: Option<&ProviderCheckpoint>,
    ) -> ProviderSdkResult<ProviderMaterializationDraft> {
        Ok(ProviderMaterializationDraft {
            used_source_paths: vec!["records.pipe".to_string()],
            attempted_network_access: true,
            facts: Vec::new(),
            quarantined_rows: Vec::new(),
            diagnostics: Vec::new(),
            checkpoint: None,
        })
    }
}

fn manifest() -> ProviderManifest {
    ProviderManifest {
        schema_version: String::new(),
        provider_id: "pkg.synthetic.provider".to_string(),
        provider_version: "1.2.3".to_string(),
        source_manifest_version: "frozen_source_manifest.v1".to_string(),
        capabilities: vec![
            ProviderCapability::FrozenSourceOnly,
            ProviderCapability::StreamingParser,
            ProviderCapability::CheckpointResume,
            ProviderCapability::QuarantineRows,
            ProviderCapability::SemanticDiff,
        ],
        parser: ProviderParserContract {
            source_format: SourceFormat::DelimitedUtf8,
            streaming: true,
            checkpoint_unit: CheckpointUnit::RecordOrdinal,
            required_fields: vec![
                "record_id".to_string(),
                "fact_type".to_string(),
                "subject".to_string(),
                "value".to_string(),
                "status".to_string(),
            ],
        },
        mapping: ProviderMappingContract {
            fact_schema: "fact.synthetic.v1".to_string(),
            fact_key_description: "fact_type + subject + value".to_string(),
            provenance_locator_kind: "line_number_record_ordinal".to_string(),
            quarantine_reason_codes: vec![
                "malformed_record".to_string(),
                "quarantine_status".to_string(),
                "unknown_status".to_string(),
                "duplicate_fact_key".to_string(),
            ],
        },
        policies: ProviderBuildPolicies {
            acquisition_separate_from_build: true,
            offline_build_only: true,
            undeclared_file_policy: UndeclaredFilePolicy::Reject,
            duplicate_fact_policy: DuplicateFactPolicy::QuarantineLaterDuplicates,
        },
        limits: ProviderBuildLimits {
            max_input_bytes: 1024,
            max_rows: 3,
            max_facts: 16,
            max_quarantine_rows: 16,
            max_diagnostics: 16,
        },
        licenses: ProviderLicenseContract {
            source_license_expression: "CC0-1.0".to_string(),
            output_license_expression: "MIT".to_string(),
            attribution_required: false,
        },
        semantic_diff_dimensions: vec![
            provider_sdk::SemanticDiffDimension::Facts,
            provider_sdk::SemanticDiffDimension::Quarantine,
            provider_sdk::SemanticDiffDimension::Diagnostics,
            provider_sdk::SemanticDiffDimension::SourceRevision,
        ],
    }
}

fn fixture_bundle(path: &str, content: &str, source_revision: &str) -> FrozenSourceBundle {
    FrozenSourceBundle {
        manifest: FrozenSourceManifest {
            manifest_version: "frozen_source_manifest.v1".to_string(),
            source_id: "synthetic_fixture_source".to_string(),
            source_version: "2026.07".to_string(),
            source_revision: source_revision.to_string(),
            files: vec![DeclaredSourceFile {
                path: path.to_string(),
                media_type: "text/plain".to_string(),
                content_digest: digest_bytes(content.as_bytes()),
                bytes: content.len(),
            }],
        },
        files: vec![FrozenSourceFile {
            path: path.to_string(),
            content: content.as_bytes().to_vec(),
        }],
    }
}

fn canonical_json(package: &ProviderMaterializationPackage) -> Vec<u8> {
    serde_json::to_vec(package).expect("package serializes")
}

fn digest_string(value: &str) -> String {
    digest_bytes(value.as_bytes())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn contains_forbidden_word(haystack: &str, needle: &str) -> bool {
    haystack
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| token == needle)
}
