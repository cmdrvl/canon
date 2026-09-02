#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_QUESTION_VERSION, CANON_GEO_REGIONAL_INVENTORY_VERSION,
    CANON_GEO_RESOURCE_BUDGET_VERSION, GEO_CLIENT_SIX_FIELD_PROFILE_TEMPLATE_ID,
    GeoAbstentionDisposition, GeoAbstentionPolicy, GeoBoundedGeography, GeoBudgetAction,
    GeoClaimClass, GeoClientDecisionBand, GeoClientDisagreementEffect, GeoClientInputChannelRole,
    GeoClientInputContract, GeoClientInputDeclaration, GeoClientInputField,
    GeoClientInputFixtureKind, GeoClientInputPresence, GeoCompositionErrorCode,
    GeoCompositionProfile, GeoControlEntityLevel, GeoCoveragePredicate, GeoEgressClass,
    GeoEvidenceClass, GeoGeometryTransformContract, GeoIdentityParticipation, GeoLicenseClass,
    GeoLocalAcquisitionState, GeoLocalArtifactRef, GeoNativeEntityScope, GeoNumericBound,
    GeoNumericMeasure, GeoPlanErrorCode, GeoPlanRequest, GeoPlanStatus, GeoRegionalInventory,
    GeoRegionalSourceInstance, GeoResourceBudget, GeoResourceCounter, GeoSourceAvailability,
    GeoSourceRelease, GeoSubjectBinding, GeoSubjectBindingClass, GeoTelemetryDeclaration,
    GeoTelemetryMetric, GeoTelemetrySemanticEffect, GeoTemporalScope, GeoValueOrigin,
    compile_geo_plan, default_geo_capabilities, validate_composition_profile,
};
use serde_json::Value;
use std::collections::BTreeSet;

const COMPOSITION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.composition_request.v0.schema.json");
const CAPABILITIES_SCHEMA: &str = include_str!("../schemas/canon.geo.capabilities.v0.schema.json");

fn client_contract(profile: &GeoCompositionProfile) -> &GeoClientInputContract {
    profile
        .client_input_contract
        .as_ref()
        .expect("client template embeds the six-field contract")
}

fn declarations(
    contract: &GeoClientInputContract,
    field: GeoClientInputField,
) -> BTreeSet<GeoClientInputDeclaration> {
    contract
        .field_contracts
        .iter()
        .find(|row| row.field == field)
        .expect("field contract exists")
        .required_when_present
        .iter()
        .copied()
        .collect()
}

fn roles(
    contract: &GeoClientInputContract,
    field: GeoClientInputField,
) -> BTreeSet<GeoClientInputChannelRole> {
    contract
        .field_contracts
        .iter()
        .find(|row| row.field == field)
        .expect("field contract exists")
        .channel_roles
        .iter()
        .copied()
        .collect()
}

fn digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.client.input.fixture".to_string(),
        geography_kind: "bounded_fixture".to_string(),
        description: "Client input contract planning fixture".to_string(),
    }
}

fn bound(id: &str, counter: GeoResourceCounter, value: u64) -> GeoNumericBound {
    GeoNumericBound {
        semantic_id: id.to_string(),
        counter,
        value,
        unit: format!("{:?}", counter).to_lowercase(),
        origin: GeoValueOrigin::CallerDeclared,
        action: GeoBudgetAction::ReportBudgetFallback,
    }
}

fn budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.client.input.fixture".to_string(),
        deterministic_bounds: vec![
            bound("budget.max_bytes", GeoResourceCounter::Bytes, 1_000_000),
            bound("budget.max_rows", GeoResourceCounter::Rows, 10_000),
            bound("budget.max_cells", GeoResourceCounter::Cells, 64),
            bound("budget.max_candidates", GeoResourceCounter::Candidates, 500),
            bound("budget.max_variables", GeoResourceCounter::Variables, 128),
            bound("budget.max_states", GeoResourceCounter::States, 100_000),
            bound("budget.max_models", GeoResourceCounter::Models, 10_000),
            bound(
                "budget.max_operations",
                GeoResourceCounter::Operations,
                1_000_000,
            ),
        ],
        telemetry: vec![GeoTelemetryDeclaration {
            metric: GeoTelemetryMetric::WallTime,
            unit: "millisecond".to_string(),
            origin: GeoValueOrigin::OperatorPolicy,
            semantic_effect: GeoTelemetrySemanticEffect::None,
        }],
    }
}

