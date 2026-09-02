#![forbid(unsafe_code)]

//! Deterministic query-side address parse forests for Geo evidence.
//!
//! This is not a geocoder and it intentionally does not choose a single best
//! parse. A declared jurisdiction and grammar version turn a query string into
//! a sorted domain of candidate address readings. PAD/address-point evidence is
//! then checked as membership in an asserted address set; number/street
//! cross-products are never synthesized during evaluation.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use super::{
    composition::{GeoEntityLevel, GeoEntityRef},
    control::GeoAsOf,
    evidence::{
        CANON_GEO_EVIDENCE_REQUEST_VERSION, GeoEvidenceCompilationRequest, GeoEvidenceError,
        GeoEvidenceRecordRef, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind,
        GeoValidTimeInterval, compile_evidence,
    },
};

pub const CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION: &str = "canon_geo_address_parse_request.v0";
pub const CANON_GEO_ADDRESS_PARSE_FOREST_VERSION: &str = "canon_geo_address_parse_forest.v0";
pub const CANON_GEO_ADDRESS_QUERY_GRAMMAR_ID: &str = "canon_geo_address_query_regular";
pub const CANON_GEO_ADDRESS_QUERY_GRAMMAR_VERSION: &str = "canon_geo_address_query_regular.v0";
pub const CANON_GEO_PAD_ADDRESS_SET_VERSION: &str = "canon_geo_pad_address_set.v0";
pub const CANON_GEO_PAD_MEMBERSHIP_VERSION: &str = "canon_geo_pad_membership.v0";
pub const CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION: &str =
    "canon_geo_address_parcel_bridge_request.v0";
pub const CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION: &str = "canon_geo_address_parcel_bridge.v0";
pub const CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION: &str =
    "canon_geo_address_parcel_evidence_request.v0";
pub const CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION: &str =
    "canon_geo_address_parcel_evidence_bundle.v0";

