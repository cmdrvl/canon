#![forbid(unsafe_code)]

//! Cross-deal collateral collision and adjacency concentration reporting.
//!
//! This module is a downstream ledger read. It performs deterministic set
//! algebra over already-materialized ledger entity ids; it does not acquire
//! data, widen candidate universes, or relax rho admission.

use super::{
    GeoCandidateReachStatus, GeoClaimClass, GeoCollateralLedger, GeoEntityLevel, GeoEntityRef,
    GeoEvidenceRecordRef, GeoLedgerError, GeoTruthPlane, canonical_collateral_ledger_bytes,
    validate_collateral_ledger_artifact,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_CROSS_DEAL_VERSION: &str = "canon_geo_cross_deal.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCollisionKind {
    SharedParcel,
    SharedBuilding,
    Adjacent,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPariPassuDeclaration {
    pub entity: GeoEntityRef,
    pub accessions: Vec<String>,
    pub source_record: GeoEvidenceRecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollisionSide {
    pub accession: String,
    pub loan_id: String,
    pub claim_class: GeoClaimClass,
    pub count_exact: bool,
    pub backbone_member: bool,
    pub truth_plane: GeoTruthPlane,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollision {
    pub kind: GeoCollisionKind,
    pub entity: GeoEntityRef,
    pub accessions: Vec<String>,
    pub loan_ids: Vec<String>,
    pub sides: Vec<GeoCollisionSide>,
    pub pari_passu: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record: Option<GeoEvidenceRecordRef>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAdjacencyConcentration {
    pub block_id: String,
    pub deal_count: u64,
    pub accessions: Vec<String>,
    pub loan_ids: Vec<String>,
    pub parcel_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCrossDealArtifact {
    pub version: String,
    pub ledger_blake3s: Vec<String>,
    pub collisions: Vec<GeoCollision>,
    pub adjacency_concentration: Vec<GeoAdjacencyConcentration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCollisionErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    CollisionPariPassuLabeled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollisionError {
    pub code: GeoCollisionErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoCollisionError {
    fn new(
        code: GeoCollisionErrorCode,
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
        Self::new(GeoCollisionErrorCode::InvalidInput, message, detail)
    }

    fn invalid_field(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::invalid(
            "Geo cross-deal collision input contains an invalid field",
            [("field", field.into()), ("value", value.into())],
        )
    }

    fn overflow(field: impl Into<String>) -> Self {
        Self::new(
            GeoCollisionErrorCode::ArithmeticOverflow,
            "Geo cross-deal collision accounting overflowed",
            [("field", field.into())],
        )
    }
}

impl fmt::Display for GeoCollisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoCollisionError {}

pub fn find_collisions(
    ledgers: &[GeoCollateralLedger],
    declarations: &[GeoPariPassuDeclaration],
    adjacency: &BTreeMap<String, String>,
) -> Result<GeoCrossDealArtifact, GeoCollisionError> {
    if ledgers.len() < 2 {
        return Err(GeoCollisionError::invalid(
            "Geo cross-deal collision requires at least two ledgers",
            [("field", "ledgers")],
        ));
    }
    for ledger in ledgers {
        validate_collateral_ledger_artifact(ledger).map_err(ledger_error)?;
    }
    let declarations = canonical_declarations(declarations)?;
    validate_adjacency(adjacency)?;

    let ledger_blake3s = ledger_blake3s(ledgers)?;
    let entity_sides = ledger_entity_sides(ledgers)?;
    let collisions = collision_rows(&entity_sides, &declarations)?;
    let adjacency_concentration = adjacency_rows(&entity_sides, adjacency)?;
    let artifact = GeoCrossDealArtifact {
        version: CANON_GEO_CROSS_DEAL_VERSION.to_string(),
        ledger_blake3s,
        collisions,
        adjacency_concentration,
    };
    validate_cross_deal_artifact(&artifact)?;
    Ok(artifact)
}

pub fn validate_cross_deal_artifact(
    artifact: &GeoCrossDealArtifact,
) -> Result<(), GeoCollisionError> {
    if artifact.version != CANON_GEO_CROSS_DEAL_VERSION {
        return Err(GeoCollisionError::new(
            GeoCollisionErrorCode::UnsupportedVersion,
            "Unsupported Geo cross-deal artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_CROSS_DEAL_VERSION),
            ],
        ));
    }
    validate_nonempty("ledger_blake3s", &artifact.ledger_blake3s)?;
    validate_sorted_unique("ledger_blake3s", &artifact.ledger_blake3s)?;
    for ledger_blake3 in &artifact.ledger_blake3s {
        validate_prefixed_blake3("ledger_blake3s[]", ledger_blake3)?;
    }
    validate_collision_rows(&artifact.collisions)?;
    validate_adjacency_rows(&artifact.adjacency_concentration)?;
    Ok(())
}

pub fn canonical_cross_deal_bytes(
    artifact: &GeoCrossDealArtifact,
) -> Result<Vec<u8>, GeoCollisionError> {
    validate_cross_deal_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoCollisionError::invalid(
            "Geo cross-deal artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EntitySide {
    kind: GeoCollisionKind,
    entity: GeoEntityRef,
    accession: String,
    loan_id: String,
    claim_class: GeoClaimClass,
    count_exact: bool,
    backbone_member: bool,
    truth_plane: GeoTruthPlane,
}

impl EntitySide {
    fn collision_side(&self) -> GeoCollisionSide {
        GeoCollisionSide {
            accession: self.accession.clone(),
            loan_id: self.loan_id.clone(),
            claim_class: self.claim_class,
            count_exact: self.count_exact,
            backbone_member: self.backbone_member,
            truth_plane: self.truth_plane,
        }
    }
}

fn ledger_blake3s(ledgers: &[GeoCollateralLedger]) -> Result<Vec<String>, GeoCollisionError> {
    ledgers
        .iter()
        .map(|ledger| {
            canonical_collateral_ledger_bytes(ledger)
                .map_err(ledger_error)
                .map(|bytes| digest_prefixed(&bytes))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(sorted_unique)
}

fn ledger_entity_sides(
    ledgers: &[GeoCollateralLedger],
) -> Result<Vec<EntitySide>, GeoCollisionError> {
    let mut sides = BTreeSet::new();
    for ledger in ledgers {
        for row in &ledger.rows {
            if row.reach == GeoCandidateReachStatus::None {
                continue;
            }
            let truth_plane = row.truth_plane.ok_or_else(|| {
                GeoCollisionError::invalid(
                    "Geo cross-deal collision requires ledger truth-plane labels",
                    [("field", "truth_plane"), ("loan_id", row.loan_id.as_str())],
                )
            })?;
            for (kind, entity_id, backbone_member) in ledger_entities_for_row(row)? {
                sides.insert(EntitySide {
                    kind,
                    entity: GeoEntityRef::new(entity_level_for_kind(kind), entity_id),
                    accession: row.accession.clone(),
                    loan_id: row.loan_id.clone(),
                    claim_class: row.claim_class,
                    count_exact: row.count_exact,
                    backbone_member,
                    truth_plane,
                });
            }
        }
    }
    Ok(sides.into_iter().collect())
}

fn ledger_entities_for_row(
    row: &super::GeoLedgerRow,
) -> Result<Vec<(GeoCollisionKind, String, bool)>, GeoCollisionError> {
    let mut entities = BTreeMap::<(GeoCollisionKind, String), bool>::new();
    for parcel_id in &row.ambiguous_parcel_set {
        validate_text("ambiguous_parcel_set[]", parcel_id)?;
        entities.insert((GeoCollisionKind::SharedParcel, parcel_id.clone()), false);
    }
    if let Some(parcel_set) = row.parcel_set.as_ref() {
        for parcel_id in parcel_set {
            validate_text("parcel_set[]", parcel_id)?;
            entities.insert((GeoCollisionKind::SharedParcel, parcel_id.clone()), true);
        }
    }
    for building_id in &row.ambiguous_building_set {
        validate_text("ambiguous_building_set[]", building_id)?;
        entities.insert(
            (GeoCollisionKind::SharedBuilding, building_id.clone()),
            false,
        );
    }
    if let Some(building_set) = row.building_set.as_ref() {
        for building_id in building_set {
            validate_text("building_set[]", building_id)?;
            entities.insert(
                (GeoCollisionKind::SharedBuilding, building_id.clone()),
                true,
            );
        }
    }
    Ok(entities
        .into_iter()
        .map(|((kind, id), backbone_member)| (kind, id, backbone_member))
        .collect())
}

fn collision_rows(
    entity_sides: &[EntitySide],
    declarations: &[GeoPariPassuDeclaration],
) -> Result<Vec<GeoCollision>, GeoCollisionError> {
    let mut grouped = BTreeMap::<(GeoCollisionKind, GeoEntityRef), Vec<EntitySide>>::new();
    for side in entity_sides {
        grouped
            .entry((side.kind, side.entity.clone()))
            .or_default()
            .push(side.clone());
    }

    let mut collisions = Vec::new();
    for ((kind, entity), mut sides) in grouped {
        let accessions = sorted_unique(sides.iter().map(|side| side.accession.clone()).collect());
        if accessions.len() < 2 {
            continue;
        }
        sides.sort();
        let loan_ids = sorted_unique(sides.iter().map(|side| side.loan_id.clone()).collect());
        let collision_sides = sides
            .iter()
            .map(EntitySide::collision_side)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let declaration = covering_declaration(&entity, &accessions, declarations);
        let (pari_passu, source_record, explanation) = match declaration {
            Some(declaration) => (
                true,
                Some(declaration.source_record.clone()),
                format!(
                    "Source-pinned pari passu declaration {} covers every colliding accession.",
                    declaration.source_record.source_record_id
                ),
            ),
            None => (
                false,
                None,
                "No source-pinned pari passu declaration covers every colliding accession."
                    .to_string(),
            ),
        };
        collisions.push(GeoCollision {
            kind,
            entity,
            accessions,
            loan_ids,
            sides: collision_sides,
            pari_passu,
            source_record,
            explanation,
        });
    }
    collisions.sort();
    Ok(collisions)
}

fn adjacency_rows(
    entity_sides: &[EntitySide],
    adjacency: &BTreeMap<String, String>,
) -> Result<Vec<GeoAdjacencyConcentration>, GeoCollisionError> {
    let mut grouped = BTreeMap::<String, AdjacencyAccumulator>::new();
    for side in entity_sides {
        if side.kind != GeoCollisionKind::SharedParcel {
            continue;
        }
        let Some(block_id) = adjacency.get(&side.entity.id) else {
            continue;
        };
        let entry = grouped.entry(block_id.clone()).or_default();
        entry.accessions.insert(side.accession.clone());
        entry.loan_ids.insert(side.loan_id.clone());
        entry.parcel_ids.insert(side.entity.id.clone());
    }

    let mut rows = Vec::new();
    for (block_id, entry) in grouped {
        if entry.accessions.len() < 2 {
            continue;
        }
        rows.push(GeoAdjacencyConcentration {
            block_id,
            deal_count: usize_to_u64(entry.accessions.len(), "adjacency_concentration.deal_count")?,
            accessions: entry.accessions.into_iter().collect(),
            loan_ids: entry.loan_ids.into_iter().collect(),
            parcel_count: usize_to_u64(
                entry.parcel_ids.len(),
                "adjacency_concentration.parcel_count",
            )?,
        });
    }
    rows.sort();
    Ok(rows)
}

#[derive(Default)]
struct AdjacencyAccumulator {
    accessions: BTreeSet<String>,
    loan_ids: BTreeSet<String>,
    parcel_ids: BTreeSet<String>,
}

fn canonical_declarations(
    declarations: &[GeoPariPassuDeclaration],
) -> Result<Vec<GeoPariPassuDeclaration>, GeoCollisionError> {
    let mut canonical = Vec::with_capacity(declarations.len());
    for declaration in declarations {
        validate_entity_ref("declarations[].entity", &declaration.entity)?;
        validate_nonempty("declarations[].accessions", &declaration.accessions)?;
        let accessions = sorted_unique(declaration.accessions.clone());
        if accessions.len() != declaration.accessions.len() {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal pari passu declarations must list unique accessions",
                [("field", "declarations[].accessions")],
            ));
        }
        for accession in &accessions {
            validate_text("declarations[].accessions[]", accession)?;
        }
        validate_evidence_record_ref(&declaration.source_record)?;
        canonical.push(GeoPariPassuDeclaration {
            entity: declaration.entity.clone(),
            accessions,
            source_record: declaration.source_record.clone(),
        });
    }
    canonical.sort();
    Ok(canonical)
}

fn covering_declaration<'a>(
    entity: &GeoEntityRef,
    accessions: &[String],
    declarations: &'a [GeoPariPassuDeclaration],
) -> Option<&'a GeoPariPassuDeclaration> {
    declarations.iter().find(|declaration| {
        declaration.entity == *entity
            && accessions
                .iter()
                .all(|accession| declaration.accessions.contains(accession))
    })
}

fn validate_collision_rows(collisions: &[GeoCollision]) -> Result<(), GeoCollisionError> {
    let mut previous: Option<&GeoCollision> = None;
    for collision in collisions {
        validate_collision_kind_entity(collision.kind, &collision.entity)?;
        validate_nonempty("collisions[].accessions", &collision.accessions)?;
        validate_nonempty("collisions[].loan_ids", &collision.loan_ids)?;
        validate_nonempty("collisions[].sides", &collision.sides)?;
        validate_sorted_unique("collisions[].accessions", &collision.accessions)?;
        validate_sorted_unique("collisions[].loan_ids", &collision.loan_ids)?;
        for accession in &collision.accessions {
            validate_text("collisions[].accessions[]", accession)?;
        }
        for loan_id in &collision.loan_ids {
            validate_text("collisions[].loan_ids[]", loan_id)?;
        }
        validate_text("collisions[].explanation", &collision.explanation)?;
        if collision.accessions.len() < 2 {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal collision rows require at least two accessions",
                [("field", "collisions[].accessions")],
            ));
        }
        if collision.pari_passu {
            let Some(source_record) = collision.source_record.as_ref() else {
                return Err(GeoCollisionError::invalid(
                    "Geo cross-deal pari passu rows must carry the declaration source record",
                    [("field", "collisions[].source_record")],
                ));
            };
            validate_evidence_record_ref(source_record)?;
        } else if collision.source_record.is_some() {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal undeclared collision rows must not carry a pari passu source record",
                [("field", "collisions[].source_record")],
            ));
        }
        let mut side_accessions = BTreeSet::new();
        let mut side_loans = BTreeSet::new();
        let mut previous_side: Option<&GeoCollisionSide> = None;
        for side in &collision.sides {
            validate_text("collisions[].sides[].accession", &side.accession)?;
            validate_text("collisions[].sides[].loan_id", &side.loan_id)?;
            if side.claim_class != GeoClaimClass::CollateralComposition {
                return Err(GeoCollisionError::invalid(
                    "Geo cross-deal collision sides must carry ledger collateral-composition claims",
                    [("field", "collisions[].sides[].claim_class")],
                ));
            }
            side_accessions.insert(side.accession.clone());
            side_loans.insert(side.loan_id.clone());
            if let Some(previous_side) = previous_side
                && previous_side >= side
            {
                return Err(GeoCollisionError::invalid(
                    "Geo cross-deal collision sides must be strictly sorted and unique",
                    [("field", "collisions[].sides")],
                ));
            }
            previous_side = Some(side);
        }
        if collision.accessions != side_accessions.into_iter().collect::<Vec<_>>() {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal collision accessions do not match side rows",
                [("field", "collisions[].accessions")],
            ));
        }
        if collision.loan_ids != side_loans.into_iter().collect::<Vec<_>>() {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal collision loan_ids do not match side rows",
                [("field", "collisions[].loan_ids")],
            ));
        }
        if let Some(previous) = previous
            && previous >= collision
        {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal collision rows must be strictly sorted and unique",
                [("field", "collisions")],
            ));
        }
        previous = Some(collision);
    }
    Ok(())
}

