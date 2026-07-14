#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/extensions/source_mapping.rs"]
mod source_mapping;

use serde_json::{Value, json};
use source_mapping::{
    AnchorMapping, AssignmentMapping, CANON_SOURCE_MAPPING_VERSION, CapturePolicy,
    CellDispositionReason, ObservationMapping, RelationshipMapping, RoleBinding, SourceFormat,
    SourceMappingDocumentationRef, SourceMappingErrorCode, SourceMappingPackage,
    SourceMappingPolicies, SourceMappingProfile, SourceMappingProfileRef, SourceRecord,
    canonical_package_bytes, finalize_package, map_record, resolve_profile_ref,
    source_mapping_package_digest, source_mapping_schema_version, validate_package_for_execution,
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.source.mapping.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/source_mapping.rs");

#[test]
fn schema_declares_generic_source_mapping_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_SOURCE_MAPPING_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_SOURCE_MAPPING_VERSION
    );
    assert_eq!(
        schema["$defs"]["profile_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(schema["x-canon-contract"]["generic_reader_only"], true);
    assert_eq!(
        schema["x-canon-contract"]["artifact_families"],
        json!(["observations", "typed_assignments", "relationship_facts"])
    );
    assert_eq!(
        schema["x-canon-contract"]["identity_inference_forbidden"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["real_world_category_enums_forbidden"],
        true
    );
    assert_eq!(
        source_mapping_schema_version(),
        CANON_SOURCE_MAPPING_VERSION
    );

    for forbidden in ["cmbs", "regab", "loan", "servicer"] {
        assert!(
            !MODULE_SOURCE.contains(forbidden),
            "module must remain domain-neutral: {forbidden}"
        );
    }
}

#[test]
fn nested_record_maps_to_separate_artifacts_with_digest_locator_and_no_identity_inference() {
    let package = finalize_package(sample_package(true)).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("digest computes");
    let reference = SourceMappingProfileRef {
        package_digest: digest.clone(),
        profile_id: "pkg.synthetic:vendor_contacts".to_string(),
    };
    let record = SourceRecord {
        format: SourceFormat::Jsonl,
        payload: json!({
            "meta": {
                "record_id": "rec-17",
                "row": "rows/17",
                "as_of": "2026-06-30",
                "fragment": "sheet-1"
            },
            "subject": {
                "display_name": "Acme Analytics LLC",
                "valid_from": "2026-01-01",
                "lei": "5493001KJTIIGC8Y1R12"
            },
            "roles": {
                "lead": {
                    "kind": "observer",
                    "name": "Jordan Example",
                    "email": "jordan@example.test"
                }
            },
            "related": {
                "primary_name": "Beta Data Cooperative",
                "lei": "254900A1B2C3D4E5F678"
            },
            "context": {
                "market": "analytics",
                "source_batch": 42
            },
            "unused": {
                "raw_tag": "keep-me"
            }
        }),
    };

    assert_eq!(
        validate_package_for_execution(&package, std::slice::from_ref(&reference)).unwrap(),
        digest
    );
    let profile = resolve_profile_ref(&package, &reference).expect("profile resolves");
    assert_eq!(profile.source_system, "synthetic_feed");

    let mapped = map_record(&package, &reference, &record).expect("record maps");
    assert_eq!(mapped.mapping_digest, digest);
    assert_eq!(mapped.profile_id, "pkg.synthetic:vendor_contacts");
    assert_eq!(mapped.object_id.as_deref(), Some("rec-17"));
    assert_eq!(
        mapped
            .source_locator
            .as_ref()
            .map(|locator| locator.locator.as_str()),
        Some("rows/17")
    );
    assert_eq!(
        mapped
            .source_locator
            .as_ref()
            .and_then(|locator| locator.fragment.as_deref()),
        Some("sheet-1")
    );
    assert_eq!(mapped.temporal.as_of.as_deref(), Some("2026-06-30"));
    assert_eq!(mapped.temporal.valid_from.as_deref(), Some("2026-01-01"));
    assert_eq!(mapped.observations.len(), 1);
    assert_eq!(mapped.assignments.len(), 1);
    assert_eq!(mapped.relationships.len(), 1);

    let observation = &mapped.observations[0];
    assert_eq!(observation.subject_type_id, "types.synthetic:organization");
    assert_eq!(observation.surface.value, "Acme Analytics LLC");
    assert_eq!(observation.anchors[0].namespace, "lei");
    assert_eq!(observation.anchors[0].value, "5493001KJTIIGC8Y1R12");
    assert_eq!(observation.provenance.mapping_digest, digest);
    assert_eq!(
        observation.provenance.source_locator.locator,
        "rows/17".to_string()
    );

    let assignment = &mapped.assignments[0];
    assert_eq!(assignment.role_id, "pkg.synthetic.role:observer");
    assert_eq!(assignment.assignee_type_id, "types.synthetic:person");
    assert_eq!(assignment.assignee_surface.value, "Jordan Example");
    assert_eq!(
        assignment.context["roles.lead.email"],
        json!("jordan@example.test")
    );

    let relationship = &mapped.relationships[0];
    assert_eq!(
        relationship.relation_type_id,
        "pkg.synthetic:related_counterparty"
    );
    assert_eq!(relationship.object_type_id, "types.synthetic:organization");
    assert_eq!(relationship.object_surface.value, "Beta Data Cooperative");

    assert!(
        mapped
            .preserved_cells
            .iter()
            .any(|cell| cell.reason == CellDispositionReason::UnknownField
                && cell.path == "unused.raw_tag")
    );

    let serialized = serde_json::to_string(&mapped).expect("mapped artifacts serialize");
    assert!(!serialized.contains("canonical_id"));
    assert!(!serialized.contains("identity_id"));
    assert!(!serialized.contains("same_as"));
}

#[test]
fn ambiguous_cells_and_unknown_roles_follow_declared_policy() {
    let mut package = sample_package(false);
    package.profiles[0].policies.unknown_field = CapturePolicy::Preserve;
    package.profiles[0].policies.ambiguous_cell = CapturePolicy::Quarantine;
    package.profiles[0].policies.unknown_role = CapturePolicy::Quarantine;
    package.profiles[0].assignments[0].role_binding = RoleBinding::Field {
        path: "roles.lead.kind".to_string(),
        namespace: "pkg.synthetic.role".to_string(),
        allowed_values: [("manager".to_string(), "pkg.synthetic:manager".to_string())]
            .into_iter()
            .collect(),
        allow_verbatim_values: false,
    };

    let package = finalize_package(package).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("digest computes");
    let reference = SourceMappingProfileRef {
        package_digest: digest,
        profile_id: "pkg.synthetic:vendor_contacts".to_string(),
    };
    let record = SourceRecord {
        format: SourceFormat::Jsonl,
        payload: json!({
            "meta": {
                "record_id": "rec-19",
                "row": "rows/19",
                "as_of": "2026-07-01"
            },
            "subject": {
                "display_name": "Acme Analytics LLC"
            },
            "roles": {
                "lead": {
                    "kind": "approver",
                    "name": ["Jordan Example", "J. Example"]
                }
            },
            "related": {
                "primary_name": "Beta Data Cooperative"
            },
            "shadow": {
                "drift": "preserve"
            }
        }),
    };

    let mapped = map_record(&package, &reference, &record).expect("record maps");
    assert!(mapped.assignments.is_empty());
    assert!(
        mapped
            .preserved_cells
            .iter()
            .any(|cell| cell.reason == CellDispositionReason::UnknownField
                && cell.path == "shadow.drift")
    );
    assert!(
        mapped
            .quarantined_cells
            .iter()
            .any(|cell| cell.reason == CellDispositionReason::UnknownRole
                && cell.path == "roles.lead.kind"
                && cell.value == json!("approver"))
    );
    assert!(
        mapped
            .quarantined_cells
            .iter()
            .any(|cell| cell.reason == CellDispositionReason::AmbiguousCell
                && cell.path == "roles.lead.name")
    );
}

#[test]
fn incompatible_digest_and_missing_required_field_fail_before_execution() {
    let package = finalize_package(sample_package(false)).expect("package finalizes");
    let reference = SourceMappingProfileRef {
        package_digest: "blake3:0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        profile_id: "pkg.synthetic:vendor_contacts".to_string(),
    };
    let error = validate_package_for_execution(&package, std::slice::from_ref(&reference))
        .expect_err("digest mismatch must fail");
    assert_eq!(error.code, SourceMappingErrorCode::CompatibilityPolicy);

    let digest = source_mapping_package_digest(&package).expect("digest computes");
    let good_reference = SourceMappingProfileRef {
        package_digest: digest,
        profile_id: "pkg.synthetic:vendor_contacts".to_string(),
    };
    let record = SourceRecord {
        format: SourceFormat::Jsonl,
        payload: json!({
            "meta": {
                "row": "rows/21"
            },
            "subject": {
                "display_name": "Acme Analytics LLC"
            },
            "roles": {
                "lead": {
                    "kind": "observer",
                    "name": "Jordan Example"
                }
            },
            "related": {
                "primary_name": "Beta Data Cooperative"
            }
        }),
    };
    let error = map_record(&package, &good_reference, &record)
        .expect_err("missing object id must fail under reject policy");
    assert_eq!(error.code, SourceMappingErrorCode::MissingField);
}

#[test]
fn canonical_bytes_are_stable_across_ordering_noise() {
    let package = finalize_package(sample_package(true)).expect("package finalizes");
    let canonical_a = canonical_package_bytes(&package).expect("bytes serialize");

    let mut reordered = sample_package(true);
    reordered.documentation.reverse();
    reordered.profiles.reverse();
    reordered.profiles[0].observations.reverse();
    reordered.profiles[0].assignments.reverse();
    reordered.profiles[0].relationships.reverse();
    let canonical_b =
        canonical_package_bytes(&finalize_package(reordered).expect("reordered finalizes"))
            .expect("bytes serialize");

    assert_eq!(canonical_a, canonical_b);
}

fn sample_package(allow_verbatim_role_values: bool) -> SourceMappingPackage {
    SourceMappingPackage {
        version: CANON_SOURCE_MAPPING_VERSION.to_string(),
        package_id: "pkg.synthetic".to_string(),
        package_version: "1.2.3".to_string(),
        profiles: vec![SourceMappingProfile {
            profile_id: "pkg.synthetic:vendor_contacts".to_string(),
            source_system: "synthetic_feed".to_string(),
            source_formats: vec![SourceFormat::Jsonl],
            object_id_path: "meta.record_id".to_string(),
            locator_path: "meta.row".to_string(),
            fragment_path: Some("meta.fragment".to_string()),
            as_of_path: Some("meta.as_of".to_string()),
            valid_from_path: Some("subject.valid_from".to_string()),
            valid_to_path: Some("subject.valid_to".to_string()),
            observations: vec![ObservationMapping {
                mapping_id: "pkg.synthetic:subject_surface".to_string(),
                subject_type_id: "types.synthetic:organization".to_string(),
                surface_path: "subject.display_name".to_string(),
                anchor_mappings: vec![AnchorMapping {
                    namespace: "lei".to_string(),
                    path: "subject.lei".to_string(),
                }],
                context_paths: vec!["context.market".to_string()],
            }],
            assignments: vec![AssignmentMapping {
                mapping_id: "pkg.synthetic:lead_assignment".to_string(),
                subject_type_id: "types.synthetic:organization".to_string(),
                assignee_type_id: "types.synthetic:person".to_string(),
                role_binding: RoleBinding::Field {
                    path: "roles.lead.kind".to_string(),
                    namespace: "pkg.synthetic.role".to_string(),
                    allowed_values: Default::default(),
                    allow_verbatim_values: allow_verbatim_role_values,
                },
                assignee_surface_path: "roles.lead.name".to_string(),
                assignee_anchor_mappings: vec![AnchorMapping {
                    namespace: "email".to_string(),
                    path: "roles.lead.email".to_string(),
                }],
                context_paths: vec!["roles.lead.email".to_string()],
            }],
            relationships: vec![RelationshipMapping {
                mapping_id: "pkg.synthetic:related_subject".to_string(),
                subject_type_id: "types.synthetic:organization".to_string(),
                relation_type_id: "pkg.synthetic:related_counterparty".to_string(),
                object_type_id: "types.synthetic:organization".to_string(),
                object_surface_path: "related.primary_name".to_string(),
                object_anchor_mappings: vec![AnchorMapping {
                    namespace: "lei".to_string(),
                    path: "related.lei".to_string(),
                }],
                context_paths: vec!["context.source_batch".to_string()],
            }],
            policies: SourceMappingPolicies::default(),
            documentation_refs: vec!["docs/source_mapping.md".to_string()],
        }],
        documentation: vec![SourceMappingDocumentationRef {
            label: "contract".to_string(),
            uri: "docs/source_mapping.md".to_string(),
        }],
    }
}
