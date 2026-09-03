//! Deterministic namekit string-similarity adapter.
//!
//! This module owns the boundary between RapidFuzz's floating-point internals
//! and canon artifacts. Callers only receive integer score units, and metrics
//! remain support evidence for review/solve stages rather than merge authority.

use super::{NAMEKIT_SCORE_SCALE, SimilarityScore};
use rapidfuzz::{
    distance::{damerau_levenshtein, jaro_winkler, levenshtein},
    fuzz,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const NAMEKIT_SIMILARITY_DECISION_VERSION: &str = "canon_namekit_similarity.v0";
pub const RAPIDFUZZ_CRATE: &str = "rapidfuzz";
pub const RAPIDFUZZ_VERSION: &str = "0.5.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityMetric {
    LevenshteinNormalized,
    DamerauLevenshteinNormalized,
    JaroWinkler,
    DiceSorensen,
    TokenSortRatio,
    TokenSetRatio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityPath {
    AsciiBytes,
    UnicodeChars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SimilarityOptions {
    pub score_cutoff: Option<SimilarityScore>,
    pub score_hint: Option<SimilarityScore>,
}

impl SimilarityOptions {
    pub const fn new(
        score_cutoff: Option<SimilarityScore>,
        score_hint: Option<SimilarityScore>,
    ) -> Self {
        Self {
            score_cutoff,
            score_hint,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarityResult {
    pub metric: SimilarityMetric,
    pub path: SimilarityPath,
    pub score: Option<SimilarityScore>,
    pub passed_cutoff: bool,
    pub score_cutoff: Option<SimilarityScore>,
    pub score_hint: Option<SimilarityScore>,
    pub batch_reused: bool,
    pub evidence_only: bool,
}

pub fn normalized_similarity(
    metric: SimilarityMetric,
    left: &str,
    right: &str,
    options: SimilarityOptions,
) -> SimilarityResult {
    let path = pair_path(left, right);
    let ratio = match (metric, path) {
        (SimilarityMetric::LevenshteinNormalized, SimilarityPath::AsciiBytes) => {
            levenshtein_ascii(left, right, options)
        }
        (SimilarityMetric::LevenshteinNormalized, SimilarityPath::UnicodeChars) => {
            levenshtein_unicode(left, right, options)
        }
        (SimilarityMetric::DamerauLevenshteinNormalized, SimilarityPath::AsciiBytes) => {
            damerau_levenshtein_ascii(left, right, options)
        }
        (SimilarityMetric::DamerauLevenshteinNormalized, SimilarityPath::UnicodeChars) => {
            damerau_levenshtein_unicode(left, right, options)
        }
        (SimilarityMetric::JaroWinkler, SimilarityPath::AsciiBytes) => {
            jaro_winkler_ascii(left, right, options)
        }
        (SimilarityMetric::JaroWinkler, SimilarityPath::UnicodeChars) => {
            jaro_winkler_unicode(left, right, options)
        }
        (SimilarityMetric::DiceSorensen, _) => dice_sorensen(left, right, options),
        (SimilarityMetric::TokenSortRatio, SimilarityPath::AsciiBytes) => {
            token_sort_ratio_ascii(left, right, options)
        }
        (SimilarityMetric::TokenSortRatio, SimilarityPath::UnicodeChars) => {
            token_sort_ratio_unicode(left, right, options)
        }
        (SimilarityMetric::TokenSetRatio, _) => token_set_ratio(left, right, options),
    };

    result_from_ratio(metric, path, ratio, options, false)
}

pub fn batch_normalized_similarity(
    metric: SimilarityMetric,
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<SimilarityResult> {
    let path = batch_path(left, rights);
    let ratios = match (metric, path) {
        (SimilarityMetric::LevenshteinNormalized, SimilarityPath::AsciiBytes) => {
            batch_levenshtein_ascii(left, rights, options)
        }
        (SimilarityMetric::LevenshteinNormalized, SimilarityPath::UnicodeChars) => {
            batch_levenshtein_unicode(left, rights, options)
        }
        (SimilarityMetric::DamerauLevenshteinNormalized, SimilarityPath::AsciiBytes) => {
            batch_damerau_levenshtein_ascii(left, rights, options)
        }
        (SimilarityMetric::DamerauLevenshteinNormalized, SimilarityPath::UnicodeChars) => {
            batch_damerau_levenshtein_unicode(left, rights, options)
        }
        (SimilarityMetric::JaroWinkler, SimilarityPath::AsciiBytes) => {
            batch_jaro_winkler_ascii(left, rights, options)
        }
        (SimilarityMetric::JaroWinkler, SimilarityPath::UnicodeChars) => {
            batch_jaro_winkler_unicode(left, rights, options)
        }
        (SimilarityMetric::DiceSorensen, _) => batch_dice_sorensen(left, rights, options),
        (SimilarityMetric::TokenSortRatio, SimilarityPath::AsciiBytes) => {
            batch_token_sort_ratio_ascii(left, rights, options)
        }
        (SimilarityMetric::TokenSortRatio, SimilarityPath::UnicodeChars) => {
            batch_token_sort_ratio_unicode(left, rights, options)
        }
        (SimilarityMetric::TokenSetRatio, _) => batch_token_set_ratio(left, rights, options),
    };

    ratios
        .into_iter()
        .map(|ratio| result_from_ratio(metric, path, ratio, options, true))
        .collect()
}

pub fn score_units_from_ratio(ratio: f64) -> SimilarityScore {
    let clamped = if ratio.is_nan() {
        0.0
    } else {
        ratio.clamp(0.0, 1.0)
    };
    let scaled = (clamped * f64::from(NAMEKIT_SCORE_SCALE) + 0.5).floor() as u16;
    SimilarityScore::from_scaled(scaled).expect("clamped score units fit namekit score scale")
}

pub fn score_units_to_ratio(score: SimilarityScore) -> f64 {
    f64::from(score.as_scaled()) / f64::from(NAMEKIT_SCORE_SCALE)
}

pub fn pair_path(left: &str, right: &str) -> SimilarityPath {
    if left.is_ascii() && right.is_ascii() {
        SimilarityPath::AsciiBytes
    } else {
        SimilarityPath::UnicodeChars
    }
}

pub fn batch_path(left: &str, rights: &[&str]) -> SimilarityPath {
    if left.is_ascii() && rights.iter().all(|right| right.is_ascii()) {
        SimilarityPath::AsciiBytes
    } else {
        SimilarityPath::UnicodeChars
    }
}

fn result_from_ratio(
    metric: SimilarityMetric,
    path: SimilarityPath,
    ratio: Option<f64>,
    options: SimilarityOptions,
    batch_reused: bool,
) -> SimilarityResult {
    SimilarityResult {
        metric,
        path,
        score: ratio.map(score_units_from_ratio),
        passed_cutoff: ratio.is_some(),
        score_cutoff: options.score_cutoff,
        score_hint: options.score_hint,
        batch_reused,
        evidence_only: true,
    }
}

fn levenshtein_ascii(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => levenshtein::normalized_similarity_with_args(
            left.bytes(),
            right.bytes(),
            &levenshtein_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(levenshtein::normalized_similarity_with_args(
            left.bytes(),
            right.bytes(),
            &levenshtein_args(options.score_hint),
        )),
    }
}

fn levenshtein_unicode(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => levenshtein::normalized_similarity_with_args(
            left.chars(),
            right.chars(),
            &levenshtein_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(levenshtein::normalized_similarity_with_args(
            left.chars(),
            right.chars(),
            &levenshtein_args(options.score_hint),
        )),
    }
}

fn damerau_levenshtein_ascii(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => damerau_levenshtein::normalized_similarity_with_args(
            left.bytes(),
            right.bytes(),
            &damerau_levenshtein_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(damerau_levenshtein::normalized_similarity_with_args(
            left.bytes(),
            right.bytes(),
            &damerau_levenshtein_args(options.score_hint),
        )),
    }
}

fn damerau_levenshtein_unicode(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => damerau_levenshtein::normalized_similarity_with_args(
            left.chars(),
            right.chars(),
            &damerau_levenshtein_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(damerau_levenshtein::normalized_similarity_with_args(
            left.chars(),
            right.chars(),
            &damerau_levenshtein_args(options.score_hint),
        )),
    }
}

fn jaro_winkler_ascii(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => jaro_winkler::normalized_similarity_with_args(
            left.bytes(),
            right.bytes(),
            &jaro_winkler_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(jaro_winkler::normalized_similarity_with_args(
            left.bytes(),
            right.bytes(),
            &jaro_winkler_args(options.score_hint),
        )),
    }
}

fn jaro_winkler_unicode(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => jaro_winkler::normalized_similarity_with_args(
            left.chars(),
            right.chars(),
            &jaro_winkler_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(jaro_winkler::normalized_similarity_with_args(
            left.chars(),
            right.chars(),
            &jaro_winkler_args(options.score_hint),
        )),
    }
}

fn batch_levenshtein_ascii(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let scorer = levenshtein::BatchComparator::new(left.bytes());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = levenshtein_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.normalized_similarity_with_args(right.bytes(), &args))
                .collect()
        }
        None => {
            let args = levenshtein_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.normalized_similarity_with_args(right.bytes(), &args)))
                .collect()
        }
    }
}

fn batch_levenshtein_unicode(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let scorer = levenshtein::BatchComparator::new(left.chars());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = levenshtein_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.normalized_similarity_with_args(right.chars(), &args))
                .collect()
        }
        None => {
            let args = levenshtein_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.normalized_similarity_with_args(right.chars(), &args)))
                .collect()
        }
    }
}

