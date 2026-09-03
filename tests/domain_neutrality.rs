#![forbid(unsafe_code)]

use canon::entity::profile_package::{
    ENTITY_PROFILE_CANONICAL_SURFACE_ROLE, EntityProfileExecutionRecord,
    EntityProfileExecutionRequest, EntityProfilePackageExecution, EntityProfilePackageRunRequest,
    EntityProfileRecordInputFormat, ProfileCapability, ProfileErrorCode, ProfileModeKind,
    build_project_lock_view, entity_profile_package_digest, execute_profile_package_from_paths,
    execute_profile_package_records, load_profile_package_bytes, load_profile_package_file,
};
use canon::extensions::{
    self, FORBIDDEN_EXTENSION_DOC_REFERENCES, REQUIRED_NEUTRAL_DOC_REFERENCES,
    render_doc_scan_report, render_source_scan_report, scan_domain_neutral_extension_sources,
    scan_extension_docs, scan_stripped_rust_source,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ALPHA_PACKAGE: &str =
    "tests/fixtures/extensions/domain-neutrality/alpha/profile_package.json";
const ALPHA_RECORDS: &str = "tests/fixtures/extensions/domain-neutrality/alpha/records.csv";
const ALPHA_EXPECTED: &str =
    "tests/fixtures/extensions/domain-neutrality/alpha/expected_execution.json";
const BETA_PACKAGE: &str = "tests/fixtures/extensions/domain-neutrality/beta/profile_package.json";
const BETA_RECORDS: &str = "tests/fixtures/extensions/domain-neutrality/beta/records.jsonl";
const BETA_EXPECTED: &str =
    "tests/fixtures/extensions/domain-neutrality/beta/expected_execution.json";
const CMBS_EXAMPLE_PACKAGE: &str =
    "examples/entity-profiles/cmbs-tenant-label/profile-package.json";
const CMBS_EXAMPLE_RECORDS: &str = "tests/fixtures/entity/cmbs/apply_loop/rows.csv";
const REGAB_EXAMPLE_PACKAGE: &str =
    "examples/entity-profiles/regab-firm-identity/profile-package.json";
const REGAB_EXAMPLE_RECORDS: &str = "tests/fixtures/entity/regab/org_mentions/org_mentions.csv";

const EXTENSIONS_DOCS: &str = include_str!("../docs/EXTENSIONS.md");

#[test]
fn extension_runtime_sources_stay_domain_neutral() {
    let violations = scan_domain_neutral_extension_sources(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("extension runtime source scan should read files");
    eprintln!(
        "checked {} extension runtime source files",
        extensions::DOMAIN_NEUTRAL_EXTENSION_SOURCE_FILES.len()
    );
    assert!(
        violations.is_empty(),
        "domain-neutral runtime scan found leaks:\n{}",
        render_source_scan_report(&violations)
    );
}

#[test]
fn source_scan_flags_runtime_leak_but_ignores_comments() {
    let source = r#"
// loan in a comment must not fail the runtime scanner
/* regab inside a block comment is also ignored */
fn neutral_contract() {
    let stage = "loan";
    let provider = "openfigi";
}
"#;
    let violations = scan_stripped_rust_source(
        "synthetic_runtime_fixture",
        "tests/domain_neutrality.rs.fixture",
        source,
        extensions::FORBIDDEN_EXTENSION_RUNTIME_TERMS,
    );
    let seen_terms = violations
        .iter()
        .map(|violation| violation.term.as_str())
        .collect::<Vec<_>>();
    assert_eq!(seen_terms, vec!["loan", "openfigi"]);
    assert!(
        violations.iter().all(|violation| violation.line >= 4),
        "comment-only lines should not be flagged:\n{}",
        render_source_scan_report(&violations)
    );
}

#[test]
fn extensions_docs_stay_neutral_and_use_synthetic_examples() {
    let violations = scan_extension_docs(
        "docs/EXTENSIONS.md",
        EXTENSIONS_DOCS,
        FORBIDDEN_EXTENSION_DOC_REFERENCES,
    );
    eprintln!(
        "checked docs/EXTENSIONS.md for {} forbidden references",
        FORBIDDEN_EXTENSION_DOC_REFERENCES.len()
    );
    assert!(
        violations.is_empty(),
        "extension docs mention shipped-domain assets or vocabulary:\n{}",
        render_doc_scan_report(&violations)
    );
    for required in REQUIRED_NEUTRAL_DOC_REFERENCES {
        assert!(
            EXTENSIONS_DOCS.contains(required),
            "extension docs should include synthetic neutral example {required}"
        );
    }
}

#[test]
fn external_profile_packages_load_without_code_changes() {
    for case in external_package_execution_cases() {
        let package =
            load_profile_package_file(&fixture_path(case.package)).expect("package loads");
        let digest = entity_profile_package_digest(&package).expect("package digest");
        let execution = execute_profile_package_from_paths(
            &fixture_path(case.package),
            &fixture_path(case.records),
            &(case.request)(Some(digest.clone())),
        )
        .unwrap_or_else(|error| panic!("{} package execution should pass: {error:?}", case.label));

        assert_eq!(execution.profile, case.expected_profile, "{}", case.label);
        assert_eq!(execution.package_digest, digest, "{}", case.label);
        assert_eq!(
            execution.plan.mode.source_object_type, case.expected_entity_type,
            "{}",
            case.label
        );
        assert_eq!(
            execution.canonical_view, case.expected_canonical_view,
            "{}",
            case.label
        );
        let canonical_surfaces = execution
            .records
            .iter()
            .map(|record| record.canonical_surface.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            canonical_surfaces.as_slice(),
            case.expected_canonical_surfaces,
            "{}",
            case.label
        );
        let record_keys = execution
            .records
            .iter()
            .map(|record| record.record_key.as_deref().expect("record key"))
            .collect::<Vec<_>>();
        assert_eq!(
            record_keys.as_slice(),
            case.expected_record_keys,
            "{}",
            case.label
        );
        if let Some(extra_view) = case.expected_extra_view {
            let extra_view_values = execution
                .records
                .iter()
                .map(|record| {
                    record
                        .normalized_views
                        .get(extra_view.name)
                        .map(String::as_str)
                        .expect("extra normalized view")
                })
                .collect::<Vec<_>>();
            assert_eq!(
                extra_view_values.as_slice(),
                extra_view.expected_values,
                "{}",
                case.label
            );
        }

        let lock = build_project_lock_view(&package, &[]).expect("lock view should build");
        assert_eq!(lock.profile, case.expected_profile, "{}", case.label);
        assert_eq!(
            lock.entity_type, case.expected_entity_type,
            "{}",
            case.label
        );
    }

    let alpha_package =
        load_profile_package_file(&fixture_path(ALPHA_PACKAGE)).expect("alpha package loads");
    let beta_package =
        load_profile_package_file(&fixture_path(BETA_PACKAGE)).expect("beta package loads");
    let alpha_digest = entity_profile_package_digest(&alpha_package).expect("alpha package digest");
    let beta_digest = entity_profile_package_digest(&beta_package).expect("beta package digest");
    let alpha = execute_profile_package_from_paths(
        &fixture_path(ALPHA_PACKAGE),
        &fixture_path(ALPHA_RECORDS),
        &alpha_cluster_request(Some(alpha_digest)),
    )
    .expect("alpha package execution should pass");
    let beta = execute_profile_package_from_paths(
        &fixture_path(BETA_PACKAGE),
        &fixture_path(BETA_RECORDS),
        &beta_link_request(Some(beta_digest)),
    )
    .expect("beta package execution should pass");
    assert_eq!(
        execution_projection(&alpha),
        expected_fixture(ALPHA_EXPECTED)
    );
    assert_eq!(execution_projection(&beta), expected_fixture(BETA_EXPECTED));
}

#[test]
fn external_profile_package_execution_refuses_mutations() {
    let alpha_package =
        load_profile_package_file(&fixture_path(ALPHA_PACKAGE)).expect("alpha package loads");

    let bad_records = b"alpha_id,alpha_group\nA-001,blue\n";
    let error = execute_profile_package_records(
        &alpha_package,
        bad_records,
        EntityProfileRecordInputFormat::Csv,
        &alpha_cluster_request(None),
    )
    .expect_err("unknown mapped field must refuse");
    assert_eq!(error.code, ProfileErrorCode::UnknownField);

    let mut missing_canonical_role = alpha_package.clone();
    for mapping in &mut missing_canonical_role.field_mappings {
        if mapping.field_role == ENTITY_PROFILE_CANONICAL_SURFACE_ROLE {
            mapping.field_role = "display_label".to_string();
        }
    }
    let records = std::fs::read(fixture_path(ALPHA_RECORDS)).expect("alpha records");
    let error = execute_profile_package_records(
        &missing_canonical_role,
        &records,
        EntityProfileRecordInputFormat::Csv,
        &alpha_cluster_request(None),
    )
    .expect_err("missing canonical role must refuse");
    assert_eq!(error.code, ProfileErrorCode::UnknownField);

    let error = execute_profile_package_from_paths(
        &fixture_path(ALPHA_PACKAGE),
        &fixture_path(ALPHA_RECORDS),
        &alpha_cluster_request(Some(sample_hash('0'))),
    )
    .expect_err("digest drift must refuse");
    assert_eq!(error.code, ProfileErrorCode::CompatibilityPolicy);

    let mut output_mismatch = alpha_cluster_request(None);
    output_mismatch
        .execution
        .required_outputs
        .push("missing_output".to_string());
    let error = execute_profile_package_from_paths(
        &fixture_path(ALPHA_PACKAGE),
        &fixture_path(ALPHA_RECORDS),
        &output_mismatch,
    )
    .expect_err("output mismatch must refuse");
    assert_eq!(error.code, ProfileErrorCode::MissingCapability);

    let mut capability_mismatch = alpha_cluster_request(None);
    capability_mismatch
        .execution
        .required_capabilities
        .push(ProfileCapability::SolveLink);
    let error = execute_profile_package_from_paths(
        &fixture_path(ALPHA_PACKAGE),
        &fixture_path(ALPHA_RECORDS),
        &capability_mismatch,
    )
    .expect_err("capability mismatch must refuse");
    assert_eq!(error.code, ProfileErrorCode::MissingCapability);

    let mut package_json: Value = serde_json::from_slice(
        &std::fs::read(fixture_path(ALPHA_PACKAGE)).expect("alpha package bytes"),
    )
    .expect("alpha package JSON");
    package_json
        .as_object_mut()
        .expect("package object")
        .insert("unexpected_control".to_string(), json!(true));
    let error = load_profile_package_bytes(
        &serde_json::to_vec(&package_json).expect("mutated package bytes"),
    )
    .expect_err("schema drift must refuse");
    assert_eq!(error.code, ProfileErrorCode::ArtifactContract);
}

#[test]
fn public_prepare_and_index_accept_external_profile_packages() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = empty_registry(temp.path().join("registry"));
    let strategy = fixture_path("tests/fixtures/entity/strategies/regab_firm_identity.yaml");
    let alpha_package = alpha_package_with_second_view(
        temp.path(),
        "alpha_public.json",
        "alpha_group_upper",
        "ascii_trim_upper",
    );

    for (
        name,
        package_path,
        rows_path,
        profile_id,
        view_name,
        view_values,
        extra_view_name,
        extra_view_values,
    ) in [
        (
            "alpha",
            alpha_package,
            fixture_path(ALPHA_RECORDS),
            "pkg.alpha.surface_profile",
            "alpha_label_core",
            vec!["north ridge", "south ridge"],
            "alpha_group_upper",
            vec!["BLUE", "GREEN"],
        ),
        (
            "beta",
            fixture_path(BETA_PACKAGE),
            fixture_path(BETA_RECORDS),
            "pkg.beta.link_profile",
            "caption_tokens",
            vec!["second|signal", "third|signal"],
            "segment_key",
            vec!["EAST", "WEST"],
        ),
        (
            "cmbs example",
            fixture_path(CMBS_EXAMPLE_PACKAGE),
            fixture_path(CMBS_EXAMPLE_RECORDS),
            "example.cmbs.tenant_label",
            "tenant_label_example_core",
            vec!["238 sand island prop", "24 hour fitness", "sears"],
            "property_context_key",
            vec!["P001", "P005", "P011"],
        ),
        (
            "regab example",
            fixture_path(REGAB_EXAMPLE_PACKAGE),
            fixture_path(REGAB_EXAMPLE_RECORDS),
            "example.regab.firm_identity",
            "firm_identity_example_core",
            vec![
                "acme review analytics llc",
                "kpmg llp",
                "kpmg securitization trust 2024-c1",
                "midland loan services, a division of pnc bank, national association",
                "pnc bank, national association",
                "wells fargo bank, national association",
                "wells fargo commercial mortgage securities platform",
                "wells fargo commercial mortgage servicing, a division of wells fargo bank, national association",
            ],
            "dataset_example_key",
            vec![
                "REGAB_ATTESTATIONS",
                "REGAB_ATTESTATIONS",
                "REGAB_PLATFORM_ROSTERS",
                "REGAB_SERVICER_SCHEDULES",
                "REGAB_SERVICER_SCHEDULES",
                "REGAB_SERVICER_SCHEDULES",
                "REGAB_SERVICER_SCHEDULES",
                "REGAB_SERVICER_SCHEDULES",
            ],
        ),
    ] {
        let work_dir = temp.path().join(format!("{name}_work"));
        let package_digest = package_digest(&package_path);

        let prepare = assert_canon_success(
            canon_output(vec![
                "entity".to_string(),
                "prepare".to_string(),
                rows_path.display().to_string(),
                "--profile".to_string(),
                package_path.display().to_string(),
                "--registry".to_string(),
                registry.display().to_string(),
                "--work-dir".to_string(),
                work_dir.display().to_string(),
            ]),
            "external package prepare",
        );
        assert_eq!(prepare["metadata"]["profile"]["id"], profile_id);
        assert_eq!(
            prepare["metadata"]["profile"]["content_hash"],
            package_digest
        );
        assert!(work_dir.join("prepare/prepare.json").exists());
        assert!(work_dir.join("prepare/surfaces.jsonl").exists());
        let surfaces = read_surface_records(&work_dir.join("prepare/surfaces.jsonl"));
        assert_eq!(
            normalized_view_values(&surfaces, view_name),
            view_values,
            "{name} must emit package-declared canonical view values"
        );
        assert_eq!(
            normalized_view_values(&surfaces, extra_view_name),
            extra_view_values,
            "{name} must preserve package-declared noncanonical view values"
        );
        assert!(
            canonical_view_has_surface_id_marker(&surfaces, view_name),
            "{name} canonical view must control surface IDs"
        );
        assert!(
            noncanonical_view_lacks_surface_id_marker(&surfaces, extra_view_name),
            "{name} noncanonical view must not control surface IDs"
        );
        assert!(
            surfaces
                .iter()
                .all(|surface| surface["normalized_views"]["core"].is_null()),
            "{name} must not fall back to the synthetic core view"
        );
        assert!(
            surface_ids(&surfaces)
                .iter()
                .all(|surface_id| surface_id.starts_with(&format!("surf:{profile_id}:"))),
            "{name} surface IDs must be profile-scoped"
        );

        let index = assert_canon_success(
            canon_output(vec![
                "entity".to_string(),
                "index".to_string(),
                "build".to_string(),
                rows_path.display().to_string(),
                "--profile".to_string(),
                package_path.display().to_string(),
                "--strategy".to_string(),
                strategy.display().to_string(),
                "--registry".to_string(),
                registry.display().to_string(),
                "--work-dir".to_string(),
                work_dir.display().to_string(),
            ]),
            "external package index",
        );
        assert_eq!(index["artifact"]["metadata"]["profile"]["id"], profile_id);
        assert_eq!(
            index["artifact"]["metadata"]["profile"]["content_hash"],
            package_digest
        );
        assert!(Path::new(index["paths"]["artifact"].as_str().expect("artifact path")).exists());
        assert!(Path::new(index["paths"]["postings"].as_str().expect("postings path")).exists());
        assert!(
            Path::new(
                index["paths"]["diagnostics"]
                    .as_str()
                    .expect("diagnostics path")
            )
            .exists()
        );
    }
}

#[test]
fn external_package_profile_id_collision_prefers_package_marker_over_legacy_view() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = empty_registry(temp.path().join("registry"));
    let work_dir = temp.path().join("collision_work");
    let rows_path = fixture_path(ALPHA_RECORDS);
    let package_path =
        alpha_package_with_profile_id(temp.path(), "alpha_as_cmbs.json", "cmbs_tenant_label");
    let strategy = fixture_path("tests/fixtures/entity/strategies/regab_firm_identity.yaml");
    let package_digest = package_digest(&package_path);

    let prepare = assert_canon_success(
        canon_output(vec![
            "entity".to_string(),
            "prepare".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            package_path.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            work_dir.display().to_string(),
        ]),
        "former built-in profile id package prepare",
    );
    assert_eq!(prepare["metadata"]["profile"]["id"], "cmbs_tenant_label");
    assert_eq!(
        prepare["metadata"]["profile"]["content_hash"],
        package_digest
    );

    let surfaces = read_surface_records(&work_dir.join("prepare/surfaces.jsonl"));
    assert_eq!(
        normalized_view_values(&surfaces, "alpha_label_core"),
        vec!["north ridge", "south ridge"]
    );
    assert!(
        canonical_view_has_surface_id_marker(&surfaces, "alpha_label_core"),
        "Alpha canonical view marker must outrank the legacy cmbs_tenant_label fallback"
    );
    assert!(
        surfaces
            .iter()
            .all(|surface| surface["normalized_views"]["tenant_core"].is_null()),
        "package path must not require legacy tenant_core"
    );
    assert!(
        surface_ids(&surfaces)
            .iter()
            .all(|surface_id| surface_id.starts_with("surf:cmbs_tenant_label:")),
        "collision package IDs remain profile-scoped while using the package canonical view"
    );

    let index = assert_canon_success(
        canon_output(vec![
            "entity".to_string(),
            "index".to_string(),
            "build".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            package_path.display().to_string(),
            "--strategy".to_string(),
            strategy.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            work_dir.display().to_string(),
        ]),
        "former built-in profile id package index",
    );
    assert_eq!(
        index["artifact"]["metadata"]["profile"]["content_hash"],
        package_digest
    );
}