fn question() -> canon::geo::GeoQuestion {
    canon::geo::GeoQuestion {
        version: CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: "question.client.input.fixture".to_string(),
        subject_bindings: vec![GeoSubjectBinding {
            role: "target".to_string(),
            binding_class: GeoSubjectBindingClass::OperatorLabel,
            value: "fixture subject".to_string(),
        }],
        bounded_geography: region(),
        requested_grains: vec![canon::geo::GeoRequestedGrain {
            entity_level: GeoControlEntityLevel::Building,
            required_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            optional_evidence_classes: vec![
                GeoEvidenceClass::GeocodePoint,
                GeoEvidenceClass::AddressSet,
                GeoEvidenceClass::AssertedAttribute,
            ],
        }],
        query_as_of: None,
        requested_claim_classes: vec![GeoClaimClass::CollateralComposition],
        presentation_limits: vec![bound(
            "presentation.max_models",
            GeoResourceCounter::Models,
            16,
        )],
        abstention_policy: GeoAbstentionPolicy {
            unsupported_grain: GeoAbstentionDisposition::ReportUnsupported,
            unresolved_residual: GeoAbstentionDisposition::ReportResidual,
            budget_fallback: GeoAbstentionDisposition::ReportResidual,
        },
        decision_policy: None,
        resource_budget_ref: "budget.client.input.fixture".to_string(),
    }
}

fn source(source_instance_id: &str, evidence_class: GeoEvidenceClass) -> GeoRegionalSourceInstance {
    GeoRegionalSourceInstance {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: "release.client.input.fixture".to_string(),
            release_digest: digest("release.client.input.fixture"),
        },
        temporal_scope: GeoTemporalScope {
            valid_time: None,
            transaction_time: None,
            release_time: None,
        },
        lineage_ids: vec!["lineage.client.input.fixture".to_string()],
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: GeoControlEntityLevel::Building,
            identity_participation: GeoIdentityParticipation::StableAlias,
        },
        evidence_classes: vec![evidence_class],
        coverage: GeoCoveragePredicate {
            coverage_id: "coverage.client.input.fixture".to_string(),
            region: region(),
            predicate: "all declared client fixture buildings".to_string(),
        },
        local_state: GeoLocalAcquisitionState {
            state: GeoSourceAvailability::Available,
            local_ref: Some(GeoLocalArtifactRef {
                artifact_id: format!("artifact.{source_instance_id}"),
                contract_version: "canon_geo_warehouse_rows.v0".to_string(),
                content_hash: digest(source_instance_id),
                media_type: "application/json".to_string(),
            }),
        },
        geometry: Some(GeoGeometryTransformContract {
            geometry_contract_version: "geometry.fixture.v1".to_string(),
            coordinate_reference_system: "EPSG:4326".to_string(),
            transform_id: "identity.fixture".to_string(),
            transform_digest: digest("identity.fixture"),
            numeric_error_bounds: vec![GeoNumericMeasure {
                semantic_id: "transform.error".to_string(),
                value: 0,
                unit: "millimeter".to_string(),
                origin: GeoValueOrigin::AdapterContract,
            }],
        }),
        license_class: GeoLicenseClass::PublicRedistributable,
        egress_class: GeoEgressClass::Shareable,
        estimates: vec![GeoNumericMeasure {
            semantic_id: "source.rows".to_string(),
            value: 100,
            unit: "row".to_string(),
            origin: GeoValueOrigin::SourceRelease,
        }],
    }
}

fn inventory() -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.client.input.fixture".to_string(),
        region: region(),
        sources: vec![source(
            "client-building-footprints",
            GeoEvidenceClass::BuildingFootprint,
        )],
        discovery_gaps: Vec::new(),
    }
}

