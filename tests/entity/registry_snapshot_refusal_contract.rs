#![forbid(unsafe_code)]

use canon::entity::ENTITY_REFUSAL_CODES;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const MATRIX: &str =
    include_str!("../fixtures/entity/promote/refusals/registry_snapshot_matrix.json");

#[derive(Debug, Deserialize)]
struct RefusalMatrix {
    version: String,
    scope: String,
    cases: Vec<RefusalCase>,
}

#[derive(Debug, Deserialize)]
struct RefusalCase {
    id: String,
    stage: String,
    downstream_contract: String,
    refusal_code: String,
    trigger: String,
    required_detail_fields: Vec<String>,
    required_hash_fields: Vec<String>,
    protected_artifacts: Vec<String>,
    writes_performed: bool,
}

#[test]
fn registry_snapshot_refusal_contract_names_required_matrix_cases() {
    let matrix = matrix();

    assert_eq!(
        matrix.version,
        "canon_entity_registry_snapshot_refusal_matrix.v0"
    );
    assert_eq!(matrix.scope, "ENT-P09 audit/promote/apply");

    let ids = matrix
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids,
        BTreeSet::from([
            "apply_stale_registry_snapshot",
            "apply_unresolved_full_resolution",
            "audit_stale_registry_snapshot",
            "partial_write_preflight_guard",
            "promote_failed_audit_gate",
            "promote_stale_audit_artifact",
            "promote_stale_registry_snapshot",
            "promote_stale_review_import_ledger",
        ])
    );
    assert_eq!(ids.len(), matrix.cases.len(), "case ids must be unique");
}

#[test]
fn registry_snapshot_refusal_contract_locks_codes_and_required_fields() {
    let refusal_codes = ENTITY_REFUSAL_CODES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let matrix = matrix();
    let by_id = matrix
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();

    for (id, expected_code) in [
        (
            "audit_stale_registry_snapshot",
            "E_ENTITY_REGISTRY_SNAPSHOT",
        ),
        ("promote_stale_audit_artifact", "E_ENTITY_AUDIT_GATE"),
        ("promote_failed_audit_gate", "E_ENTITY_AUDIT_GATE"),
        (
            "promote_stale_review_import_ledger",
            "E_ENTITY_ARTIFACT_CONTRACT",
        ),
        (
            "promote_stale_registry_snapshot",
            "E_ENTITY_REGISTRY_SNAPSHOT",
        ),
        (
            "apply_stale_registry_snapshot",
            "E_ENTITY_REGISTRY_SNAPSHOT",
        ),
        (
            "apply_unresolved_full_resolution",
            "E_ENTITY_APPLY_UNRESOLVED",
        ),
        (
            "partial_write_preflight_guard",
            "E_ENTITY_ARTIFACT_CONTRACT",
        ),
    ] {
        let case = by_id.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(case.refusal_code, expected_code);
        assert!(
            refusal_codes.contains(case.refusal_code.as_str()),
            "unknown refusal code for {id}"
        );
        assert_non_empty(&case.stage, id, "stage");
        assert_non_empty(&case.downstream_contract, id, "downstream_contract");
        assert_non_empty(&case.trigger, id, "trigger");
        assert!(
            case.required_detail_fields
                .iter()
                .any(|field| field == "writes_performed"),
            "{id} must require writes_performed detail"
        );
        assert!(
            !case.writes_performed,
            "{id} must refuse before mutating protected artifacts"
        );
        assert!(
            !case.protected_artifacts.is_empty(),
            "{id} must name protected artifacts"
        );

        for hash_field in &case.required_hash_fields {
            assert!(
                hash_field.ends_with("_hash"),
                "{id} hash field must be explicit: {hash_field}"
            );
            assert!(
                case.required_detail_fields.contains(hash_field),
                "{id} required hash field {hash_field} must also be a detail field"
            );
        }
    }
}

#[test]
fn registry_snapshot_refusal_contract_requires_expected_actual_snapshot_fields() {
    let matrix = matrix();
    for case in matrix
        .cases
        .iter()
        .filter(|case| case.id.contains("stale_registry_snapshot"))
    {
        for field in [
            "expected_registry_snapshot_hash",
            "actual_registry_snapshot_hash",
        ] {
            assert!(
                case.required_detail_fields
                    .iter()
                    .any(|value| value == field),
                "{} must require {field}",
                case.id
            );
            assert!(
                case.required_hash_fields.iter().any(|value| value == field),
                "{} must mark {field} as a required hash",
                case.id
            );
        }
    }
}

fn matrix() -> RefusalMatrix {
    serde_json::from_str(MATRIX).expect("registry snapshot refusal matrix fixture parses")
}

fn assert_non_empty(value: &str, case_id: &str, field: &str) {
    assert!(
        !value.trim().is_empty(),
        "{case_id} must include non-empty {field}"
    );
}