const MAX_RANGE_CARDINALITY: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNycBorough {
    Bronx,
    Brooklyn,
    Manhattan,
    Queens,
    StatenIsland,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoAddressJurisdiction {
    /// NYC PAD/SND semantics. `borough: None` is deliberately ambiguous: the
    /// Queens hyphenate rule is jurisdictional, so callers must name a borough.
    Nyc { borough: Option<GeoNycBorough> },
}

impl GeoAddressJurisdiction {
    pub fn nyc_borough(borough: GeoNycBorough) -> Self {
        Self::Nyc {
            borough: Some(borough),
        }
    }

    fn required_borough(&self) -> Result<GeoNycBorough, GeoAddressError> {
        match self {
            GeoAddressJurisdiction::Nyc {
                borough: Some(borough),
            } => Ok(*borough),
            GeoAddressJurisdiction::Nyc { borough: None } => Err(GeoAddressError::new(
                GeoAddressErrorCode::AmbiguousJurisdiction,
                "NYC address parsing requires an explicit borough",
                [("jurisdiction", "nyc")],
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParseRequest {
    pub version: String,
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<GeoAddressJurisdiction>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressGrammarRef {
    pub id: String,
    pub version: String,
}

impl Default for GeoAddressGrammarRef {
    fn default() -> Self {
        Self {
            id: CANON_GEO_ADDRESS_QUERY_GRAMMAR_ID.to_string(),
            version: CANON_GEO_ADDRESS_QUERY_GRAMMAR_VERSION.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAddressPlaceholderKind {
    Various,
    Multiple,
    Unknown,
    ToBeDetermined,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressPlaceholder {
    pub kind: GeoAddressPlaceholderKind,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParseForest {
    pub version: String,
    pub request_version: String,
    pub grammar: GeoAddressGrammarRef,
    pub jurisdiction: GeoAddressJurisdiction,
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<GeoAddressPlaceholder>,
    pub candidates: Vec<GeoAddressCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressCandidate {
    pub reading_id: String,
    pub canonical_key: String,
    pub display: String,
    pub source_grammar: GeoAddressGrammarRef,
    pub source_segment: String,
    pub house: GeoAddressHouseNumber,
    pub street: GeoAddressStreet,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<GeoAddressAnnotation>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAddressAnnotation {
    AkaAlternative,
    EnumeratedHouseNumber,
    SlashRange,
    DashRange,
    DashListRange,
    QueensHyphenateLiteral,
    ImplicitOrdinalStreetSuffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAddressParity {
    Any,
    Even,
    Odd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAddressRangeOperator {
    Slash,
    Dash,
    DashList,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoAddressHouseNumber {
    Discrete {
        value: u32,
    },
    Range {
        start: u32,
        end: u32,
        parity: GeoAddressParity,
        operator: GeoAddressRangeOperator,
        asserted_numbers: Vec<u32>,
    },
    HyphenatedLiteral {
        value: String,
    },
}

impl GeoAddressHouseNumber {
    pub fn discrete(value: u32) -> Self {
        Self::Discrete { value }
    }

    pub fn range(
        start: u32,
        end: u32,
        parity: GeoAddressParity,
        operator: GeoAddressRangeOperator,
        asserted_numbers: Vec<u32>,
    ) -> Result<Self, GeoAddressError> {
        if start > end {
            return Err(GeoAddressError::invalid_input(
                "address range start is greater than end",
                [("range", format!("{start}-{end}"))],
            ));
        }
        Ok(Self::Range {
            start,
            end,
            parity,
            operator,
            asserted_numbers: canonical_numbers(asserted_numbers),
        })
    }

    pub fn queens_hyphenated_literal(value: impl Into<String>) -> Self {
        Self::HyphenatedLiteral {
            value: value.into().to_ascii_uppercase(),
        }
    }

    fn canonical_key(&self) -> String {
        match self {
            GeoAddressHouseNumber::Discrete { value } => format!("n:{value:010}"),
            GeoAddressHouseNumber::Range {
                start,
                end,
                parity,
                asserted_numbers,
                ..
            } => format!(
                "r:{start:010}:{end:010}:{}:{}",
                parity_key(parity),
                asserted_numbers
                    .iter()
                    .map(|number| format!("{number:010}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            GeoAddressHouseNumber::HyphenatedLiteral { value } => format!("h:{value}"),
        }
    }

    fn display(&self) -> String {
        match self {
            GeoAddressHouseNumber::Discrete { value } => value.to_string(),
            GeoAddressHouseNumber::Range {
                start,
                end,
                operator,
                ..
            } => match operator {
                GeoAddressRangeOperator::Slash => format!("{start}/{end}"),
                GeoAddressRangeOperator::Dash | GeoAddressRangeOperator::DashList => {
                    format!("{start}-{end}")
                }
            },
            GeoAddressHouseNumber::HyphenatedLiteral { value } => value.clone(),
        }
    }

    fn required_numeric_members(&self) -> Result<Option<Vec<u32>>, GeoAddressError> {
        match self {
            GeoAddressHouseNumber::Discrete { value } => Ok(Some(vec![*value])),
            GeoAddressHouseNumber::Range {
                start, end, parity, ..
            } => {
                let span = end - start + 1;
                if span as usize > MAX_RANGE_CARDINALITY {
                    return Err(GeoAddressError::invalid_input(
                        "address range exceeds membership expansion budget",
                        [
                            ("start", start.to_string()),
                            ("end", end.to_string()),
                            ("max_cardinality", MAX_RANGE_CARDINALITY.to_string()),
                        ],
                    ));
                }
                let mut values = Vec::new();
                for value in *start..=*end {
                    if parity_accepts(*parity, value) {
                        values.push(value);
                    }
                }
                Ok(Some(values))
            }
            GeoAddressHouseNumber::HyphenatedLiteral { .. } => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoStreetDirection {
    North,
    South,
    East,
    West,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoStreetSuffix {
    Avenue,
    Boulevard,
    Court,
    Drive,
    Lane,
    Place,
    Road,
    Street,
    Terrace,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoStreetNameToken {
    Literal { value: String },
    Ordinal { value: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressStreet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_direction: Option<GeoStreetDirection>,
    pub name: Vec<GeoStreetNameToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<GeoStreetSuffix>,
}

impl GeoAddressStreet {
    pub fn new(
        pre_direction: Option<GeoStreetDirection>,
        name: Vec<GeoStreetNameToken>,
        suffix: Option<GeoStreetSuffix>,
    ) -> Result<Self, GeoAddressError> {
        if name.is_empty() {
            return Err(GeoAddressError::invalid_input(
                "street name must contain at least one token",
                [("field", "street.name".to_string())],
            ));
        }
        Ok(Self {
            pre_direction,
            name,
            suffix,
        })
    }

    pub fn literal(
        pre_direction: Option<GeoStreetDirection>,
        name: &[&str],
        suffix: Option<GeoStreetSuffix>,
    ) -> Result<Self, GeoAddressError> {
        Self::new(
            pre_direction,
            name.iter()
                .map(|token| GeoStreetNameToken::Literal {
                    value: canonical_literal_token(token),
                })
                .collect(),
            suffix,
        )
    }

    pub fn ordinal(
        pre_direction: Option<GeoStreetDirection>,
        value: u16,
        suffix: Option<GeoStreetSuffix>,
    ) -> Result<Self, GeoAddressError> {
        Self::new(
            pre_direction,
            vec![GeoStreetNameToken::Ordinal { value }],
            suffix,
        )
    }

    fn canonical_key(&self) -> String {
        let direction = self.pre_direction.map_or("_".to_string(), |direction| {
            format!("dir:{}", direction_key(direction))
        });
        let name = self
            .name
            .iter()
            .map(|token| match token {
                GeoStreetNameToken::Literal { value } => format!("lit:{value}"),
                GeoStreetNameToken::Ordinal { value } => format!("ord:{value:04}"),
            })
            .collect::<Vec<_>>()
            .join(".");
        let suffix = self.suffix.map_or("_".to_string(), |suffix| {
            format!("sfx:{}", suffix_key(suffix))
        });
        format!("{direction}|name:{name}|{suffix}")
    }

    fn display(&self) -> String {
        let mut parts = Vec::new();
        if let Some(direction) = self.pre_direction {
            parts.push(direction_display(direction).to_string());
        }
        for token in &self.name {
            match token {
                GeoStreetNameToken::Literal { value } => parts.push(title_token(value)),
                GeoStreetNameToken::Ordinal { value } => parts.push(ordinal_display(*value)),
            }
        }
        if let Some(suffix) = self.suffix {
            parts.push(suffix_display(suffix).to_string());
        }
        parts.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPadAddressSet {
    pub version: String,
    pub jurisdiction: GeoAddressJurisdiction,
    pub members: Vec<GeoPadAddressMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPadAddressMember {
    pub member_id: String,
    pub lot_id: String,
    pub house: GeoAddressHouseNumber,
    pub street: GeoAddressStreet,
}

impl GeoPadAddressMember {
    pub fn new(
        member_id: impl Into<String>,
        lot_id: impl Into<String>,
        house: GeoAddressHouseNumber,
        street: GeoAddressStreet,
    ) -> Self {
        Self {
            member_id: member_id.into(),
            lot_id: lot_id.into(),
            house,
            street,
        }
    }

    fn canonical_key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.house.canonical_key(),
            self.street.canonical_key(),
            self.lot_id,
            self.member_id
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPadMembershipEvaluation {
    pub version: String,
    pub parse_forest_version: String,
    pub address_set_version: String,
    pub grammar: GeoAddressGrammarRef,
    pub jurisdiction: GeoAddressJurisdiction,
    pub input: String,
    pub results: Vec<GeoPadCandidateEvaluation>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPadCandidateEvaluation {
    pub candidate_key: String,
    pub candidate: GeoAddressCandidate,
    pub status: GeoPadCandidateStatus,
    pub asserted_member: bool,
    pub compatible: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_member_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatible_member_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<GeoPadCompatibilityReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPadCandidateStatus {
    ExactMember,
    RangeContained,
    CoveredByAddressSet,
    CompatibleOnly,
    NoSourceMember,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPadCompatibilityReason {
    ExactStreet,
    QuerySuffixUnspecified,
    ExactHouseNumber,
    PadRangeContainsQuery,
    QueryRangeCoveredByMembers,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParcelBridgeRequest {
    pub version: String,
    pub observation_id: String,
    pub contract_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_as_of: Option<GeoAsOf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_time: Option<GeoValidTimeInterval>,
    pub member_source_records: Vec<GeoPadMemberSourceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPadMemberSourceRecord {
    pub member_id: String,
    /// BLAKE3 of the normalized `GeoPadAddressMember` JSON bytes. This binds
    /// the lot/address payload used by the bridge to the source-record
    /// association without pretending to authenticate the upstream row.
    pub normalized_member_blake3: String,
    pub source_record: GeoEvidenceRecordRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAddressParcelBridgeStatus {
    EvidenceObservation,
    DiagnosticAbstention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAddressParcelDiagnosticCode {
    NoParseReadings,
    NoSourceMemberSupport,
    NoBoundSourceRecords,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParcelDiagnostic {
    pub code: GeoAddressParcelDiagnosticCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_member_ids_without_source_records: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParcelReadingSupport {
    pub reading_id: String,
    pub candidate_key: String,
    pub status: GeoPadCandidateStatus,
    pub asserted_member: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_member_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_supported_member_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parcel_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_record_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParcelBridge {
    pub version: String,
    pub request_version: String,
    pub parse_forest_version: String,
    pub pad_membership_version: String,
    pub grammar: GeoAddressGrammarRef,
    pub jurisdiction: GeoAddressJurisdiction,
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_as_of: Option<GeoAsOf>,
    pub status: GeoAddressParcelBridgeStatus,
    pub readings: Vec<GeoAddressParcelReadingSupport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parcel_candidates: Vec<GeoEntityRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_records: Vec<GeoEvidenceRecordRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_time: Option<GeoValidTimeInterval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<GeoRhoObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<GeoAddressParcelDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParcelEvidenceRequest {
    pub version: String,
    pub parse_request: GeoAddressParseRequest,
    pub address_set: GeoPadAddressSet,
    pub bridge_request: GeoAddressParcelBridgeRequest,
    /// Optional compiler envelope supplied by the caller. When the bridge emits
    /// positive evidence, Canon fills this with the bridge observation and
    /// validates it through the normal evidence compiler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_request: Option<GeoEvidenceCompilationRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressParcelEvidenceBundle {
    pub version: String,
    pub request_version: String,
    pub parse_forest: GeoAddressParseForest,
    pub pad_membership: GeoPadMembershipEvaluation,
    pub bridge: GeoAddressParcelBridge,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_request: Option<GeoEvidenceCompilationRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAddressErrorCode {
    UnsupportedVersion,
    MissingJurisdiction,
    AmbiguousJurisdiction,
    EmptyInput,
    PlaceholderOnly,
    UnsupportedPattern,
    InvalidHouseNumber,
    InvalidStreet,
    InvalidPadAddressSet,
    JurisdictionMismatch,
    InvalidInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAddressError {
    pub code: GeoAddressErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoAddressError {
    fn new(
        code: GeoAddressErrorCode,
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

    fn invalid_input(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoAddressErrorCode::InvalidInput, message, detail)
    }

    fn unsupported_pattern(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoAddressErrorCode::UnsupportedPattern, message, detail)
    }
}

impl fmt::Display for GeoAddressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoAddressError {}

pub fn parse_address_forest(
    request: &GeoAddressParseRequest,
) -> Result<GeoAddressParseForest, GeoAddressError> {
    if request.version != CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported address parse request version",
            [
                ("expected", CANON_GEO_ADDRESS_PARSE_REQUEST_VERSION),
                ("actual", request.version.as_str()),
            ],
        ));
    }
    let jurisdiction = request.jurisdiction.clone().ok_or_else(|| {
        GeoAddressError::new(
            GeoAddressErrorCode::MissingJurisdiction,
            "address parsing requires an explicit jurisdiction",
            [("field", "jurisdiction")],
        )
    })?;
    let borough = jurisdiction.required_borough()?;
    let normalized = normalize_input(&request.input);
    if normalized.is_empty() {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::EmptyInput,
            "address input is empty after ASCII trim",
            [("input", request.input.as_str())],
        ));
    }

    if let Some(placeholder) = detect_placeholder(&normalized) {
        return Ok(GeoAddressParseForest {
            version: CANON_GEO_ADDRESS_PARSE_FOREST_VERSION.to_string(),
            request_version: request.version.clone(),
            grammar: GeoAddressGrammarRef::default(),
            jurisdiction,
            input: request.input.clone(),
            placeholder: Some(placeholder),
            candidates: Vec::new(),
        });
    }

    let alternatives = split_alternatives(&normalized);
    let has_aka = alternatives.len() > 1;
    let mut candidates = BTreeMap::<String, PendingCandidate>::new();
    for segment in alternatives {
        for candidate in parse_segment(&segment, borough, has_aka)? {
            candidates
                .entry(candidate.canonical_key.clone())
                .and_modify(|existing| existing.merge_annotations(&candidate.annotations))
                .or_insert(candidate);
        }
    }

    if candidates.is_empty() {
        return Err(GeoAddressError::unsupported_pattern(
            "address input did not produce any candidate readings",
            [("input", normalized)],
        ));
    }

    let mut materialized = Vec::with_capacity(candidates.len());
    for (index, (_, pending)) in candidates.into_iter().enumerate() {
        materialized.push(pending.materialize(index + 1));
    }

    Ok(GeoAddressParseForest {
        version: CANON_GEO_ADDRESS_PARSE_FOREST_VERSION.to_string(),
        request_version: request.version.clone(),
        grammar: GeoAddressGrammarRef::default(),
        jurisdiction,
        input: request.input.clone(),
        placeholder: None,
        candidates: materialized,
    })
}

pub fn evaluate_pad_membership(
    forest: &GeoAddressParseForest,
    address_set: &GeoPadAddressSet,
) -> Result<GeoPadMembershipEvaluation, GeoAddressError> {
    if forest.version != CANON_GEO_ADDRESS_PARSE_FOREST_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported address parse forest version",
            [
                ("expected", CANON_GEO_ADDRESS_PARSE_FOREST_VERSION),
                ("actual", forest.version.as_str()),
            ],
        ));
    }
    if address_set.version != CANON_GEO_PAD_ADDRESS_SET_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported PAD address set version",
            [
                ("expected", CANON_GEO_PAD_ADDRESS_SET_VERSION),
                ("actual", address_set.version.as_str()),
            ],
        ));
    }
    if forest.jurisdiction != address_set.jurisdiction {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::JurisdictionMismatch,
            "parse forest and PAD address set jurisdictions differ",
            [
                ("forest", format!("{:?}", forest.jurisdiction)),
                ("address_set", format!("{:?}", address_set.jurisdiction)),
            ],
        ));
    }
    validate_pad_address_set(address_set)?;

    let mut members = address_set.members.clone();
    members.sort_by_key(GeoPadAddressMember::canonical_key);

    let mut results = Vec::with_capacity(forest.candidates.len());
    for candidate in &forest.candidates {
        results.push(evaluate_candidate(candidate, &members)?);
    }

    Ok(GeoPadMembershipEvaluation {
        version: CANON_GEO_PAD_MEMBERSHIP_VERSION.to_string(),
        parse_forest_version: forest.version.clone(),
        address_set_version: address_set.version.clone(),
        grammar: forest.grammar.clone(),
        jurisdiction: forest.jurisdiction.clone(),
        input: forest.input.clone(),
        results,
    })
}

pub fn build_address_parcel_evidence(
    request: &GeoAddressParcelEvidenceRequest,
) -> Result<GeoAddressParcelEvidenceBundle, GeoAddressError> {
    if request.version != CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported address parcel evidence request version",
            [
                (
                    "expected",
                    CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION,
                ),
                ("actual", request.version.as_str()),
            ],
        ));
    }

    let parse_forest = parse_address_forest(&request.parse_request)?;
    let pad_membership = evaluate_pad_membership(&parse_forest, &request.address_set)?;
    let bridge = bridge_pad_membership_to_parcel_observation(
        &parse_forest,
        &request.address_set,
        &pad_membership,
        &request.bridge_request,
    )?;
    let evidence_request = request
        .evidence_request
        .as_ref()
        .map(|template| build_address_parcel_compilation_request(&bridge, template))
        .transpose()?
        .flatten();

    Ok(GeoAddressParcelEvidenceBundle {
        version: CANON_GEO_ADDRESS_PARCEL_EVIDENCE_BUNDLE_VERSION.to_string(),
        request_version: request.version.clone(),
        parse_forest,
        pad_membership,
        bridge,
        evidence_request,
    })
}

pub fn build_address_parcel_compilation_request(
    bridge: &GeoAddressParcelBridge,
    template: &GeoEvidenceCompilationRequest,
) -> Result<Option<GeoEvidenceCompilationRequest>, GeoAddressError> {
    if bridge.version != CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported address parcel bridge version",
            [
                ("expected", CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION),
                ("actual", bridge.version.as_str()),
            ],
        ));
    }
    validate_address_evidence_template(template)?;
    if bridge.status == GeoAddressParcelBridgeStatus::DiagnosticAbstention {
        return Ok(None);
    }

    let observation = bridge.observation.clone().ok_or_else(|| {
        GeoAddressError::invalid_input(
            "address parcel bridge marked evidence_observation without an observation",
            [("field", "bridge.observation")],
        )
    })?;
    validate_address_observation_matches_bridge(bridge, &observation)?;
    validate_address_template_contracts(template, &observation.contract_id)?;

    let mut request = template.clone();
    request.observations = vec![observation];
    Ok(Some(canonical_address_evidence_request(&request)?))
}

pub fn bridge_pad_membership_to_parcel_observation(
    forest: &GeoAddressParseForest,
    address_set: &GeoPadAddressSet,
    membership: &GeoPadMembershipEvaluation,
    request: &GeoAddressParcelBridgeRequest,
) -> Result<GeoAddressParcelBridge, GeoAddressError> {
    if request.version != CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported address parcel bridge request version",
            [
                ("expected", CANON_GEO_ADDRESS_PARCEL_BRIDGE_REQUEST_VERSION),
                ("actual", request.version.as_str()),
            ],
        ));
    }
    validate_bridge_identifier("observation_id", &request.observation_id)?;
    validate_bridge_identifier("contract_id", &request.contract_id)?;
    let query_day = request
        .query_as_of
        .as_ref()
        .map(validate_bridge_query_as_of)
        .transpose()?;
    if let Some(interval) = request.valid_time
        && interval.start_day > interval.end_day
    {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge valid-time intervals must be ordered",
            [("field", "valid_time")],
        ));
    }
    if let (Some(query_day), Some(interval)) = (query_day, request.valid_time)
        && (query_day < interval.start_day || query_day > interval.end_day)
    {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge query_as_of must fall inside valid_time",
            [
                ("query_day", query_day.to_string()),
                ("valid_time_start_day", interval.start_day.to_string()),
                ("valid_time_end_day", interval.end_day.to_string()),
            ],
        ));
    }
    if membership.version != CANON_GEO_PAD_MEMBERSHIP_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported PAD membership evaluation version",
            [
                ("expected", CANON_GEO_PAD_MEMBERSHIP_VERSION),
                ("actual", membership.version.as_str()),
            ],
        ));
    }

    let replayed = evaluate_pad_membership(forest, address_set)?;
    if canonical_pad_membership_bytes(&replayed)? != canonical_pad_membership_bytes(membership)? {
        return Err(GeoAddressError::invalid_input(
            "PAD membership evaluation does not replay from the supplied parse forest and address set",
            [("field", "membership")],
        ));
    }

    let mut members_by_id = BTreeMap::new();
    for member in &address_set.members {
        members_by_id.insert(member.member_id.clone(), member);
    }

    let mut records_by_member: BTreeMap<String, BTreeMap<String, GeoEvidenceRecordRef>> =
        BTreeMap::new();
    for binding in &request.member_source_records {
        validate_bridge_identifier("member_source_records[].member_id", &binding.member_id)?;
        validate_bridge_source_record(&binding.source_record)?;
        let Some(member) = members_by_id.get(&binding.member_id) else {
            return Err(GeoAddressError::invalid_input(
                "address parcel bridge source-record binding references an unknown PAD member",
                [("member_id", binding.member_id.as_str())],
            ));
        };
        let expected_member_blake3 = geo_pad_member_blake3(member)?;
        if binding.normalized_member_blake3 != expected_member_blake3 {
            return Err(GeoAddressError::invalid_input(
                "address parcel bridge source-record binding does not match the normalized PAD member payload",
                [
                    ("member_id", binding.member_id.as_str()),
                    ("expected_member_blake3", expected_member_blake3.as_str()),
                    (
                        "actual_member_blake3",
                        binding.normalized_member_blake3.as_str(),
                    ),
                ],
            ));
        }
        insert_bridge_source_record(
            records_by_member
                .entry(binding.member_id.clone())
                .or_default(),
            binding.source_record.clone(),
        )?;
    }

    let mut canonical_results = replayed.results;
    canonical_results.sort_by(|left, right| left.candidate_key.cmp(&right.candidate_key));

    let mut readings = Vec::with_capacity(canonical_results.len());
    let mut parcel_candidates = BTreeMap::<String, GeoEntityRef>::new();
    let mut observation_source_records = BTreeMap::<String, GeoEvidenceRecordRef>::new();
    let mut matched_member_ids_without_source_records = BTreeSet::new();

    for result in canonical_results {
        let mut matched_member_ids = result.matched_member_ids.clone();
        matched_member_ids.sort();
        matched_member_ids.dedup();
        let mut source_supported_member_ids = BTreeSet::new();
        let mut parcel_ids = BTreeSet::new();
        let mut source_record_ids = BTreeSet::new();

        if result.asserted_member {
            for member_id in &matched_member_ids {
                let Some(source_records) = records_by_member.get(member_id) else {
                    matched_member_ids_without_source_records.insert(member_id.clone());
                    continue;
                };
                let member = members_by_id
                    .get(member_id)
                    .expect("membership replay only emits known PAD member ids");
                validate_bridge_identifier("address_set.members[].lot_id", &member.lot_id)?;
                source_supported_member_ids.insert(member_id.clone());
                parcel_ids.insert(member.lot_id.clone());
                for source_record in source_records.values() {
                    source_record_ids.insert(source_record.source_record_id.clone());
                    insert_bridge_source_record(
                        &mut observation_source_records,
                        source_record.clone(),
                    )?;
                }
            }
        }

        for parcel_id in &parcel_ids {
            parcel_candidates.insert(
                parcel_id.clone(),
                GeoEntityRef::new(GeoEntityLevel::Parcel, parcel_id.clone()),
            );
        }

        readings.push(GeoAddressParcelReadingSupport {
            reading_id: result.candidate.reading_id,
            candidate_key: result.candidate_key,
            status: result.status,
            asserted_member: result.asserted_member,
            matched_member_ids,
            source_supported_member_ids: source_supported_member_ids.into_iter().collect(),
            parcel_ids: parcel_ids.into_iter().collect(),
            source_record_ids: source_record_ids.into_iter().collect(),
        });
    }

    let parcel_candidates = parcel_candidates.into_values().collect::<Vec<_>>();
    let source_records = observation_source_records.into_values().collect::<Vec<_>>();
    let observation = if parcel_candidates.is_empty() {
        None
    } else {
        Some(GeoRhoObservation {
            id: request.observation_id.clone(),
            contract_id: request.contract_id.clone(),
            source_records: source_records.clone(),
            valid_time: request.valid_time,
            observation: GeoRhoObservationKind::ExistentialMembership {
                members: parcel_candidates.clone(),
            },
        })
    };
    let diagnostic = if observation.is_some() {
        None
    } else {
        Some(address_parcel_bridge_diagnostic(
            forest,
            &readings,
            matched_member_ids_without_source_records
                .into_iter()
                .collect(),
        ))
    };

    Ok(GeoAddressParcelBridge {
        version: CANON_GEO_ADDRESS_PARCEL_BRIDGE_VERSION.to_string(),
        request_version: request.version.clone(),
        parse_forest_version: forest.version.clone(),
        pad_membership_version: membership.version.clone(),
        grammar: forest.grammar.clone(),
        jurisdiction: forest.jurisdiction.clone(),
        input: forest.input.clone(),
        query_as_of: request.query_as_of.clone(),
        status: if observation.is_some() {
            GeoAddressParcelBridgeStatus::EvidenceObservation
        } else {
            GeoAddressParcelBridgeStatus::DiagnosticAbstention
        },
        readings,
        parcel_candidates,
        source_records,
        valid_time: request.valid_time,
        observation,
        diagnostic,
    })
}

pub fn canonical_address_parse_forest_bytes(
    forest: &GeoAddressParseForest,
) -> Result<Vec<u8>, GeoAddressError> {
    let mut canonical = forest.clone();
    canonical.candidates.sort_by(|left, right| {
        left.canonical_key
            .cmp(&right.canonical_key)
            .then(left.reading_id.cmp(&right.reading_id))
    });
    for (index, candidate) in canonical.candidates.iter_mut().enumerate() {
        candidate.reading_id = format!("reading:{:04}", index + 1);
        candidate.annotations.sort();
        candidate.annotations.dedup();
    }
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoAddressError::invalid_input(
            "failed to serialize canonical address parse forest",
            [("serde", error.to_string())],
        )
    })
}

pub fn geo_pad_member_blake3(member: &GeoPadAddressMember) -> Result<String, GeoAddressError> {
    serde_json::to_vec(member)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| {
            GeoAddressError::invalid_input(
                "failed to hash normalized PAD member payload",
                [("serde", error.to_string())],
            )
        })
}

pub fn canonical_address_parcel_bridge_bytes(
    bridge: &GeoAddressParcelBridge,
) -> Result<Vec<u8>, GeoAddressError> {
    let mut canonical = bridge.clone();
    canonical.readings.sort_by(|left, right| {
        left.candidate_key
            .cmp(&right.candidate_key)
            .then(left.reading_id.cmp(&right.reading_id))
    });
    for reading in &mut canonical.readings {
        reading.matched_member_ids.sort();
        reading.matched_member_ids.dedup();
        reading.source_supported_member_ids.sort();
        reading.source_supported_member_ids.dedup();
        reading.parcel_ids.sort();
        reading.parcel_ids.dedup();
        reading.source_record_ids.sort();
        reading.source_record_ids.dedup();
    }
    canonical.parcel_candidates.sort();
    canonical.parcel_candidates.dedup();
    canonical.source_records.sort();
    canonical.source_records.dedup();
    if let Some(observation) = &mut canonical.observation {
        observation.source_records.sort();
        observation.source_records.dedup();
        if let GeoRhoObservationKind::ExistentialMembership { members } =
            &mut observation.observation
        {
            members.sort();
            members.dedup();
        }
    }
    if let Some(diagnostic) = &mut canonical.diagnostic {
        diagnostic.candidate_keys.sort();
        diagnostic.candidate_keys.dedup();
        diagnostic.matched_member_ids_without_source_records.sort();
        diagnostic.matched_member_ids_without_source_records.dedup();
    }
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoAddressError::invalid_input(
            "failed to serialize canonical address parcel bridge",
            [("serde", error.to_string())],
        )
    })
}

fn validate_address_evidence_template(
    template: &GeoEvidenceCompilationRequest,
) -> Result<(), GeoAddressError> {
    if template.version != CANON_GEO_EVIDENCE_REQUEST_VERSION {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::UnsupportedVersion,
            "unsupported Geo evidence request version in address compile envelope",
            [
                ("expected", CANON_GEO_EVIDENCE_REQUEST_VERSION),
                ("actual", template.version.as_str()),
            ],
        ));
    }
    if !template.observations.is_empty() {
        return Err(GeoAddressError::invalid_input(
            "address compile envelope observations must be empty before bridge materialization",
            [("field", "evidence_request.observations")],
        ));
    }
    compile_evidence(template)
        .map(|_| ())
        .map_err(address_evidence_error)
}

fn validate_address_template_contracts(
    template: &GeoEvidenceCompilationRequest,
    contract_id: &str,
) -> Result<(), GeoAddressError> {
    if template.contracts.len() != 1 {
        let count = template.contracts.len().to_string();
        return Err(GeoAddressError::invalid_input(
            "address compile envelope must carry exactly one rho contract",
            [
                ("field", "evidence_request.contracts"),
                ("count", count.as_str()),
            ],
        ));
    }
    if template.contracts[0].id != contract_id {
        return Err(GeoAddressError::invalid_input(
            "address compile envelope rho contract does not match the bridge observation",
            [
                ("expected_contract_id", contract_id),
                ("actual_contract_id", template.contracts[0].id.as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_address_observation_matches_bridge(
    bridge: &GeoAddressParcelBridge,
    observation: &GeoRhoObservation,
) -> Result<(), GeoAddressError> {
    let GeoRhoObservationKind::ExistentialMembership { members } = &observation.observation else {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge observations must be existential parcel membership",
            [("field", "bridge.observation.observation.kind")],
        ));
    };
    let bridge_members = bridge
        .parcel_candidates
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let observation_members = members.iter().cloned().collect::<BTreeSet<_>>();
    if observation_members.is_empty() || observation_members != bridge_members {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge observation members do not match parcel candidates",
            [("field", "bridge.observation.observation.members")],
        ));
    }
    if observation_members
        .iter()
        .any(|member| member.level != GeoEntityLevel::Parcel)
    {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge observations must remain parcel-grain",
            [("field", "bridge.observation.observation.members")],
        ));
    }

    let bridge_records = bridge
        .source_records
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let observation_records = observation
        .source_records
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if bridge_records.is_empty() || observation_records != bridge_records {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge observation source records do not match the bridge audit",
            [("field", "bridge.observation.source_records")],
        ));
    }
    Ok(())
}

fn canonical_address_evidence_request(
    request: &GeoEvidenceCompilationRequest,
) -> Result<GeoEvidenceCompilationRequest, GeoAddressError> {
    let compiled = compile_evidence(request).map_err(address_evidence_error)?;
    let mut contracts = BTreeMap::<String, GeoRhoContract>::new();
    let observations = compiled
        .admissions
        .iter()
        .map(|admission| {
            contracts
                .entry(admission.contract.id.clone())
                .or_insert_with(|| admission.contract.clone());
            GeoRhoObservation {
                id: admission.observation_id.clone(),
                contract_id: admission.contract.id.clone(),
                source_records: admission.source_records.clone(),
                valid_time: admission.valid_time,
                observation: admission.observation.clone(),
            }
        })
        .collect::<Vec<_>>();

    Ok(GeoEvidenceCompilationRequest {
        version: compiled.request_version,
        profile: compiled.composition_request.profile,
        universe: compiled.composition_request.universe,
        contracts: contracts.into_values().collect(),
        observations,
        max_assignments: compiled.composition_request.max_assignments,
        max_materialized_models: compiled.composition_request.max_materialized_models,
    })
}

fn canonical_address_bundle_evidence_request(
    bridge: &GeoAddressParcelBridge,
    request: &GeoEvidenceCompilationRequest,
) -> Result<GeoEvidenceCompilationRequest, GeoAddressError> {
    if bridge.status == GeoAddressParcelBridgeStatus::DiagnosticAbstention {
        return Err(GeoAddressError::invalid_input(
            "diagnostic address parcel evidence bundles cannot carry a compile request",
            [("field", "evidence_request")],
        ));
    }
    let bridge_observation = bridge.observation.as_ref().ok_or_else(|| {
        GeoAddressError::invalid_input(
            "address bundle bridge must carry an observation when evidence_request is present",
            [("field", "bridge.observation")],
        )
    })?;
    if request.observations.len() != 1 {
        let count = request.observations.len().to_string();
        return Err(GeoAddressError::invalid_input(
            "address bundle evidence request must carry exactly one bridge observation",
            [
                ("field", "evidence_request.observations"),
                ("count", count.as_str()),
            ],
        ));
    }
    validate_address_template_contracts(request, bridge_observation.contract_id.as_str())?;
    validate_address_observation_matches_bridge(bridge, &request.observations[0])?;

    let canonical_request = canonical_address_evidence_request(request)?;
    validate_address_template_contracts(
        &canonical_request,
        bridge_observation.contract_id.as_str(),
    )?;
    validate_address_observation_matches_bridge(bridge, &canonical_request.observations[0])?;
    Ok(canonical_request)
}

fn address_evidence_error(error: GeoEvidenceError) -> GeoAddressError {
    let mut detail = error.detail;
    detail.insert(
        "geo_evidence_error_code".to_string(),
        format!("{:?}", error.code),
    );
    GeoAddressError::invalid_input(
        format!(
            "address compile envelope does not produce a valid Geo evidence request: {}",
            error.message
        ),
        detail,
    )
}

pub fn canonical_address_parcel_evidence_bundle_bytes(
    bundle: &GeoAddressParcelEvidenceBundle,
) -> Result<Vec<u8>, GeoAddressError> {
    let mut canonical = bundle.clone();
    canonical.parse_forest = serde_json::from_slice(&canonical_address_parse_forest_bytes(
        &canonical.parse_forest,
    )?)
    .map_err(|error| {
        GeoAddressError::invalid_input(
            "failed to rebuild canonical address parse forest",
            [("serde", error.to_string())],
        )
    })?;
    canonical.pad_membership =
        serde_json::from_slice(&canonical_pad_membership_bytes(&canonical.pad_membership)?)
            .map_err(|error| {
                GeoAddressError::invalid_input(
                    "failed to rebuild canonical PAD membership evaluation",
                    [("serde", error.to_string())],
                )
            })?;
    canonical.bridge =
        serde_json::from_slice(&canonical_address_parcel_bridge_bytes(&canonical.bridge)?)
            .map_err(|error| {
                GeoAddressError::invalid_input(
                    "failed to rebuild canonical address parcel bridge",
                    [("serde", error.to_string())],
                )
            })?;
    if let Some(evidence_request) = canonical.evidence_request.take() {
        canonical.evidence_request = Some(canonical_address_bundle_evidence_request(
            &canonical.bridge,
            &evidence_request,
        )?);
    }
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoAddressError::invalid_input(
            "failed to serialize canonical address parcel evidence bundle",
            [("serde", error.to_string())],
        )
    })
}

pub fn canonical_pad_membership_bytes(
    evaluation: &GeoPadMembershipEvaluation,
) -> Result<Vec<u8>, GeoAddressError> {
    let mut canonical = evaluation.clone();
    canonical
        .results
        .sort_by(|left, right| left.candidate_key.cmp(&right.candidate_key));
    for result in &mut canonical.results {
        result.matched_member_ids.sort();
        result.matched_member_ids.dedup();
        result.compatible_member_ids.sort();
        result.compatible_member_ids.dedup();
        result.reasons.sort();
        result.reasons.dedup();
    }
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoAddressError::invalid_input(
            "failed to serialize canonical PAD membership evaluation",
            [("serde", error.to_string())],
        )
    })
}

fn address_parcel_bridge_diagnostic(
    forest: &GeoAddressParseForest,
    readings: &[GeoAddressParcelReadingSupport],
    matched_member_ids_without_source_records: Vec<String>,
) -> GeoAddressParcelDiagnostic {
    let candidate_keys = readings
        .iter()
        .map(|reading| reading.candidate_key.clone())
        .collect::<Vec<_>>();
    let any_asserted_member = readings.iter().any(|reading| reading.asserted_member);
    let code = if forest.candidates.is_empty() {
        GeoAddressParcelDiagnosticCode::NoParseReadings
    } else if any_asserted_member && !matched_member_ids_without_source_records.is_empty() {
        GeoAddressParcelDiagnosticCode::NoBoundSourceRecords
    } else {
        GeoAddressParcelDiagnosticCode::NoSourceMemberSupport
    };
    let message = match code {
        GeoAddressParcelDiagnosticCode::NoParseReadings => {
            "address parse forest has no candidate readings"
        }
        GeoAddressParcelDiagnosticCode::NoSourceMemberSupport => {
            "no address reading has source-member support"
        }
        GeoAddressParcelDiagnosticCode::NoBoundSourceRecords => {
            "supported address readings lack immutable source-record bindings"
        }
    };
    GeoAddressParcelDiagnostic {
        code,
        message: message.to_string(),
        candidate_keys,
        matched_member_ids_without_source_records,
    }
}

fn insert_bridge_source_record(
    target: &mut BTreeMap<String, GeoEvidenceRecordRef>,
    record: GeoEvidenceRecordRef,
) -> Result<(), GeoAddressError> {
    if let Some(existing) = target.get(&record.source_record_id) {
        if existing != &record {
            return Err(GeoAddressError::invalid_input(
                "source-record bindings reuse an id with different metadata",
                [("source_record_id", record.source_record_id.as_str())],
            ));
        }
        return Ok(());
    }
    target.insert(record.source_record_id.clone(), record);
    Ok(())
}

fn validate_bridge_source_record(record: &GeoEvidenceRecordRef) -> Result<(), GeoAddressError> {
    validate_bridge_identifier(
        "member_source_records[].source_record.source_record_id",
        &record.source_record_id,
    )?;
    validate_bridge_identifier(
        "member_source_records[].source_record.source_vintage",
        &record.source_vintage,
    )?;
    if record.record_blake3.len() != 64
        || !record
            .record_blake3
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge source-record digests must be 64 lowercase BLAKE3 hex characters",
            [
                (
                    "field",
                    "member_source_records[].source_record.record_blake3",
                ),
                ("value", record.record_blake3.as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_bridge_query_as_of(as_of: &GeoAsOf) -> Result<i64, GeoAddressError> {
    validate_bridge_identifier("query_as_of.semantic_id", &as_of.semantic_id)?;
    if as_of.unit != "utc_day" {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge query_as_of unit must be utc_day",
            [("unit", as_of.unit.as_str())],
        ));
    }
    let day = chrono::NaiveDate::parse_from_str(&as_of.utc_day, "%Y-%m-%d").map_err(|error| {
        GeoAddressError::invalid_input(
            "address parcel bridge query_as_of must be a valid YYYY-MM-DD UTC day",
            [
                ("utc_day".to_string(), as_of.utc_day.clone()),
                ("error".to_string(), error.to_string()),
            ],
        )
    })?;
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("Unix epoch is valid");
    Ok(day.signed_duration_since(epoch).num_days())
}

fn validate_bridge_identifier(field: &str, value: &str) -> Result<(), GeoAddressError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoAddressError::invalid_input(
            "address parcel bridge identifiers must be non-empty and already canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct PendingCandidate {
    canonical_key: String,
    display: String,
    source_segment: String,
    house: GeoAddressHouseNumber,
    street: GeoAddressStreet,
    annotations: Vec<GeoAddressAnnotation>,
}

impl PendingCandidate {
    fn new(
        source_segment: String,
        house: GeoAddressHouseNumber,
        street: GeoAddressStreet,
        annotations: Vec<GeoAddressAnnotation>,
    ) -> Self {
        let canonical_key = format!("{}|{}", house.canonical_key(), street.canonical_key());
        let display = format!("{} {}", house.display(), street.display());
        Self {
            canonical_key,
            display,
            source_segment,
            house,
            street,
            annotations: canonical_annotations(annotations),
        }
    }

    fn merge_annotations(&mut self, annotations: &[GeoAddressAnnotation]) {
        self.annotations.extend_from_slice(annotations);
        self.annotations = canonical_annotations(std::mem::take(&mut self.annotations));
    }

    fn materialize(self, index: usize) -> GeoAddressCandidate {
        GeoAddressCandidate {
            reading_id: format!("reading:{index:04}"),
            canonical_key: self.canonical_key,
            display: self.display,
            source_grammar: GeoAddressGrammarRef::default(),
            source_segment: self.source_segment,
            house: self.house,
            street: self.street,
            annotations: self.annotations,
        }
    }
}

fn parse_segment(
    segment: &str,
    borough: GeoNycBorough,
    has_aka: bool,
) -> Result<Vec<PendingCandidate>, GeoAddressError> {
    let tokens = truncate_secondary_units(lex_segment(segment));
    let mut cursor = 0;
    let mut candidates = Vec::new();
    while cursor < tokens.len() {
        while cursor < tokens.len() && is_group_separator(&tokens[cursor]) {
            cursor += 1;
        }
        if cursor >= tokens.len() {
            break;
        }
        let mut house_terms = Vec::new();
        let mut saw_house_separator = false;
        loop {
            if cursor >= tokens.len() {
                break;
            }
            if !looks_like_house_token(&tokens[cursor]) {
                return Err(GeoAddressError::unsupported_pattern(
                    "expected a house number at the start of an address group",
                    [
                        ("segment", segment.to_string()),
                        ("token", tokens[cursor].clone()),
                    ],
                ));
            }
            house_terms.push(parse_house_token(&tokens[cursor], borough)?);
            cursor += 1;
            let separator_start = cursor;
            let mut consumed_separator = false;
            while cursor < tokens.len() && is_group_separator(&tokens[cursor]) {
                consumed_separator = true;
                cursor += 1;
            }
            if consumed_separator
                && cursor < tokens.len()
                && looks_like_house_token(&tokens[cursor])
            {
                saw_house_separator = true;
                continue;
            }
            cursor = separator_start;
            break;
        }

        while cursor < tokens.len() && is_group_separator(&tokens[cursor]) {
            cursor += 1;
        }
        let street_start = cursor;
        let mut street_end = tokens.len();
        while cursor < tokens.len() {
            if is_group_separator(&tokens[cursor]) {
                let mut next = cursor + 1;
                while next < tokens.len() && is_group_separator(&tokens[next]) {
                    next += 1;
                }
                if next < tokens.len() && looks_like_house_token(&tokens[next]) {
                    street_end = cursor;
                    break;
                }
            }
            cursor += 1;
        }
        if street_start >= street_end {
            return Err(GeoAddressError::new(
                GeoAddressErrorCode::InvalidStreet,
                "address group has no street tokens",
                [("segment", segment.to_string())],
            ));
        }
        let street_tokens = tokens[street_start..street_end].to_vec();
        let street_variants = parse_street_variants(&street_tokens)?;
        for house in house_terms {
            let mut annotations = house.annotations.clone();
            if has_aka {
                annotations.push(GeoAddressAnnotation::AkaAlternative);
            }
            if saw_house_separator {
                annotations.push(GeoAddressAnnotation::EnumeratedHouseNumber);
            }
            for street in &street_variants {
                let mut candidate_annotations = annotations.clone();
                candidate_annotations.extend(street.annotations.iter().cloned());
                candidates.push(PendingCandidate::new(
                    segment.to_string(),
                    house.house.clone(),
                    street.street.clone(),
                    candidate_annotations,
                ));
            }
        }
        cursor = street_end;
    }

    Ok(candidates)
}

#[derive(Debug, Clone)]
struct ParsedHouse {
    house: GeoAddressHouseNumber,
    annotations: Vec<GeoAddressAnnotation>,
}

#[derive(Debug, Clone)]
struct ParsedStreet {
    street: GeoAddressStreet,
    annotations: Vec<GeoAddressAnnotation>,
}

fn parse_house_token(token: &str, borough: GeoNycBorough) -> Result<ParsedHouse, GeoAddressError> {
    if token.contains('/') {
        let numbers = parse_numeric_parts(token, '/')?;
        if numbers.len() != 2 {
            return Err(GeoAddressError::new(
                GeoAddressErrorCode::InvalidHouseNumber,
                "slash address ranges must have exactly two endpoints",
                [("token", token.to_string())],
            ));
        }
        return Ok(ParsedHouse {
            house: GeoAddressHouseNumber::range(
                numbers[0],
                numbers[1],
                infer_range_parity(numbers[0], numbers[1]),
                GeoAddressRangeOperator::Slash,
                numbers.clone(),
            )?,
            annotations: vec![GeoAddressAnnotation::SlashRange],
        });
    }

    if token.contains('-') {
        let parts = parse_numeric_parts(token, '-')?;
        if parts.len() == 2 && borough == GeoNycBorough::Queens {
            return Ok(ParsedHouse {
                house: GeoAddressHouseNumber::queens_hyphenated_literal(token),
                annotations: vec![GeoAddressAnnotation::QueensHyphenateLiteral],
            });
        }
        if parts.len() < 2 {
            return Err(GeoAddressError::new(
                GeoAddressErrorCode::InvalidHouseNumber,
                "dash address range must contain at least two numbers",
                [("token", token.to_string())],
            ));
        }
        let start = parts[0];
        let end = *parts.last().expect("parts is non-empty");
        if parts.windows(2).any(|window| window[0] >= window[1]) {
            return Err(GeoAddressError::new(
                GeoAddressErrorCode::InvalidHouseNumber,
                "dash address ranges must be strictly increasing outside Queens",
                [("token", token.to_string())],
            ));
        }
        return Ok(ParsedHouse {
            house: GeoAddressHouseNumber::range(
                start,
                end,
                infer_range_parity(start, end),
                if parts.len() == 2 {
                    GeoAddressRangeOperator::Dash
                } else {
                    GeoAddressRangeOperator::DashList
                },
                parts,
            )?,
            annotations: vec![if token.matches('-').count() == 1 {
                GeoAddressAnnotation::DashRange
            } else {
                GeoAddressAnnotation::DashListRange
            }],
        });
    }

    let value = parse_u32_token(token).ok_or_else(|| {
        GeoAddressError::new(
            GeoAddressErrorCode::InvalidHouseNumber,
            "house number token is not numeric",
            [("token", token.to_string())],
        )
    })?;
    Ok(ParsedHouse {
        house: GeoAddressHouseNumber::Discrete { value },
        annotations: Vec::new(),
    })
}

fn parse_street_variants(tokens: &[String]) -> Result<Vec<ParsedStreet>, GeoAddressError> {
    let filtered = tokens
        .iter()
        .filter(|token| !is_group_separator(token))
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::InvalidStreet,
            "street token sequence is empty",
            [("tokens", format!("{tokens:?}"))],
        ));
    }

    let mut start = 0;
    let pre_direction = parse_direction(&filtered[0]);
    if pre_direction.is_some() {
        start = 1;
    }
    let mut end = filtered.len();
    let suffix = if end > start {
        parse_suffix(&filtered[end - 1])
    } else {
        None
    };
    if suffix.is_some() {
        end -= 1;
    }
    if start >= end {
        return Err(GeoAddressError::new(
            GeoAddressErrorCode::InvalidStreet,
            "street name is missing after direction/suffix tokens",
            [("tokens", format!("{filtered:?}"))],
        ));
    }

    let name = filtered[start..end]
        .iter()
        .map(|token| {
            parse_ordinal(token).map_or_else(
                || GeoStreetNameToken::Literal {
                    value: canonical_literal_token(token),
                },
                |value| GeoStreetNameToken::Ordinal { value },
            )
        })
        .collect::<Vec<_>>();
    let base = GeoAddressStreet::new(pre_direction, name.clone(), suffix)?;
    let mut variants = vec![ParsedStreet {
        street: base,
        annotations: Vec::new(),
    }];

    if suffix.is_none()
        && pre_direction.is_some()
        && name
            .iter()
            .all(|token| matches!(token, GeoStreetNameToken::Ordinal { .. }))
    {
        variants.push(ParsedStreet {
            street: GeoAddressStreet::new(pre_direction, name, Some(GeoStreetSuffix::Street))?,
            annotations: vec![GeoAddressAnnotation::ImplicitOrdinalStreetSuffix],
        });
    }

    Ok(variants)
}

