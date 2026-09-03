//! Schema-drift guard for the registered Geo contracts.
//!
//! For each contract this test: (a) pins the schema file's `title` and
//! `properties.version.const`, and asserts top-level `additionalProperties`
//! is `false`; (b) builds a real instance through the library API, serializes
//! it with `serde_json`, and walks every key present in the serialized value
//! to confirm the schema declares it somewhere reachable from the root
//! object (`properties`, `$defs`, `$ref`, array `items`, and `oneOf`
//! alternatives for tagged enums). This does not add a `jsonschema`
//! dependency; it only catches keys the schema forgot to declare.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use canon::entity::run::link::multisource::EntitySourceRole;
use canon::geo::CANON_GEO_EXPLANATION_VERSION;
use canon::geo::assessment_roll::{
    CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION, CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION,
    GeoAssessmentRollCaseDocument, GeoAssessmentRollLotRow, GeoAssessmentRollOwnerCalibration,
    GeoAssessmentRollOwnerContractSource, GeoAssessmentRollOwnerProofClass,
    GeoAssessmentRollOwnerRequest, GeoAssessmentRollPartyRow,
    canonical_assessment_roll_owner_bytes, produce_assessment_roll_owner_evidence,
};
use canon::geo::condo::{
    CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION, CANON_GEO_CONDO_BRIDGE_VERSION,
    GeoCondoBridgeCaseRequest, GeoCondoBridgeRequest, GeoPadBblRow, build_condo_bridge,
    canonical_condo_bridge_bytes,
};
use canon::geo::footprint_roll::{
    CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION, GeoAssessmentRollGrossSqftRow,
    GeoBuildingFootprintRow, GeoFootprintRollCalibration, GeoFootprintRollEvidenceRequest,
    GeoFootprintRollLoanFields, GeoFootprintRollSourceConfig,
    canonical_footprint_roll_evidence_request_bytes,
};
use canon::geo::property::{
    CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION, CANON_GEO_PROPERTY_ASSERTION_VERSION,
    GeoPropertyAssertionProofClass, GeoPropertyAssertionRequest, GeoPropertyBlockingStrategy,
    GeoPropertyDocumentAssertionRequest, GeoPropertyMemberAssertion, GeoPropertyMembershipStatus,
    GeoPropertyRelationGraphProduct, GeoPropertySourceCorpus, GeoPropertySourceRecordRef,
    canonical_property_assertion_bytes, materialize_property_assertions,
};
use canon::geo::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
    CANON_GEO_DISCOVERY_REQUEST_VERSION, CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION,
    GeoAcquisitionArtifactReleaseRelation, GeoAcquisitionCounts, GeoAcquisitionDenominator,
    GeoAcquisitionProofClass, GeoAcquisitionReceipt, GeoAcquisitionRequest,
    GeoAcquisitionResumability, GeoAcquisitionTerminalState, GeoBoundedSubset,
    GeoColumnReadabilityProbe, GeoDenominatorSource, GeoDigest, GeoDigestAlgorithm,
    GeoDiscoveryReleaseSelectionPolicy, GeoDiscoveryRequest, GeoDiscoveryStep, GeoExecutorKind,
    GeoExecutorTrace, GeoFieldRole, GeoInventoryAdvancementEffect, GeoLocalArtifactDigest,
    GeoNullOrdering, GeoOrderDirection, GeoOrderingTerm, GeoPaginationReceipt,
    GeoPaginationRequest, GeoProjectionOperation, GeoRegionalInventoryAdvancement,
    GeoRegionalInventorySourceAdvancement, GeoReleasePin, GeoReleaseSelectionMode,
    GeoRequestedField, GeoRowByteCeilings, GeoSatisfactionExecutionRef, GeoSatisfactionFileAudit,
    GeoSubsetPredicate, GeoSubsetPredicateKind, canonical_geo_regional_inventory_advancement_bytes,
    geo_acquisition_request_id, geo_acquisition_request_semantic_hash, geo_discovery_request_id,
    geo_regional_inventory_advancement_semantic_hash,
};
use canon::geo::{
    CANON_GEO_ADDRESS_PARSE_FOREST_VERSION, CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION,
    CANON_GEO_CAPABILITIES_VERSION, CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION,
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_ERROR_POPULATION_VERSION,
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_GEOMETRY_REQUEST_VERSION,
    CANON_GEO_H7_ACRIS_RELEASE_DT, CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION,
    CANON_GEO_H7_BRIDGE_BUILD_ID, CANON_GEO_H7_COLLATERAL_SCOPE,
    CANON_GEO_H7_LENDER_MATCH_TRANSFORM, CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION,
    CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION, CANON_GEO_H7_POPULATION_ROWS_VERSION,
    CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE,
    CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS,
    CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
    CANON_GEO_LOCAL_FRAME_VERSION, CANON_GEO_MULTISOURCE_REQUEST_VERSION,
    CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION, CANON_GEO_NEXT_EVIDENCE_VERSION,
    CANON_GEO_PAD_ADDRESS_SET_VERSION, CANON_GEO_PAD_MEMBERSHIP_VERSION,
    CANON_GEO_POINT_POPULATION_VERSION, CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
    CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    CANON_GEO_PRE_RESOLUTION_VERSION, CANON_GEO_PROPAGATION_VERSION, CANON_GEO_QUESTION_VERSION,
    CANON_GEO_REDACTED_ARTIFACT_VERSION, CANON_GEO_REGIONAL_INVENTORY_VERSION,
    CANON_GEO_RESOURCE_BUDGET_VERSION, CANON_GEO_SEPARATION_REQUEST_VERSION,
    CANON_GEO_SEPARATION_VERSION, CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
    CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
    CANON_GEO_WAREHOUSE_ROWS_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoAbstentionDisposition,
    GeoAbstentionPolicy, GeoAddressHouseNumber, GeoAddressJurisdiction, GeoAddressParity,
    GeoAddressParseRequest, GeoAddressRangeOperator, GeoAddressStreet, GeoAffineProjectionMm,
    GeoArtifactFieldClassification, GeoArtifactFieldLicenseClass, GeoAsOf, GeoBoundedGeography,
    GeoBudgetAction, GeoBuildingCandidate, GeoClaimClass, GeoClientTileCoverageExtent,
    GeoClientTileCoverageExtentKind, GeoClientTileIngestRequest, GeoClientTileSourceFormat,
    GeoClientTileVendorIdentifier, GeoCompositionModel, GeoCompositionProfile,
    GeoCompositionRequest, GeoCompositionUniverse, GeoControlEntityLevel, GeoCoveragePredicate,
    GeoEgressClass, GeoEntityLevel, GeoEntityRef, GeoErrorPopulationArtifact,
    GeoErrorPopulationSubject, GeoEvidenceClaimRole, GeoEvidenceClass,
    GeoEvidenceCompilationRequest, GeoEvidenceRecordRef, GeoExactSourceUnitMm,
    GeoExplanationBudget, GeoGeometryFeatureInput, GeoGeometryTileRequest, GeoH7AssociationPlane,
    GeoH7BoroughEdge, GeoH7CandidateReachStatus, GeoH7FiledCountyMapping, GeoH7MapplutoReleasePin,
    GeoH7PlaneDenominator, GeoH7PopulationProvenance, GeoH7PopulationRowsRequest,
    GeoH7PopulationScope, GeoH7PopulationWarehouseRow, GeoH7QueryDisposition, GeoH7QueryReceipt,
    GeoH7ResultMode, GeoH7SourceEvidenceRecord, GeoH7SourceRecordRole,
    GeoH7StagingEvidenceRecordRef, GeoH7StagingSourceEvidenceRecord,
    GeoH7StagingSourceRecordBytesBatchRequest, GeoH7StagingSourceRecordBytesRow, GeoHardConstraint,
    GeoHardConstraintKind, GeoHomeCellAssignmentArtifact, GeoHomeCellRow, GeoHomeCellRowsRequest,
    GeoIdentityParticipation, GeoIntegerMeasure, GeoIntegerMemberValue, GeoIntegerValueOrigin,
    GeoLabeledCompositionCase, GeoLicenseClass, GeoLocalAcquisitionState, GeoLocalArtifactRef,
    GeoLocalFrameContract, GeoMultisourceRequest, GeoMultisourceSource, GeoNativeEntityScope,
    GeoNextAction, GeoNextActionClass, GeoNextActionKind, GeoNextEvidenceRequest, GeoNumericBound,
    GeoNumericMeasure, GeoNycBorough, GeoPadAddressMember, GeoPadAddressSet, GeoPlanInventoryRef,
    GeoPointPopulationArtifact, GeoPopulationCaseEvidenceOverlay, GeoPopulationEvaluationRequest,
    GeoPopulationEvidenceStackRequest, GeoPreResolutionArtifact, GeoPreResolutionBuildReceipt,
    GeoPreResolutionCorpusKind, GeoPreResolutionProofClass, GeoPreResolutionRequest,
    GeoPreResolutionRunStatus, GeoPreResolutionSourceCorpus, GeoPreResolutionSourceRow,
    GeoProjectionProvenance, GeoPropagationBudget, GeoProspectiveObservation,
    GeoProspectiveOutcome, GeoQuestion, GeoRegionalInventory, GeoRegionalSourceInstance,
    GeoReliabilityOrder, GeoRequestedGrain, GeoResourceBudget, GeoResourceCounter, GeoRhoBasis,
    GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind, GeoSeparationRequest,
    GeoSourceAvailability, GeoSourceAxisDomain, GeoSourceGeometry, GeoSourcePointDecimal,
    GeoSourcePointFixed, GeoSourceRelease, GeoStreetDirection, GeoStreetSuffix, GeoSubjectBinding,
    GeoSubjectBindingClass, GeoTelemetryDeclaration, GeoTelemetryMetric,
    GeoTelemetrySemanticEffect, GeoTemporalScope, GeoTileCandidateReachReference,
    GeoTileCandidateReachReferenceKind, GeoTileDecisionBatch, GeoTileDecisionMember,
    GeoTileDecisionProposal, GeoTileDecisionSemantics, GeoTileFeatureRef,
    GeoTileReconciliationArtifact, GeoTileReconciliationRequest, GeoTileSourceBinding,
    GeoTileWorkRequest, GeoTileWorkUnitArtifact, GeoTruthPlane, GeoValueOrigin,
    GeoWarehouseEvidenceRow, GeoWarehouseGeometryRow, GeoWarehouseGeometryRowsRequest,
    GeoWarehouseParcelRow, GeoWarehouseRowsRequest, canonical_composition_bytes,
    canonical_error_population_bytes, canonical_explanation_bytes, canonical_next_evidence_bytes,
    canonical_next_evidence_request_bytes, canonical_pre_resolution_bytes,
    canonical_propagation_bytes, canonical_redacted_artifact_bytes, canonical_separation_bytes,
    canonical_separation_request_bytes, compile_evidence, correction_sets,
    default_geo_capabilities, evaluate_pad_membership, evaluate_population,
    ingest_client_geometry_tile, materialize_geo_multisource, materialize_geometry_tile,
    materialize_h7_population_rows, materialize_home_cells, materialize_pre_resolution,
    materialize_tile_work_unit, materialize_warehouse_geometry, minimal_core, parse_address_forest,
    propagate, recommend, reconcile_tile_decisions, redact_geo_artifact,
    regional_inventory_semantic_hash, separate, solve_composition, stack_population_evidence,
    validate_point_population_artifact, validate_pre_resolution_artifact,
    validate_redacted_artifact,
};
use canon::geo::{
    CANON_GEO_TEMPORAL_CONTAINMENT_VERSION, GeoTemporalContainmentArtifact,
    GeoTemporalContainmentCluster, GeoTemporalContainmentEdge, GeoTemporalContainmentInterval,
    GeoTemporalContainmentRelation, GeoTemporalContainmentSourceReceipt,
    GeoTemporalContainmentSummary, canonical_temporal_containment_bytes,
    validate_temporal_containment_artifact,
};
use h3o::{LatLng, Resolution};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{fs, path::Path, str::FromStr};

const COMPOSITION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.composition_request.v0.schema.json");
const COMPOSITION_SCHEMA: &str = include_str!("../schemas/canon.geo.composition.v0.schema.json");
const EVIDENCE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.evidence_request.v0.schema.json");
const EVIDENCE_COMPILATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.evidence_compilation.v0.schema.json");
const POPULATION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.population_request.v0.schema.json");
const POPULATION_EVALUATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.population_evaluation.v0.schema.json");
const POINT_POPULATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.point_population.v0.schema.json");
const PRE_RESOLUTION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pre_resolution.v0.schema.json");
const TEMPORAL_CONTAINMENT_SCHEMA: &str =
    include_str!("../schemas/canon.geo.temporal_containment.v0.schema.json");
const PROPAGATION_SCHEMA: &str = include_str!("../schemas/canon.geo.propagation.v0.schema.json");
const EXPLANATION_SCHEMA: &str = include_str!("../schemas/canon.geo.explanation.v0.schema.json");
const NEXT_EVIDENCE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.next_evidence_request.v0.schema.json");
const NEXT_EVIDENCE_SCHEMA: &str =
    include_str!("../schemas/canon.geo.next_evidence.v0.schema.json");
const ASSESSMENT_ROLL_OWNER_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.assessment_roll_owner_request.v0.schema.json");
const ASSESSMENT_ROLL_OWNER_SCHEMA: &str =
    include_str!("../schemas/canon.geo.assessment_roll_owner.v0.schema.json");
const PROPERTY_ASSERTION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.property_assertion_request.v0.schema.json");
const PROPERTY_ASSERTION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.property_assertion.v0.schema.json");
const CONDO_BRIDGE_SCHEMA: &str = include_str!("../schemas/canon.geo.condo_bridge.v0.schema.json");
const FOOTPRINT_ROLL_EVIDENCE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.footprint_roll_evidence_request.v0.schema.json");
const SEPARATION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.separation_request.v0.schema.json");
const SEPARATION_SCHEMA: &str = include_str!("../schemas/canon.geo.separation.v0.schema.json");
const POPULATION_EVIDENCE_STACK_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.population_evidence_stack_request.v0.schema.json");
const POPULATION_EVIDENCE_STACK_SCHEMA: &str =
    include_str!("../schemas/canon.geo.population_evidence_stack.v0.schema.json");
const WAREHOUSE_ROWS_SCHEMA: &str =
    include_str!("../schemas/canon.geo.warehouse_rows.v0.schema.json");
const GEOMETRY_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.geometry_request.v0.schema.json");
const CLIENT_TILE_INGEST_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.client_tile_ingest_request.v0.schema.json");
const GEOMETRY_TILE_SCHEMA: &str =
    include_str!("../schemas/canon.geo.geometry_tile.v0.schema.json");
const REDACTED_ARTIFACT_SCHEMA: &str =
    include_str!("../schemas/canon.geo.redacted_artifact.v0.schema.json");
const WAREHOUSE_GEOMETRY_ROWS_SCHEMA: &str =
    include_str!("../schemas/canon.geo.warehouse_geometry_rows.v0.schema.json");
const WAREHOUSE_GEOMETRY_SCHEMA: &str =
    include_str!("../schemas/canon.geo.warehouse_geometry.v0.schema.json");
const HOME_CELL_ROWS_SCHEMA: &str =
    include_str!("../schemas/canon.geo.home_cell_rows.v1.schema.json");
const HOME_CELL_ASSIGNMENT_SCHEMA: &str =
    include_str!("../schemas/canon.geo.home_cell_assignment.v1.schema.json");
const TILE_WORK_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_work_request.v1.schema.json");
const TILE_WORK_UNIT_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_work_unit.v1.schema.json");
const TILE_RECONCILIATION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_reconciliation_request.v1.schema.json");
const TILE_RECONCILIATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_reconciliation.v1.schema.json");
const MULTISOURCE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.multisource_request.v0.schema.json");
const MULTISOURCE_ARTIFACT_SCHEMA: &str =
    include_str!("../schemas/canon.entity.multisource_link.v1.schema.json");
const H7_POPULATION_ROWS_SCHEMA: &str =
    include_str!("../schemas/canon.geo.h7_population_rows.v0.schema.json");
const H7_POPULATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.h7_population.v0.schema.json");
const H7_STAGING_SOURCE_RECORD_BYTES_BATCH_SCHEMA: &str =
    include_str!("../schemas/canon.geo.h7_staging_source_record_bytes_batch.v0.schema.json");
const H7_PIP_BLOCK_POPULATION_BATCH_SCHEMA: &str =
    include_str!("../schemas/canon.geo.h7_pip_block_population_batch.v0.schema.json");
const ADDRESS_PARSE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parse_request.v0.schema.json");
const ADDRESS_PARSE_FOREST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.address_parse_forest.v0.schema.json");
const PAD_ADDRESS_SET_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pad_address_set.v0.schema.json");
const PAD_MEMBERSHIP_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pad_membership.v0.schema.json");
const CONTROL_QUESTION_SCHEMA: &str = include_str!("../schemas/canon.geo.question.v0.schema.json");
const CONTROL_CAPABILITIES_SCHEMA: &str =
    include_str!("../schemas/canon.geo.capabilities.v0.schema.json");
const CONTROL_REGIONAL_INVENTORY_SCHEMA: &str =
    include_str!("../schemas/canon.geo.regional_inventory.v1.schema.json");
const REGIONAL_INVENTORY_ADVANCEMENT_SCHEMA: &str =
    include_str!("../schemas/canon.geo.regional_inventory_advancement.v0.schema.json");
const CONTROL_RESOURCE_BUDGET_SCHEMA: &str =
    include_str!("../schemas/canon.geo.resource_budget.v0.schema.json");
const DISCOVERY_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.discovery_request.v0.schema.json");
const ACQUISITION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.acquisition_request.v0.schema.json");
const ACQUISITION_RECEIPT_SCHEMA: &str =
    include_str!("../schemas/canon.geo.acquisition_receipt.v0.schema.json");
const ERROR_POPULATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.error_population.v0.schema.json");

fn parsed(source: &str) -> Value {
    serde_json::from_str(source).expect("schema file must be valid JSON")
}

fn assert_schema_shape(schema: &Value, expected_title: &str, expected_version_const: &str) {
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some(expected_title),
        "title mismatch"
    );
    let version_const = schema
        .pointer("/properties/version/const")
        .and_then(Value::as_str);
    assert_eq!(
        version_const,
        Some(expected_version_const),
        "properties.version.const mismatch"
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false),
        "top-level additionalProperties must be false"
    );
}

fn required_contains(schema: &Value, pointer: &str, field: &str) -> bool {
    schema
        .pointer(pointer)
        .and_then(Value::as_array)
        .is_some_and(|fields| fields.iter().any(|value| value.as_str() == Some(field)))
}

/// Resolve a `$ref` such as `#/$defs/entity_ref` against the schema root.
fn resolve_ref<'a>(schema: &'a Value, reference: &str) -> &'a Value {
    let path = reference
        .strip_prefix('#')
        .expect("only local $ref pointers are supported");
    schema
        .pointer(path)
        .unwrap_or_else(|| panic!("$ref {reference} does not resolve"))
}