fn batch_damerau_levenshtein_ascii(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let scorer = damerau_levenshtein::BatchComparator::new(left.bytes());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = damerau_levenshtein_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.normalized_similarity_with_args(right.bytes(), &args))
                .collect()
        }
        None => {
            let args = damerau_levenshtein_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.normalized_similarity_with_args(right.bytes(), &args)))
                .collect()
        }
    }
}

fn batch_damerau_levenshtein_unicode(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let scorer = damerau_levenshtein::BatchComparator::new(left.chars());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = damerau_levenshtein_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.normalized_similarity_with_args(right.chars(), &args))
                .collect()
        }
        None => {
            let args = damerau_levenshtein_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.normalized_similarity_with_args(right.chars(), &args)))
                .collect()
        }
    }
}

fn batch_jaro_winkler_ascii(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let scorer = jaro_winkler::BatchComparator::new(left.bytes());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = jaro_winkler_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.normalized_similarity_with_args(right.bytes(), &args))
                .collect()
        }
        None => {
            let args = jaro_winkler_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.normalized_similarity_with_args(right.bytes(), &args)))
                .collect()
        }
    }
}

fn batch_jaro_winkler_unicode(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let scorer = jaro_winkler::BatchComparator::new(left.chars());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = jaro_winkler_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.normalized_similarity_with_args(right.chars(), &args))
                .collect()
        }
        None => {
            let args = jaro_winkler_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.normalized_similarity_with_args(right.chars(), &args)))
                .collect()
        }
    }
}

