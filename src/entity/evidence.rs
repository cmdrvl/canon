#![forbid(unsafe_code)]

//! Support-evidence helpers for `canon entity edge`.
//!
//! This module converts already-normalized views and namekit metric output into
//! edge hits. It deliberately emits only support-lane hits; anti-merge and
//! relation-hint evidence live in their own lanes.

#[path = "value_frequency.rs"]
pub mod value_frequency;

use crate::{
    Refusal,
    entity::{
        edge::EdgeEvidenceHit,
        error::EntityRefusalKind,
        postings::EntityPostingIndex,
        score::{ScoreLane, ScoreUnits},
    },
    namekit::{
        SimilarityScore,
        similarity::{SimilarityMetric, SimilarityOptions, SimilarityPath, normalized_similarity},
    },
};
use serde_json::json;
use std::{collections::BTreeSet, fmt};
use value_frequency::{
    EntityValueFrequencyAdjustment, EntityValueFrequencyError, EntityValueFrequencyStrategyConfig,
    EntityValueFrequencyTable, scale_score_units_by_frequency,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactViewSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub view_name: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenOverlapSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub left_tokens: &'a [&'a str],
    pub right_tokens: &'a [&'a str],
    pub min_shared_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringSimilaritySupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub metric: SimilarityMetric,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub score_cutoff: Option<ScoreUnits>,
    pub score_hint: Option<ScoreUnits>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequencyWeightedExactViewSupportRequest<'a> {
    pub support: ExactViewSupportRequest<'a>,
    pub adjustment: Option<EntityValueFrequencyAdjustment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequencyWeightedStringSimilaritySupportRequest<'a> {
    pub support: StringSimilaritySupportRequest<'a>,
    pub adjustment: Option<EntityValueFrequencyAdjustment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumericWithinToleranceSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub view_name: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub absolute_tolerance: &'a str,
    pub relative_tolerance_bps: Option<u32>,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateExactSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub view_name: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateNearSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub view_name: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub max_days: u32,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoricalMatchSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub view_name: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredSupportError {
    field: &'static str,
    reason: &'static str,
}

impl StructuredSupportError {
    pub fn field(&self) -> &'static str {
        self.field
    }

    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for StructuredSupportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.field, self.reason)
    }
}

impl std::error::Error for StructuredSupportError {}

pub fn exact_view_support_hit(request: ExactViewSupportRequest<'_>) -> Option<EdgeEvidenceHit> {
    let left = request.left_value.trim();
    let right = request.right_value.trim();
    if left.is_empty() || right.is_empty() || left != right {
        return None;
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "exact view {} matched with score_units={}",
            request.view_name,
            request.score_units.as_u32()
        ),
    ))
}

pub fn frequency_weighted_exact_view_support_hit(
    request: FrequencyWeightedExactViewSupportRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let hit = exact_view_support_hit(request.support)?;
    Some(match request.adjustment {
        Some(adjustment) => apply_value_frequency_adjustment(hit, &adjustment),
        None => hit,
    })
}

pub fn token_overlap_support_hit(
    request: TokenOverlapSupportRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let left = token_set(request.left_tokens);
    let right = token_set(request.right_tokens);
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let shared_tokens = left.intersection(&right).count();
    if shared_tokens == 0 || shared_tokens < request.min_shared_tokens {
        return None;
    }

    let denominator = left.len().max(right.len()) as u64;
    let score_units =
        ScoreUnits::from_ratio_parts(shared_tokens as u64, denominator).unwrap_or(ScoreUnits::ZERO);

    Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        score_units,
        false,
        format!(
            "token overlap shared_tokens={} score_units={}",
            shared_tokens,
            score_units.as_u32()
        ),
    ))
}

pub fn string_similarity_support_hit(
    request: StringSimilaritySupportRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let result = normalized_similarity(
        request.metric,
        request.left_value,
        request.right_value,
        SimilarityOptions::new(
            request.score_cutoff.map(score_units_to_namekit),
            request.score_hint.map(score_units_to_namekit),
        ),
    );
    let score_units = result.score.map(ScoreUnits::from)?;

    Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        score_units,
        false,
        format!(
            "string metric {} path={} score_units={}",
            metric_id(request.metric),
            path_id(result.path),
            score_units.as_u32()
        ),
    ))
}

