#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/project/mod.rs"]
mod project;

use project::{
    ProjectManifestError, ProjectManifestErrorCode, ProjectModeKind, ProjectNetworkPolicy,
    canonical_project_manifest_bytes, load_project_manifest_toml, project_manifest_digest,
    project_manifest_projection, project_manifest_schema_version,
};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.project.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/project/manifest.rs");
const MINIMAL_TOML: &str = include_str!("./fixtures/project/minimal.toml");

#[test]
fn schema_declares_declarative_project_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], project_manifest_schema_version());
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        project_manifest_schema_version()
    );
    assert_eq!(schema["x-canon-contract"]["declarative_only"], true);
    assert_eq!(
        schema["x-canon-contract"]["paths_resolve_relative_to_manifest"],
        true
    );
    assert_eq!(schema["x-canon-contract"]["secret_handles_only"], true);
    assert_eq!(
        schema["x-canon-contract"]["unknown_extensions_supported"],
        true
    );
    assert_eq!(
        schema["properties"]["extensions"]["items"]["$ref"],
        "#/$defs/extension_activation"
    );
}

#[test]
fn minimal_fixture_validates_twice_with_stable_projection_and_digest() {
    let manifest_a = load_project_manifest_toml(MINIMAL_TOML).expect("minimal manifest loads");
    let manifest_b = load_project_manifest_toml(MINIMAL_TOML).expect("minimal manifest reloads");

    assert_eq!(
        canonical_project_manifest_bytes(&manifest_a).expect("canonical bytes a"),
        canonical_project_manifest_bytes(&manifest_b).expect("canonical bytes b")
    );
    assert_eq!(
        project_manifest_digest(&manifest_a).expect("digest a"),
        project_manifest_digest(&manifest_b).expect("digest b")
    );

    let env = BTreeMap::new();
    let projection_a = project_manifest_projection(
        &manifest_a,
        Path::new("tests/fixtures/project/minimal.toml"),
        &env,
    )
    .expect("projection a");
    let projection_b = project_manifest_projection(
        &manifest_b,
        Path::new("tests/fixtures/project/minimal.toml"),
        &env,
    )
    .expect("projection b");

    assert_eq!(
        serde_json::to_vec(&projection_a).expect("projection a serializes"),
        serde_json::to_vec(&projection_b).expect("projection b serializes")
    );
    assert_eq!(projection_a.project_id, "project.synthetic.alpha");
    assert_eq!(projection_a.sources.len(), 1);
    assert_eq!(projection_a.outputs.len(), 1);
    assert_eq!(projection_a.modes.len(), 1);
    assert_eq!(projection_a.modes[0].kind, ProjectModeKind::Cluster);
    assert!(projection_a.redacted_secrets.is_empty());
    assert!(
        projection_a.sources[0]
            .path
            .ends_with("tests/fixtures/project/input/minimal.csv")
    );
    assert!(
        projection_a.outputs[0]
            .path
            .ends_with("tests/fixtures/project/out/summary.json")
    );
    log_validation("minimal", "0");
}

#[test]
fn multi_source_link_unknown_extension_and_secret_redaction_validate() {
    let manifest = load_project_manifest_toml(&complete_manifest_toml())
        .expect("complete multi-source manifest loads");

    let env = BTreeMap::from([("REL_ROOT".to_string(), "feeds".to_string())]);
    let projection = project_manifest_projection(
        &manifest,
        Path::new("tests/fixtures/project/complete.toml"),
        &env,
    )
    .expect("projection resolves");

    assert_eq!(
        manifest.runtime.network_policy,
        ProjectNetworkPolicy::AllowDeclaredHosts
    );
    assert_eq!(projection.sources.len(), 2);
    assert_eq!(projection.outputs.len(), 2);
    assert_eq!(projection.modes.len(), 2);
    assert!(
        projection
            .modes
            .iter()
            .any(|mode| mode.kind == ProjectModeKind::Link && mode.source_ids.len() == 2)
    );
    assert_eq!(projection.extensions.len(), 1);
    assert_eq!(projection.extensions[0].extension_id, "portable_export");
    assert_eq!(
        projection.extensions[0].config_path.as_deref(),
        Some("tests/fixtures/project/extensions/export.json")
    );
    assert_eq!(projection.redacted_secrets.len(), 1);
    assert_eq!(projection.redacted_secrets[0].handle, "env:[redacted]");
    assert!(
        projection.sources[1]
            .path
            .ends_with("tests/fixtures/project/feeds/right.jsonl")
    );
    log_validation("complete_multi_source", "0");
}