fn dice_sorensen(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    apply_cutoff(dice_sorensen_ratio(left, right), options.score_cutoff)
}

fn batch_dice_sorensen(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    rights
        .iter()
        .map(|right| dice_sorensen(left, right, options))
        .collect()
}

fn token_sort_ratio_ascii(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    let left = sorted_token_string(left);
    let right = sorted_token_string(right);
    fuzz_ratio_ascii(&left, &right, options)
}

fn token_sort_ratio_unicode(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    let left = sorted_token_string(left);
    let right = sorted_token_string(right);
    fuzz_ratio_unicode(&left, &right, options)
}

fn batch_token_sort_ratio_ascii(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let left = sorted_token_string(left);
    let rights = rights
        .iter()
        .map(|right| sorted_token_string(right))
        .collect::<Vec<_>>();
    let scorer = fuzz::RatioBatchComparator::new(left.bytes());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = fuzz_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.similarity_with_args(right.bytes(), &args))
                .collect()
        }
        None => {
            let args = fuzz_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.similarity_with_args(right.bytes(), &args)))
                .collect()
        }
    }
}

fn batch_token_sort_ratio_unicode(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    let left = sorted_token_string(left);
    let rights = rights
        .iter()
        .map(|right| sorted_token_string(right))
        .collect::<Vec<_>>();
    let scorer = fuzz::RatioBatchComparator::new(left.chars());
    match options.score_cutoff {
        Some(cutoff) => {
            let args = fuzz_args_with_cutoff(cutoff, options.score_hint);
            rights
                .iter()
                .map(|right| scorer.similarity_with_args(right.chars(), &args))
                .collect()
        }
        None => {
            let args = fuzz_args(options.score_hint);
            rights
                .iter()
                .map(|right| Some(scorer.similarity_with_args(right.chars(), &args)))
                .collect()
        }
    }
}

fn token_set_ratio(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    apply_cutoff(token_set_ratio_value(left, right), options.score_cutoff)
}

fn batch_token_set_ratio(
    left: &str,
    rights: &[&str],
    options: SimilarityOptions,
) -> Vec<Option<f64>> {
    rights
        .iter()
        .map(|right| token_set_ratio(left, right, options))
        .collect()
}

fn fuzz_ratio_ascii(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => fuzz::ratio_with_args(
            left.bytes(),
            right.bytes(),
            &fuzz_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(fuzz::ratio_with_args(
            left.bytes(),
            right.bytes(),
            &fuzz_args(options.score_hint),
        )),
    }
}

