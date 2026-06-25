use canon::{
    Outcome,
    entity::error::{EntityRefusalKind, entity_refusal},
};
use serde_json::{Value, json};

#[test]
fn entity_refusal_codes_serialize_to_stable_strings() {
    let expected = [
        "E_ENTITY_PROFILE",
        "E_ENTITY_STRATEGY",
        "E_ENTITY_INPUT_CONTRACT",
        "E_ENTITY_SURFACE_ID_COLLISION",
        "E_ENTITY_PATCH_CONFLICT",
        "E_ENTITY_REGISTRY_SNAPSHOT",
        "E_ENTITY_CACHE_MISMATCH",
        "E_ENTITY_INDEX_LIMIT",
        "E_ENTITY_CANDIDATE_BUDGET",
        "E_ENTITY_ARTIFACT_CONTRACT",
        "E_ENTITY_CANNOT_LINK_OVERRIDE",
        "E_ENTITY_REVIEW_IMPORT",
        "E_ENTITY_AUDIT_GATE",
        "E_ENTITY_APPLY_UNRESOLVED",
        "E_ENTITY_IO_BUDGET",
    ];

    let actual: Vec<_> = EntityRefusalKind::all()
        .iter()
        .map(|kind| kind.code_str())
        .collect();
    assert_eq!(actual, expected);

    for kind in EntityRefusalKind::all() {
        let serialized = serde_json::to_value(kind.refusal_code()).unwrap();
        assert_eq!(serialized, json!(kind.code_str()));
    }
}

#[test]
fn entity_refusal_envelope_uses_normal_canon_refusal_shape() {
    for kind in EntityRefusalKind::all() {
        let refusal = entity_refusal(
            *kind,
            format!("{} contract failed", kind.stage_hint()),
            json!({
                "stage": kind.stage_hint(),
                "expected": "matching profile, strategy, registry, and artifact hashes",
                "actual": "mismatch"
            }),
        );
        let output = refusal.to_canon_output();

        assert_eq!(output.version, "canon.v0");
        assert_eq!(output.outcome, Outcome::Refusal);
        assert!(output.registry.is_none());
        assert!(output.summary.is_none());
        assert!(output.mappings.is_empty());
        assert!(output.unresolved.is_empty());

        let refusal = output.refusal.expect("refusal envelope");
        assert_eq!(
            serde_json::to_value(refusal.code).unwrap(),
            json!(kind.code_str())
        );
        assert!(refusal.message.contains("contract failed"));
        assert_eq!(refusal.detail["stage"], kind.stage_hint());
        assert_non_empty_string(&refusal.detail["expected"]);
        assert_non_empty_string(&refusal.detail["actual"]);
        assert_non_empty_string(&json!(refusal.next_command));
    }
}

#[test]
fn entity_refusal_can_override_next_command() {
    let refusal = EntityRefusalKind::CandidateBudget.to_refusal(
        "Candidate budget exceeded before bounded emission",
        json!({
            "stage": "block",
            "limit": "max_candidates_per_surface",
            "configured": 25,
            "observed": 64
        }),
        Some("canon entity block <PREPARE_DIR> --strategy <STRATEGY.yaml> --max-candidates-per-surface 64".to_string()),
    );

    assert_eq!(
        serde_json::to_value(refusal.code).unwrap(),
        json!("E_ENTITY_CANDIDATE_BUDGET")
    );
    assert_eq!(refusal.detail["observed"], 64);
    assert!(
        refusal
            .next_command
            .as_deref()
            .unwrap()
            .contains("--max-candidates-per-surface 64")
    );
}

fn assert_non_empty_string(value: &Value) {
    assert!(
        value.as_str().is_some_and(|text| !text.trim().is_empty()),
        "expected non-empty string, got {value:?}"
    );
}
