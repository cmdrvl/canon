#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/extensions/source_mapping.rs"]
mod source_mapping;

use serde_json::{Value, json};
use source_mapping::{
    AnchorMapping, AssignmentMapping, CANON_ENTITY_RECORD_LINK_INPUT_VERSION,
    CANON_SOURCE_MAPPING_VERSION, CapturePolicy, ObservationMapping, RecordLinkComparisonKind,
    RecordLinkComparisonMapping, RecordLinkComparisonPolicies, RecordLinkComparisonSource,
    RecordLinkFieldDispositionReason, RecordLinkInputBuildRequest, RoleBinding, SourceFormat,
    SourceMappingDocumentationRef, SourceMappingErrorCode, SourceMappingPackage,
    SourceMappingProfile, SourceMappingProfileRef, SourceRecord, build_record_link_input_sidecar,
    canonical_record_link_input_bytes, finalize_package, map_record,
    record_link_input_schema_version, source_mapping_package_digest,
    validate_record_link_input_sidecar,
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.entity.record_link_input.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/source_mapping.rs");

#[test]
fn schema_declares_record_grain_assignment_firewall_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_ENTITY_RECORD_LINK_INPUT_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_ENTITY_RECORD_LINK_INPUT_VERSION
    );
    assert_eq!(schema["x-canon-contract"]["record_grain"], true);
    assert_eq!(
        schema["x-canon-contract"]["identity_inference_forbidden"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["assignment_as_identity_forbidden"],
        true
    );
    assert_eq!(
        record_link_input_schema_version(),
        CANON_ENTITY_RECORD_LINK_INPUT_VERSION
    );
    let required = schema["required"]
        .as_array()
        .expect("top-level required is an array");
    assert!(
        required
            .iter()
            .any(|value| value.as_str() == Some("quarantined_records")),
        "quarantined_records must be present even when empty"
    );
    assert!(
        required
            .iter()
            .any(|value| value.as_str() == Some("source_cell_dispositions")),
        "source cell dispositions must be present even when empty"
    );
    assert_eq!(schema["properties"]["records"]["uniqueItems"], true);
    assert_eq!(
        schema["properties"]["quarantined_records"]["uniqueItems"],
        true
    );
    assert_eq!(
        schema["$defs"]["record"]["properties"]["comparison_views"]["uniqueItems"],
        true
    );
    assert_eq!(
        schema["$defs"]["record"]["properties"]["quarantined_fields"]["uniqueItems"],
        true
    );
    assert_eq!(
        schema["$defs"]["comparison_view"]["oneOf"][1]["properties"]["value"]["$ref"],
        "#/$defs/calendar_date"
    );
    assert_eq!(
        schema["$defs"]["calendar_date"]["x-canon-validation"],
        "is_iso_day_date"
    );
    assert_eq!(
        schema["x-canon-contract"]["non_numeric_units_and_scale_forbidden"],
        true
    );
    assert!(
        schema["$defs"]["calendar_date"]["description"]
            .as_str()
            .expect("calendar_date description")
            .contains("regex alone is not the contract")
    );

    for forbidden in ["canonical_id", "same_as", "identity_id"] {
        assert!(
            !SCHEMA_JSON.contains(forbidden),
            "record-link schema must not mint identity field {forbidden}"
        );
    }
    for forbidden in ["cmbs", "regab", "servicer", "tranche", "loan"] {
        assert!(
            !MODULE_SOURCE.to_ascii_lowercase().contains(forbidden),
            "record-link implementation must remain domain-neutral: {forbidden}"
        );
    }
}