#[test]
fn public_prepare_is_deterministic_for_canonicalized_package_order() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = empty_registry(temp.path().join("registry"));
    let work_dir = temp.path().join("work");
    let rows_path = fixture_path(ALPHA_RECORDS);
    let original_package = fixture_path(ALPHA_PACKAGE);
    let shuffled_package = shuffled_package_copy(temp.path(), ALPHA_PACKAGE, "alpha_shuffled.json");
    assert_eq!(
        package_digest(&original_package),
        package_digest(&shuffled_package)
    );

    assert_canon_success(
        canon_output(vec![
            "entity".to_string(),
            "prepare".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            original_package.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            work_dir.display().to_string(),
        ]),
        "original package prepare",
    );
    let prepare_bytes = fs::read(work_dir.join("prepare/prepare.json")).expect("prepare artifact");
    let surfaces_bytes =
        fs::read(work_dir.join("prepare/surfaces.jsonl")).expect("surface records");
    let surface_ids_before = surface_ids(&read_surface_records(
        &work_dir.join("prepare/surfaces.jsonl"),
    ));

    assert_canon_success(
        canon_output(vec![
            "entity".to_string(),
            "prepare".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            shuffled_package.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            work_dir.display().to_string(),
        ]),
        "shuffled package prepare",
    );
    assert_eq!(
        fs::read(work_dir.join("prepare/prepare.json")).expect("prepare artifact"),
        prepare_bytes
    );
    assert_eq!(
        fs::read(work_dir.join("prepare/surfaces.jsonl")).expect("surface records"),
        surfaces_bytes
    );
    assert_eq!(
        surface_ids(&read_surface_records(
            &work_dir.join("prepare/surfaces.jsonl")
        )),
        surface_ids_before
    );
}