fn validate_adjacency_rows(rows: &[GeoAdjacencyConcentration]) -> Result<(), GeoCollisionError> {
    let mut previous: Option<&GeoAdjacencyConcentration> = None;
    for row in rows {
        validate_text("adjacency_concentration[].block_id", &row.block_id)?;
        validate_nonempty("adjacency_concentration[].accessions", &row.accessions)?;
        validate_nonempty("adjacency_concentration[].loan_ids", &row.loan_ids)?;
        validate_sorted_unique("adjacency_concentration[].accessions", &row.accessions)?;
        validate_sorted_unique("adjacency_concentration[].loan_ids", &row.loan_ids)?;
        if row.accessions.len() < 2 {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal adjacency concentration requires at least two accessions",
                [("field", "adjacency_concentration[].accessions")],
            ));
        }
        if row.deal_count
            != usize_to_u64(row.accessions.len(), "adjacency_concentration.deal_count")?
        {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal adjacency deal_count must equal distinct accessions",
                [("field", "adjacency_concentration[].deal_count")],
            ));
        }
        if row.parcel_count == 0 {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal adjacency parcel_count must be positive",
                [("field", "adjacency_concentration[].parcel_count")],
            ));
        }
        for accession in &row.accessions {
            validate_text("adjacency_concentration[].accessions[]", accession)?;
        }
        for loan_id in &row.loan_ids {
            validate_text("adjacency_concentration[].loan_ids[]", loan_id)?;
        }
        if let Some(previous) = previous
            && previous >= row
        {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal adjacency concentration rows must be strictly sorted and unique",
                [("field", "adjacency_concentration")],
            ));
        }
        previous = Some(row);
    }
    Ok(())
}

