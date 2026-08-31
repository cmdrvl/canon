#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/project/lock.rs"]
mod lock;
#[allow(dead_code)]
#[path = "../src/project/manifest.rs"]
mod manifest;
#[allow(dead_code)]
#[path = "../src/project/plan.rs"]
mod plan;

use lock::{
    ProjectLock, ProjectLockInput, ProjectLockManifestProjection, ProjectLockRefKind,
    ProjectLockResolvedRef, digest_bytes, refresh_project_lock,
};
use manifest::{
    ProjectManifest, ProjectNetworkPolicy, ProjectPackageKind, load_project_manifest_toml,
    project_manifest_digest,
};
use plan::{
    ProjectExtensionDagNode, ProjectExtensionDagOutput, ProjectExtensionDagRequest,
    ProjectExtensionNodePolicy, ProjectPlanCacheDecision, ProjectPlanErrorCode,
    ProjectPlanNodeClass, ProjectPlanNodeKind, ProjectPlanOutputMaterialization,
    ProjectPlanRefusalCondition, ProjectPlanRequest, ProjectPlanSideEffect,
    ProjectPlanSideEffectKind, canonical_project_plan_bytes, compile_extension_project_plan,
    compile_project_plan, project_plan_node_cache_key, project_plan_schema_version,
    render_project_plan_summary, validate_extension_node_effects, validate_project_plan,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.project.plan.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/project/plan.rs");
const MINIMAL_TOML: &str = include_str!("./fixtures/project/minimal.toml");

#[test]
fn schema_declares_pure_dry_run_dag_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], project_plan_schema_version());
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        project_plan_schema_version()
    );
    assert_eq!(schema["properties"]["plan_kind"]["const"], "dry_run");
    assert_eq!(schema["x-canon-contract"]["pure_dry_run"], true);
    assert_eq!(schema["x-canon-contract"]["no_provider_probe"], true);
    assert_eq!(schema["x-canon-contract"]["detects_cycles"], true);
    assert_eq!(
        schema["x-canon-contract"]["detects_missing_producers"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["detects_output_collisions"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["distinguishes_node_classes"][2],
        "review_pause"
    );
}

#[test]
fn minimal_fixture_compiles_byte_stable_dry_run_dag() {
    let manifest = minimal_manifest();
    let lock = lock_for_manifest(&manifest);
    let request =
        request(manifest.clone(), lock.clone()).with_plan_path("target/project.plan.json");

    let first = compile_project_plan(request.clone()).expect("first plan compiles");
    let second = compile_project_plan(request).expect("second plan compiles");

    assert_eq!(
        canonical_project_plan_bytes(&first).unwrap(),
        canonical_project_plan_bytes(&second).unwrap()
    );
    assert_eq!(first.graph_hash, second.graph_hash);
    assert_eq!(first.schema_version, "canon.project.plan.v1");
    assert_eq!(first.plan_kind, "dry_run");
    assert_eq!(first.summary.total_nodes, 11);
    assert_eq!(first.summary.review_pause_nodes, 1);
    assert_eq!(first.summary.mutation_gate_nodes, 1);
    assert_eq!(first.summary.export_nodes, 1);
    assert_eq!(first.summary.runnable_nodes, 1);
    assert!(first.next_commands.contains_key("intake.source_alpha"));
    assert!(
        first
            .nodes
            .iter()
            .any(|node| node.kind == ProjectPlanNodeKind::ExactReplay)
    );
    assert!(render_project_plan_summary(&first).contains("next=[intake.source_alpha]"));
    log_plan("minimal", &first.graph_hash, first.summary.total_nodes);
}

#[test]
fn link_plan_distinguishes_external_materialization_review_and_mutation_gates() {
    let manifest = load_project_manifest_toml(&link_manifest_toml()).expect("manifest loads");
    assert_eq!(
        manifest.runtime.network_policy,
        ProjectNetworkPolicy::AllowDeclaredHosts
    );
    let plan = compile_project_plan(request(manifest.clone(), lock_for_manifest(&manifest)))
        .expect("link plan compiles");

    let materialize = node(&plan, "materialize.external");
    assert_eq!(
        materialize.class,
        ProjectPlanNodeClass::ExternalMaterialization
    );
    assert_eq!(
        materialize.kind,
        ProjectPlanNodeKind::ExternalMaterialization
    );
    assert!(
        materialize
            .side_effects
            .iter()
            .any(|effect| effect.description.contains("declared external"))
    );

    assert_eq!(
        node(&plan, "link.link_cross").kind,
        ProjectPlanNodeKind::Link
    );
    assert_eq!(
        node(&plan, "review.link_cross").class,
        ProjectPlanNodeClass::ReviewPause
    );
    assert_eq!(
        node(&plan, "promote.link_cross").class,
        ProjectPlanNodeClass::MutationGate
    );
    assert_eq!(plan.summary.external_materialization_nodes, 1);
    assert_eq!(plan.summary.export_nodes, 2);
    assert!(
        plan.nodes
            .iter()
            .all(|node| !node.command.contains("api.example.test"))
    );
    log_plan("link_external", &plan.graph_hash, plan.summary.total_nodes);
}

#[test]
fn cache_hits_are_explicit_and_make_next_uncached_node_runnable() {
    let manifest = minimal_manifest();
    let lock = lock_for_manifest(&manifest);
    let mut request = request(manifest, lock);
    request.cache_hits = BTreeSet::from([
        "intake.source_alpha".to_string(),
        "normalize.source_alpha".to_string(),
    ]);

    let plan = compile_project_plan(request).expect("cached plan compiles");
    assert_eq!(
        node(&plan, "intake.source_alpha").cache.decision,
        ProjectPlanCacheDecision::Hit
    );
    assert_eq!(
        node(&plan, "normalize.source_alpha").cache.decision,
        ProjectPlanCacheDecision::Hit
    );
    assert!(node(&plan, "index.cluster_default").runnable);
    assert!(plan.next_commands.contains_key("index.cluster_default"));
    assert_eq!(plan.summary.cache_hits, 2);
    assert_eq!(plan.summary.runnable_nodes, 1);
    log_plan("cached", &plan.graph_hash, plan.summary.total_nodes);
}

#[test]
fn planning_refuses_unknown_or_ineligible_cache_hits() {
    let manifest = minimal_manifest();
    let lock = lock_for_manifest(&manifest);
    let mut request = request(manifest, lock);
    request.cache_hits = BTreeSet::from(["review.cluster_default".to_string()]);

    let error = compile_project_plan(request).expect_err("ineligible hit refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::CachePolicy);
    assert_eq!(
        error.diagnostics[0].node_id.as_deref(),
        Some("review.cluster_default")
    );
}

#[test]
fn output_collisions_and_impossible_review_policy_refuse_before_execution() {
    let collision = format!(
        "{MINIMAL_TOML}\n[[modes]]\nmode_id = \"cluster_again\"\nkind = \"cluster\"\nsource_ids = [\"source_alpha\"]\nregistry_package = \"registry\"\nstrategy_package = \"strategy\"\nprofile_package = \"profile\"\noutput_ids = [\"summary\"]\n"
    );
    let manifest = load_project_manifest_toml(&collision).expect("manifest allows shared output");
    let error = compile_project_plan(request(manifest.clone(), lock_for_manifest(&manifest)))
        .expect_err("plan refuses output collision");
    assert_eq!(error.code, ProjectPlanErrorCode::OutputCollision);
    assert!(error.diagnostics[0].message.contains("out/summary.json"));

    let impossible_review = MINIMAL_TOML.replace(
        "auto_promote_min_score_basis_points = 9500",
        "auto_promote_min_score_basis_points = 7000",
    );
    let manifest =
        load_project_manifest_toml(&impossible_review).expect("manifest allows equal thresholds");
    let error = compile_project_plan(request(manifest.clone(), lock_for_manifest(&manifest)))
        .expect_err("plan refuses zero-width review band");
    assert_eq!(error.code, ProjectPlanErrorCode::ReviewPolicy);
}

#[test]
fn lock_must_cover_manifest_sources_and_packages() {
    let manifest = minimal_manifest();
    let mut lock = lock_for_manifest(&manifest);
    lock.inputs.clear();
    lock = refresh_project_lock(&ProjectLockManifestProjection {
        project_id: lock.project_id.clone(),
        project_digest: lock.project_digest.clone(),
        inputs: vec![],
        resolved_refs: lock.resolved_refs.clone(),
    })
    .expect("lock refreshes without inputs");

    let error = compile_project_plan(request(manifest, lock)).expect_err("missing source refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::MissingProducer);
}

#[test]
fn validate_project_plan_reports_missing_producers_and_cycles() {
    let manifest = minimal_manifest();
    let mut plan = compile_project_plan(request(manifest.clone(), lock_for_manifest(&manifest)))
        .expect("plan compiles");
    node_mut(&mut plan, "index.cluster_default")
        .dependencies
        .push("missing.node".to_string());
    let error = validate_project_plan(&plan).expect_err("missing dependency refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::MissingProducer);

    let mut plan = compile_project_plan(request(manifest.clone(), lock_for_manifest(&manifest)))
        .expect("plan compiles");
    node_mut(&mut plan, "intake.source_alpha")
        .dependencies
        .push("export.cluster_default.summary".to_string());
    let error = validate_project_plan(&plan).expect_err("cycle refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::Cycle);
}

#[test]
fn planning_does_not_write_workspace_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_path = temp.path().join("canon.project.toml");
    let lock_path = temp.path().join("canon.project.lock.json");
    fs::write(&manifest_path, MINIMAL_TOML).expect("write manifest fixture");
    fs::write(&lock_path, "{}\n").expect("write sentinel lock file");
    let before = tree(temp.path());

    let manifest = minimal_manifest();
    let lock = lock_for_manifest(&manifest);
    let mut request = request(manifest, lock);
    request.manifest_path = manifest_path;
    request.lock_path = lock_path;
    request.plan_artifact_path = Some(temp.path().join("out/plan.json"));
    let plan = compile_project_plan(request).expect("plan compiles");
    let expected_plan_path = format!("{}/out/plan.json", temp.path().display());
    assert_eq!(
        plan.plan_artifact_path.as_deref(),
        Some(expected_plan_path.as_str())
    );

    let after = tree(temp.path());
    assert_eq!(
        before, after,
        "dry-run planning must not write plan artifacts"
    );
}

#[test]
fn source_scan_keeps_project_plan_contract_domain_neutral() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in [
        "cmbs", "regab", "servicer", "tranche", "loan", "geo", "vendor", "parcel",
    ] {
        assert!(
            !lower_source.contains(banned),
            "project plan module should remain domain-neutral: {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "project plan schema should remain domain-neutral: {banned}"
        );
    }
}

#[test]
fn max_runtime_seconds_is_runtime_policy_not_a_semantic_node_limit() {
    let first_manifest = minimal_manifest();
    let second_manifest = load_project_manifest_toml(
        &MINIMAL_TOML.replace("max_runtime_seconds = 600", "max_runtime_seconds = 3600"),
    )
    .expect("runtime-only manifest loads");

    let first_plan = compile_project_plan(request(
        first_manifest.clone(),
        lock_for_manifest(&first_manifest),
    ))
    .expect("first plan compiles");
    let second_plan = compile_project_plan(request(
        second_manifest.clone(),
        lock_for_manifest(&second_manifest),
    ))
    .expect("second plan compiles");

    assert_eq!(first_plan.nodes.len(), second_plan.nodes.len());
    for (first, second) in first_plan.nodes.iter().zip(second_plan.nodes.iter()) {
        assert_eq!(first.node_id, second.node_id);
        assert!(!first.limits.contains_key("max_runtime_seconds"));
        assert!(!second.limits.contains_key("max_runtime_seconds"));
        assert_eq!(first.cache.cache_key, second.cache.cache_key);
    }
}

#[test]
fn semantic_node_cache_key_binds_command_side_effects_and_refusals() {
    let manifest = minimal_manifest();
    let plan = compile_project_plan(request(manifest.clone(), lock_for_manifest(&manifest)))
        .expect("plan compiles");
    let base = node(&plan, "intake.source_alpha");
    assert_eq!(
        base.cache.cache_key,
        project_plan_node_cache_key(base).expect("base cache key")
    );

    let mut command_changed = base.clone();
    command_changed.command =
        "canon project execute --plan work/plan.json --node intake.source_alpha.v2".to_string();
    assert_ne!(
        base.cache.cache_key,
        project_plan_node_cache_key(&command_changed).expect("command cache key")
    );

    let mut effect_changed = base.clone();
    effect_changed.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::ReadsInput,
        description: "additional declared input read".to_string(),
    });
    assert_ne!(
        base.cache.cache_key,
        project_plan_node_cache_key(&effect_changed).expect("effect cache key")
    );

    let mut refusal_changed = base.clone();
    refusal_changed
        .refusal_conditions
        .push(ProjectPlanRefusalCondition {
            code: ProjectPlanErrorCode::ManifestPolicy,
            message: "additional declared precondition".to_string(),
            next_command: Some("canon project validate --manifest <MANIFEST>".to_string()),
        });
    assert_ne!(
        base.cache.cache_key,
        project_plan_node_cache_key(&refusal_changed).expect("refusal cache key")
    );
}

#[test]
fn extension_node_validation_rejects_undeclared_network_and_mutation_effects() {
    let manifest = minimal_manifest();
    let plan = compile_project_plan(request(manifest.clone(), lock_for_manifest(&manifest)))
        .expect("plan compiles");
    let mut extension_node = node(&plan, "intake.source_alpha").clone();
    extension_node.node_id = "extension.synthetic".to_string();
    extension_node.command = "canon extension run synthetic_entrypoint".to_string();
    extension_node.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::MayUseNetwork,
        description: "network request declared by extension package".to_string(),
    });
    extension_node.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::MutatesRegistry,
        description: "registry mutation declared by extension package".to_string(),
    });

    let error = validate_extension_node_effects(
        &extension_node,
        &ProjectExtensionNodePolicy::offline_read_only(),
    )
    .expect_err("undeclared side effects refuse");
    assert_eq!(error.code, ProjectPlanErrorCode::ManifestPolicy);
    assert_eq!(error.diagnostics.len(), 2);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("network access"))
    );
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("registry mutation"))
    );

    let mut shell_node = extension_node.clone();
    shell_node.command = "canon extension run synthetic; curl example.invalid".to_string();
    let error = validate_extension_node_effects(
        &shell_node,
        &ProjectExtensionNodePolicy {
            allow_network: true,
            allow_registry_mutation: true,
        },
    )
    .expect_err("ambient shell command refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::ArtifactContract);
}