pub fn frequency_weighted_string_similarity_support_hit(
    request: FrequencyWeightedStringSimilaritySupportRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let hit = string_similarity_support_hit(request.support)?;
    Some(match request.adjustment {
        Some(adjustment) => apply_value_frequency_adjustment(hit, &adjustment),
        None => hit,
    })
}

pub fn apply_value_frequency_adjustment(
    mut hit: EdgeEvidenceHit,
    adjustment: &EntityValueFrequencyAdjustment,
) -> EdgeEvidenceHit {
    if hit.lane != ScoreLane::Support {
        return hit;
    }
    let original_score_units = hit.score_units;
    let adjusted_score_units = scale_score_units_by_frequency(original_score_units, adjustment);
    hit.score_units = adjusted_score_units;
    hit.explanation = format!(
        "{} value_frequency version={} table_hash={} view={} value={} count={} band={} floor_applied={} multiplier_basis_points={} original_score_units={} adjusted_score_units={}",
        hit.explanation,
        adjustment.version,
        adjustment.table_content_hash,
        adjustment.view_name,
        adjustment.value,
        adjustment.count,
        adjustment.band.as_str(),
        adjustment.floor_applied,
        adjustment.multiplier_basis_points,
        original_score_units.as_u32(),
        adjusted_score_units.as_u32()
    );
    hit
}

pub fn validate_value_frequency_table_for_scoring(
    table: &EntityValueFrequencyTable,
    posting_index: &EntityPostingIndex,
) -> Result<(), Refusal> {
    table
        .validate_for_posting_index(posting_index)
        .map_err(value_frequency_refusal)
}

pub fn validate_value_frequency_strategy_for_scoring(
    config: &EntityValueFrequencyStrategyConfig,
    table: &EntityValueFrequencyTable,
    posting_index: &EntityPostingIndex,
) -> Result<(), Refusal> {
    config
        .validate_table(table, posting_index)
        .map_err(value_frequency_refusal)
}

pub fn numeric_within_tolerance_support_hit(
    request: NumericWithinToleranceSupportRequest<'_>,
) -> Result<Option<EdgeEvidenceHit>, StructuredSupportError> {
    let Some(left) = FixedDecimal::parse_optional(request.left_value, "left_value")? else {
        return Ok(None);
    };
    let Some(right) = FixedDecimal::parse_optional(request.right_value, "right_value")? else {
        return Ok(None);
    };
    let tolerance = parse_non_negative_decimal(request.absolute_tolerance, "absolute_tolerance")?;
    let difference = left.absolute_difference(right)?;
    let absolute_match = difference.less_than_or_equal(tolerance)?;
    let relative_match = match request.relative_tolerance_bps {
        Some(bps) => {
            let basis = left.max_absolute(right)?;
            FixedDecimal::within_basis_points(difference, basis, bps)?
        }
        None => false,
    };
    if !absolute_match && !relative_match {
        return Ok(None);
    }

    Ok(Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "numeric view {} matched tolerance with score_units={}",
            request.view_name,
            request.score_units.as_u32()
        ),
    )))
}

pub fn validate_numeric_tolerance_params(
    absolute_tolerance: &str,
) -> Result<(), StructuredSupportError> {
    parse_non_negative_decimal(absolute_tolerance, "absolute_tolerance").map(|_| ())
}

pub fn date_exact_support_hit(
    request: DateExactSupportRequest<'_>,
) -> Result<Option<EdgeEvidenceHit>, StructuredSupportError> {
    let Some(left) = IsoDate::parse_optional(request.left_value, "left_value")? else {
        return Ok(None);
    };
    let Some(right) = IsoDate::parse_optional(request.right_value, "right_value")? else {
        return Ok(None);
    };
    if left != right {
        return Ok(None);
    }

    Ok(Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "date view {} matched exactly with score_units={}",
            request.view_name,
            request.score_units.as_u32()
        ),
    )))
}