#[test]
fn public_prepare_and_index_refuse_package_contract_drift_before_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = empty_registry(temp.path().join("registry"));
    let rows_path = fixture_path(ALPHA_RECORDS);
    let strategy = fixture_path("tests/fixtures/entity/strategies/regab_firm_identity.yaml");

    for (name, expected_code, package) in [
        (
            "schema",
            "E_ENTITY_PROFILE",
            mutated_package(temp.path(), ALPHA_PACKAGE, "schema.json", |value| {
                value
                    .as_object_mut()
                    .expect("package object")
                    .insert("unexpected_control".to_string(), json!(true));
            }),
        ),
        (
            "operator",
            "E_ENTITY_PROFILE",
            mutated_package(temp.path(), ALPHA_PACKAGE, "operator.json", |value| {
                value["normalized_views"]["alpha_label_core"]["operators"][0] =
                    json!({ "op": "domain_runtime_magic" });
            }),
        ),
        (
            "no_canonical_surface",
            "E_ENTITY_PROFILE",
            mutated_package(temp.path(), ALPHA_PACKAGE, "no_surface.json", |value| {
                for mapping in value["field_mappings"]
                    .as_array_mut()
                    .expect("field mappings")
                {
                    if mapping["field_role"] == ENTITY_PROFILE_CANONICAL_SURFACE_ROLE {
                        mapping["field_role"] = json!("context_value");
                    }
                }
            }),
        ),
    ] {
        let work_dir = temp.path().join(format!("bad_{name}_work"));
        let refusal = assert_canon_refusal(
            canon_output(vec![
                "entity".to_string(),
                "prepare".to_string(),
                rows_path.display().to_string(),
                "--profile".to_string(),
                package.display().to_string(),
                "--registry".to_string(),
                registry.display().to_string(),
                "--work-dir".to_string(),
                work_dir.display().to_string(),
            ]),
            name,
        );
        assert_eq!(refusal["outcome"], "REFUSAL");
        assert_eq!(refusal["refusal"]["code"], expected_code);
        assert!(
            !work_dir.join("prepare/prepare.json").exists(),
            "{name} refusal must not write prepare artifact"
        );
    }

    let digest_work = temp.path().join("digest_work");
    let package = alpha_package_with_second_view(
        temp.path(),
        "profile.json",
        "alpha_group_upper",
        "ascii_trim_upper",
    );
    assert_canon_success(
        canon_output(vec![
            "entity".to_string(),
            "prepare".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            package.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            digest_work.display().to_string(),
        ]),
        "prepare before digest drift",
    );
    let original_surfaces = read_surface_records(&digest_work.join("prepare/surfaces.jsonl"));
    let original_surface_ids = surface_ids(&original_surfaces);
    assert_eq!(
        normalized_view_values(&original_surfaces, "alpha_group_upper"),
        vec!["BLUE", "GREEN"]
    );

    let noncanonical_operator_package = alpha_package_with_second_view(
        temp.path(),
        "profile_noncanonical_operator.json",
        "alpha_group_upper",
        "lowercase",
    );
    let noncanonical_work = temp.path().join("noncanonical_operator_work");
    assert_canon_success(
        canon_output(vec![
            "entity".to_string(),
            "prepare".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            noncanonical_operator_package.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            noncanonical_work.display().to_string(),
        ]),
        "prepare after noncanonical operator drift",
    );
    let noncanonical_surfaces =
        read_surface_records(&noncanonical_work.join("prepare/surfaces.jsonl"));
    assert_eq!(
        normalized_view_values(&noncanonical_surfaces, "alpha_group_upper"),
        vec!["blue", "green"]
    );
    assert_eq!(
        surface_ids(&noncanonical_surfaces),
        original_surface_ids,
        "noncanonical view changes must not change canonical-view surface IDs"
    );
    let refusal = assert_canon_refusal(
        canon_output(vec![
            "entity".to_string(),
            "index".to_string(),
            "build".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            noncanonical_operator_package.display().to_string(),
            "--strategy".to_string(),
            strategy.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            digest_work.display().to_string(),
        ]),
        "index after noncanonical operator digest drift",
    );
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_PROFILE");
    assert!(
        !digest_work.join("index").exists(),
        "noncanonical operator digest drift refusal must not write index artifacts"
    );

    rewrite_package_json(&package, |value| {
        value["expected_outputs"]
            .as_array_mut()
            .expect("expected outputs")
            .push(json!("new_noncanonical_output"));
    });
    let refusal = assert_canon_refusal(
        canon_output(vec![
            "entity".to_string(),
            "index".to_string(),
            "build".to_string(),
            rows_path.display().to_string(),
            "--profile".to_string(),
            package.display().to_string(),
            "--strategy".to_string(),
            strategy.display().to_string(),
            "--registry".to_string(),
            registry.display().to_string(),
            "--work-dir".to_string(),
            digest_work.display().to_string(),
        ]),
        "index after package digest drift",
    );
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_PROFILE");
    assert!(
        !digest_work.join("index").exists(),
        "output digest drift refusal must not write index artifacts"
    );
}