#[test]
fn sidecar_preserves_record_grain_assignments_and_is_order_stable() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);
    let rows = sample_rows();
    let mut mapped = rows
        .iter()
        .map(|payload| {
            map_record(
                &package,
                &reference,
                &SourceRecord {
                    format: SourceFormat::Jsonl,
                    payload: payload.clone(),
                },
            )
            .expect("record maps")
        })
        .collect::<Vec<_>>();
    let request = request(&digest);

    let first =
        build_record_link_input_sidecar(&request, &mapped).expect("record-link input builds");
    mapped.reverse();
    let second =
        build_record_link_input_sidecar(&request, &mapped).expect("record-link input rebuilds");

    let first_bytes = canonical_record_link_input_bytes(&first).expect("first bytes");
    let second_bytes = canonical_record_link_input_bytes(&second).expect("second bytes");
    assert_eq!(first_bytes, second_bytes);
    validate_record_link_input_sidecar(&first).expect("sidecar validates");
    assert_eq!(first.version, CANON_ENTITY_RECORD_LINK_INPUT_VERSION);
    assert_eq!(first.source_mapping_digest, digest);
    assert_eq!(first.summary["record_count"], 4);
    assert_eq!(first.summary["comparison_view_count"], 12);
    assert_eq!(first.summary["quarantined_field_count"], 0);
    assert_eq!(first.summary["preserved_source_cell_count"], 0);
    assert_eq!(first.summary["quarantined_source_cell_count"], 0);

    let row_one_records = first
        .records
        .iter()
        .filter(|record| record.source_ref.source_locator.locator == "rows/1")
        .collect::<Vec<_>>();
    assert_eq!(row_one_records.len(), 2);
    assert_eq!(
        row_one_records[0].subject_observation_ref.observation_id,
        row_one_records[1].subject_observation_ref.observation_id,
        "same source row assignments point to one subject observation"
    );
    assert_ne!(
        row_one_records[0]
            .assignment_ref
            .as_ref()
            .expect("assignment")
            .assignment_id,
        row_one_records[1]
            .assignment_ref
            .as_ref()
            .expect("assignment")
            .assignment_id
    );

    let numeric = first
        .records
        .iter()
        .flat_map(|record| &record.comparison_views)
        .find_map(|view| match view {
            source_mapping::RecordLinkComparisonView::Numeric {
                feature_id,
                units,
                scaled_value,
                scale,
                ..
            } if feature_id == "pkg.synthetic:amount" => {
                Some((units.clone(), *scaled_value, *scale))
            }
            _ => None,
        })
        .expect("numeric comparison view");
    assert_eq!(numeric, ("basis_points".to_string(), 10025, 2));

    let serialized = String::from_utf8(first_bytes).expect("utf8");
    assert!(
        serialized.contains("\"quarantined_records\":[]"),
        "empty quarantined records remain part of the hash-bound contract"
    );
    assert!(!serialized.contains("canonical_id"));
    assert!(!serialized.contains("identity_id"));
    assert!(!serialized.contains("same_as"));
    assert!(!serialized.contains("aliases"));
}

#[test]
fn malformed_missing_overflow_and_duplicate_records_follow_declared_policy() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);
    let mut payload = sample_rows().remove(0);
    payload["context"]
        .as_object_mut()
        .expect("context object")
        .remove("category");
    let mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload,
            },
        )
        .expect("record maps"),
    ];

    let mut quarantine_request = request(&digest);
    quarantine_request
        .comparison_mappings
        .iter_mut()
        .find(|mapping| mapping.feature_id == "pkg.synthetic:category")
        .expect("category mapping")
        .policies
        .missing = CapturePolicy::Quarantine;
    let sidecar = build_record_link_input_sidecar(&quarantine_request, &mapped)
        .expect("missing field quarantines");
    assert_eq!(sidecar.summary["record_count"], 2);
    assert_eq!(sidecar.summary["quarantined_field_count"], 2);
    assert!(sidecar.records.iter().all(|record| {
        record
            .quarantined_fields
            .iter()
            .any(|field| field.reason == RecordLinkFieldDispositionReason::MissingField)
    }));

    let complete_mapped = map_record(
        &package,
        &reference,
        &SourceRecord {
            format: SourceFormat::Jsonl,
            payload: sample_rows().remove(0),
        },
    )
    .expect("complete record maps");
    let duplicate_error = build_record_link_input_sidecar(
        &request(&digest),
        &[complete_mapped.clone(), complete_mapped],
    )
    .expect_err("duplicate record IDs reject by default");
    assert_eq!(
        duplicate_error.code,
        SourceMappingErrorCode::PolicyConstraint
    );

    let mut overflow_payload = sample_rows().remove(0);
    overflow_payload["context"]["amount"] = json!("999999999999999999999999999999999.00");
    let overflow_mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload: overflow_payload,
            },
        )
        .expect("record maps"),
    ];
    let overflow_error = build_record_link_input_sidecar(&request(&digest), &overflow_mapped)
        .expect_err("overflow rejects");
    assert_eq!(
        overflow_error.code,
        SourceMappingErrorCode::PolicyConstraint
    );
}