fn fuzz_ratio_unicode(left: &str, right: &str, options: SimilarityOptions) -> Option<f64> {
    match options.score_cutoff {
        Some(cutoff) => fuzz::ratio_with_args(
            left.chars(),
            right.chars(),
            &fuzz_args_with_cutoff(cutoff, options.score_hint),
        ),
        None => Some(fuzz::ratio_with_args(
            left.chars(),
            right.chars(),
            &fuzz_args(options.score_hint),
        )),
    }
}

fn apply_cutoff(ratio: f64, cutoff: Option<SimilarityScore>) -> Option<f64> {
    match cutoff {
        Some(cutoff) if ratio < score_units_to_ratio(cutoff) => None,
        _ => Some(ratio),
    }
}

fn dice_sorensen_ratio(left: &str, right: &str) -> f64 {
    let left = char_bigram_set(left);
    let right = char_bigram_set(right);
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let overlap = left.intersection(&right).count() as f64;
    (2.0 * overlap) / ((left.len() + right.len()) as f64)
}

fn token_set_ratio_value(left: &str, right: &str) -> f64 {
    let left = sorted_unique_tokens(left);
    let right = sorted_unique_tokens(right);
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let overlap = left.intersection(&right).count() as f64;
    (2.0 * overlap) / ((left.len() + right.len()) as f64)
}

fn sorted_token_string(input: &str) -> String {
    let mut tokens = input.split_whitespace().collect::<Vec<_>>();
    tokens.sort_unstable();
    tokens.join(" ")
}

fn sorted_unique_tokens(input: &str) -> BTreeSet<&str> {
    input.split_whitespace().collect()
}

fn char_bigram_set(input: &str) -> BTreeSet<String> {
    let chars = input
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<Vec<_>>();
    match chars.len() {
        0 => BTreeSet::new(),
        1 => BTreeSet::from([chars[0].to_string()]),
        _ => chars
            .windows(2)
            .map(|window| window.iter().collect::<String>())
            .collect(),
    }
}

fn levenshtein_args(
    hint: Option<SimilarityScore>,
) -> levenshtein::Args<f64, rapidfuzz::common::NoScoreCutoff> {
    let mut args = levenshtein::Args::default();
    if let Some(hint) = hint {
        args = args.score_hint(score_units_to_ratio(hint));
    }
    args
}

fn levenshtein_args_with_cutoff(
    cutoff: SimilarityScore,
    hint: Option<SimilarityScore>,
) -> levenshtein::Args<f64, rapidfuzz::common::WithScoreCutoff<f64>> {
    levenshtein_args(hint).score_cutoff(score_units_to_ratio(cutoff))
}

fn damerau_levenshtein_args(
    hint: Option<SimilarityScore>,
) -> damerau_levenshtein::Args<f64, rapidfuzz::common::NoScoreCutoff> {
    let mut args = damerau_levenshtein::Args::default();
    if let Some(hint) = hint {
        args = args.score_hint(score_units_to_ratio(hint));
    }
    args
}

fn damerau_levenshtein_args_with_cutoff(
    cutoff: SimilarityScore,
    hint: Option<SimilarityScore>,
) -> damerau_levenshtein::Args<f64, rapidfuzz::common::WithScoreCutoff<f64>> {
    damerau_levenshtein_args(hint).score_cutoff(score_units_to_ratio(cutoff))
}

fn jaro_winkler_args(
    hint: Option<SimilarityScore>,
) -> jaro_winkler::Args<f64, rapidfuzz::common::NoScoreCutoff> {
    let mut args = jaro_winkler::Args::default();
    if let Some(hint) = hint {
        args = args.score_hint(score_units_to_ratio(hint));
    }
    args
}

fn jaro_winkler_args_with_cutoff(
    cutoff: SimilarityScore,
    hint: Option<SimilarityScore>,
) -> jaro_winkler::Args<f64, rapidfuzz::common::WithScoreCutoff<f64>> {
    jaro_winkler_args(hint).score_cutoff(score_units_to_ratio(cutoff))
}

fn fuzz_args(hint: Option<SimilarityScore>) -> fuzz::Args<f64, rapidfuzz::common::NoScoreCutoff> {
    let mut args = fuzz::Args::default();
    if let Some(hint) = hint {
        args = args.score_hint(score_units_to_ratio(hint));
    }
    args
}

fn fuzz_args_with_cutoff(
    cutoff: SimilarityScore,
    hint: Option<SimilarityScore>,
) -> fuzz::Args<f64, rapidfuzz::common::WithScoreCutoff<f64>> {
    fuzz_args(hint).score_cutoff(score_units_to_ratio(cutoff))
}