fn evaluate_candidate(
    candidate: &GeoAddressCandidate,
    members: &[GeoPadAddressMember],
) -> Result<GeoPadCandidateEvaluation, GeoAddressError> {
    let mut exact_ids = BTreeSet::new();
    let mut range_ids = BTreeSet::new();
    let mut compatible_ids = BTreeSet::new();
    let mut reasons = BTreeSet::new();

    for member in members {
        let street_relation = street_relation(&candidate.street, &member.street);
        let Some(street_relation) = street_relation else {
            continue;
        };
        let house_relation = house_relation(&candidate.house, &member.house)?;
        let Some(house_relation) = house_relation else {
            continue;
        };

        compatible_ids.insert(member.member_id.clone());
        match street_relation {
            StreetRelation::Exact => {
                reasons.insert(GeoPadCompatibilityReason::ExactStreet);
            }
            StreetRelation::QuerySuffixUnspecified => {
                reasons.insert(GeoPadCompatibilityReason::QuerySuffixUnspecified);
            }
        }
        match house_relation {
            HouseRelation::Exact => {
                exact_ids.insert(member.member_id.clone());
                reasons.insert(GeoPadCompatibilityReason::ExactHouseNumber);
            }
            HouseRelation::ContainedByPadRange => {
                range_ids.insert(member.member_id.clone());
                reasons.insert(GeoPadCompatibilityReason::PadRangeContainsQuery);
            }
            HouseRelation::Compatible => {}
        }
    }

    let covered = if exact_ids.is_empty() && range_ids.is_empty() {
        query_range_covered_by_member_set(candidate, members)?
    } else {
        false
    };
    if covered {
        reasons.insert(GeoPadCompatibilityReason::QueryRangeCoveredByMembers);
    }

    let status = if !exact_ids.is_empty() {
        GeoPadCandidateStatus::ExactMember
    } else if !range_ids.is_empty() {
        GeoPadCandidateStatus::RangeContained
    } else if covered {
        GeoPadCandidateStatus::CoveredByAddressSet
    } else if !compatible_ids.is_empty() {
        GeoPadCandidateStatus::CompatibleOnly
    } else {
        GeoPadCandidateStatus::NoSourceMember
    };
    let asserted_member = matches!(
        status,
        GeoPadCandidateStatus::ExactMember
            | GeoPadCandidateStatus::RangeContained
            | GeoPadCandidateStatus::CoveredByAddressSet
    );
    let compatible = asserted_member || !compatible_ids.is_empty();
    let mut matched_member_ids = exact_ids
        .iter()
        .chain(range_ids.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    if covered {
        for member in members {
            if street_relation(&candidate.street, &member.street) == Some(StreetRelation::Exact)
                && house_relation(&candidate.house, &member.house)?.is_some()
            {
                matched_member_ids.insert(member.member_id.clone());
            }
        }
    }

    Ok(GeoPadCandidateEvaluation {
        candidate_key: candidate.canonical_key.clone(),
        candidate: candidate.clone(),
        status,
        asserted_member,
        compatible,
        matched_member_ids: matched_member_ids.into_iter().collect(),
        compatible_member_ids: compatible_ids.into_iter().collect(),
        reasons: reasons.into_iter().collect(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreetRelation {
    Exact,
    QuerySuffixUnspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HouseRelation {
    Exact,
    ContainedByPadRange,
    Compatible,
}

fn street_relation(query: &GeoAddressStreet, pad: &GeoAddressStreet) -> Option<StreetRelation> {
    if query == pad {
        return Some(StreetRelation::Exact);
    }
    if query.pre_direction == pad.pre_direction
        && query.name == pad.name
        && query.suffix.is_none()
        && pad.suffix.is_some()
    {
        return Some(StreetRelation::QuerySuffixUnspecified);
    }
    None
}

fn house_relation(
    query: &GeoAddressHouseNumber,
    pad: &GeoAddressHouseNumber,
) -> Result<Option<HouseRelation>, GeoAddressError> {
    match (query, pad) {
        (
            GeoAddressHouseNumber::Discrete { value: query },
            GeoAddressHouseNumber::Discrete { value: pad },
        ) if query == pad => Ok(Some(HouseRelation::Exact)),
        (
            GeoAddressHouseNumber::HyphenatedLiteral { value: query },
            GeoAddressHouseNumber::HyphenatedLiteral { value: pad },
        ) if query == pad => Ok(Some(HouseRelation::Exact)),
        (
            GeoAddressHouseNumber::Discrete { value },
            GeoAddressHouseNumber::Range {
                start, end, parity, ..
            },
        ) if start <= value && value <= end && parity_accepts(*parity, *value) => {
            Ok(Some(HouseRelation::ContainedByPadRange))
        }
        (
            GeoAddressHouseNumber::Range {
                start: query_start,
                end: query_end,
                parity: query_parity,
                ..
            },
            GeoAddressHouseNumber::Range {
                start: pad_start,
                end: pad_end,
                parity: pad_parity,
                ..
            },
        ) if pad_start <= query_start
            && query_end <= pad_end
            && parity_contains(*pad_parity, *query_parity) =>
        {
            Ok(Some(HouseRelation::ContainedByPadRange))
        }
        (GeoAddressHouseNumber::Range { .. }, GeoAddressHouseNumber::Discrete { value }) => {
            if query
                .required_numeric_members()?
                .is_some_and(|members| members.binary_search(value).is_ok())
            {
                Ok(Some(HouseRelation::Compatible))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn query_range_covered_by_member_set(
    candidate: &GeoAddressCandidate,
    members: &[GeoPadAddressMember],
) -> Result<bool, GeoAddressError> {
    let Some(required) = candidate.house.required_numeric_members()? else {
        return Ok(false);
    };
    if !matches!(candidate.house, GeoAddressHouseNumber::Range { .. }) {
        return Ok(false);
    }
    for value in required {
        let mut found = false;
        for member in members {
            if street_relation(&candidate.street, &member.street) != Some(StreetRelation::Exact) {
                continue;
            }
            if house_relation(&GeoAddressHouseNumber::Discrete { value }, &member.house)?.is_some()
            {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_pad_address_set(address_set: &GeoPadAddressSet) -> Result<(), GeoAddressError> {
    address_set.jurisdiction.required_borough()?;
    let mut ids = BTreeSet::new();
    for member in &address_set.members {
        if member.member_id.trim().is_empty() {
            return Err(GeoAddressError::new(
                GeoAddressErrorCode::InvalidPadAddressSet,
                "PAD address member id is empty",
                [("lot_id", member.lot_id.as_str())],
            ));
        }
        if !ids.insert(member.member_id.clone()) {
            return Err(GeoAddressError::new(
                GeoAddressErrorCode::InvalidPadAddressSet,
                "PAD address member ids must be unique",
                [("member_id", member.member_id.as_str())],
            ));
        }
    }
    Ok(())
}

fn normalize_input(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => {
                normalized.push('-')
            }
            ',' | '&' | '+' => {
                normalized.push(' ');
                normalized.push(ch);
                normalized.push(' ');
            }
            '.' | ';' | ':' | '(' | ')' | '[' | ']' => normalized.push(' '),
            _ if ch.is_whitespace() => normalized.push(' '),
            _ => normalized.push(ch.to_ascii_lowercase()),
        }
    }
    normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" also known as ", " a/k/a ")
        .replace(" aka ", " a/k/a ")
}

fn split_alternatives(normalized: &str) -> Vec<String> {
    normalized
        .split(" a/k/a ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn lex_segment(segment: &str) -> Vec<String> {
    segment
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
}

fn truncate_secondary_units(tokens: Vec<String>) -> Vec<String> {
    let mut truncated = Vec::new();
    for token in tokens {
        if matches!(
            token.as_str(),
            "unit" | "apt" | "apartment" | "suite" | "ste" | "floor" | "fl" | "#"
        ) {
            break;
        }
        truncated.push(token);
    }
    while truncated.last().is_some_and(|token| token == ",") {
        truncated.pop();
    }
    truncated
}

fn detect_placeholder(normalized: &str) -> Option<GeoAddressPlaceholder> {
    let token = normalized.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
    let kind = match token {
        "various" | "various addresses" | "multiple addresses various" => {
            GeoAddressPlaceholderKind::Various
        }
        "multiple" | "multiple addresses" => GeoAddressPlaceholderKind::Multiple,
        "unknown" | "n/a" | "na" => GeoAddressPlaceholderKind::Unknown,
        "tbd" | "to be determined" => GeoAddressPlaceholderKind::ToBeDetermined,
        _ => return None,
    };
    Some(GeoAddressPlaceholder {
        kind,
        token: normalized.to_string(),
    })
}

fn is_group_separator(token: &str) -> bool {
    matches!(token, "," | "&" | "+" | "and")
}

fn looks_like_house_token(token: &str) -> bool {
    token
        .chars()
        .all(|ch| ch.is_ascii_digit() || ch == '-' || ch == '/')
        && token.chars().any(|ch| ch.is_ascii_digit())
}

fn parse_numeric_parts(token: &str, separator: char) -> Result<Vec<u32>, GeoAddressError> {
    let mut parts = Vec::new();
    for part in token.split(separator) {
        let value = parse_u32_token(part).ok_or_else(|| {
            GeoAddressError::new(
                GeoAddressErrorCode::InvalidHouseNumber,
                "house range endpoint is not numeric",
                [("token", token.to_string()), ("endpoint", part.to_string())],
            )
        })?;
        parts.push(value);
    }
    Ok(parts)
}

fn parse_u32_token(token: &str) -> Option<u32> {
    if token.is_empty() || !token.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    token.parse::<u32>().ok()
}

fn parse_direction(token: &str) -> Option<GeoStreetDirection> {
    match token {
        "n" | "north" => Some(GeoStreetDirection::North),
        "s" | "south" => Some(GeoStreetDirection::South),
        "e" | "east" => Some(GeoStreetDirection::East),
        "w" | "west" => Some(GeoStreetDirection::West),
        _ => None,
    }
}

fn parse_suffix(token: &str) -> Option<GeoStreetSuffix> {
    match token {
        "ave" | "av" | "avenue" => Some(GeoStreetSuffix::Avenue),
        "blvd" | "boulevard" => Some(GeoStreetSuffix::Boulevard),
        "ct" | "court" => Some(GeoStreetSuffix::Court),
        "dr" | "drive" => Some(GeoStreetSuffix::Drive),
        "ln" | "lane" => Some(GeoStreetSuffix::Lane),
        "pl" | "place" => Some(GeoStreetSuffix::Place),
        "rd" | "road" => Some(GeoStreetSuffix::Road),
        "st" | "street" => Some(GeoStreetSuffix::Street),
        "ter" | "terrace" => Some(GeoStreetSuffix::Terrace),
        _ => None,
    }
}

fn parse_ordinal(token: &str) -> Option<u16> {
    match token {
        "first" => Some(1),
        "second" => Some(2),
        "third" => Some(3),
        "fourth" => Some(4),
        "fifth" => Some(5),
        "sixth" => Some(6),
        "seventh" => Some(7),
        "eighth" => Some(8),
        "ninth" => Some(9),
        "tenth" => Some(10),
        "eleventh" => Some(11),
        "twelfth" => Some(12),
        _ => parse_numeric_ordinal(token),
    }
}

fn parse_numeric_ordinal(token: &str) -> Option<u16> {
    let number = token
        .strip_suffix("st")
        .or_else(|| token.strip_suffix("nd"))
        .or_else(|| token.strip_suffix("rd"))
        .or_else(|| token.strip_suffix("th"))?;
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    number.parse::<u16>().ok()
}

fn canonical_literal_token(token: &str) -> String {
    token
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn canonical_numbers(numbers: Vec<u32>) -> Vec<u32> {
    let mut numbers = numbers;
    numbers.sort_unstable();
    numbers.dedup();
    numbers
}

fn canonical_annotations(mut annotations: Vec<GeoAddressAnnotation>) -> Vec<GeoAddressAnnotation> {
    annotations.sort();
    annotations.dedup();
    annotations
}

fn infer_range_parity(start: u32, end: u32) -> GeoAddressParity {
    if start % 2 == end % 2 {
        if start.is_multiple_of(2) {
            GeoAddressParity::Even
        } else {
            GeoAddressParity::Odd
        }
    } else {
        GeoAddressParity::Any
    }
}

fn parity_accepts(parity: GeoAddressParity, value: u32) -> bool {
    match parity {
        GeoAddressParity::Any => true,
        GeoAddressParity::Even => value.is_multiple_of(2),
        GeoAddressParity::Odd => value % 2 == 1,
    }
}

fn parity_contains(container: GeoAddressParity, contained: GeoAddressParity) -> bool {
    container == GeoAddressParity::Any || container == contained
}

fn parity_key(parity: &GeoAddressParity) -> &'static str {
    match parity {
        GeoAddressParity::Any => "any",
        GeoAddressParity::Even => "even",
        GeoAddressParity::Odd => "odd",
    }
}

fn direction_key(direction: GeoStreetDirection) -> &'static str {
    match direction {
        GeoStreetDirection::North => "n",
        GeoStreetDirection::South => "s",
        GeoStreetDirection::East => "e",
        GeoStreetDirection::West => "w",
    }
}

fn direction_display(direction: GeoStreetDirection) -> &'static str {
    match direction {
        GeoStreetDirection::North => "North",
        GeoStreetDirection::South => "South",
        GeoStreetDirection::East => "East",
        GeoStreetDirection::West => "West",
    }
}

fn suffix_key(suffix: GeoStreetSuffix) -> &'static str {
    match suffix {
        GeoStreetSuffix::Avenue => "avenue",
        GeoStreetSuffix::Boulevard => "boulevard",
        GeoStreetSuffix::Court => "court",
        GeoStreetSuffix::Drive => "drive",
        GeoStreetSuffix::Lane => "lane",
        GeoStreetSuffix::Place => "place",
        GeoStreetSuffix::Road => "road",
        GeoStreetSuffix::Street => "street",
        GeoStreetSuffix::Terrace => "terrace",
    }
}

fn suffix_display(suffix: GeoStreetSuffix) -> &'static str {
    match suffix {
        GeoStreetSuffix::Avenue => "Avenue",
        GeoStreetSuffix::Boulevard => "Boulevard",
        GeoStreetSuffix::Court => "Court",
        GeoStreetSuffix::Drive => "Drive",
        GeoStreetSuffix::Lane => "Lane",
        GeoStreetSuffix::Place => "Place",
        GeoStreetSuffix::Road => "Road",
        GeoStreetSuffix::Street => "Street",
        GeoStreetSuffix::Terrace => "Terrace",
    }
}

fn ordinal_display(value: u16) -> String {
    let suffix = match value % 100 {
        11..=13 => "th",
        _ => match value % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{value}{suffix}")
}

fn title_token(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut output = String::new();
    output.push(first.to_ascii_uppercase());
    output.extend(chars);
    output
}
