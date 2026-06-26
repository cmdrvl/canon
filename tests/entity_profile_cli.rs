#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::entity::profile::EntityProfileDocument;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

fn canon_command() -> Command {
    assert_cmd::cargo::cargo_bin_cmd!("canon")
}

#[test]
fn entity_profile_cli_lists_robot_friendly_templates() {
    let output = canon_command()
        .args(["entity", "profile", "list", "--emit", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let catalog: Value = serde_json::from_slice(&output).expect("catalog json");

    assert_eq!(catalog["version"], "canon_entity_profile_templates.v0");
    let profiles = catalog["profiles"].as_array().expect("profiles array");
    assert_eq!(profiles.len(), 2);

    let cmbs = profiles
        .iter()
        .find(|profile| profile["profile"] == "cmbs_tenant_label")
        .expect("cmbs template listed");
    assert_eq!(cmbs["identity_semantics"], "canonical_display_label");
    assert_eq!(cmbs["canonical_type"], "tenant_label");
    assert_eq!(
        cmbs["init_command"],
        "canon entity profile init cmbs_tenant_label --output cmbs_tenant_label.yaml"
    );
    assert!(
        cmbs["non_goals"]
            .as_array()
            .unwrap()
            .contains(&Value::String(
                "does_not_claim_legal_entity_identity".to_string()
            ))
    );

    let regab = profiles
        .iter()
        .find(|profile| profile["profile"] == "regab_firm_identity")
        .expect("regab template listed");
    assert_eq!(regab["identity_semantics"], "same_firm_or_reviewed_alias");
    assert_eq!(regab["canonical_type"], "org");
}

#[test]
fn entity_profile_cli_init_writes_valid_commented_templates() {
    let temp = tempdir().expect("tempdir");
    for (profile_id, expected_semantics, required_comment) in [
        (
            "cmbs_tenant_label",
            "canonical_display_label",
            "does not claim legal-entity",
        ),
        (
            "regab_firm_identity",
            "same_firm_or_reviewed_alias",
            "does not collapse parent/subsidiary",
        ),
    ] {
        let output_path = temp.path().join(format!("{profile_id}.yaml"));
        let output = canon_command()
            .arg("entity")
            .arg("profile")
            .arg("init")
            .arg(profile_id)
            .arg("--output")
            .arg(&output_path)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let report: Value = serde_json::from_slice(&output).expect("init json");

        assert_eq!(report["profile"], profile_id);
        assert_eq!(report["output"], output_path.display().to_string());
        assert_eq!(report["template_valid"], true);
        assert!(
            report["next_command"]
                .as_str()
                .unwrap()
                .contains("canon entity prepare")
        );

        let yaml = fs::read_to_string(&output_path).expect("template written");
        assert!(yaml.contains(required_comment), "{profile_id}");
        let profile =
            EntityProfileDocument::from_yaml_str(&yaml).expect("initialized template validates");
        assert_eq!(profile.profile, profile_id);
        assert_eq!(profile.identity_semantics, expected_semantics);
        assert!(profile.to_reference().is_complete());
    }
}

#[test]
fn entity_profile_cli_unknown_template_refuses_with_recovery() {
    let output = canon_command()
        .args([
            "entity",
            "profile",
            "init",
            "legal_entity_identity",
            "--output",
            "/tmp/unused-entity-profile.yaml",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&output).expect("refusal json");

    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_PROFILE");
    assert_eq!(
        refusal["refusal"]["detail"]["available_profiles"],
        serde_json::json!(["cmbs_tenant_label", "regab_firm_identity"])
    );
    assert_eq!(
        refusal["refusal"]["next_command"],
        "canon entity profile list --emit json"
    );
}