#[test]
fn extension_dag_builder_emits_domain_neutral_project_plan_without_manifest_modes() {
    let prepare = extension_node("extension.prepare", [], "work/extension/prepare.json");
    let finish = extension_node(
        "extension.finish",
        ["extension.prepare"],
        "work/extension/finish.json",
    );
    let mut first_request = extension_request(vec![finish.clone(), prepare.clone()]);
    first_request.plan_artifact_path = Some("work/extension/project.plan.json".to_string());
    let mut second_request = extension_request(vec![prepare.clone(), finish.clone()]);
    second_request.plan_artifact_path = first_request.plan_artifact_path.clone();
    let mut duplicate_dependency_request = extension_request(vec![
        prepare.clone(),
        extension_node(
            "extension.finish",
            ["extension.prepare", "extension.prepare"],
            "work/extension/finish.json",
        ),
    ]);
    duplicate_dependency_request.plan_artifact_path = first_request.plan_artifact_path.clone();

    let first = compile_extension_project_plan(first_request).expect("extension DAG plan compiles");
    let second =
        compile_extension_project_plan(second_request).expect("reordered extension DAG compiles");
    let duplicate_dependency = compile_extension_project_plan(duplicate_dependency_request)
        .expect("duplicate dependencies canonicalize");

    assert_eq!(first.schema_version, "canon.project.plan.v1");
    assert_eq!(first.plan_kind, "dry_run");
    assert_eq!(first.summary.total_nodes, 2);
    assert_eq!(first.summary.edge_count, 1);
    assert_eq!(first.summary.computation_nodes, 2);
    assert_eq!(first.summary.runnable_nodes, 1);
    assert!(first.next_commands.contains_key("extension.prepare"));
    assert_eq!(first.graph_hash, second.graph_hash);
    assert_eq!(
        canonical_project_plan_bytes(&first).expect("first canonical bytes"),
        canonical_project_plan_bytes(&second).expect("second canonical bytes")
    );
    assert_eq!(first.graph_hash, duplicate_dependency.graph_hash);
    assert_eq!(
        node(&duplicate_dependency, "extension.finish")
            .dependencies
            .as_slice(),
        ["extension.prepare"]
    );
    assert_eq!(
        node(&first, "extension.finish")
            .content_hash_inputs
            .iter()
            .filter(|input| input.ref_id.starts_with("node.extension.prepare."))
            .count(),
        1
    );
    validate_project_plan(&first).expect("extension plan validates");

    let mut cached_request = extension_request(vec![prepare, finish]);
    cached_request
        .cache_hits
        .insert("extension.prepare".to_string());
    let cached =
        compile_extension_project_plan(cached_request).expect("cached extension DAG compiles");
    assert_eq!(
        node(&cached, "extension.prepare").cache.decision,
        ProjectPlanCacheDecision::Hit
    );
    assert!(cached.next_commands.contains_key("extension.finish"));
}

