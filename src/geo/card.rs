#![forbid(unsafe_code)]

//! Evidence-card artifact contracts.
//!
//! Canon emits data only. Presentation code and source acquisition stay
//! outside this module.

use super::{
    CANON_GEO_IMAGE_TILE_PIN_VERSION, GeoArtifactFieldClassification, GeoArtifactFieldLicenseClass,
    GeoCandidateReachStatus, GeoCompositionArtifact, GeoCompositionBackbone, GeoCompositionModel,
    GeoCompositionRequest, GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel,
    GeoEntityRef, GeoEvidenceAdmission, GeoEvidenceCompilationArtifact, GeoEvidenceRecordRef,
    GeoExplanationArtifact, GeoImageTilePin, GeoImageTilePinArtifact, GeoRedactedArtifact,
    GeoRedactionEgressPolicy, GeoSourceReleasePin, GeoTruthPlane, GeoTruthRepresentationGrain,
    GeoTypedGeometry, canonical_composition_bytes, canonical_evidence_compilation_bytes,
    canonical_explanation_bytes, canonical_geometry_bytes, canonicalize_composition_request,
    redact_geo_artifact_with_policy, validate_evidence_compilation_artifact,
    verify_image_tile_pin_replay,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_EVIDENCE_CARD_VERSION: &str = "canon_geo_evidence_card.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceCardProofClass {
    Fixture,
    RetainedArtifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceCardCoverageState {
    Covered,
    Partial,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoEvidenceCardCoverage {
    pub state: GeoEvidenceCardCoverageState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoEvidenceCardSubjectRef {
    pub accession: String,
    pub deal_id: String,
    pub loan_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deed_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoEvidenceCardCandidate {
    pub id: String,
    pub geometry_blake3: String,
    pub geometry: GeoTypedGeometry,
    pub in_halo: bool,
    pub evidence_observation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoEvidenceCardBuildContext {
    pub proof_class: GeoEvidenceCardProofClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<GeoEvidenceCardSubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth_plane: Option<GeoTruthPlane>,
    pub reach: GeoCandidateReachStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_none_reason: Option<String>,
    pub coverage: GeoEvidenceCardCoverage,
    pub answer_grain: GeoTruthRepresentationGrain,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_grain_caveat: Option<String>,
    pub geometry_source_pins: Vec<GeoSourceReleasePin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_cell: Option<String>,
    #[serde(default)]
    pub halo_members: Vec<GeoEntityRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_containment_cardinality: Option<u64>,
    pub field_classifications: Vec<GeoArtifactFieldClassification>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoEvidenceCard {
    pub version: String,
    pub proof_class: GeoEvidenceCardProofClass,
    pub subject_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<GeoEvidenceCardSubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth_plane: Option<GeoTruthPlane>,
    pub reach: GeoCandidateReachStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_none_reason: Option<String>,
    pub coverage: GeoEvidenceCardCoverage,
    pub answer_grain: GeoTruthRepresentationGrain,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_grain_caveat: Option<String>,
    pub ortho_pin: GeoImageTilePin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_cell: Option<String>,
    pub halo_members: Vec<GeoEntityRef>,
    pub candidate_parcels: Vec<GeoEvidenceCardCandidate>,
    pub candidate_buildings: Vec<GeoEvidenceCardCandidate>,
    pub forced: GeoCompositionBackbone,
    pub ambiguous_members: Vec<GeoEntityRef>,
    pub conflicting_records: Vec<GeoEvidenceRecordRef>,
    pub evidence_admissions: Vec<GeoEvidenceAdmission>,
    pub geometry_source_pins: Vec<GeoSourceReleasePin>,
    pub field_classifications: Vec<GeoArtifactFieldClassification>,
    pub composition_status: GeoCompositionStatus,
    pub residual_model_count: u64,
    pub count_exact: bool,
    pub backbone_complete: bool,
    pub multi_containment_cardinality: u64,
    pub composition_blake3: String,
    pub evidence_blake3: String,
    pub request_blake3: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation_blake3: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCardErrorCode {
    UnsupportedVersion,
    InvalidInput,
    ArithmeticOverflow,
    CardArtifactMismatch,
    TileReplayMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCardError {
    pub code: GeoCardErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoCardError {
    fn new(
        code: GeoCardErrorCode,
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
        Self::new(GeoCardErrorCode::InvalidInput, message, detail)
    }

    fn mismatch(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoCardErrorCode::CardArtifactMismatch, message, detail)
    }
}

impl fmt::Display for GeoCardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoCardError {}

impl GeoEvidenceCardBuildContext {
    pub fn fixture(
        geometry_source_pins: Vec<GeoSourceReleasePin>,
        field_classifications: Vec<GeoArtifactFieldClassification>,
    ) -> Self {
        Self {
            proof_class: GeoEvidenceCardProofClass::Fixture,
            subject_ref: None,
            truth_plane: None,
            reach: GeoCandidateReachStatus::Full,
            reach_none_reason: None,
            coverage: GeoEvidenceCardCoverage {
                state: GeoEvidenceCardCoverageState::Covered,
                reason: None,
            },
            answer_grain: GeoTruthRepresentationGrain::UnitLot,
            answer_grain_caveat: None,
            geometry_source_pins,
            home_cell: None,
            halo_members: Vec::new(),
            multi_containment_cardinality: None,
            field_classifications,
        }
    }
}

pub fn build_evidence_card(
    subject_id: &str,
    composition: &GeoCompositionArtifact,
    evidence: &GeoEvidenceCompilationArtifact,
    explanation: Option<&GeoExplanationArtifact>,
    ortho: &GeoImageTilePin,
    geometry: &BTreeMap<String, GeoTypedGeometry>,
) -> Result<GeoEvidenceCard, GeoCardError> {
    let geometry_source_pin = GeoSourceReleasePin {
        source_dataset: ortho.source_dataset.clone(),
        source_release: interval_release_token(&ortho.vintage),
        blake3: format!("blake3:{}", ortho.blake3),
    };
    let context = GeoEvidenceCardBuildContext::fixture(
        vec![geometry_source_pin],
        default_field_classifications(),
    );
    build_evidence_card_with_context(
        subject_id,
        composition,
        evidence,
        explanation,
        ortho,
        geometry,
        context,
    )
}

pub fn build_evidence_card_with_context(
    subject_id: &str,
    composition: &GeoCompositionArtifact,
    evidence: &GeoEvidenceCompilationArtifact,
    explanation: Option<&GeoExplanationArtifact>,
    ortho: &GeoImageTilePin,
    geometry: &BTreeMap<String, GeoTypedGeometry>,
    context: GeoEvidenceCardBuildContext,
) -> Result<GeoEvidenceCard, GeoCardError> {
    validate_text("subject_id", subject_id)?;
    if context.reach == GeoCandidateReachStatus::None {
        return Err(GeoCardError::invalid(
            "Geo evidence card reach-none artifacts must use the reach-none builder",
            [("field", "reach")],
        ));
    }
    validate_evidence_compilation_artifact(evidence).map_err(|error| {
        GeoCardError::mismatch(
            "Geo evidence card received an invalid evidence compilation artifact",
            [
                ("field".to_string(), "evidence".to_string()),
                ("evidence_error".to_string(), error.message),
            ],
        )
    })?;
    validate_composition_evidence_chain(composition, evidence)?;
    validate_context_for_reached_card(&context, subject_id, ortho)?;

    let request_blake3 = request_digest(&evidence.composition_request)?;
    let evidence_blake3 = evidence_digest(evidence)?;
    let composition_blake3 = composition_digest(composition)?;
    let explanation_blake3 = validate_optional_explanation(
        explanation,
        &request_blake3,
        &evidence_blake3,
        composition.status,
    )?;
    let evidence_by_member = evidence_observation_ids_by_member(evidence);
    let halo_members = sorted_unique(context.halo_members.clone());
    let halo_set = halo_members.iter().cloned().collect::<BTreeSet<_>>();
    let candidate_parcels = candidate_geometries(
        GeoEntityLevel::Parcel,
        &evidence.composition_request.universe.parcels,
        geometry,
        &halo_set,
        &evidence_by_member,
    )?;
    let building_ids = evidence
        .composition_request
        .universe
        .buildings
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let candidate_buildings = candidate_geometries(
        GeoEntityLevel::Building,
        &building_ids,
        geometry,
        &halo_set,
        &evidence_by_member,
    )?;
    let ambiguous_members = ambiguous_members(composition, &evidence.composition_request.universe);
    let conflicting_records = explanation
        .map(conflicting_source_records)
        .unwrap_or_default();
    let multi_containment_cardinality = context
        .multi_containment_cardinality
        .unwrap_or(candidate_count(&candidate_parcels)?);

    let card = GeoEvidenceCard {
        version: CANON_GEO_EVIDENCE_CARD_VERSION.to_string(),
        proof_class: context.proof_class,
        subject_id: subject_id.to_string(),
        subject_ref: context.subject_ref,
        truth_plane: context.truth_plane,
        reach: context.reach,
        reach_none_reason: context.reach_none_reason,
        coverage: context.coverage,
        answer_grain: context.answer_grain,
        answer_grain_caveat: context.answer_grain_caveat,
        ortho_pin: ortho.clone(),
        home_cell: context.home_cell,
        halo_members,
        candidate_parcels,
        candidate_buildings,
        forced: composition.hard_forced.clone(),
        ambiguous_members,
        conflicting_records,
        evidence_admissions: evidence.admissions.clone(),
        geometry_source_pins: sorted_unique(context.geometry_source_pins),
        field_classifications: sorted_unique(context.field_classifications),
        composition_status: composition.status,
        residual_model_count: composition.summary.residual_model_count,
        count_exact: composition.summary.residual_model_count_complete
            && !composition.summary.residual_model_count_saturated,
        backbone_complete: composition.backbone_complete,
        multi_containment_cardinality,
        composition_blake3,
        evidence_blake3,
        request_blake3,
        explanation_blake3,
    };
    validate_evidence_card_artifact(&card)?;
    Ok(card)
}

pub fn build_reach_none_evidence_card(
    subject_id: &str,
    ortho: &GeoImageTilePin,
    context: GeoEvidenceCardBuildContext,
) -> Result<GeoEvidenceCard, GeoCardError> {
    validate_text("subject_id", subject_id)?;
    validate_context_for_reach_none_card(&context, subject_id, ortho)?;
    let card = GeoEvidenceCard {
        version: CANON_GEO_EVIDENCE_CARD_VERSION.to_string(),
        proof_class: context.proof_class,
        subject_id: subject_id.to_string(),
        subject_ref: context.subject_ref,
        truth_plane: context.truth_plane,
        reach: context.reach,
        reach_none_reason: context.reach_none_reason,
        coverage: context.coverage,
        answer_grain: context.answer_grain,
        answer_grain_caveat: context.answer_grain_caveat,
        ortho_pin: ortho.clone(),
        home_cell: context.home_cell,
        halo_members: sorted_unique(context.halo_members),
        candidate_parcels: Vec::new(),
        candidate_buildings: Vec::new(),
        forced: GeoCompositionBackbone {
            parcels: Vec::new(),
            buildings: Vec::new(),
        },
        ambiguous_members: Vec::new(),
        conflicting_records: Vec::new(),
        evidence_admissions: Vec::new(),
        geometry_source_pins: sorted_unique(context.geometry_source_pins),
        field_classifications: sorted_unique(context.field_classifications),
        composition_status: GeoCompositionStatus::Ambiguous,
        residual_model_count: 0,
        count_exact: true,
        backbone_complete: false,
        multi_containment_cardinality: 0,
        composition_blake3: String::new(),
        evidence_blake3: String::new(),
        request_blake3: String::new(),
        explanation_blake3: None,
    };
    validate_evidence_card_artifact(&card)?;
    Ok(card)
}

pub fn validate_evidence_card_artifact(card: &GeoEvidenceCard) -> Result<(), GeoCardError> {
    if card.version != CANON_GEO_EVIDENCE_CARD_VERSION {
        return Err(GeoCardError::new(
            GeoCardErrorCode::UnsupportedVersion,
            "Unsupported Geo evidence card artifact version",
            [
                ("actual", card.version.as_str()),
                ("expected", CANON_GEO_EVIDENCE_CARD_VERSION),
            ],
        ));
    }
    validate_text("subject_id", &card.subject_id)?;
    if let Some(subject) = &card.subject_ref {
        validate_subject_ref(subject)?;
    }
    validate_tile_pin_for_proof_class(&card.ortho_pin, card.proof_class)?;
    validate_source_release_pins(&card.geometry_source_pins, card.proof_class)?;
    validate_field_classifications(&card.field_classifications)?;
    validate_reach_state(card)?;
    validate_answer_grain(card)?;
    validate_sorted_entity_refs("halo_members", &card.halo_members)?;
    validate_candidates(
        "candidate_parcels",
        GeoEntityLevel::Parcel,
        &card.candidate_parcels,
    )?;
    validate_candidates(
        "candidate_buildings",
        GeoEntityLevel::Building,
        &card.candidate_buildings,
    )?;
    validate_forced_members(card)?;
    validate_ambiguous_members(card)?;
    validate_source_record_refs(&card.conflicting_records)?;
    validate_evidence_admissions(&card.evidence_admissions)?;
    validate_digest_shape_for_reach("composition_blake3", &card.composition_blake3, card.reach)?;
    validate_digest_shape_for_reach("evidence_blake3", &card.evidence_blake3, card.reach)?;
    validate_digest_shape_for_reach("request_blake3", &card.request_blake3, card.reach)?;
    if let Some(explanation) = &card.explanation_blake3 {
        validate_prefixed_blake3("explanation_blake3", explanation)?;
    }
    if card.composition_status == GeoCompositionStatus::Conflict
        && card.explanation_blake3.is_none()
    {
        return Err(GeoCardError::mismatch(
            "Geo evidence card conflict state requires a chained explanation artifact",
            [("field", "explanation_blake3")],
        ));
    }
    Ok(())
}

pub fn canonical_evidence_card_bytes(card: &GeoEvidenceCard) -> Result<Vec<u8>, GeoCardError> {
    validate_evidence_card_artifact(card)?;
    serde_json::to_vec(card).map_err(|error| {
        GeoCardError::invalid(
            "Geo evidence card artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn redact_evidence_card_artifact(
    card: &GeoEvidenceCard,
) -> Result<GeoRedactedArtifact, GeoCardError> {
    redact_evidence_card_artifact_with_policy(card, GeoRedactionEgressPolicy::shareable())
}

pub fn redact_evidence_card_artifact_with_policy(
    card: &GeoEvidenceCard,
    policy: GeoRedactionEgressPolicy,
) -> Result<GeoRedactedArtifact, GeoCardError> {
    validate_evidence_card_artifact(card)?;
    let artifact = serde_json::to_value(card).map_err(|error| {
        GeoCardError::invalid(
            "Geo evidence card artifact could not be serialized before redaction",
            [("serde_error", error.to_string())],
        )
    })?;
    redact_geo_artifact_with_policy(
        &card.version,
        &artifact,
        &classify_evidence_card_artifact_fields(card),
        policy,
    )
    .map_err(|error| {
        GeoCardError::invalid(
            "Geo evidence card redaction failed",
            [
                ("geo_error_code".to_string(), format!("{:?}", error.code)),
                ("geo_error".to_string(), error.message),
            ],
        )
    })
}

pub fn classify_evidence_card_artifact_fields(
    card: &GeoEvidenceCard,
) -> Vec<GeoArtifactFieldClassification> {
    let mut classifications = vec![
        field_classification(
            "$.version",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "evidence card contract identifier",
        ),
        field_classification(
            "$.proof_class",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "proof-class label must survive redaction",
        ),
        field_classification(
            "$.subject_id",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "subject identifier links the redacted projection to retained full artifacts",
        ),
        field_classification(
            "$.subject_ref",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "loan and deal references identify retained artifact lineage",
        ),
        field_classification(
            "$.truth_plane",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "truth-plane label states the evaluation boundary",
        ),
        field_classification(
            "$.reach",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "reach state is a shareable control-plane result",
        ),
        field_classification(
            "$.reach_none_reason",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "reach-none reason is a shareable abstention reason",
        ),
        field_classification(
            "$.coverage",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "coverage state is shareable and distinct from geometry",
        ),
        field_classification(
            "$.answer_grain",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "answer grain is required so visual claims do not silently change meaning",
        ),
        field_classification(
            "$.answer_grain_caveat",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "answer-grain caveat must survive redaction",
        ),
        field_classification(
            "$.ortho_pin",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "image tile pin is content-addressed for replay from retained bytes",
        ),
        field_classification(
            "$.home_cell",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "home H3 cell is a blocking index, not geometry truth",
        ),
        field_classification(
            "$.halo_members",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "halo membership explains local ownership without raw geometry",
        ),
        field_classification(
            "$.forced",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "forced backbone is the rendered answer state",
        ),
        field_classification(
            "$.ambiguous_members",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "ambiguous member ids preserve tie topology",
        ),
        field_classification(
            "$.conflicting_records",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "conflict source-record references link retained evidence",
        ),
        field_classification(
            "$.evidence_admissions",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "typed admitted evidence explains the decision without raw geometry",
        ),
        field_classification(
            "$.geometry_source_pins",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "geometry pins identify retained bytes and prevent live re-query",
        ),
        field_classification(
            "$.field_classifications",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "embedded field classifications let clients inspect the egress boundary",
        ),
        field_classification(
            "$.composition_status",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "decision state",
        ),
        field_classification(
            "$.residual_model_count",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "residual model count is a non-reconstructive solver diagnostic",
        ),
        field_classification(
            "$.count_exact",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "residual count exactness flag is shareable",
        ),
        field_classification(
            "$.backbone_complete",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "backbone completeness flag is shareable",
        ),
        field_classification(
            "$.multi_containment_cardinality",
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            "multi-containment count is not enough to reconstruct geometry by itself",
        ),
        field_classification(
            "$.composition_blake3",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "composition digest binds the card to retained solve state",
        ),
        field_classification(
            "$.evidence_blake3",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "evidence digest binds the card to retained evidence state",
        ),
        field_classification(
            "$.request_blake3",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "request digest binds the card to retained input state",
        ),
        field_classification(
            "$.explanation_blake3",
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            "explanation digest binds the card to retained explanation state",
        ),
    ];
    classifications.extend(candidate_field_classifications(
        "$.candidate_parcels",
        !card.candidate_parcels.is_empty(),
        "parcel",
    ));
    classifications.extend(candidate_field_classifications(
        "$.candidate_buildings",
        !card.candidate_buildings.is_empty(),
        "building",
    ));
    sorted_unique(classifications)
}

fn candidate_field_classifications(
    path: &'static str,
    has_candidates: bool,
    grain: &'static str,
) -> Vec<GeoArtifactFieldClassification> {
    if !has_candidates {
        return vec![field_classification(
            path,
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            format!("empty {grain} candidate container"),
        )];
    }
    vec![
        field_classification(
            format!("{path}[].id"),
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            format!("{grain} candidate identifier"),
        ),
        field_classification(
            format!("{path}[].geometry_blake3"),
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            format!("{grain} candidate geometry digest"),
        ),
        field_classification(
            format!("{path}[].geometry"),
            GeoArtifactFieldLicenseClass::EncumberedGeometry,
            true,
            format!("{grain} candidate geometry is reconstructive"),
        ),
        field_classification(
            format!("{path}[].in_halo"),
            GeoArtifactFieldLicenseClass::Shareable,
            false,
            format!("{grain} candidate halo ownership flag"),
        ),
        field_classification(
            format!("{path}[].evidence_observation_ids"),
            GeoArtifactFieldLicenseClass::InternalDigestLink,
            false,
            format!("{grain} candidate evidence-observation ids"),
        ),
    ]
}

pub fn verify_evidence_card_tile_replay(
    card: &GeoEvidenceCard,
    bytes_by_blake3: &BTreeMap<String, Vec<u8>>,
) -> Result<(), GeoCardError> {
    validate_evidence_card_artifact(card)?;
    let artifact = GeoImageTilePinArtifact {
        version: CANON_GEO_IMAGE_TILE_PIN_VERSION.to_string(),
        source_profile_id: card.ortho_pin.source_dataset.clone(),
        rows: vec![card.ortho_pin.clone()],
    };
    verify_image_tile_pin_replay(&artifact, bytes_by_blake3).map_err(|error| {
        GeoCardError::new(
            GeoCardErrorCode::TileReplayMismatch,
            "Geo evidence card retained tile replay failed",
            error.detail,
        )
    })
}

fn validate_composition_evidence_chain(
    composition: &GeoCompositionArtifact,
    evidence: &GeoEvidenceCompilationArtifact,
) -> Result<(), GeoCardError> {
    if composition.request_version != evidence.composition_request.version {
        return Err(GeoCardError::mismatch(
            "Geo evidence card composition/evidence request-version chain mismatch",
            [
                (
                    "composition_request_version",
                    composition.request_version.as_str(),
                ),
                (
                    "evidence_composition_request_version",
                    evidence.composition_request.version.as_str(),
                ),
            ],
        ));
    }
    let evidence_hex = evidence_digest_hex(evidence)?;
    let evidence_blake3 = format!("blake3:{evidence_hex}");
    let reference = composition.evidence_compilation.as_ref().ok_or_else(|| {
        GeoCardError::mismatch(
            "Geo evidence card composition artifact does not bind an evidence compilation",
            [
                ("field".to_string(), "evidence_compilation".to_string()),
                ("evidence_blake3".to_string(), evidence_blake3.clone()),
            ],
        )
    })?;
    if reference.version != evidence.version {
        return Err(GeoCardError::mismatch(
            "Geo evidence card composition/evidence version chain mismatch",
            [
                (
                    "field".to_string(),
                    "evidence_compilation.version".to_string(),
                ),
                ("expected".to_string(), evidence.version.clone()),
                ("actual".to_string(), reference.version.clone()),
                ("evidence_blake3".to_string(), evidence_blake3),
            ],
        ));
    }
    if reference.request_version != evidence.request_version {
        return Err(GeoCardError::mismatch(
            "Geo evidence card composition/evidence request-version chain mismatch",
            [
                (
                    "field".to_string(),
                    "evidence_compilation.request_version".to_string(),
                ),
                ("expected".to_string(), evidence.request_version.clone()),
                ("actual".to_string(), reference.request_version.clone()),
                ("evidence_blake3".to_string(), evidence_blake3),
            ],
        ));
    }
    if reference.blake3 != evidence_hex {
        return Err(GeoCardError::mismatch(
            "Geo evidence card composition/evidence digest chain mismatch",
            [
                (
                    "field".to_string(),
                    "evidence_compilation.blake3".to_string(),
                ),
                (
                    "composition_evidence_blake3".to_string(),
                    format!("blake3:{}", reference.blake3),
                ),
                ("evidence_blake3".to_string(), evidence_blake3),
            ],
        ));
    }
    Ok(())
}

fn validate_optional_explanation(
    explanation: Option<&GeoExplanationArtifact>,
    request_blake3: &str,
    evidence_blake3: &str,
    composition_status: GeoCompositionStatus,
) -> Result<Option<String>, GeoCardError> {
    match explanation {
        Some(explanation) => {
            let bytes = canonical_explanation_bytes(explanation).map_err(|error| {
                GeoCardError::mismatch(
                    "Geo evidence card received an invalid explanation artifact",
                    [
                        ("field".to_string(), "explanation".to_string()),
                        ("explanation_error".to_string(), error.message),
                    ],
                )
            })?;
            if explanation.request_blake3 != request_blake3 {
                return Err(GeoCardError::mismatch(
                    "Geo evidence card explanation request digest mismatch",
                    [
                        ("field", "request_blake3"),
                        ("expected", request_blake3),
                        ("actual", explanation.request_blake3.as_str()),
                    ],
                ));
            }
            if explanation.evidence_blake3 != evidence_blake3 {
                return Err(GeoCardError::mismatch(
                    "Geo evidence card explanation evidence digest mismatch",
                    [
                        ("field", "evidence_blake3"),
                        ("expected", evidence_blake3),
                        ("actual", explanation.evidence_blake3.as_str()),
                    ],
                ));
            }
            if composition_status != GeoCompositionStatus::Conflict
                && !explanation
                    .counters
                    .get("not_conflict")
                    .is_some_and(|count| *count == 1)
            {
                return Err(GeoCardError::mismatch(
                    "Geo evidence card non-conflict explanations must be explicit not-conflict artifacts",
                    [("field", "explanation")],
                ));
            }
            Ok(Some(format!("blake3:{}", blake3::hash(&bytes).to_hex())))
        }
        None if composition_status == GeoCompositionStatus::Conflict => {
            Err(GeoCardError::mismatch(
                "Geo evidence card conflict state requires a chained explanation artifact",
                [("field", "explanation")],
            ))
        }
        None => Ok(None),
    }
}

fn candidate_geometries(
    level: GeoEntityLevel,
    ids: &[String],
    geometry: &BTreeMap<String, GeoTypedGeometry>,
    halo_members: &BTreeSet<GeoEntityRef>,
    evidence_by_member: &BTreeMap<GeoEntityRef, Vec<String>>,
) -> Result<Vec<GeoEvidenceCardCandidate>, GeoCardError> {
    let mut candidates = Vec::with_capacity(ids.len());
    for id in sorted_unique(ids.to_vec()) {
        let value = geometry.get(&id).ok_or_else(|| {
            GeoCardError::mismatch(
                "Geo evidence card candidate is missing retained geometry",
                [("member", id.as_str()), ("level", level_name(level))],
            )
        })?;
        let bytes = canonical_geometry_bytes(value).map_err(|error| {
            GeoCardError::invalid(
                "Geo evidence card geometry could not be serialized",
                [("serde_error", error.to_string())],
            )
        })?;
        let member = GeoEntityRef::new(level, &id);
        candidates.push(GeoEvidenceCardCandidate {
            id,
            geometry_blake3: format!("blake3:{}", blake3::hash(&bytes).to_hex()),
            geometry: value.clone(),
            in_halo: halo_members.contains(&member),
            evidence_observation_ids: evidence_by_member.get(&member).cloned().unwrap_or_default(),
        });
    }
    Ok(candidates)
}

fn evidence_observation_ids_by_member(
    evidence: &GeoEvidenceCompilationArtifact,
) -> BTreeMap<GeoEntityRef, Vec<String>> {
    let mut by_member = BTreeMap::<GeoEntityRef, BTreeSet<String>>::new();
    for admission in &evidence.admissions {
        for member in observation_members(&admission.observation) {
            by_member
                .entry(member)
                .or_default()
                .insert(admission.observation_id.clone());
        }
    }
    by_member
        .into_iter()
        .map(|(member, ids)| (member, ids.into_iter().collect()))
        .collect()
}

fn observation_members(observation: &super::GeoRhoObservationKind) -> Vec<GeoEntityRef> {
    match observation {
        super::GeoRhoObservationKind::ExactSets { level, sets } => sorted_unique(
            sets.iter()
                .flatten()
                .map(|id| GeoEntityRef::new(*level, id))
                .collect(),
        ),
        super::GeoRhoObservationKind::ExistentialMembership { members } => {
            sorted_unique(members.clone())
        }
        super::GeoRhoObservationKind::AllOf { members } => sorted_unique(members.clone()),
        super::GeoRhoObservationKind::ExactCardinality { .. } => Vec::new(),
        super::GeoRhoObservationKind::IntegerSumBand { level, values, .. } => sorted_unique(
            values
                .iter()
                .map(|value| GeoEntityRef::new(*level, &value.id))
                .collect(),
        ),
        super::GeoRhoObservationKind::PreferMember { member, .. } => vec![member.clone()],
    }
}

fn ambiguous_members(
    composition: &GeoCompositionArtifact,
    universe: &GeoCompositionUniverse,
) -> Vec<GeoEntityRef> {
    let forced = composition
        .hard_forced
        .parcels
        .iter()
        .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id))
        .chain(
            composition
                .hard_forced
                .buildings
                .iter()
                .map(|id| GeoEntityRef::new(GeoEntityLevel::Building, id)),
        )
        .collect::<BTreeSet<_>>();
    let mut members = model_members(&composition.residual_models)
        .difference(&forced)
        .cloned()
        .collect::<Vec<_>>();
    if members.is_empty() && composition.status == GeoCompositionStatus::Ambiguous {
        members = universe_members(universe)
            .difference(&forced)
            .cloned()
            .collect();
    }
    sorted_unique(members)
}

fn model_members(models: &[GeoCompositionModel]) -> BTreeSet<GeoEntityRef> {
    models
        .iter()
        .flat_map(|model| {
            model
                .parcels
                .iter()
                .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id))
                .chain(
                    model
                        .buildings
                        .iter()
                        .map(|id| GeoEntityRef::new(GeoEntityLevel::Building, id)),
                )
        })
        .collect()
}

fn universe_members(universe: &GeoCompositionUniverse) -> BTreeSet<GeoEntityRef> {
    universe
        .parcels
        .iter()
        .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id))
        .chain(
            universe
                .buildings
                .iter()
                .map(|candidate| GeoEntityRef::new(GeoEntityLevel::Building, &candidate.id)),
        )
        .collect()
}

fn conflicting_source_records(explanation: &GeoExplanationArtifact) -> Vec<GeoEvidenceRecordRef> {
    sorted_unique(
        explanation
            .cores
            .iter()
            .flat_map(|core| core.source_record_refs.iter().cloned())
            .collect(),
    )
}

fn validate_context_for_reached_card(
    context: &GeoEvidenceCardBuildContext,
    subject_id: &str,
    ortho: &GeoImageTilePin,
) -> Result<(), GeoCardError> {
    validate_context_common(context, subject_id, ortho)?;
    if context.coverage.state == GeoEvidenceCardCoverageState::Absent {
        return Err(GeoCardError::invalid(
            "Geo evidence card reached artifacts cannot declare absent coverage",
            [("field", "coverage.state")],
        ));
    }
    if context
        .reach_none_reason
        .as_deref()
        .is_some_and(|reason| !reason.trim().is_empty())
    {
        return Err(GeoCardError::invalid(
            "Geo evidence card reach_none_reason is only valid for reach-none cards",
            [("field", "reach_none_reason")],
        ));
    }
    Ok(())
}

fn validate_context_for_reach_none_card(
    context: &GeoEvidenceCardBuildContext,
    subject_id: &str,
    ortho: &GeoImageTilePin,
) -> Result<(), GeoCardError> {
    validate_context_common(context, subject_id, ortho)?;
    if context.reach != GeoCandidateReachStatus::None {
        return Err(GeoCardError::invalid(
            "Geo evidence card reach-none builder requires reach none",
            [("field", "reach")],
        ));
    }
    normalize_required_reason(context.reach_none_reason.as_deref())?;
    if context.coverage.state != GeoEvidenceCardCoverageState::Absent {
        return Err(GeoCardError::invalid(
            "Geo evidence card reach-none coverage must be absent",
            [("field", "coverage.state")],
        ));
    }
    normalize_required_reason(context.coverage.reason.as_deref())?;
    Ok(())
}

fn validate_context_common(
    context: &GeoEvidenceCardBuildContext,
    subject_id: &str,
    ortho: &GeoImageTilePin,
) -> Result<(), GeoCardError> {
    validate_text("subject_id", subject_id)?;
    validate_tile_pin_for_proof_class(ortho, context.proof_class)?;
    if let Some(subject) = &context.subject_ref {
        validate_subject_ref(subject)?;
    }
    if let Some(home_cell) = &context.home_cell {
        validate_text("home_cell", home_cell)?;
    }
    validate_sorted_entity_refs("halo_members", &sorted_unique(context.halo_members.clone()))?;
    validate_source_release_pins(&context.geometry_source_pins, context.proof_class)?;
    validate_field_classifications(&context.field_classifications)?;
    validate_answer_grain_fields(context.answer_grain, context.answer_grain_caveat.as_deref())
}

fn validate_reach_state(card: &GeoEvidenceCard) -> Result<(), GeoCardError> {
    match card.reach {
        GeoCandidateReachStatus::Full | GeoCandidateReachStatus::Partial => {
            if card.candidate_parcels.is_empty() && card.candidate_buildings.is_empty() {
                return Err(GeoCardError::invalid(
                    "Geo evidence card with candidate reach requires candidate geometry",
                    [("field", "candidate_parcels")],
                ));
            }
            if card
                .reach_none_reason
                .as_deref()
                .is_some_and(|reason| !reason.trim().is_empty())
            {
                return Err(GeoCardError::invalid(
                    "Geo evidence card reach_none_reason is only valid when reach is none",
                    [("field", "reach_none_reason")],
                ));
            }
            validate_prefixed_blake3("composition_blake3", &card.composition_blake3)?;
            validate_prefixed_blake3("evidence_blake3", &card.evidence_blake3)?;
            validate_prefixed_blake3("request_blake3", &card.request_blake3)?;
        }
        GeoCandidateReachStatus::None => {
            normalize_required_reason(card.reach_none_reason.as_deref())?;
            if !card.candidate_parcels.is_empty()
                || !card.candidate_buildings.is_empty()
                || !card.forced.parcels.is_empty()
                || !card.forced.buildings.is_empty()
                || !card.ambiguous_members.is_empty()
                || !card.conflicting_records.is_empty()
                || !card.evidence_admissions.is_empty()
            {
                return Err(GeoCardError::invalid(
                    "Geo evidence card reach-none state cannot carry fabricated candidates or evidence",
                    [("field", "reach")],
                ));
            }
            for (field, value) in [
                ("composition_blake3", card.composition_blake3.as_str()),
                ("evidence_blake3", card.evidence_blake3.as_str()),
                ("request_blake3", card.request_blake3.as_str()),
            ] {
                if !value.is_empty() {
                    return Err(GeoCardError::invalid(
                        "Geo evidence card reach-none digest fields must be empty",
                        [("field", field)],
                    ));
                }
            }
        }
    }
    if card.coverage.state == GeoEvidenceCardCoverageState::Absent
        && card
            .coverage
            .reason
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(GeoCardError::invalid(
            "Geo evidence card absent coverage requires a reason",
            [("field", "coverage.reason")],
        ));
    }
    Ok(())
}

fn validate_answer_grain(card: &GeoEvidenceCard) -> Result<(), GeoCardError> {
    validate_answer_grain_fields(card.answer_grain, card.answer_grain_caveat.as_deref())
}

fn validate_answer_grain_fields(
    answer_grain: GeoTruthRepresentationGrain,
    caveat: Option<&str>,
) -> Result<(), GeoCardError> {
    if answer_grain == GeoTruthRepresentationGrain::BillingLot {
        normalize_required_reason(caveat)?;
    }
    Ok(())
}

fn validate_candidates(
    field: &'static str,
    level: GeoEntityLevel,
    candidates: &[GeoEvidenceCardCandidate],
) -> Result<(), GeoCardError> {
    let mut previous: Option<&str> = None;
    for candidate in candidates {
        validate_text(field, &candidate.id)?;
        if previous.is_some_and(|previous| previous >= candidate.id.as_str()) {
            return Err(GeoCardError::invalid(
                "Geo evidence card candidate vectors must be strictly sorted and unique",
                [("field", field), ("candidate", candidate.id.as_str())],
            ));
        }
        previous = Some(&candidate.id);
        validate_prefixed_blake3("candidate.geometry_blake3", &candidate.geometry_blake3)?;
        let bytes = canonical_geometry_bytes(&candidate.geometry).map_err(|error| {
            GeoCardError::invalid(
                "Geo evidence card geometry could not be serialized",
                [("serde_error", error.to_string())],
            )
        })?;
        let actual = format!("blake3:{}", blake3::hash(&bytes).to_hex());
        if actual != candidate.geometry_blake3 {
            return Err(GeoCardError::mismatch(
                "Geo evidence card candidate geometry digest mismatch",
                [
                    ("field".to_string(), "geometry_blake3".to_string()),
                    ("level".to_string(), level_name(level).to_string()),
                    ("candidate".to_string(), candidate.id.clone()),
                    ("expected".to_string(), actual),
                    ("actual".to_string(), candidate.geometry_blake3.clone()),
                ],
            ));
        }
        validate_sorted_ids(
            "candidate.evidence_observation_ids",
            &candidate.evidence_observation_ids,
        )?;
    }
    Ok(())
}

fn validate_forced_members(card: &GeoEvidenceCard) -> Result<(), GeoCardError> {
    validate_sorted_ids("forced.parcels", &card.forced.parcels)?;
    validate_sorted_ids("forced.buildings", &card.forced.buildings)?;
    let parcel_ids = card
        .candidate_parcels
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<BTreeSet<_>>();
    for parcel in &card.forced.parcels {
        if !parcel_ids.contains(parcel.as_str()) {
            return Err(GeoCardError::mismatch(
                "Geo evidence card forced parcel is absent from retained candidates",
                [("member", parcel.as_str()), ("field", "candidate_parcels")],
            ));
        }
    }
    let building_ids = card
        .candidate_buildings
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<BTreeSet<_>>();
    for building in &card.forced.buildings {
        if !building_ids.contains(building.as_str()) {
            return Err(GeoCardError::mismatch(
                "Geo evidence card forced building is absent from retained candidates",
                [
                    ("member", building.as_str()),
                    ("field", "candidate_buildings"),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_ambiguous_members(card: &GeoEvidenceCard) -> Result<(), GeoCardError> {
    validate_sorted_entity_refs("ambiguous_members", &card.ambiguous_members)?;
    let forced = card
        .forced
        .parcels
        .iter()
        .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id))
        .chain(
            card.forced
                .buildings
                .iter()
                .map(|id| GeoEntityRef::new(GeoEntityLevel::Building, id)),
        )
        .collect::<BTreeSet<_>>();
    let candidate_members = card
        .candidate_parcels
        .iter()
        .map(|candidate| GeoEntityRef::new(GeoEntityLevel::Parcel, &candidate.id))
        .chain(
            card.candidate_buildings
                .iter()
                .map(|candidate| GeoEntityRef::new(GeoEntityLevel::Building, &candidate.id)),
        )
        .collect::<BTreeSet<_>>();
    for member in &card.ambiguous_members {
        if forced.contains(member) {
            return Err(GeoCardError::mismatch(
                "Geo evidence card cannot classify a forced member as ambiguous",
                [
                    ("member", member.id.as_str()),
                    ("level", level_name(member.level)),
                ],
            ));
        }
        if !candidate_members.contains(member) {
            return Err(GeoCardError::mismatch(
                "Geo evidence card ambiguous member is absent from retained candidates",
                [
                    ("member", member.id.as_str()),
                    ("level", level_name(member.level)),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_evidence_admissions(admissions: &[GeoEvidenceAdmission]) -> Result<(), GeoCardError> {
    let mut previous: Option<&str> = None;
    for admission in admissions {
        validate_text(
            "evidence_admissions[].observation_id",
            &admission.observation_id,
        )?;
        if previous.is_some_and(|previous| previous >= admission.observation_id.as_str()) {
            return Err(GeoCardError::invalid(
                "Geo evidence card admissions must be strictly sorted by observation id",
                [("field", "evidence_admissions")],
            ));
        }
        previous = Some(&admission.observation_id);
        validate_source_record_refs(&admission.source_records)?;
    }
    Ok(())
}

fn validate_source_record_refs(refs: &[GeoEvidenceRecordRef]) -> Result<(), GeoCardError> {
    let mut previous: Option<&GeoEvidenceRecordRef> = None;
    for reference in refs {
        validate_text("source_record_id", &reference.source_record_id)?;
        validate_text("source_vintage", &reference.source_vintage)?;
        validate_hex_blake3("record_blake3", &reference.record_blake3)?;
        if previous.is_some_and(|previous| previous >= reference) {
            return Err(GeoCardError::invalid(
                "Geo evidence card source record refs must be strictly sorted",
                [("field", "source_record_refs")],
            ));
        }
        previous = Some(reference);
    }
    Ok(())
}

fn validate_subject_ref(subject: &GeoEvidenceCardSubjectRef) -> Result<(), GeoCardError> {
    validate_text("subject_ref.accession", &subject.accession)?;
    validate_text("subject_ref.deal_id", &subject.deal_id)?;
    validate_text("subject_ref.loan_id", &subject.loan_id)?;
    validate_sorted_ids("subject_ref.deed_ids", &subject.deed_ids)
}

fn validate_tile_pin_for_proof_class(
    pin: &GeoImageTilePin,
    proof_class: GeoEvidenceCardProofClass,
) -> Result<(), GeoCardError> {
    validate_text("ortho_pin.url", &pin.url)?;
    validate_hex_blake3("ortho_pin.blake3", &pin.blake3)?;
    validate_hex_blake3("ortho_pin.license_text_blake3", &pin.license_text_blake3)?;
    validate_text("ortho_pin.license_id", &pin.license_id)?;
    validate_text("ortho_pin.source_dataset", &pin.source_dataset)?;
    if pin.vintage.start_day > pin.vintage.end_day {
        return Err(GeoCardError::invalid(
            "Geo evidence card tile pin vintage interval is inverted",
            [("field", "ortho_pin.vintage")],
        ));
    }
    match proof_class {
        GeoEvidenceCardProofClass::Fixture if !pin.source_dataset.starts_with("fixture.") => Err(
            proof_class_error("ortho_pin.source_dataset", &pin.source_dataset),
        ),
        GeoEvidenceCardProofClass::RetainedArtifact
            if pin.source_dataset.starts_with("fixture.") =>
        {
            Err(proof_class_error(
                "ortho_pin.source_dataset",
                &pin.source_dataset,
            ))
        }
        GeoEvidenceCardProofClass::Fixture | GeoEvidenceCardProofClass::RetainedArtifact => Ok(()),
    }
}

fn validate_source_release_pins(
    pins: &[GeoSourceReleasePin],
    proof_class: GeoEvidenceCardProofClass,
) -> Result<(), GeoCardError> {
    if pins.is_empty() {
        return Err(GeoCardError::invalid(
            "Geo evidence card requires at least one geometry source pin",
            [("field", "geometry_source_pins")],
        ));
    }
    let mut previous: Option<(&str, &str)> = None;
    for pin in pins {
        validate_text("geometry_source_pins[].source_dataset", &pin.source_dataset)?;
        validate_text("geometry_source_pins[].source_release", &pin.source_release)?;
        validate_prefixed_blake3("geometry_source_pins[].blake3", &pin.blake3)?;
        let key = (pin.source_dataset.as_str(), pin.source_release.as_str());
        if previous.is_some_and(|previous| previous >= key) {
            return Err(GeoCardError::invalid(
                "Geo evidence card geometry source pins must be strictly sorted and unique",
                [("field", "geometry_source_pins")],
            ));
        }
        previous = Some(key);
        match proof_class {
            GeoEvidenceCardProofClass::Fixture if !pin.source_dataset.starts_with("fixture.") => {
                return Err(proof_class_error(
                    "geometry_source_pins[].source_dataset",
                    &pin.source_dataset,
                ));
            }
            GeoEvidenceCardProofClass::RetainedArtifact
                if pin.source_dataset.starts_with("fixture.") =>
            {
                return Err(proof_class_error(
                    "geometry_source_pins[].source_dataset",
                    &pin.source_dataset,
                ));
            }
            GeoEvidenceCardProofClass::Fixture | GeoEvidenceCardProofClass::RetainedArtifact => {}
        }
    }
    Ok(())
}

fn proof_class_error(field: &'static str, value: &str) -> GeoCardError {
    GeoCardError::invalid(
        "Geo evidence card source markers do not match the artifact proof class",
        [("field", field), ("value", value)],
    )
}

fn validate_field_classifications(
    classifications: &[GeoArtifactFieldClassification],
) -> Result<(), GeoCardError> {
    if classifications.is_empty() {
        return Err(GeoCardError::invalid(
            "Geo evidence card requires field-level license classification",
            [("field", "field_classifications")],
        ));
    }
    let mut previous: Option<&str> = None;
    for classification in classifications {
        validate_text(
            "field_classifications[].field_path",
            &classification.field_path,
        )?;
        if previous.is_some_and(|previous| previous >= classification.field_path.as_str()) {
            return Err(GeoCardError::invalid(
                "Geo evidence card field classifications must be strictly sorted and unique",
                [("field", "field_classifications")],
            ));
        }
        previous = Some(&classification.field_path);
        validate_text(
            "field_classifications[].rationale",
            &classification.rationale,
        )?;
        if let Some(source) = &classification.source_instance_id {
            validate_text("field_classifications[].source_instance_id", source)?;
        }
    }
    Ok(())
}

fn default_field_classifications() -> Vec<GeoArtifactFieldClassification> {
    vec![
        field_classification(
            "$.candidate_buildings[].geometry",
            GeoArtifactFieldLicenseClass::LicensedGeometry,
            true,
            "retained candidate building geometry",
        ),
        field_classification(
            "$.candidate_parcels[].geometry",
            GeoArtifactFieldLicenseClass::LicensedGeometry,
            true,
            "retained candidate parcel geometry",
        ),
        field_classification(
            "$.composition_status",
            GeoArtifactFieldLicenseClass::Public,
            false,
            "decision state",
        ),
        field_classification(
            "$.geometry_source_pins",
            GeoArtifactFieldLicenseClass::Identifier,
            false,
            "geometry source release pins",
        ),
        field_classification(
            "$.multi_containment_cardinality",
            GeoArtifactFieldLicenseClass::DerivedMeasure,
            false,
            "candidate containment count",
        ),
    ]
}

fn field_classification(
    field_path: impl Into<String>,
    license_class: GeoArtifactFieldLicenseClass,
    reconstructive: bool,
    rationale: impl Into<String>,
) -> GeoArtifactFieldClassification {
    GeoArtifactFieldClassification {
        field_path: field_path.into(),
        license_class,
        source_instance_id: None,
        reconstructive,
        rationale: rationale.into(),
    }
}

fn validate_sorted_ids(field: &'static str, values: &[String]) -> Result<(), GeoCardError> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_text(field, value)?;
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(GeoCardError::invalid(
                "Geo evidence card id vectors must be strictly sorted and unique",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_sorted_entity_refs(
    field: &'static str,
    values: &[GeoEntityRef],
) -> Result<(), GeoCardError> {
    let mut previous: Option<&GeoEntityRef> = None;
    for value in values {
        validate_text(field, &value.id)?;
        if !matches!(
            value.level,
            GeoEntityLevel::Parcel | GeoEntityLevel::Building
        ) {
            return Err(GeoCardError::invalid(
                "Geo evidence card supports only parcel/building entity refs",
                [("field", field), ("level", level_name(value.level))],
            ));
        }
        if previous.is_some_and(|previous| previous >= value) {
            return Err(GeoCardError::invalid(
                "Geo evidence card entity refs must be strictly sorted and unique",
                [("field", field), ("member", value.id.as_str())],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_digest_shape_for_reach(
    field: &'static str,
    value: &str,
    reach: GeoCandidateReachStatus,
) -> Result<(), GeoCardError> {
    match reach {
        GeoCandidateReachStatus::Full | GeoCandidateReachStatus::Partial => {
            validate_prefixed_blake3(field, value)
        }
        GeoCandidateReachStatus::None if value.is_empty() => Ok(()),
        GeoCandidateReachStatus::None => Err(GeoCardError::invalid(
            "Geo evidence card reach-none digest fields must be empty",
            [("field", field)],
        )),
    }
}

fn validate_prefixed_blake3(field: &'static str, value: &str) -> Result<(), GeoCardError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoCardError::invalid(
            "Geo evidence card digest must be blake3-prefixed lowercase hex",
            [("field", field), ("value", value)],
        ));
    };
    validate_hex_blake3(field, hex)
}

fn validate_hex_blake3(field: &'static str, value: &str) -> Result<(), GeoCardError> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(GeoCardError::invalid(
            "Geo evidence card digest must be lowercase fixed-width hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), GeoCardError> {
    if value.is_empty() || value.trim_matches(|ch: char| ch.is_ascii_whitespace()) != value {
        return Err(GeoCardError::invalid(
            "Geo evidence card strings must be non-empty and ASCII-trimmed",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn normalize_required_reason(reason: Option<&str>) -> Result<String, GeoCardError> {
    let Some(reason) = reason else {
        return Err(GeoCardError::invalid(
            "Geo evidence card requires an explicit reason",
            [("field", "reason")],
        ));
    };
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(GeoCardError::invalid(
            "Geo evidence card requires an explicit reason",
            [("field", "reason")],
        ));
    }
    Ok(reason.to_string())
}

fn request_digest(request: &GeoCompositionRequest) -> Result<String, GeoCardError> {
    let canonical = canonicalize_composition_request(request).map_err(|error| {
        GeoCardError::mismatch(
            "Geo evidence card composition request is not canonical",
            [
                ("field".to_string(), "composition_request".to_string()),
                ("composition_error".to_string(), error.message),
            ],
        )
    })?;
    let bytes = serde_json::to_vec(&canonical).map_err(|error| {
        GeoCardError::invalid(
            "Geo evidence card request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn composition_digest(composition: &GeoCompositionArtifact) -> Result<String, GeoCardError> {
    let bytes = canonical_composition_bytes(composition).map_err(|error| {
        GeoCardError::invalid(
            "Geo evidence card composition could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn evidence_digest(evidence: &GeoEvidenceCompilationArtifact) -> Result<String, GeoCardError> {
    Ok(format!("blake3:{}", evidence_digest_hex(evidence)?))
}

fn evidence_digest_hex(evidence: &GeoEvidenceCompilationArtifact) -> Result<String, GeoCardError> {
    let bytes = canonical_evidence_compilation_bytes(evidence).map_err(|error| {
        GeoCardError::invalid(
            "Geo evidence card evidence could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn interval_release_token(interval: &super::GeoValidTimeInterval) -> String {
    format!("day{}..day{}", interval.start_day, interval.end_day)
}

fn candidate_count(candidates: &[GeoEvidenceCardCandidate]) -> Result<u64, GeoCardError> {
    u64::try_from(candidates.len()).map_err(|_| {
        GeoCardError::new(
            GeoCardErrorCode::ArithmeticOverflow,
            "Geo evidence card candidate count exceeds u64",
            [("field", "multi_containment_cardinality")],
        )
    })
}

fn level_name(level: GeoEntityLevel) -> &'static str {
    match level {
        GeoEntityLevel::Parcel => "parcel",
        GeoEntityLevel::Building => "building",
        GeoEntityLevel::PoiUnit => "poi_unit",
        GeoEntityLevel::Property => "property",
    }
}

fn sorted_unique<T: Ord>(values: Vec<T>) -> Vec<T> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