#[test]
fn six_field_template_declares_presence_metadata_and_channel_bands() {
    let profile = GeoCompositionProfile::client_six_field_building();
    validate_composition_profile(&profile).expect("built-in client profile validates");

    let contract = client_contract(&profile);
    assert_eq!(
        contract.profile_template_id,
        GEO_CLIENT_SIX_FIELD_PROFILE_TEMPLATE_ID
    );
    assert_eq!(
        contract
            .field_contracts
            .iter()
            .map(|row| row.field)
            .collect::<Vec<_>>(),
        vec![
            GeoClientInputField::Geocode,
            GeoClientInputField::Address,
            GeoClientInputField::Geometry,
            GeoClientInputField::BuildingSize,
            GeoClientInputField::YearBuilt,
            GeoClientInputField::PropertyType,
        ]
    );
    for field in &contract.field_contracts {
        assert_eq!(
            field.presence_modes,
            vec![
                GeoClientInputPresence::Present,
                GeoClientInputPresence::Absent,
                GeoClientInputPresence::PresentButUnreliable,
            ],
            "{:?} must model present/absent/unreliable explicitly",
            field.field
        );
    }

    assert!(
        declarations(contract, GeoClientInputField::Address)
            .contains(&GeoClientInputDeclaration::Locale)
    );
    assert!(
        declarations(contract, GeoClientInputField::Geometry)
            .contains(&GeoClientInputDeclaration::CoordinateReferenceSystem)
    );
    assert!(
        declarations(contract, GeoClientInputField::BuildingSize)
            .contains(&GeoClientInputDeclaration::SizeMeasure)
    );
    assert!(
        declarations(contract, GeoClientInputField::PropertyType)
            .contains(&GeoClientInputDeclaration::NeutralCategoryMapping)
    );
    assert_eq!(
        roles(contract, GeoClientInputField::BuildingSize),
        BTreeSet::from([
            GeoClientInputChannelRole::AttributeRejector,
            GeoClientInputChannelRole::AssemblageConstraint,
        ])
    );

    let agreement = &contract.channel_agreement;
    assert!(agreement.channel_sum_forbidden);
    assert!(agreement.available_disagreement_stronger_than_missing);
    assert!(
        agreement
            .candidate_universe_rule
            .contains("bounded profile universe")
    );
    assert_eq!(
        agreement
            .decision_bands
            .iter()
            .map(|band| (
                band.band,
                band.minimum_reliable_agreements,
                band.disagreement_effect
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                GeoClientDecisionBand::HardForcedCandidate,
                2,
                GeoClientDisagreementEffect::RejectCandidate,
            ),
            (
                GeoClientDecisionBand::ExactResidualOrSoftRanked,
                1,
                GeoClientDisagreementEffect::RejectCandidate,
            ),
            (
                GeoClientDecisionBand::AbstainReacquire,
                0,
                GeoClientDisagreementEffect::AbstainReacquire,
            ),
            (
                GeoClientDecisionBand::UnsupportedOrWaitingForInput,
                0,
                GeoClientDisagreementEffect::ReportUnsupported,
            ),
        ]
    );
}

#[test]
fn built_in_capabilities_emit_a_client_profile_template_that_round_trips() {
    let capabilities = default_geo_capabilities().expect("default capabilities");
    let template = capabilities
        .profile_templates
        .iter()
        .find(|template| template.profile_id == GEO_CLIENT_SIX_FIELD_PROFILE_TEMPLATE_ID)
        .expect("client six-field profile template is advertised");

    let serialized = serde_json::to_value(&template.template).expect("serialize template");
    let parsed: GeoCompositionProfile =
        serde_json::from_value(serialized).expect("template round-trips");
    validate_composition_profile(&parsed).expect("advertised profile validates");
}

#[test]
fn fixture_declarations_cover_full_address_geometry_and_ginnie_shapes() {
    let profile = GeoCompositionProfile::client_six_field_building();
    let contract = client_contract(&profile);
    assert_eq!(
        contract
            .conformance_fixtures
            .iter()
            .map(|fixture| fixture.kind)
            .collect::<Vec<_>>(),
        vec![
            GeoClientInputFixtureKind::FullyPopulated,
            GeoClientInputFixtureKind::AddressOnly,
            GeoClientInputFixtureKind::GeometryOnly,
            GeoClientInputFixtureKind::GinnieNativeNoAddressNoGeocode,
        ]
    );
    let ginnie = contract
        .conformance_fixtures
        .iter()
        .find(|fixture| fixture.kind == GeoClientInputFixtureKind::GinnieNativeNoAddressNoGeocode)
        .expect("Ginnie-shaped no-address/no-geocode fixture exists");
    let presence = ginnie
        .field_presence
        .iter()
        .map(|row| (row.field, row.presence))
        .collect::<BTreeSet<_>>();
    assert!(presence.contains(&(GeoClientInputField::Geocode, GeoClientInputPresence::Absent)));
    assert!(presence.contains(&(GeoClientInputField::Address, GeoClientInputPresence::Absent)));
    assert_eq!(
        ginnie.expected_band,
        GeoClientDecisionBand::UnsupportedOrWaitingForInput
    );
}