fn fixture_path(relative_path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}

#[derive(Clone, Copy)]
struct PackageExecutionCase {
    label: &'static str,
    package: &'static str,
    records: &'static str,
    request: fn(Option<String>) -> EntityProfilePackageRunRequest,
    expected_profile: &'static str,
    expected_entity_type: &'static str,
    expected_canonical_view: &'static str,
    expected_canonical_surfaces: &'static [&'static str],
    expected_record_keys: &'static [&'static str],
    expected_extra_view: Option<ExpectedView>,
}

#[derive(Clone, Copy)]
struct ExpectedView {
    name: &'static str,
    expected_values: &'static [&'static str],
}

fn external_package_execution_cases() -> Vec<PackageExecutionCase> {
    vec![
        PackageExecutionCase {
            label: "alpha",
            package: ALPHA_PACKAGE,
            records: ALPHA_RECORDS,
            request: alpha_cluster_request,
            expected_profile: "pkg.alpha.surface_profile",
            expected_entity_type: "pkg.alpha:entry",
            expected_canonical_view: "alpha_label_core",
            expected_canonical_surfaces: &["north ridge", "south ridge"],
            expected_record_keys: &["A-001", "A-002"],
            expected_extra_view: None,
        },
        PackageExecutionCase {
            label: "beta",
            package: BETA_PACKAGE,
            records: BETA_RECORDS,
            request: beta_link_request,
            expected_profile: "pkg.beta.link_profile",
            expected_entity_type: "pkg.beta:item",
            expected_canonical_view: "caption_tokens",
            expected_canonical_surfaces: &["second|signal", "third|signal"],
            expected_record_keys: &["B-100", "B-200"],
            expected_extra_view: Some(ExpectedView {
                name: "segment_key",
                expected_values: &["EAST", "WEST"],
            }),
        },
        PackageExecutionCase {
            label: "cmbs example",
            package: CMBS_EXAMPLE_PACKAGE,
            records: CMBS_EXAMPLE_RECORDS,
            request: cmbs_example_cluster_request,
            expected_profile: "example.cmbs.tenant_label",
            expected_entity_type: "example.cmbs:tenant_label",
            expected_canonical_view: "tenant_label_example_core",
            expected_canonical_surfaces: &["sears", "24 hour fitness", "238 sand island prop"],
            expected_record_keys: &["cmbs-apply-001", "cmbs-apply-002", "cmbs-apply-003"],
            expected_extra_view: Some(ExpectedView {
                name: "property_context_key",
                expected_values: &["P001", "P005", "P011"],
            }),
        },
        PackageExecutionCase {
            label: "regab example",
            package: REGAB_EXAMPLE_PACKAGE,
            records: REGAB_EXAMPLE_RECORDS,
            request: regab_example_cluster_request,
            expected_profile: "example.regab.firm_identity",
            expected_entity_type: "example.regab:firm",
            expected_canonical_view: "firm_identity_example_core",
            expected_canonical_surfaces: &[
                "pnc bank, national association",
                "midland loan services, a division of pnc bank, national association",
                "wells fargo bank, national association",
                "wells fargo commercial mortgage servicing, a division of wells fargo bank, national association",
                "wells fargo commercial mortgage securities platform",
                "kpmg llp",
                "kpmg securitization trust 2024-c1",
                "acme review analytics llc",
            ],
            expected_record_keys: &[
                "record-regab-001",
                "record-regab-002",
                "record-regab-003",
                "record-regab-004",
                "record-regab-005",
                "record-regab-006",
                "record-regab-007",
                "record-regab-008",
            ],
            expected_extra_view: Some(ExpectedView {
                name: "dataset_example_key",
                expected_values: &[
                    "REGAB_SERVICER_SCHEDULES",
                    "REGAB_SERVICER_SCHEDULES",
                    "REGAB_SERVICER_SCHEDULES",
                    "REGAB_SERVICER_SCHEDULES",
                    "REGAB_PLATFORM_ROSTERS",
                    "REGAB_ATTESTATIONS",
                    "REGAB_ATTESTATIONS",
                    "REGAB_SERVICER_SCHEDULES",
                ],
            }),
        },
    ]
}