fn validate_adjacency(adjacency: &BTreeMap<String, String>) -> Result<(), GeoCollisionError> {
    for (parcel_id, block_id) in adjacency {
        validate_text("adjacency.parcel_id", parcel_id)?;
        validate_text("adjacency.block_id", block_id)?;
    }
    Ok(())
}

fn validate_collision_kind_entity(
    kind: GeoCollisionKind,
    entity: &GeoEntityRef,
) -> Result<(), GeoCollisionError> {
    validate_entity_ref("collisions[].entity", entity)?;
    let expected = entity_level_for_kind(kind);
    if entity.level != expected {
        return Err(GeoCollisionError::invalid(
            "Geo cross-deal collision kind and entity level disagree",
            [
                ("field", "collisions[].entity.level"),
                ("entity_id", entity.id.as_str()),
            ],
        ));
    }
    Ok(())
}

fn entity_level_for_kind(kind: GeoCollisionKind) -> GeoEntityLevel {
    match kind {
        GeoCollisionKind::SharedParcel | GeoCollisionKind::Adjacent => GeoEntityLevel::Parcel,
        GeoCollisionKind::SharedBuilding => GeoEntityLevel::Building,
    }
}

fn validate_entity_ref(
    field: &'static str,
    entity: &GeoEntityRef,
) -> Result<(), GeoCollisionError> {
    validate_text(field, &entity.id)
}