pub fn date_near_support_hit(
    request: DateNearSupportRequest<'_>,
) -> Result<Option<EdgeEvidenceHit>, StructuredSupportError> {
    let Some(left) = IsoDate::parse_optional(request.left_value, "left_value")? else {
        return Ok(None);
    };
    let Some(right) = IsoDate::parse_optional(request.right_value, "right_value")? else {
        return Ok(None);
    };
    let distance = left.day_number.abs_diff(right.day_number);
    if distance > u64::from(request.max_days) {
        return Ok(None);
    }

    Ok(Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "date view {} matched within max_days={} score_units={}",
            request.view_name,
            request.max_days,
            request.score_units.as_u32()
        ),
    )))
}

pub fn categorical_match_support_hit(
    request: CategoricalMatchSupportRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let left = request.left_value;
    let right = request.right_value;
    if left.is_empty() || right.is_empty() || left != right {
        return None;
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "categorical view {} matched exactly with score_units={}",
            request.view_name,
            request.score_units.as_u32()
        ),
    ))
}

fn token_set<'a>(tokens: &'a [&'a str]) -> BTreeSet<&'a str> {
    tokens
        .iter()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .collect()
}

fn score_units_to_namekit(score_units: ScoreUnits) -> SimilarityScore {
    SimilarityScore::from_scaled(score_units.as_u32() as u16)
        .expect("entity score units share the namekit score scale")
}

fn metric_id(metric: SimilarityMetric) -> &'static str {
    match metric {
        SimilarityMetric::LevenshteinNormalized => "levenshtein_normalized",
        SimilarityMetric::DamerauLevenshteinNormalized => "damerau_levenshtein_normalized",
        SimilarityMetric::JaroWinkler => "jaro_winkler",
        SimilarityMetric::DiceSorensen => "dice_sorensen",
        SimilarityMetric::TokenSortRatio => "token_sort_ratio",
        SimilarityMetric::TokenSetRatio => "token_set_ratio",
    }
}

fn path_id(path: SimilarityPath) -> &'static str {
    match path {
        SimilarityPath::AsciiBytes => "ascii_bytes",
        SimilarityPath::UnicodeChars => "unicode_chars",
    }
}

fn value_frequency_refusal(error: EntityValueFrequencyError) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Value-frequency table does not match the current evidence scoring inputs",
        json!({
            "stage": "evidence",
            "artifact": "value_frequency_table",
            "reason": error.reason(),
            "field": error.field(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        Some(
            "Use the matching frequency table for this index or rerun canon entity index"
                .to_string(),
        ),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FixedDecimal {
    mantissa: i128,
    scale: u32,
}

impl FixedDecimal {
    fn parse_optional(
        value: &str,
        field: &'static str,
    ) -> Result<Option<Self>, StructuredSupportError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let bytes = trimmed.as_bytes();
        let mut index = 0usize;
        let negative = matches!(bytes.first(), Some(b'-'));
        if negative {
            index += 1;
            if index == bytes.len() {
                return Err(structured_error(field, "malformed_decimal"));
            }
        }

        let whole_start = index;
        while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
            index += 1;
        }
        let whole_digits = index - whole_start;
        if whole_digits == 0 {
            return Err(structured_error(field, "malformed_decimal"));
        }

        let mut scale = 0u32;
        if matches!(bytes.get(index), Some(b'.')) {
            index += 1;
            let fractional_start = index;
            while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
                index += 1;
            }
            let fractional_digits = index - fractional_start;
            if fractional_digits == 0 {
                return Err(structured_error(field, "malformed_decimal"));
            }
            scale = u32::try_from(fractional_digits)
                .map_err(|_| structured_error(field, "decimal_overflow"))?;
        }

        if index != bytes.len() {
            return Err(structured_error(field, "malformed_decimal"));
        }

        let mut mantissa = 0i128;
        for byte in bytes {
            if byte.is_ascii_digit() {
                mantissa = mantissa
                    .checked_mul(10)
                    .and_then(|value| value.checked_add(i128::from(byte - b'0')))
                    .ok_or_else(|| structured_error(field, "decimal_overflow"))?;
            }
        }
        if negative {
            mantissa = mantissa
                .checked_neg()
                .ok_or_else(|| structured_error(field, "decimal_overflow"))?;
        }

        Ok(Some(Self { mantissa, scale }))
    }

    fn absolute(self, field: &'static str) -> Result<Self, StructuredSupportError> {
        Ok(Self {
            mantissa: self
                .mantissa
                .checked_abs()
                .ok_or_else(|| structured_error(field, "decimal_overflow"))?,
            scale: self.scale,
        })
    }

    fn absolute_difference(self, other: Self) -> Result<Self, StructuredSupportError> {
        let (left, right, scale) = align_decimals(self, other, "numeric_difference")?;
        let mantissa = left
            .checked_sub(right)
            .and_then(i128::checked_abs)
            .ok_or_else(|| structured_error("numeric_difference", "decimal_overflow"))?;
        Ok(Self { mantissa, scale })
    }

    fn max_absolute(self, other: Self) -> Result<Self, StructuredSupportError> {
        let left = self.absolute("left_value")?;
        let right = other.absolute("right_value")?;
        if left.less_than_or_equal(right)? {
            Ok(right)
        } else {
            Ok(left)
        }
    }

    fn less_than_or_equal(self, other: Self) -> Result<bool, StructuredSupportError> {
        let (left, right, _) = align_decimals(self, other, "decimal_compare")?;
        Ok(left <= right)
    }

    fn within_basis_points(
        difference: Self,
        basis: Self,
        basis_points: u32,
    ) -> Result<bool, StructuredSupportError> {
        let (difference, basis, _) = align_decimals(difference, basis, "relative_tolerance")?;
        let scaled_difference = difference
            .checked_mul(10_000)
            .ok_or_else(|| structured_error("relative_tolerance_bps", "decimal_overflow"))?;
        let scaled_basis = basis
            .checked_mul(i128::from(basis_points))
            .ok_or_else(|| structured_error("relative_tolerance_bps", "decimal_overflow"))?;
        Ok(scaled_difference <= scaled_basis)
    }
}