fn external_schema_source(schema_file: &str, reference: &str) -> &'static str {
    match schema_file {
        "canon.geo.address_parse_request.v0.schema.json" => ADDRESS_PARSE_REQUEST_SCHEMA,
        "canon.geo.address_parse_forest.v0.schema.json" => ADDRESS_PARSE_FOREST_SCHEMA,
        "canon.geo.acquisition_request.v0.schema.json" => ACQUISITION_REQUEST_SCHEMA,
        "canon.geo.pad_address_set.v0.schema.json" => PAD_ADDRESS_SET_SCHEMA,
        "canon.geo.pad_membership.v0.schema.json" => PAD_MEMBERSHIP_SCHEMA,
        "canon.geo.composition_request.v0.schema.json" => COMPOSITION_REQUEST_SCHEMA,
        "canon.geo.geometry_tile.v0.schema.json" => GEOMETRY_TILE_SCHEMA,
        "canon.geo.h7_population_rows.v0.schema.json" => H7_POPULATION_ROWS_SCHEMA,
        "canon.geo.evidence_request.v0.schema.json" => EVIDENCE_REQUEST_SCHEMA,
        "canon.geo.population_request.v0.schema.json" => POPULATION_REQUEST_SCHEMA,
        "canon.geo.population_evidence_stack_request.v0.schema.json" => {
            POPULATION_EVIDENCE_STACK_REQUEST_SCHEMA
        }
        "canon.geo.regional_inventory.v1.schema.json" => CONTROL_REGIONAL_INVENTORY_SCHEMA,
        "canon.geo.resource_budget.v0.schema.json" => CONTROL_RESOURCE_BUDGET_SCHEMA,
        _ => panic!("external $ref {reference} is not registered in the schema test"),
    }
}

fn resolve_external_ref(reference: &str) -> Value {
    let (schema_file, fragment) = reference
        .split_once('#')
        .map_or((reference, ""), |(schema_file, fragment)| {
            (schema_file, fragment)
        });
    let external = parsed(external_schema_source(schema_file, reference));
    if fragment.is_empty() {
        external
    } else {
        external
            .pointer(fragment)
            .unwrap_or_else(|| panic!("external $ref {reference} does not resolve"))
            .clone()
    }
}

/// Recursively assert that every key present in `instance` is declared by
/// `subschema` (following `$ref`, `oneOf` alternatives whose `const`/`enum`
/// match a discriminant key, `items` for arrays, and `additionalProperties`
/// pattern schemas). `root` is the whole schema document, needed to resolve
/// `$ref`.
fn assert_instance_matches_schema(root: &Value, subschema: &Value, instance: &Value, path: &str) {
    if let Some(reference) = subschema.get("$ref").and_then(Value::as_str) {
        if reference.starts_with('#') {
            let resolved = resolve_ref(root, reference);
            assert_instance_matches_schema(root, resolved, instance, path);
        } else {
            let (schema_file, fragment) = reference
                .split_once('#')
                .map_or((reference, ""), |(schema_file, fragment)| {
                    (schema_file, fragment)
                });
            let external = parsed(external_schema_source(schema_file, reference));
            let external_schema = if fragment.is_empty() {
                &external
            } else {
                external
                    .pointer(fragment)
                    .unwrap_or_else(|| panic!("external $ref {reference} does not resolve"))
            };
            assert_instance_matches_schema(&external, external_schema, instance, path);
        }
        return;
    }

    if subschema.get("properties").is_none()
        && let Some(all_of) = subschema.get("allOf").and_then(Value::as_array)
    {
        let Value::Object(object) = instance else {
            return;
        };
        for (key, value) in object {
            let child_path = format!("{path}.{key}");
            let walked = all_of
                .iter()
                .any(|part| walk_property_if_declared(root, part, key, value, &child_path));
            assert!(walked, "{child_path}: key not declared by allOf at {path}");
        }
        return;
    }

    if let Some(alternatives) = subschema.get("oneOf").and_then(Value::as_array) {
        if instance.is_null() {
            let has_null_alt = alternatives
                .iter()
                .any(|alt| alt.get("type").and_then(Value::as_str) == Some("null"));
            assert!(has_null_alt, "{path}: null value but no null alternative");
            return;
        }
        if instance.is_boolean() || instance.is_number() || instance.is_string() {
            // Scalar oneOf alternative (e.g. Option<bool>, Option<u64>): the
            // schema declared it, nothing further to walk into.
            return;
        }
        if instance.is_array() {
            let chosen = alternatives
                .iter()
                .find(|alt| alt.get("type").and_then(Value::as_str) == Some("array"))
                .unwrap_or_else(|| panic!("{path}: no oneOf array alternative matches"));
            assert_instance_matches_schema(root, chosen, instance, path);
            return;
        }
        let Value::Object(object) = instance else {
            panic!("{path}: expected an object for oneOf, got {instance:?}");
        };
        // Pick the alternative whose object-level shape matches: prefer one
        // declaring a `kind` const equal to the instance's `kind`, else the
        // first alternative whose required properties are all present.
        let chosen = alternatives
            .iter()
            .find(|alt| object_one_of_alternative_matches(root, alt, object, true))
            .or_else(|| {
                alternatives
                    .iter()
                    .find(|alt| object_one_of_alternative_matches(root, alt, object, false))
            })
            .unwrap_or_else(|| panic!("{path}: no oneOf alternative matches {instance:?}"));
        if chosen.get("$ref").is_some()
            || chosen.get("properties").is_some()
            || chosen.get("items").is_some()
            || chosen.get("oneOf").is_some()
        {
            assert_instance_matches_schema(root, chosen, instance, path);
            return;
        }
        // Some schemas use oneOf only to require one of several column-name
        // conventions; the enclosing schema still declares the object fields.
    }

    match instance {
        Value::Object(object) => {
            let properties = subschema
                .get("properties")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (key, value) in object {
                let child_path = format!("{path}.{key}");
                if let Some(child_schema) = properties.get(key) {
                    assert_instance_matches_schema(root, child_schema, value, &child_path);
                } else if let Some(child_schema) = subschema
                    .get("additionalProperties")
                    .filter(|schema| schema.is_object())
                {
                    assert_instance_matches_schema(root, child_schema, value, &child_path);
                } else {
                    panic!(
                        "{child_path}: key not declared in schema properties at {path} (available: {:?})",
                        properties.keys().collect::<Vec<_>>()
                    )
                }
            }
        }
        Value::Array(items) => {
            if let Some(item_schema) = subschema.get("items") {
                for (index, item) in items.iter().enumerate() {
                    assert_instance_matches_schema(
                        root,
                        item_schema,
                        item,
                        &format!("{path}[{index}]"),
                    );
                }
            } else if !items.is_empty() {
                panic!("{path}: array has items but schema declares no `items`");
            }
        }
        _ => {
            // Scalars (string/number/bool/null) terminate the walk: the
            // parent already confirmed the key is declared.
        }
    }
}

fn object_one_of_alternative_matches(
    root: &Value,
    alternative: &Value,
    object: &serde_json::Map<String, Value>,
    require_kind_match: bool,
) -> bool {
    if alternative.get("type").and_then(Value::as_str) == Some("null") {
        return false;
    }
    let resolved_external;
    let resolved = match alternative.get("$ref").and_then(Value::as_str) {
        Some(reference) if reference.starts_with('#') => resolve_ref(root, reference),
        Some(reference) => {
            resolved_external = resolve_external_ref(reference);
            &resolved_external
        }
        None => alternative,
    };
    if let Some(kind_const) = resolved
        .pointer("/properties/kind/const")
        .and_then(Value::as_str)
    {
        return object.get("kind").and_then(Value::as_str) == Some(kind_const);
    }
    if require_kind_match {
        return false;
    }
    let required = required_fields_for_object_match(root, resolved);
    !required.is_empty() && required.iter().all(|key| object.contains_key(key.as_str()))
}

fn required_fields_for_object_match(root: &Value, schema: &Value) -> Vec<String> {
    let resolved_external;
    let resolved = match schema.get("$ref").and_then(Value::as_str) {
        Some(reference) if reference.starts_with('#') => resolve_ref(root, reference),
        Some(reference) => {
            resolved_external = resolve_external_ref(reference);
            &resolved_external
        }
        None => schema,
    };
    let mut required = resolved
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if let Some(all_of) = resolved.get("allOf").and_then(Value::as_array) {
        for part in all_of {
            required.extend(required_fields_for_object_match(root, part));
        }
    }
    required.sort();
    required.dedup();
    required
}

fn walk_property_if_declared(
    root: &Value,
    subschema: &Value,
    key: &str,
    value: &Value,
    child_path: &str,
) -> bool {
    let resolved_external;
    let resolved = match subschema.get("$ref").and_then(Value::as_str) {
        Some(reference) if reference.starts_with('#') => resolve_ref(root, reference),
        Some(reference) => {
            resolved_external = resolve_external_ref(reference);
            &resolved_external
        }
        None => subschema,
    };
    if let Some(properties) = resolved.get("properties").and_then(Value::as_object)
        && let Some(child_schema) = properties.get(key)
    {
        assert_instance_matches_schema(root, child_schema, value, child_path);
        return true;
    }
    resolved
        .get("allOf")
        .and_then(Value::as_array)
        .is_some_and(|all_of| {
            all_of
                .iter()
                .any(|part| walk_property_if_declared(root, part, key, value, child_path))
        })
}

fn assert_drift_free(
    schema_source: &str,
    expected_title: &str,
    expected_version: &str,
    instance: &Value,
) {
    let schema = parsed(schema_source);
    assert_schema_shape(&schema, expected_title, expected_version);
    assert_instance_matches_schema(&schema, &schema, instance, "$");
}

fn assert_schema_walk_rejects(schema_source: &str, instance: &Value, expected: &str) {
    let schema = parsed(schema_source);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_instance_matches_schema(&schema, &schema, instance, "$");
    }))
    .expect_err("schema declaration walk must reject the instance");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("<non-string panic>");
    assert!(
        message.contains(expected),
        "expected schema rejection containing {expected:?}, got {message:?}"
    );
}

fn lowercase_hex_with_len(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn prefixed_blake3_shape(value: &str) -> bool {
    value
        .strip_prefix("blake3:")
        .is_some_and(|hex| lowercase_hex_with_len(hex, 64))
}

fn regional_inventory_advancement_id_shape(value: &str) -> bool {
    value
        .strip_prefix("canon_geo_regional_inventory_advancement.v0:")
        .is_some_and(|hex| lowercase_hex_with_len(hex, 64))
}

fn json_integer(text: &str) -> Value {
    serde_json::from_str(text).expect("integer JSON literal parses")
}

fn numeric_schema_errors(root: &Value, subschema: &Value, instance: &Value) -> Vec<String> {
    fn validate(root: &Value, subschema: &Value, instance: &Value, errors: &mut Vec<String>) {
        if let Some(reference) = subschema.get("$ref").and_then(Value::as_str) {
            validate(root, resolve_ref(root, reference), instance, errors);
        }

        if subschema.get("type").and_then(Value::as_str) == Some("integer") {
            let is_integer = instance
                .as_number()
                .is_some_and(|number| number.as_i64().is_some() || number.as_u64().is_some());
            if !is_integer {
                errors.push("expected type integer".to_string());
                return;
            }
        }

        if let Some(minimum) = subschema.get("minimum").and_then(Value::as_i64)
            && instance.as_i64().is_some_and(|value| value < minimum)
        {
            errors.push(format!("value below minimum {minimum}"));
        }
        if let Some(maximum) = subschema.get("maximum").and_then(Value::as_u64)
            && instance.as_u64().is_some_and(|value| value > maximum)
        {
            errors.push(format!("value greater than maximum {maximum}"));
        }
    }

    let mut errors = Vec::new();
    validate(root, subschema, instance, &mut errors);
    errors
}

fn assert_numeric_schema_rejects(
    schema_source: &str,
    pointer: &str,
    instance: Value,
    expected: &str,
) {
    let schema = parsed(schema_source);
    let subschema = schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("numeric schema pointer {pointer} resolves"));
    let errors = numeric_schema_errors(&schema, subschema, &instance);
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected numeric schema rejection containing {expected:?}, got {errors:#?}"
    );
}

fn address_parse_request() -> GeoAddressParseRequest {
    GeoAddressParseRequest {
        version: CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION.to_string(),
        input: "241/249 West 74th Street".to_string(),
        jurisdiction: Some(GeoAddressJurisdiction::nyc_borough(
            GeoNycBorough::Manhattan,
        )),
    }
}

fn address_parse_forest() -> canon::geo::GeoAddressParseForest {
    parse_address_forest(&address_parse_request()).expect("address fixture parses")
}

fn address_pad_set() -> GeoPadAddressSet {
    let west_74th = GeoAddressStreet::ordinal(
        Some(GeoStreetDirection::West),
        74,
        Some(GeoStreetSuffix::Street),
    )
    .expect("street fixture is valid");
    GeoPadAddressSet {
        version: CANON_GEO_PAD_ADDRESS_SET_VERSION.to_string(),
        jurisdiction: GeoAddressJurisdiction::nyc_borough(GeoNycBorough::Manhattan),
        members: vec![GeoPadAddressMember::new(
            "pad:w74:241-249",
            "mn:w74:lot",
            GeoAddressHouseNumber::range(
                241,
                249,
                GeoAddressParity::Odd,
                GeoAddressRangeOperator::Slash,
                vec![241, 249],
            )
            .expect("range fixture is valid"),
            west_74th,
        )],
    }
}

fn address_pad_membership() -> canon::geo::GeoPadMembershipEvaluation {
    evaluate_pad_membership(&address_parse_forest(), &address_pad_set())
        .expect("address membership evaluates")
}

fn control_digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn prefixed_blake3(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn contract_blake3_digest(digest_id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: digest_id.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: blake3::hash(bytes).to_hex().to_string(),
    }
}

fn contract_sha256_digest(digest_id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: digest_id.to_string(),
        algorithm: GeoDigestAlgorithm::Sha256,
        hex_digest: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

fn artifact_release_relation(
    local_artifact_id: &str,
    release: &GeoReleasePin,
) -> GeoAcquisitionArtifactReleaseRelation {
    GeoAcquisitionArtifactReleaseRelation {
        local_artifact_id: local_artifact_id.to_string(),
        source_instance_id: release.source_instance_id.clone(),
        release_id: release.release_id.clone(),
        release_digest: format!(
            "{}:{}",
            digest_algorithm_name(release.release_digest.algorithm),
            release.release_digest.hex_digest
        ),
    }
}

fn digest_algorithm_name(algorithm: GeoDigestAlgorithm) -> &'static str {
    match algorithm {
        GeoDigestAlgorithm::Blake3 => "blake3",
        GeoDigestAlgorithm::Sha256 => "sha256",
        GeoDigestAlgorithm::Sha512 => "sha512",
    }
}

fn control_region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.control".to_string(),
        geography_kind: "declared_test_region".to_string(),
        description: "Control contract fixture region".to_string(),
    }
}

fn discovery_contract_subset() -> GeoBoundedSubset {
    GeoBoundedSubset {
        subset_id: "subset.fixture.discovery-r8-k1".to_string(),
        geography: control_region(),
        h3_cells: vec!["882a107707fffff".to_string()],
        predicates: vec![GeoSubsetPredicate {
            predicate_id: "predicate.discovery.h3".to_string(),
            kind: GeoSubsetPredicateKind::H3Cells,
            expression: "declared h3 r8 center plus controlled halo k=1".to_string(),
        }],
    }
}

fn discovery_contract_fields() -> Vec<GeoRequestedField> {
    vec![
        GeoRequestedField {
            field_id: "source_record_id".to_string(),
            role: GeoFieldRole::Identifier,
            required: true,
        },
        GeoRequestedField {
            field_id: "geometry_wkb_sha256".to_string(),
            role: GeoFieldRole::Digest,
            required: true,
        },
        GeoRequestedField {
            field_id: "h3_cell".to_string(),
            role: GeoFieldRole::Ordering,
            required: true,
        },
    ]
}

fn discovery_contract_fields_with_geometry() -> Vec<GeoRequestedField> {
    let mut fields = discovery_contract_fields();
    fields.push(GeoRequestedField {
        field_id: "footprint_wkb".to_string(),
        role: GeoFieldRole::Geometry,
        required: true,
    });
    fields
}

fn discovery_contract_projection() -> GeoProjectionOperation {
    GeoProjectionOperation {
        coordinate_reference_system: "EPSG:4326".to_string(),
        operation_id: "identity-wgs84".to_string(),
        operation_version: "v1".to_string(),
        operation_digest: contract_sha256_digest("projection.identity-wgs84", b"identity-wgs84"),
    }
}

fn discovery_contract_request() -> GeoDiscoveryRequest {
    let subset = discovery_contract_subset();
    let fields = discovery_contract_fields();
    let mut request = GeoDiscoveryRequest {
        version: CANON_GEO_DISCOVERY_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        bounded_geography: control_region(),
        subset: subset.clone(),
        requested_entity_levels: vec![GeoControlEntityLevel::Building],
        requested_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
        release_selection: GeoDiscoveryReleaseSelectionPolicy {
            as_of_utc_day: "2026-08-31".to_string(),
            mode: GeoReleaseSelectionMode::LatestNotAfterAsOf,
            candidate_release_ids: Vec::new(),
        },
        releases: Vec::new(),
        fields: fields.clone(),
        required_steps: vec![
            GeoDiscoveryStep::CatalogSearch,
            GeoDiscoveryStep::ListReleases,
            GeoDiscoveryStep::DescribeSchema,
            GeoDiscoveryStep::ColumnReadabilityProbe,
        ],
        column_readability_probe: GeoColumnReadabilityProbe {
            probe_id: "probe.fixture.discovery-columns".to_string(),
            fields: fields.iter().map(|field| field.field_id.clone()).collect(),
            subset,
            ceilings: GeoRowByteCeilings {
                max_rows: 5,
                max_bytes: 8192,
            },
        },
        ceilings: GeoRowByteCeilings {
            max_rows: 5,
            max_bytes: 8192,
        },
    };
    request.request_id = geo_discovery_request_id(&request).expect("discovery id computes");
    request
}

fn acquisition_contract_request() -> GeoAcquisitionRequest {
    let mut request = GeoAcquisitionRequest {
        version: CANON_GEO_ACQUISITION_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        discovery_request_id: Some(discovery_contract_request().request_id),
        bounded_geography: control_region(),
        subset: discovery_contract_subset(),
        releases: vec![GeoReleasePin {
            source_instance_id: "source.fixture.building-footprints".to_string(),
            release_id: "release.fixture.2026-08-31".to_string(),
            release_digest: contract_sha256_digest(
                "release.fixture.building-footprints",
                b"release.fixture.2026-08-31",
            ),
        }],
        fields: discovery_contract_fields(),
        projection: None,
        ordering: vec![GeoOrderingTerm {
            position: 0,
            field_id: "source_record_id".to_string(),
            direction: GeoOrderDirection::Asc,
            nulls: GeoNullOrdering::Last,
        }],
        pagination: GeoPaginationRequest {
            page_size_rows: 10,
            page_token: None,
        },
        ceilings: GeoRowByteCeilings {
            max_rows: 10,
            max_bytes: 1_048_576,
        },
        positive_path_min_rows: 1,
    };
    request.request_id = geo_acquisition_request_id(&request).expect("acquisition id computes");
    request
}

fn geometric_acquisition_contract_request() -> GeoAcquisitionRequest {
    let mut request = acquisition_contract_request();
    request.fields = discovery_contract_fields_with_geometry();
    request.projection = Some(discovery_contract_projection());
    request.request_id = geo_acquisition_request_id(&request).expect("acquisition id computes");
    request
}

fn acquisition_contract_receipt() -> GeoAcquisitionReceipt {
    acquisition_contract_receipt_for(acquisition_contract_request())
}

fn geometric_acquisition_contract_receipt() -> GeoAcquisitionReceipt {
    acquisition_contract_receipt_for(geometric_acquisition_contract_request())
}