#[test]
fn source_mapping_preserved_and_quarantined_cells_are_hash_bound() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);
    let mut payload = sample_rows().remove(0);
    payload["unmapped"] = json!({ "note": "private-preserved-value" });
    let mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload,
            },
        )
        .expect("record maps with preserved unknown field"),
    ];
    let sidecar = build_record_link_input_sidecar(&request(&digest), &mapped)
        .expect("record-link input builds with preserved source cell");
    assert_eq!(sidecar.summary["preserved_source_cell_count"], 1);
    assert_eq!(sidecar.summary["quarantined_source_cell_count"], 0);
    assert_eq!(sidecar.source_cell_dispositions[0].path, "unmapped.note");
    assert_eq!(
        sidecar.source_cell_dispositions[0].disposition,
        source_mapping::RecordLinkSourceCellDispositionKind::Preserved
    );
    assert!(
        sidecar.source_cell_dispositions[0]
            .value_hash
            .starts_with("blake3:")
    );
    let serialized = String::from_utf8(canonical_record_link_input_bytes(&sidecar).expect("bytes"))
        .expect("utf8");
    assert!(!serialized.contains("private-preserved-value"));

    let mut quarantine_package = sample_package();
    quarantine_package.profiles[0].policies.unknown_field = CapturePolicy::Quarantine;
    let quarantine_package = finalize_package(quarantine_package).expect("package finalizes");
    let quarantine_digest =
        source_mapping_package_digest(&quarantine_package).expect("source mapping digest");
    let quarantine_reference = profile_ref(&quarantine_digest);
    let mut quarantine_payload = sample_rows().remove(0);
    quarantine_payload["unmapped"] = json!({ "note": "private-quarantined-value" });
    let quarantine_mapped = vec![
        map_record(
            &quarantine_package,
            &quarantine_reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload: quarantine_payload,
            },
        )
        .expect("record maps with quarantined unknown field"),
    ];
    let quarantine_sidecar =
        build_record_link_input_sidecar(&request(&quarantine_digest), &quarantine_mapped)
            .expect("record-link input builds with quarantined source cell");
    assert_eq!(quarantine_sidecar.summary["preserved_source_cell_count"], 0);
    assert_eq!(
        quarantine_sidecar.summary["quarantined_source_cell_count"],
        1
    );
    assert_eq!(
        quarantine_sidecar.source_cell_dispositions[0].disposition,
        source_mapping::RecordLinkSourceCellDispositionKind::Quarantined
    );
    let serialized =
        String::from_utf8(canonical_record_link_input_bytes(&quarantine_sidecar).expect("bytes"))
            .expect("utf8");
    assert!(!serialized.contains("private-quarantined-value"));
}

#[test]
fn scale_mismatch_and_invalid_dates_refuse_without_inference() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);

    let mut scale_payload = sample_rows().remove(0);
    scale_payload["context"]["amount"] = json!("100.255");
    let scale_mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload: scale_payload,
            },
        )
        .expect("record maps"),
    ];
    let scale_error = build_record_link_input_sidecar(&request(&digest), &scale_mapped)
        .expect_err("extra fractional digits reject at declared scale");
    assert_eq!(scale_error.code, SourceMappingErrorCode::PolicyConstraint);

    let mut quarantine_request = request(&digest);
    quarantine_request
        .comparison_mappings
        .iter_mut()
        .find(|mapping| mapping.feature_id == "pkg.synthetic:amount")
        .expect("amount mapping")
        .policies
        .incomparable = CapturePolicy::Quarantine;
    let scale_sidecar = build_record_link_input_sidecar(&quarantine_request, &scale_mapped)
        .expect("scale mismatch can quarantine under declared policy");
    assert!(scale_sidecar.records.iter().all(|record| {
        record.quarantined_fields.iter().any(|field| {
            field.reason == RecordLinkFieldDispositionReason::IncomparableField
                && field.feature_id == "pkg.synthetic:amount"
        })
    }));

    let mut invalid_date_payload = sample_rows().remove(0);
    invalid_date_payload["context"]["effective_date"] = json!("2026-02-30");
    let invalid_date_mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload: invalid_date_payload,
            },
        )
        .expect("record maps"),
    ];
    let date_error = build_record_link_input_sidecar(&request(&digest), &invalid_date_mapped)
        .expect_err("calendar-invalid date rejects");
    assert_eq!(date_error.code, SourceMappingErrorCode::PolicyConstraint);
}