fn package_digest(path: &Path) -> String {
    let package = load_profile_package_file(path).expect("profile package loads");
    entity_profile_package_digest(&package).expect("profile package digest")
}

fn empty_registry(path: PathBuf) -> PathBuf {
    fs::create_dir_all(&path).expect("registry directory");
    fs::write(
        path.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "neutral-empty",
            "version": "2026.07.14",
            "description": "neutral empty test registry",
            "updated": "2026-07-14",
            "entry_count": 0
        }))
        .expect("registry json"),
    )
    .expect("write registry json");
    path
}

fn canon_output(args: Vec<String>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .expect("canon subprocess should run")
}

fn assert_canon_success(output: Output, label: &str) -> Value {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("canon stdout JSON")
}

fn assert_canon_refusal(output: Output, label: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(2),
        "{label} should refuse\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let refusal_bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    serde_json::from_slice(refusal_bytes).expect("canon refusal JSON")
}

fn read_surface_records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("surface records")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("surface JSON"))
        .collect()
}

fn normalized_view_values(surfaces: &[Value], view_name: &str) -> Vec<String> {
    let mut values = surfaces
        .iter()
        .map(|surface| {
            surface["normalized_views"][view_name]["value"]
                .as_str()
                .expect("normalized view value")
                .to_string()
        })
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn surface_ids(surfaces: &[Value]) -> Vec<String> {
    let mut ids = surfaces
        .iter()
        .map(|surface| {
            surface["surface_id"]
                .as_str()
                .expect("surface id")
                .to_string()
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn canonical_view_has_surface_id_marker(surfaces: &[Value], view_name: &str) -> bool {
    surfaces
        .iter()
        .all(|surface| view_reason_codes(surface, view_name).contains(&"surface_id_view"))
}

fn noncanonical_view_lacks_surface_id_marker(surfaces: &[Value], view_name: &str) -> bool {
    surfaces
        .iter()
        .all(|surface| !view_reason_codes(surface, view_name).contains(&"surface_id_view"))
}

fn view_reason_codes<'a>(surface: &'a Value, view_name: &str) -> Vec<&'a str> {
    surface["normalized_views"][view_name]["reason_codes"]
        .as_array()
        .expect("reason codes")
        .iter()
        .map(|value| value.as_str().expect("reason code"))
        .collect()
}

fn alpha_package_with_second_view(
    temp: &Path,
    name: &str,
    view_name: &str,
    operator: &str,
) -> PathBuf {
    mutated_package(temp, ALPHA_PACKAGE, name, |value| {
        value["normalized_views"]
            .as_object_mut()
            .expect("normalized views")
            .insert(
                view_name.to_string(),
                json!({ "operators": [{ "op": operator }] }),
            );
        for mapping in value["field_mappings"]
            .as_array_mut()
            .expect("field mappings")
        {
            if mapping["field_path"] == "alpha_group" {
                mapping["normalized_view"] = json!(view_name);
            }
        }
        for mode in value["execution_modes"]
            .as_array_mut()
            .expect("execution modes")
        {
            let field_paths = mode["field_paths"].as_array_mut().expect("field paths");
            if !field_paths.iter().any(|field| field == "alpha_group") {
                field_paths.push(json!("alpha_group"));
            }
        }
    })
}

fn alpha_package_with_profile_id(temp: &Path, name: &str, profile_id: &str) -> PathBuf {
    mutated_package(temp, ALPHA_PACKAGE, name, |value| {
        value["profile"] = json!(profile_id);
        value["patch_namespaces"] = json!({
            "aliases": format!("{profile_id}.aliases"),
            "distinct": format!("{profile_id}.distinct"),
            "relations": format!("{profile_id}.relations")
        });
    })
}

fn shuffled_package_copy(temp: &Path, source: &str, name: &str) -> PathBuf {
    mutated_package(temp, source, name, |value| {
        reverse_array(value, &["field_mappings"]);
        reverse_array(value, &["available_capabilities"]);
        reverse_array(value, &["expected_outputs"]);
        reverse_array(value, &["execution_modes"]);
    })
}

fn mutated_package(
    temp: &Path,
    source: &str,
    name: &str,
    mutate: impl FnOnce(&mut Value),
) -> PathBuf {
    let path = temp.join(name);
    let mut value: Value =
        serde_json::from_slice(&fs::read(fixture_path(source)).expect("profile package bytes"))
            .expect("profile package JSON");
    mutate(&mut value);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&value).expect("mutated package JSON"),
    )
    .expect("write mutated profile package");
    path
}

