//! Deterministic tokenization primitives for normalized entity-name views.
//!
//! Tokenization is deliberately small and locale-free: split normalized text on
//! Unicode whitespace, keep token bytes intact, and assign IDs from the
//! canonical sorted symbol table rather than from row order.

use crate::namekit::ids::TokenSymbolTable;
use crate::namekit::{
    NamekitReason, NamekitToken, ReasonCode, ReasonStage, SourceTechnique, sort_reasons,
};
use serde::{Deserialize, Serialize};

pub const NAMEKIT_TOKENIZE_VERSION: &str = "canon_namekit_tokenize.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenizeView {
    Sequence,
    SortedUnique,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitTokenization {
    pub version: String,
    pub raw: String,
    pub view: TokenizeView,
    pub tokens: Vec<NamekitToken>,
    pub symbol_table: TokenSymbolTable,
    pub lossy: bool,
    pub reasons: Vec<NamekitReason>,
}

pub fn tokenize_words(input: &str) -> NamekitTokenization {
    tokenize(input, TokenizeView::Sequence)
}

pub fn tokenize_sorted_unique(input: &str) -> NamekitTokenization {
    tokenize(input, TokenizeView::SortedUnique)
}

pub fn tokenize(input: &str, view: TokenizeView) -> NamekitTokenization {
    let source_tokens = split_words(input);
    let (tokens, mut reasons) = match view {
        TokenizeView::Sequence => (source_tokens, Vec::new()),
        TokenizeView::SortedUnique => sorted_unique_tokens(source_tokens),
    };
    sort_reasons(&mut reasons);
    let lossy = reasons.iter().any(|reason| reason.lossy);
    let symbol_table = TokenSymbolTable::from_tokens(tokens.iter().cloned());
    let tokens = tokens
        .into_iter()
        .map(|text| NamekitToken::new(symbol_table.token_id(&text), text))
        .collect();

    NamekitTokenization {
        version: NAMEKIT_TOKENIZE_VERSION.to_string(),
        raw: input.to_string(),
        view,
        tokens,
        symbol_table,
        lossy,
        reasons,
    }
}

fn split_words(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn sorted_unique_tokens(mut tokens: Vec<String>) -> (Vec<String>, Vec<NamekitReason>) {
    let original = tokens.clone();
    tokens.sort();
    let sorted = tokens != original;

    let sorted_len = tokens.len();
    tokens.dedup();
    let deduped = tokens.len() != sorted_len;

    let mut reasons = Vec::new();
    if sorted {
        reasons.push(
            NamekitReason::new(ReasonCode::TokensSorted, ReasonStage::Tokenize)
                .with_source(SourceTechnique::OpenSanctionsRigour)
                .with_detail("operation", "token_sort"),
        );
    }
    if deduped {
        reasons.push(
            NamekitReason::new(ReasonCode::TokensDeduped, ReasonStage::Tokenize)
                .with_source(SourceTechnique::OpenSanctionsRigour)
                .with_detail("operation", "token_dedupe"),
        );
    }

    (tokens, reasons)
}
