#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::cli::Cli;
use canon::entity::run::link::multisource::ENTITY_MULTISOURCE_LINK_VERSION;
use canon::geo::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
    CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION, CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION,
    CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION,
    CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION, CANON_GEO_ADDRESS_PARSE_FOREST_VERSION,
    CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION, CANON_GEO_ADDRESS_QUERY_GRAMMAR_VERSION,
    CANON_GEO_CAPABILITIES_VERSION, CANON_GEO_COMPOSITION_PROFILE_VERSION,
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_COMPOSITION_VERSION,
    CANON_GEO_DISCOVERY_REQUEST_VERSION, CANON_GEO_ENTITY_PROJECTION_VERSION,
    CANON_GEO_EVIDENCE_COMPILATION_VERSION, CANON_GEO_EVIDENCE_REQUEST_VERSION,
    CANON_GEO_GEOMETRY_REQUEST_VERSION, CANON_GEO_GEOMETRY_TILE_VERSION,
    CANON_GEO_GEOMETRY_VALUE_VERSION, CANON_GEO_H7_POPULATION_ROWS_VERSION,
    CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
    CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
    CANON_GEO_LOCAL_FRAME_VERSION, CANON_GEO_MULTISOURCE_REQUEST_VERSION,
    CANON_GEO_PAD_ADDRESS_SET_VERSION, CANON_GEO_PAD_MEMBERSHIP_VERSION, CANON_GEO_PLAN_VERSION,
    CANON_GEO_POPULATION_EVALUATION_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    CANON_GEO_QUESTION_VERSION, CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION,
    CANON_GEO_REGIONAL_INVENTORY_VERSION, CANON_GEO_RESIDUAL_BENCHMARK_VERSION,
    CANON_GEO_RESIDUAL_OBDD_VERSION, CANON_GEO_RESOURCE_BUDGET_VERSION, CANON_GEO_RUN_VERSION,
    CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION, CANON_GEO_TILE_RECONCILIATION_VERSION,
    CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION,
    CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION, CANON_GEO_WAREHOUSE_GEOMETRY_VERSION,
    CANON_GEO_WAREHOUSE_ROWS_VERSION, GeoAbstentionDisposition, GeoAbstentionPolicy, GeoAsOf,
    GeoBoundedGeography, GeoBudgetAction, GeoCapabilities, GeoCapabilityStatus,
    GeoCapabilityStatusSets, GeoClaimClass, GeoCommandCapability, GeoContractCapability,
    GeoControlEntityLevel, GeoControlErrorCode, GeoControlProperty, GeoEgressClass,
    GeoEvidenceClass, GeoGeometryTransformContract, GeoIdentityParticipation,
    GeoInventorySupportStatus, GeoLicenseClass, GeoLocalAcquisitionState, GeoLocalArtifactRef,
    GeoNativeEntityScope, GeoNumericBound, GeoNumericMeasure, GeoQuestion, GeoRegionalInventory,
    GeoRegionalSourceInstance, GeoRequestedGrain, GeoResourceBudget, GeoResourceCounter,
    GeoSourceAvailability, GeoSourceRelease, GeoSubjectBinding, GeoSubjectBindingClass,
    GeoTelemetryDeclaration, GeoTelemetryMetric, GeoTelemetrySemanticEffect, GeoTemporalScope,
    GeoValueOrigin, canonical_capabilities_bytes, canonical_question_bytes,
    canonical_regional_inventory_bytes, canonical_resource_budget_bytes,
    capabilities_semantic_hash, default_geo_capabilities, evaluate_inventory_support,
    question_semantic_hash, regional_inventory_planning_hash, resource_budget_semantic_hash,
};
use clap::CommandFactory;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const QUESTION_SCHEMA: &str = include_str!("../schemas/canon.geo.question.v0.schema.json");
const CAPABILITIES_SCHEMA: &str = include_str!("../schemas/canon.geo.capabilities.v0.schema.json");
const INVENTORY_SCHEMA: &str =
    include_str!("../schemas/canon.geo.regional_inventory.v1.schema.json");
const BUDGET_SCHEMA: &str = include_str!("../schemas/canon.geo.resource_budget.v0.schema.json");

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn collect_leaf_paths(prefix: &str, command: &clap::Command, out: &mut BTreeSet<String>) {
    let subcommands = command
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help")
        .collect::<Vec<_>>();
    if subcommands.is_empty() {
        out.insert(prefix.to_string());
        return;
    }
    for subcommand in subcommands {
        collect_leaf_paths(
            &format!("{prefix} {}", subcommand.get_name()),
            subcommand,
            out,
        );
    }
}

fn geo_clap_leaf_paths() -> BTreeSet<String> {
    let root = Cli::command();
    let geo = root
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "geo")
        .expect("geo subcommand is compiled");
    let mut out = BTreeSet::new();
    for subcommand in geo
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help")
    {
        collect_leaf_paths(
            &format!("geo {}", subcommand.get_name()),
            subcommand,
            &mut out,
        );
    }
    out
}

fn command_leaf(command: &str, leafs: &BTreeSet<String>) -> Option<String> {
    let without_binary = command.strip_prefix("canon ")?;
    leafs
        .iter()
        .find(|leaf| {
            without_binary == leaf.as_str()
                || without_binary
                    .strip_prefix(leaf.as_str())
                    .is_some_and(|rest| rest.starts_with(' '))
        })
        .cloned()
}

fn contract_versions(contracts: &[GeoContractCapability]) -> BTreeSet<&str> {
    contracts
        .iter()
        .map(|contract| contract.contract_version.as_str())
        .collect()
}

