#![forbid(unsafe_code)]

#[path = "../src/provider_sdk.rs"]
mod provider_sdk;

use provider_sdk::{
    DeclaredSourceFile, FrozenSourceBundle, FrozenSourceFile, FrozenSourceManifest,
    FrozenSourceProvider, ProviderCheckpoint, ProviderFactRecord, ProviderManifest,
    ProviderMaterializationDraft, ProviderSdkError, ProviderSdkErrorCode, ProviderSdkResult,
    SourceRecordLocator, provider_manifest_schema_version, run_provider_conformance, semantic_diff,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const PROVIDER_SDK_DOC: &str = include_str!("../docs/PROVIDER_SDK.md");
const PROVIDER_SDK_SOURCE: &str = include_str!("../src/provider_sdk.rs");
const SCHEMA_JSON: &str = include_str!("../schemas/canon.provider.manifest.v1.schema.json");
const FIXTURE_MANIFEST_JSON: &str =
    include_str!("fixtures/providers/neutral-identity/manifest.json");
const FIXTURE_SOURCE_JSONL: &str = include_str!("fixtures/providers/neutral-identity/source.jsonl");
const EXPECTED_FACTS_JSON: &str =
    include_str!("fixtures/providers/neutral-identity/expected_facts.json");

#[derive(Debug, Clone, Deserialize)]
struct NeutralFixtureManifest {
    fixture_manifest_version: String,
    provider_manifest: ProviderManifest,
    source_path: String,
    source_media_type: String,
    source_manifest_version: String,
    source_id: String,
    source_version: String,
    source_revision: String,
    selection_predicate: String,
    package_bindings: NeutralPackageBindings,
}

#[derive(Debug, Clone, Deserialize)]
struct NeutralPackageBindings {
    ontology_package: String,
    identity_vocabulary_package: String,
    relationship_vocabulary_package: String,
    status_vocabulary_package: String,
    exception_vocabulary_package: String,
    identifier_namespace_package: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NeutralExpectedFacts {
    selection_predicate: String,
    required_fact_families: Vec<String>,
    excluded_record_ids: Vec<String>,
    projected_facts: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct NeutralSourceRow {
    record_id: String,
    record_kind: String,
    cohort: String,
    entity_key: Option<String>,
    primary_name: Option<String>,
    alias_name: Option<String>,
    alias_kind: Option<String>,
    former_name: Option<String>,
    identifier_namespace: Option<String>,
    identifier_value: Option<String>,
    status: Option<String>,
    as_of: String,
    valid_from: Option<String>,
    valid_to: Option<String>,
    subject_entity_key: Option<String>,
    relation_type: Option<String>,
    object_entity_key: Option<String>,
    exception_code: Option<String>,
    exception_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderMode {
    Valid,
    MissingProvenance,
    IncompatibleBindings,
    HiddenNetwork,
}

struct NeutralIdentityProvider {
    mode: ProviderMode,
}

struct EmissionContext<'a> {
    fact_schema: &'a str,
    source_path: &'a str,
    source_digest: &'a str,
    selection_predicate: &'a str,
    bindings: &'a NeutralPackageBindings,
}

struct FactSpec<'a> {
    fact_key: String,
    assertion_kind: &'a str,
    fact_family: &'a str,
    entity_key: Option<&'a str>,
    value: &'a str,
    vocabulary_package: &'a str,
    namespace_package: Option<&'a str>,
    field_path: &'a str,
}

#[test]
fn provider_sdk_docs_and_schema_describe_neutral_fact_family_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    let fixture = fixture_manifest();
    let lower_doc = PROVIDER_SDK_DOC.to_ascii_lowercase();

    assert_eq!(schema["title"], provider_manifest_schema_version());
    assert_eq!(schema["x-canon-contract"]["offline_build_only"], true);
    assert_eq!(
        schema["x-canon-contract"]["typed_fact_provenance_required"],
        true
    );
    assert_eq!(
        fixture.fixture_manifest_version,
        "neutral_provider_fixture.v1"
    );
    assert_eq!(
        fixture.provider_manifest.mapping.fact_schema,
        "canon.provider.neutral_identity_fact.v1"
    );

    for required in [
        "identity facts",
        "relationship facts",
        "status facts",
        "exception facts",
        "subset predicate",
        "compatibility policy",
        "invented neutral fixture records",
    ] {
        assert!(
            lower_doc.contains(required),
            "provider sdk docs should mention {required}"
        );
    }
}

#[test]
fn neutral_fixture_provider_emits_distinct_black_box_fact_families() {
    let fixture = fixture_manifest();
    let expected = expected_facts();
    let bundle = fixture_bundle(FIXTURE_SOURCE_JSONL, &fixture.source_revision);
    let package = run_provider_conformance(
        &NeutralIdentityProvider {
            mode: ProviderMode::Valid,
        },
        &bundle,
        None,
    )
    .expect("neutral provider conforms");

    let projected = package.facts.iter().map(project_fact).collect::<Vec<_>>();
    assert_eq!(projected, expected.projected_facts);
    assert_eq!(expected.selection_predicate, fixture.selection_predicate);
    assert!(package.quarantined_rows.is_empty());
    assert!(package.diagnostics.is_empty());

    let families = package
        .facts
        .iter()
        .map(|fact| fact.fields["fact_family"].clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        families,
        expected
            .required_fact_families
            .into_iter()
            .collect::<BTreeSet<_>>()
    );

    let expected_source_digest = source_digest(FIXTURE_SOURCE_JSONL);
    assert!(
        package
            .facts
            .iter()
            .all(|fact| fact.source_digest == expected_source_digest)
    );
    assert!(
        package
            .facts
            .iter()
            .all(|fact| fact.locator.field_path.is_some())
    );
    assert!(
        package
            .facts
            .iter()
            .all(|fact| fact.fields["subset_predicate"] == fixture.selection_predicate)
    );
    assert!(
        package.facts.iter().all(
            |fact| fact.fields["ontology_package"] == fixture.package_bindings.ontology_package
        )
    );
    assert!(package.facts.iter().all(|fact| {
        !expected
            .excluded_record_ids
            .contains(&fact.fields["source_record_id"])
    }));
}

#[test]
fn subset_rebuilds_are_deterministic_and_declare_their_selection_predicate() {
    let fixture = fixture_manifest();
    let provider = NeutralIdentityProvider {
        mode: ProviderMode::Valid,
    };
    let base_bundle = fixture_bundle(FIXTURE_SOURCE_JSONL, &fixture.source_revision);
    let rerun_bundle = fixture_bundle(FIXTURE_SOURCE_JSONL, &fixture.source_revision);
    let expanded_source =
        FIXTURE_SOURCE_JSONL.replace("\"cohort\":\"subset-b\"", "\"cohort\":\"subset-a\"");
    let expanded_bundle = fixture_bundle(&expanded_source, "subset-a-r2");

    let base = run_provider_conformance(&provider, &base_bundle, None).expect("base build");
    let rerun = run_provider_conformance(&provider, &rerun_bundle, None).expect("rerun build");
    let expanded =
        run_provider_conformance(&provider, &expanded_bundle, None).expect("expanded build");

    assert_eq!(canonical_json(&base), canonical_json(&rerun));
    assert!(
        base.facts
            .iter()
            .all(|fact| fact.fields["subset_predicate"] == fixture.selection_predicate)
    );
    let diff = semantic_diff(&base, &expanded).expect("semantic diff");
    assert!(diff.source_manifest_digest_changed);
    assert_eq!(
        diff.added_fact_keys,
        vec![
            "identity:identifier:entity.excluded:id.synthetic.external:row-006".to_string(),
            "identity:name:entity.excluded:primary:row-006".to_string(),
            "status:entity.excluded:active:row-006".to_string(),
        ]
    );
}

#[test]
fn incompatible_bindings_missing_provenance_and_hidden_network_fail_conformance() {
    let fixture = fixture_manifest();
    let bundle = fixture_bundle(FIXTURE_SOURCE_JSONL, &fixture.source_revision);

    let compatibility = run_provider_conformance(
        &NeutralIdentityProvider {
            mode: ProviderMode::IncompatibleBindings,
        },
        &bundle,
        None,
    )
    .expect_err("incompatible vocabulary binding must fail");
    assert_eq!(
        compatibility.code,
        ProviderSdkErrorCode::CompatibilityPolicy
    );

    let provenance = run_provider_conformance(
        &NeutralIdentityProvider {
            mode: ProviderMode::MissingProvenance,
        },
        &bundle,
        None,
    )
    .expect_err("missing provenance must fail");
    assert_eq!(provenance.code, ProviderSdkErrorCode::ArtifactContract);

    let network = run_provider_conformance(
        &NeutralIdentityProvider {
            mode: ProviderMode::HiddenNetwork,
        },
        &bundle,
        None,
    )
    .expect_err("hidden network access must fail");
    assert_eq!(network.code, ProviderSdkErrorCode::OfflinePolicy);
}

#[test]
fn source_scan_keeps_provider_fixture_and_docs_domain_neutral() {
    let lower_source = PROVIDER_SDK_SOURCE.to_ascii_lowercase();
    let lower_doc = PROVIDER_SDK_DOC.to_ascii_lowercase();
    let lower_fixture = FIXTURE_SOURCE_JSONL.to_ascii_lowercase();

    for banned in ["openfigi", "sec", "loan", "issuer", "servicer"] {
        assert!(
            !contains_forbidden_word(&lower_source, banned),
            "provider sdk should not embed concrete domain term {banned}"
        );
        assert!(
            !contains_forbidden_word(&lower_doc, banned),
            "provider docs should not embed concrete domain term {banned}"
        );
        assert!(
            !contains_forbidden_word(&lower_fixture, banned),
            "neutral fixture should not embed concrete domain term {banned}"
        );
    }
}

impl FrozenSourceProvider for NeutralIdentityProvider {
    fn manifest(&self) -> ProviderManifest {
        fixture_manifest().provider_manifest
    }

    fn materialize(
        &self,
        bundle: &FrozenSourceBundle,
        _checkpoint: Option<&ProviderCheckpoint>,
    ) -> ProviderSdkResult<ProviderMaterializationDraft> {
        let fixture = fixture_manifest();
        let source_file = &bundle.files[0];
        let source_digest = bundle.manifest.files[0].content_digest.clone();

        if self.mode == ProviderMode::HiddenNetwork {
            return Ok(ProviderMaterializationDraft {
                used_source_paths: vec![source_file.path.clone()],
                attempted_network_access: true,
                facts: Vec::new(),
                quarantined_rows: Vec::new(),
                diagnostics: Vec::new(),
                checkpoint: None,
            });
        }

        let context = EmissionContext {
            fact_schema: &fixture.provider_manifest.mapping.fact_schema,
            source_path: &source_file.path,
            source_digest: &source_digest,
            selection_predicate: &fixture.selection_predicate,
            bindings: &fixture.package_bindings,
        };

        let mut facts = Vec::new();
        for (index, line) in std::str::from_utf8(&source_file.content)
            .expect("fixture source is utf-8")
            .lines()
            .enumerate()
        {
            let record_ordinal = u64::try_from(index + 1).expect("record ordinal fits u64");
            let row: NeutralSourceRow =
                serde_json::from_str(line).expect("neutral fixture row parses");
            if row.cohort != "subset-a" {
                continue;
            }

            match row.record_kind.as_str() {
                "identity_profile" => {
                    self.emit_identity_profile_facts(&row, record_ordinal, &context, &mut facts)?
                }
                "relationship" => {
                    self.emit_relationship_fact(&row, record_ordinal, &context, &mut facts)?
                }
                "exception" => {
                    self.emit_exception_fact(&row, record_ordinal, &context, &mut facts)?
                }
                other => {
                    return Err(ProviderSdkError::new(
                        ProviderSdkErrorCode::CompatibilityPolicy,
                        format!("unsupported neutral fixture record_kind {other}"),
                    ));
                }
            }
        }

        if self.mode == ProviderMode::MissingProvenance {
            let first = facts.first_mut().expect("fixture emits at least one fact");
            first.locator.line_number = 0;
        }

        Ok(ProviderMaterializationDraft {
            used_source_paths: vec![source_file.path.clone()],
            attempted_network_access: false,
            facts,
            quarantined_rows: Vec::new(),
            diagnostics: Vec::new(),
            checkpoint: None,
        })
    }
}

impl NeutralIdentityProvider {
    fn emit_identity_profile_facts(
        &self,
        row: &NeutralSourceRow,
        record_ordinal: u64,
        context: &EmissionContext<'_>,
        facts: &mut Vec<ProviderFactRecord>,
    ) -> ProviderSdkResult<()> {
        let entity_key = required_field(&row.entity_key, "entity_key")?;
        let primary_name = required_field(&row.primary_name, "primary_name")?;
        let status = required_field(&row.status, "status")?;

        self.validate_binding(
            "identity_vocabulary_package",
            &context.bindings.identity_vocabulary_package,
            &self.identity_vocabulary_package(context),
        )?;
        self.validate_binding(
            "status_vocabulary_package",
            &context.bindings.status_vocabulary_package,
            &self.status_vocabulary_package(context),
        )?;
        if row.identifier_value.is_some() {
            self.validate_binding(
                "identifier_namespace_package",
                &context.bindings.identifier_namespace_package,
                &self.identifier_namespace_package(context),
            )?;
        }

        facts.push(self.build_fact(
            FactSpec {
                fact_key: format!("identity:name:{entity_key}:primary:{}", row.record_id),
                assertion_kind: "primary_name",
                fact_family: "identity",
                entity_key: Some(entity_key),
                value: primary_name,
                vocabulary_package: &context.bindings.identity_vocabulary_package,
                namespace_package: None,
                field_path: "primary_name",
            },
            record_ordinal,
            row,
            context,
        ));

        if let Some(alias_name) = &row.alias_name {
            let alias_kind = required_field(&row.alias_kind, "alias_kind")?;
            let mut fact = self.build_fact(
                FactSpec {
                    fact_key: format!("identity:name:{entity_key}:alias:{}", row.record_id),
                    assertion_kind: "alias_name",
                    fact_family: "identity",
                    entity_key: Some(entity_key),
                    value: alias_name,
                    vocabulary_package: &context.bindings.identity_vocabulary_package,
                    namespace_package: None,
                    field_path: "alias_name",
                },
                record_ordinal,
                row,
                context,
            );
            fact.fields
                .insert("alias_kind".to_string(), alias_kind.to_string());
            facts.push(fact);
        }

        if let Some(former_name) = &row.former_name {
            facts.push(self.build_fact(
                FactSpec {
                    fact_key: format!("identity:name:{entity_key}:former:{}", row.record_id),
                    assertion_kind: "former_name",
                    fact_family: "identity",
                    entity_key: Some(entity_key),
                    value: former_name,
                    vocabulary_package: &context.bindings.identity_vocabulary_package,
                    namespace_package: None,
                    field_path: "former_name",
                },
                record_ordinal,
                row,
                context,
            ));
        }

        if let Some(identifier_value) = &row.identifier_value {
            let identifier_namespace =
                required_field(&row.identifier_namespace, "identifier_namespace")?;
            let mut fact = self.build_fact(
                FactSpec {
                    fact_key: format!(
                        "identity:identifier:{entity_key}:{identifier_namespace}:{}",
                        row.record_id
                    ),
                    assertion_kind: "identifier",
                    fact_family: "identity",
                    entity_key: Some(entity_key),
                    value: identifier_value,
                    vocabulary_package: &context.bindings.identity_vocabulary_package,
                    namespace_package: Some(&context.bindings.identifier_namespace_package),
                    field_path: "identifier_value",
                },
                record_ordinal,
                row,
                context,
            );
            fact.fields.insert(
                "identifier_namespace".to_string(),
                identifier_namespace.to_string(),
            );
            facts.push(fact);
        }

        let mut status_fact = self.build_fact(
            FactSpec {
                fact_key: format!("status:{entity_key}:{status}:{}", row.record_id),
                assertion_kind: "status",
                fact_family: "status",
                entity_key: Some(entity_key),
                value: status,
                vocabulary_package: &context.bindings.status_vocabulary_package,
                namespace_package: None,
                field_path: "status",
            },
            record_ordinal,
            row,
            context,
        );
        status_fact
            .fields
            .insert("status_code".to_string(), status.to_string());
        facts.push(status_fact);

        Ok(())
    }

    fn emit_relationship_fact(
        &self,
        row: &NeutralSourceRow,
        record_ordinal: u64,
        context: &EmissionContext<'_>,
        facts: &mut Vec<ProviderFactRecord>,
    ) -> ProviderSdkResult<()> {
        let subject_entity_key = required_field(&row.subject_entity_key, "subject_entity_key")?;
        let relation_type = required_field(&row.relation_type, "relation_type")?;
        let object_entity_key = required_field(&row.object_entity_key, "object_entity_key")?;

        let relationship_vocabulary = self.relationship_vocabulary_package(context);
        self.validate_binding(
            "relationship_vocabulary_package",
            &context.bindings.relationship_vocabulary_package,
            &relationship_vocabulary,
        )?;

        let mut fact = self.build_fact(
            FactSpec {
                fact_key: format!(
                    "relationship:{subject_entity_key}:{relation_type}:{object_entity_key}:{}",
                    row.record_id
                ),
                assertion_kind: "relationship",
                fact_family: "relationship",
                entity_key: None,
                value: relation_type,
                vocabulary_package: &relationship_vocabulary,
                namespace_package: None,
                field_path: "relation_type",
            },
            record_ordinal,
            row,
            context,
        );
        fact.fields.insert(
            "subject_entity_key".to_string(),
            subject_entity_key.to_string(),
        );
        fact.fields
            .insert("relation_type".to_string(), relation_type.to_string());
        fact.fields.insert(
            "object_entity_key".to_string(),
            object_entity_key.to_string(),
        );
        facts.push(fact);
        Ok(())
    }

    fn emit_exception_fact(
        &self,
        row: &NeutralSourceRow,
        record_ordinal: u64,
        context: &EmissionContext<'_>,
        facts: &mut Vec<ProviderFactRecord>,
    ) -> ProviderSdkResult<()> {
        let entity_key = required_field(&row.entity_key, "entity_key")?;
        let exception_code = required_field(&row.exception_code, "exception_code")?;
        let exception_detail = required_field(&row.exception_detail, "exception_detail")?;

        self.validate_binding(
            "exception_vocabulary_package",
            &context.bindings.exception_vocabulary_package,
            &self.exception_vocabulary_package(context),
        )?;

        let mut fact = self.build_fact(
            FactSpec {
                fact_key: format!("exception:{entity_key}:{exception_code}:{}", row.record_id),
                assertion_kind: "exception",
                fact_family: "exception",
                entity_key: Some(entity_key),
                value: exception_detail,
                vocabulary_package: &context.bindings.exception_vocabulary_package,
                namespace_package: None,
                field_path: "exception_code",
            },
            record_ordinal,
            row,
            context,
        );
        fact.fields
            .insert("exception_code".to_string(), exception_code.to_string());
        fact.fields
            .insert("detail".to_string(), exception_detail.to_string());
        facts.push(fact);
        Ok(())
    }

    fn build_fact(
        &self,
        spec: FactSpec<'_>,
        record_ordinal: u64,
        row: &NeutralSourceRow,
        context: &EmissionContext<'_>,
    ) -> ProviderFactRecord {
        let mut fields = std::collections::BTreeMap::from([
            (
                "assertion_kind".to_string(),
                spec.assertion_kind.to_string(),
            ),
            ("fact_family".to_string(), spec.fact_family.to_string()),
            (
                "ontology_package".to_string(),
                context.bindings.ontology_package.clone(),
            ),
            ("source_record_id".to_string(), row.record_id.clone()),
            (
                "subset_predicate".to_string(),
                context.selection_predicate.to_string(),
            ),
            ("value".to_string(), spec.value.to_string()),
            (
                "vocabulary_package".to_string(),
                spec.vocabulary_package.to_string(),
            ),
            ("as_of".to_string(), row.as_of.clone()),
        ]);
        if let Some(entity_key) = spec.entity_key {
            fields.insert("entity_key".to_string(), entity_key.to_string());
        }
        if let Some(valid_from) = &row.valid_from {
            fields.insert("valid_from".to_string(), valid_from.clone());
        }
        if let Some(valid_to) = &row.valid_to {
            fields.insert("valid_to".to_string(), valid_to.clone());
        }
        if let Some(namespace_package) = spec.namespace_package {
            fields.insert(
                "namespace_package".to_string(),
                namespace_package.to_string(),
            );
        }

        ProviderFactRecord {
            fact_key: spec.fact_key,
            fact_schema: context.fact_schema.to_string(),
            fields,
            source_digest: context.source_digest.to_string(),
            locator: SourceRecordLocator {
                source_path: context.source_path.to_string(),
                record_ordinal,
                line_number: record_ordinal,
                field_path: Some(spec.field_path.to_string()),
            },
        }
    }

    fn identity_vocabulary_package(&self, context: &EmissionContext<'_>) -> String {
        context.bindings.identity_vocabulary_package.clone()
    }

    fn relationship_vocabulary_package(&self, context: &EmissionContext<'_>) -> String {
        if self.mode == ProviderMode::IncompatibleBindings {
            "pkg.neutral.relationship_vocab@blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .to_string()
        } else {
            context.bindings.relationship_vocabulary_package.clone()
        }
    }

    fn status_vocabulary_package(&self, context: &EmissionContext<'_>) -> String {
        context.bindings.status_vocabulary_package.clone()
    }

    fn exception_vocabulary_package(&self, context: &EmissionContext<'_>) -> String {
        context.bindings.exception_vocabulary_package.clone()
    }

    fn identifier_namespace_package(&self, context: &EmissionContext<'_>) -> String {
        context.bindings.identifier_namespace_package.clone()
    }

    fn validate_binding(&self, label: &str, expected: &str, actual: &str) -> ProviderSdkResult<()> {
        if expected == actual {
            return Ok(());
        }
        Err(ProviderSdkError::new(
            ProviderSdkErrorCode::CompatibilityPolicy,
            format!("{label} mismatch: expected {expected} but provider emitted {actual}"),
        ))
    }
}

fn fixture_manifest() -> NeutralFixtureManifest {
    serde_json::from_str(FIXTURE_MANIFEST_JSON).expect("fixture manifest parses")
}

fn expected_facts() -> NeutralExpectedFacts {
    serde_json::from_str(EXPECTED_FACTS_JSON).expect("expected facts fixture parses")
}

fn fixture_bundle(source_text: &str, source_revision: &str) -> FrozenSourceBundle {
    let fixture = fixture_manifest();
    let content = source_text.as_bytes().to_vec();
    FrozenSourceBundle {
        manifest: FrozenSourceManifest {
            manifest_version: fixture.source_manifest_version,
            source_id: fixture.source_id,
            source_version: fixture.source_version,
            source_revision: source_revision.to_string(),
            files: vec![DeclaredSourceFile {
                path: fixture.source_path.clone(),
                media_type: fixture.source_media_type,
                content_digest: source_digest(source_text),
                bytes: content.len(),
            }],
        },
        files: vec![FrozenSourceFile {
            path: fixture.source_path,
            content,
        }],
    }
}

fn source_digest(source_text: &str) -> String {
    format!("blake3:{}", blake3::hash(source_text.as_bytes()).to_hex())
}

fn project_fact(fact: &ProviderFactRecord) -> Value {
    let mut object = Map::new();
    object.insert("fact_key".to_string(), Value::String(fact.fact_key.clone()));
    insert_field(&mut object, "fact_family", fact);
    insert_field(&mut object, "assertion_kind", fact);
    insert_field(&mut object, "entity_key", fact);
    insert_field(&mut object, "subject_entity_key", fact);
    insert_field(&mut object, "relation_type", fact);
    insert_field(&mut object, "object_entity_key", fact);
    insert_field(&mut object, "value", fact);
    insert_field(&mut object, "status_code", fact);
    insert_field(&mut object, "exception_code", fact);
    insert_field(&mut object, "detail", fact);
    insert_field(&mut object, "alias_kind", fact);
    insert_field(&mut object, "identifier_namespace", fact);
    insert_field(&mut object, "subset_predicate", fact);
    insert_field(&mut object, "ontology_package", fact);
    insert_field(&mut object, "vocabulary_package", fact);
    insert_field(&mut object, "namespace_package", fact);
    insert_field(&mut object, "source_record_id", fact);
    insert_field(&mut object, "as_of", fact);
    insert_field(&mut object, "valid_from", fact);
    insert_field(&mut object, "valid_to", fact);
    object.insert(
        "source_path".to_string(),
        Value::String(fact.locator.source_path.clone()),
    );
    if let Some(field_path) = &fact.locator.field_path {
        object.insert("field_path".to_string(), Value::String(field_path.clone()));
    }
    Value::Object(object)
}

fn insert_field(object: &mut Map<String, Value>, key: &str, fact: &ProviderFactRecord) {
    if let Some(value) = fact.fields.get(key) {
        object.insert(key.to_string(), Value::String(value.clone()));
    }
}

fn canonical_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("value serializes")
}

fn required_field<'a>(value: &'a Option<String>, field: &str) -> ProviderSdkResult<&'a str> {
    value.as_deref().ok_or_else(|| {
        ProviderSdkError::new(
            ProviderSdkErrorCode::CompatibilityPolicy,
            format!("neutral fixture row is missing required field {field}"),
        )
    })
}

fn contains_forbidden_word(haystack: &str, needle: &str) -> bool {
    haystack
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == needle)
}