#[test]
fn non_numeric_comparison_mappings_refuse_units_and_scale_metadata() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);
    let mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload: sample_rows().remove(0),
            },
        )
        .expect("record maps"),
    ];

    let cases = [
        (
            "date with units",
            "pkg.synthetic:effective_date",
            Some("days"),
            None,
        ),
        (
            "date with scale",
            "pkg.synthetic:effective_date",
            None,
            Some(0),
        ),
        (
            "categorical with units",
            "pkg.synthetic:category",
            Some("class"),
            None,
        ),
        (
            "categorical with scale",
            "pkg.synthetic:category",
            None,
            Some(0),
        ),
    ];
    for (label, feature_id, units, scale) in cases {
        let mut request = request(&digest);
        let mapping = request
            .comparison_mappings
            .iter_mut()
            .find(|mapping| mapping.feature_id == feature_id)
            .expect("mapping exists");
        mapping.units = units.map(str::to_string);
        mapping.scale = scale;
        let error = build_record_link_input_sidecar(&request, &mapped)
            .expect_err("non-numeric units/scale must refuse before sidecar return");
        assert_eq!(
            error.code,
            SourceMappingErrorCode::ArtifactContract,
            "{label}"
        );
    }
}

#[test]
fn missing_requested_assignment_is_checked_per_source_row() {
    let mut package = sample_package();
    package.profiles[0].policies.missing_required = CapturePolicy::Quarantine;
    let package = finalize_package(package).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);
    let mut payload = sample_rows().remove(0);
    payload["assignments"]
        .as_object_mut()
        .expect("assignments object")
        .remove("secondary");
    let mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload,
            },
        )
        .expect("source mapping quarantines missing assignment field"),
    ];

    let error = build_record_link_input_sidecar(&request(&digest), &mapped)
        .expect_err("missing requested assignment rejects per row");
    assert_eq!(error.code, SourceMappingErrorCode::MissingField);

    let mut quarantine_request = request(&digest);
    quarantine_request.missing_assignment_policy = CapturePolicy::Quarantine;
    let sidecar = build_record_link_input_sidecar(&quarantine_request, &mapped)
        .expect("missing requested assignment can quarantine under declared policy");
    assert_eq!(sidecar.summary["record_count"], 1);
    assert_eq!(sidecar.summary["quarantined_record_count"], 1);
    assert_eq!(sidecar.summary["quarantined_source_cell_count"], 1);
    assert_eq!(
        sidecar.source_cell_dispositions[0].path,
        "assignments.secondary.name"
    );
    assert_eq!(
        sidecar.quarantined_records[0].reason,
        RecordLinkFieldDispositionReason::MissingField
    );
    assert_eq!(
        sidecar.quarantined_records[0]
            .missing_assignment_mapping_id
            .as_deref(),
        Some("pkg.synthetic:secondary_assignment")
    );
    assert!(sidecar.quarantined_records[0].assignment_ref.is_none());
    validate_record_link_input_sidecar(&sidecar).expect("sidecar validates");
}

#[test]
fn duplicate_request_mappings_refuse_instead_of_silent_dedup() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);
    let mapped = vec![
        map_record(
            &package,
            &reference,
            &SourceRecord {
                format: SourceFormat::Jsonl,
                payload: sample_rows().remove(0),
            },
        )
        .expect("record maps"),
    ];

    let mut duplicate_assignment = request(&digest);
    duplicate_assignment
        .assignment_mapping_ids
        .push("pkg.synthetic:primary_assignment".to_string());
    let error = build_record_link_input_sidecar(&duplicate_assignment, &mapped)
        .expect_err("duplicate assignment mapping refuses");
    assert_eq!(error.code, SourceMappingErrorCode::ArtifactContract);

    let mut duplicate_comparison = request(&digest);
    duplicate_comparison
        .comparison_mappings
        .push(duplicate_comparison.comparison_mappings[0].clone());
    let error = build_record_link_input_sidecar(&duplicate_comparison, &mapped)
        .expect_err("duplicate comparison mapping refuses");
    assert_eq!(error.code, SourceMappingErrorCode::ArtifactContract);
}

