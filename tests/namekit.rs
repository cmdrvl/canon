mod namekit {
    pub use canon::namekit::{NgramId, TokenId};
}

#[path = "namekit/mod.rs"]
mod boundary;

#[path = "namekit/anti_overmerge.rs"]
mod anti_overmerge;

#[path = "namekit/tfidf.rs"]
mod tfidf;

#[path = "namekit/review_reason_mapping.rs"]
mod review_reason_mapping;

#[path = "namekit/source_parity.rs"]
mod source_parity;

#[path = "namekit/token_ids.rs"]
mod token_ids;