#[test]
fn extension_dag_builder_represents_a_valid_no_work_plan() {
    let plan = compile_extension_project_plan(extension_request(Vec::new()))
        .expect("an unsupported extension can truthfully produce an empty execution DAG");

    assert!(plan.nodes.is_empty());
    assert!(plan.next_commands.is_empty());
    assert_eq!(plan.summary.total_nodes, 0);
    assert_eq!(plan.summary.runnable_nodes, 0);
    validate_project_plan(&plan).expect("empty project DAG remains a valid plan artifact");
    let bytes = canonical_project_plan_bytes(&plan).expect("empty plan serializes");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("empty plan JSON");
    assert_eq!(value["nodes"], serde_json::json!([]));
}

#[test]
fn extension_dag_builder_rejects_ambient_shell_and_undeclared_network() {
    let mut shell = extension_node("extension.shell", [], "work/extension/shell.json");
    shell.command = "canon extension run package.entry; curl example.invalid".to_string();
    let error = compile_extension_project_plan(extension_request(vec![shell]))
        .expect_err("ambient shell command refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::ArtifactContract);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("shell control"))
    );

    let mut network = extension_node("extension.network", [], "work/extension/network.json");
    network.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::MayUseNetwork,
        description: "declared external request".to_string(),
    });
    let error = compile_extension_project_plan(extension_request(vec![network]))
        .expect_err("undeclared network refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::ManifestPolicy);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("network access"))
    );
}

