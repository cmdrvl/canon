#![forbid(unsafe_code)]

//! Cannot-link evidence helpers for `canon entity edge`.
//!
//! These helpers convert profile/namekit anti-overmerge signals into hard
//! anti-merge edge hits. They never emit support-lane evidence.

use crate::entity::{
    edge::EdgeEvidenceHit,
    score::{ScoreLane, ScoreUnits},
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedTokenConflictRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub left_tokens: &'a [&'a str],
    pub right_tokens: &'a [&'a str],
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedDistinctPhraseRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub phrases: &'a [&'a str],
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorConflictRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub field: &'a str,
    pub left_values: &'a [&'a str],
    pub right_values: &'a [&'a str],
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeConflictComparison {
    Date,
    DecimalBasisPoints { scale: BasisPointScale },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasisPointScale {
    Percent,
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeConflictRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub field: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub comparison: AttributeConflictComparison,
    pub tolerance_bps: u32,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredAntiMergeError {
    field: &'static str,
    reason: &'static str,
}

impl StructuredAntiMergeError {
    pub fn field(&self) -> &'static str {
        self.field
    }

    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl std::fmt::Display for StructuredAntiMergeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.field, self.reason)
    }
}

impl std::error::Error for StructuredAntiMergeError {}

pub fn protected_token_conflict_hit(
    request: ProtectedTokenConflictRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let left = token_set(request.left_tokens);
    let right = token_set(request.right_tokens);
    if left.is_empty() && right.is_empty() {
        return None;
    }
    if left == right {
        return None;
    }

    let left_only = sorted_difference(&left, &right);
    let right_only = sorted_difference(&right, &left);

    Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        true,
        format!(
            "protected token conflict left_only={} right_only={} score_units={}",
            list_or_none(&left_only),
            list_or_none(&right_only),
            request.score_units.as_u32()
        ),
    ))
}

pub fn related_distinct_phrase_hit(
    request: RelatedDistinctPhraseRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let left = request.left_value.to_ascii_lowercase();
    let right = request.right_value.to_ascii_lowercase();
    let mut matched = request
        .phrases
        .iter()
        .map(|phrase| phrase.trim())
        .filter(|phrase| !phrase.is_empty())
        .filter(|phrase| left.contains(phrase) ^ right.contains(phrase))
        .collect::<Vec<_>>();
    matched.sort_unstable();
    matched.dedup();

    if matched.is_empty() {
        return None;
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        true,
        format!(
            "related distinct phrase matched={} score_units={}",
            matched.join("|"),
            request.score_units.as_u32()
        ),
    ))
}

pub fn anchor_conflict_hit(request: AnchorConflictRequest<'_>) -> Option<EdgeEvidenceHit> {
    let left = token_set(request.left_values);
    let right = token_set(request.right_values);
    if left.is_empty() || right.is_empty() || left == right {
        return None;
    }

    let left_only = sorted_difference(&left, &right);
    let right_only = sorted_difference(&right, &left);

    Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        true,
        format!(
            "anchor field {} conflict left_only={} right_only={} score_units={}",
            request.field,
            list_or_none(&left_only),
            list_or_none(&right_only),
            request.score_units.as_u32()
        ),
    ))
}

pub fn attribute_conflict_hit(
    request: AttributeConflictRequest<'_>,
) -> Result<Option<EdgeEvidenceHit>, StructuredAntiMergeError> {
    let conflict = match request.comparison {
        AttributeConflictComparison::Date => {
            let Some(left) = IsoDate::parse_optional(request.left_value, "left_value")? else {
                return Ok(None);
            };
            let Some(right) = IsoDate::parse_optional(request.right_value, "right_value")? else {
                return Ok(None);
            };
            left != right
        }
        AttributeConflictComparison::DecimalBasisPoints { scale } => {
            let Some(left) = FixedDecimal::parse_optional(request.left_value, "left_value")? else {
                return Ok(None);
            };
            let Some(right) = FixedDecimal::parse_optional(request.right_value, "right_value")?
            else {
                return Ok(None);
            };
            !left.within_basis_points(right, request.tolerance_bps, scale)?
        }
    };
    if !conflict {
        return Ok(None);
    }

    Ok(Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        true,
        format!(
            "attribute field {} conflict comparison={} tolerance_bps={} score_units={}",
            request.field,
            comparison_name(request.comparison),
            request.tolerance_bps,
            request.score_units.as_u32()
        ),
    )))
}

