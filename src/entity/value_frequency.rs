//! Deterministic value-frequency tables for entity support evidence.
//!
//! The table is derived from posting exact-view buckets and hash-bound to the
//! posting index that produced it. Frequency bands are explicit integer strategy
//! input; no learned weights are introduced here.

use crate::{
    entity::{
        postings::{EntityPostingIndex, PostingLayoutError},
        score::ScoreUnits,
    },
    witness,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

pub const CANON_ENTITY_VALUE_FREQUENCY_VERSION: &str = "canon_entity_value_frequency.v0";

pub const FREQUENCY_TABLE_HASH_PARAM: &str = "frequency_table_hash";
pub const FREQUENCY_MINIMUM_COUNT_PARAM: &str = "frequency_minimum_count";
pub const FREQUENCY_RARE_MAX_COUNT_PARAM: &str = "frequency_rare_max_count";
pub const FREQUENCY_UNCOMMON_MAX_COUNT_PARAM: &str = "frequency_uncommon_max_count";
pub const FREQUENCY_COMMON_MAX_COUNT_PARAM: &str = "frequency_common_max_count";
pub const FREQUENCY_RARE_MULTIPLIER_PARAM: &str = "frequency_rare_multiplier_basis_points";
pub const FREQUENCY_UNCOMMON_MULTIPLIER_PARAM: &str = "frequency_uncommon_multiplier_basis_points";
pub const FREQUENCY_COMMON_MULTIPLIER_PARAM: &str = "frequency_common_multiplier_basis_points";
pub const FREQUENCY_VERY_COMMON_MULTIPLIER_PARAM: &str =
    "frequency_very_common_multiplier_basis_points";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityValueFrequencyTable {
    pub version: String,
    pub content_hash: String,
    pub source_posting_index_hash: String,
    pub surface_count: u64,
    pub records: Vec<EntityValueFrequencyRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityValueFrequencyRecord {
    pub term_id: u32,
    pub view_name: String,
    pub value: String,
    pub count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityValueFrequencyBand {
    Rare,
    Uncommon,
    Common,
    VeryCommon,
}

impl EntityValueFrequencyBand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rare => "rare",
            Self::Uncommon => "uncommon",
            Self::Common => "common",
            Self::VeryCommon => "very_common",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityValueFrequencyBandConfig {
    pub minimum_count: u64,
    pub rare_max_count: u64,
    pub uncommon_max_count: u64,
    pub common_max_count: u64,
    pub rare_multiplier_basis_points: u32,
    pub uncommon_multiplier_basis_points: u32,
    pub common_multiplier_basis_points: u32,
    pub very_common_multiplier_basis_points: u32,
}

impl EntityValueFrequencyBandConfig {
    pub fn from_operator_params(
        params: &BTreeMap<String, String>,
    ) -> Result<Option<Self>, EntityValueFrequencyError> {
        Ok(
            EntityValueFrequencyStrategyConfig::from_operator_params(params)?
                .map(|config| config.bands),
        )
    }

    pub fn validate(&self) -> Result<(), EntityValueFrequencyError> {
        if self.minimum_count == 0 {
            return Err(invalid_config(
                FREQUENCY_MINIMUM_COUNT_PARAM,
                "must_be_positive",
                self.minimum_count,
            ));
        }
        if self.rare_max_count == 0 {
            return Err(invalid_config(
                FREQUENCY_RARE_MAX_COUNT_PARAM,
                "must_be_positive",
                self.rare_max_count,
            ));
        }
        if self.rare_max_count > self.uncommon_max_count {
            return Err(invalid_config(
                FREQUENCY_UNCOMMON_MAX_COUNT_PARAM,
                "must_be_at_least_rare_max_count",
                self.uncommon_max_count,
            ));
        }
        if self.uncommon_max_count > self.common_max_count {
            return Err(invalid_config(
                FREQUENCY_COMMON_MAX_COUNT_PARAM,
                "must_be_at_least_uncommon_max_count",
                self.common_max_count,
            ));
        }
        Ok(())
    }

    pub fn classify_count(&self, count: u64) -> EntityValueFrequencyBand {
        if count < self.minimum_count {
            return EntityValueFrequencyBand::Uncommon;
        }
        if count <= self.rare_max_count {
            EntityValueFrequencyBand::Rare
        } else if count <= self.uncommon_max_count {
            EntityValueFrequencyBand::Uncommon
        } else if count <= self.common_max_count {
            EntityValueFrequencyBand::Common
        } else {
            EntityValueFrequencyBand::VeryCommon
        }
    }

    pub fn multiplier_basis_points(&self, band: EntityValueFrequencyBand) -> u32 {
        match band {
            EntityValueFrequencyBand::Rare => self.rare_multiplier_basis_points,
            EntityValueFrequencyBand::Uncommon => self.uncommon_multiplier_basis_points,
            EntityValueFrequencyBand::Common => self.common_multiplier_basis_points,
            EntityValueFrequencyBand::VeryCommon => self.very_common_multiplier_basis_points,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityValueFrequencyStrategyConfig {
    pub table_content_hash: String,
    pub bands: EntityValueFrequencyBandConfig,
}

impl EntityValueFrequencyStrategyConfig {
    pub fn from_operator_params(
        params: &BTreeMap<String, String>,
    ) -> Result<Option<Self>, EntityValueFrequencyError> {
        let keys = [
            FREQUENCY_TABLE_HASH_PARAM,
            FREQUENCY_MINIMUM_COUNT_PARAM,
            FREQUENCY_RARE_MAX_COUNT_PARAM,
            FREQUENCY_UNCOMMON_MAX_COUNT_PARAM,
            FREQUENCY_COMMON_MAX_COUNT_PARAM,
            FREQUENCY_RARE_MULTIPLIER_PARAM,
            FREQUENCY_UNCOMMON_MULTIPLIER_PARAM,
            FREQUENCY_COMMON_MULTIPLIER_PARAM,
            FREQUENCY_VERY_COMMON_MULTIPLIER_PARAM,
        ];
        if !keys.iter().any(|key| params.contains_key(*key)) {
            return Ok(None);
        }

        let config = Self {
            table_content_hash: required_blake3_digest_param(params, FREQUENCY_TABLE_HASH_PARAM)?,
            bands: EntityValueFrequencyBandConfig {
                minimum_count: required_u64_param(params, FREQUENCY_MINIMUM_COUNT_PARAM)?,
                rare_max_count: required_u64_param(params, FREQUENCY_RARE_MAX_COUNT_PARAM)?,
                uncommon_max_count: required_u64_param(params, FREQUENCY_UNCOMMON_MAX_COUNT_PARAM)?,
                common_max_count: required_u64_param(params, FREQUENCY_COMMON_MAX_COUNT_PARAM)?,
                rare_multiplier_basis_points: required_u32_param(
                    params,
                    FREQUENCY_RARE_MULTIPLIER_PARAM,
                )?,
                uncommon_multiplier_basis_points: required_u32_param(
                    params,
                    FREQUENCY_UNCOMMON_MULTIPLIER_PARAM,
                )?,
                common_multiplier_basis_points: required_u32_param(
                    params,
                    FREQUENCY_COMMON_MULTIPLIER_PARAM,
                )?,
                very_common_multiplier_basis_points: required_u32_param(
                    params,
                    FREQUENCY_VERY_COMMON_MULTIPLIER_PARAM,
                )?,
            },
        };
        config.validate()?;
        Ok(Some(config))
    }

    pub fn validate(&self) -> Result<(), EntityValueFrequencyError> {
        self.bands.validate()?;
        Ok(())
    }

    pub fn validate_table(
        &self,
        table: &EntityValueFrequencyTable,
        index: &EntityPostingIndex,
    ) -> Result<(), EntityValueFrequencyError> {
        table.validate_for_posting_index(index)?;
        if table.content_hash != self.table_content_hash {
            return Err(EntityValueFrequencyError::StrategyTableHashMismatch {
                expected: self.table_content_hash.clone(),
                actual: table.content_hash.clone(),
            });
        }
        Ok(())
    }

    pub fn adjustment_for_exact_value(
        &self,
        table: &EntityValueFrequencyTable,
        view_name: &str,
        value: &str,
    ) -> Result<EntityValueFrequencyAdjustment, EntityValueFrequencyError> {
        table.adjustment_for_exact_value(&self.bands, view_name, value)
    }

    pub fn adjustment_for_fuzzy_values(
        &self,
        table: &EntityValueFrequencyTable,
        view_name: &str,
        left_value: &str,
        right_value: &str,
    ) -> Result<EntityValueFrequencyAdjustment, EntityValueFrequencyError> {
        table.adjustment_for_fuzzy_values(&self.bands, view_name, left_value, right_value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityValueFrequencyAdjustment {
    pub version: String,
    pub table_content_hash: String,
    pub view_name: String,
    pub value: String,
    pub count: u64,
    pub band: EntityValueFrequencyBand,
    pub floor_applied: bool,
    pub multiplier_basis_points: u32,
}

impl EntityValueFrequencyTable {
    pub fn from_posting_index(
        index: &EntityPostingIndex,
    ) -> Result<Self, EntityValueFrequencyError> {
        let mut records = index
            .exact_view_value_frequencies()?
            .into_iter()
            .map(|frequency| EntityValueFrequencyRecord {
                term_id: frequency.term_id,
                view_name: frequency.view_name,
                value: frequency.value,
                count: frequency.count,
            })
            .collect::<Vec<_>>();
        records.sort_by(value_frequency_record_cmp);

        let mut table = Self {
            version: CANON_ENTITY_VALUE_FREQUENCY_VERSION.to_string(),
            content_hash: String::new(),
            source_posting_index_hash: index.content_hash()?,
            surface_count: u64::from(index.exact_view_layout.surface_count),
            records,
        };
        table.content_hash = hash_table_without_self(&table)?;
        table.validate()?;
        Ok(table)
    }

    pub fn validate(&self) -> Result<(), EntityValueFrequencyError> {
        if self.version != CANON_ENTITY_VALUE_FREQUENCY_VERSION {
            return Err(EntityValueFrequencyError::VersionMismatch {
                expected: CANON_ENTITY_VALUE_FREQUENCY_VERSION.to_string(),
                actual: self.version.clone(),
            });
        }
        if self.source_posting_index_hash.trim().is_empty() {
            return Err(EntityValueFrequencyError::MissingField {
                field: "source_posting_index_hash",
            });
        }
        if self.content_hash.trim().is_empty() {
            return Err(EntityValueFrequencyError::MissingField {
                field: "content_hash",
            });
        }

        let mut previous: Option<&EntityValueFrequencyRecord> = None;
        for record in &self.records {
            if record.view_name.trim().is_empty() {
                return Err(EntityValueFrequencyError::InvalidRecord {
                    field: "view_name",
                    value: record.view_name.clone(),
                });
            }
            if record.value.trim().is_empty() {
                return Err(EntityValueFrequencyError::InvalidRecord {
                    field: "value",
                    value: record.value.clone(),
                });
            }
            if record.count == 0 {
                return Err(EntityValueFrequencyError::InvalidRecord {
                    field: "count",
                    value: record.count.to_string(),
                });
            }
            if let Some(left) = previous {
                let ordering = value_frequency_record_cmp(left, record);
                if left.view_name == record.view_name && left.value == record.value {
                    return Err(EntityValueFrequencyError::DuplicateRecord {
                        view_name: record.view_name.clone(),
                        value: record.value.clone(),
                    });
                }
                if ordering.is_gt() {
                    return Err(EntityValueFrequencyError::RecordsNotSorted);
                }
            }
            previous = Some(record);
        }

        let expected = hash_table_without_self(self)?;
        if self.content_hash != expected {
            return Err(EntityValueFrequencyError::ContentHashMismatch {
                expected,
                actual: self.content_hash.clone(),
            });
        }
        Ok(())
    }

    pub fn validate_for_posting_index(
        &self,
        index: &EntityPostingIndex,
    ) -> Result<(), EntityValueFrequencyError> {
        self.validate()?;
        let expected_source_hash = index.content_hash()?;
        if self.source_posting_index_hash != expected_source_hash {
            return Err(EntityValueFrequencyError::SourcePostingIndexHashMismatch {
                expected: expected_source_hash,
                actual: self.source_posting_index_hash.clone(),
            });
        }
        let expected = Self::from_posting_index(index)?;
        if self.records != expected.records {
            return Err(EntityValueFrequencyError::NonCanonicalRecords {
                expected: expected.content_hash,
                actual: self.content_hash.clone(),
            });
        }
        Ok(())
    }

    pub fn count_for(
        &self,
        view_name: &str,
        value: &str,
    ) -> Result<u64, EntityValueFrequencyError> {
        self.records
            .iter()
            .find(|record| record.view_name == view_name && record.value == value)
            .map(|record| record.count)
            .ok_or_else(|| EntityValueFrequencyError::MissingValue {
                view_name: view_name.to_string(),
                value: value.to_string(),
            })
    }

    pub fn adjustment_for_exact_value(
        &self,
        config: &EntityValueFrequencyBandConfig,
        view_name: &str,
        value: &str,
    ) -> Result<EntityValueFrequencyAdjustment, EntityValueFrequencyError> {
        let count = self.count_for(view_name, value)?;
        Ok(adjustment_for_count(self, config, view_name, value, count))
    }

    pub fn adjustment_for_fuzzy_values(
        &self,
        config: &EntityValueFrequencyBandConfig,
        view_name: &str,
        left_value: &str,
        right_value: &str,
    ) -> Result<EntityValueFrequencyAdjustment, EntityValueFrequencyError> {
        let left_count = self.count_for(view_name, left_value)?;
        let right_count = self.count_for(view_name, right_value)?;
        let (value, count) = if left_count >= right_count {
            (left_value, left_count)
        } else {
            (right_value, right_count)
        };
        Ok(adjustment_for_count(self, config, view_name, value, count))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityValueFrequencyError {
    VersionMismatch {
        expected: String,
        actual: String,
    },
    MissingField {
        field: &'static str,
    },
    ContentHashMismatch {
        expected: String,
        actual: String,
    },
    SourcePostingIndexHashMismatch {
        expected: String,
        actual: String,
    },
    StrategyTableHashMismatch {
        expected: String,
        actual: String,
    },
    NonCanonicalRecords {
        expected: String,
        actual: String,
    },
    RecordsNotSorted,
    DuplicateRecord {
        view_name: String,
        value: String,
    },
    InvalidRecord {
        field: &'static str,
        value: String,
    },
    MissingValue {
        view_name: String,
        value: String,
    },
    InvalidConfig {
        field: &'static str,
        reason: &'static str,
        value: String,
    },
    PostingLayout(PostingLayoutError),
    Serialization(String),
}

impl EntityValueFrequencyError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::VersionMismatch { .. } => "wrong_version",
            Self::MissingField { .. } => "missing_field",
            Self::ContentHashMismatch { .. } => "content_hash_mismatch",
            Self::SourcePostingIndexHashMismatch { .. } => "stale_frequency_table",
            Self::StrategyTableHashMismatch { .. } => "strategy_frequency_table_hash_mismatch",
            Self::NonCanonicalRecords { .. } => "noncanonical_frequency_records",
            Self::RecordsNotSorted => "frequency_records_not_sorted",
            Self::DuplicateRecord { .. } => "duplicate_frequency_record",
            Self::InvalidRecord { .. } => "invalid_frequency_record",
            Self::MissingValue { .. } => "missing_frequency_value",
            Self::InvalidConfig { .. } => "invalid_frequency_config",
            Self::PostingLayout(_) => "invalid_posting_layout",
            Self::Serialization(_) => "serialization_error",
        }
    }

    pub fn field(&self) -> &'static str {
        match self {
            Self::VersionMismatch { .. } => "version",
            Self::MissingField { field } => field,
            Self::ContentHashMismatch { .. } => "content_hash",
            Self::SourcePostingIndexHashMismatch { .. } => "source_posting_index_hash",
            Self::StrategyTableHashMismatch { .. } => FREQUENCY_TABLE_HASH_PARAM,
            Self::NonCanonicalRecords { .. } => "records",
            Self::RecordsNotSorted => "records",
            Self::DuplicateRecord { .. } => "records",
            Self::InvalidRecord { field, .. } => field,
            Self::MissingValue { .. } => "value",
            Self::InvalidConfig { field, .. } => field,
            Self::PostingLayout(_) => "posting_layout",
            Self::Serialization(_) => "serialization",
        }
    }
}

impl From<PostingLayoutError> for EntityValueFrequencyError {
    fn from(error: PostingLayoutError) -> Self {
        Self::PostingLayout(error)
    }
}

impl fmt::Display for EntityValueFrequencyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EntityValueFrequencyError {}

pub fn scale_score_units_by_frequency(
    score_units: ScoreUnits,
    adjustment: &EntityValueFrequencyAdjustment,
) -> ScoreUnits {
    let scaled =
        (u128::from(score_units.as_u32()) * u128::from(adjustment.multiplier_basis_points) + 5_000)
            / 10_000;
    ScoreUnits::saturating_from_units(u64::try_from(scaled).unwrap_or(u64::MAX))
}

fn adjustment_for_count(
    table: &EntityValueFrequencyTable,
    config: &EntityValueFrequencyBandConfig,
    view_name: &str,
    value: &str,
    count: u64,
) -> EntityValueFrequencyAdjustment {
    let band = config.classify_count(count);
    EntityValueFrequencyAdjustment {
        version: CANON_ENTITY_VALUE_FREQUENCY_VERSION.to_string(),
        table_content_hash: table.content_hash.clone(),
        view_name: view_name.to_string(),
        value: value.to_string(),
        count,
        band,
        floor_applied: count < config.minimum_count,
        multiplier_basis_points: config.multiplier_basis_points(band),
    }
}

fn value_frequency_record_cmp(
    left: &EntityValueFrequencyRecord,
    right: &EntityValueFrequencyRecord,
) -> std::cmp::Ordering {
    left.view_name
        .as_bytes()
        .cmp(right.view_name.as_bytes())
        .then_with(|| left.value.as_bytes().cmp(right.value.as_bytes()))
        .then_with(|| left.term_id.cmp(&right.term_id))
}

fn hash_table_without_self(
    table: &EntityValueFrequencyTable,
) -> Result<String, EntityValueFrequencyError> {
    let mut hashable = table.clone();
    hashable.content_hash.clear();
    let bytes = serde_json::to_vec(&hashable)
        .map_err(|error| EntityValueFrequencyError::Serialization(error.to_string()))?;
    Ok(witness::hash_bytes(&bytes))
}

fn required_blake3_digest_param(
    params: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<String, EntityValueFrequencyError> {
    let value = params
        .get(field)
        .ok_or_else(|| missing_config(field))?
        .trim();
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_config_with_value(
            field,
            "must_be_blake3_digest",
            value,
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_config_with_value(
            field,
            "must_be_blake3_digest",
            value,
        ));
    }
    Ok(value.to_string())
}

fn required_u64_param(
    params: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<u64, EntityValueFrequencyError> {
    let value = params
        .get(field)
        .ok_or_else(|| missing_config(field))?
        .trim();
    let parsed = value
        .parse::<u64>()
        .map_err(|_| invalid_config_with_value(field, "must_be_integer", value))?;
    if parsed == 0 {
        return Err(invalid_config_with_value(field, "must_be_positive", value));
    }
    Ok(parsed)
}

fn required_u32_param(
    params: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<u32, EntityValueFrequencyError> {
    let value = params
        .get(field)
        .ok_or_else(|| missing_config(field))?
        .trim();
    value
        .parse::<u32>()
        .map_err(|_| invalid_config_with_value(field, "must_be_integer", value))
}

fn missing_config(field: &'static str) -> EntityValueFrequencyError {
    EntityValueFrequencyError::InvalidConfig {
        field,
        reason: "missing_required_param",
        value: String::new(),
    }
}

fn invalid_config(
    field: &'static str,
    reason: &'static str,
    value: u64,
) -> EntityValueFrequencyError {
    invalid_config_with_value(field, reason, &value.to_string())
}

fn invalid_config_with_value(
    field: &'static str,
    reason: &'static str,
    value: &str,
) -> EntityValueFrequencyError {
    EntityValueFrequencyError::InvalidConfig {
        field,
        reason,
        value: value.to_string(),
    }
}