fn expected_implemented_contracts() -> BTreeSet<&'static str> {
    BTreeSet::from([
        CANON_GEO_QUESTION_VERSION,
        CANON_GEO_CAPABILITIES_VERSION,
        CANON_GEO_REGIONAL_INVENTORY_VERSION,
        CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION,
        CANON_GEO_RESOURCE_BUDGET_VERSION,
        CANON_GEO_PLAN_VERSION,
        CANON_GEO_RUN_VERSION,
        CANON_GEO_DISCOVERY_REQUEST_VERSION,
        CANON_GEO_ACQUISITION_REQUEST_VERSION,
        CANON_GEO_ACQUISITION_RECEIPT_VERSION,
        CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION,
        CANON_GEO_ADDRESS_PARSE_FOREST_VERSION,
        CANON_GEO_ADDRESS_QUERY_GRAMMAR_VERSION,
        CANON_GEO_PAD_ADDRESS_SET_VERSION,
        CANON_GEO_PAD_MEMBERSHIP_VERSION,
        CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION,
        CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION,
        CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION,
        CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION,
        CANON_GEO_COMPOSITION_PROFILE_VERSION,
        CANON_GEO_COMPOSITION_REQUEST_VERSION,
        CANON_GEO_COMPOSITION_VERSION,
        CANON_GEO_ENTITY_PROJECTION_VERSION,
        CANON_GEO_GEOMETRY_REQUEST_VERSION,
        CANON_GEO_GEOMETRY_VALUE_VERSION,
        CANON_GEO_GEOMETRY_TILE_VERSION,
        CANON_GEO_LOCAL_FRAME_VERSION,
        CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
        CANON_GEO_WAREHOUSE_GEOMETRY_VERSION,
        CANON_GEO_HOME_CELL_ROWS_VERSION,
        CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION,
        CANON_GEO_TILE_WORK_UNIT_VERSION,
        CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
        CANON_GEO_TILE_RECONCILIATION_VERSION,
        CANON_GEO_MULTISOURCE_REQUEST_VERSION,
        ENTITY_MULTISOURCE_LINK_VERSION,
        CANON_GEO_EVIDENCE_REQUEST_VERSION,
        CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        CANON_GEO_WAREHOUSE_ROWS_VERSION,
        CANON_GEO_H7_POPULATION_ROWS_VERSION,
        CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
        CANON_GEO_H7_POPULATION_VERSION,
        CANON_GEO_POPULATION_REQUEST_VERSION,
        CANON_GEO_POPULATION_EVALUATION_VERSION,
    ])
}

fn expected_diagnostic_contracts() -> BTreeSet<&'static str> {
    BTreeSet::from([
        CANON_GEO_RESIDUAL_BENCHMARK_VERSION,
        CANON_GEO_RESIDUAL_OBDD_VERSION,
    ])
}

fn expected_implemented_commands() -> BTreeMap<&'static str, (&'static str, bool, bool)> {
    BTreeMap::from([
        (
            "canon geo capabilities --emit json",
            (CANON_GEO_CAPABILITIES_VERSION, true, false),
        ),
        (
            "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>",
            (CANON_GEO_PLAN_VERSION, true, false),
        ),
        (
            "canon geo run --plan <PLAN.json> --work-dir <DIR> [--input <NODE_ID:BINDING_ID=PATH>...] [--satisfy <REQUEST_ID=RECEIPT.json>...]",
            (CANON_GEO_RUN_VERSION, false, false),
        ),
        (
            "canon geo replan-from-acquisition --base-plan <PLAN.json> --base-inventory <INVENTORY.json> --question <QUESTION.json> --capabilities <CAPABILITIES.json> --profile <PROFILE.json> --budget <BUDGET.json> --satisfy <REQUEST_ID=RECEIPT.json> --local-artifact <LOCAL_ARTIFACT_ID=PATH>... [--result <DIGEST_ID=PATH>...] --advancement-out <ADVANCEMENT.json>",
            (CANON_GEO_PLAN_VERSION, false, false),
        ),
        (
            "canon geo link-sources --request <REQUEST.json> --rows-out <ROWS.csv>",
            (ENTITY_MULTISOURCE_LINK_VERSION, false, false),
        ),
        (
            "canon geo materialize-home-cells --rows <ROWS.json>",
            (CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, true, false),
        ),
        (
            "canon geo tile-work --request <REQUEST.json>",
            (CANON_GEO_TILE_WORK_UNIT_VERSION, true, false),
        ),
        (
            "canon geo reconcile-tiles --request <REQUEST.json>",
            (CANON_GEO_TILE_RECONCILIATION_VERSION, true, false),
        ),
        (
            "canon geo solve --request <REQUEST.json>",
            (CANON_GEO_COMPOSITION_VERSION, true, false),
        ),
        (
            "canon geo materialize-geometry --request <REQUEST.json>",
            (CANON_GEO_GEOMETRY_TILE_VERSION, true, false),
        ),
        (
            "canon geo materialize-warehouse-geometry --rows <ROWS.json>",
            (CANON_GEO_WAREHOUSE_GEOMETRY_VERSION, true, false),
        ),
        (
            "canon geo materialize-evidence --rows <ROWS.json>",
            (CANON_GEO_EVIDENCE_REQUEST_VERSION, true, false),
        ),
        (
            "canon geo materialize-address-evidence --request <REQUEST.json>",
            (
                CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION,
                true,
                false,
            ),
        ),
        (
            "canon geo materialize-h7-population --rows <ROWS.json>",
            (CANON_GEO_H7_POPULATION_VERSION, true, false),
        ),
        (
            "canon geo materialize-h7-staging-batch --batch <BATCH.json>",
            (CANON_GEO_H7_POPULATION_VERSION, true, false),
        ),
        (
            "canon geo compile-evidence --request <REQUEST.json>",
            (CANON_GEO_EVIDENCE_COMPILATION_VERSION, true, false),
        ),
        (
            "canon geo evaluate --population <POPULATION.json>",
            (CANON_GEO_POPULATION_EVALUATION_VERSION, true, false),
        ),
    ])
}