fn rewrite_package_json(path: &Path, mutate: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&fs::read(path).expect("profile package bytes"))
        .expect("profile package JSON");
    mutate(&mut value);
    fs::write(
        path,
        serde_json::to_vec_pretty(&value).expect("mutated package JSON"),
    )
    .expect("rewrite profile package");
}

fn reverse_array(value: &mut Value, path: &[&str]) {
    let mut current = value;
    for key in path {
        current = &mut current[*key];
    }
    current.as_array_mut().expect("array path").reverse();
}

fn expected_fixture(relative_path: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(fixture_path(relative_path)).expect("expected fixture bytes"),
    )
    .expect("expected fixture JSON")
}

fn execution_projection(execution: &EntityProfilePackageExecution) -> Value {
    json!({
        "schema_version": execution.schema_version,
        "profile": execution.profile,
        "mode": execution.plan.mode.mode,
        "records_format": execution.records_format,
        "record_count": execution.record_count,
        "canonical_view": execution.canonical_view,
        "canonical_surfaces": execution.records.iter().map(|record| {
            record.canonical_surface.clone()
        }).collect::<Vec<_>>(),
        "record_keys": execution.records.iter().map(record_key).collect::<Vec<_>>(),
        "required_outputs": execution.output_status.keys().cloned().collect::<Vec<_>>(),
    })
}