#[test]
fn unknown_field_and_future_version_refuse_stably() {
    let unknown_field = replace_once(
        MINIMAL_TOML,
        "project_id = \"project.synthetic.alpha\"\n",
        "project_id = \"project.synthetic.alpha\"\nunexpected = \"nope\"\n",
    );
    let bytes_first = parse_error_bytes(&unknown_field);
    let bytes_second = parse_error_bytes(&unknown_field);
    assert_eq!(bytes_first, bytes_second);
    let error: ProjectManifestError =
        serde_json::from_slice(&bytes_first).expect("error bytes deserialize");
    assert_eq!(error.code, ProjectManifestErrorCode::UnknownField);
    log_validation("unknown_field", "unknown_field");

    let future_version = replace_once(
        MINIMAL_TOML,
        "schema_version = \"canon.project.v1\"",
        "schema_version = \"canon.project.v2\"",
    );
    let bytes_first = parse_error_bytes(&future_version);
    let bytes_second = parse_error_bytes(&future_version);
    assert_eq!(bytes_first, bytes_second);
    let error: ProjectManifestError =
        serde_json::from_slice(&bytes_first).expect("error bytes deserialize");
    assert_eq!(error.code, ProjectManifestErrorCode::CompatibilityPolicy);
    log_validation("future_version", "compatibility_policy");
}

#[test]
fn path_traversal_secret_leak_and_incompatible_mode_refuse_stably() {
    let path_traversal = replace_once(
        MINIMAL_TOML,
        "path = \"input/minimal.csv\"",
        "path = \"../escape.csv\"",
    );
    let error = projection_error(
        &path_traversal,
        Path::new("tests/fixtures/project/minimal.toml"),
    );
    assert_eq!(error.code, ProjectManifestErrorCode::PathPolicy);
    assert_eq!(
        serde_json::to_vec(&error).expect("serialize a"),
        serde_json::to_vec(&projection_error(
            &path_traversal,
            Path::new("tests/fixtures/project/minimal.toml")
        ))
        .expect("serialize b")
    );
    log_validation("path_traversal", "path_policy");

    let secret_leak = format!(
        "{MINIMAL_TOML}\n[[secrets]]\nname = \"api_token\"\nhandle = \"sk_live_not_allowed\"\npurpose = \"network auth\"\n"
    );
    let error = parse_error(&secret_leak);
    assert_eq!(error.code, ProjectManifestErrorCode::SecretPolicy);
    assert_eq!(
        serde_json::to_vec(&error).expect("serialize a"),
        serde_json::to_vec(&parse_error(&secret_leak)).expect("serialize b")
    );
    log_validation("secret_leak", "secret_policy");

    let incompatible_link = replace_once(MINIMAL_TOML, "kind = \"cluster\"", "kind = \"link\"");
    let error = parse_error(&incompatible_link);
    assert_eq!(error.code, ProjectManifestErrorCode::CompatibilityPolicy);
    log_validation("incompatible_mode", "compatibility_policy");
}

#[test]
fn source_scan_keeps_project_contract_domain_neutral() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "servicer", "tranche", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "project manifest module should remain domain-neutral: {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "project manifest schema should remain domain-neutral: {banned}"
        );
    }
}