fn error_population_contract_artifact() -> GeoErrorPopulationArtifact {
    let subjects = vec![
        GeoErrorPopulationSubject {
            subject_id: "subject.h7.non-round".to_string(),
            truth_plane: GeoTruthPlane::NonRoundAmountDateLegalBorough,
            window_blake3: blake3::hash(b"window.h7.non-round").to_hex().to_string(),
            parcel_ids: vec!["1000000001".to_string(), "1000000002".to_string()],
        },
        GeoErrorPopulationSubject {
            subject_id: "subject.h7.round".to_string(),
            truth_plane: GeoTruthPlane::RoundExactLenderParty,
            window_blake3: blake3::hash(b"window.h7.round").to_hex().to_string(),
            parcel_ids: vec!["2000000001".to_string(), "2000000002".to_string()],
        },
    ];
    GeoErrorPopulationArtifact {
        version: CANON_GEO_ERROR_POPULATION_VERSION.to_string(),
        population_id: "population.fixture.h7.observer".to_string(),
        region: "nyc".to_string(),
        selection_seed: Some(0x2d6d3440a11f2026),
        selection_query_blake3: Some(
            blake3::hash(b"query.fixture.h7.observer")
                .to_hex()
                .to_string(),
        ),
        source_population_blake3: Some(
            blake3::hash(b"source.population.fixture.h7.observer")
                .to_hex()
                .to_string(),
        ),
        subjects,
        declared_before_observer_ids: vec![
            "observer.count.frozen".to_string(),
            "observer.null_footprint".to_string(),
        ],
        stratum_counts: [
            ("non_round_amount_date_legal_borough".to_string(), 1),
            ("round_exact_lender_party".to_string(), 1),
        ]
        .into_iter()
        .collect(),
    }
}

fn acquisition_contract_receipt_for(request: GeoAcquisitionRequest) -> GeoAcquisitionReceipt {
    GeoAcquisitionReceipt {
        version: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        request_id: request.request_id.clone(),
        request_semantic_hash: geo_acquisition_request_semantic_hash(&request)
            .expect("request hash computes"),
        terminal_state: GeoAcquisitionTerminalState::Complete,
        proof_class: GeoAcquisitionProofClass::Live,
        executor: Some(GeoExecutorTrace {
            executor_kind: GeoExecutorKind::QueryEngine,
            executor_id: "fixture-query-engine".to_string(),
            executor_version: "v1".to_string(),
            tool_id: "fixture-query-tool".to_string(),
            tool_version: "v1".to_string(),
            executor_request_id: "request-123".to_string(),
            executor_query_id: "query-123".to_string(),
            executor_attempt_id: None,
        }),
        fixture_id: None,
        retained_receipt_id: None,
        bounded_geography: request.bounded_geography.clone(),
        subset: request.subset.clone(),
        releases: request.releases.clone(),
        fields: request.fields.clone(),
        projection: request.projection.clone(),
        normalized_executed_request_digest: contract_sha256_digest(
            "executor.normalized_request",
            b"normalized executor request",
        ),
        pagination: GeoPaginationReceipt {
            requested_page: request.pagination.clone(),
            next_page_token: None,
            rows_truncated: false,
            bytes_truncated: false,
        },
        counts: GeoAcquisitionCounts {
            rows: 2,
            bytes: 512,
        },
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "denominator.result.rows".to_string(),
            source: GeoDenominatorSource::ResultArtifact,
            count: 2,
            unit: "row".to_string(),
            description: "Rows in the bounded subset result".to_string(),
        }],
        source_digests: vec![request.releases[0].release_digest.clone()],
        result_digests: vec![contract_blake3_digest("result.rows", b"result rows")],
        local_artifacts: vec![GeoLocalArtifactDigest {
            artifact_id: "artifact.fixture.rows".to_string(),
            media_type: "application/jsonl".to_string(),
            byte_count: 512,
            digest: contract_blake3_digest("artifact.rows", b"artifact rows"),
        }],
        artifact_release_relations: vec![artifact_release_relation(
            "artifact.fixture.rows",
            &request.releases[0],
        )],
        unreadable_columns: Vec::new(),
        resumability: GeoAcquisitionResumability {
            resumable: false,
            resume_token: None,
            resume_request_id: None,
            retry_guidance: "terminal receipt requires no resume action".to_string(),
        },
        terminal_detail: None,
    }
}

fn control_as_of() -> GeoAsOf {
    GeoAsOf {
        utc_day: "2026-08-31".to_string(),
        semantic_id: "question.query_as_of.utc_day".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    }
}

fn control_budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.control".to_string(),
        deterministic_bounds: vec![GeoNumericBound {
            semantic_id: "budget.max_models".to_string(),
            counter: GeoResourceCounter::Models,
            value: 16,
            unit: "model".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::TruncatePresentationOnly,
        }],
        telemetry: vec![GeoTelemetryDeclaration {
            metric: GeoTelemetryMetric::WallTime,
            unit: "millisecond".to_string(),
            origin: GeoValueOrigin::OperatorPolicy,
            semantic_effect: GeoTelemetrySemanticEffect::None,
        }],
    }
}

fn control_question() -> GeoQuestion {
    GeoQuestion {
        version: CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: "question.fixture.control".to_string(),
        subject_bindings: vec![
            GeoSubjectBinding {
                role: "operator_case".to_string(),
                binding_class: GeoSubjectBindingClass::OperatorLabel,
                value: "case-control".to_string(),
            },
            GeoSubjectBinding {
                role: "input_address".to_string(),
                binding_class: GeoSubjectBindingClass::AddressText,
                value: "10 Fixture St".to_string(),
            },
        ],
        bounded_geography: control_region(),
        requested_grains: vec![GeoRequestedGrain {
            entity_level: GeoControlEntityLevel::Building,
            required_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            optional_evidence_classes: vec![GeoEvidenceClass::AddressSet],
        }],
        query_as_of: Some(control_as_of()),
        requested_claim_classes: vec![GeoClaimClass::StableIdentity, GeoClaimClass::CandidateReach],
        presentation_limits: vec![GeoNumericBound {
            semantic_id: "question.presentation.max_models".to_string(),
            counter: GeoResourceCounter::Models,
            value: 16,
            unit: "model".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::TruncatePresentationOnly,
        }],
        abstention_policy: GeoAbstentionPolicy {
            unsupported_grain: GeoAbstentionDisposition::ReportUnsupported,
            unresolved_residual: GeoAbstentionDisposition::ReportResidual,
            budget_fallback: GeoAbstentionDisposition::ReportResidual,
        },
        decision_policy: None,
        resource_budget_ref: "budget.fixture.control".to_string(),
    }
}

fn control_inventory() -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.control".to_string(),
        region: control_region(),
        sources: vec![GeoRegionalSourceInstance {
            source_instance_id: "source.fixture.building-footprints".to_string(),
            release: GeoSourceRelease {
                release_id: "release.fixture.building-footprints".to_string(),
                release_digest: control_digest("release.fixture.building-footprints"),
            },
            temporal_scope: GeoTemporalScope {
                valid_time: Some(canon::geo::GeoDateInterval {
                    start_utc_day: "2026-01-01".to_string(),
                    end_utc_day: "2026-12-31".to_string(),
                }),
                transaction_time: None,
                release_time: Some(control_as_of()),
            },
            lineage_ids: vec!["lineage.fixture.building-footprints".to_string()],
            native_scope: GeoNativeEntityScope::NativeEntity {
                entity_level: GeoControlEntityLevel::Building,
                identity_participation: GeoIdentityParticipation::EvidenceOnly,
            },
            evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            coverage: GeoCoveragePredicate {
                coverage_id: "coverage.fixture.control".to_string(),
                region: control_region(),
                predicate: "all declared fixture records in the control region".to_string(),
            },
            local_state: GeoLocalAcquisitionState {
                state: GeoSourceAvailability::Available,
                local_ref: Some(GeoLocalArtifactRef {
                    artifact_id: "local.fixture.building-footprints".to_string(),
                    contract_version: "canon_geo_warehouse_rows.v0".to_string(),
                    content_hash: control_digest("local.fixture.building-footprints"),
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
        }],
        discovery_gaps: Vec::new(),
    }
}

fn regional_inventory_advancement_contract_artifact() -> GeoRegionalInventoryAdvancement {
    let advanced_inventory = control_inventory();
    let source = advanced_inventory.sources[0].clone();
    let local_ref = source
        .local_state
        .local_ref
        .clone()
        .expect("advanced fixture source is locally available");
    let subset = GeoBoundedSubset {
        subset_id: "subset.fixture.inventory-advancement".to_string(),
        geography: control_region(),
        h3_cells: vec!["882a107707fffff".to_string()],
        predicates: vec![GeoSubsetPredicate {
            predicate_id: "predicate.fixture.inventory-advancement.h3".to_string(),
            kind: GeoSubsetPredicateKind::H3Cells,
            expression: "declared h3 r8 cell for the advancement contract fixture".to_string(),
        }],
    };
    let bounded_subset_hash = format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(&subset).expect("subset serializes")).to_hex()
    );
    let release_hex = source
        .release
        .release_digest
        .strip_prefix("blake3:")
        .expect("fixture release digest is blake3")
        .to_string();
    let advanced_inventory_semantic_hash =
        regional_inventory_semantic_hash(&advanced_inventory).expect("advanced inventory hash");
    let mut advancement = GeoRegionalInventoryAdvancement {
        version: CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION.to_string(),
        advancement_id: String::new(),
        semantic_hash: String::new(),
        effect: GeoInventoryAdvancementEffect::LocalAvailabilityOnly,
        plan_id: format!("canon_geo_plan.v0:{}", "a".repeat(64)),
        plan_semantic_hash: control_digest("plan.fixture.inventory-advancement"),
        request_id: format!("canon_geo_acquisition_request.v0:{}", "b".repeat(64)),
        request_semantic_hash: control_digest("request.fixture.inventory-advancement"),
        base_inventory_id: "inventory.fixture.base".to_string(),
        base_inventory_semantic_hash: control_digest("inventory.fixture.base"),
        advanced_inventory_id: advanced_inventory.inventory_id.clone(),
        advanced_inventory_semantic_hash,
        bounded_geography: control_region(),
        bounded_subset: subset,
        bounded_subset_hash,
        receipt_file: GeoSatisfactionFileAudit {
            file_id: "receipt.fixture.inventory-advancement".to_string(),
            byte_count: 512,
            digest: control_digest("receipt.fixture.inventory-advancement"),
        },
        receipt_execution: GeoSatisfactionExecutionRef {
            proof_class: GeoAcquisitionProofClass::Live,
            terminal_state: GeoAcquisitionTerminalState::Complete,
            fixture_id: None,
            retained_receipt_id: None,
            executor_request_id: Some("request-123".to_string()),
            executor_query_id: Some("query-123".to_string()),
            executor_attempt_id: Some("attempt-1".to_string()),
        },
        receipt_terminal_state: GeoAcquisitionTerminalState::Complete,
        proof_class: GeoAcquisitionProofClass::Live,
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "denominator.result.rows".to_string(),
            source: GeoDenominatorSource::ResultArtifact,
            count: 2,
            unit: "row".to_string(),
            description: "Rows in the bounded acquisition result".to_string(),
        }],
        source_digests: vec![GeoDigest {
            digest_id: "source.release.fixture".to_string(),
            algorithm: GeoDigestAlgorithm::Blake3,
            hex_digest: release_hex,
        }],
        result_digests: vec![GeoDigest {
            digest_id: "result.rows".to_string(),
            algorithm: GeoDigestAlgorithm::Blake3,
            hex_digest: local_ref
                .content_hash
                .strip_prefix("blake3:")
                .expect("local ref content hash is blake3")
                .to_string(),
        }],
        source_advancements: vec![GeoRegionalInventorySourceAdvancement {
            source_instance_id: source.source_instance_id,
            release: source.release,
            previous_state: GeoSourceAvailability::Missing,
            advanced_state: GeoSourceAvailability::Available,
            local_ref: local_ref.clone(),
            local_artifact_byte_count: 512,
            local_artifact_contract_version: Some(local_ref.contract_version.clone()),
            result_digest_ids: vec!["result.rows".to_string()],
        }],
        advanced_inventory,
    };
    advancement.semantic_hash = geo_regional_inventory_advancement_semantic_hash(&advancement)
        .expect("advancement hash computes");
    advancement.advancement_id = format!(
        "{CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION}:{}",
        advancement
            .semantic_hash
            .strip_prefix("blake3:")
            .expect("advancement semantic hash is blake3")
    );
    canonical_geo_regional_inventory_advancement_bytes(&advancement)
        .expect("advancement fixture passes canonical validation");
    advancement
}

fn small_universe() -> GeoCompositionUniverse {
    GeoCompositionUniverse {
        parcels: vec!["parcel-a".to_string(), "parcel-b".to_string()],
        buildings: vec![GeoBuildingCandidate {
            id: "building-a".to_string(),
            parcel_ids: vec!["parcel-a".to_string()],
        }],
    }
}

fn composition_request() -> GeoCompositionRequest {
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: small_universe(),
        hard_constraints: vec![GeoHardConstraint {
            id: "any-of-parcels".to_string(),
            constraint: GeoHardConstraintKind::AnyOf {
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
                ],
            },
        }],
        soft_preferences: Vec::new(),
        max_assignments: 64,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn propagation_request() -> GeoCompositionRequest {
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        universe: GeoCompositionUniverse {
            parcels: vec![
                "parcel-a".to_string(),
                "parcel-b".to_string(),
                "parcel-c".to_string(),
            ],
            buildings: Vec::new(),
        },
        hard_constraints: vec![GeoHardConstraint {
            id: "band.fixture.schema.area".to_string(),
            constraint: GeoHardConstraintKind::IntegerSumBand {
                level: GeoEntityLevel::Parcel,
                measure: GeoIntegerMeasure {
                    semantic_id: "fixture.area".to_string(),
                    unit: "square_foot".to_string(),
                    value_origin: GeoIntegerValueOrigin::SourceAsserted,
                },
                values: vec![
                    GeoIntegerMemberValue {
                        id: "parcel-a".to_string(),
                        value: 100,
                    },
                    GeoIntegerMemberValue {
                        id: "parcel-b".to_string(),
                        value: 200,
                    },
                    GeoIntegerMemberValue {
                        id: "parcel-c".to_string(),
                        value: 5_000,
                    },
                ],
                min: 5_000,
                max: 5_200,
            },
        }],
        soft_preferences: Vec::new(),
        max_assignments: 64,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn explanation_evidence_request() -> GeoEvidenceCompilationRequest {
    GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        universe: GeoCompositionUniverse {
            parcels: vec!["schema-parcel-a".to_string(), "schema-parcel-b".to_string()],
            buildings: Vec::new(),
        },
        contracts: vec![
            schema_rho_contract("schema-contract-a"),
            schema_rho_contract("schema-contract-b"),
        ],
        observations: vec![
            GeoRhoObservation {
                id: "schema-observation-a".to_string(),
                contract_id: "schema-contract-a".to_string(),
                source_records: vec![schema_source_record("schema-record-a")],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![vec!["schema-parcel-a".to_string()]],
                },
            },
            GeoRhoObservation {
                id: "schema-observation-b".to_string(),
                contract_id: "schema-contract-b".to_string(),
                source_records: vec![schema_source_record("schema-record-b")],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![vec!["schema-parcel-b".to_string()]],
                },
            },
        ],
        max_assignments: 16,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn explanation_request_order() -> GeoReliabilityOrder {
    GeoReliabilityOrder {
        contract_ids_most_reliable_first: vec![
            "schema-contract-a".to_string(),
            "schema-contract-b".to_string(),
        ],
    }
}

fn separation_request() -> GeoSeparationRequest {
    GeoSeparationRequest {
        version: CANON_GEO_SEPARATION_REQUEST_VERSION.to_string(),
        subject_ref: None,
        request: composition_request(),
        prospective: vec![GeoProspectiveObservation {
            id: "schema-observation-parcel-choice".to_string(),
            contract_id: "schema-contract-parcel-choice".to_string(),
            cost_units: 2,
            outcomes: vec![
                GeoProspectiveOutcome {
                    outcome_id: "schema-outcome-parcel-a".to_string(),
                    induced: vec![GeoHardConstraintKind::Require {
                        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                    }],
                },
                GeoProspectiveOutcome {
                    outcome_id: "schema-outcome-parcel-b".to_string(),
                    induced: vec![GeoHardConstraintKind::Require {
                        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
                    }],
                },
            ],
        }],
    }
}

fn next_evidence_fixture() -> (
    canon::geo::GeoCompositionArtifact,
    canon::geo::GeoSeparationArtifact,
    GeoNextEvidenceRequest,
) {
    let composition =
        solve_composition(&composition_request()).expect("schema next-evidence composition solves");
    let separation = separate(&separation_request(), &GeoExplanationBudget::default())
        .expect("schema next-evidence separation builds");
    let observation = separation
        .per_observation
        .first()
        .expect("schema separation has an observation");
    let candidates = vec![GeoNextAction {
        action_id: observation.observation_id.clone(),
        class: GeoNextActionClass::SeparateResidual,
        kind: GeoNextActionKind::Observe(observation.observation_id.clone()),
        observation_id: Some(observation.observation_id.clone()),
        cost_units: 2,
        separation: observation.per_outcome.clone(),
        worst_case_remaining: observation.worst_case_remaining,
        redundant: observation.redundant,
        lineage_ids: Vec::new(),
        dominated_by: Vec::new(),
        stop_reason: None,
    }];
    let request = GeoNextEvidenceRequest {
        version: CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION.to_string(),
        composition_blake3: prefixed_blake3(
            &canonical_composition_bytes(&composition).expect("schema composition canonicalizes"),
        ),
        separation_blake3: prefixed_blake3(
            &canonical_separation_bytes(&separation).expect("schema separation canonicalizes"),
        ),
        candidates,
        policy: None,
        budget: control_budget(),
        budget_spent: BTreeMap::new(),
    };
    (composition, separation, request)
}

fn schema_rho_contract(id: &str) -> GeoRhoContract {
    GeoRhoContract {
        id: id.to_string(),
        version: "v1".to_string(),
        source_dataset: "schema-fixture-dataset".to_string(),
        source_release: "schema-fixture-release".to_string(),
        source_lineage_ids: vec![format!("schema-lineage-{id}")],
        method_id: "schema-fixture-method".to_string(),
        method_version: "v1".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: format!("schema-invariant-{id}"),
        },
    }
}

fn schema_source_record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "schema-fixture-release".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn evidence_request() -> GeoEvidenceCompilationRequest {
    GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: small_universe(),
        contracts: vec![GeoRhoContract {
            id: "contract-1".to_string(),
            version: "v1".to_string(),
            source_dataset: "fixture:dataset".to_string(),
            source_release: "fixture-v1".to_string(),
            source_lineage_ids: vec!["fixture:upstream-dataset".to_string()],
            method_id: "fixture:method".to_string(),
            method_version: "v1".to_string(),
            claim_role: GeoEvidenceClaimRole::AttributeObservation,
            basis: GeoRhoBasis::LogicalRelaxation {
                invariant_id: "fixture:invariant".to_string(),
            },
        }],
        observations: vec![GeoRhoObservation {
            id: "obs-1".to_string(),
            contract_id: "contract-1".to_string(),
            source_records: vec![GeoEvidenceRecordRef {
                source_record_id: "row-1".to_string(),
                source_vintage: "fixture-v1".to_string(),
                record_blake3: blake3::hash(b"row-1").to_hex().to_string(),
            }],
            valid_time: None,
            observation: GeoRhoObservationKind::ExistentialMembership {
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
                ],
            },
        }],
        max_assignments: 64,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn warehouse_rows_request() -> GeoWarehouseRowsRequest {
    let evidence = evidence_request();
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: Default::default(),
        parcel_rows: evidence
            .universe
            .parcels
            .iter()
            .cloned()
            .map(|parcel_id| GeoWarehouseParcelRow { parcel_id })
            .collect(),
        building_parcel_rows: Vec::new(),
        contracts: evidence.contracts,
        evidence_rows: evidence
            .observations
            .into_iter()
            .flat_map(|observation| {
                observation
                    .source_records
                    .into_iter()
                    .map(move |source_record| GeoWarehouseEvidenceRow {
                        observation_id: observation.id.clone(),
                        contract_id: observation.contract_id.clone(),
                        source_record,
                        valid_time: observation.valid_time,
                        observation: observation.observation.clone(),
                    })
            })
            .collect(),
        max_assignments: evidence.max_assignments,
        max_materialized_models: evidence.max_materialized_models,
    }
}

