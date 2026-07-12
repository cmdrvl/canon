use canon::{
    Outcome,
    entity::error::{EntityRefusalKind, entity_refusal},
};
use serde_json::{Value, json};
use std::{fs, process::Command};

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

#[test]
fn artifact_backed_entity_run_refuses_dense_candidate_expansion_before_emission() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("dense_org_mentions.csv");
    let mut csv = String::from(
        "source_row_id,record_id,dataset,record_version,field_name,org_name,doc_id,as_of_date,filing_cik,accession,filing_form,filed_date,period,source_exhibit_document_name,source_exhibit_type,source_item,role_context,capacity,capacity_normalized,reporting_party_capacity,platform_capacity,platform_capacity_normalized,subject_role,deal_key,transaction_name,alias_surfaces_json,mention_surfaces_json\n",
    );
    let groups = [
        "aurora",
        "borealis",
        "cascade",
        "driftwood",
        "emberline",
        "frostline",
        "granite",
        "harborview",
        "ironwood",
        "juniper",
        "keystone",
    ];
    for (group_index, group) in groups.iter().enumerate() {
        for member in 0..100 {
            let index = group_index * 100 + member;
            csv.push_str(&format!(
                "row-{index:04},record-{index:04},regab_servicer_schedules,sec10d.regab_servicer_schedule.v0,servicer_name,Budget {group} Shared Servicing Firm {member:03} LLC,doc-1,2026-03-31,0000000000,0000000000-26-000001,10-K,2026-03-31,2025-12-31,fixture.htm,EX-35,1123,servicer_name:servicer,Servicer,servicer,master servicer,,,,DEAL-1,Deal 1,[],[]\n"
            ));
        }
    }
    fs::write(&rows, csv).expect("dense rows fixture");
    let work_dir = temp.path().join("entity-work");

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "run",
            rows.to_str().expect("rows path"),
            "--profile",
            "regab_firm_identity",
            "--strategy",
            "tests/fixtures/entity/strategies/regab_firm_identity.yaml",
            "--registry",
            "tests/fixtures/entity/regab/sec10d_baseline_public/registry_snapshot/firms",
            "--work-dir",
            work_dir.to_str().expect("work dir path"),
            "--emit",
            "json",
            "--no-witness",
        ])
        .output()
        .expect("canon entity run executes");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("refusal JSON");
    assert_eq!(value["outcome"], "REFUSAL");
    assert_eq!(value["refusal"]["code"], "E_ENTITY_CANDIDATE_BUDGET");
    assert_eq!(value["refusal"]["detail"]["stage"], "block");
    assert_eq!(
        value["refusal"]["detail"]["reason"],
        "candidate_budget_exceeded"
    );
    assert_non_empty_string(&value["refusal"]["detail"]["policy_id"]);
    assert_eq!(
        value["refusal"]["detail"]["policy_id"],
        value["refusal"]["detail"]["budget"]["policy_id"]
    );
    let observed = value["refusal"]["detail"]["observed"]
        .as_u64()
        .expect("observed count");
    let configured = value["refusal"]["detail"]["configured"]
        .as_u64()
        .expect("configured budget");
    assert!(
        observed > configured,
        "expected native budget breach, observed={observed}, configured={configured}"
    );
    assert_eq!(
        value["refusal"]["detail"]["budget"]["observed"],
        json!(observed)
    );
    assert_eq!(
        value["refusal"]["detail"]["budget"]["configured"],
        json!(configured)
    );
    assert_eq!(
        value["refusal"]["detail"]["candidate_artifact_written"],
        json!(false)
    );
    assert_eq!(
        value["refusal"]["detail"]["partial_candidate_artifact_written"],
        json!(false)
    );
    assert!(!work_dir.join("block/candidates.jsonl").exists());
    assert!(!work_dir.join("block/block.json").exists());
}

fn assert_non_empty_string(value: &Value) {
    assert!(
        value.as_str().is_some_and(|text| !text.trim().is_empty()),
        "expected non-empty string, got {value:?}"
    );
}
