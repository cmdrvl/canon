//! Agent-discoverable registry build provider catalog and schemas.
//!
//! These tests exercise `canon registry providers` and
//! `canon registry provider-schema`, and guard the embedded operator.json
//! provider catalog against drift from the live CLI output. They never contact
//! api.openfigi.com: the schema/catalog surfaces are deterministic and offline.

use assert_cmd::Command;
use serde_json::Value;

/// The compiled operator manifest, the same bytes `canon --describe` emits.
const OPERATOR_JSON: &str = include_str!("../operator.json");

fn run_json(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .unwrap();
    let code = output.status.code().unwrap();
    let value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    (code, value)
}

#[test]
fn registry_providers_lists_implemented_providers() {
    let (code, value) = run_json(&["registry", "providers", "--emit", "json"]);
    assert_eq!(code, 0);
    assert_eq!(value["version"], "canon_registry_providers.v0");
    let ids: Vec<&str> = value["providers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"mock"));
    assert!(ids.contains(&"openfigi"));
}

#[test]
fn operator_json_provider_catalog_matches_cli() {
    // Drift guard: the static manifest's `providers` must equal the live catalog
    // the CLI emits, so agents reading either surface get identical answers.
    let (_, cli) = run_json(&["registry", "providers", "--emit", "json"]);
    let manifest: Value = serde_json::from_str(OPERATOR_JSON).unwrap();
    assert_eq!(
        manifest["providers"], cli["providers"],
        "operator.json providers catalog drifted from `canon registry providers`"
    );
}

#[test]
fn operator_json_declares_provider_discovery_subcommands() {
    let manifest: Value = serde_json::from_str(OPERATOR_JSON).unwrap();
    let names: Vec<&str> = manifest["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"registry providers"));
    assert!(names.contains(&"registry provider-schema"));
}

#[test]
fn provider_schema_openfigi_publishes_the_full_contract() {
    let (code, schema) = run_json(&["registry", "provider-schema", "openfigi", "--emit", "json"]);
    assert_eq!(code, 0);
    assert_eq!(schema["version"], "canon_registry_provider_schema.v0");
    assert_eq!(schema["id"], "openfigi");

    // id types + inference
    let id_types: Vec<&str> = schema["id_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(id_types, ["ID_CUSIP", "ID_ISIN", "ID_SEDOL"]);
    assert_eq!(schema["id_type_inference"]["cusip"], "ID_CUSIP");

    let options = schema["options"].as_array().unwrap();
    let find = |key: &str| {
        options
            .iter()
            .find(|o| o["key"] == key)
            .unwrap_or_else(|| panic!("option {key} present"))
    };

    // secret flag + env fallback on api_key
    let api_key = find("api_key");
    assert_eq!(api_key["secret"], true);
    assert_eq!(api_key["env_fallback"], "OPENFIGI_API_KEY");

    // interval filter typing
    assert_eq!(find("coupon")["type"], "numeric_interval");
    assert_eq!(find("maturity")["type"], "date_interval");
    assert_eq!(find("includeUnlistedEquities")["type"], "bool");
    assert_eq!(find("exchCode")["type"], "string");

    // mutual exclusion + interval encoding rule are discoverable
    let exclusions = schema["mutual_exclusions"].as_array().unwrap();
    assert!(exclusions.iter().any(|pair| {
        let set: Vec<&str> = pair
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        set.contains(&"exchCode") && set.contains(&"micCode")
    }));
    assert!(schema["interval_encoding"].is_string());
}

#[test]
fn provider_schema_unknown_provider_refuses_with_recovery_path() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["registry", "provider-schema", "bogus", "--emit", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["outcome"], "REFUSAL");
    assert_eq!(value["refusal"]["code"], "E_PARSE");
    // The recovery path points the agent at the discovery command.
    assert_eq!(
        value["refusal"]["next_command"],
        "canon registry providers --emit json"
    );
    let available: Vec<&str> = value["refusal"]["detail"]["available_providers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(available.contains(&"openfigi"));
}

#[test]
fn provider_schema_summary_is_human_readable() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "provider-schema",
            "openfigi",
            "--emit",
            "summary",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("provider-config options:"));
    assert!(text.contains("api_key (string, secret, env OPENFIGI_API_KEY)"));
    assert!(text.contains("mutually exclusive: exchCode | micCode"));
}