fn token_set<'a>(tokens: &'a [&'a str]) -> BTreeSet<&'a str> {
    tokens
        .iter()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .collect()
}

fn sorted_difference(left: &BTreeSet<&str>, right: &BTreeSet<&str>) -> Vec<String> {
    left.difference(right)
        .map(|token| (*token).to_string())
        .collect()
}

fn list_or_none(tokens: &[String]) -> String {
    if tokens.is_empty() {
        "none".to_string()
    } else {
        tokens.join("|")
    }
}

fn comparison_name(comparison: AttributeConflictComparison) -> &'static str {
    match comparison {
        AttributeConflictComparison::Date => "date",
        AttributeConflictComparison::DecimalBasisPoints {
            scale: BasisPointScale::Percent,
        } => "decimal_bps_percent",
        AttributeConflictComparison::DecimalBasisPoints {
            scale: BasisPointScale::Unit,
        } => "decimal_bps_unit",
    }
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
    ) -> Result<Option<Self>, StructuredAntiMergeError> {
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
                return Err(anti_merge_error(field, "malformed_decimal"));
            }
        }

        let whole_start = index;
        while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
            index += 1;
        }
        if index == whole_start {
            return Err(anti_merge_error(field, "malformed_decimal"));
        }

        let mut scale = 0u32;
        if matches!(bytes.get(index), Some(b'.')) {
            index += 1;
            let fractional_start = index;
            while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
                index += 1;
            }
            if index == fractional_start {
                return Err(anti_merge_error(field, "malformed_decimal"));
            }
            scale = u32::try_from(index - fractional_start)
                .map_err(|_| anti_merge_error(field, "decimal_overflow"))?;
        }
        if index != bytes.len() {
            return Err(anti_merge_error(field, "malformed_decimal"));
        }

        let mut mantissa = 0i128;
        for byte in bytes {
            if byte.is_ascii_digit() {
                mantissa = mantissa
                    .checked_mul(10)
                    .and_then(|value| value.checked_add(i128::from(byte - b'0')))
                    .ok_or_else(|| anti_merge_error(field, "decimal_overflow"))?;
            }
        }
        if negative {
            mantissa = mantissa
                .checked_neg()
                .ok_or_else(|| anti_merge_error(field, "decimal_overflow"))?;
        }
        Ok(Some(Self { mantissa, scale }))
    }

    fn within_basis_points(
        self,
        other: Self,
        tolerance_bps: u32,
        scale: BasisPointScale,
    ) -> Result<bool, StructuredAntiMergeError> {
        let common_scale = self.scale.max(other.scale);
        let left = scale_decimal(self, common_scale, "basis_point_compare")?;
        let right = scale_decimal(other, common_scale, "basis_point_compare")?;
        let difference = left
            .checked_sub(right)
            .and_then(i128::checked_abs)
            .ok_or_else(|| anti_merge_error("basis_point_compare", "decimal_overflow"))?;
        let multiplier = match scale {
            BasisPointScale::Percent => 100_i128,
            BasisPointScale::Unit => 10_000_i128,
        };
        let scaled_difference = difference
            .checked_mul(multiplier)
            .ok_or_else(|| anti_merge_error("tolerance_bps", "decimal_overflow"))?;
        let factor = checked_pow10(common_scale, "tolerance_bps")?;
        let scaled_tolerance = i128::from(tolerance_bps)
            .checked_mul(factor)
            .ok_or_else(|| anti_merge_error("tolerance_bps", "decimal_overflow"))?;
        Ok(scaled_difference <= scaled_tolerance)
    }
}

fn scale_decimal(
    decimal: FixedDecimal,
    scale: u32,
    field: &'static str,
) -> Result<i128, StructuredAntiMergeError> {
    let factor = checked_pow10(scale - decimal.scale, field)?;
    decimal
        .mantissa
        .checked_mul(factor)
        .ok_or_else(|| anti_merge_error(field, "decimal_overflow"))
}

fn checked_pow10(exponent: u32, field: &'static str) -> Result<i128, StructuredAntiMergeError> {
    let mut value = 1i128;
    for _ in 0..exponent {
        value = value
            .checked_mul(10)
            .ok_or_else(|| anti_merge_error(field, "decimal_overflow"))?;
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IsoDate {
    year: u32,
    month: u32,
    day: u32,
}

impl IsoDate {
    fn parse_optional(
        value: &str,
        field: &'static str,
    ) -> Result<Option<Self>, StructuredAntiMergeError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let bytes = trimmed.as_bytes();
        if bytes.len() != 10 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
            return Err(anti_merge_error(field, "malformed_date"));
        }
        let year = parse_date_component(
            bytes
                .get(0..4)
                .ok_or_else(|| anti_merge_error(field, "malformed_date"))?,
            field,
        )?;
        let month = parse_date_component(
            bytes
                .get(5..7)
                .ok_or_else(|| anti_merge_error(field, "malformed_date"))?,
            field,
        )?;
        let day = parse_date_component(
            bytes
                .get(8..10)
                .ok_or_else(|| anti_merge_error(field, "malformed_date"))?,
            field,
        )?;
        if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return Err(anti_merge_error(field, "invalid_date"));
        }
        Ok(Some(Self { year, month, day }))
    }
}

fn parse_date_component(
    bytes: &[u8],
    field: &'static str,
) -> Result<u32, StructuredAntiMergeError> {
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(anti_merge_error(field, "malformed_date"));
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

fn anti_merge_error(field: &'static str, reason: &'static str) -> StructuredAntiMergeError {
    StructuredAntiMergeError { field, reason }
}
