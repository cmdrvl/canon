#[path = "../../src/namekit/ids.rs"]
mod ids;

use crate::namekit::{NgramId, TokenId};
use ids::{NAMEKIT_IDS_VERSION, NgramSymbolTable, TokenSymbolTable};
use serde_json::json;

#[test]
fn namekit_symbol_ids() {
    let table = TokenSymbolTable::from_tokens(["sears", "llc", "sears", "auto"]);

    assert_eq!(table.version, NAMEKIT_IDS_VERSION);
    assert_eq!(
        table
            .entries
            .iter()
            .map(|entry| (entry.id.as_u32(), entry.value.as_str()))
            .collect::<Vec<_>>(),
        [(0, "auto"), (1, "llc"), (2, "sears")]
    );
    assert_eq!(table.token_id("sears").unwrap().as_u32(), 2);
    assert_eq!(table.token_id("missing"), None);
    assert_eq!(table.token(TokenId::new(0)), Some("auto"));
    assert_eq!(table.token(TokenId::new(99)), None);

    let payload = serde_json::to_value(&table).expect("table serializes");
    assert_eq!(
        payload,
        json!({
            "version": "canon_namekit_ids.v0",
            "entries": [
                {"id": 0, "value": "auto"},
                {"id": 1, "value": "llc"},
                {"id": 2, "value": "sears"}
            ]
        })
    );
}

#[test]
fn token_ids_row_order_stability() {
    let first = TokenSymbolTable::from_tokens(["sears", "roebuck", "co", "sears"]);
    let second = TokenSymbolTable::from_tokens(["co", "sears", "roebuck", "sears"]);

    assert_eq!(first, second);
    assert_eq!(
        first
            .entries
            .iter()
            .map(|entry| entry.id.as_u32())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn ngram_ids_are_compact_and_lookupable() {
    let table = NgramSymbolTable::from_ngrams(["sea", "ear", "ars", "sea"]);

    assert_eq!(
        table
            .entries
            .iter()
            .map(|entry| (entry.id.as_u32(), entry.value.as_str()))
            .collect::<Vec<_>>(),
        [(0, "ars"), (1, "ear"), (2, "sea")]
    );
    assert_eq!(table.ngram_id("ear").unwrap().as_u32(), 1);
    assert_eq!(table.ngram_id("missing"), None);
    assert_eq!(table.ngram(NgramId::new(2)), Some("sea"));
    assert_eq!(table.ngram(NgramId::new(99)), None);
}