fn geometry_request() -> GeoGeometryTileRequest {
    GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.to_string(),
        frame: GeoLocalFrameContract {
            version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
            frame_id: "tile:fixture:local-mm:v1".to_string(),
            tile_id: "fixture".to_string(),
            source_crs: "LOCAL:FIXTURE".to_string(),
            source_axis_domain: GeoSourceAxisDomain::Planar,
            source_decimal_places: 3,
            source_origin: GeoSourcePointFixed { x: 0, y: 0 },
            affine: GeoAffineProjectionMm {
                x_from_source_x_numerator: 1,
                x_from_source_y_numerator: 0,
                y_from_source_x_numerator: 0,
                y_from_source_y_numerator: 1,
                denominator: 1,
            },
            projection: GeoProjectionProvenance {
                method_id: "fixture-affine".to_string(),
                method_version: "v1".to_string(),
                parameters_blake3: blake3::hash(b"fixture-affine").to_hex().to_string(),
                max_projection_error_micrometres: 200,
            },
            max_abs_coordinate_mm: 2_000_000,
        },
        features: vec![GeoGeometryFeatureInput {
            feature_id: "parcel-a".to_string(),
            source_crs: "LOCAL:FIXTURE".to_string(),
            geometry: GeoSourceGeometry::Polygon {
                exterior: vec![
                    source_point("0", "0"),
                    source_point("5", "0"),
                    source_point("5", "5"),
                    source_point("0", "5"),
                    source_point("0", "0"),
                ],
                holes: Vec::new(),
            },
        }],
        max_vertices_per_geometry: 100,
        max_geometry_bytes_per_tile: 100_000,
    }
}

fn client_tile_ingest_schema_fixture() -> (GeoClientTileIngestRequest, String) {
    let resolution = Resolution::Nine;
    let center = LatLng::new(40.753000, -73.977000)
        .unwrap()
        .to_cell(resolution)
        .to_string();
    let mut work_cells = h3o::CellIndex::from_str(&center)
        .unwrap()
        .grid_disk::<Vec<_>>(1)
        .into_iter()
        .map(|cell| cell.to_string())
        .collect::<Vec<_>>();
    work_cells.sort();
    let source = serde_json::json!({
        "type": "Feature",
        "id": "row-1",
        "properties": {
            "apn": "client-apn-1"
        },
        "geometry": {
            "type": "Polygon",
            "coordinates": [[
                [-73.977100, 40.752900],
                [-73.976900, 40.752900],
                [-73.976900, 40.753100],
                [-73.977100, 40.753100],
                [-73.977100, 40.752900]
            ]]
        }
    })
    .to_string();
    (
        GeoClientTileIngestRequest {
            version: CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION.to_string(),
            tile_id: center.clone(),
            source_format: GeoClientTileSourceFormat::GeoJson,
            source_path: "client/parcels.geojson".to_string(),
            source_digest: blake3::hash(source.as_bytes()).to_hex().to_string(),
            declared_crs: "EPSG:4326".to_string(),
            frame: GeoLocalFrameContract {
                version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
                frame_id: format!("client:{center}:wgs84-local-affine:v0"),
                tile_id: center.clone(),
                source_crs: "EPSG:4326".to_string(),
                source_axis_domain: GeoSourceAxisDomain::GeographicLongitudeLatitude,
                source_decimal_places: 6,
                source_origin: GeoSourcePointFixed {
                    x: -74_000_000,
                    y: 40_000_000,
                },
                affine: GeoAffineProjectionMm {
                    x_from_source_x_numerator: 1,
                    x_from_source_y_numerator: 0,
                    y_from_source_x_numerator: 0,
                    y_from_source_y_numerator: 1,
                    denominator: 1,
                },
                projection: GeoProjectionProvenance {
                    method_id: "fixture:wgs84-local-affine".to_string(),
                    method_version: "v0".to_string(),
                    parameters_blake3: blake3::hash(format!("fixture:{center}").as_bytes())
                        .to_hex()
                        .to_string(),
                    max_projection_error_micrometres: 10_000_000,
                },
                max_abs_coordinate_mm: 10_000_000,
            },
            source_instance_id: "source.client.parcels".to_string(),
            release_id: "client-parcels-2026-q3".to_string(),
            release_digest: blake3::hash(b"client-parcels-2026-q3").to_hex().to_string(),
            vendor: "county".to_string(),
            vintage: "2026-Q3".to_string(),
            vendor_identifier: GeoClientTileVendorIdentifier {
                issuer: "county".to_string(),
                role: "apn".to_string(),
                property: "apn".to_string(),
            },
            source_record_id_property: None,
            supplemental_h3_cells_property: None,
            license_expression: "LicenseRef-Client-Parcel-Local".to_string(),
            coverage_extent: GeoClientTileCoverageExtent {
                extent_id: "client-declared-h3-k1".to_string(),
                kind: GeoClientTileCoverageExtentKind::ClientDeclaredH3CellSet,
                h3_cells: work_cells.clone(),
            },
            mutual_exclusivity_declared: false,
            h3_resolution: 9,
            halo_k: 1,
            work_cells,
            max_features: 8,
            max_vertices_per_geometry: 64,
            max_geometry_bytes_per_tile: 100_000,
        },
        source,
    )
}

fn warehouse_geometry_request() -> GeoWarehouseGeometryRowsRequest {
    let points = [
        (980_252.301_632_881_2_f64, 191_655.610_172_272_3_f64),
        (980_352.301_632_881_2, 191_655.610_172_272_3),
        (980_352.301_632_881_2, 191_755.610_172_272_3),
        (980_252.301_632_881_2, 191_755.610_172_272_3),
        (980_252.301_632_881_2, 191_655.610_172_272_3),
    ];
    let mut wkb = Vec::new();
    wkb.push(1);
    wkb.extend_from_slice(&3_u32.to_le_bytes());
    wkb.extend_from_slice(&1_u32.to_le_bytes());
    wkb.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for (x, y) in points {
        wkb.extend_from_slice(&x.to_le_bytes());
        wkb.extend_from_slice(&y.to_le_bytes());
    }
    let sha256: String = Sha256::digest(&wkb)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    GeoWarehouseGeometryRowsRequest {
        version: CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION.to_string(),
        tile_id: "892a100d26bffff".to_string(),
        frame_id: "tile:892a100d26bffff:epsg2263-mm:v0".to_string(),
        source_crs: "EPSG:2263".to_string(),
        source_srid: 2263,
        source_decimal_places: 9,
        source_origin: source_point("980000", "191000"),
        source_unit_to_millimetres: GeoExactSourceUnitMm {
            unit_id: "us-survey-foot".to_string(),
            numerator: 1_200_000,
            denominator: 3_937,
        },
        rows: vec![GeoWarehouseGeometryRow {
            feature_id: "parcel-a".to_string(),
            source_record_id: "mn/000000/1".to_string(),
            source_dataset: "nyc_dcp_mappluto".to_string(),
            source_release: "26v2".to_string(),
            source_release_date: "2026-08-01".to_string(),
            source_geometry_contract_version: "nyc_dcp_mappluto_geometry_evidence.v3".to_string(),
            source_archive_sha256:
                "e06eca9034731bc23f058bf532090e3c1ea6aed44a8128c6928f33872da34ab5".to_string(),
            source_crs: "EPSG:2263".to_string(),
            source_srid: 2263,
            source_geom_wkb_base64: BASE64_STANDARD.encode(&wkb),
            source_geom_wkb_sha256: sha256,
            transform_execution_id: "sha256-execution-26v2".to_string(),
            transform_definition_id: "sha256-definition-hpgn".to_string(),
        }],
        max_abs_coordinate_mm: 1_000_000,
        max_vertices_per_geometry: 10_000,
        max_geometry_bytes_per_tile: 1_000_000,
    }
}

fn tile_source_binding(
    source_instance_id: &str,
    entity_level: GeoControlEntityLevel,
    identity_participation: GeoIdentityParticipation,
) -> GeoTileSourceBinding {
    GeoTileSourceBinding {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: format!("{source_instance_id}.release"),
            release_digest: format!(
                "blake3:{}",
                blake3::hash(source_instance_id.as_bytes()).to_hex()
            ),
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level,
            identity_participation,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "inventory.fixture.schemas".to_string(),
            semantic_hash: format!("blake3:{}", blake3::hash(b"schema-semantic").to_hex()),
            planning_hash: format!("blake3:{}", blake3::hash(b"schema-planning").to_hex()),
        },
    }
}

fn tile_work_request() -> GeoTileWorkRequest {
    let feature = GeoTileFeatureRef {
        source: tile_source_binding(
            "mappluto-parcel",
            GeoControlEntityLevel::Parcel,
            GeoIdentityParticipation::StableAlias,
        ),
        feature_id: "parcel-a".to_string(),
        home_cell: "892a100d26bffff".to_string(),
    };
    GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: "892a100d26bffff".to_string(),
        halo_k: 1,
        features: vec![feature.clone()],
        candidate_reach_reference: Some(GeoTileCandidateReachReference {
            reference_id: "reference.fixture.schemas".to_string(),
            reference_kind: GeoTileCandidateReachReferenceKind::CompleteBoundedReference,
            members: vec![feature],
            max_members: 8,
        }),
        max_features: 8,
        max_work_cells: 7,
    }
}

fn home_cell_rows_request() -> GeoHomeCellRowsRequest {
    GeoHomeCellRowsRequest {
        version: CANON_GEO_HOME_CELL_ROWS_VERSION.to_string(),
        coordinate_crs: "EPSG:4326".to_string(),
        coordinate_decimal_places: 9,
        h3_resolution: 8,
        stability_radius_fixed: 1_000,
        rows: vec![GeoHomeCellRow {
            source: tile_source_binding(
                "mappluto-parcel",
                GeoControlEntityLevel::Parcel,
                GeoIdentityParticipation::StableAlias,
            ),
            feature_id: "parcel-a".to_string(),
            source_record_id: "mn/000000/1".to_string(),
            geometry_sha256: "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed"
                .to_string(),
            representative_point_method: "centroid_of_derived_wgs84_geometry".to_string(),
            longitude: "-73.977264000".to_string(),
            latitude: "40.753429000".to_string(),
            transform_execution_id: Some("sha256-execution-26v2".to_string()),
            transform_definition_id: Some("sha256-definition-hpgn".to_string()),
            claimed_home_cell: None,
        }],
        max_rows: 8,
    }
}

fn tile_reconciliation_request() -> GeoTileReconciliationRequest {
    let work_unit =
        materialize_tile_work_unit(&tile_work_request()).expect("tile work unit materializes");
    let work_unit_blake3 = work_unit.work_unit_blake3.clone();
    GeoTileReconciliationRequest {
        version: CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION.to_string(),
        halo_k: 1,
        inventory_lineage: None,
        batches: vec![GeoTileDecisionBatch {
            work_unit,
            proposals: vec![GeoTileDecisionProposal {
                semantics: GeoTileDecisionSemantics::Composition,
                work_unit_blake3,
                payload_blake3: format!("blake3:{}", blake3::hash(b"fixture decision").to_hex()),
                members: vec![GeoTileDecisionMember {
                    source: tile_source_binding(
                        "mappluto-parcel",
                        GeoControlEntityLevel::Parcel,
                        GeoIdentityParticipation::StableAlias,
                    ),
                    feature_id: "parcel-a".to_string(),
                    candidate_entity_level: GeoControlEntityLevel::Parcel,
                    home_cell: "892a100d26bffff".to_string(),
                }],
            }],
        }],
        max_batches: 4,
        max_proposals: 8,
        max_members_per_decision: 8,
        max_features_per_batch: 8,
        max_work_cells_per_batch: 7,
    }
}

fn source_point(x: &str, y: &str) -> GeoSourcePointDecimal {
    GeoSourcePointDecimal {
        x: x.to_string(),
        y: y.to_string(),
    }
}

fn multisource_request(root: &Path) -> GeoMultisourceRequest {
    let source_specs = [
        ("parcel", EntitySourceRole::Reference, "entity:parcel"),
        ("property", EntitySourceRole::Target, "entity:parcel"),
        ("footprint", EntitySourceRole::Peer, "entity:other"),
    ];
    let sources = source_specs
        .into_iter()
        .map(|(name, role, canonical_id)| {
            let rows_path = root.join(format!("{name}.csv"));
            fs::write(
                &rows_path,
                format!("source_row_id,anchor_id,canonical_id\n{name}-1,shared,{canonical_id}\n"),
            )
            .expect("write multisource schema fixture");
            GeoMultisourceSource {
                name: name.to_string(),
                role,
                rows_path,
                local_id_column: Some("source_row_id".to_string()),
                anchor_namespace: Some("fixture-anchor".to_string()),
                anchor_column: Some("anchor_id".to_string()),
                canonical_id_column: Some("canonical_id".to_string()),
            }
        })
        .collect();
    GeoMultisourceRequest {
        version: CANON_GEO_MULTISOURCE_REQUEST_VERSION.to_string(),
        sources,
        comparison_graph: Vec::new(),
        default_pair_budget: 8,
    }
}

#[test]
fn address_parse_request_schema_matches_a_real_instance() {
    let instance =
        serde_json::to_value(address_parse_request()).expect("address request serializes");
    assert_drift_free(
        ADDRESS_PARSE_REQUEST_SCHEMA,
        "canon.geo.address_parse_request.v0",
        CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn address_parse_forest_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(address_parse_forest()).expect("forest serializes");
    assert_drift_free(
        ADDRESS_PARSE_FOREST_SCHEMA,
        "canon.geo.address_parse_forest.v0",
        CANON_GEO_ADDRESS_PARSE_FOREST_VERSION,
        &instance,
    );
}

#[test]
fn pad_address_set_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(address_pad_set()).expect("PAD set serializes");
    assert_drift_free(
        PAD_ADDRESS_SET_SCHEMA,
        "canon.geo.pad_address_set.v0",
        CANON_GEO_PAD_ADDRESS_SET_VERSION,
        &instance,
    );
}

#[test]
fn pad_membership_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(address_pad_membership()).expect("membership serializes");
    assert_drift_free(
        PAD_MEMBERSHIP_SCHEMA,
        "canon.geo.pad_membership.v0",
        CANON_GEO_PAD_MEMBERSHIP_VERSION,
        &instance,
    );
}

#[test]
fn control_question_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(control_question()).expect("question serializes");
    assert_drift_free(
        CONTROL_QUESTION_SCHEMA,
        "canon.geo.question.v0",
        CANON_GEO_QUESTION_VERSION,
        &instance,
    );
}

#[test]
fn control_capabilities_schema_matches_a_real_instance() {
    let capabilities = default_geo_capabilities().expect("capabilities build");
    let instance = serde_json::to_value(capabilities).expect("capabilities serialize");
    assert_drift_free(
        CONTROL_CAPABILITIES_SCHEMA,
        "canon.geo.capabilities.v0",
        CANON_GEO_CAPABILITIES_VERSION,
        &instance,
    );
}

#[test]
fn control_regional_inventory_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(control_inventory()).expect("inventory serializes");
    assert_drift_free(
        CONTROL_REGIONAL_INVENTORY_SCHEMA,
        "canon.geo.regional_inventory.v1",
        CANON_GEO_REGIONAL_INVENTORY_VERSION,
        &instance,
    );
}

#[test]
fn regional_inventory_advancement_schema_matches_a_real_instance() {
    let advancement = regional_inventory_advancement_contract_artifact();
    let canonical_bytes = canonical_geo_regional_inventory_advancement_bytes(&advancement)
        .expect("advancement canonical bytes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("advancement canonical JSON parses");
    assert_drift_free(
        REGIONAL_INVENTORY_ADVANCEMENT_SCHEMA,
        "canon.geo.regional_inventory_advancement.v0",
        CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION,
        &instance,
    );

    let schema = parsed(REGIONAL_INVENTORY_ADVANCEMENT_SCHEMA);
    assert!(
        required_contains(&schema, "/required", "advanced_inventory"),
        "advancement schema must embed the advanced inventory bytes it binds"
    );
    assert_eq!(
        schema
            .pointer("/properties/effect/const")
            .and_then(Value::as_str),
        Some("local_availability_only")
    );
    assert_eq!(
        schema
            .pointer("/properties/proof_class/const")
            .and_then(Value::as_str),
        Some("live")
    );
    assert_eq!(
        schema
            .pointer("/properties/receipt_terminal_state/const")
            .and_then(Value::as_str),
        Some("COMPLETE")
    );
}

#[test]
fn regional_inventory_advancement_schema_rejects_unknown_fields_and_version_shape() {
    let advancement = regional_inventory_advancement_contract_artifact();
    let canonical_bytes = canonical_geo_regional_inventory_advancement_bytes(&advancement)
        .expect("advancement canonical bytes");
    let mut instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("advancement canonical JSON parses");

    let mut top_unknown = instance.clone();
    top_unknown["path"] = serde_json::json!("/tmp/receipt.json");
    assert_schema_walk_rejects(
        REGIONAL_INVENTORY_ADVANCEMENT_SCHEMA,
        &top_unknown,
        "$.path: key not declared",
    );
    let top_unknown_error = serde_json::from_value::<GeoRegionalInventoryAdvancement>(top_unknown)
        .expect_err("serde must reject top-level unknown fields")
        .to_string();
    assert!(
        top_unknown_error.contains("unknown field"),
        "unexpected serde error: {top_unknown_error}"
    );

    let mut nested_unknown = instance.clone();
    nested_unknown["source_advancements"][0]["path"] =
        serde_json::json!("/tmp/local-warehouse-rows.json");
    assert_schema_walk_rejects(
        REGIONAL_INVENTORY_ADVANCEMENT_SCHEMA,
        &nested_unknown,
        "$.source_advancements[0].path: key not declared",
    );
    let nested_unknown_error =
        serde_json::from_value::<GeoRegionalInventoryAdvancement>(nested_unknown)
            .expect_err("serde must reject nested unknown fields")
            .to_string();
    assert!(
        nested_unknown_error.contains("unknown field"),
        "unexpected serde error: {nested_unknown_error}"
    );

    let schema = parsed(REGIONAL_INVENTORY_ADVANCEMENT_SCHEMA);
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION)
    );
    instance["version"] = serde_json::json!("canon_geo_regional_inventory_advancement.v1");
    let wrong_version: GeoRegionalInventoryAdvancement =
        serde_json::from_value(instance.clone()).expect("version is semantically validated");
    assert!(
        canonical_geo_regional_inventory_advancement_bytes(&wrong_version).is_err(),
        "wrong version must not canonicalize even though serde can parse the string"
    );

    assert_eq!(
        schema
            .pointer("/properties/advancement_id/pattern")
            .and_then(Value::as_str),
        Some("^canon_geo_regional_inventory_advancement\\.v0:[0-9a-f]{64}$")
    );
    assert!(regional_inventory_advancement_id_shape(
        advancement.advancement_id.as_str()
    ));
    assert!(!regional_inventory_advancement_id_shape(
        "canon_geo_regional_inventory_advancement.v0:ABC"
    ));
    assert_eq!(
        schema
            .pointer("/$defs/blake3_digest/pattern")
            .and_then(Value::as_str),
        Some("^blake3:[0-9a-f]{64}$")
    );
    assert!(prefixed_blake3_shape(advancement.semantic_hash.as_str()));
    assert!(!prefixed_blake3_shape("blake3:ABC"));
}