#[test]
fn quarantined_record_order_and_binding_are_validated() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let digest = source_mapping_package_digest(&package).expect("source mapping digest");
    let reference = profile_ref(&digest);
    let complete_mapped = map_record(
        &package,
        &reference,
        &SourceRecord {
            format: SourceFormat::Jsonl,
            payload: sample_rows().remove(0),
        },
    )
    .expect("complete record maps");
    let mut request = request(&digest);
    request.duplicate_record_policy = CapturePolicy::Quarantine;
    let mut sidecar =
        build_record_link_input_sidecar(&request, &[complete_mapped.clone(), complete_mapped])
            .expect("duplicate record quarantines");
    assert_eq!(sidecar.summary["quarantined_record_count"], 2);

    let mut out_of_order = sidecar.clone();
    out_of_order.quarantined_records.reverse();
    assert_ne!(
        out_of_order.quarantined_records,
        sidecar.quarantined_records
    );
    reseal_sidecar_for_test(&mut out_of_order);
    let error = validate_record_link_input_sidecar(&out_of_order)
        .expect_err("out-of-order quarantined records refuse");
    assert_eq!(error.code, SourceMappingErrorCode::ArtifactContract);

    sidecar.quarantined_records[0].source_ref.scope_id = "wrong_scope".to_string();
    reseal_sidecar_for_test(&mut sidecar);
    let error = validate_record_link_input_sidecar(&sidecar)
        .expect_err("quarantined record binding refuses");
    assert_eq!(error.code, SourceMappingErrorCode::ArtifactContract);
}

fn request(source_mapping_digest: &str) -> RecordLinkInputBuildRequest {
    RecordLinkInputBuildRequest {
        source_id: "synthetic_feed".to_string(),
        scope_id: "public_fixture".to_string(),
        profile_id: "pkg.synthetic:record_link".to_string(),
        profile_digest: hash_bytes(b"profile fixture"),
        input_digest: hash_bytes(b"input fixture"),
        source_mapping_digest: source_mapping_digest.to_string(),
        subject_observation_mapping_id: "pkg.synthetic:subject_surface".to_string(),
        assignment_mapping_ids: vec![
            "pkg.synthetic:primary_assignment".to_string(),
            "pkg.synthetic:secondary_assignment".to_string(),
        ],
        missing_assignment_policy: CapturePolicy::Reject,
        comparison_mappings: vec![
            RecordLinkComparisonMapping {
                feature_id: "pkg.synthetic:amount".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                path: "context.amount".to_string(),
                value_kind: RecordLinkComparisonKind::Numeric,
                units: Some("basis_points".to_string()),
                scale: Some(2),
                policies: RecordLinkComparisonPolicies::default(),
            },
            RecordLinkComparisonMapping {
                feature_id: "pkg.synthetic:effective_date".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                path: "context.effective_date".to_string(),
                value_kind: RecordLinkComparisonKind::Date,
                units: None,
                scale: None,
                policies: RecordLinkComparisonPolicies::default(),
            },
            RecordLinkComparisonMapping {
                feature_id: "pkg.synthetic:category".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                path: "context.category".to_string(),
                value_kind: RecordLinkComparisonKind::Categorical,
                units: None,
                scale: None,
                policies: RecordLinkComparisonPolicies::default(),
            },
        ],
        duplicate_record_policy: CapturePolicy::Reject,
    }
}

fn profile_ref(digest: &str) -> SourceMappingProfileRef {
    SourceMappingProfileRef {
        package_digest: digest.to_string(),
        profile_id: "pkg.synthetic:record_link".to_string(),
    }
}

fn sample_rows() -> Vec<Value> {
    vec![
        json!({
            "meta": { "record_id": "rec-1", "row": "rows/1", "as_of": "2026-03-31" },
            "subject": { "display_name": "Example Operating Unit", "public_anchor": "PUB-001" },
            "context": {
                "amount": "100.25",
                "effective_date": "2026-03-31",
                "category": "baseline"
            },
            "assignments": {
                "primary": { "name": "Example Assignment A", "public_ref": "ASG-A" },
                "secondary": { "name": "Example Assignment B", "public_ref": "ASG-B" }
            }
        }),
        json!({
            "meta": { "record_id": "rec-2", "row": "rows/2", "as_of": "2026-04-30" },
            "subject": { "display_name": "Example Operating Unit", "public_anchor": "PUB-001" },
            "context": {
                "amount": "101.00",
                "effective_date": "2026-04-30",
                "category": "refresh"
            },
            "assignments": {
                "primary": { "name": "Example Assignment C", "public_ref": "ASG-C" },
                "secondary": { "name": "Example Assignment D", "public_ref": "ASG-D" }
            }
        }),
    ]
}