#[test]
fn extension_dag_builder_preserves_collision_and_cycle_validation() {
    let left = extension_node("extension.left", [], "work/extension/shared.json");
    let right = extension_node("extension.right", [], "work/extension/shared.json");
    let error = compile_extension_project_plan(extension_request(vec![left, right]))
        .expect_err("output collision refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::OutputCollision);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("shared.json"))
    );

    let first = extension_node(
        "extension.first",
        ["extension.second"],
        "work/extension/first.json",
    );
    let second = extension_node(
        "extension.second",
        ["extension.first"],
        "work/extension/second.json",
    );
    let error = compile_extension_project_plan(extension_request(vec![first, second]))
        .expect_err("cycle refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::Cycle);
}

#[test]
fn extension_dag_builder_rejects_incomplete_node_contracts() {
    let mut outputless = extension_node("extension.outputless", [], "work/extension/out.json");
    outputless.outputs.clear();
    let error = compile_extension_project_plan(extension_request(vec![outputless]))
        .expect_err("outputless node refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::ArtifactContract);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("at least one output"))
    );

    let mut conflicting = extension_node("extension.conflict", [], "work/extension/out.json");
    conflicting
        .content_hash_inputs
        .push(plan::ProjectPlanHashRef {
            ref_id: "extension.input".to_string(),
            content_hash: digest_bytes(b"different"),
        });
    let error = compile_extension_project_plan(extension_request(vec![conflicting]))
        .expect_err("conflicting hash ref refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::ArtifactContract);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("conflicting content hashes"))
    );

    let mut uppercase = extension_node("extension.uppercase", [], "work/extension/out.json");
    let digest = digest_bytes(b"uppercase");
    uppercase.content_hash_inputs[0].content_hash = format!(
        "blake3:{}",
        digest.trim_start_matches("blake3:").to_ascii_uppercase()
    );
    let error = compile_extension_project_plan(extension_request(vec![uppercase]))
        .expect_err("uppercase hex digest refuses");
    assert_eq!(error.code, ProjectPlanErrorCode::ArtifactContract);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("blake3 content hash"))
    );
}