fn parse_non_negative_decimal(
    value: &str,
    field: &'static str,
) -> Result<FixedDecimal, StructuredSupportError> {
    let decimal = FixedDecimal::parse_optional(value, field)?
        .ok_or_else(|| structured_error(field, "missing_decimal"))?;
    if decimal.mantissa < 0 {
        return Err(structured_error(field, "negative_decimal"));
    }
    Ok(decimal)
}

fn align_decimals(
    left: FixedDecimal,
    right: FixedDecimal,
    field: &'static str,
) -> Result<(i128, i128, u32), StructuredSupportError> {
    let scale = left.scale.max(right.scale);
    let left = scale_decimal(left, scale, field)?;
    let right = scale_decimal(right, scale, field)?;
    Ok((left, right, scale))
}

fn scale_decimal(
    decimal: FixedDecimal,
    scale: u32,
    field: &'static str,
) -> Result<i128, StructuredSupportError> {
    let factor = checked_pow10(scale - decimal.scale, field)?;
    decimal
        .mantissa
        .checked_mul(factor)
        .ok_or_else(|| structured_error(field, "decimal_overflow"))
}

fn checked_pow10(exponent: u32, field: &'static str) -> Result<i128, StructuredSupportError> {
    let mut value = 1i128;
    for _ in 0..exponent {
        value = value
            .checked_mul(10)
            .ok_or_else(|| structured_error(field, "decimal_overflow"))?;
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IsoDate {
    day_number: i64,
}

impl IsoDate {
    fn parse_optional(
        value: &str,
        field: &'static str,
    ) -> Result<Option<Self>, StructuredSupportError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let bytes = trimmed.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(structured_error(field, "malformed_date"));
        }
        let year = parse_date_component(&bytes[0..4], field)?;
        let month = parse_date_component(&bytes[5..7], field)?;
        let day = parse_date_component(&bytes[8..10], field)?;
        if year == 0 || !(1..=12).contains(&month) {
            return Err(structured_error(field, "invalid_date"));
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(structured_error(field, "invalid_date"));
        }

        Ok(Some(Self {
            day_number: days_from_civil(year as i32, month, day),
        }))
    }
}

fn parse_date_component(bytes: &[u8], field: &'static str) -> Result<u32, StructuredSupportError> {
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(structured_error(field, "malformed_date"));
        }
        value = value * 10 + u32::from(byte - b'0');
    }
    Ok(value)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era
}

fn structured_error(field: &'static str, reason: &'static str) -> StructuredSupportError {
    StructuredSupportError { field, reason }
}