fn validate_evidence_record_ref(record: &GeoEvidenceRecordRef) -> Result<(), GeoCollisionError> {
    validate_text("source_record.source_record_id", &record.source_record_id)?;
    validate_text("source_record.source_vintage", &record.source_vintage)?;
    validate_hex_blake3("source_record.record_blake3", &record.record_blake3)
}

fn validate_text(field: &str, value: &str) -> Result<(), GeoCollisionError> {
    if value.trim().is_empty() {
        return Err(GeoCollisionError::invalid_field(field, value));
    }
    Ok(())
}

fn validate_nonempty<T>(field: &'static str, values: &[T]) -> Result<(), GeoCollisionError> {
    if values.is_empty() {
        return Err(GeoCollisionError::invalid(
            "Geo cross-deal collision vectors must be nonempty",
            [("field", field)],
        ));
    }
    Ok(())
}

fn validate_sorted_unique(field: &'static str, values: &[String]) -> Result<(), GeoCollisionError> {
    for pair in values.windows(2) {
        if pair[0] >= pair[1] {
            return Err(GeoCollisionError::invalid(
                "Geo cross-deal collision vectors must be strictly sorted and unique",
                [("field", field), ("value", pair[1].as_str())],
            ));
        }
    }
    Ok(())
}

fn validate_prefixed_blake3(field: &str, value: &str) -> Result<(), GeoCollisionError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoCollisionError::invalid(
            "Geo cross-deal collision digests must use blake3:<hex>",
            [("field", field), ("value", value)],
        ));
    };
    validate_hex_blake3(field, hex)
}

fn validate_hex_blake3(field: &str, value: &str) -> Result<(), GeoCollisionError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoCollisionError::invalid(
            "Geo cross-deal collision digests must use lowercase blake3 hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn ledger_error(error: GeoLedgerError) -> GeoCollisionError {
    let mut detail = BTreeMap::new();
    detail.insert("ledger_code".to_string(), format!("{:?}", error.code));
    detail.extend(error.detail);
    GeoCollisionError::new(
        GeoCollisionErrorCode::InvalidInput,
        "Geo cross-deal collision received an invalid collateral ledger",
        detail,
    )
}

fn digest_prefixed(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn usize_to_u64(value: usize, field: impl Into<String>) -> Result<u64, GeoCollisionError> {
    u64::try_from(value).map_err(|_| GeoCollisionError::overflow(field))
}

fn sorted_unique<T: Ord>(values: Vec<T>) -> Vec<T> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
