//! Deterministic Unicode and OpenRefine-style normalization primitives.
//!
//! The v0 contract uses explicit Rust logic only: no Python, host ICU, Java,
//! locale-sensitive OS APIs, network calls, or runtime model downloads. Unknown
//! non-ASCII letters are preserved rather than erased.

use crate::namekit::{NamekitReason, ReasonCode, ReasonStage, SourceTechnique, sort_reasons};
use serde::{Deserialize, Serialize};

pub const NAMEKIT_NORMALIZATION_VERSION: &str = "canon_namekit_normalize.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationView {
    Normality,
    OpenRefineFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitNormalization {
    pub version: String,
    pub raw: String,
    pub normalized: String,
    pub fingerprint: String,
    pub lossy: bool,
    pub reasons: Vec<NamekitReason>,
}

impl NamekitNormalization {
    pub fn reason_codes(&self) -> Vec<&'static str> {
        self.reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect()
    }
}

#[derive(Debug, Default)]
struct NormalizationFlags {
    unicode_folded: bool,
    punctuation_removed: bool,
    control_removed: bool,
    whitespace_collapsed: bool,
}

pub fn normalize_text(raw: &str, view: NormalizationView) -> NamekitNormalization {
    let (normalized, flags) = normalize_base(raw);
    let fingerprint = fingerprint(&normalized);
    let mut reasons = base_reasons(&flags);

    if matches!(view, NormalizationView::OpenRefineFingerprint)
        && normalized.split_whitespace().count() > 1
    {
        reasons.push(
            NamekitReason::new(ReasonCode::TokensSorted, ReasonStage::Fingerprint)
                .with_source(SourceTechnique::CanonProfile)
                .with_detail("view", "openrefine_fingerprint"),
        );
    }
    if matches!(view, NormalizationView::OpenRefineFingerprint) && has_duplicate_tokens(&normalized)
    {
        reasons.push(
            NamekitReason::new(ReasonCode::TokensDeduped, ReasonStage::Fingerprint)
                .with_source(SourceTechnique::CanonProfile)
                .with_detail("view", "openrefine_fingerprint"),
        );
    }

    reasons.push(
        NamekitReason::new(ReasonCode::SourceParityReference, ReasonStage::SourceParity)
            .with_source(SourceTechnique::Normality)
            .with_detail(
                "technique",
                match view {
                    NormalizationView::Normality => "normality_observable_semantics",
                    NormalizationView::OpenRefineFingerprint => {
                        "openrefine_key_collision_fingerprint"
                    }
                },
            ),
    );

    if !reasons.iter().any(|reason| reason.lossy) {
        reasons.push(
            NamekitReason::new(ReasonCode::NoLoss, ReasonStage::Normalize)
                .with_source(SourceTechnique::CanonProfile)
                .with_detail("operation", "no_lossy_transform"),
        );
    }

    sort_reasons(&mut reasons);
    let lossy = reasons.iter().any(|reason| reason.lossy);
    NamekitNormalization {
        version: NAMEKIT_NORMALIZATION_VERSION.to_string(),
        raw: raw.to_string(),
        normalized,
        fingerprint,
        lossy,
        reasons,
    }
}

pub fn normalize_normality(raw: &str) -> NamekitNormalization {
    normalize_text(raw, NormalizationView::Normality)
}

pub fn normalize_openrefine_fingerprint(raw: &str) -> NamekitNormalization {
    normalize_text(raw, NormalizationView::OpenRefineFingerprint)
}

fn normalize_base(raw: &str) -> (String, NormalizationFlags) {
    let mut flags = NormalizationFlags::default();
    let mut output = String::with_capacity(raw.len());
    let mut last_was_separator = true;

    for character in raw.chars() {
        if character.is_control() {
            flags.control_removed = true;
            push_separator(&mut output, &mut last_was_separator, &mut flags);
            continue;
        }
        if character.is_whitespace() {
            push_separator(&mut output, &mut last_was_separator, &mut flags);
            continue;
        }
        if is_punctuation_or_symbol(character) {
            flags.punctuation_removed = true;
            push_separator(&mut output, &mut last_was_separator, &mut flags);
            continue;
        }
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            last_was_separator = false;
            continue;
        }
        if let Some(folded) = fold_latin_ascii(character) {
            flags.unicode_folded = true;
            output.push_str(folded);
            last_was_separator = false;
            continue;
        }

        for folded in character.to_lowercase() {
            output.push(folded);
        }
        last_was_separator = false;
    }

    if output.ends_with(' ') {
        flags.whitespace_collapsed = true;
        output.pop();
    }

    (output, flags)
}

