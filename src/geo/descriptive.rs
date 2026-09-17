#![forbid(unsafe_code)]

//! Experimental addressless, single-member asset profile. Geography supplies
//! the inventory; attributes never supply candidate identifiers. Acquisition,
//! category mapping, and source error models are explicitly upstream concerns.

use super::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, GeoBoundedGeography, GeoBuildingCandidate,
    GeoCompositionProfile, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef,
    GeoEvidenceClaimRole, GeoEvidenceCompilationRequest, GeoEvidenceError, GeoEvidenceErrorCode,
    GeoEvidenceRecordRef, GeoIntegerMeasure, GeoIntegerMemberValue, GeoIntegerValueOrigin,
    GeoRhoBasis, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind, GeoTileSourceBinding,
    compile_evidence,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_GEO_DESCRIPTIVE_ASSET_REQUEST_VERSION: &str =
    "canon_geo_descriptive_asset_request.v0";
pub const GEO_DESCRIPTIVE_ASSET_PROFILE_ID: &str = "descriptive_asset_single_member_v0";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDescriptiveAttributes {
    /// Exact source name only. Even byte equality is presentation-only evidence.
    pub property_name: Option<String>,
    /// Neutral category from the mapping named by the channel's rho contract.
    pub property_type: Option<String>,
    /// Total dwelling units at the selected candidate grain, never bedrooms.
    pub unit_count: Option<u64>,
    /// Original construction year; effective/renovation/sentinel years are absent.
    pub year_built: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDescriptiveClaim {
    pub claim_id: String,
    /// Source evidence date, not a claim of a temporally complete inventory.
    pub as_of: String,
    pub attributes: GeoDescriptiveAttributes,
    pub source_records: Vec<GeoEvidenceRecordRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDescriptiveCandidate {
    pub id: String,
    pub attributes: GeoDescriptiveAttributes,
    pub source_records: Vec<GeoEvidenceRecordRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDescriptiveChannel {
    PropertyName,
    PropertyType,
    UnitCount,
    YearBuilt,
}

impl GeoDescriptiveChannel {
    fn name(self) -> &'static str {
        match self {
            Self::PropertyName => "property_name",
            Self::PropertyType => "property_type",
            Self::UnitCount => "unit_count",
            Self::YearBuilt => "year_built",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDescriptiveChannelPolicy {
    pub channel: GeoDescriptiveChannel,
    /// Inclusive absolute band in dwelling units or original construction years.
    /// Must be zero for categorical/name channels. No implicit normalization.
    pub tolerance: u64,
    /// The existing compiler owns admission. Uncalibrated observations must be
    /// diagnostic or soft, not mislabeled logical facts by this adapter.
    pub contract: GeoRhoContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDescriptiveAssetProfile {
    pub profile_id: String,
    pub selection_level: GeoEntityLevel,
    pub channels: Vec<GeoDescriptiveChannelPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDescriptiveAssetRequest {
    pub version: String,
    pub profile: GeoDescriptiveAssetProfile,
    pub bounded_geography: GeoBoundedGeography,
    pub inventory_source: GeoTileSourceBinding,
    pub claim: GeoDescriptiveClaim,
    pub candidates: Vec<GeoDescriptiveCandidate>,
    pub max_candidates: usize,
    pub max_assignments: u64,
    pub max_materialized_models: u64,
}

/// Attribute counts are comparisons, not hard admissions or empirical reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoDescriptiveChannelCounts {
    pub claim_present: bool,
    pub compatible: u64,
    pub incompatible: u64,
    pub unknown: u64,
}

pub struct GeoDescriptiveMaterialization {
    pub evidence: GeoEvidenceCompilationRequest,
    pub channels: BTreeMap<String, GeoDescriptiveChannelCounts>,
}

fn invalid(message: impl Into<String>) -> GeoEvidenceError {
    GeoEvidenceError {
        code: GeoEvidenceErrorCode::InvalidInput,
        message: message.into(),
        detail: BTreeMap::from([(
            "profile".to_string(),
            GEO_DESCRIPTIVE_ASSET_PROFILE_ID.to_string(),
        )]),
    }
}

fn text_present(value: &str) -> Result<(), GeoEvidenceError> {
    if value.trim().is_empty() {
        return Err(invalid(
            "Descriptive identifiers and present text must be nonempty",
        ));
    }
    Ok(())
}

fn validate_attributes(value: &GeoDescriptiveAttributes) -> Result<(), GeoEvidenceError> {
    for text in [&value.property_name, &value.property_type]
        .into_iter()
        .flatten()
    {
        text_present(text)?;
    }
    if value.year_built == Some(0) {
        return Err(invalid(
            "Sentinel construction years must be explicitly absent",
        ));
    }
    Ok(())
}

fn comparison(
    channel: GeoDescriptiveChannel,
    claim: &GeoDescriptiveAttributes,
    candidate: &GeoDescriptiveAttributes,
    tolerance: u64,
) -> Option<bool> {
    match channel {
        GeoDescriptiveChannel::PropertyName => {
            Some(claim.property_name.as_ref()? == candidate.property_name.as_ref()?)
        }
        GeoDescriptiveChannel::PropertyType => {
            Some(claim.property_type.as_ref()? == candidate.property_type.as_ref()?)
        }
        GeoDescriptiveChannel::UnitCount => {
            Some(claim.unit_count?.abs_diff(candidate.unit_count?) <= tolerance)
        }
        GeoDescriptiveChannel::YearBuilt => {
            Some(u64::from(claim.year_built?.abs_diff(candidate.year_built?)) <= tolerance)
        }
    }
}

pub fn materialize_descriptive_asset(
    request: &GeoDescriptiveAssetRequest,
) -> Result<GeoDescriptiveMaterialization, GeoEvidenceError> {
    if request.version != CANON_GEO_DESCRIPTIVE_ASSET_REQUEST_VERSION
        || request.profile.profile_id != GEO_DESCRIPTIVE_ASSET_PROFILE_ID
    {
        return Err(invalid(
            "Unsupported descriptive asset request or profile version",
        ));
    }
    let level = request.profile.selection_level;
    let profile = match level {
        GeoEntityLevel::Parcel => GeoCompositionProfile::default(),
        GeoEntityLevel::Building => GeoCompositionProfile::building(),
        _ => {
            return Err(invalid(
                "Descriptive v0 selects one parcel or one building; assemblages require another profile",
            ));
        }
    };
    super::tile::validate_source_binding("inventory_source", &request.inventory_source)
        .map_err(|e| invalid(e.to_string()))?;
    let native_level = request.inventory_source.native_entity_level();
    let expected_level = match level {
        GeoEntityLevel::Parcel => super::GeoControlEntityLevel::Parcel,
        GeoEntityLevel::Building => super::GeoControlEntityLevel::Building,
        _ => unreachable!(),
    };
    if native_level != Some(expected_level) {
        return Err(invalid(
            "Inventory native grain must match the descriptive selection level",
        ));
    }
    if request.claim.as_of.len() != 10
        || chrono::NaiveDate::parse_from_str(&request.claim.as_of, "%Y-%m-%d").is_err()
    {
        return Err(invalid(
            "Claim as_of must be an explicit YYYY-MM-DD evidence date",
        ));
    }
    for value in [
        &request.bounded_geography.geography_id,
        &request.bounded_geography.geography_kind,
        &request.bounded_geography.description,
        &request.claim.claim_id,
        &request.claim.as_of,
    ] {
        text_present(value)?;
    }
    if request.candidates.is_empty() || request.candidates.len() > request.max_candidates {
        return Err(invalid(
            "Descriptive inventory must be nonempty and within max_candidates",
        ));
    }
    if request.claim.source_records.is_empty() {
        return Err(invalid(
            "Descriptive claim requires source record provenance",
        ));
    }
    validate_attributes(&request.claim.attributes)?;
    let mut candidates = request.candidates.iter().collect::<Vec<_>>();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    let mut ids = BTreeSet::new();
    let mut source_records = request.claim.source_records.clone();
    for candidate in &candidates {
        text_present(&candidate.id)?;
        if !ids.insert(&candidate.id) || candidate.source_records.is_empty() {
            return Err(invalid(
                "Candidate ids must be unique and each candidate needs source provenance",
            ));
        }
        validate_attributes(&candidate.attributes)?;
        source_records.extend(candidate.source_records.clone());
    }
    source_records.sort();
    source_records.dedup();
    // Preserve the profile, evidence date, geography, and explicit missingness
    // in compiled provenance even if changing them leaves the masks unchanged.
    let mut canonical_input = request.clone();
    canonical_input.candidates.sort_by(|a, b| a.id.cmp(&b.id));
    canonical_input.profile.channels.sort_by_key(|p| p.channel);
    canonical_input.claim.source_records.sort();
    for candidate in &mut canonical_input.candidates {
        candidate.source_records.sort();
    }
    let input_bytes = serde_json::to_vec(&canonical_input).map_err(|e| invalid(e.to_string()))?;
    let input_digest = blake3::hash(&input_bytes).to_hex().to_string();
    source_records.push(GeoEvidenceRecordRef {
        source_record_id: format!("descriptive_input:{input_digest}"),
        source_vintage: request.claim.as_of.clone(),
        record_blake3: input_digest,
    });
    let seed_id = "descriptive:single_member";
    let mut evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile,
        universe: GeoCompositionUniverse {
            parcels: if level == GeoEntityLevel::Parcel {
                candidates.iter().map(|c| c.id.clone()).collect()
            } else {
                Vec::new()
            },
            buildings: if level == GeoEntityLevel::Building {
                candidates
                    .iter()
                    .map(|c| GeoBuildingCandidate {
                        id: c.id.clone(),
                        parcel_ids: Vec::new(),
                    })
                    .collect()
            } else {
                Vec::new()
            },
        },
        contracts: vec![GeoRhoContract {
            id: seed_id.to_string(),
            version: "1".to_string(),
            source_dataset: request.inventory_source.source_instance_id.clone(),
            source_release: request.inventory_source.release.release_id.clone(),
            source_lineage_ids: vec![request.inventory_source.release.release_digest.clone()],
            method_id: GEO_DESCRIPTIVE_ASSET_PROFILE_ID.to_string(),
            method_version: "1".to_string(),
            claim_role: GeoEvidenceClaimRole::AttributeObservation,
            basis: GeoRhoBasis::LogicalRelaxation {
                invariant_id: "declared-single-member-question-relative-to-bounded-inventory"
                    .to_string(),
            },
        }],
        observations: Vec::new(),
        max_assignments: request.max_assignments,
        max_materialized_models: request.max_materialized_models,
    };
    let mut emit = |id: &str,
                    contract_id: &str,
                    observation: GeoRhoObservationKind,
                    records: &[GeoEvidenceRecordRef]| {
        evidence.observations.push(GeoRhoObservation {
            id: id.to_string(),
            contract_id: contract_id.to_string(),
            source_records: records.to_vec(),
            valid_time: None,
            observation,
        });
    };
    emit(
        seed_id,
        seed_id,
        GeoRhoObservationKind::ExactCardinality { level, count: 1 },
        &source_records,
    );
    let mut channels = BTreeMap::new();
    for policy in &request.profile.channels {
        let name = policy.channel.name();
        if channels.contains_key(name) || policy.contract.id == seed_id {
            return Err(invalid(
                "Duplicate descriptive channel or reserved contract id",
            ));
        }
        if matches!(
            policy.channel,
            GeoDescriptiveChannel::PropertyName | GeoDescriptiveChannel::PropertyType
        ) && policy.tolerance != 0
        {
            return Err(invalid(
                "Text channels require zero tolerance; fuzzy names cannot become identity rules",
            ));
        }
        let present = comparison(
            policy.channel,
            &request.claim.attributes,
            &request.claim.attributes,
            policy.tolerance,
        )
        .is_some();
        let mut counts = GeoDescriptiveChannelCounts {
            claim_present: present,
            compatible: 0,
            incompatible: 0,
            unknown: 0,
        };
        let mut values = Vec::new();
        for candidate in &candidates {
            let agrees = comparison(
                policy.channel,
                &request.claim.attributes,
                &candidate.attributes,
                policy.tolerance,
            );
            match agrees {
                Some(true) => counts.compatible += 1,
                Some(false) => counts.incompatible += 1,
                None => counts.unknown += 1,
            }
            if policy.channel == GeoDescriptiveChannel::PropertyName {
                if agrees == Some(true) {
                    let mut records = request.claim.source_records.clone();
                    records.extend(candidate.source_records.clone());
                    records.sort();
                    records.dedup();
                    emit(
                        &format!("descriptive:{name}:{}", candidate.id),
                        &policy.contract.id,
                        GeoRhoObservationKind::PreferMember {
                            member: GeoEntityRef::new(level, &candidate.id),
                            cost_if_absent: 1,
                        },
                        &records,
                    );
                }
            } else {
                // Unknown values do not refute a candidate. A zero means only
                // "not contradicted", never independently corroborated truth.
                values.push(GeoIntegerMemberValue {
                    id: candidate.id.clone(),
                    value: u64::from(agrees == Some(false)),
                });
            }
        }
        if present && policy.channel != GeoDescriptiveChannel::PropertyName {
            emit(
                &format!("descriptive:{name}"),
                &policy.contract.id,
                GeoRhoObservationKind::IntegerSumBand {
                    level,
                    measure: GeoIntegerMeasure {
                        semantic_id: format!(
                            "descriptive:{name}:contradiction_mask:tolerance={}",
                            policy.tolerance
                        ),
                        unit: "contradicted_member".to_string(),
                        value_origin: GeoIntegerValueOrigin::SourceAsserted,
                    },
                    values,
                    min: 0,
                    max: 0,
                },
                &source_records,
            );
        }
        evidence.contracts.push(policy.contract.clone());
        channels.insert(name.to_string(), counts);
    }
    // Each present field needs an explicit policy, including a diagnostic policy.
    for channel in [
        GeoDescriptiveChannel::PropertyName,
        GeoDescriptiveChannel::PropertyType,
        GeoDescriptiveChannel::UnitCount,
        GeoDescriptiveChannel::YearBuilt,
    ] {
        if comparison(
            channel,
            &request.claim.attributes,
            &request.claim.attributes,
            0,
        )
        .is_some()
            && !channels.contains_key(channel.name())
        {
            return Err(invalid(format!(
                "Present {} requires an explicit channel policy",
                channel.name()
            )));
        }
    }
    compile_evidence(&evidence)?;
    Ok(GeoDescriptiveMaterialization { evidence, channels })
}