fn record_key(record: &EntityProfileExecutionRecord) -> String {
    record.record_key.clone().expect("record key")
}

fn alpha_cluster_request(
    expected_package_digest: Option<String>,
) -> EntityProfilePackageRunRequest {
    EntityProfilePackageRunRequest {
        execution: EntityProfileExecutionRequest {
            mode: ProfileModeKind::Cluster,
            source_object_type: "pkg.alpha:entry".to_string(),
            target_object_type: None,
            required_capabilities: vec![
                ProfileCapability::Prepare,
                ProfileCapability::Index,
                ProfileCapability::Block,
                ProfileCapability::Evidence,
                ProfileCapability::SolveCluster,
                ProfileCapability::Review,
                ProfileCapability::Promote,
                ProfileCapability::Apply,
            ],
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "cluster_assignments".to_string(),
                "review_queue".to_string(),
            ],
        },
        expected_package_digest,
    }
}

fn beta_link_request(expected_package_digest: Option<String>) -> EntityProfilePackageRunRequest {
    EntityProfilePackageRunRequest {
        execution: EntityProfileExecutionRequest {
            mode: ProfileModeKind::Link,
            source_object_type: "pkg.beta:item".to_string(),
            target_object_type: Some("pkg.beta:item".to_string()),
            required_capabilities: vec![
                ProfileCapability::Prepare,
                ProfileCapability::Index,
                ProfileCapability::Block,
                ProfileCapability::Evidence,
                ProfileCapability::SolveLink,
                ProfileCapability::Review,
                ProfileCapability::Promote,
                ProfileCapability::Apply,
            ],
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "link_candidates".to_string(),
                "link_decisions".to_string(),
            ],
        },
        expected_package_digest,
    }
}