fn assert_command_status_buckets_are_disjoint(
    sets: &GeoCapabilityStatusSets<GeoCommandCapability>,
) {
    let mut seen = BTreeMap::new();
    for (bucket, commands) in [
        ("implemented", sets.implemented.as_slice()),
        ("diagnostic_only", sets.diagnostic_only.as_slice()),
        ("unavailable", sets.unavailable.as_slice()),
    ] {
        for command in commands {
            assert_eq!(
                seen.insert(command.command.as_str(), bucket),
                None,
                "command {} appears in more than one status bucket",
                command.command
            );
        }
    }
}

fn assert_contract_status_buckets_are_disjoint(
    sets: &GeoCapabilityStatusSets<GeoContractCapability>,
) {
    let mut seen = BTreeMap::new();
    for (bucket, expected_status, contracts) in [
        (
            "implemented",
            GeoCapabilityStatus::Implemented,
            sets.implemented.as_slice(),
        ),
        (
            "diagnostic_only",
            GeoCapabilityStatus::DiagnosticOnly,
            sets.diagnostic_only.as_slice(),
        ),
        (
            "unavailable",
            GeoCapabilityStatus::Unavailable,
            sets.unavailable.as_slice(),
        ),
    ] {
        for contract in contracts {
            assert_eq!(
                contract.status, expected_status,
                "contract {} status does not match {} bucket",
                contract.contract_version, bucket
            );
            assert_eq!(
                seen.insert(contract.contract_version.as_str(), bucket),
                None,
                "contract {} appears in more than one status bucket",
                contract.contract_version
            );
        }
    }
}

fn digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.no-parcel".to_string(),
        geography_kind: "declared_test_region".to_string(),
        description: "Parcel-free fixture region".to_string(),
    }
}

fn as_of(day: &str) -> GeoAsOf {
    GeoAsOf {
        utc_day: day.to_string(),
        semantic_id: "question.query_as_of.utc_day".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    }
}

fn abstention_policy() -> GeoAbstentionPolicy {
    GeoAbstentionPolicy {
        unsupported_grain: GeoAbstentionDisposition::ReportUnsupported,
        unresolved_residual: GeoAbstentionDisposition::ReportResidual,
        budget_fallback: GeoAbstentionDisposition::ReportResidual,
    }
}

fn building_question() -> GeoQuestion {
    GeoQuestion {
        version: CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: "question.fixture.building".to_string(),
        subject_bindings: vec![
            GeoSubjectBinding {
                role: "operator_case".to_string(),
                binding_class: GeoSubjectBindingClass::OperatorLabel,
                value: "case-building".to_string(),
            },
            GeoSubjectBinding {
                role: "input_address".to_string(),
                binding_class: GeoSubjectBindingClass::AddressText,
                value: "10 Fixture St".to_string(),
            },
        ],
        bounded_geography: region(),
        requested_grains: vec![GeoRequestedGrain {
            entity_level: GeoControlEntityLevel::Building,
            required_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            optional_evidence_classes: vec![GeoEvidenceClass::AddressSet],
        }],
        query_as_of: None,
        requested_claim_classes: vec![GeoClaimClass::StableIdentity, GeoClaimClass::CandidateReach],
        presentation_limits: vec![GeoNumericBound {
            semantic_id: "question.presentation.max_models".to_string(),
            counter: GeoResourceCounter::Models,
            value: 16,
            unit: "model".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::TruncatePresentationOnly,
        }],
        abstention_policy: abstention_policy(),
        decision_policy: None,
        resource_budget_ref: "budget.fixture.control".to_string(),
    }
}

fn building_and_parcel_question() -> GeoQuestion {
    let mut question = building_question();
    question.question_id = "question.fixture.building-and-parcel".to_string();
    question.requested_grains.push(GeoRequestedGrain {
        entity_level: GeoControlEntityLevel::Parcel,
        required_evidence_classes: vec![GeoEvidenceClass::ParcelGeometry],
        optional_evidence_classes: vec![GeoEvidenceClass::AddressSet],
    });
    question
}

fn budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.control".to_string(),
        deterministic_bounds: vec![
            GeoNumericBound {
                semantic_id: "budget.max_models".to_string(),
                counter: GeoResourceCounter::Models,
                value: 16,
                unit: "model".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::TruncatePresentationOnly,
            },
            GeoNumericBound {
                semantic_id: "budget.max_rows".to_string(),
                counter: GeoResourceCounter::Rows,
                value: 100,
                unit: "row".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::RefuseBeforeWork,
            },
        ],
        telemetry: vec![GeoTelemetryDeclaration {
            metric: GeoTelemetryMetric::WallTime,
            unit: "millisecond".to_string(),
            origin: GeoValueOrigin::OperatorPolicy,
            semantic_effect: GeoTelemetrySemanticEffect::None,
        }],
    }
}

fn source(
    source_instance_id: &str,
    entity_level: GeoControlEntityLevel,
    evidence_classes: Vec<GeoEvidenceClass>,
) -> GeoRegionalSourceInstance {
    GeoRegionalSourceInstance {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: "release.fixture.shared".to_string(),
            release_digest: digest("release.fixture.shared"),
        },
        temporal_scope: GeoTemporalScope {
            valid_time: None,
            transaction_time: None,
            release_time: None,
        },
        lineage_ids: vec!["lineage.fixture.shared".to_string()],
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level,
            identity_participation: GeoIdentityParticipation::StableAlias,
        },
        evidence_classes,
        coverage: canon::geo::GeoCoveragePredicate {
            coverage_id: "coverage.fixture.no-parcel".to_string(),
            region: region(),
            predicate: "all declared records in the fixture region".to_string(),
        },
        local_state: GeoLocalAcquisitionState {
            state: GeoSourceAvailability::Available,
            local_ref: Some(GeoLocalArtifactRef {
                artifact_id: format!("local.{source_instance_id}"),
                contract_version: "canon_geo_warehouse_rows.v0".to_string(),
                content_hash: digest("local.fixture.shared"),
                media_type: "application/json".to_string(),
            }),
        },
        geometry: None,
        license_class: GeoLicenseClass::PublicRedistributable,
        egress_class: GeoEgressClass::Shareable,
        estimates: vec![GeoNumericMeasure {
            semantic_id: "source.estimated_rows".to_string(),
            value: 2,
            unit: "row".to_string(),
            origin: GeoValueOrigin::SourceRelease,
        }],
    }
}