fn push_separator(
    output: &mut String,
    last_was_separator: &mut bool,
    flags: &mut NormalizationFlags,
) {
    flags.whitespace_collapsed = true;
    if output.is_empty() || *last_was_separator {
        return;
    }
    output.push(' ');
    *last_was_separator = true;
}

fn base_reasons(flags: &NormalizationFlags) -> Vec<NamekitReason> {
    let mut reasons = Vec::new();
    if flags.unicode_folded {
        reasons.push(
            NamekitReason::new(ReasonCode::UnicodeFolded, ReasonStage::Normalize)
                .with_source(SourceTechnique::Normality)
                .with_detail("operation", "latin_ascii_fold"),
        );
    }
    if flags.punctuation_removed {
        reasons.push(
            NamekitReason::new(ReasonCode::PunctuationRemoved, ReasonStage::Normalize)
                .with_source(SourceTechnique::Normality)
                .with_detail("operation", "punctuation_to_separator"),
        );
    }
    if flags.control_removed {
        reasons.push(
            NamekitReason::new(ReasonCode::ControlRemoved, ReasonStage::Normalize)
                .with_source(SourceTechnique::Normality)
                .with_detail("operation", "control_to_separator"),
        );
    }
    if flags.whitespace_collapsed {
        reasons.push(
            NamekitReason::new(ReasonCode::WhitespaceCollapsed, ReasonStage::Normalize)
                .with_source(SourceTechnique::Normality)
                .with_detail("operation", "trim_and_single_space"),
        );
    }
    reasons
}

fn fingerprint(normalized: &str) -> String {
    let mut tokens = normalized
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    tokens.sort_unstable();
    tokens.dedup();
    tokens.join(" ")
}

fn has_duplicate_tokens(normalized: &str) -> bool {
    let mut tokens = normalized
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let original_len = tokens.len();
    tokens.sort_unstable();
    tokens.dedup();
    tokens.len() != original_len
}

fn is_punctuation_or_symbol(character: char) -> bool {
    character.is_ascii_punctuation()
        || matches!(
            character,
            '¡' | '¿'
                | '§'
                | '¶'
                | '·'
                | '‐'
                | '‑'
                | '‒'
                | '–'
                | '—'
                | '―'
                | '‘'
                | '’'
                | '‚'
                | '“'
                | '”'
                | '„'
                | '†'
                | '‡'
                | '•'
                | '…'
                | '‰'
                | '′'
                | '″'
                | '‹'
                | '›'
                | '€'
                | '™'
        )
}

fn fold_latin_ascii(character: char) -> Option<&'static str> {
    match character {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å'
        | 'ā' | 'ă' | 'ą' => Some("a"),
        'Æ' | 'æ' => Some("ae"),
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' | 'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => Some("c"),
        'Ð' | 'Ď' | 'Đ' | 'ð' | 'ď' | 'đ' => Some("d"),
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ'
        | 'ė' | 'ę' | 'ě' => Some("e"),
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' | 'ĝ' | 'ğ' | 'ġ' | 'ģ' => Some("g"),
        'Ĥ' | 'Ħ' | 'ĥ' | 'ħ' => Some("h"),
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' | 'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī'
        | 'ĭ' | 'į' | 'ı' => Some("i"),
        'Ĵ' | 'ĵ' => Some("j"),
        'Ķ' | 'ķ' => Some("k"),
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' | 'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => Some("l"),
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'ñ' | 'ń' | 'ņ' | 'ň' => Some("n"),
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø'
        | 'ō' | 'ŏ' | 'ő' => Some("o"),
        'Œ' | 'œ' => Some("oe"),
        'Ŕ' | 'Ŗ' | 'Ř' | 'ŕ' | 'ŗ' | 'ř' => Some("r"),
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'ś' | 'ŝ' | 'ş' | 'š' => Some("s"),
        'ß' => Some("ss"),
        'Ţ' | 'Ť' | 'Ŧ' | 'ţ' | 'ť' | 'ŧ' => Some("t"),
        'Þ' | 'þ' => Some("th"),
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' | 'ù' | 'ú' | 'û' | 'ü' | 'ũ'
        | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => Some("u"),
        'Ŵ' | 'ŵ' => Some("w"),
        'Ý' | 'Ŷ' | 'Ÿ' | 'ý' | 'ÿ' | 'ŷ' => Some("y"),
        'Ź' | 'Ż' | 'Ž' | 'ź' | 'ż' | 'ž' => Some("z"),
        _ => None,
    }
}