fn cmbs_example_cluster_request(
    expected_package_digest: Option<String>,
) -> EntityProfilePackageRunRequest {
    EntityProfilePackageRunRequest {
        execution: EntityProfileExecutionRequest {
            mode: ProfileModeKind::Cluster,
            source_object_type: "example.cmbs:tenant_label".to_string(),
            target_object_type: None,
            required_capabilities: vec![
                ProfileCapability::Prepare,
                ProfileCapability::Index,
                ProfileCapability::Block,
                ProfileCapability::Evidence,
                ProfileCapability::SolveCluster,
                ProfileCapability::Review,
                ProfileCapability::Promote,
                ProfileCapability::Apply,
            ],
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "cluster_assignments".to_string(),
                "review_queue".to_string(),
            ],
        },
        expected_package_digest,
    }
}

fn regab_example_cluster_request(
    expected_package_digest: Option<String>,
) -> EntityProfilePackageRunRequest {
    EntityProfilePackageRunRequest {
        execution: EntityProfileExecutionRequest {
            mode: ProfileModeKind::Cluster,
            source_object_type: "example.regab:firm".to_string(),
            target_object_type: None,
            required_capabilities: vec![
                ProfileCapability::Prepare,
                ProfileCapability::Index,
                ProfileCapability::Block,
                ProfileCapability::Evidence,
                ProfileCapability::SolveCluster,
                ProfileCapability::Review,
                ProfileCapability::Promote,
                ProfileCapability::Apply,
            ],
            required_outputs: vec![
                "prepare_bundle".to_string(),
                "cluster_assignments".to_string(),
                "review_queue".to_string(),
            ],
        },
        expected_package_digest,
    }
}

fn sample_hash(hex: char) -> String {
    format!(
        "blake3:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}