fn complete_manifest_toml() -> String {
    r#"schema_version = "canon.project.v1"
project_id = "project.synthetic.beta"

[review]
cannot_link_max_score_basis_points = 2500
review_required_min_score_basis_points = 7200
auto_promote_min_score_basis_points = 9600

[temporal]
valid_at = "2026-07-10T12:00:00Z"
known_as_of = "2026-07-09T18:00:00Z"
scope_ref = "pkg.synthetic:global_scope"

[budgets]
max_input_bytes = 2097152
max_rows = 75000
max_candidates = 8000
max_review_items = 2000
max_runtime_seconds = 1200

[runtime]
offline_build_only = false
network_policy = "allow_declared_hosts"
declared_hosts = ["api.example.test"]

[[packages]]
alias = "registry"
kind = "registry_package"
id = "pkg.synthetic.registry"
version = "1.2.0"
content_hash = "blake3:1111111111111111111111111111111111111111111111111111111111111111"

[[packages]]
alias = "strategy"
kind = "strategy_package"
id = "pkg.synthetic.strategy"
version = "1.2.0"
content_hash = "blake3:2222222222222222222222222222222222222222222222222222222222222222"

[[packages]]
alias = "profile"
kind = "entity_profile_package"
id = "pkg.synthetic.profile"
version = "1.2.0"
content_hash = "blake3:3333333333333333333333333333333333333333333333333333333333333333"

[[packages]]
alias = "mapping"
kind = "source_mapping_package"
id = "pkg.synthetic.mapping"
version = "1.2.0"
content_hash = "blake3:4444444444444444444444444444444444444444444444444444444444444444"

[[packages]]
alias = "extension"
kind = "extension_package"
id = "pkg.synthetic.extension"
version = "1.2.0"
content_hash = "blake3:5555555555555555555555555555555555555555555555555555555555555555"

[[sources]]
source_id = "left_feed"
path = "input/left.csv"
format = "csv"
mapping_package = "mapping"
mapping_profile = "pkg.synthetic:left_records"
required = true

[[sources]]
source_id = "right_feed"
path = "${REL_ROOT}/right.jsonl"
format = "jsonl"
mapping_package = "mapping"
mapping_profile = "pkg.synthetic:right_records"
required = false

[[outputs]]
output_id = "summary"
kind = "summary_json"
path = "out/summary.json"
redact_identity = false

[[outputs]]
output_id = "review_queue"
kind = "review_queue_csv"
path = "out/review.csv"
redact_identity = true

[[modes]]
mode_id = "cluster_primary"
kind = "cluster"
source_ids = ["left_feed"]
registry_package = "registry"
strategy_package = "strategy"
profile_package = "profile"
output_ids = ["summary"]

[[modes]]
mode_id = "link_cross"
kind = "link"
source_ids = ["left_feed", "right_feed"]
registry_package = "registry"
strategy_package = "strategy"
profile_package = "profile"
output_ids = ["summary", "review_queue"]

[[secrets]]
name = "service_token"
handle = "env:CANON_SERVICE_TOKEN"
purpose = "declared network auth"

[[extensions]]
extension_id = "portable_export"
package = "extension"
entrypoint = "pkg.synthetic.extension:emit"
mode_ids = ["link_cross"]
config_path = "extensions/export.json"
"#
    .to_string()
}

fn parse_error_bytes(input: &str) -> Vec<u8> {
    serde_json::to_vec(&parse_error(input)).expect("error serializes")
}

fn parse_error(input: &str) -> ProjectManifestError {
    load_project_manifest_toml(input).expect_err("manifest should refuse")
}

fn projection_error(input: &str, manifest_path: &Path) -> ProjectManifestError {
    let manifest = load_project_manifest_toml(input).expect("manifest parses before projection");
    project_manifest_projection(&manifest, manifest_path, &BTreeMap::new())
        .expect_err("projection should refuse")
}

fn replace_once(haystack: &str, needle: &str, replacement: &str) -> String {
    haystack.replacen(needle, replacement, 1)
}

fn log_validation(fixture: &str, code: &str) {
    eprintln!(
        "fixture={fixture} code={code} schema_hash=blake3:{}",
        blake3::hash(SCHEMA_JSON.as_bytes()).to_hex()
    );
}
