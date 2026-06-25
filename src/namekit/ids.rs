//! Compact deterministic symbol tables for namekit token and ngram features.
//!
//! IDs are assigned from the sorted unique symbol set, not from raw row order.
//! This keeps downstream sparse features and artifacts byte-stable when inputs
//! are presented in a different order.

use crate::namekit::{NgramId, TokenId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const NAMEKIT_IDS_VERSION: &str = "canon_namekit_ids.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSymbol {
    pub id: TokenId,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSymbolTable {
    pub version: String,
    pub entries: Vec<TokenSymbol>,
}

impl TokenSymbolTable {
    pub fn from_tokens(tokens: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let entries = unique_sorted(tokens)
            .into_iter()
            .enumerate()
            .map(|(index, value)| TokenSymbol {
                id: TokenId::new(symbol_index(index)),
                value,
            })
            .collect();

        Self {
            version: NAMEKIT_IDS_VERSION.to_string(),
            entries,
        }
    }

    pub fn token_id(&self, token: &str) -> Option<TokenId> {
        self.entries
            .binary_search_by(|entry| entry.value.as_str().cmp(token))
            .ok()
            .map(|index| self.entries[index].id)
    }

    pub fn token(&self, id: TokenId) -> Option<&str> {
        let index = usize::try_from(id.as_u32()).ok()?;
        self.entries
            .get(index)
            .filter(|entry| entry.id == id)
            .map(|entry| entry.value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NgramSymbol {
    pub id: NgramId,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NgramSymbolTable {
    pub version: String,
    pub entries: Vec<NgramSymbol>,
}

impl NgramSymbolTable {
    pub fn from_ngrams(ngrams: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let entries = unique_sorted(ngrams)
            .into_iter()
            .enumerate()
            .map(|(index, value)| NgramSymbol {
                id: NgramId::new(symbol_index(index)),
                value,
            })
            .collect();

        Self {
            version: NAMEKIT_IDS_VERSION.to_string(),
            entries,
        }
    }

    pub fn ngram_id(&self, ngram: &str) -> Option<NgramId> {
        self.entries
            .binary_search_by(|entry| entry.value.as_str().cmp(ngram))
            .ok()
            .map(|index| self.entries[index].id)
    }

    pub fn ngram(&self, id: NgramId) -> Option<&str> {
        let index = usize::try_from(id.as_u32()).ok()?;
        self.entries
            .get(index)
            .filter(|entry| entry.id == id)
            .map(|entry| entry.value.as_str())
    }
}

fn unique_sorted(values: impl IntoIterator<Item = impl Into<String>>) -> Vec<String> {
    values
        .into_iter()
        .map(Into::into)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn symbol_index(index: usize) -> u32 {
    u32::try_from(index).expect("namekit symbol table exceeded u32 id space")
}