#[test]
fn regional_inventory_advancement_serializer_rejects_forged_execution_and_digest_links() {
    let mut forged_execution = regional_inventory_advancement_contract_artifact();
    forged_execution.receipt_execution.proof_class = GeoAcquisitionProofClass::Retained;
    forged_execution.receipt_execution.terminal_state = GeoAcquisitionTerminalState::ZeroRows;
    forged_execution.receipt_execution.retained_receipt_id = Some("retained.forged".to_string());
    forged_execution.receipt_execution.executor_request_id = None;
    forged_execution.receipt_execution.executor_query_id = None;
    let execution_error = canonical_geo_regional_inventory_advancement_bytes(&forged_execution)
        .expect_err("nested receipt execution must agree with top-level live COMPLETE proof");
    assert!(
        execution_error.message.contains("receipt execution"),
        "unexpected execution error: {execution_error:?}"
    );

    let mut missing_digest = regional_inventory_advancement_contract_artifact();
    missing_digest.source_advancements[0].result_digest_ids = vec!["result.missing".to_string()];
    let digest_error = canonical_geo_regional_inventory_advancement_bytes(&missing_digest)
        .expect_err("every source result digest id must resolve inside the advancement");
    assert!(
        digest_error.message.contains("resolve exactly once"),
        "unexpected digest error: {digest_error:?}"
    );

    let mut wrong_algorithm = regional_inventory_advancement_contract_artifact();
    wrong_algorithm.result_digests[0].algorithm = GeoDigestAlgorithm::Sha256;
    let algorithm_error = canonical_geo_regional_inventory_advancement_bytes(&wrong_algorithm)
        .expect_err("advancement result digests must preserve the BLAKE3 handoff contract");
    assert!(
        algorithm_error.message.contains("BLAKE3"),
        "unexpected algorithm error: {algorithm_error:?}"
    );
}

#[test]
fn control_resource_budget_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(control_budget()).expect("budget serializes");
    assert_drift_free(
        CONTROL_RESOURCE_BUDGET_SCHEMA,
        "canon.geo.resource_budget.v0",
        CANON_GEO_RESOURCE_BUDGET_VERSION,
        &instance,
    );
}