#[test]
fn compile_geo_plan_admits_the_valid_client_profile_template() {
    let plan = compile_geo_plan(GeoPlanRequest {
        question: question(),
        capabilities: default_geo_capabilities().expect("default capabilities"),
        inventory: inventory(),
        profile: GeoCompositionProfile::client_six_field_building(),
        budget: budget(),
    })
    .expect("geo plan accepts the valid client profile");

    assert_eq!(plan.status, GeoPlanStatus::Planned);
    assert_eq!(
        plan.profile_ref.selection_level,
        canon::geo::GeoEntityLevel::Building
    );
}

#[test]
fn missing_required_declarations_are_contract_violations_not_absence_failures() {
    let mut profile = GeoCompositionProfile::client_six_field_building();
    let contract = profile
        .client_input_contract
        .as_mut()
        .expect("client contract exists");
    let geometry = contract
        .field_contracts
        .iter_mut()
        .find(|field| field.field == GeoClientInputField::Geometry)
        .expect("geometry field exists");
    geometry
        .required_when_present
        .retain(|declaration| *declaration != GeoClientInputDeclaration::CoordinateReferenceSystem);

    let error = validate_composition_profile(&profile).expect_err("missing CRS refuses");
    assert_eq!(error.code, GeoCompositionErrorCode::InvalidInput);
    assert!(
        error
            .detail
            .get("expected")
            .is_some_and(|detail| detail.contains("CoordinateReferenceSystem"))
    );

    let mut plan_request = GeoPlanRequest {
        question: question(),
        capabilities: default_geo_capabilities().expect("default capabilities"),
        inventory: inventory(),
        profile,
        budget: budget(),
    };
    let error =
        compile_geo_plan(plan_request.clone()).expect_err("planner refuses malformed profile");
    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);

    plan_request.profile = GeoCompositionProfile::client_six_field_building();
    compile_geo_plan(plan_request).expect("declared field absence remains valid");
}

#[test]
fn channel_sum_or_missing_fixture_members_do_not_validate() {
    let mut profile = GeoCompositionProfile::client_six_field_building();
    profile
        .client_input_contract
        .as_mut()
        .expect("client contract exists")
        .channel_agreement
        .channel_sum_forbidden = false;
    let error = validate_composition_profile(&profile).expect_err("channel sum refuses");
    assert_eq!(error.code, GeoCompositionErrorCode::InvalidInput);
    assert!(error.message.contains("channel-sum"));

    let mut profile = GeoCompositionProfile::client_six_field_building();
    let ginnie = profile
        .client_input_contract
        .as_mut()
        .expect("client contract exists")
        .conformance_fixtures
        .iter_mut()
        .find(|fixture| fixture.kind == GeoClientInputFixtureKind::GinnieNativeNoAddressNoGeocode)
        .expect("Ginnie fixture exists");
    ginnie
        .field_presence
        .retain(|presence| presence.field != GeoClientInputField::Address);

    let error = validate_composition_profile(&profile).expect_err("fixture must remain exhaustive");
    assert_eq!(error.code, GeoCompositionErrorCode::InvalidInput);
}

#[test]
fn public_schemas_declare_the_profile_template_contract() {
    let composition_schema: Value =
        serde_json::from_str(COMPOSITION_SCHEMA).expect("composition schema parses");
    assert_eq!(
        composition_schema
            .pointer("/$defs/composition_profile/properties/client_input_contract/$ref")
            .and_then(Value::as_str),
        Some("#/$defs/client_input_contract")
    );
    assert_eq!(
        composition_schema
            .pointer("/$defs/client_input_contract/additionalProperties")
            .and_then(Value::as_bool),
        Some(false)
    );

    let capabilities_schema: Value =
        serde_json::from_str(CAPABILITIES_SCHEMA).expect("capabilities schema parses");
    assert_eq!(
        capabilities_schema
            .pointer("/properties/profile_templates/items/$ref")
            .and_then(Value::as_str),
        Some("#/$defs/profile_template_capability")
    );
    assert_eq!(
        capabilities_schema
            .pointer("/$defs/profile_template_capability/properties/template/$ref")
            .and_then(Value::as_str),
        Some("canon.geo.composition_request.v0.schema.json#/$defs/composition_profile")
    );
}
