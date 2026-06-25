//! Deterministic character n-gram primitives for namekit feature views.
//!
//! The module operates on Rust `char` boundaries, removes whitespace before
//! windowing, and assigns stable IDs through the canonical n-gram symbol table.

use crate::namekit::ids::NgramSymbolTable;
use crate::namekit::{
    NamekitNgram, NamekitReason, ReasonCode, ReasonStage, SourceTechnique, sort_reasons,
};
use serde::{Deserialize, Serialize};

pub const NAMEKIT_NGRAM_VERSION: &str = "canon_namekit_ngram.v0";
pub const DEFAULT_CHAR_NGRAM_WIDTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NgramConfig {
    pub width: usize,
}

impl NgramConfig {
    pub const DEFAULT: Self = Self {
        width: DEFAULT_CHAR_NGRAM_WIDTH,
    };

    pub const fn new(width: usize) -> Option<Self> {
        if width == 0 {
            None
        } else {
            Some(Self { width })
        }
    }
}

impl Default for NgramConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NgramView {
    Sequence,
    Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitNgramSet {
    pub version: String,
    pub raw: String,
    pub view: NgramView,
    pub width: usize,
    pub ngrams: Vec<NamekitNgram>,
    pub symbol_table: NgramSymbolTable,
    pub fingerprint: String,
    pub lossy: bool,
    pub reasons: Vec<NamekitReason>,
}

pub fn trigrams(input: &str) -> NamekitNgramSet {
    char_ngrams(input, NgramConfig::DEFAULT)
}

pub fn char_ngrams(input: &str, config: NgramConfig) -> NamekitNgramSet {
    ngrams(input, config, NgramView::Sequence)
}

pub fn ngram_fingerprint(input: &str, config: NgramConfig) -> NamekitNgramSet {
    ngrams(input, config, NgramView::Fingerprint)
}

pub fn ngrams(input: &str, config: NgramConfig, view: NgramView) -> NamekitNgramSet {
    let source_ngrams = ngram_windows(input, config.width);
    let (ngrams, mut reasons) = match view {
        NgramView::Sequence => (source_ngrams, Vec::new()),
        NgramView::Fingerprint => fingerprint_ngrams(source_ngrams, config.width),
    };
    sort_reasons(&mut reasons);
    let lossy = reasons.iter().any(|reason| reason.lossy);
    let symbol_table = NgramSymbolTable::from_ngrams(ngrams.iter().cloned());
    let fingerprint = ngrams.join(" ");
    let ngrams = ngrams
        .into_iter()
        .map(|text| NamekitNgram::new(symbol_table.ngram_id(&text), text))
        .collect();

    NamekitNgramSet {
        version: NAMEKIT_NGRAM_VERSION.to_string(),
        raw: input.to_string(),
        view,
        width: config.width,
        ngrams,
        symbol_table,
        fingerprint,
        lossy,
        reasons,
    }
}

fn ngram_windows(input: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let chars = input
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<Vec<_>>();
    if chars.len() < width {
        return Vec::new();
    }

    chars
        .windows(width)
        .map(|window| window.iter().collect::<String>())
        .collect()
}

fn fingerprint_ngrams(mut ngrams: Vec<String>, width: usize) -> (Vec<String>, Vec<NamekitReason>) {
    ngrams.sort();
    ngrams.dedup();
    let reasons = vec![
        NamekitReason::new(
            ReasonCode::NgramFingerprintCollision,
            ReasonStage::Fingerprint,
        )
        .with_source(SourceTechnique::IngEntityMatchingModel)
        .with_detail("operation", "ngram_fingerprint")
        .with_detail("width", width.to_string()),
    ];

    (ngrams, reasons)
}