fn extension_request(nodes: Vec<ProjectExtensionDagNode>) -> ProjectExtensionDagRequest {
    ProjectExtensionDagRequest::offline_read_only(
        "project.extension.test",
        digest_bytes(b"manifest"),
        digest_bytes(b"lock"),
        nodes,
    )
}

fn extension_node<const N: usize>(
    node_id: &str,
    dependencies: [&str; N],
    output_path: &str,
) -> ProjectExtensionDagNode {
    ProjectExtensionDagNode {
        node_id: node_id.to_string(),
        kind: ProjectPlanNodeKind::Evidence,
        class: ProjectPlanNodeClass::Computation,
        command: format!("canon extension run package.entry --node {node_id}"),
        dependencies: dependencies
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
        content_hash_inputs: vec![plan::ProjectPlanHashRef {
            ref_id: "extension.input".to_string(),
            content_hash: digest_bytes(format!("{node_id}:input").as_bytes()),
        }],
        outputs: vec![ProjectExtensionDagOutput {
            output_id: node_id.to_string(),
            path: output_path.to_string(),
            materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
        }],
        limits: BTreeMap::from([("max_rows".to_string(), 10)]),
        cache_eligible: true,
        side_effects: vec![ProjectPlanSideEffect {
            kind: ProjectPlanSideEffectKind::WritesArtifact,
            description: "writes a declared extension artifact".to_string(),
        }],
        refusal_conditions: vec![ProjectPlanRefusalCondition {
            code: ProjectPlanErrorCode::ArtifactContract,
            message: "extension artifact contract must validate".to_string(),
            next_command: Some(
                "canon project plan --manifest <MANIFEST> --lock <LOCK>".to_string(),
            ),
        }],
    }
}

