#![forbid(unsafe_code)]

//! Source-generic Canon Geo control contracts.
//!
//! These contracts describe intent, build capabilities, regional evidence
//! availability, and deterministic resource budgets. They do not acquire data,
//! contact catalogs, schedule work, or loosen Canon's exact replay boundary.

use super::{
    address::{
        CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION, CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION,
        CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION,
        CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION, CANON_GEO_ADDRESS_PARSE_FOREST_VERSION,
        CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION, CANON_GEO_ADDRESS_QUERY_GRAMMAR_VERSION,
        CANON_GEO_PAD_ADDRESS_SET_VERSION, CANON_GEO_PAD_MEMBERSHIP_VERSION,
    },
    composition::{
        CANON_GEO_COMPOSITION_PROFILE_VERSION, CANON_GEO_COMPOSITION_REQUEST_VERSION,
        CANON_GEO_COMPOSITION_VERSION, CANON_GEO_ENTITY_PROJECTION_VERSION,
    },
    discovery::{
        CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
        CANON_GEO_DISCOVERY_REQUEST_VERSION,
    },
    evaluation::{CANON_GEO_POPULATION_EVALUATION_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION},
    evidence::{CANON_GEO_EVIDENCE_COMPILATION_VERSION, CANON_GEO_EVIDENCE_REQUEST_VERSION},
    geometry_value::{
        CANON_GEO_GEOMETRY_REQUEST_VERSION, CANON_GEO_GEOMETRY_TILE_VERSION,
        CANON_GEO_GEOMETRY_VALUE_VERSION, CANON_GEO_LOCAL_FRAME_VERSION,
        CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION, CANON_GEO_WAREHOUSE_GEOMETRY_VERSION,
    },
    materialize::{
        CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION, CANON_GEO_H7_POPULATION_ROWS_VERSION,
        CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
        CANON_GEO_WAREHOUSE_ROWS_VERSION,
    },
    multisource::CANON_GEO_MULTISOURCE_REQUEST_VERSION,
    plan::CANON_GEO_PLAN_VERSION,
    residual_benchmark::{CANON_GEO_RESIDUAL_BENCHMARK_VERSION, CANON_GEO_RESIDUAL_OBDD_VERSION},
    run::CANON_GEO_RUN_VERSION,
    satisfy::CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION,
    stack::{
        CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
        CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION,
    },
    tile::{
        CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
        CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION, CANON_GEO_TILE_RECONCILIATION_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION,
    },
};
use crate::entity::run::link::multisource::ENTITY_MULTISOURCE_LINK_VERSION;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_QUESTION_VERSION: &str = "canon_geo_question.v0";
pub const CANON_GEO_CAPABILITIES_VERSION: &str = "canon_geo_capabilities.v0";
pub const CANON_GEO_REGIONAL_INVENTORY_VERSION: &str = "canon_geo_regional_inventory.v1";
pub const CANON_GEO_RESOURCE_BUDGET_VERSION: &str = "canon_geo_resource_budget.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoControlEntityLevel {
    Site,
    Property,
    Parcel,
    Building,
    Unit,
    Address,
    Poi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoControlRelation {
    SameAs,
    Contains,
    PartOf,
    Within,
    On,
    Fronts,
    Intersects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceClass {
    GeocodePoint,
    AddressString,
    AddressSet,
    ParcelGeometry,
    BuildingFootprint,
    AssertedAttribute,
    EntityRelation,
    TemporalObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoClaimClass {
    CandidateReach,
    StableIdentity,
    CollateralComposition,
    TemporalOccupancy,
    LifecycleState,
    AttributeBand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoControlProperty {
    Deterministic,
    Confluent,
    Sound,
    Complete,
    Canonical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoValueOrigin {
    CallerDeclared,
    BinaryDefault,
    SourceRelease,
    LocalArtifact,
    AdapterContract,
    OperatorPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoResourceCounter {
    Bytes,
    Rows,
    Cells,
    Candidates,
    Variables,
    States,
    Models,
    Operations,
    ProofBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoBudgetAction {
    RefuseBeforeWork,
    RefuseBeforeOutput,
    TruncatePresentationOnly,
    ReportBudgetFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNumericBound {
    pub semantic_id: String,
    pub counter: GeoResourceCounter,
    pub value: u64,
    pub unit: String,
    pub origin: GeoValueOrigin,
    pub action: GeoBudgetAction,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNumericMeasure {
    pub semantic_id: String,
    pub value: u64,
    pub unit: String,
    pub origin: GeoValueOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAsOf {
    /// Whole UTC day in `YYYY-MM-DD` form.
    pub utc_day: String,
    pub semantic_id: String,
    pub unit: String,
    pub origin: GeoValueOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDateInterval {
    pub start_utc_day: String,
    pub end_utc_day: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoBoundedGeography {
    pub geography_id: String,
    pub geography_kind: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSubjectBindingClass {
    AddressText,
    SourceIdentifier,
    Coordinate,
    OperatorLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSubjectBinding {
    pub role: String,
    pub binding_class: GeoSubjectBindingClass,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRequestedGrain {
    pub entity_level: GeoControlEntityLevel,
    pub required_evidence_classes: Vec<GeoEvidenceClass>,
    pub optional_evidence_classes: Vec<GeoEvidenceClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAbstentionDisposition {
    ReportUnsupported,
    ReportResidual,
    Refuse,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAbstentionPolicy {
    pub unsupported_grain: GeoAbstentionDisposition,
    pub unresolved_residual: GeoAbstentionDisposition,
    pub budget_fallback: GeoAbstentionDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDecisionPolicyRef {
    pub policy_id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoQuestion {
    pub version: String,
    pub question_id: String,
    pub subject_bindings: Vec<GeoSubjectBinding>,
    pub bounded_geography: GeoBoundedGeography,
    pub requested_grains: Vec<GeoRequestedGrain>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_as_of: Option<GeoAsOf>,
    pub requested_claim_classes: Vec<GeoClaimClass>,
    pub presentation_limits: Vec<GeoNumericBound>,
    pub abstention_policy: GeoAbstentionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_policy: Option<GeoDecisionPolicyRef>,
    pub resource_budget_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSourceRelease {
    pub release_id: String,
    pub release_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTemporalScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_time: Option<GeoDateInterval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_time: Option<GeoDateInterval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_time: Option<GeoAsOf>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoIdentityParticipation {
    StableAlias,
    EvidenceOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoNativeEntityScope {
    NativeEntity {
        entity_level: GeoControlEntityLevel,
        identity_participation: GeoIdentityParticipation,
    },
    ObservationOnly,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum GeoNativeEntityScopeWire {
    NativeEntity {
        entity_level: GeoControlEntityLevel,
        identity_participation: GeoIdentityParticipation,
        #[serde(flatten)]
        unknown_fields: BTreeMap<String, serde_json::Value>,
    },
    ObservationOnly {
        #[serde(flatten)]
        unknown_fields: BTreeMap<String, serde_json::Value>,
    },
}

impl<'de> Deserialize<'de> for GeoNativeEntityScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = GeoNativeEntityScopeWire::deserialize(deserializer)?;
        let (scope, unknown_fields) = match wire {
            GeoNativeEntityScopeWire::NativeEntity {
                entity_level,
                identity_participation,
                unknown_fields,
            } => (
                Self::NativeEntity {
                    entity_level,
                    identity_participation,
                },
                unknown_fields,
            ),
            GeoNativeEntityScopeWire::ObservationOnly { unknown_fields } => {
                (Self::ObservationOnly, unknown_fields)
            }
        };
        if unknown_fields.is_empty() {
            Ok(scope)
        } else {
            Err(serde::de::Error::custom(format!(
                "unknown native_scope field(s): {}",
                unknown_fields
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }
}

impl GeoNativeEntityScope {
    pub const fn may_contribute_stable_alias(&self) -> bool {
        matches!(
            self,
            Self::NativeEntity {
                identity_participation: GeoIdentityParticipation::StableAlias,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCoveragePredicate {
    pub coverage_id: String,
    pub region: GeoBoundedGeography,
    pub predicate: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSourceAvailability {
    Available,
    Partial,
    Missing,
    DiscoveryRequired,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLocalArtifactRef {
    pub artifact_id: String,
    pub contract_version: String,
    pub content_hash: String,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLocalAcquisitionState {
    pub state: GeoSourceAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_ref: Option<GeoLocalArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoGeometryTransformContract {
    pub geometry_contract_version: String,
    pub coordinate_reference_system: String,
    pub transform_id: String,
    pub transform_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub numeric_error_bounds: Vec<GeoNumericMeasure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoLicenseClass {
    PublicRedistributable,
    PublicAttributionRequired,
    RestrictedLocalOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEgressClass {
    Shareable,
    DerivedOnly,
    LocalOnly,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRegionalSourceInstance {
    pub source_instance_id: String,
    pub release: GeoSourceRelease,
    pub temporal_scope: GeoTemporalScope,
    pub lineage_ids: Vec<String>,
    pub native_scope: GeoNativeEntityScope,
    pub evidence_classes: Vec<GeoEvidenceClass>,
    pub coverage: GeoCoveragePredicate,
    pub local_state: GeoLocalAcquisitionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<GeoGeometryTransformContract>,
    pub license_class: GeoLicenseClass,
    pub egress_class: GeoEgressClass,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub estimates: Vec<GeoNumericMeasure>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDiscoveryGap {
    pub gap_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_entity_level: Option<GeoControlEntityLevel>,
    pub requested_evidence_class: GeoEvidenceClass,
    pub reason: String,
    pub next_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRegionalInventory {
    pub version: String,
    pub inventory_id: String,
    pub region: GeoBoundedGeography,
    pub sources: Vec<GeoRegionalSourceInstance>,
    pub discovery_gaps: Vec<GeoDiscoveryGap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTelemetryMetric {
    WallTime,
    CpuTime,
    PeakRssBytes,
    CurrencyCost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTelemetrySemanticEffect {
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTelemetryDeclaration {
    pub metric: GeoTelemetryMetric,
    pub unit: String,
    pub origin: GeoValueOrigin,
    pub semantic_effect: GeoTelemetrySemanticEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoResourceBudget {
    pub version: String,
    pub budget_id: String,
    pub deterministic_bounds: Vec<GeoNumericBound>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub telemetry: Vec<GeoTelemetryDeclaration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCapabilityStatus {
    Implemented,
    DiagnosticOnly,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCommandSurface {
    Primary,
    Leaf,
    Measurement,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoContractCapability {
    pub contract_version: String,
    pub schema_path: String,
    pub status: GeoCapabilityStatus,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCommandCapability {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<GeoCommandSurface>,
    pub output_contract: String,
    pub read_only: bool,
    pub uses_network: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCapabilityStatusSets<T> {
    pub implemented: Vec<T>,
    pub diagnostic_only: Vec<T>,
    pub unavailable: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCapabilityVocabularies {
    pub entity_levels: Vec<GeoControlEntityLevel>,
    pub relations: Vec<GeoControlRelation>,
    pub evidence_classes: Vec<GeoEvidenceClass>,
    pub claim_classes: Vec<GeoClaimClass>,
    pub properties: Vec<GeoControlProperty>,
    pub rho_families: GeoCapabilityStatusSets<String>,
    pub geometry_predicates: GeoCapabilityStatusSets<String>,
    pub solver_backends: GeoCapabilityStatusSets<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoScopedProperty {
    pub property: GeoControlProperty,
    pub scope: String,
    pub status: GeoCapabilityStatus,
    pub basis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRuntimeSideEffects {
    pub read_only: bool,
    pub reads_stdin: bool,
    pub reads_input_files: bool,
    pub reads_catalog: bool,
    pub writes_files: bool,
    pub uses_network: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCapabilities {
    pub version: String,
    pub semantic_hash: String,
    pub crate_version: String,
    pub status: GeoCapabilityStatus,
    pub next_command: String,
    pub contracts: GeoCapabilityStatusSets<GeoContractCapability>,
    pub commands: GeoCapabilityStatusSets<GeoCommandCapability>,
    pub vocabularies: GeoCapabilityVocabularies,
    pub deterministic_ceilings: Vec<GeoNumericBound>,
    pub properties: Vec<GeoScopedProperty>,
    pub runtime_side_effects: GeoRuntimeSideEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoInventorySupportStatus {
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRequestedGrainSupport {
    pub entity_level: GeoControlEntityLevel,
    pub status: GeoInventorySupportStatus,
    pub satisfied_evidence_classes: Vec<GeoEvidenceClass>,
    pub missing_evidence_classes: Vec<GeoEvidenceClass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInventorySupportReport {
    pub question_semantic_hash: String,
    pub inventory_semantic_hash: String,
    pub inventory_planning_hash: String,
    pub budget_semantic_hash: String,
    pub status: GeoInventorySupportStatus,
    pub grain_support: Vec<GeoRequestedGrainSupport>,
    pub discovery_gaps: Vec<GeoDiscoveryGap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoControlErrorCode {
    UnsupportedVersion,
    InvalidInput,
    InvalidAsOf,
    MissingQueryAsOf,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoControlError {
    pub code: GeoControlErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoControlError {
    fn new(
        code: GeoControlErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    fn invalid(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoControlErrorCode::InvalidInput, message, detail)
    }
}

impl fmt::Display for GeoControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoControlError {}

pub fn canonicalize_question(question: &GeoQuestion) -> Result<GeoQuestion, GeoControlError> {
    if question.version != CANON_GEO_QUESTION_VERSION {
        return Err(GeoControlError::new(
            GeoControlErrorCode::UnsupportedVersion,
            "Unsupported Geo question contract version",
            [
                ("actual", question.version.as_str()),
                ("expected", CANON_GEO_QUESTION_VERSION),
            ],
        ));
    }
    validate_identifier("question_id", &question.question_id)?;
    validate_geography(&question.bounded_geography)?;
    validate_identifier("resource_budget_ref", &question.resource_budget_ref)?;

    let mut canonical = question.clone();
    for binding in &canonical.subject_bindings {
        validate_identifier("subject_bindings[].role", &binding.role)?;
        validate_text("subject_bindings[].value", &binding.value)?;
    }
    sort_distinct("subject_bindings", &mut canonical.subject_bindings)?;

    for grain in &mut canonical.requested_grains {
        if grain.required_evidence_classes.is_empty() {
            return Err(GeoControlError::invalid(
                "Geo requested grains require at least one source-generic evidence class",
                [(
                    "entity_level",
                    entity_level_name(grain.entity_level).to_string(),
                )],
            ));
        }
        sort_distinct(
            "requested_grains[].required_evidence_classes",
            &mut grain.required_evidence_classes,
        )?;
        sort_distinct(
            "requested_grains[].optional_evidence_classes",
            &mut grain.optional_evidence_classes,
        )?;
    }
    canonical
        .requested_grains
        .sort_by_key(|grain| grain.entity_level);
    reject_duplicate_keys(
        "requested_grains",
        canonical
            .requested_grains
            .iter()
            .map(|grain| entity_level_name(grain.entity_level).to_string()),
    )?;

    if let Some(as_of) = &canonical.query_as_of {
        validate_as_of("query_as_of", as_of)?;
    }
    sort_distinct(
        "requested_claim_classes",
        &mut canonical.requested_claim_classes,
    )?;
    if canonical.requested_claim_classes.is_empty() {
        return Err(GeoControlError::invalid(
            "Geo question requires at least one requested claim class",
            [("field", "requested_claim_classes")],
        ));
    }
    for bound in &canonical.presentation_limits {
        validate_numeric_bound("presentation_limits[]", bound)?;
    }
    sort_distinct("presentation_limits", &mut canonical.presentation_limits)?;
    if let Some(policy) = &canonical.decision_policy {
        validate_identifier("decision_policy.policy_id", &policy.policy_id)?;
        validate_identifier("decision_policy.version", &policy.version)?;
        validate_blake3_hash("decision_policy.content_hash", &policy.content_hash)?;
    }

    if question_requires_query_as_of(&canonical) && canonical.query_as_of.is_none() {
        return Err(GeoControlError::new(
            GeoControlErrorCode::MissingQueryAsOf,
            "Geo question uses temporal claim/evidence vocabulary without query_as_of",
            [("field", "query_as_of")],
        ));
    }

    Ok(canonical)
}

pub fn canonical_question_bytes(question: &GeoQuestion) -> Result<Vec<u8>, GeoControlError> {
    serialize_canonical(&canonicalize_question(question)?)
}

pub fn question_semantic_hash(question: &GeoQuestion) -> Result<String, GeoControlError> {
    canonical_question_bytes(question).map(|bytes| digest_bytes(&bytes))
}

pub fn canonicalize_regional_inventory(
    inventory: &GeoRegionalInventory,
) -> Result<GeoRegionalInventory, GeoControlError> {
    if inventory.version != CANON_GEO_REGIONAL_INVENTORY_VERSION {
        return Err(GeoControlError::new(
            GeoControlErrorCode::UnsupportedVersion,
            "Unsupported Geo regional inventory contract version",
            [
                ("actual", inventory.version.as_str()),
                ("expected", CANON_GEO_REGIONAL_INVENTORY_VERSION),
            ],
        ));
    }
    validate_identifier("inventory_id", &inventory.inventory_id)?;
    validate_geography(&inventory.region)?;

    let mut canonical = inventory.clone();
    for source in &mut canonical.sources {
        validate_source(source)?;
        sort_distinct("sources[].lineage_ids", &mut source.lineage_ids)?;
        sort_distinct("sources[].evidence_classes", &mut source.evidence_classes)?;
        for measure in &source.estimates {
            validate_numeric_measure("sources[].estimates[]", measure)?;
        }
        source.estimates.sort();
        if let Some(geometry) = &mut source.geometry {
            for bound in &geometry.numeric_error_bounds {
                validate_numeric_measure("sources[].geometry.numeric_error_bounds[]", bound)?;
            }
            geometry.numeric_error_bounds.sort();
        }
    }
    canonical.sources.sort();
    reject_duplicate_keys(
        "sources",
        canonical
            .sources
            .iter()
            .map(|source| source.source_instance_id.clone()),
    )?;

    for gap in &canonical.discovery_gaps {
        validate_discovery_gap(gap)?;
    }
    canonical.discovery_gaps.sort();
    reject_duplicate_keys(
        "discovery_gaps",
        canonical
            .discovery_gaps
            .iter()
            .map(|gap| gap.gap_id.clone()),
    )?;

    Ok(canonical)
}

pub fn canonical_regional_inventory_bytes(
    inventory: &GeoRegionalInventory,
) -> Result<Vec<u8>, GeoControlError> {
    serialize_canonical(&canonicalize_regional_inventory(inventory)?)
}

pub fn regional_inventory_semantic_hash(
    inventory: &GeoRegionalInventory,
) -> Result<String, GeoControlError> {
    canonical_regional_inventory_bytes(inventory).map(|bytes| digest_bytes(&bytes))
}

pub fn regional_inventory_planning_hash(
    inventory: &GeoRegionalInventory,
) -> Result<String, GeoControlError> {
    let canonical = canonicalize_regional_inventory(inventory)?;
    let mut sources = canonical
        .sources
        .iter()
        .map(|source| GeoRegionalSourcePlanningProjection {
            release: source.release.clone(),
            temporal_scope: source.temporal_scope.clone(),
            native_scope: source.native_scope.clone(),
            evidence_classes: source.evidence_classes.clone(),
            coverage: source.coverage.clone(),
            availability: source.local_state.state,
            local_content_hash: source
                .local_state
                .local_ref
                .as_ref()
                .map(|reference| reference.content_hash.clone()),
            local_contract_version: source
                .local_state
                .local_ref
                .as_ref()
                .map(|reference| reference.contract_version.clone()),
            local_media_type: source
                .local_state
                .local_ref
                .as_ref()
                .map(|reference| reference.media_type.clone()),
            geometry: source.geometry.clone(),
            license_class: source.license_class,
            egress_class: source.egress_class,
            estimates: source.estimates.clone(),
        })
        .collect::<Vec<_>>();
    sources.sort();
    let projection = GeoRegionalInventoryPlanningProjection {
        region: canonical.region,
        sources,
        discovery_gaps: canonical.discovery_gaps,
    };
    serialize_canonical(&projection).map(|bytes| digest_bytes(&bytes))
}

pub fn canonicalize_resource_budget(
    budget: &GeoResourceBudget,
) -> Result<GeoResourceBudget, GeoControlError> {
    if budget.version != CANON_GEO_RESOURCE_BUDGET_VERSION {
        return Err(GeoControlError::new(
            GeoControlErrorCode::UnsupportedVersion,
            "Unsupported Geo resource budget contract version",
            [
                ("actual", budget.version.as_str()),
                ("expected", CANON_GEO_RESOURCE_BUDGET_VERSION),
            ],
        ));
    }
    validate_identifier("budget_id", &budget.budget_id)?;
    if budget.deterministic_bounds.is_empty() {
        return Err(GeoControlError::invalid(
            "Geo resource budget requires deterministic semantic bounds",
            [("field", "deterministic_bounds")],
        ));
    }

    let mut canonical = budget.clone();
    for bound in &canonical.deterministic_bounds {
        validate_numeric_bound("deterministic_bounds[]", bound)?;
    }
    sort_distinct("deterministic_bounds", &mut canonical.deterministic_bounds)?;
    for telemetry in &canonical.telemetry {
        validate_identifier("telemetry[].unit", &telemetry.unit)?;
        if telemetry.semantic_effect != GeoTelemetrySemanticEffect::None {
            return Err(GeoControlError::invalid(
                "Geo telemetry must not affect semantic output",
                [("metric", telemetry_metric_name(telemetry.metric))],
            ));
        }
    }
    canonical.telemetry.sort();
    Ok(canonical)
}

pub fn canonical_resource_budget_bytes(
    budget: &GeoResourceBudget,
) -> Result<Vec<u8>, GeoControlError> {
    serialize_canonical(&canonicalize_resource_budget(budget)?)
}

pub fn resource_budget_semantic_hash(
    budget: &GeoResourceBudget,
) -> Result<String, GeoControlError> {
    canonical_resource_budget_bytes(budget).map(|bytes| digest_bytes(&bytes))
}

pub fn default_geo_capabilities() -> Result<GeoCapabilities, GeoControlError> {
    finalized_capabilities(GeoCapabilities {
        version: CANON_GEO_CAPABILITIES_VERSION.to_string(),
        semantic_hash: String::new(),
        crate_version: env!("CARGO_PKG_VERSION").to_string(),
        status: GeoCapabilityStatus::Implemented,
        next_command: "canon --describe".to_string(),
        contracts: GeoCapabilityStatusSets {
            implemented: implemented_geo_contracts(),
            diagnostic_only: diagnostic_geo_contracts(),
            unavailable: Vec::new(),
        },
        commands: GeoCapabilityStatusSets {
            implemented: implemented_geo_commands(),
            diagnostic_only: Vec::new(),
            unavailable: unavailable_geo_commands(),
        },
        vocabularies: GeoCapabilityVocabularies {
            entity_levels: all_entity_levels(),
            relations: all_relations(),
            evidence_classes: all_evidence_classes(),
            claim_classes: all_claim_classes(),
            properties: all_properties(),
            rho_families: GeoCapabilityStatusSets {
                implemented: vec![
                    "logical_relaxation".to_string(),
                    "empirical_calibration_diagnostic".to_string(),
                ],
                diagnostic_only: vec![
                    "time_scoped_observation_without_temporal_solver".to_string(),
                ],
                unavailable: vec!["allen_interval_algebra_solver".to_string()],
            },
            geometry_predicates: GeoCapabilityStatusSets {
                implemented: vec![
                    "integer_local_geometry_materialization".to_string(),
                    "h3_blocking_and_ownership_metadata".to_string(),
                ],
                diagnostic_only: Vec::new(),
                unavailable: vec!["national_single_solve_geometry_truth".to_string()],
            },
            solver_backends: GeoCapabilityStatusSets {
                implemented: vec![
                    "any_of_inclusion_exclusion".to_string(),
                    "exhaustive_enumeration".to_string(),
                    "pruned_depth_first_budgeted".to_string(),
                ],
                diagnostic_only: Vec::new(),
                unavailable: vec![
                    "canonical_sdd".to_string(),
                    "allen_stp_temporal".to_string(),
                ],
            },
        },
        deterministic_ceilings: vec![GeoNumericBound {
            semantic_id: "composition.default_max_materialized_models".to_string(),
            counter: GeoResourceCounter::Models,
            value: super::composition::DEFAULT_MAX_MATERIALIZED_MODELS,
            unit: "model".to_string(),
            origin: GeoValueOrigin::BinaryDefault,
            action: GeoBudgetAction::TruncatePresentationOnly,
        }],
        properties: vec![
            scoped_property(
                GeoControlProperty::Deterministic,
                "canon_geo_capabilities.v0 canonical JSON",
                GeoCapabilityStatus::Implemented,
                "sorted vectors, BTreeMap refusal details, and no ambient time inputs",
            ),
            scoped_property(
                GeoControlProperty::Confluent,
                "source-generic control contract canonicalization",
                GeoCapabilityStatus::DiagnosticOnly,
                "order-invariant canonicalization is implemented for declared lists, but no join/solver confluence guarantee is shipped",
            ),
            scoped_property(
                GeoControlProperty::Sound,
                "rho admission vocabulary",
                GeoCapabilityStatus::DiagnosticOnly,
                "soundness is declared by versioned rho families, not by source count",
            ),
            scoped_property(
                GeoControlProperty::Complete,
                "regional inventory reach",
                GeoCapabilityStatus::Unavailable,
                "truth reach requires declared regional evidence and is not implied by capabilities",
            ),
            scoped_property(
                GeoControlProperty::Canonical,
                "capability artifact normal form",
                GeoCapabilityStatus::Implemented,
                "semantic_hash is computed over canonical bytes with the hash field empty",
            ),
        ],
        runtime_side_effects: GeoRuntimeSideEffects {
            read_only: true,
            reads_stdin: false,
            reads_input_files: false,
            reads_catalog: false,
            writes_files: false,
            uses_network: false,
        },
    })
}

fn implemented_geo_contracts() -> Vec<GeoContractCapability> {
    vec![
        contract(
            CANON_GEO_QUESTION_VERSION,
            "schemas/canon.geo.question.v0.schema.json",
            "source-generic question intent contract",
        ),
        contract(
            CANON_GEO_CAPABILITIES_VERSION,
            "schemas/canon.geo.capabilities.v0.schema.json",
            "compiled offline capability contract",
        ),
        contract(
            CANON_GEO_REGIONAL_INVENTORY_VERSION,
            "schemas/canon.geo.regional_inventory.v1.schema.json",
            "regional source availability and discovery gap contract",
        ),
        contract(
            CANON_GEO_RESOURCE_BUDGET_VERSION,
            "schemas/canon.geo.resource_budget.v0.schema.json",
            "deterministic semantic counter budget contract",
        ),
        contract(
            CANON_GEO_PLAN_VERSION,
            "schemas/canon.geo.plan.v0.schema.json",
            "offline Geo semantic plan overlay contract",
        ),
        contract(
            CANON_GEO_RUN_VERSION,
            "schemas/canon.geo.run.v0.schema.json",
            "bounded resumable Geo run artifact contract",
        ),
        contract(
            CANON_GEO_DISCOVERY_REQUEST_VERSION,
            "schemas/canon.geo.discovery_request.v0.schema.json",
            "protocol-neutral bounded catalog discovery request contract",
        ),
        contract(
            CANON_GEO_ACQUISITION_REQUEST_VERSION,
            "schemas/canon.geo.acquisition_request.v0.schema.json",
            "release-pinned bounded external acquisition request contract",
        ),
        contract(
            CANON_GEO_ACQUISITION_RECEIPT_VERSION,
            "schemas/canon.geo.acquisition_receipt.v0.schema.json",
            "verified external acquisition receipt contract",
        ),
        contract(
            CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION,
            "schemas/canon.geo.regional_inventory_advancement.v0.schema.json",
            "immutable plan-bound regional inventory advancement contract",
        ),
        contract(
            CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION,
            "schemas/canon.geo.address_parse_request.v0.schema.json",
            "regular-address parse request contract",
        ),
        contract(
            CANON_GEO_ADDRESS_PARSE_FOREST_VERSION,
            "schemas/canon.geo.address_parse_forest.v0.schema.json",
            "regular-address parse forest contract",
        ),
        contract(
            CANON_GEO_ADDRESS_QUERY_GRAMMAR_VERSION,
            "src/geo/address.rs",
            "versioned regular-address grammar contract",
        ),
        contract(
            CANON_GEO_PAD_ADDRESS_SET_VERSION,
            "schemas/canon.geo.pad_address_set.v0.schema.json",
            "PAD address-set input contract",
        ),
        contract(
            CANON_GEO_PAD_MEMBERSHIP_VERSION,
            "schemas/canon.geo.pad_membership.v0.schema.json",
            "PAD membership evaluation contract",
        ),
        contract(
            CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION,
            "schemas/canon.geo.address_parcel_bridge_request.v0.schema.json",
            "address/PAD evidence bridge request contract",
        ),
        contract(
            CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION,
            "schemas/canon.geo.address_parcel_bridge.v0.schema.json",
            "address/PAD parcel-candidate evidence bridge contract",
        ),
        contract(
            CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION,
            "schemas/canon.geo.address_parcel_evidence_request.v0.schema.json",
            "single-call address-to-parcel evidence request contract",
        ),
        contract(
            CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION,
            "schemas/canon.geo.address_parcel_evidence_bundle.v0.schema.json",
            "single-call address-to-parcel evidence bundle contract",
        ),
        contract(
            CANON_GEO_COMPOSITION_PROFILE_VERSION,
            "schemas/canon.geo.composition_request.v0.schema.json#/$defs/composition_profile",
            "parcel/building composition profile contract",
        ),
        contract(
            CANON_GEO_COMPOSITION_REQUEST_VERSION,
            "schemas/canon.geo.composition_request.v0.schema.json",
            "bounded composition request contract",
        ),
        contract(
            CANON_GEO_COMPOSITION_VERSION,
            "schemas/canon.geo.composition.v0.schema.json",
            "exact residual composition artifact contract",
        ),
        contract(
            CANON_GEO_ENTITY_PROJECTION_VERSION,
            "schemas/canon.geo.composition.v0.schema.json#/$defs/entity_projection",
            "entity-level projection over a bounded composition residual",
        ),
        contract(
            CANON_GEO_GEOMETRY_REQUEST_VERSION,
            "schemas/canon.geo.geometry_request.v0.schema.json",
            "offline geometry materialization request contract",
        ),
        contract(
            CANON_GEO_GEOMETRY_VALUE_VERSION,
            "schemas/canon.geo.geometry_tile.v0.schema.json#/$defs/typed_geometry",
            "canonical tile-local typed geometry value contract",
        ),
        contract(
            CANON_GEO_GEOMETRY_TILE_VERSION,
            "schemas/canon.geo.geometry_tile.v0.schema.json",
            "source-plane geometry tile artifact contract",
        ),
        contract(
            CANON_GEO_LOCAL_FRAME_VERSION,
            "schemas/canon.geo.geometry_tile.v0.schema.json#/$defs/frame",
            "tile-local integer frame contract",
        ),
        contract(
            CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
            "schemas/canon.geo.warehouse_geometry_rows.v0.schema.json",
            "release-pinned warehouse geometry row contract",
        ),
        contract(
            CANON_GEO_WAREHOUSE_GEOMETRY_VERSION,
            "schemas/canon.geo.warehouse_geometry.v0.schema.json",
            "warehouse geometry materialization artifact contract",
        ),
        contract(
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            "schemas/canon.geo.home_cell_rows.v1.schema.json",
            "representative-point home-cell row contract",
        ),
        contract(
            CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
            "schemas/canon.geo.home_cell_assignment.v1.schema.json",
            "H3 blocking/ownership assignment artifact contract",
        ),
        contract(
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            "schemas/canon.geo.tile_work_request.v1.schema.json",
            "bounded tile-work request contract",
        ),
        contract(
            CANON_GEO_TILE_WORK_UNIT_VERSION,
            "schemas/canon.geo.tile_work_unit.v1.schema.json",
            "center-plus-halo tile work-unit artifact contract",
        ),
        contract(
            CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
            "schemas/canon.geo.tile_reconciliation_request.v1.schema.json",
            "cross-tile reconciliation request contract",
        ),
        contract(
            CANON_GEO_TILE_RECONCILIATION_VERSION,
            "schemas/canon.geo.tile_reconciliation.v1.schema.json",
            "owned tile-decision reconciliation artifact contract",
        ),
        contract(
            CANON_GEO_MULTISOURCE_REQUEST_VERSION,
            "schemas/canon.geo.multisource_request.v0.schema.json",
            "Geo N-source row composition request contract",
        ),
        contract(
            ENTITY_MULTISOURCE_LINK_VERSION,
            "schemas/canon.entity.multisource_link.v1.schema.json",
            "entity multisource artifact produced by geo link-sources",
        ),
        contract(
            CANON_GEO_EVIDENCE_REQUEST_VERSION,
            "schemas/canon.geo.evidence_request.v0.schema.json",
            "rho evidence admission request contract",
        ),
        contract(
            CANON_GEO_EVIDENCE_COMPILATION_VERSION,
            "schemas/canon.geo.evidence_compilation.v0.schema.json",
            "admitted evidence compilation artifact contract",
        ),
        contract(
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            "schemas/canon.geo.warehouse_rows.v0.schema.json",
            "release-pinned offline evidence row contract",
        ),
        contract(
            CANON_GEO_H7_POPULATION_ROWS_VERSION,
            "schemas/canon.geo.h7_population_rows.v0.schema.json",
            "Appendix H.7 accepted-loan population row contract",
        ),
        contract(
            CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
            "schemas/canon.geo.h7_staging_source_record_bytes_batch.v0.schema.json",
            "H.7 NYC staging-profile source-record byte batch contract",
        ),
        contract(
            CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION,
            "schemas/canon.geo.h7_pip_block_population_batch.v0.schema.json",
            "H.7 PIP-block observed population adapter contract",
        ),
        contract(
            CANON_GEO_H7_POPULATION_VERSION,
            "schemas/canon.geo.h7_population.v0.schema.json",
            "typed H.7 population artifact contract",
        ),
        contract(
            CANON_GEO_POPULATION_REQUEST_VERSION,
            "schemas/canon.geo.population_request.v0.schema.json",
            "labeled population evaluation request contract",
        ),
        contract(
            CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
            "schemas/canon.geo.population_evidence_stack_request.v0.schema.json",
            "truth-blind bounded population evidence-overlay request contract",
        ),
        contract(
            CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION,
            "schemas/canon.geo.population_evidence_stack.v0.schema.json",
            "replay-validatable accretive population evidence-stack artifact",
        ),
        contract(
            CANON_GEO_POPULATION_EVALUATION_VERSION,
            "schemas/canon.geo.population_evaluation.v0.schema.json",
            "population evaluation artifact contract",
        ),
    ]
}

fn diagnostic_geo_contracts() -> Vec<GeoContractCapability> {
    vec![
        diagnostic_contract(
            CANON_GEO_RESIDUAL_BENCHMARK_VERSION,
            "src/geo/residual_benchmark.rs",
            "diagnostic residual representation benchmark report contract",
        ),
        diagnostic_contract(
            CANON_GEO_RESIDUAL_OBDD_VERSION,
            "src/geo/residual_benchmark.rs",
            "diagnostic residual OBDD equivalence contract",
        ),
    ]
}

fn implemented_geo_commands() -> Vec<GeoCommandCapability> {
    vec![
        command(
            "canon geo capabilities --emit json",
            GeoCommandSurface::Primary,
            CANON_GEO_CAPABILITIES_VERSION,
            true,
            false,
        ),
        command(
            "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>",
            GeoCommandSurface::Primary,
            CANON_GEO_PLAN_VERSION,
            true,
            false,
        ),
        command(
            "canon geo run --plan <PLAN.json> --work-dir <DIR> [--input <NODE_ID:BINDING_ID=PATH>...] [--satisfy <REQUEST_ID=RECEIPT.json>...]",
            GeoCommandSurface::Primary,
            CANON_GEO_RUN_VERSION,
            false,
            false,
        ),
        command(
            "canon geo replan-from-acquisition --base-plan <PLAN.json> --base-inventory <INVENTORY.json> --question <QUESTION.json> --capabilities <CAPABILITIES.json> --profile <PROFILE.json> --budget <BUDGET.json> --satisfy <REQUEST_ID=RECEIPT.json> --local-artifact <LOCAL_ARTIFACT_ID=PATH>... [--result <DIGEST_ID=PATH>...] --advancement-out <ADVANCEMENT.json>",
            GeoCommandSurface::Primary,
            CANON_GEO_PLAN_VERSION,
            false,
            false,
        ),
        command(
            "canon geo link-sources --request <REQUEST.json> --rows-out <ROWS.csv>",
            GeoCommandSurface::Leaf,
            ENTITY_MULTISOURCE_LINK_VERSION,
            false,
            false,
        ),
        command(
            "canon geo materialize-home-cells --rows <ROWS.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
            true,
            false,
        ),
        command(
            "canon geo tile-work --request <REQUEST.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_TILE_WORK_UNIT_VERSION,
            true,
            false,
        ),
        command(
            "canon geo reconcile-tiles --request <REQUEST.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_TILE_RECONCILIATION_VERSION,
            true,
            false,
        ),
        command(
            "canon geo solve --request <REQUEST.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_COMPOSITION_VERSION,
            true,
            false,
        ),
        command(
            "canon geo materialize-geometry --request <REQUEST.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_GEOMETRY_TILE_VERSION,
            true,
            false,
        ),
        command(
            "canon geo materialize-warehouse-geometry --rows <ROWS.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_WAREHOUSE_GEOMETRY_VERSION,
            true,
            false,
        ),
        command(
            "canon geo materialize-evidence --rows <ROWS.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_EVIDENCE_REQUEST_VERSION,
            true,
            false,
        ),
        command(
            "canon geo materialize-address-evidence --request <REQUEST.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION,
            true,
            false,
        ),
        command(
            "canon geo materialize-h7-population --rows <ROWS.json>",
            GeoCommandSurface::Measurement,
            CANON_GEO_H7_POPULATION_VERSION,
            true,
            false,
        ),
        command(
            "canon geo materialize-h7-staging-batch --batch <BATCH.json>",
            GeoCommandSurface::Measurement,
            CANON_GEO_H7_POPULATION_VERSION,
            true,
            false,
        ),
        command(
            "canon geo materialize-h7-pip-block-batch --batch <BATCH.json>",
            GeoCommandSurface::Measurement,
            CANON_GEO_H7_POPULATION_VERSION,
            true,
            false,
        ),
        command(
            "canon geo compile-evidence --request <REQUEST.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_EVIDENCE_COMPILATION_VERSION,
            true,
            false,
        ),
        command(
            "canon geo stack-evidence --population <POPULATION.json> --overlay <OVERLAY.json>",
            GeoCommandSurface::Leaf,
            CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION,
            true,
            false,
        ),
        command(
            "canon geo evaluate --population <POPULATION.json>",
            GeoCommandSurface::Primary,
            CANON_GEO_POPULATION_EVALUATION_VERSION,
            true,
            false,
        ),
    ]
}

fn unavailable_geo_commands() -> Vec<GeoCommandCapability> {
    vec![
        command(
            "canon geo inspect",
            GeoCommandSurface::Primary,
            "planned_not_implemented",
            true,
            false,
        ),
        command(
            "canon geo ledger",
            GeoCommandSurface::Primary,
            "planned_not_implemented",
            true,
            false,
        ),
    ]
}

pub fn canonicalize_capabilities(
    capabilities: &GeoCapabilities,
) -> Result<GeoCapabilities, GeoControlError> {
    if capabilities.version != CANON_GEO_CAPABILITIES_VERSION {
        return Err(GeoControlError::new(
            GeoControlErrorCode::UnsupportedVersion,
            "Unsupported Geo capabilities contract version",
            [
                ("actual", capabilities.version.as_str()),
                ("expected", CANON_GEO_CAPABILITIES_VERSION),
            ],
        ));
    }
    finalized_capabilities(capabilities.clone())
}

pub fn canonical_capabilities_bytes(
    capabilities: &GeoCapabilities,
) -> Result<Vec<u8>, GeoControlError> {
    serialize_canonical(&canonicalize_capabilities(capabilities)?)
}

pub fn capabilities_semantic_hash(
    capabilities: &GeoCapabilities,
) -> Result<String, GeoControlError> {
    canonicalize_capabilities(capabilities).map(|canonical| canonical.semantic_hash)
}

pub fn evaluate_inventory_support(
    question: &GeoQuestion,
    inventory: &GeoRegionalInventory,
    budget: &GeoResourceBudget,
) -> Result<GeoInventorySupportReport, GeoControlError> {
    let question = canonicalize_question(question)?;
    let inventory = canonicalize_regional_inventory(inventory)?;
    let budget = canonicalize_resource_budget(budget)?;
    if question.resource_budget_ref != budget.budget_id {
        return Err(GeoControlError::invalid(
            "Geo question resource_budget_ref must match the supplied resource budget id",
            [
                ("resource_budget_ref", question.resource_budget_ref.as_str()),
                ("budget_id", budget.budget_id.as_str()),
            ],
        ));
    }
    let query_day = question
        .query_as_of
        .as_ref()
        .map(|as_of| parse_utc_day("query_as_of.utc_day", &as_of.utc_day))
        .transpose()?;
    let inventory_region_matches = inventory.region == question.bounded_geography;
    let stable_identity_requested = question
        .requested_claim_classes
        .binary_search(&GeoClaimClass::StableIdentity)
        .is_ok();

    let mut grain_support = Vec::new();
    let mut discovery_gaps = Vec::new();
    for grain in &question.requested_grains {
        let mut satisfied = Vec::new();
        let mut missing = Vec::new();
        for evidence_class in &grain.required_evidence_classes {
            let mut is_satisfied = false;
            let mut requires_query_as_of = false;
            let mut outside_valid_interval = false;
            if inventory_region_matches {
                for source in &inventory.sources {
                    if !source_matches_requested_grain(
                        source,
                        grain.entity_level,
                        *evidence_class,
                        &question.bounded_geography,
                        stable_identity_requested,
                    ) {
                        continue;
                    }
                    match source_time_support(source, query_day)? {
                        GeoSourceTimeSupport::Supported => {
                            is_satisfied = true;
                            break;
                        }
                        GeoSourceTimeSupport::MissingQueryAsOf => {
                            requires_query_as_of = true;
                        }
                        GeoSourceTimeSupport::OutsideValidInterval => {
                            outside_valid_interval = true;
                        }
                    }
                }
            }
            if requires_query_as_of {
                return Err(GeoControlError::new(
                    GeoControlErrorCode::MissingQueryAsOf,
                    "Geo cannot use a time-scoped source as timeless evidence",
                    [
                        ("entity_level", entity_level_name(grain.entity_level)),
                        ("evidence_class", evidence_class_name(*evidence_class)),
                        ("field", "query_as_of"),
                    ],
                ));
            }
            if is_satisfied {
                satisfied.push(*evidence_class);
            } else {
                missing.push(*evidence_class);
                discovery_gaps.push(GeoDiscoveryGap {
                    gap_id: format!(
                        "gap:{}:{}",
                        entity_level_name(grain.entity_level),
                        evidence_class_name(*evidence_class)
                    ),
                    requested_entity_level: Some(grain.entity_level),
                    requested_evidence_class: *evidence_class,
                    reason: if !inventory_region_matches {
                        "regional inventory region does not match the question bounded_geography"
                    } else if outside_valid_interval {
                        "available regional source exists, but query_as_of is outside its declared valid_time interval"
                    } else if stable_identity_requested {
                        "no available native source with stable-alias participation declares the requested evidence class at the requested entity level"
                    } else {
                        "no available regional source declares the requested evidence class at the requested entity level"
                    }
                    .to_string(),
                    next_command: "supply a local source instance in canon_geo_regional_inventory.v1".to_string(),
                });
            }
        }
        grain_support.push(GeoRequestedGrainSupport {
            entity_level: grain.entity_level,
            status: if missing.is_empty() {
                GeoInventorySupportStatus::Supported
            } else {
                GeoInventorySupportStatus::Unsupported
            },
            satisfied_evidence_classes: satisfied,
            missing_evidence_classes: missing,
        });
    }

    discovery_gaps.sort();
    discovery_gaps.dedup();
    let status = if grain_support
        .iter()
        .all(|support| support.status == GeoInventorySupportStatus::Supported)
    {
        GeoInventorySupportStatus::Supported
    } else {
        GeoInventorySupportStatus::Unsupported
    };

    Ok(GeoInventorySupportReport {
        question_semantic_hash: question_semantic_hash(&question)?,
        inventory_semantic_hash: regional_inventory_semantic_hash(&inventory)?,
        inventory_planning_hash: regional_inventory_planning_hash(&inventory)?,
        budget_semantic_hash: resource_budget_semantic_hash(&budget)?,
        status,
        grain_support,
        discovery_gaps,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct GeoRegionalSourcePlanningProjection {
    release: GeoSourceRelease,
    temporal_scope: GeoTemporalScope,
    native_scope: GeoNativeEntityScope,
    evidence_classes: Vec<GeoEvidenceClass>,
    coverage: GeoCoveragePredicate,
    availability: GeoSourceAvailability,
    local_content_hash: Option<String>,
    local_contract_version: Option<String>,
    local_media_type: Option<String>,
    geometry: Option<GeoGeometryTransformContract>,
    license_class: GeoLicenseClass,
    egress_class: GeoEgressClass,
    estimates: Vec<GeoNumericMeasure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GeoRegionalInventoryPlanningProjection {
    region: GeoBoundedGeography,
    sources: Vec<GeoRegionalSourcePlanningProjection>,
    discovery_gaps: Vec<GeoDiscoveryGap>,
}

fn finalized_capabilities(
    mut capabilities: GeoCapabilities,
) -> Result<GeoCapabilities, GeoControlError> {
    capabilities.semantic_hash.clear();
    validate_identifier("crate_version", &capabilities.crate_version)?;
    validate_identifier("next_command", &capabilities.next_command)?;
    sort_status_sets("contracts", &mut capabilities.contracts)?;
    reject_cross_status_contract_versions("contracts", &capabilities.contracts)?;
    sort_status_sets("commands", &mut capabilities.commands)?;
    reject_cross_status_command_names("commands", &capabilities.commands)?;
    sort_distinct(
        "vocabularies.entity_levels",
        &mut capabilities.vocabularies.entity_levels,
    )?;
    sort_distinct(
        "vocabularies.relations",
        &mut capabilities.vocabularies.relations,
    )?;
    sort_distinct(
        "vocabularies.evidence_classes",
        &mut capabilities.vocabularies.evidence_classes,
    )?;
    sort_distinct(
        "vocabularies.claim_classes",
        &mut capabilities.vocabularies.claim_classes,
    )?;
    sort_distinct(
        "vocabularies.properties",
        &mut capabilities.vocabularies.properties,
    )?;
    sort_status_sets(
        "vocabularies.rho_families",
        &mut capabilities.vocabularies.rho_families,
    )?;
    sort_status_sets(
        "vocabularies.geometry_predicates",
        &mut capabilities.vocabularies.geometry_predicates,
    )?;
    sort_status_sets(
        "vocabularies.solver_backends",
        &mut capabilities.vocabularies.solver_backends,
    )?;
    for bound in &capabilities.deterministic_ceilings {
        validate_numeric_bound("deterministic_ceilings[]", bound)?;
    }
    sort_distinct(
        "deterministic_ceilings",
        &mut capabilities.deterministic_ceilings,
    )?;
    for property in &capabilities.properties {
        validate_identifier("properties[].scope", &property.scope)?;
        validate_text("properties[].basis", &property.basis)?;
    }
    capabilities.properties.sort();
    let bytes = serialize_canonical(&capabilities)?;
    capabilities.semantic_hash = digest_bytes(&bytes);
    Ok(capabilities)
}

fn sort_status_sets<T: Ord + Clone>(
    field: &str,
    sets: &mut GeoCapabilityStatusSets<T>,
) -> Result<(), GeoControlError> {
    sort_distinct(&format!("{field}.implemented"), &mut sets.implemented)?;
    sort_distinct(
        &format!("{field}.diagnostic_only"),
        &mut sets.diagnostic_only,
    )?;
    sort_distinct(&format!("{field}.unavailable"), &mut sets.unavailable)?;
    reject_cross_status_items(field, sets)
}

fn reject_cross_status_items<T: Ord + Clone>(
    field: &str,
    sets: &GeoCapabilityStatusSets<T>,
) -> Result<(), GeoControlError> {
    let mut seen = BTreeSet::new();
    for (bucket, values) in [
        ("implemented", sets.implemented.as_slice()),
        ("diagnostic_only", sets.diagnostic_only.as_slice()),
        ("unavailable", sets.unavailable.as_slice()),
    ] {
        for value in values {
            if !seen.insert(value.clone()) {
                return Err(GeoControlError::invalid(
                    "Geo capability status items must appear in exactly one status bucket",
                    [
                        (String::from("field"), field.to_string()),
                        (String::from("bucket"), bucket.to_string()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn reject_cross_status_contract_versions(
    field: &str,
    sets: &GeoCapabilityStatusSets<GeoContractCapability>,
) -> Result<(), GeoControlError> {
    let mut seen = BTreeMap::new();
    for (bucket, expected_status, values) in [
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
        for value in values {
            if value.status != expected_status {
                return Err(GeoControlError::invalid(
                    "Geo contract capability row status must match its status bucket",
                    [
                        (String::from("field"), field.to_string()),
                        (String::from("bucket"), bucket.to_string()),
                        (
                            String::from("contract_version"),
                            value.contract_version.clone(),
                        ),
                    ],
                ));
            }
            if let Some(first_bucket) = seen.insert(value.contract_version.as_str(), bucket) {
                return Err(GeoControlError::invalid(
                    "Geo contract versions must appear in exactly one status bucket",
                    [
                        (String::from("field"), field.to_string()),
                        (
                            String::from("contract_version"),
                            value.contract_version.clone(),
                        ),
                        (String::from("first_bucket"), first_bucket.to_string()),
                        (String::from("second_bucket"), bucket.to_string()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn reject_cross_status_command_names(
    field: &str,
    sets: &GeoCapabilityStatusSets<GeoCommandCapability>,
) -> Result<(), GeoControlError> {
    let mut seen = BTreeMap::new();
    for (bucket, values) in [
        ("implemented", sets.implemented.as_slice()),
        ("diagnostic_only", sets.diagnostic_only.as_slice()),
        ("unavailable", sets.unavailable.as_slice()),
    ] {
        for value in values {
            if let Some(first_bucket) = seen.insert(value.command.as_str(), bucket) {
                return Err(GeoControlError::invalid(
                    "Geo command names must appear in exactly one status bucket",
                    [
                        (String::from("field"), field.to_string()),
                        (String::from("command"), value.command.clone()),
                        (String::from("first_bucket"), first_bucket.to_string()),
                        (String::from("second_bucket"), bucket.to_string()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn source_supports_level(source: &GeoRegionalSourceInstance, level: GeoControlEntityLevel) -> bool {
    match source.native_scope {
        GeoNativeEntityScope::NativeEntity { entity_level, .. } => entity_level == level,
        GeoNativeEntityScope::ObservationOnly => false,
    }
}

fn source_matches_requested_grain(
    source: &GeoRegionalSourceInstance,
    entity_level: GeoControlEntityLevel,
    evidence_class: GeoEvidenceClass,
    geography: &GeoBoundedGeography,
    stable_identity_requested: bool,
) -> bool {
    source.local_state.state == GeoSourceAvailability::Available
        && regional_source_has_usable_local_evidence(source)
        && source
            .evidence_classes
            .binary_search(&evidence_class)
            .is_ok()
        && source_supports_level(source, entity_level)
        && (!stable_identity_requested || source.native_scope.may_contribute_stable_alias())
        && source.coverage.region == *geography
}

pub(crate) fn regional_source_has_usable_local_evidence(
    source: &GeoRegionalSourceInstance,
) -> bool {
    source
        .local_state
        .local_ref
        .as_ref()
        .is_some_and(|reference| {
            reference.media_type == "application/json"
                && reference.contract_version == CANON_GEO_WAREHOUSE_ROWS_VERSION
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeoSourceTimeSupport {
    Supported,
    MissingQueryAsOf,
    OutsideValidInterval,
}

fn source_time_support(
    source: &GeoRegionalSourceInstance,
    query_day: Option<chrono::NaiveDate>,
) -> Result<GeoSourceTimeSupport, GeoControlError> {
    let Some(interval) = &source.temporal_scope.valid_time else {
        return Ok(GeoSourceTimeSupport::Supported);
    };
    let Some(query_day) = query_day else {
        return Ok(GeoSourceTimeSupport::MissingQueryAsOf);
    };
    if interval_contains_utc_day("sources[].temporal_scope.valid_time", interval, query_day)? {
        Ok(GeoSourceTimeSupport::Supported)
    } else {
        Ok(GeoSourceTimeSupport::OutsideValidInterval)
    }
}

fn interval_contains_utc_day(
    field: &str,
    interval: &GeoDateInterval,
    day: chrono::NaiveDate,
) -> Result<bool, GeoControlError> {
    let start = parse_utc_day(&format!("{field}.start_utc_day"), &interval.start_utc_day)?;
    let end = parse_utc_day(&format!("{field}.end_utc_day"), &interval.end_utc_day)?;
    Ok(start <= day && day <= end)
}

fn question_requires_query_as_of(question: &GeoQuestion) -> bool {
    question.requested_claim_classes.iter().any(|claim| {
        matches!(
            claim,
            GeoClaimClass::TemporalOccupancy | GeoClaimClass::LifecycleState
        )
    }) || question.requested_grains.iter().any(|grain| {
        grain
            .required_evidence_classes
            .binary_search(&GeoEvidenceClass::TemporalObservation)
            .is_ok()
            || grain
                .optional_evidence_classes
                .binary_search(&GeoEvidenceClass::TemporalObservation)
                .is_ok()
    })
}

fn validate_source(source: &GeoRegionalSourceInstance) -> Result<(), GeoControlError> {
    validate_identifier("sources[].source_instance_id", &source.source_instance_id)?;
    validate_identifier("sources[].release.release_id", &source.release.release_id)?;
    validate_blake3_hash(
        "sources[].release.release_digest",
        &source.release.release_digest,
    )?;
    validate_temporal_scope(&source.temporal_scope)?;
    if source.lineage_ids.is_empty() {
        return Err(GeoControlError::invalid(
            "Geo source instances require at least one lineage identifier",
            [("source_instance_id", source.source_instance_id.as_str())],
        ));
    }
    for lineage_id in &source.lineage_ids {
        validate_identifier("sources[].lineage_ids[]", lineage_id)?;
    }
    if source.evidence_classes.is_empty() {
        return Err(GeoControlError::invalid(
            "Geo source instances require at least one source-generic evidence class",
            [("source_instance_id", source.source_instance_id.as_str())],
        ));
    }
    validate_coverage(&source.coverage)?;
    validate_local_state(&source.local_state)?;
    if let Some(geometry) = &source.geometry {
        validate_identifier(
            "sources[].geometry.geometry_contract_version",
            &geometry.geometry_contract_version,
        )?;
        validate_identifier(
            "sources[].geometry.coordinate_reference_system",
            &geometry.coordinate_reference_system,
        )?;
        validate_identifier("sources[].geometry.transform_id", &geometry.transform_id)?;
        validate_blake3_hash(
            "sources[].geometry.transform_digest",
            &geometry.transform_digest,
        )?;
    }
    Ok(())
}

fn validate_temporal_scope(scope: &GeoTemporalScope) -> Result<(), GeoControlError> {
    if let Some(interval) = &scope.valid_time {
        validate_interval("temporal_scope.valid_time", interval)?;
    }
    if let Some(interval) = &scope.transaction_time {
        validate_interval("temporal_scope.transaction_time", interval)?;
    }
    if let Some(as_of) = &scope.release_time {
        validate_as_of("temporal_scope.release_time", as_of)?;
    }
    Ok(())
}

fn validate_interval(field: &str, interval: &GeoDateInterval) -> Result<(), GeoControlError> {
    let start = parse_utc_day(&format!("{field}.start_utc_day"), &interval.start_utc_day)?;
    let end = parse_utc_day(&format!("{field}.end_utc_day"), &interval.end_utc_day)?;
    if start > end {
        return Err(GeoControlError::new(
            GeoControlErrorCode::InvalidAsOf,
            "Geo temporal interval start must not exceed end",
            [
                ("field", field.to_string()),
                ("start_utc_day", interval.start_utc_day.clone()),
                ("end_utc_day", interval.end_utc_day.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_as_of(field: &str, as_of: &GeoAsOf) -> Result<(), GeoControlError> {
    parse_utc_day(&format!("{field}.utc_day"), &as_of.utc_day)?;
    validate_identifier(&format!("{field}.semantic_id"), &as_of.semantic_id)?;
    validate_identifier(&format!("{field}.unit"), &as_of.unit)?;
    if as_of.unit != "utc_day" {
        return Err(GeoControlError::invalid(
            "Geo as-of values must declare the utc_day unit",
            [("field", field), ("unit", as_of.unit.as_str())],
        ));
    }
    Ok(())
}

fn parse_utc_day(field: &str, value: &str) -> Result<chrono::NaiveDate, GeoControlError> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|error| {
        GeoControlError::new(
            GeoControlErrorCode::InvalidAsOf,
            "Geo as-of dates must be valid YYYY-MM-DD UTC days",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("error", error.to_string()),
            ],
        )
    })
}

fn validate_geography(geography: &GeoBoundedGeography) -> Result<(), GeoControlError> {
    validate_identifier("geography.geography_id", &geography.geography_id)?;
    validate_identifier("geography.geography_kind", &geography.geography_kind)?;
    validate_text("geography.description", &geography.description)
}

fn validate_coverage(coverage: &GeoCoveragePredicate) -> Result<(), GeoControlError> {
    validate_identifier("coverage.coverage_id", &coverage.coverage_id)?;
    validate_geography(&coverage.region)?;
    validate_text("coverage.predicate", &coverage.predicate)
}

fn validate_local_state(state: &GeoLocalAcquisitionState) -> Result<(), GeoControlError> {
    match state.state {
        GeoSourceAvailability::Available | GeoSourceAvailability::Partial => {
            let Some(reference) = &state.local_ref else {
                return Err(GeoControlError::invalid(
                    "Available Geo source instances require a local artifact reference",
                    [("field", "local_state.local_ref")],
                ));
            };
            validate_identifier("local_state.local_ref.artifact_id", &reference.artifact_id)?;
            validate_identifier(
                "local_state.local_ref.contract_version",
                &reference.contract_version,
            )?;
            if !is_contract_version(&reference.contract_version) {
                return Err(GeoControlError::invalid(
                    "Geo local artifact contract versions must be canonical versioned Canon identifiers",
                    [
                        (
                            String::from("field"),
                            String::from("local_state.local_ref.contract_version"),
                        ),
                        (String::from("value"), reference.contract_version.clone()),
                    ],
                ));
            }
            validate_blake3_hash(
                "local_state.local_ref.content_hash",
                &reference.content_hash,
            )?;
            validate_identifier("local_state.local_ref.media_type", &reference.media_type)?;
        }
        GeoSourceAvailability::Missing
        | GeoSourceAvailability::DiscoveryRequired
        | GeoSourceAvailability::Unreadable => {
            if state.local_ref.is_some() {
                return Err(GeoControlError::invalid(
                    "Unavailable Geo source states must not carry a local artifact reference",
                    [("field", "local_state.local_ref")],
                ));
            }
        }
    }
    Ok(())
}

fn validate_discovery_gap(gap: &GeoDiscoveryGap) -> Result<(), GeoControlError> {
    validate_identifier("discovery_gaps[].gap_id", &gap.gap_id)?;
    validate_text("discovery_gaps[].reason", &gap.reason)?;
    validate_text("discovery_gaps[].next_command", &gap.next_command)
}

fn validate_numeric_bound(field: &str, bound: &GeoNumericBound) -> Result<(), GeoControlError> {
    validate_identifier(&format!("{field}.semantic_id"), &bound.semantic_id)?;
    validate_identifier(&format!("{field}.unit"), &bound.unit)
}

fn validate_numeric_measure(
    field: &str,
    measure: &GeoNumericMeasure,
) -> Result<(), GeoControlError> {
    validate_identifier(&format!("{field}.semantic_id"), &measure.semantic_id)?;
    validate_identifier(&format!("{field}.unit"), &measure.unit)
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoControlError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoControlError::invalid(
            "Geo control identifiers must be non-empty and have no surrounding whitespace",
            [
                (String::from("field"), field.to_string()),
                (String::from("value"), value.to_string()),
            ],
        ));
    }
    Ok(())
}

fn is_contract_version(value: &str) -> bool {
    if value.len() > 128 {
        return false;
    }
    let Some(rest) = value
        .strip_prefix("canon_")
        .or_else(|| value.strip_prefix("canon."))
    else {
        return false;
    };
    let Some((name, version)) = rest.rsplit_once(".v") else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        && !version.is_empty()
        && version.bytes().all(|byte| byte.is_ascii_digit())
}

fn validate_text(field: &str, value: &str) -> Result<(), GeoControlError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(GeoControlError::invalid(
            "Geo control text fields must be non-empty and have no surrounding whitespace",
            [
                (String::from("field"), field.to_string()),
                (String::from("value"), value.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_blake3_hash(field: &str, value: &str) -> Result<(), GeoControlError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoControlError::invalid(
            "Geo content hashes must use blake3: prefix",
            [
                (String::from("field"), field.to_string()),
                (String::from("value"), value.to_string()),
            ],
        ));
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Err(GeoControlError::invalid(
            "Geo content hashes must be lowercase BLAKE3 hex",
            [
                (String::from("field"), field.to_string()),
                (String::from("value"), value.to_string()),
            ],
        ));
    }
    Ok(())
}

fn sort_distinct<T: Ord + Clone>(field: &str, values: &mut [T]) -> Result<(), GeoControlError> {
    values.sort();
    let mut previous = None;
    for value in values.iter() {
        if previous.as_ref().is_some_and(|prev| prev == value) {
            return Err(GeoControlError::invalid(
                "Geo control collections must be sorted to distinct canonical values",
                [("field", field)],
            ));
        }
        previous = Some(value.clone());
    }
    Ok(())
}

fn reject_duplicate_keys(
    field: &str,
    keys: impl IntoIterator<Item = String>,
) -> Result<(), GeoControlError> {
    let mut seen = BTreeSet::new();
    for key in keys {
        if !seen.insert(key.clone()) {
            return Err(GeoControlError::invalid(
                "Geo control collection contains duplicate keys",
                [
                    (String::from("field"), field.to_string()),
                    (String::from("key"), key),
                ],
            ));
        }
    }
    Ok(())
}

fn serialize_canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, GeoControlError> {
    serde_json::to_vec(value).map_err(|error| {
        GeoControlError::new(
            GeoControlErrorCode::Serialization,
            "Geo control artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn contract(version: &str, schema_path: &str, notes: &str) -> GeoContractCapability {
    contract_with_status(
        version,
        schema_path,
        GeoCapabilityStatus::Implemented,
        notes,
    )
}

fn diagnostic_contract(version: &str, schema_path: &str, notes: &str) -> GeoContractCapability {
    contract_with_status(
        version,
        schema_path,
        GeoCapabilityStatus::DiagnosticOnly,
        notes,
    )
}

fn contract_with_status(
    version: &str,
    schema_path: &str,
    status: GeoCapabilityStatus,
    notes: &str,
) -> GeoContractCapability {
    GeoContractCapability {
        contract_version: version.to_string(),
        schema_path: schema_path.to_string(),
        status,
        notes: notes.to_string(),
    }
}

fn command(
    command: &str,
    surface: GeoCommandSurface,
    output_contract: &str,
    read_only: bool,
    uses_network: bool,
) -> GeoCommandCapability {
    GeoCommandCapability {
        command: command.to_string(),
        surface: Some(surface),
        output_contract: output_contract.to_string(),
        read_only,
        uses_network,
    }
}

fn scoped_property(
    property: GeoControlProperty,
    scope: &str,
    status: GeoCapabilityStatus,
    basis: &str,
) -> GeoScopedProperty {
    GeoScopedProperty {
        property,
        scope: scope.to_string(),
        status,
        basis: basis.to_string(),
    }
}

fn all_entity_levels() -> Vec<GeoControlEntityLevel> {
    vec![
        GeoControlEntityLevel::Address,
        GeoControlEntityLevel::Building,
        GeoControlEntityLevel::Parcel,
        GeoControlEntityLevel::Poi,
        GeoControlEntityLevel::Property,
        GeoControlEntityLevel::Site,
        GeoControlEntityLevel::Unit,
    ]
}

fn all_relations() -> Vec<GeoControlRelation> {
    vec![
        GeoControlRelation::Contains,
        GeoControlRelation::Fronts,
        GeoControlRelation::Intersects,
        GeoControlRelation::On,
        GeoControlRelation::PartOf,
        GeoControlRelation::SameAs,
        GeoControlRelation::Within,
    ]
}

fn all_evidence_classes() -> Vec<GeoEvidenceClass> {
    vec![
        GeoEvidenceClass::AddressSet,
        GeoEvidenceClass::AddressString,
        GeoEvidenceClass::AssertedAttribute,
        GeoEvidenceClass::BuildingFootprint,
        GeoEvidenceClass::EntityRelation,
        GeoEvidenceClass::GeocodePoint,
        GeoEvidenceClass::ParcelGeometry,
        GeoEvidenceClass::TemporalObservation,
    ]
}

fn all_claim_classes() -> Vec<GeoClaimClass> {
    vec![
        GeoClaimClass::AttributeBand,
        GeoClaimClass::CandidateReach,
        GeoClaimClass::CollateralComposition,
        GeoClaimClass::LifecycleState,
        GeoClaimClass::StableIdentity,
        GeoClaimClass::TemporalOccupancy,
    ]
}

fn all_properties() -> Vec<GeoControlProperty> {
    vec![
        GeoControlProperty::Canonical,
        GeoControlProperty::Complete,
        GeoControlProperty::Confluent,
        GeoControlProperty::Deterministic,
        GeoControlProperty::Sound,
    ]
}

fn entity_level_name(level: GeoControlEntityLevel) -> &'static str {
    match level {
        GeoControlEntityLevel::Site => "site",
        GeoControlEntityLevel::Property => "property",
        GeoControlEntityLevel::Parcel => "parcel",
        GeoControlEntityLevel::Building => "building",
        GeoControlEntityLevel::Unit => "unit",
        GeoControlEntityLevel::Address => "address",
        GeoControlEntityLevel::Poi => "poi",
    }
}

fn evidence_class_name(evidence_class: GeoEvidenceClass) -> &'static str {
    match evidence_class {
        GeoEvidenceClass::GeocodePoint => "geocode_point",
        GeoEvidenceClass::AddressString => "address_string",
        GeoEvidenceClass::AddressSet => "address_set",
        GeoEvidenceClass::ParcelGeometry => "parcel_geometry",
        GeoEvidenceClass::BuildingFootprint => "building_footprint",
        GeoEvidenceClass::AssertedAttribute => "asserted_attribute",
        GeoEvidenceClass::EntityRelation => "entity_relation",
        GeoEvidenceClass::TemporalObservation => "temporal_observation",
    }
}

fn telemetry_metric_name(metric: GeoTelemetryMetric) -> &'static str {
    match metric {
        GeoTelemetryMetric::WallTime => "wall_time",
        GeoTelemetryMetric::CpuTime => "cpu_time",
        GeoTelemetryMetric::PeakRssBytes => "peak_rss_bytes",
        GeoTelemetryMetric::CurrencyCost => "currency_cost",
    }
}