fn sample_package() -> SourceMappingPackage {
    SourceMappingPackage {
        version: CANON_SOURCE_MAPPING_VERSION.to_string(),
        package_id: "pkg.synthetic".to_string(),
        package_version: "1.0.0".to_string(),
        profiles: vec![SourceMappingProfile {
            profile_id: "pkg.synthetic:record_link".to_string(),
            source_system: "synthetic_feed".to_string(),
            source_formats: vec![SourceFormat::Jsonl],
            object_id_path: "meta.record_id".to_string(),
            locator_path: "meta.row".to_string(),
            fragment_path: None,
            as_of_path: Some("meta.as_of".to_string()),
            valid_from_path: None,
            valid_to_path: None,
            observations: vec![ObservationMapping {
                mapping_id: "pkg.synthetic:subject_surface".to_string(),
                subject_type_id: "types.synthetic:subject".to_string(),
                surface_path: "subject.display_name".to_string(),
                anchor_mappings: vec![AnchorMapping {
                    namespace: "public_anchor".to_string(),
                    path: "subject.public_anchor".to_string(),
                }],
                context_paths: vec![
                    "context.amount".to_string(),
                    "context.effective_date".to_string(),
                    "context.category".to_string(),
                ],
            }],
            assignments: vec![
                AssignmentMapping {
                    mapping_id: "pkg.synthetic:primary_assignment".to_string(),
                    subject_type_id: "types.synthetic:subject".to_string(),
                    assignee_type_id: "types.synthetic:assignment".to_string(),
                    role_binding: RoleBinding::Literal {
                        role_id: "pkg.synthetic:primary".to_string(),
                    },
                    assignee_surface_path: "assignments.primary.name".to_string(),
                    assignee_anchor_mappings: vec![AnchorMapping {
                        namespace: "assignment_ref".to_string(),
                        path: "assignments.primary.public_ref".to_string(),
                    }],
                    context_paths: Vec::new(),
                },
                AssignmentMapping {
                    mapping_id: "pkg.synthetic:secondary_assignment".to_string(),
                    subject_type_id: "types.synthetic:subject".to_string(),
                    assignee_type_id: "types.synthetic:assignment".to_string(),
                    role_binding: RoleBinding::Literal {
                        role_id: "pkg.synthetic:secondary".to_string(),
                    },
                    assignee_surface_path: "assignments.secondary.name".to_string(),
                    assignee_anchor_mappings: vec![AnchorMapping {
                        namespace: "assignment_ref".to_string(),
                        path: "assignments.secondary.public_ref".to_string(),
                    }],
                    context_paths: Vec::new(),
                },
            ],
            relationships: Vec::new(),
            policies: Default::default(),
            documentation_refs: vec!["docs/record_link.md".to_string()],
        }],
        documentation: vec![SourceMappingDocumentationRef {
            label: "record-link contract".to_string(),
            uri: "docs/record_link.md".to_string(),
        }],
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn reseal_sidecar_for_test(sidecar: &mut source_mapping::RecordLinkInputSidecar) {
    let comparison_view_count = sidecar
        .records
        .iter()
        .map(|record| u64::try_from(record.comparison_views.len()).expect("view count fits"))
        .sum();
    let quarantined_field_count = sidecar
        .records
        .iter()
        .map(|record| u64::try_from(record.quarantined_fields.len()).expect("field count fits"))
        .sum();
    sidecar.summary = std::collections::BTreeMap::from([
        (
            "record_count".to_string(),
            u64::try_from(sidecar.records.len()).expect("record count fits"),
        ),
        ("comparison_view_count".to_string(), comparison_view_count),
        (
            "quarantined_field_count".to_string(),
            quarantined_field_count,
        ),
        (
            "quarantined_record_count".to_string(),
            u64::try_from(sidecar.quarantined_records.len()).expect("quarantine count fits"),
        ),
        (
            "preserved_source_cell_count".to_string(),
            u64::try_from(
                sidecar
                    .source_cell_dispositions
                    .iter()
                    .filter(|cell| {
                        cell.disposition
                            == source_mapping::RecordLinkSourceCellDispositionKind::Preserved
                    })
                    .count(),
            )
            .expect("source cell count fits"),
        ),
        (
            "quarantined_source_cell_count".to_string(),
            u64::try_from(
                sidecar
                    .source_cell_dispositions
                    .iter()
                    .filter(|cell| {
                        cell.disposition
                            == source_mapping::RecordLinkSourceCellDispositionKind::Quarantined
                    })
                    .count(),
            )
            .expect("source cell count fits"),
        ),
    ]);
    sidecar.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(sidecar).expect("sidecar serializes");
    sidecar.artifact_content_hash = hash_bytes(&bytes);
}