fn minimal_manifest() -> ProjectManifest {
    load_project_manifest_toml(MINIMAL_TOML).expect("minimal manifest loads")
}

fn request(manifest: ProjectManifest, lock: ProjectLock) -> ProjectPlanRequest {
    ProjectPlanRequest::new(
        manifest,
        lock,
        PathBuf::from("tests/fixtures/project/minimal.toml"),
        PathBuf::from("tests/fixtures/project/minimal.lock.json"),
    )
}

trait WithPlanPath {
    fn with_plan_path(self, path: &str) -> Self;
}

impl WithPlanPath for ProjectPlanRequest {
    fn with_plan_path(mut self, path: &str) -> Self {
        self.plan_artifact_path = Some(PathBuf::from(path));
        self
    }
}

fn lock_for_manifest(manifest: &ProjectManifest) -> ProjectLock {
    let digest = project_manifest_digest(manifest).expect("manifest digest");
    refresh_project_lock(&ProjectLockManifestProjection {
        project_id: manifest.project_id.clone(),
        project_digest: digest,
        inputs: manifest
            .sources
            .iter()
            .map(|source| ProjectLockInput {
                input_id: source.source_id.clone(),
                relative_path: source.path.clone(),
                content_digest: digest_bytes(source.path.as_bytes()),
            })
            .collect(),
        resolved_refs: manifest
            .packages
            .iter()
            .map(|package| ProjectLockResolvedRef {
                ref_id: package.alias.clone(),
                kind: match package.kind {
                    ProjectPackageKind::Strategy => ProjectLockRefKind::Strategy,
                    ProjectPackageKind::Registry
                    | ProjectPackageKind::EntityProfile
                    | ProjectPackageKind::SourceMapping
                    | ProjectPackageKind::Extension => ProjectLockRefKind::Package,
                },
                resolved_digest: package.content_hash.clone(),
            })
            .collect(),
    })
    .expect("project lock builds")
}

fn node<'a>(plan: &'a plan::ProjectPlan, node_id: &str) -> &'a plan::ProjectPlanNode {
    plan.nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .expect("node exists")
}

fn node_mut<'a>(plan: &'a mut plan::ProjectPlan, node_id: &str) -> &'a mut plan::ProjectPlanNode {
    plan.nodes
        .iter_mut()
        .find(|node| node.node_id == node_id)
        .expect("node exists")
}

fn tree(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .expect("strip root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

fn link_manifest_toml() -> String {
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
path = "feeds/left.csv"
format = "csv"
mapping_package = "mapping"
mapping_profile = "pkg.synthetic:left"
required = true

[[sources]]
source_id = "right_feed"
path = "feeds/right.jsonl"
format = "jsonl"
mapping_package = "mapping"
mapping_profile = "pkg.synthetic:right"
required = true

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
mode_id = "link_cross"
kind = "link"
source_ids = ["left_feed", "right_feed"]
registry_package = "registry"
strategy_package = "strategy"
profile_package = "profile"
output_ids = ["summary", "review_queue"]

[[extensions]]
extension_id = "portable_export"
package = "extension"
entrypoint = "pkg.synthetic.extension:emit"
mode_ids = ["link_cross"]
config_path = "extensions/export.json"
"#
    .to_string()
}

fn log_plan(fixture: &str, graph_hash: &str, nodes: usize) {
    eprintln!("fixture={fixture} nodes={nodes} graph_hash={graph_hash}");
}