fn no_parcel_inventory(source_instance_id: &str) -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.no-parcel".to_string(),
        region: region(),
        sources: vec![source(
            source_instance_id,
            GeoControlEntityLevel::Building,
            vec![GeoEvidenceClass::BuildingFootprint],
        )],
        discovery_gaps: Vec::new(),
    }
}

#[test]
fn geo_capabilities_cli_emits_deterministic_offline_contract() {
    let first = canon_command()
        .args(["geo", "capabilities", "--emit", "json"])
        .assert()
        .success();
    let second = canon_command()
        .args(["geo", "capabilities", "--emit", "json"])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
    assert!(first.get_output().stderr.is_empty());
    let stdout = String::from_utf8(first.get_output().stdout.clone()).expect("utf-8 stdout");
    assert!(stdout.ends_with('\n'));
    assert!(
        !stdout.to_ascii_lowercase().contains("mcp"),
        "capabilities must not claim an MCP-backed surface"
    );

    let artifact: GeoCapabilities =
        serde_json::from_str(stdout.trim_end()).expect("capabilities JSON parses");
    assert_eq!(artifact.version, CANON_GEO_CAPABILITIES_VERSION);
    assert_eq!(artifact.next_command, "canon --describe");
    assert!(artifact.runtime_side_effects.read_only);
    assert!(!artifact.runtime_side_effects.reads_input_files);
    assert!(!artifact.runtime_side_effects.reads_catalog);
    assert!(!artifact.runtime_side_effects.writes_files);
    assert!(!artifact.runtime_side_effects.uses_network);
    assert_eq!(
        artifact.semantic_hash,
        capabilities_semantic_hash(&artifact).expect("semantic hash recomputes")
    );

    let implemented_contracts = artifact
        .contracts
        .implemented
        .iter()
        .map(|contract| contract.contract_version.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for expected in [
        CANON_GEO_QUESTION_VERSION,
        CANON_GEO_CAPABILITIES_VERSION,
        CANON_GEO_REGIONAL_INVENTORY_VERSION,
        CANON_GEO_RESOURCE_BUDGET_VERSION,
    ] {
        assert!(implemented_contracts.contains(expected));
    }
    assert!(artifact.commands.implemented.iter().any(|command| {
        command.command
            == "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>"
            && command.output_contract == CANON_GEO_PLAN_VERSION
    }));
    let confluence = artifact
        .properties
        .iter()
        .find(|property| property.property == GeoControlProperty::Confluent)
        .expect("confluence property row");
    assert_eq!(confluence.status, GeoCapabilityStatus::DiagnosticOnly);
    assert!(
        confluence
            .basis
            .contains("no join/solver confluence guarantee"),
        "order-invariant canonicalization must not be presented as confluence"
    );
}

#[test]
fn geo_capabilities_cover_compiled_leaf_commands_and_public_contracts() {
    let artifact = default_geo_capabilities().expect("default capabilities");
    assert_command_status_buckets_are_disjoint(&artifact.commands);
    assert_contract_status_buckets_are_disjoint(&artifact.contracts);

    let expected_commands = expected_implemented_commands();
    let actual_commands = artifact
        .commands
        .implemented
        .iter()
        .map(|command| {
            (
                command.command.as_str(),
                (
                    command.output_contract.as_str(),
                    command.read_only,
                    command.uses_network,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual_commands, expected_commands);

    let clap_leafs = geo_clap_leaf_paths();
    let implemented_leafs = artifact
        .commands
        .implemented
        .iter()
        .map(|command| {
            command_leaf(&command.command, &clap_leafs)
                .unwrap_or_else(|| panic!("{} is not a compiled Geo Clap leaf", command.command))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        implemented_leafs, clap_leafs,
        "every compiled canon geo leaf must be present in canon_geo_capabilities.v0"
    );

    let implemented_contracts = contract_versions(&artifact.contracts.implemented);
    assert_eq!(implemented_contracts, expected_implemented_contracts());
    let diagnostic_contracts = contract_versions(&artifact.contracts.diagnostic_only);
    assert_eq!(diagnostic_contracts, expected_diagnostic_contracts());
    assert!(artifact.contracts.unavailable.is_empty());

    for command in &artifact.commands.implemented {
        assert!(
            implemented_contracts.contains(command.output_contract.as_str()),
            "{} outputs an unlisted contract {}",
            command.command,
            command.output_contract
        );
    }

    let unavailable_commands = artifact
        .commands
        .unavailable
        .iter()
        .map(|command| {
            (
                command.command.as_str(),
                (
                    command.output_contract.as_str(),
                    command.read_only,
                    command.uses_network,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        unavailable_commands,
        BTreeMap::from([(
            "canon geo inspect",
            ("planned_not_implemented", true, false),
        )])
    );
    for command in &artifact.commands.unavailable {
        assert!(
            command_leaf(&command.command, &clap_leafs).is_none(),
            "{} must not be both compiled and unavailable",
            command.command
        );
    }
}

#[test]
fn reordered_question_inventory_and_budget_have_identical_canonical_bytes() {
    let mut question_a = building_and_parcel_question();
    question_a.subject_bindings.reverse();
    question_a.requested_grains.reverse();
    question_a.requested_claim_classes.reverse();
    let mut question_b = building_and_parcel_question();
    question_b.requested_grains[0]
        .optional_evidence_classes
        .reverse();
    assert_eq!(
        canonical_question_bytes(&question_a).expect("question a canonicalizes"),
        canonical_question_bytes(&question_b).expect("question b canonicalizes")
    );
    assert_eq!(
        question_semantic_hash(&question_a).expect("question a hashes"),
        question_semantic_hash(&question_b).expect("question b hashes")
    );

    let mut inventory_a = no_parcel_inventory("arbitrary-building-source-a");
    inventory_a.sources.push(source(
        "arbitrary-address-source",
        GeoControlEntityLevel::Building,
        vec![
            GeoEvidenceClass::AddressSet,
            GeoEvidenceClass::AssertedAttribute,
        ],
    ));
    let mut inventory_b = inventory_a.clone();
    inventory_b.sources.reverse();
    inventory_b.sources[0].lineage_ids.reverse();
    inventory_b.sources[0].evidence_classes.reverse();
    assert_eq!(
        canonical_regional_inventory_bytes(&inventory_a).expect("inventory a canonicalizes"),
        canonical_regional_inventory_bytes(&inventory_b).expect("inventory b canonicalizes")
    );

    let mut budget_a = budget();
    budget_a.deterministic_bounds.reverse();
    let budget_b = budget();
    assert_eq!(
        canonical_resource_budget_bytes(&budget_a).expect("budget a canonicalizes"),
        canonical_resource_budget_bytes(&budget_b).expect("budget b canonicalizes")
    );
    assert_eq!(
        resource_budget_semantic_hash(&budget_a).expect("budget a hashes"),
        resource_budget_semantic_hash(&budget_b).expect("budget b hashes")
    );
}

#[test]
fn arbitrary_source_names_do_not_change_planning_signature() {
    let question = building_question();
    let budget = budget();
    let inventory_a = no_parcel_inventory("arbitrary-building-source-a");
    let inventory_b = no_parcel_inventory("renamed-building-source-z");

    assert_ne!(
        canonical_regional_inventory_bytes(&inventory_a).expect("inventory a canonicalizes"),
        canonical_regional_inventory_bytes(&inventory_b).expect("inventory b canonicalizes"),
        "inventory identity still records the declared source instance id"
    );
    assert_eq!(
        regional_inventory_planning_hash(&inventory_a).expect("inventory a planning hash"),
        regional_inventory_planning_hash(&inventory_b).expect("inventory b planning hash"),
        "source instance names and local artifact labels are not planning semantics"
    );

    let report_a = evaluate_inventory_support(&question, &inventory_a, &budget)
        .expect("inventory a evaluates");
    let report_b = evaluate_inventory_support(&question, &inventory_b, &budget)
        .expect("inventory b evaluates");
    assert_eq!(
        report_a.inventory_planning_hash,
        report_b.inventory_planning_hash
    );
    assert_eq!(report_a.grain_support, report_b.grain_support);
}

#[test]
fn local_artifact_contract_version_changes_inventory_planning_identity() {
    let inventory_a = no_parcel_inventory("arbitrary-building-source-a");
    let mut inventory_b = inventory_a.clone();
    inventory_b.sources[0]
        .local_state
        .local_ref
        .as_mut()
        .expect("available source has a local ref")
        .contract_version = "canon_geo_warehouse_rows.v1".to_string();

    assert_ne!(
        regional_inventory_planning_hash(&inventory_a).expect("inventory a planning hash"),
        regional_inventory_planning_hash(&inventory_b).expect("inventory b planning hash"),
        "artifact contract changes must invalidate plans that bind those local bytes"
    );
}

#[test]
fn local_artifact_contract_must_be_canonical_and_currently_usable() {
    let mut malformed = no_parcel_inventory("malformed-contract-source");
    malformed.sources[0]
        .local_state
        .local_ref
        .as_mut()
        .expect("available source has a local ref")
        .contract_version = "warehouse rows latest".to_string();
    let error = canonical_regional_inventory_bytes(&malformed)
        .expect_err("non-versioned local contracts must fail closed");
    assert_eq!(error.code, GeoControlErrorCode::InvalidInput);

    let mut unsupported = no_parcel_inventory("unsupported-contract-source");
    unsupported.sources[0]
        .local_state
        .local_ref
        .as_mut()
        .expect("available source has a local ref")
        .contract_version = "canon_geo_unknown_rows.v9".to_string();
    let report = evaluate_inventory_support(&building_question(), &unsupported, &budget())
        .expect("canonical but unusable contract remains an unsupported inventory finding");
    assert_eq!(report.status, GeoInventorySupportStatus::Unsupported);
    assert_eq!(
        report.grain_support[0].missing_evidence_classes,
        vec![GeoEvidenceClass::BuildingFootprint]
    );
}

#[test]
fn no_parcel_inventory_reports_supported_and_unsupported_grains_separately() {
    let report = evaluate_inventory_support(
        &building_and_parcel_question(),
        &no_parcel_inventory("arbitrary-building-source-a"),
        &budget(),
    )
    .expect("parcel-free inventory is valid");

    assert_eq!(report.status, GeoInventorySupportStatus::Unsupported);
    let building = report
        .grain_support
        .iter()
        .find(|support| support.entity_level == GeoControlEntityLevel::Building)
        .expect("building support row");
    assert_eq!(building.status, GeoInventorySupportStatus::Supported);
    assert_eq!(
        building.satisfied_evidence_classes,
        vec![GeoEvidenceClass::BuildingFootprint]
    );

    let parcel = report
        .grain_support
        .iter()
        .find(|support| support.entity_level == GeoControlEntityLevel::Parcel)
        .expect("parcel support row");
    assert_eq!(parcel.status, GeoInventorySupportStatus::Unsupported);
    assert_eq!(
        parcel.missing_evidence_classes,
        vec![GeoEvidenceClass::ParcelGeometry]
    );
    assert_eq!(report.discovery_gaps.len(), 1);
}

#[test]
fn available_source_lacking_requested_evidence_class_does_not_satisfy_gate() {
    let mut inventory = no_parcel_inventory("arbitrary-parcel-name-without-parcel-geometry");
    inventory.sources[0] = source(
        "arbitrary-parcel-name-without-parcel-geometry",
        GeoControlEntityLevel::Parcel,
        vec![GeoEvidenceClass::AddressSet],
    );
    let mut question = building_question();
    question.requested_grains = vec![GeoRequestedGrain {
        entity_level: GeoControlEntityLevel::Parcel,
        required_evidence_classes: vec![GeoEvidenceClass::ParcelGeometry],
        optional_evidence_classes: Vec::new(),
    }];

    let report = evaluate_inventory_support(&question, &inventory, &budget())
        .expect("support gate evaluates");
    assert_eq!(report.status, GeoInventorySupportStatus::Unsupported);
    assert_eq!(
        report.grain_support[0].missing_evidence_classes,
        vec![GeoEvidenceClass::ParcelGeometry]
    );
    assert!(
        report.grain_support[0]
            .satisfied_evidence_classes
            .is_empty()
    );
}

#[test]
fn resource_budget_ref_must_match_supplied_budget_id() {
    let mut wrong_budget = budget();
    wrong_budget.budget_id = "budget.fixture.other".to_string();

    let error = evaluate_inventory_support(
        &building_question(),
        &no_parcel_inventory("arbitrary-building-source-a"),
        &wrong_budget,
    )
    .expect_err("mismatched budget binding refuses");
    assert_eq!(error.code, GeoControlErrorCode::InvalidInput);
    assert_eq!(
        error.detail["resource_budget_ref"],
        "budget.fixture.control"
    );
    assert_eq!(error.detail["budget_id"], "budget.fixture.other");
}

#[test]
fn mismatched_inventory_region_cannot_satisfy_question_geography() {
    let mut inventory = no_parcel_inventory("arbitrary-building-source-a");
    inventory.region.geography_id = "region.fixture.other".to_string();
    inventory.region.description = "Different declared fixture region".to_string();

    let report = evaluate_inventory_support(&building_question(), &inventory, &budget())
        .expect("region mismatch is an unsupported inventory result");
    assert_eq!(report.status, GeoInventorySupportStatus::Unsupported);
    assert_eq!(
        report.grain_support[0].missing_evidence_classes,
        vec![GeoEvidenceClass::BuildingFootprint]
    );
    assert!(
        report.discovery_gaps[0].reason.contains("inventory region"),
        "gap explains that the inventory region does not bind to the question"
    );
}

#[test]
fn observation_only_source_does_not_satisfy_native_grain_support() {
    let mut inventory = no_parcel_inventory("observation-only-footprint-source");
    inventory.sources[0].native_scope = GeoNativeEntityScope::ObservationOnly;

    let report = evaluate_inventory_support(&building_question(), &inventory, &budget())
        .expect("support gate evaluates");
    assert_eq!(report.status, GeoInventorySupportStatus::Unsupported);
    assert_eq!(
        report.grain_support[0].status,
        GeoInventorySupportStatus::Unsupported
    );
    assert_eq!(
        report.grain_support[0].missing_evidence_classes,
        vec![GeoEvidenceClass::BuildingFootprint]
    );
    assert_eq!(report.discovery_gaps.len(), 1);
}

#[test]
fn evidence_only_native_source_supports_non_identity_evidence_but_not_stable_identity() {
    let mut inventory = no_parcel_inventory("evidence-only-building-source");
    inventory.sources[0].native_scope = GeoNativeEntityScope::NativeEntity {
        entity_level: GeoControlEntityLevel::Building,
        identity_participation: GeoIdentityParticipation::EvidenceOnly,
    };

    let stable_identity_report =
        evaluate_inventory_support(&building_question(), &inventory, &budget())
            .expect("stable identity support report");
    assert_eq!(
        stable_identity_report.status,
        GeoInventorySupportStatus::Unsupported
    );
    assert_eq!(
        stable_identity_report.grain_support[0].missing_evidence_classes,
        vec![GeoEvidenceClass::BuildingFootprint]
    );
    assert!(
        stable_identity_report.discovery_gaps[0]
            .reason
            .contains("stable-alias participation")
    );

    let mut non_identity_question = building_question();
    non_identity_question
        .requested_claim_classes
        .retain(|claim| *claim != GeoClaimClass::StableIdentity);
    let non_identity_report =
        evaluate_inventory_support(&non_identity_question, &inventory, &budget())
            .expect("evidence-only native source remains eligible non-identity evidence");
    assert_eq!(
        non_identity_report.status,
        GeoInventorySupportStatus::Supported
    );
    assert!(
        !inventory.sources[0]
            .native_scope
            .may_contribute_stable_alias(),
        "a geometry-derived source locator must never become a registry alias"
    );

    inventory.sources[0].native_scope = GeoNativeEntityScope::ObservationOnly;
    assert!(
        !inventory.sources[0]
            .native_scope
            .may_contribute_stable_alias(),
        "an observation-only source is non-native and cannot contribute an alias"
    );

    let stable_source = source(
        "stable-id-building-source",
        GeoControlEntityLevel::Building,
        vec![GeoEvidenceClass::BuildingFootprint],
    );
    assert!(
        stable_source.native_scope.may_contribute_stable_alias(),
        "stable alias participation must remain an explicit, separate declaration"
    );
    inventory.sources[0] = stable_source;
    let stable_identity_report =
        evaluate_inventory_support(&building_question(), &inventory, &budget())
            .expect("stable alias source supports stable identity evidence");
    assert_eq!(
        stable_identity_report.status,
        GeoInventorySupportStatus::Supported
    );
}

#[test]
fn time_scoped_sources_require_query_as_of_inside_valid_interval() {
    let mut inventory = no_parcel_inventory("time-scoped-building-source");
    inventory.sources[0].temporal_scope.valid_time = Some(canon::geo::GeoDateInterval {
        start_utc_day: "2026-01-01".to_string(),
        end_utc_day: "2026-12-31".to_string(),
    });

    let error = evaluate_inventory_support(&building_question(), &inventory, &budget())
        .expect_err("time-scoped source cannot satisfy a timeless question");
    assert_eq!(error.code, GeoControlErrorCode::MissingQueryAsOf);
    assert_eq!(error.detail["field"], "query_as_of");

    for day in ["2026-01-01", "2026-12-31"] {
        let mut boundary_question = building_question();
        boundary_question.query_as_of = Some(as_of(day));
        let report = evaluate_inventory_support(&boundary_question, &inventory, &budget())
            .expect("boundary date evaluates");
        assert_eq!(
            report.status,
            GeoInventorySupportStatus::Supported,
            "{day} should be included in the source valid_time interval"
        );
    }

    let mut out_of_interval_question = building_question();
    out_of_interval_question.query_as_of = Some(as_of("2027-01-01"));
    let report = evaluate_inventory_support(&out_of_interval_question, &inventory, &budget())
        .expect("out-of-interval date is an unsupported result, not a refusal");
    assert_eq!(report.status, GeoInventorySupportStatus::Unsupported);
    assert_eq!(
        report.grain_support[0].missing_evidence_classes,
        vec![GeoEvidenceClass::BuildingFootprint]
    );
    assert_eq!(report.discovery_gaps.len(), 1);
    assert!(
        report.discovery_gaps[0].reason.contains("outside"),
        "discovery gap should explain valid_time interval miss"
    );
}

#[test]
fn transaction_and_release_time_alone_do_not_force_world_query_as_of() {
    let mut inventory = no_parcel_inventory("provenance-clock-building-source");
    inventory.sources[0].temporal_scope.transaction_time = Some(canon::geo::GeoDateInterval {
        start_utc_day: "2024-01-01".to_string(),
        end_utc_day: "2024-12-31".to_string(),
    });
    inventory.sources[0].temporal_scope.release_time = Some(as_of("2025-01-15"));

    // valid_time scopes world truth and must contain query_as_of. Transaction
    // and release time are provenance/vintage clocks; they do not by
    // themselves make a timeless stable-identity question temporal.
    let report = evaluate_inventory_support(&building_question(), &inventory, &budget())
        .expect("provenance clocks alone do not require query_as_of");
    assert_eq!(report.status, GeoInventorySupportStatus::Supported);
}

#[test]
fn invalid_as_of_refuses() {
    let mut bad_question = building_question();
    bad_question.query_as_of = Some(as_of("2026-02-31"));
    let error = canonical_question_bytes(&bad_question).expect_err("invalid as-of date refuses");
    assert_eq!(error.code, GeoControlErrorCode::InvalidAsOf);
}

#[test]
fn capability_status_sets_reject_cross_bucket_duplicates() {
    let mut capabilities = default_geo_capabilities().expect("default capabilities");
    let duplicate = capabilities.vocabularies.solver_backends.implemented[0].clone();
    capabilities
        .vocabularies
        .solver_backends
        .unavailable
        .push(duplicate);

    let error = canonical_capabilities_bytes(&capabilities)
        .expect_err("cross-status capability duplicates must fail");
    assert_eq!(error.code, GeoControlErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "vocabularies.solver_backends");

    let mut capabilities = default_geo_capabilities().expect("default capabilities");
    let mut duplicate_command = capabilities.commands.implemented[0].clone();
    duplicate_command.output_contract = "planned_not_implemented".to_string();
    capabilities.commands.unavailable.push(duplicate_command);
    let error = canonical_capabilities_bytes(&capabilities)
        .expect_err("same command in two status buckets must fail even if metadata differs");
    assert_eq!(error.code, GeoControlErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "commands");

    let mut capabilities = default_geo_capabilities().expect("default capabilities");
    let mut duplicate_contract = capabilities.contracts.implemented[0].clone();
    duplicate_contract.status = GeoCapabilityStatus::Unavailable;
    duplicate_contract.schema_path = "schemas/changed-path.schema.json".to_string();
    capabilities.contracts.unavailable.push(duplicate_contract);
    let error = canonical_capabilities_bytes(&capabilities).expect_err(
        "same contract version in two status buckets must fail even if metadata differs",
    );
    assert_eq!(error.code, GeoControlErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "contracts");
}

#[test]
fn control_contract_roots_reject_unknown_fields_on_deserialize() {
    assert_root_unknown_field_rejected::<GeoQuestion>(
        serde_json::to_value(building_question()).expect("question serializes"),
    );
    assert_root_unknown_field_rejected::<GeoCapabilities>(
        serde_json::to_value(default_geo_capabilities().expect("capabilities build"))
            .expect("capabilities serializes"),
    );
    assert_root_unknown_field_rejected::<GeoRegionalInventory>(
        serde_json::to_value(no_parcel_inventory("arbitrary-building-source-a"))
            .expect("inventory serializes"),
    );
    assert_root_unknown_field_rejected::<GeoResourceBudget>(
        serde_json::to_value(budget()).expect("budget serializes"),
    );
}

fn assert_root_unknown_field_rejected<T>(mut value: Value)
where
    T: serde::de::DeserializeOwned + std::fmt::Debug,
{
    value
        .as_object_mut()
        .expect("root value is object")
        .insert("unknown_control_field".to_string(), json!(true));
    serde_json::from_value::<T>(value).expect_err("unknown root field must be rejected");
}

#[test]
fn regional_inventory_schema_declares_runtime_local_state_requirements() {
    let schema: Value = serde_json::from_str(INVENTORY_SCHEMA).expect("inventory schema parses");
    assert_eq!(
        schema.pointer("/$defs/local_state/allOf/0/if/properties/state/enum"),
        Some(&json!(["available", "partial"])),
        "available/partial branch must be explicit"
    );
    assert_eq!(
        schema.pointer("/$defs/local_state/allOf/0/then/required"),
        Some(&json!(["local_ref"])),
        "available/partial sources require a local artifact reference"
    );
    assert_eq!(
        schema.pointer("/$defs/local_state/allOf/1/if/properties/state/enum"),
        Some(&json!(["missing", "discovery_required", "unreadable"])),
        "unavailable branches must be explicit"
    );
    assert_eq!(
        schema.pointer("/$defs/local_state/allOf/1/then/properties/local_ref/type"),
        Some(&json!("null")),
        "missing/discovery/unreadable sources must not carry a local artifact object"
    );
    assert_eq!(
        schema.pointer("/$defs/local_artifact_ref/required"),
        Some(&json!([
            "artifact_id",
            "contract_version",
            "content_hash",
            "media_type"
        ])),
        "reusable local refs must preserve the typed artifact contract"
    );
    assert_eq!(
        schema.pointer("/$defs/local_artifact_ref/properties/contract_version/maxLength"),
        Some(&json!(128)),
        "local artifact contracts use the same bounded canonical identifier surface as run inputs"
    );
    assert_eq!(
        schema.pointer("/$defs/local_artifact_ref/properties/contract_version/pattern"),
        Some(&json!("^(canon_|canon\\.)[A-Za-z0-9_.-]+\\.v[0-9]+$")),
        "inventory availability cannot be asserted with an unversioned contract label"
    );
}

#[test]
fn regional_inventory_schema_requires_explicit_native_identity_participation() {
    let schema: Value = serde_json::from_str(INVENTORY_SCHEMA).expect("inventory schema parses");
    assert_eq!(
        schema.pointer("/$defs/identity_participation/enum"),
        Some(&json!(["stable_alias", "evidence_only"])),
        "native sources must distinguish stable aliases from evidence-only participation"
    );
    assert_eq!(
        schema.pointer("/$defs/native_scope/oneOf/0/required"),
        Some(&json!(["kind", "entity_level", "identity_participation"])),
        "native sources cannot omit their identity participation"
    );
    assert_eq!(
        schema.pointer("/$defs/native_scope/oneOf/1/additionalProperties"),
        Some(&json!(false)),
        "observation-only sources cannot smuggle native identity participation"
    );

    let mut missing_declaration =
        serde_json::to_value(no_parcel_inventory("missing-identity-declaration"))
            .expect("inventory serializes");
    missing_declaration["sources"][0]["native_scope"]
        .as_object_mut()
        .expect("native scope object")
        .remove("identity_participation");
    serde_json::from_value::<GeoRegionalInventory>(missing_declaration)
        .expect_err("native sources must explicitly declare identity participation");

    let mut observation_with_alias =
        serde_json::to_value(no_parcel_inventory("observation-with-alias"))
            .expect("inventory serializes");
    observation_with_alias["sources"][0]["native_scope"] = json!({
        "kind": "observation_only",
        "identity_participation": "stable_alias"
    });
    serde_json::from_value::<GeoRegionalInventory>(observation_with_alias)
        .expect_err("observation-only sources cannot declare stable alias participation");
}

#[test]
fn schema_examples_parse_and_canonicalize() {
    assert_schema_examples(
        QUESTION_SCHEMA,
        "canon.geo.question.v0",
        CANON_GEO_QUESTION_VERSION,
        |example| {
            let question: GeoQuestion =
                serde_json::from_value(example.clone()).expect("question example parses");
            canonical_question_bytes(&question).expect("question example canonicalizes");
        },
    );
    assert_schema_examples(
        CAPABILITIES_SCHEMA,
        "canon.geo.capabilities.v0",
        CANON_GEO_CAPABILITIES_VERSION,
        |example| {
            let capabilities: GeoCapabilities =
                serde_json::from_value(example.clone()).expect("capabilities example parses");
            canonical_capabilities_bytes(&capabilities)
                .expect("capabilities example canonicalizes");
        },
    );
    assert_schema_examples(
        INVENTORY_SCHEMA,
        "canon.geo.regional_inventory.v1",
        CANON_GEO_REGIONAL_INVENTORY_VERSION,
        |example| {
            let inventory: GeoRegionalInventory =
                serde_json::from_value(example.clone()).expect("inventory example parses");
            canonical_regional_inventory_bytes(&inventory)
                .expect("inventory example canonicalizes");
        },
    );
    assert_schema_examples(
        BUDGET_SCHEMA,
        "canon.geo.resource_budget.v0",
        CANON_GEO_RESOURCE_BUDGET_VERSION,
        |example| {
            let budget: GeoResourceBudget =
                serde_json::from_value(example.clone()).expect("budget example parses");
            canonical_resource_budget_bytes(&budget).expect("budget example canonicalizes");
        },
    );

    let default_capabilities = default_geo_capabilities().expect("default capabilities");
    canonical_capabilities_bytes(&default_capabilities)
        .expect("default capabilities follow their schema-facing contract");
}

fn assert_schema_examples(
    schema_source: &str,
    title: &str,
    version: &str,
    check_example: impl Fn(&Value),
) {
    let schema: Value = serde_json::from_str(schema_source).expect("schema parses");
    assert_eq!(schema["title"], title);
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["version"]["const"], version);
    let examples = schema["examples"]
        .as_array()
        .expect("schema examples array");
    assert!(!examples.is_empty(), "{title} needs at least one example");
    for example in examples {
        check_example(example);
    }
}

#[test]
fn geometry_contract_numeric_bounds_keep_unit_and_origin() {
    let mut inventory = no_parcel_inventory("geometry-source");
    inventory.sources[0].geometry = Some(GeoGeometryTransformContract {
        geometry_contract_version: "fixture.geometry.v0".to_string(),
        coordinate_reference_system: "LOCAL:FIXTURE".to_string(),
        transform_id: "fixture-transform".to_string(),
        transform_digest: digest("fixture-transform"),
        numeric_error_bounds: vec![GeoNumericMeasure {
            semantic_id: "geometry.decoder_loss".to_string(),
            value: 0,
            unit: "micrometre".to_string(),
            origin: GeoValueOrigin::AdapterContract,
        }],
    });
    canonical_regional_inventory_bytes(&inventory)
        .expect("geometry numeric bounds include explicit unit and origin");
}