#[test]
fn discovery_request_schema_matches_a_real_instance() {
    let instance =
        serde_json::to_value(discovery_contract_request()).expect("discovery request serializes");
    assert_drift_free(
        DISCOVERY_REQUEST_SCHEMA,
        "canon.geo.discovery_request.v0",
        CANON_GEO_DISCOVERY_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn acquisition_request_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(acquisition_contract_request())
        .expect("acquisition request serializes");
    assert_drift_free(
        ACQUISITION_REQUEST_SCHEMA,
        "canon.geo.acquisition_request.v0",
        CANON_GEO_ACQUISITION_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn geometric_acquisition_request_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(geometric_acquisition_contract_request())
        .expect("geometric acquisition request serializes");
    assert_drift_free(
        ACQUISITION_REQUEST_SCHEMA,
        "canon.geo.acquisition_request.v0",
        CANON_GEO_ACQUISITION_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn acquisition_receipt_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(acquisition_contract_receipt())
        .expect("acquisition receipt serializes");
    assert_drift_free(
        ACQUISITION_RECEIPT_SCHEMA,
        "canon.geo.acquisition_receipt.v0",
        CANON_GEO_ACQUISITION_RECEIPT_VERSION,
        &instance,
    );
}

#[test]
fn geometric_acquisition_receipt_schema_matches_a_real_instance() {
    let instance = serde_json::to_value(geometric_acquisition_contract_receipt())
        .expect("geometric acquisition receipt serializes");
    assert_drift_free(
        ACQUISITION_RECEIPT_SCHEMA,
        "canon.geo.acquisition_receipt.v0",
        CANON_GEO_ACQUISITION_RECEIPT_VERSION,
        &instance,
    );
}

#[test]
fn error_population_schema_matches_a_real_instance() {
    let artifact = error_population_contract_artifact();
    let canonical_bytes =
        canonical_error_population_bytes(&artifact).expect("error population canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical error population parses");
    assert_drift_free(
        ERROR_POPULATION_SCHEMA,
        "canon.geo.error_population.v0",
        CANON_GEO_ERROR_POPULATION_VERSION,
        &instance,
    );

    let mut nested_unknown = instance.clone();
    nested_unknown["subjects"][0]["unregistered_window_field"] = serde_json::json!(true);
    assert_schema_walk_rejects(
        ERROR_POPULATION_SCHEMA,
        &nested_unknown,
        "$.subjects[0].unregistered_window_field: key not declared",
    );
}

#[test]
fn composition_request_schema_matches_a_real_instance() {
    let request = composition_request();
    let instance = serde_json::to_value(&request).expect("request must serialize");
    assert_drift_free(
        COMPOSITION_REQUEST_SCHEMA,
        "canon.geo.composition_request.v0",
        "canon_geo_composition_request.v0",
        &instance,
    );
}

#[test]
fn propagation_schema_matches_a_real_instance() {
    let artifact = propagate(
        &propagation_request(),
        None,
        &GeoPropagationBudget::default(),
    )
    .expect("propagation artifact builds");
    assert!(
        !artifact.prunings.is_empty(),
        "schema fixture must exercise nonempty typed pruning output"
    );
    let canonical_bytes =
        canonical_propagation_bytes(&artifact).expect("propagation artifact canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical propagation JSON parses");
    assert_drift_free(
        PROPAGATION_SCHEMA,
        "canon.geo.propagation.v0",
        CANON_GEO_PROPAGATION_VERSION,
        &instance,
    );
}

#[test]
fn explanation_schema_matches_a_real_instance() {
    let evidence = compile_evidence(&explanation_evidence_request())
        .expect("schema explanation evidence compiles");
    let budget = GeoExplanationBudget::default();
    let mut artifact = minimal_core(
        &evidence.composition_request,
        &evidence,
        &explanation_request_order(),
        &budget,
    )
    .expect("schema explanation artifact builds");
    correction_sets(
        &mut artifact,
        &evidence.composition_request,
        &evidence,
        &budget,
    )
    .expect("schema explanation correction sets build");
    let canonical_bytes =
        canonical_explanation_bytes(&artifact).expect("explanation artifact canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical explanation JSON parses");
    assert_drift_free(
        EXPLANATION_SCHEMA,
        "canon.geo.explanation.v0",
        CANON_GEO_EXPLANATION_VERSION,
        &instance,
    );
}

fn assessment_roll_owner_request() -> GeoAssessmentRollOwnerRequest {
    GeoAssessmentRollOwnerRequest {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION.to_string(),
        proof_class: GeoAssessmentRollOwnerProofClass::Fixture,
        population: GeoPopulationEvaluationRequest {
            version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
            cases: vec![GeoLabeledCompositionCase {
                id: "case-assessment-roll-owner".to_string(),
                evidence: GeoEvidenceCompilationRequest {
                    version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                    profile: GeoCompositionProfile::parcel(),
                    universe: GeoCompositionUniverse {
                        parcels: vec!["1000010001".to_string()],
                        buildings: Vec::new(),
                    },
                    contracts: Vec::new(),
                    observations: Vec::new(),
                    max_assignments: 16,
                    max_materialized_models: 16,
                },
                truth_plane: GeoTruthPlane::HumanAdjudication,
                truth: GeoCompositionModel {
                    parcels: vec!["1000010001".to_string()],
                    buildings: Vec::new(),
                },
            }],
            max_cases: 1,
        },
        case_documents: vec![GeoAssessmentRollCaseDocument {
            case_id: "case-assessment-roll-owner".to_string(),
            document_id: "doc-assessment-roll-owner".to_string(),
        }],
        contract_source: GeoAssessmentRollOwnerContractSource {
            source_dataset: "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES"
                .to_string(),
            source_release: "FY2026P3_acris-latest".to_string(),
            source_lineage_ids: vec![
                "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest".to_string(),
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3"
                    .to_string(),
            ],
        },
        calibration: GeoAssessmentRollOwnerCalibration {
            population_id: "h7-d1-residuals-2026-09-03-roll".to_string(),
            calibration_blake3: blake3::hash(b"assessment-roll-owner-schema-calibration")
                .to_hex()
                .to_string(),
            exact_falsification_rule_id: "truth-lot-owner-not-exact".to_string(),
            affiliate_falsification_rule_id: "truth-lot-owner-mismatch".to_string(),
        },
        roll_rows: vec![
            GeoAssessmentRollLotRow {
                bbl: "1000010001".to_string(),
                owner: "ACME BORROWER LLC".to_string(),
                gross_sqft: "1000".to_string(),
                units: "1".to_string(),
                condo_number: String::new(),
                source_record_id:
                    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:1000010001"
                        .to_string(),
                source_vintage: "FY2026P3".to_string(),
            },
            GeoAssessmentRollLotRow {
                bbl: "1000010002".to_string(),
                owner: "OTHER OWNER LLC".to_string(),
                gross_sqft: "1500".to_string(),
                units: "1".to_string(),
                condo_number: String::new(),
                source_record_id:
                    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:1000010002"
                        .to_string(),
                source_vintage: "FY2026P3".to_string(),
            },
        ],
        party_rows: vec![GeoAssessmentRollPartyRow {
            document_id: "doc-assessment-roll-owner".to_string(),
            party_type: "1".to_string(),
            party_name_norm: "ACME BORROWER LLC".to_string(),
            source_record_id:
                "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:doc-assessment-roll-owner:ACME_BORROWER_LLC"
                    .to_string(),
            source_vintage: "latest".to_string(),
        }],
        max_cases: 1,
        max_roll_rows: 2,
        max_party_rows: 1,
        max_overlay_observations: 4,
    }
}

#[test]
fn assessment_roll_owner_request_schema_matches_a_real_instance() {
    let request = assessment_roll_owner_request();
    let instance =
        serde_json::to_value(&request).expect("assessment-roll owner request serializes");
    assert_drift_free(
        ASSESSMENT_ROLL_OWNER_REQUEST_SCHEMA,
        "canon.geo.assessment_roll_owner_request.v0",
        CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn assessment_roll_owner_schema_matches_a_real_instance() {
    let artifact = produce_assessment_roll_owner_evidence(&assessment_roll_owner_request())
        .expect("assessment-roll owner artifact builds");
    let canonical_bytes = canonical_assessment_roll_owner_bytes(&artifact)
        .expect("assessment-roll owner artifact canonicalizes");
    let instance: Value = serde_json::from_slice(&canonical_bytes)
        .expect("canonical assessment-roll owner JSON parses");
    assert_drift_free(
        ASSESSMENT_ROLL_OWNER_SCHEMA,
        "canon.geo.assessment_roll_owner.v0",
        CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION,
        &instance,
    );
}

fn property_source_record(id: &str, seed: &str) -> GeoPropertySourceRecordRef {
    GeoPropertySourceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "fixture-2026-09-03".to_string(),
        record_blake3: format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex()),
    }
}

fn property_member(id: &str, tile_id: &str, seed: &str) -> GeoPropertyMemberAssertion {
    GeoPropertyMemberAssertion {
        member: GeoEntityRef::new(GeoEntityLevel::Parcel, id),
        tile_id: tile_id.to_string(),
        status: GeoPropertyMembershipStatus::AssertedMember,
        abstention_reason: None,
        source_record: property_source_record(&format!("fixture.property.member:{seed}"), seed),
    }
}

fn property_assertion_request() -> GeoPropertyAssertionRequest {
    GeoPropertyAssertionRequest {
        version: CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION.to_string(),
        proof_class: GeoPropertyAssertionProofClass::Fixture,
        blocking_strategy: GeoPropertyBlockingStrategy::DocumentFirstThenGeography,
        relation_graph_product: GeoPropertyRelationGraphProduct::PublishedDerivedProjection,
        source_corpus: GeoPropertySourceCorpus {
            corpus_id: "fixture.cmbs.annex_a".to_string(),
            corpus_version: "2026-09-03".to_string(),
            temporal_scope: "document_valid_time".to_string(),
            native_key_fields: vec![
                "accession".to_string(),
                "deal_id".to_string(),
                "loan_id".to_string(),
            ],
        },
        assertions: vec![GeoPropertyDocumentAssertionRequest {
            assertion_id: "assertion-property-schema-span".to_string(),
            document_id: "document-property-schema-span".to_string(),
            accession: "0000000000-26-000201".to_string(),
            deal_id: "schema-deal".to_string(),
            loan_id: "loan-201".to_string(),
            collateral_set_id: "collateral:0000000000-26-000201:loan-201".to_string(),
            source_record: property_source_record("fixture.property.document:schema-span", "doc"),
            members: vec![
                property_member("cmdrvl:parcel:01J7X0000000000000S1", "h3:r8:alpha", "s1"),
                property_member("cmdrvl:parcel:01J7X0000000000000S2", "h3:r8:alpha", "s2"),
                property_member("cmdrvl:parcel:01J7X0000000000000S3", "h3:r8:beta", "s3"),
            ],
        }],
        max_assertions: 4,
        max_members_per_assertion: 8,
        max_pairwise_comparisons: 6,
    }
}

#[test]
fn property_assertion_request_schema_matches_a_real_instance() {
    let request = property_assertion_request();
    let instance = serde_json::to_value(&request).expect("property assertion request serializes");
    assert_drift_free(
        PROPERTY_ASSERTION_REQUEST_SCHEMA,
        "canon.geo.property_assertion_request.v0",
        CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn property_assertion_schema_matches_a_real_instance() {
    let artifact = materialize_property_assertions(&property_assertion_request())
        .expect("property assertion artifact builds");
    let canonical_bytes =
        canonical_property_assertion_bytes(&artifact).expect("property artifact canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical property JSON parses");
    assert_drift_free(
        PROPERTY_ASSERTION_SCHEMA,
        "canon.geo.property_assertion.v0",
        CANON_GEO_PROPERTY_ASSERTION_VERSION,
        &instance,
    );
}

#[test]
fn footprint_roll_evidence_request_schema_matches_a_real_instance() {
    let request = GeoFootprintRollEvidenceRequest {
        version: CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        case_id: "case-footprint-roll".to_string(),
        universe: GeoCompositionUniverse {
            parcels: vec!["1000010001".to_string(), "1000010002".to_string()],
            buildings: Vec::new(),
        },
        loan: GeoFootprintRollLoanFields {
            loan_key: "loan-footprint-roll".to_string(),
            filed_size: Some(1_000),
            size_measure: "SQFT".to_string(),
            loan_county_property_count: Some(3),
            size_source_record_id:
                "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:loan-footprint-roll:size".to_string(),
            size_source_vintage: "latest_reporting_period".to_string(),
            county_property_count_source_record_id:
                "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:loan-footprint-roll:county_count"
                    .to_string(),
            county_property_count_source_vintage: "current".to_string(),
        },
        source_config: GeoFootprintRollSourceConfig::default(),
        calibration: GeoFootprintRollCalibration::default(),
        assessment_roll_rows: vec![
            GeoAssessmentRollGrossSqftRow {
                bbl: "1000010001".to_string(),
                gross_sqft: Some(400),
                units: Some(1),
            },
            GeoAssessmentRollGrossSqftRow {
                bbl: "1000010002".to_string(),
                gross_sqft: Some(700),
                units: Some(1),
            },
        ],
        footprint_rows: vec![
            GeoBuildingFootprintRow {
                mappluto_bbl: "1000010001".to_string(),
                bin: "1000010001-bin-1".to_string(),
                active: true,
            },
            GeoBuildingFootprintRow {
                mappluto_bbl: "1000010002".to_string(),
                bin: "1000010002-bin-1".to_string(),
                active: true,
            },
        ],
        max_assignments: 32,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let canonical_bytes = canonical_footprint_roll_evidence_request_bytes(&request)
        .expect("footprint/roll request canonicalizes");
    let instance: Value = serde_json::from_slice(&canonical_bytes)
        .expect("canonical footprint/roll request JSON parses");
    assert_drift_free(
        FOOTPRINT_ROLL_EVIDENCE_REQUEST_SCHEMA,
        "canon.geo.footprint_roll_evidence_request.v0",
        CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION,
        &instance,
    );
}

fn condo_bridge_request() -> GeoCondoBridgeRequest {
    GeoCondoBridgeRequest {
        version: CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION.to_string(),
        source_dataset: "fixture.schema.pad_bbl".to_string(),
        source_release: "26B_2026-05-01".to_string(),
        source_lineage_ids: vec!["EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT:26B".to_string()],
        pad_rows: vec![
            GeoPadBblRow {
                bbl_key: "1000011001".to_string(),
                low_bbl_key: "1000011001".to_string(),
                high_bbl_key: "1000011001".to_string(),
                billing_bbl_key: Some("1000017501".to_string()),
                condo_number: Some(7),
                condo_flag: Some("C".to_string()),
            },
            GeoPadBblRow {
                bbl_key: "1000012001".to_string(),
                low_bbl_key: "1000012001".to_string(),
                high_bbl_key: "1000012001".to_string(),
                billing_bbl_key: Some("1000017502".to_string()),
                condo_number: Some(8),
                condo_flag: Some("C".to_string()),
            },
            GeoPadBblRow {
                bbl_key: "1000012001".to_string(),
                low_bbl_key: "1000012001".to_string(),
                high_bbl_key: "1000012001".to_string(),
                billing_bbl_key: Some("1000017503".to_string()),
                condo_number: Some(8),
                condo_flag: Some("C".to_string()),
            },
        ],
        cases: vec![GeoCondoBridgeCaseRequest {
            case_id: "case-condo-bridge-schema".to_string(),
            loan_key: Some("schema-loan".to_string()),
            truth_parcels: vec!["1000011001".to_string(), "1000012001".to_string()],
            universe_parcels: vec!["1000017501".to_string(), "1000017502".to_string()],
        }],
        max_pad_rows: 8,
        max_cases: 1,
    }
}

#[test]
fn condo_bridge_schema_matches_a_real_instance() {
    let artifact = build_condo_bridge(&condo_bridge_request()).expect("condo bridge builds");
    let canonical_bytes =
        canonical_condo_bridge_bytes(&artifact).expect("condo bridge artifact canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical condo bridge JSON parses");
    assert_drift_free(
        CONDO_BRIDGE_SCHEMA,
        "canon.geo.condo_bridge.v0",
        CANON_GEO_CONDO_BRIDGE_VERSION,
        &instance,
    );
}

#[test]
fn separation_request_schema_matches_a_real_instance() {
    let request = separation_request();
    let canonical_bytes =
        canonical_separation_request_bytes(&request).expect("separation request canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical separation request JSON parses");
    assert_drift_free(
        SEPARATION_REQUEST_SCHEMA,
        "canon.geo.separation_request.v0",
        CANON_GEO_SEPARATION_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn separation_schema_matches_a_real_instance() {
    let artifact = separate(&separation_request(), &GeoExplanationBudget::default())
        .expect("schema separation artifact builds");
    let canonical_bytes =
        canonical_separation_bytes(&artifact).expect("separation artifact canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical separation JSON parses");
    assert_drift_free(
        SEPARATION_SCHEMA,
        "canon.geo.separation.v0",
        CANON_GEO_SEPARATION_VERSION,
        &instance,
    );
}

#[test]
fn next_evidence_request_schema_matches_a_real_instance() {
    let (_, _, request) = next_evidence_fixture();
    let canonical_bytes = canonical_next_evidence_request_bytes(&request)
        .expect("next-evidence request canonicalizes");
    let instance: Value = serde_json::from_slice(&canonical_bytes)
        .expect("canonical next-evidence request JSON parses");
    assert_drift_free(
        NEXT_EVIDENCE_REQUEST_SCHEMA,
        "canon.geo.next_evidence_request.v0",
        CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn next_evidence_schema_matches_a_real_instance() {
    let (composition, separation, request) = next_evidence_fixture();
    let artifact = recommend(
        &composition,
        &separation,
        &request.candidates,
        request.policy.as_ref(),
        &request.budget,
        &request.budget_spent,
    )
    .expect("schema next-evidence artifact builds");
    let canonical_bytes =
        canonical_next_evidence_bytes(&artifact).expect("next-evidence artifact canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical next-evidence JSON parses");
    assert_drift_free(
        NEXT_EVIDENCE_SCHEMA,
        "canon.geo.next_evidence.v0",
        CANON_GEO_NEXT_EVIDENCE_VERSION,
        &instance,
    );
}

#[test]
fn geometry_request_schema_matches_a_real_instance() {
    let request = geometry_request();
    let instance = serde_json::to_value(&request).expect("geometry request must serialize");
    assert_drift_free(
        GEOMETRY_REQUEST_SCHEMA,
        "canon.geo.geometry_request.v0",
        CANON_GEO_GEOMETRY_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn client_tile_ingest_request_schema_matches_a_real_instance() {
    let (request, _source) = client_tile_ingest_schema_fixture();
    let instance =
        serde_json::to_value(&request).expect("client tile ingest request must serialize");
    assert_drift_free(
        CLIENT_TILE_INGEST_REQUEST_SCHEMA,
        "canon.geo.client_tile_ingest_request.v0",
        CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn geometry_tile_schema_matches_a_real_instance() {
    let artifact =
        materialize_geometry_tile(&geometry_request()).expect("geometry request materializes");
    let instance = serde_json::to_value(&artifact).expect("geometry tile must serialize");
    assert_drift_free(
        GEOMETRY_TILE_SCHEMA,
        "canon.geo.geometry_tile.v0",
        "canon_geo_geometry_tile.v0",
        &instance,
    );
}

#[test]
fn redacted_artifact_schema_matches_a_real_instance() {
    let source = serde_json::json!({
        "version": "canon_geo_geometry_tile.v0",
        "tile_id": "892a100d26bffff",
        "geometry": [123456789, 987654321],
        "decision": "accepted",
        "candidate_count": 2
    });
    let redacted = redact_geo_artifact(
        "canon_geo_geometry_tile.v0",
        &source,
        &[
            GeoArtifactFieldClassification {
                field_path: "$.tile_id".to_string(),
                license_class: GeoArtifactFieldLicenseClass::Identifier,
                source_instance_id: None,
                reconstructive: false,
                rationale: "tile identifier".to_string(),
            },
            GeoArtifactFieldClassification {
                field_path: "$.geometry".to_string(),
                license_class: GeoArtifactFieldLicenseClass::LicensedGeometry,
                source_instance_id: Some("source.client.parcels".to_string()),
                reconstructive: true,
                rationale: "client parcel geometry is licensed".to_string(),
            },
            GeoArtifactFieldClassification {
                field_path: "$.decision".to_string(),
                license_class: GeoArtifactFieldLicenseClass::Public,
                source_instance_id: None,
                reconstructive: false,
                rationale: "decision state".to_string(),
            },
            GeoArtifactFieldClassification {
                field_path: "$.candidate_count".to_string(),
                license_class: GeoArtifactFieldLicenseClass::DerivedMeasure,
                source_instance_id: None,
                reconstructive: false,
                rationale: "candidate denominator".to_string(),
            },
        ],
    )
    .expect("redacted fixture builds");
    validate_redacted_artifact(&redacted).expect("redacted fixture validates");
    let canonical =
        canonical_redacted_artifact_bytes(&redacted).expect("redacted fixture canonical bytes");
    let instance: Value =
        serde_json::from_slice(&canonical).expect("redacted fixture canonical JSON parses");
    assert_drift_free(
        REDACTED_ARTIFACT_SCHEMA,
        "canon.geo.redacted_artifact.v0",
        CANON_GEO_REDACTED_ARTIFACT_VERSION,
        &instance,
    );

    let schema = parsed(REDACTED_ARTIFACT_SCHEMA);
    let license_classes = schema
        .pointer("/$defs/field_license_class/enum")
        .and_then(Value::as_array)
        .expect("field license class enum");
    for expected in [
        "licensed-geometry",
        "derived-measure",
        "identifier",
        "public",
    ] {
        assert!(
            license_classes
                .iter()
                .any(|actual| actual.as_str() == Some(expected)),
            "missing field license class {expected}"
        );
    }
}

#[test]
fn client_ingested_geometry_tile_schema_matches_a_real_instance() {
    let (request, source) = client_tile_ingest_schema_fixture();
    let artifact = ingest_client_geometry_tile(&request, source.as_bytes())
        .expect("client ingest materializes a geometry tile");
    let instance = serde_json::to_value(&artifact).expect("client geometry tile serializes");
    assert_drift_free(
        GEOMETRY_TILE_SCHEMA,
        "canon.geo.geometry_tile.v0",
        "canon_geo_geometry_tile.v0",
        &instance,
    );
}

#[test]
fn warehouse_geometry_rows_schema_matches_a_real_instance() {
    let request = warehouse_geometry_request();
    let instance = serde_json::to_value(&request).expect("warehouse geometry rows serialize");
    assert_drift_free(
        WAREHOUSE_GEOMETRY_ROWS_SCHEMA,
        "canon.geo.warehouse_geometry_rows.v0",
        CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
        &instance,
    );
}

#[test]
fn warehouse_geometry_schema_matches_a_real_instance() {
    let artifact = materialize_warehouse_geometry(&warehouse_geometry_request())
        .expect("warehouse geometry materializes");
    let instance = serde_json::to_value(&artifact).expect("warehouse geometry serializes");
    assert_drift_free(
        WAREHOUSE_GEOMETRY_SCHEMA,
        "canon.geo.warehouse_geometry.v0",
        "canon_geo_warehouse_geometry.v0",
        &instance,
    );
}

#[test]
fn home_cell_rows_schema_matches_a_real_instance() {
    let request = home_cell_rows_request();
    let instance = serde_json::to_value(&request).expect("home-cell rows serialize");
    assert_drift_free(
        HOME_CELL_ROWS_SCHEMA,
        "canon.geo.home_cell_rows.v1",
        CANON_GEO_HOME_CELL_ROWS_VERSION,
        &instance,
    );
}

#[test]
fn home_cell_assignment_schema_matches_a_real_instance() {
    let artifact = materialize_home_cells(&home_cell_rows_request())
        .expect("home-cell assignment materializes");
    let instance = serde_json::to_value(&artifact).expect("home-cell assignment serializes");
    assert_drift_free(
        HOME_CELL_ASSIGNMENT_SCHEMA,
        "canon.geo.home_cell_assignment.v1",
        "canon_geo_home_cell_assignment.v1",
        &instance,
    );
}

#[test]
fn tile_work_request_schema_matches_a_real_instance() {
    let request = tile_work_request();
    let instance = serde_json::to_value(&request).expect("tile-work request must serialize");
    assert_drift_free(
        TILE_WORK_REQUEST_SCHEMA,
        "canon.geo.tile_work_request.v1",
        CANON_GEO_TILE_WORK_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn tile_work_unit_schema_matches_a_real_instance() {
    let artifact =
        materialize_tile_work_unit(&tile_work_request()).expect("tile work unit materializes");
    let instance = serde_json::to_value(&artifact).expect("tile work unit must serialize");
    assert_drift_free(
        TILE_WORK_UNIT_SCHEMA,
        "canon.geo.tile_work_unit.v1",
        "canon_geo_tile_work_unit.v1",
        &instance,
    );
}

#[test]
fn tile_reconciliation_request_schema_matches_a_real_instance() {
    let request = tile_reconciliation_request();
    let instance = serde_json::to_value(&request).expect("tile reconciliation request serializes");
    assert_drift_free(
        TILE_RECONCILIATION_REQUEST_SCHEMA,
        "canon.geo.tile_reconciliation_request.v1",
        CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn tile_reconciliation_schema_matches_a_real_instance() {
    let artifact = reconcile_tile_decisions(&tile_reconciliation_request())
        .expect("tile reconciliation succeeds");
    let instance = serde_json::to_value(&artifact).expect("tile reconciliation must serialize");
    assert_drift_free(
        TILE_RECONCILIATION_SCHEMA,
        "canon.geo.tile_reconciliation.v1",
        "canon_geo_tile_reconciliation.v1",
        &instance,
    );
}

#[test]
fn v1_tile_relation_semantics_schema_uses_non_equivalence_vocabulary() {
    for (label, schema_source) in [
        (
            "tile reconciliation request",
            TILE_RECONCILIATION_REQUEST_SCHEMA,
        ),
        ("tile reconciliation artifact", TILE_RECONCILIATION_SCHEMA),
    ] {
        let schema = parsed(schema_source);
        assert_eq!(
            schema
                .pointer("/$defs/decision_semantics/oneOf/2/properties/relation/$ref")
                .and_then(Value::as_str),
            Some("#/$defs/non_equivalence_relation"),
            "{label} relation semantics must not reference the all-relations enum"
        );
        let values = schema
            .pointer("/$defs/non_equivalence_relation/enum")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{label} must declare non_equivalence_relation enum"));
        assert!(
            !values.iter().any(|value| value.as_str() == Some("same_as")),
            "{label} must forbid same_as in relation semantics at schema level"
        );
        for required in [
            "contains",
            "part_of",
            "within",
            "on",
            "fronts",
            "intersects",
        ] {
            assert!(
                values.iter().any(|value| value.as_str() == Some(required)),
                "{label} missing relation {required}"
            );
        }
    }

    let schema = parsed(TILE_RECONCILIATION_SCHEMA);
    for pointer in [
        "/$defs/relationship_edge/properties/relation/$ref",
        "/$defs/relationship_group/properties/relation/$ref",
    ] {
        assert_eq!(
            schema.pointer(pointer).and_then(Value::as_str),
            Some("#/$defs/non_equivalence_relation"),
            "typed relationship output must use the non-equivalence vocabulary"
        );
    }
    assert!(
        required_contains(&schema, "/required", "relationships")
            && required_contains(&schema, "/required", "relationship_groups"),
        "typed relationship output must be a first-class artifact field"
    );
}

#[test]
fn v1_tile_schemas_pin_exact_rust_integer_envelopes() {
    for schema_source in [
        HOME_CELL_ROWS_SCHEMA,
        HOME_CELL_ASSIGNMENT_SCHEMA,
        TILE_WORK_REQUEST_SCHEMA,
        TILE_WORK_UNIT_SCHEMA,
        TILE_RECONCILIATION_REQUEST_SCHEMA,
        TILE_RECONCILIATION_SCHEMA,
    ] {
        let schema = parsed(schema_source);
        assert_eq!(
            schema
                .pointer("/$defs/int64/minimum")
                .and_then(Value::as_i64),
            Some(i64::MIN)
        );
        assert_eq!(
            schema
                .pointer("/$defs/int64/maximum")
                .and_then(Value::as_i64),
            Some(i64::MAX)
        );
        assert_eq!(
            schema
                .pointer("/$defs/uint64/minimum")
                .and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            schema
                .pointer("/$defs/uint64/maximum")
                .and_then(Value::as_u64),
            Some(u64::MAX)
        );
    }

    let required_refs = [
        (
            HOME_CELL_ROWS_SCHEMA,
            "/properties/stability_radius_fixed/$ref",
            "#/$defs/uint64",
        ),
        (
            HOME_CELL_ASSIGNMENT_SCHEMA,
            "/$defs/fixed_point/properties/longitude/$ref",
            "#/$defs/int64",
        ),
        (
            HOME_CELL_ASSIGNMENT_SCHEMA,
            "/$defs/summary/properties/total/$ref",
            "#/$defs/uint64",
        ),
        (
            TILE_WORK_REQUEST_SCHEMA,
            "/properties/max_features/$ref",
            "#/$defs/uint64",
        ),
        (
            TILE_WORK_UNIT_SCHEMA,
            "/properties/center_feature_count/$ref",
            "#/$defs/uint64",
        ),
        (
            TILE_WORK_UNIT_SCHEMA,
            "/properties/candidate_reach/$ref",
            "#/$defs/candidate_reach_report",
        ),
        (
            TILE_RECONCILIATION_REQUEST_SCHEMA,
            "/$defs/work_unit/properties/candidate_reach/$ref",
            "#/$defs/candidate_reach_report",
        ),
        (
            TILE_RECONCILIATION_REQUEST_SCHEMA,
            "/$defs/work_unit/properties/halo_feature_count/$ref",
            "#/$defs/uint64",
        ),
        (
            TILE_RECONCILIATION_SCHEMA,
            "/$defs/reconciled_decision/properties/proposal_copies/$ref",
            "#/$defs/uint64",
        ),
    ];
    for (schema_source, pointer, expected) in required_refs {
        let schema = parsed(schema_source);
        assert_eq!(
            schema.pointer(pointer).and_then(Value::as_str),
            Some(expected)
        );
    }
}

#[test]
fn v1_tile_schemas_reject_numbers_outside_rust_integer_bounds() {
    let over_u64 = || json_integer("18446744073709551616");
    let under_i64 = || json_integer("-9223372036854775809");
    let over_i64 = || json_integer("9223372036854775808");

    assert_numeric_schema_rejects(
        HOME_CELL_ROWS_SCHEMA,
        "/properties/stability_radius_fixed",
        over_u64(),
        "expected type integer",
    );
    assert_numeric_schema_rejects(
        HOME_CELL_ASSIGNMENT_SCHEMA,
        "/$defs/fixed_point/properties/longitude",
        under_i64(),
        "expected type integer",
    );
    assert_numeric_schema_rejects(
        HOME_CELL_ASSIGNMENT_SCHEMA,
        "/$defs/fixed_point/properties/latitude",
        over_i64(),
        "value greater than maximum 9223372036854775807",
    );
    assert_numeric_schema_rejects(
        HOME_CELL_ASSIGNMENT_SCHEMA,
        "/$defs/summary/properties/total",
        over_u64(),
        "expected type integer",
    );
    assert_numeric_schema_rejects(
        TILE_WORK_REQUEST_SCHEMA,
        "/properties/max_features",
        over_u64(),
        "expected type integer",
    );
    assert_numeric_schema_rejects(
        TILE_WORK_UNIT_SCHEMA,
        "/properties/center_feature_count",
        over_u64(),
        "expected type integer",
    );
    assert_numeric_schema_rejects(
        TILE_RECONCILIATION_REQUEST_SCHEMA,
        "/$defs/work_unit/properties/halo_feature_count",
        over_u64(),
        "expected type integer",
    );
    assert_numeric_schema_rejects(
        TILE_RECONCILIATION_SCHEMA,
        "/$defs/batch_receipt/properties/proposal_count",
        over_u64(),
        "expected type integer",
    );

    let mut home_rows = serde_json::to_value(home_cell_rows_request()).unwrap();
    home_rows["stability_radius_fixed"] = over_u64();
    assert!(
        serde_json::from_value::<GeoHomeCellRowsRequest>(home_rows).is_err(),
        "Rust u64 home-cell limits must reject values above u64::MAX"
    );

    let mut assignment = serde_json::to_value(
        materialize_home_cells(&home_cell_rows_request()).expect("assignment fixture"),
    )
    .unwrap();
    assignment["features"][0]["representative_point_fixed"]["longitude"] = under_i64();
    assert!(
        serde_json::from_value::<GeoHomeCellAssignmentArtifact>(assignment).is_err(),
        "Rust i64 fixed coordinates must reject values below i64::MIN"
    );

    let mut work_unit = serde_json::to_value(
        materialize_tile_work_unit(&tile_work_request()).expect("work-unit fixture"),
    )
    .unwrap();
    work_unit["center_feature_count"] = over_u64();
    assert!(
        serde_json::from_value::<GeoTileWorkUnitArtifact>(work_unit).is_err(),
        "Rust u64 work-unit counts must reject values above u64::MAX"
    );

    let mut reconciliation = serde_json::to_value(
        reconcile_tile_decisions(&tile_reconciliation_request()).expect("reconciliation fixture"),
    )
    .unwrap();
    reconciliation["batch_receipts"][0]["proposal_count"] = over_u64();
    assert!(
        serde_json::from_value::<GeoTileReconciliationArtifact>(reconciliation).is_err(),
        "Rust u64 reconciliation counts must reject values above u64::MAX"
    );
}

#[test]
fn v1_tile_schemas_reject_negative_or_fractional_unsigned_fields() {
    for (schema_source, pointer) in [
        (
            HOME_CELL_ASSIGNMENT_SCHEMA,
            "/$defs/summary/properties/boundary_sensitive",
        ),
        (TILE_WORK_UNIT_SCHEMA, "/properties/halo_feature_count"),
        (
            TILE_RECONCILIATION_REQUEST_SCHEMA,
            "/$defs/work_unit/properties/center_feature_count",
        ),
        (
            TILE_RECONCILIATION_SCHEMA,
            "/properties/discarded_halo_proposals",
        ),
        (
            TILE_RECONCILIATION_SCHEMA,
            "/$defs/reconciled_decision/properties/proposal_copies",
        ),
    ] {
        assert_numeric_schema_rejects(
            schema_source,
            pointer,
            serde_json::json!(-1),
            "value below minimum 0",
        );
        assert_numeric_schema_rejects(
            schema_source,
            pointer,
            serde_json::json!(1.5),
            "expected type integer",
        );
    }

    assert_numeric_schema_rejects(
        HOME_CELL_ASSIGNMENT_SCHEMA,
        "/$defs/fixed_point/properties/longitude",
        serde_json::json!(1.5),
        "expected type integer",
    );

    let mut work_request = serde_json::to_value(tile_work_request()).unwrap();
    work_request["max_features"] = serde_json::json!(-1);
    assert!(
        serde_json::from_value::<GeoTileWorkRequest>(work_request).is_err(),
        "Rust u64 work limits must reject negative JSON integers"
    );

    let mut reconciliation_request = serde_json::to_value(tile_reconciliation_request()).unwrap();
    reconciliation_request["max_proposals"] = serde_json::json!(1.5);
    assert!(
        serde_json::from_value::<GeoTileReconciliationRequest>(reconciliation_request).is_err(),
        "Rust u64 reconciliation limits must reject fractional JSON numbers"
    );
}

#[test]
fn v1_tile_schemas_disclose_runtime_only_identity_invariants() {
    let schemas = [
        HOME_CELL_ROWS_SCHEMA,
        HOME_CELL_ASSIGNMENT_SCHEMA,
        TILE_WORK_REQUEST_SCHEMA,
        TILE_WORK_UNIT_SCHEMA,
        TILE_RECONCILIATION_REQUEST_SCHEMA,
        TILE_RECONCILIATION_SCHEMA,
    ];
    for schema in schemas {
        let schema: Value = serde_json::from_str(schema).expect("v1 tile schema parses");
        let invariants = schema["x-canon-contract"]["runtime_only_invariants"]
            .as_array()
            .expect("v1 tile schema declares runtime-only invariants");
        assert!(
            invariants.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|text| text.contains("UTF-8 bytes"))
            }),
            "schema must disclose the Rust byte cap that JSON maxLength cannot express"
        );
        assert!(
            invariants.iter().any(|value| {
                value.as_str().is_some_and(|text| {
                    text.contains("canonical sorted order") || text.contains("canonicalized")
                })
            }),
            "schema must disclose runtime canonical ordering"
        );
    }

    let reconciliation: Value = serde_json::from_str(TILE_RECONCILIATION_REQUEST_SCHEMA)
        .expect("reconciliation schema parses");
    let joined = reconciliation["x-canon-contract"]["runtime_only_invariants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for required in [
        "exact work-unit membership",
        "candidate_entity_level equals",
        "ObservationOnly",
        "Composition semantics",
        "StableIdentity semantics",
        "canonical bytes recompute",
        "work_unit_blake3 equals",
        "not proof that an external solver consumed",
        "jointly determine confluence scope",
        "one source_instance_id",
    ] {
        assert!(
            joined.contains(required),
            "missing runtime invariant: {required}"
        );
    }

    let home_rows: Value =
        serde_json::from_str(HOME_CELL_ROWS_SCHEMA).expect("home-cell rows schema parses");
    let home_joined = home_rows["x-canon-contract"]["runtime_only_invariants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(home_joined.contains("both present or both absent"));
}

#[test]
fn multisource_request_schema_matches_a_real_instance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let request = multisource_request(temp.path());
    let instance = serde_json::to_value(&request).expect("multisource request must serialize");
    assert_drift_free(
        MULTISOURCE_REQUEST_SCHEMA,
        "canon.geo.multisource_request.v0",
        CANON_GEO_MULTISOURCE_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn multisource_artifact_schema_matches_a_real_instance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let request = multisource_request(temp.path());
    let artifact = materialize_geo_multisource(&request, &temp.path().join("rows.csv"))
        .expect("multisource request materializes");
    let instance = serde_json::to_value(&artifact).expect("multisource artifact serializes");
    assert_drift_free(
        MULTISOURCE_ARTIFACT_SCHEMA,
        "canon.entity.multisource_link.v1",
        "canon_entity_multisource_link.v1",
        &instance,
    );
}

#[test]
fn h7_population_schemas_are_registered_with_version_constants() {
    assert_schema_shape(
        &parsed(H7_POPULATION_ROWS_SCHEMA),
        "canon.geo.h7_population_rows.v0",
        CANON_GEO_H7_POPULATION_ROWS_VERSION,
    );
    assert_schema_shape(
        &parsed(H7_POPULATION_SCHEMA),
        "canon.geo.h7_population.v0",
        CANON_GEO_H7_POPULATION_VERSION,
    );
    assert_schema_shape(
        &parsed(H7_STAGING_SOURCE_RECORD_BYTES_BATCH_SCHEMA),
        "canon.geo.h7_staging_source_record_bytes_batch.v0",
        CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
    );
    assert_schema_shape(
        &parsed(H7_PIP_BLOCK_POPULATION_BATCH_SCHEMA),
        "canon.geo.h7_pip_block_population_batch.v0",
        CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION,
    );
}

#[test]
fn h7_population_rows_schema_matches_a_real_instance() {
    let request = h7_population_rows_request();
    let instance = serde_json::to_value(&request).expect("H7 rows request must serialize");
    assert_drift_free(
        H7_POPULATION_ROWS_SCHEMA,
        "canon.geo.h7_population_rows.v0",
        CANON_GEO_H7_POPULATION_ROWS_VERSION,
        &instance,
    );
}

#[test]
fn h7_population_artifact_schema_matches_a_real_instance() {
    let artifact = materialize_h7_population_rows(&h7_population_rows_request())
        .expect("H7 fixture subset must materialize");
    let instance = serde_json::to_value(&artifact).expect("H7 artifact must serialize");
    assert_drift_free(
        H7_POPULATION_SCHEMA,
        "canon.geo.h7_population.v0",
        CANON_GEO_H7_POPULATION_VERSION,
        &instance,
    );
}

#[test]
fn h7_staging_source_record_bytes_batch_schema_matches_a_real_instance() {
    let request = h7_staging_source_record_bytes_batch_request();
    let instance = serde_json::to_value(&request).expect("H7 staging batch must serialize");
    assert_drift_free(
        H7_STAGING_SOURCE_RECORD_BYTES_BATCH_SCHEMA,
        "canon.geo.h7_staging_source_record_bytes_batch.v0",
        CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
        &instance,
    );
}

#[test]
fn composition_artifact_schema_matches_a_real_instance() {
    let artifact = solve_composition(&composition_request()).expect("request must solve");
    let instance = serde_json::to_value(&artifact).expect("artifact must serialize");
    assert_drift_free(
        COMPOSITION_SCHEMA,
        "canon.geo.composition.v0",
        "canon_geo_composition.v0",
        &instance,
    );
}

#[test]
fn evidence_request_schema_matches_a_real_instance() {
    let request = evidence_request();
    let instance = serde_json::to_value(&request).expect("request must serialize");
    assert_drift_free(
        EVIDENCE_REQUEST_SCHEMA,
        "canon.geo.evidence_request.v0",
        "canon_geo_evidence_request.v0",
        &instance,
    );
}

#[test]
fn warehouse_rows_schema_matches_a_real_instance() {
    let rows = warehouse_rows_request();
    let instance = serde_json::to_value(&rows).expect("warehouse rows must serialize");
    assert_drift_free(
        WAREHOUSE_ROWS_SCHEMA,
        "canon.geo.warehouse_rows.v0",
        CANON_GEO_WAREHOUSE_ROWS_VERSION,
        &instance,
    );
}

#[test]
fn warehouse_rows_schema_requires_profile_ref() {
    let schema = parsed(WAREHOUSE_ROWS_SCHEMA);
    assert_eq!(
        schema
            .pointer("/properties/profile/$ref")
            .and_then(Value::as_str),
        Some("canon.geo.composition_request.v0.schema.json#/$defs/composition_profile")
    );
    assert!(required_contains(&schema, "/required", "profile"));

    let rows = GeoWarehouseRowsRequest {
        profile: GeoCompositionProfile::building(),
        ..warehouse_rows_request()
    };
    let instance = serde_json::to_value(&rows).expect("warehouse rows must serialize");
    assert_drift_free(
        WAREHOUSE_ROWS_SCHEMA,
        "canon.geo.warehouse_rows.v0",
        CANON_GEO_WAREHOUSE_ROWS_VERSION,
        &instance,
    );
}

#[test]
fn evidence_compilation_artifact_schema_matches_a_real_instance() {
    let artifact = compile_evidence(&evidence_request()).expect("evidence must compile");
    let instance = serde_json::to_value(&artifact).expect("artifact must serialize");
    assert_drift_free(
        EVIDENCE_COMPILATION_SCHEMA,
        "canon.geo.evidence_compilation.v0",
        "canon_geo_evidence_compilation.v0",
        &instance,
    );
}

#[test]
fn emitted_composition_artifact_schemas_require_profile() {
    let composition_schema = parsed(COMPOSITION_SCHEMA);
    assert!(required_contains(
        &composition_schema,
        "/required",
        "profile"
    ));

    let evidence_compilation_schema = parsed(EVIDENCE_COMPILATION_SCHEMA);
    assert!(required_contains(
        &evidence_compilation_schema,
        "/$defs/composition_request/required",
        "profile"
    ));

    let composition_request_schema = parsed(COMPOSITION_REQUEST_SCHEMA);
    assert!(!required_contains(
        &composition_request_schema,
        "/required",
        "profile"
    ));

    let evidence_request_schema = parsed(EVIDENCE_REQUEST_SCHEMA);
    assert!(!required_contains(
        &evidence_request_schema,
        "/required",
        "profile"
    ));
}

#[test]
fn population_request_schema_matches_a_real_instance() {
    let request = GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "case-1".to_string(),
            evidence: evidence_request(),
            truth_plane: GeoTruthPlane::GateV2Historical,
            truth: GeoCompositionModel {
                parcels: vec!["parcel-a".to_string()],
                buildings: Vec::new(),
            },
        }],
        max_cases: 8,
    };
    let instance = serde_json::to_value(&request).expect("request must serialize");
    assert_drift_free(
        POPULATION_REQUEST_SCHEMA,
        "canon.geo.population_request.v0",
        "canon_geo_population_request.v0",
        &instance,
    );
}

#[test]
fn population_request_schema_matches_runtime_max_cases_boundary() {
    let schema = parsed(POPULATION_REQUEST_SCHEMA);
    assert_eq!(
        schema
            .pointer("/properties/max_cases/minimum")
            .and_then(Value::as_u64),
        Some(1),
        "schema must reject max_cases=0 because the evaluator rejects it"
    );
}

#[test]
fn point_population_schema_matches_a_real_instance() {
    let artifact: GeoPointPopulationArtifact =
        serde_json::from_str(include_str!("fixtures/geo/e1_gross_class_points.json"))
            .expect("point population fixture parses");
    validate_point_population_artifact(&artifact).expect("point population fixture validates");
    let instance = serde_json::to_value(&artifact).expect("point population serializes");
    assert_drift_free(
        POINT_POPULATION_SCHEMA,
        "canon.geo.point_population.v0",
        CANON_GEO_POINT_POPULATION_VERSION,
        &instance,
    );

    let schema = parsed(POINT_POPULATION_SCHEMA);
    assert_eq!(
        schema
            .pointer("/properties/source_dataset/pattern")
            .and_then(Value::as_str),
        Some("^fixture\\.")
    );
    assert_eq!(
        schema
            .pointer("/$defs/landed_geocode/properties/lon_e7/minimum")
            .and_then(Value::as_i64),
        Some(-743000000)
    );
    assert_eq!(
        schema
            .pointer("/$defs/landed_geocode/properties/lat_e7/maximum")
            .and_then(Value::as_i64),
        Some(410000000)
    );
}

#[test]
fn pre_resolution_schema_matches_a_real_instance() {
    let artifact = pre_resolution_artifact();
    validate_pre_resolution_artifact(&artifact).expect("pre-resolution artifact validates");
    let canonical =
        canonical_pre_resolution_bytes(&artifact).expect("pre-resolution canonical bytes");
    let instance: Value =
        serde_json::from_slice(&canonical).expect("pre-resolution canonical JSON parses");
    assert_drift_free(
        PRE_RESOLUTION_SCHEMA,
        "canon.geo.pre_resolution.v0",
        CANON_GEO_PRE_RESOLUTION_VERSION,
        &instance,
    );

    let schema = parsed(PRE_RESOLUTION_SCHEMA);
    assert_eq!(
        schema
            .pointer("/$defs/registry_proposal/properties/version/const")
            .and_then(Value::as_str),
        Some("canon_geo_registry_proposal.v0")
    );
    assert!(
        schema
            .pointer("/$defs/source_corpus/properties/corpus_kind/enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values
                .iter()
                .any(|value| value.as_str() == Some("ginnie_pool_no_address"))
                && values
                    .iter()
                    .any(|value| value.as_str() == Some("reit_schedule_iii_name_only")))
    );
}

#[test]
fn temporal_containment_schema_matches_a_real_instance() {
    let artifact = temporal_containment_artifact();
    validate_temporal_containment_artifact(&artifact)
        .expect("temporal containment fixture validates");
    let canonical = canonical_temporal_containment_bytes(&artifact)
        .expect("temporal containment canonical bytes");
    let instance: Value =
        serde_json::from_slice(&canonical).expect("temporal containment canonical JSON parses");
    assert_drift_free(
        TEMPORAL_CONTAINMENT_SCHEMA,
        "canon.geo.temporal_containment.v0",
        CANON_GEO_TEMPORAL_CONTAINMENT_VERSION,
        &instance,
    );

    let schema = parsed(TEMPORAL_CONTAINMENT_SCHEMA);
    assert_eq!(
        schema
            .pointer("/$defs/edge/properties/parent_level/const")
            .and_then(Value::as_str),
        Some("parcel")
    );
    assert_eq!(
        schema
            .pointer("/$defs/edge/properties/child_level/const")
            .and_then(Value::as_str),
        Some("building")
    );
}

fn temporal_containment_artifact() -> GeoTemporalContainmentArtifact {
    GeoTemporalContainmentArtifact {
        version: CANON_GEO_TEMPORAL_CONTAINMENT_VERSION.to_string(),
        mart_id: "fixture.nyc.lifecycle.schema".to_string(),
        clusters: vec![
            GeoTemporalContainmentCluster {
                cluster_id: "cmdrvl:building:nyc:bin:fixture-schema-001".to_string(),
                entity_level: GeoEntityLevel::Building,
            },
            GeoTemporalContainmentCluster {
                cluster_id: "cmdrvl:parcel:nyc:bbl:fixture-schema-001".to_string(),
                entity_level: GeoEntityLevel::Parcel,
            },
        ],
        edges: vec![GeoTemporalContainmentEdge {
            edge_id: "edge-fixture-schema-001".to_string(),
            parent_cluster_id: "cmdrvl:parcel:nyc:bbl:fixture-schema-001".to_string(),
            parent_level: GeoEntityLevel::Parcel,
            child_cluster_id: "cmdrvl:building:nyc:bin:fixture-schema-001".to_string(),
            child_level: GeoEntityLevel::Building,
            relation: GeoTemporalContainmentRelation::PartOf,
            valid_interval: GeoTemporalContainmentInterval {
                start_utc_day: "2020-01-01".to_string(),
                end_utc_day: "2020-12-31".to_string(),
            },
            source_receipt: GeoTemporalContainmentSourceReceipt {
                receipt_id: "receipt-fixture-schema-001".to_string(),
                source_dataset: "fixture.nyc.lifecycle".to_string(),
                source_record_id: "dob-job:fixture-schema-001".to_string(),
                source_record_blake3: pre_resolution_blake3("dob-job:fixture-schema-001"),
                proof_class: "fixture".to_string(),
                rule_id: "geo_temporal_containment_fixture.v1".to_string(),
            },
        }],
        summary: GeoTemporalContainmentSummary {
            clusters: 2,
            edges: 1,
        },
    }
}

fn pre_resolution_artifact() -> GeoPreResolutionArtifact {
    let request = GeoPreResolutionRequest {
        version: CANON_GEO_PRE_RESOLUTION_VERSION.to_string(),
        source_corpus: GeoPreResolutionSourceCorpus {
            corpus_id: "cmdrvl.cmbs.annex_a".to_string(),
            corpus_kind: GeoPreResolutionCorpusKind::CmbsAnnexA,
            corpus_version: "fixture-2026-09-02".to_string(),
            temporal_scope: "as_of=2026-08".to_string(),
            native_key_fields: vec![
                "accession".to_string(),
                "loan_id".to_string(),
                "property_address".to_string(),
            ],
        },
        proof_class: GeoPreResolutionProofClass::Fixture,
        build_receipts: vec![GeoPreResolutionBuildReceipt {
            receipt_id: "receipt-001".to_string(),
            query_id: "fixture-query:cmbs-annex-a-pre-resolution:2026-09-02".to_string(),
            source_artifact_blake3: pre_resolution_blake3("cmbs-annex-a-source-artifact"),
            row_count: 2,
            run_status: GeoPreResolutionRunStatus::Completed,
        }],
        rows: vec![
            GeoPreResolutionSourceRow {
                row_id: "annexa-row-001".to_string(),
                source_record_id: "cmbs-annexa:0000000000-26-000001:loan-a".to_string(),
                accession: "0000000000-26-000001".to_string(),
                deal_id: "fixture-deal-a".to_string(),
                loan_id: "loan-a".to_string(),
                source_record_blake3: pre_resolution_blake3("annexa-row-001"),
                asserted_address: Some("305 EAST 72 STREET".to_string()),
                reach: Some("full".to_string()),
                reach_none_reason: None,
                parcel_set: vec!["parcel:nyc:bbl:1004540041".to_string()],
                building_set: vec!["building:nyc:bin:1006494".to_string()],
            },
            GeoPreResolutionSourceRow {
                row_id: "annexa-row-002".to_string(),
                source_record_id: "cmbs-annexa:0000000000-26-000001:loan-b".to_string(),
                accession: "0000000000-26-000001".to_string(),
                deal_id: "fixture-deal-a".to_string(),
                loan_id: "loan-b".to_string(),
                source_record_blake3: pre_resolution_blake3("annexa-row-002"),
                asserted_address: None,
                reach: Some("none".to_string()),
                reach_none_reason: Some("no_candidate_parcels".to_string()),
                parcel_set: Vec::new(),
                building_set: Vec::new(),
            },
        ],
    };
    materialize_pre_resolution(&request).expect("pre-resolution fixture materializes")
}

fn pre_resolution_blake3(input: &str) -> String {
    format!("blake3:{}", blake3::hash(input.as_bytes()).to_hex())
}

fn population_stack_fixture() -> (
    GeoPopulationEvaluationRequest,
    GeoPopulationEvidenceStackRequest,
) {
    let population = GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "case-1".to_string(),
            evidence: evidence_request(),
            truth_plane: GeoTruthPlane::HumanAdjudication,
            truth: GeoCompositionModel {
                parcels: vec!["parcel-a".to_string()],
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    };
    let overlay = GeoPopulationEvidenceStackRequest {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        case_overlays: vec![GeoPopulationCaseEvidenceOverlay {
            case_id: "case-1".to_string(),
            expected_base_evidence_blake3: None,
            contracts: vec![GeoRhoContract {
                id: "stack-contract".to_string(),
                version: "v1".to_string(),
                source_dataset: "fixture:stack".to_string(),
                source_release: "fixture-v1".to_string(),
                source_lineage_ids: vec!["fixture:stack:lineage".to_string()],
                method_id: "fixture:stack:rho".to_string(),
                method_version: "v1".to_string(),
                claim_role: GeoEvidenceClaimRole::AttributeObservation,
                basis: GeoRhoBasis::LogicalRelaxation {
                    invariant_id: "fixture:stack:invariant".to_string(),
                },
            }],
            observations: vec![GeoRhoObservation {
                id: "stack-observation".to_string(),
                contract_id: "stack-contract".to_string(),
                source_records: vec![GeoEvidenceRecordRef {
                    source_record_id: "stack-row".to_string(),
                    source_vintage: "fixture-v1".to_string(),
                    record_blake3: blake3::hash(b"stack-row").to_hex().to_string(),
                }],
                valid_time: None,
                observation: GeoRhoObservationKind::PreferMember {
                    member: GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                    cost_if_absent: 3,
                },
            }],
        }],
        max_overlay_cases: 1,
        max_overlay_observations: 1,
    };
    (population, overlay)
}

#[test]
fn population_evidence_stack_request_schema_matches_a_real_instance() {
    let (_, request) = population_stack_fixture();
    let instance = serde_json::to_value(request).expect("stack request serializes");
    assert_drift_free(
        POPULATION_EVIDENCE_STACK_REQUEST_SCHEMA,
        "canon.geo.population_evidence_stack_request.v0",
        CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn population_evidence_stack_artifact_schema_matches_a_real_instance() {
    let (population, request) = population_stack_fixture();
    let artifact = stack_population_evidence(&population, &request).expect("stack fixture");
    let instance = serde_json::to_value(artifact).expect("stack artifact serializes");
    assert_drift_free(
        POPULATION_EVIDENCE_STACK_SCHEMA,
        "canon.geo.population_evidence_stack.v0",
        CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION,
        &instance,
    );
}

#[test]
fn population_evaluation_artifact_schema_matches_a_real_instance() {
    let request = GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "case-1".to_string(),
            evidence: evidence_request(),
            truth_plane: GeoTruthPlane::GateV2Historical,
            truth: GeoCompositionModel {
                parcels: vec!["parcel-a".to_string()],
                buildings: Vec::new(),
            },
        }],
        max_cases: 8,
    };
    let artifact = evaluate_population(&request).expect("population must evaluate");
    let instance = serde_json::to_value(&artifact).expect("artifact must serialize");
    assert_drift_free(
        POPULATION_EVALUATION_SCHEMA,
        "canon.geo.population_evaluation.v0",
        "canon_geo_population_evaluation.v0",
        &instance,
    );
}

fn h7_population_rows_request() -> GeoH7PopulationRowsRequest {
    GeoH7PopulationRowsRequest {
        version: CANON_GEO_H7_POPULATION_ROWS_VERSION.to_string(),
        population_scope: GeoH7PopulationScope::FixtureSubset,
        provenance: GeoH7PopulationProvenance {
            result_mode: GeoH7ResultMode::Replay,
            as_of: "2026-08-30T00:00:00Z".to_string(),
            acris_release_dt: CANON_GEO_H7_ACRIS_RELEASE_DT.to_string(),
            bridge_build_id: CANON_GEO_H7_BRIDGE_BUILD_ID.to_string(),
            collateral_scope: CANON_GEO_H7_COLLATERAL_SCOPE.to_string(),
            mappluto_releases: vec![h7_mappluto_pin("26v2"), h7_mappluto_pin("26v1")],
            primary_candidate_release: h7_mappluto_pin(CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE),
            amount_cents_quantization: CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION.to_string(),
            round_amount_lattice_cents: CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS,
            lender_match_transform: CANON_GEO_H7_LENDER_MATCH_TRANSFORM.to_string(),
            filed_county_mapping: h7_filed_county_mapping(),
            source_hashes: Vec::new(),
            query_receipts: vec![h7_query_receipt(
                "fixture_raw_property_state_ny_control_653_2321",
                7,
            )],
            external_receipts: Vec::new(),
            empirical_discrepancies: Vec::new(),
            row_cap: 10,
            observed_rows: 0,
            observed_payload_blake3: None,
        },
        plane_denominators: vec![
            GeoH7PlaneDenominator {
                truth_plane: GeoTruthPlane::NonRoundAmountDateLegalBorough,
                eligible_loans: 653,
                candidate_loans: 262,
                legal_confirmed_candidate_loans: 221,
                accepted_loans: 172,
                ambiguous_loans: 49,
                candidate_no_legal_confirmation_loans: 41,
                no_candidate_loans: 391,
                selected_multi_parcel_loans: 1,
            },
            GeoH7PlaneDenominator {
                truth_plane: GeoTruthPlane::RoundExactLenderParty,
                eligible_loans: 2321,
                candidate_loans: 182,
                legal_confirmed_candidate_loans: 179,
                accepted_loans: 149,
                ambiguous_loans: 30,
                candidate_no_legal_confirmation_loans: 3,
                no_candidate_loans: 2139,
                selected_multi_parcel_loans: 1,
            },
        ],
        rows: vec![
            h7_row("schema-non-round", "schema-doc-non-round", "26v1", false),
            h7_row("schema-non-round", "schema-doc-non-round", "26v2", false),
            h7_row("schema-round", "schema-doc-round", "26v1", true),
            h7_row("schema-round", "schema-doc-round", "26v2", true),
        ],
        max_cases: 8,
        max_assignments: 64,
        max_materialized_models: 64,
    }
}

fn h7_staging_source_record_bytes_batch_request() -> GeoH7StagingSourceRecordBytesBatchRequest {
    let rows_request = h7_population_rows_request();
    let denominators = rows_request
        .plane_denominators
        .iter()
        .map(|denominator| (denominator.truth_plane, denominator))
        .collect::<std::collections::BTreeMap<_, _>>();
    let staging_rows = rows_request
        .rows
        .iter()
        .map(|row| {
            h7_staging_source_record_bytes_row(
                row,
                denominators
                    .get(&row.truth_plane)
                    .expect("denominator for row plane"),
            )
        })
        .collect();

    GeoH7StagingSourceRecordBytesBatchRequest {
        version: CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION.to_string(),
        population_scope: rows_request.population_scope,
        provenance: rows_request.provenance,
        plane_denominators: rows_request.plane_denominators,
        staging_rows,
        max_cases: rows_request.max_cases,
        max_assignments: rows_request.max_assignments,
        max_materialized_models: rows_request.max_materialized_models,
    }
}

fn h7_staging_source_record_bytes_row(
    row: &GeoH7PopulationWarehouseRow,
    denominator: &GeoH7PlaneDenominator,
) -> GeoH7StagingSourceRecordBytesRow {
    let source_records = row
        .source_records
        .iter()
        .map(h7_staging_source_record)
        .collect::<Vec<_>>();
    let payload_counts = h7_staging_payload_counts(&source_records);

    GeoH7StagingSourceRecordBytesRow {
        row_contract: "h7_staging_source_record_bytes_export_row.v0".to_string(),
        row_kind: "source_record_payload_release_row".to_string(),
        guard_status: "ok".to_string(),
        refusal_reason: None,
        pip_block_population_query_id: "schema-pip-block-query".to_string(),
        payload_contract: "h7_derived_source_record_payload.v0".to_string(),
        source_record_class: "derived_immutable_evidence_record".to_string(),
        accepted_truth_query_id: Some("schema-accepted-truth-query".to_string()),
        loan_key: Some(row.loan_key.clone()),
        document_id: Some(row.document_id.clone()),
        truth_plane: Some(row.truth_plane),
        association_plane: Some(row.association_plane),
        mappluto_release: Some(row.candidate_release.release.clone()),
        mappluto_release_dt: Some(row.candidate_release.release_dt.clone()),
        mappluto_variant: Some(row.candidate_release.variant.clone()),
        candidate_release: Some(row.candidate_release.clone()),
        property_state: Some(row.property_state.clone()),
        filed_county: Some(row.filed_county.clone()),
        filed_borough: Some(row.filed_borough),
        legal_borough: Some(row.legal_borough),
        accepted_borough_edges: Some(row.accepted_borough_edges.clone()),
        geocoded_county_fips: row.geocoded_county_fips.clone(),
        doc_type: Some(row.doc_type.clone()),
        originationdate: Some(row.originationdate.clone()),
        amount_cents: Some(row.amount_cents),
        is_round_100k_lattice: Some(row.is_round_100k_lattice),
        originatorname: row.originatorname.clone(),
        originator_match_text: row.originator_match_text.clone(),
        lender_match_text: row.lender_match_text.clone(),
        lender_party_type: row.lender_party_type.clone(),
        loan_field_distinct_counts: Some(row.loan_field_distinct_counts.clone()),
        truth_parcels: Some(row.truth_parcels.clone()),
        candidate_parcels: Some(row.candidate_parcels.clone()),
        reach_status: Some(row.reach_status),
        reach_reason: Some(row.reach_reason.clone()),
        source_records: Some(source_records.clone()),
        source_record_count: Some(source_records.len() as u64),
        bridge_source_record_count: Some(h7_staging_role_count(
            &source_records,
            GeoH7SourceRecordRole::BridgeLoan,
        )),
        acris_master_source_record_count: Some(h7_staging_role_count(
            &source_records,
            GeoH7SourceRecordRole::AcrisMaster,
        )),
        acris_party_source_record_count: Some(h7_staging_role_count(
            &source_records,
            GeoH7SourceRecordRole::AcrisParty,
        )),
        acris_legal_source_record_count: Some(h7_staging_role_count(
            &source_records,
            GeoH7SourceRecordRole::AcrisLegal,
        )),
        mappluto_source_record_count: Some(h7_staging_role_count(
            &source_records,
            GeoH7SourceRecordRole::MapplutoCandidate,
        )),
        min_source_record_payload_utf8_bytes: Some(payload_counts.0),
        max_source_record_payload_utf8_bytes: Some(payload_counts.1),
        total_source_record_payload_utf8_bytes: Some(payload_counts.2),
        max_source_record_payload_base64_chars: Some(payload_counts.3),
        candidate_bbl_count: Some(row.candidate_parcels.len() as u64),
        truth_bbl_count: Some(row.truth_parcels.len() as u64),
        reached_truth_bbls: Some(
            row.truth_parcels
                .iter()
                .filter(|parcel_id| row.candidate_parcels.contains(parcel_id))
                .count() as u64,
        ),
        whole_accepted_loans: Some(2),
        whole_release_rows: Some(4),
        whole_zero_candidate_release_rows: Some(0),
        accepted_plane_eligible_loans: Some(denominator.eligible_loans),
        accepted_plane_legal_candidate_loans: Some(denominator.candidate_loans),
        accepted_plane_legal_confirmed_candidate_loans: Some(
            denominator.legal_confirmed_candidate_loans,
        ),
        accepted_plane_accepted_loans: Some(denominator.accepted_loans),
        accepted_plane_ambiguous_loans: Some(denominator.ambiguous_loans),
        accepted_plane_candidate_without_legal_loans: Some(
            denominator.candidate_no_legal_confirmation_loans,
        ),
        accepted_plane_no_candidate_loans: Some(denominator.no_candidate_loans),
        accepted_plane_selected_multi_parcel_loans: Some(denominator.selected_multi_parcel_loans),
    }
}

fn h7_row(
    loan_key: &str,
    document_id: &str,
    release: &str,
    round: bool,
) -> GeoH7PopulationWarehouseRow {
    let candidate_release = h7_mappluto_pin(release);
    let mut source_records = vec![
        h7_source_record(
            GeoH7SourceRecordRole::BridgeLoan,
            &format!("{loan_key}:bridge-loan"),
            CANON_GEO_H7_BRIDGE_BUILD_ID,
            &[],
        ),
        h7_source_record(
            GeoH7SourceRecordRole::AcrisMaster,
            &format!("{document_id}:master"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &[],
        ),
        h7_source_record(
            GeoH7SourceRecordRole::AcrisLegal,
            &format!("{document_id}:legal-bbl-1"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &["1000000001"],
        ),
        h7_source_record(
            GeoH7SourceRecordRole::AcrisLegal,
            &format!("{document_id}:legal-bbl-2"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &["1000000002"],
        ),
        h7_source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:mappluto-candidate-1"),
            candidate_release.release_dt.as_str(),
            &["1000000001"],
        ),
    ];
    if release == "26v1" {
        source_records.push(h7_source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:mappluto-candidate-2"),
            candidate_release.release_dt.as_str(),
            &["1000000002"],
        ));
        source_records.push(h7_source_record(
            GeoH7SourceRecordRole::MapplutoCandidate,
            &format!("{loan_key}:{release}:mappluto-candidate-3"),
            candidate_release.release_dt.as_str(),
            &["1000000003"],
        ));
    }
    if round {
        source_records.push(h7_source_record(
            GeoH7SourceRecordRole::AcrisParty,
            &format!("{document_id}:party"),
            CANON_GEO_H7_ACRIS_RELEASE_DT,
            &[],
        ));
    }
    GeoH7PopulationWarehouseRow {
        loan_key: loan_key.to_string(),
        document_id: document_id.to_string(),
        truth_plane: if round {
            GeoTruthPlane::RoundExactLenderParty
        } else {
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        },
        association_plane: GeoH7AssociationPlane::MultiProperty,
        candidate_release,
        property_state: "NY".to_string(),
        filed_county: "KINGS".to_string(),
        filed_borough: 3,
        legal_borough: 3,
        accepted_borough_edges: vec![GeoH7BoroughEdge {
            filed_county: "KINGS".to_string(),
            filed_borough: 3,
            legal_borough: 3,
        }],
        geocoded_county_fips: Some("36047".to_string()),
        doc_type: if round { "MMTG" } else { "MTGE" }.to_string(),
        originationdate: "2025-01-15".to_string(),
        amount_cents: if round { 50_000_000 } else { 12_345_678 },
        is_round_100k_lattice: round,
        originatorname: round.then(|| "Acme Bank".to_string()),
        originator_match_text: round.then(|| "ACME BANK".to_string()),
        lender_match_text: round.then(|| "ACME BANK".to_string()),
        lender_party_type: round.then(|| "1".to_string()),
        loan_field_distinct_counts: canon::geo::GeoH7LoanFieldDistinctCounts {
            originatorname: if round { 1 } else { 0 },
            originator_match_text: if round { 1 } else { 0 },
            originationdate: 1,
            originalloanamount: 1,
            filed_borough: 1,
        },
        truth_parcels: vec!["1000000002".to_string(), "1000000001".to_string()],
        candidate_parcels: if release == "26v2" {
            vec!["1000000001".to_string()]
        } else {
            vec![
                "1000000003".to_string(),
                "1000000002".to_string(),
                "1000000001".to_string(),
            ]
        },
        reach_status: if release == "26v2" {
            GeoH7CandidateReachStatus::Partial
        } else {
            GeoH7CandidateReachStatus::Full
        },
        reach_reason: "schema_fixture_candidate_release_scored_against_truth".to_string(),
        source_records,
    }
}

fn h7_mappluto_pin(release: &str) -> GeoH7MapplutoReleasePin {
    let release_dt = match release {
        "26v1" => "2026-05-01",
        "26v2" => "2026-08-01",
        _ => "2026-12-01",
    };
    GeoH7MapplutoReleasePin {
        release: release.to_string(),
        release_dt: release_dt.to_string(),
        variant: "shoreline_clipped".to_string(),
        geometry_contract_version: CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION.to_string(),
    }
}

fn h7_filed_county_mapping() -> Vec<GeoH7FiledCountyMapping> {
    [
        ("NEW YORK", 1),
        ("MANHATTAN", 1),
        ("NY061", 1),
        ("BRONX", 2),
        ("KINGS", 3),
        ("BROOKLYN", 3),
        ("QUEENS", 4),
        ("RICHMOND", 5),
    ]
    .into_iter()
    .map(|(filed_county, acris_borough)| GeoH7FiledCountyMapping {
        filed_county: filed_county.to_string(),
        acris_borough,
    })
    .collect()
}

fn h7_source_record(
    role: GeoH7SourceRecordRole,
    source_record_id: &str,
    source_vintage: &str,
    parcel_ids: &[&str],
) -> GeoH7SourceEvidenceRecord {
    GeoH7SourceEvidenceRecord {
        role,
        parcel_ids: parcel_ids
            .iter()
            .map(|parcel_id| (*parcel_id).to_string())
            .collect(),
        source_record: GeoEvidenceRecordRef {
            source_record_id: source_record_id.to_string(),
            source_vintage: source_vintage.to_string(),
            record_blake3: blake3::hash(
                format!("schema-h7-source\0{source_record_id}\0{source_vintage}").as_bytes(),
            )
            .to_hex()
            .to_string(),
        },
    }
}

fn h7_staging_source_record(
    record: &GeoH7SourceEvidenceRecord,
) -> GeoH7StagingSourceEvidenceRecord {
    let source_record_bytes_base64 = h7_staging_source_record_payload_base64(record);
    let record_blake3 = blake3::hash(
        BASE64_STANDARD
            .decode(source_record_bytes_base64.as_bytes())
            .expect("canonical staging source payload")
            .as_slice(),
    )
    .to_hex()
    .to_string();
    GeoH7StagingSourceEvidenceRecord {
        role: record.role,
        parcel_ids: record.parcel_ids.clone(),
        source_record: GeoH7StagingEvidenceRecordRef {
            source_record_id: record.source_record.source_record_id.clone(),
            source_vintage: record.source_record.source_vintage.clone(),
            record_blake3,
        },
        source_record_bytes_base64,
    }
}

fn h7_staging_source_record_payload_base64(record: &GeoH7SourceEvidenceRecord) -> String {
    let mut pairs = vec![
        vec![
            "payload_contract".to_string(),
            "h7_derived_source_record_payload.v0".to_string(),
        ],
        vec![
            "source_record_class".to_string(),
            "derived_immutable_evidence_record".to_string(),
        ],
        vec![
            "role".to_string(),
            h7_source_record_role_name(record.role).to_string(),
        ],
        vec![
            "source_record_id".to_string(),
            record.source_record.source_record_id.clone(),
        ],
        vec![
            "source_vintage".to_string(),
            record.source_record.source_vintage.clone(),
        ],
    ];
    if let Some(parcel_id) = record.parcel_ids.first() {
        match record.role {
            GeoH7SourceRecordRole::AcrisLegal => {
                pairs.push(vec!["legal_bbl".to_string(), parcel_id.clone()]);
            }
            GeoH7SourceRecordRole::MapplutoCandidate => {
                pairs.push(vec!["bbl_key".to_string(), parcel_id.clone()]);
            }
            _ => {}
        }
    }
    pairs.sort_by(|left, right| left[0].cmp(&right[0]));
    BASE64_STANDARD.encode(serde_json::to_vec(&pairs).expect("staging source payload JSON"))
}

fn h7_staging_payload_counts(records: &[GeoH7StagingSourceEvidenceRecord]) -> (u64, u64, u64, u64) {
    let mut min_utf8_bytes: Option<u64> = None;
    let mut max_utf8_bytes = 0_u64;
    let mut total_utf8_bytes = 0_u64;
    let mut max_base64_chars = 0_u64;

    for record in records {
        let bytes = BASE64_STANDARD
            .decode(record.source_record_bytes_base64.as_bytes())
            .expect("canonical staging source payload");
        let utf8_bytes = bytes.len() as u64;
        min_utf8_bytes = Some(min_utf8_bytes.map_or(utf8_bytes, |current| current.min(utf8_bytes)));
        max_utf8_bytes = max_utf8_bytes.max(utf8_bytes);
        total_utf8_bytes += utf8_bytes;
        max_base64_chars = max_base64_chars.max(record.source_record_bytes_base64.len() as u64);
    }

    (
        min_utf8_bytes.unwrap_or(0),
        max_utf8_bytes,
        total_utf8_bytes,
        max_base64_chars,
    )
}

fn h7_staging_role_count(
    records: &[GeoH7StagingSourceEvidenceRecord],
    role: GeoH7SourceRecordRole,
) -> u64 {
    records.iter().filter(|record| record.role == role).count() as u64
}

fn h7_source_record_role_name(role: GeoH7SourceRecordRole) -> &'static str {
    match role {
        GeoH7SourceRecordRole::BridgeLoan => "bridge_loan",
        GeoH7SourceRecordRole::AcrisMaster => "acris_master",
        GeoH7SourceRecordRole::AcrisLegal => "acris_legal",
        GeoH7SourceRecordRole::AcrisParty => "acris_party",
        GeoH7SourceRecordRole::MapplutoCandidate => "mappluto_candidate",
        GeoH7SourceRecordRole::GeocodeDiagnostic => "geocode_diagnostic",
    }
}

fn h7_query_receipt(purpose: &str, result_rows: u64) -> GeoH7QueryReceipt {
    let query_text_ref = format!("fixture:h7-schema:{purpose}");
    let fixture_text = format!("{query_text_ref}:synthetic-diagnostic");
    GeoH7QueryReceipt {
        purpose: purpose.to_string(),
        truth_plane: None,
        query_id: None,
        query_text_ref,
        normalized_query_text: None,
        query_blake3: blake3::hash(fixture_text.as_bytes()).to_hex().to_string(),
        result_rows,
        row_cap: 100,
        disposition: GeoH7QueryDisposition::DiagnosticOnly,
    }
}
