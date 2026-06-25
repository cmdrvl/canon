//! Compact deterministic posting-list layout for entity index accelerators.
//!
//! The layout is intentionally semantic-free: it stores corpus-local feature
//! IDs, CSR-style offsets, and sorted postings so later block/edge stages can
//! reload indexes without choosing a second sparse representation.

use serde::{Deserialize, Serialize};
use std::{fmt, ops::Range};

pub const ENTITY_POSTINGS_LAYOUT_VERSION: &str = "canon_entity_postings.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingFeatureKind {
    ExactView,
    Token,
    Ngram,
    TfidfTerm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingDictionaryEntry {
    pub kind: PostingFeatureKind,
    pub term_id: u32,
    pub key: String,
}

impl PostingDictionaryEntry {
    pub fn new(kind: PostingFeatureKind, term_id: u32, key: impl Into<String>) -> Self {
        Self {
            kind,
            term_id,
            key: key.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingInput {
    pub term_id: u32,
    pub surface_ordinal: u32,
    pub weight_units: u64,
}

impl PostingInput {
    pub const fn new(term_id: u32, surface_ordinal: u32, weight_units: u64) -> Self {
        Self {
            term_id,
            surface_ordinal,
            weight_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingRecord {
    pub surface_ordinal: u32,
    pub weight_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonPostingDiagnostic {
    pub term_id: u32,
    pub key: String,
    pub posting_count: usize,
    pub configured_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingLayout {
    pub version: String,
    pub surface_count: u32,
    pub dictionary_hash: String,
    pub dictionary: Vec<PostingDictionaryEntry>,
    pub term_offsets: Vec<usize>,
    pub postings: Vec<PostingRecord>,
    pub common_posting_diagnostics: Vec<CommonPostingDiagnostic>,
}

impl PostingLayout {
    pub fn build(
        surface_count: u32,
        dictionary: Vec<PostingDictionaryEntry>,
        postings: Vec<PostingInput>,
        common_posting_limit: usize,
    ) -> Result<Self, PostingLayoutError> {
        let dictionary = normalize_dictionary(dictionary)?;
        let dictionary_hash = dictionary_hash(&dictionary)?;
        let mut postings_by_term = vec![Vec::<PostingRecord>::new(); dictionary.len()];

        for posting in postings {
            let term_index = usize::try_from(posting.term_id)
                .map_err(|_| PostingLayoutError::TermIdOverflow(posting.term_id))?;
            if term_index >= dictionary.len() {
                return Err(PostingLayoutError::UnknownTermId(posting.term_id));
            }
            if posting.surface_ordinal >= surface_count {
                return Err(PostingLayoutError::SurfaceOrdinalOutOfRange {
                    term_id: posting.term_id,
                    surface_ordinal: posting.surface_ordinal,
                    surface_count,
                });
            }
            postings_by_term[term_index].push(PostingRecord {
                surface_ordinal: posting.surface_ordinal,
                weight_units: posting.weight_units,
            });
        }

        let mut term_offsets = Vec::with_capacity(dictionary.len() + 1);
        let mut flattened = Vec::new();
        let mut diagnostics = Vec::new();
        term_offsets.push(0);

        for (term_index, mut term_postings) in postings_by_term.into_iter().enumerate() {
            term_postings.sort_by_key(|posting| posting.surface_ordinal);
            reject_duplicate_surface(dictionary[term_index].term_id, &term_postings)?;
            if common_posting_limit > 0 && term_postings.len() > common_posting_limit {
                diagnostics.push(CommonPostingDiagnostic {
                    term_id: dictionary[term_index].term_id,
                    key: dictionary[term_index].key.clone(),
                    posting_count: term_postings.len(),
                    configured_limit: common_posting_limit,
                });
            }
            flattened.extend(term_postings);
            term_offsets.push(flattened.len());
        }

        let layout = Self {
            version: ENTITY_POSTINGS_LAYOUT_VERSION.to_string(),
            surface_count,
            dictionary_hash,
            dictionary,
            term_offsets,
            postings: flattened,
            common_posting_diagnostics: diagnostics,
        };
        layout.validate_reload()?;
        Ok(layout)
    }

    pub fn postings_for_term(&self, term_id: u32) -> Result<&[PostingRecord], PostingLayoutError> {
        let range = self.posting_range(term_id)?;
        Ok(&self.postings[range])
    }

    pub fn posting_range(&self, term_id: u32) -> Result<Range<usize>, PostingLayoutError> {
        let index =
            usize::try_from(term_id).map_err(|_| PostingLayoutError::TermIdOverflow(term_id))?;
        if index >= self.dictionary.len() {
            return Err(PostingLayoutError::UnknownTermId(term_id));
        }
        Ok(self.term_offsets[index]..self.term_offsets[index + 1])
    }

    pub fn validate_reload(&self) -> Result<(), PostingLayoutError> {
        if self.version != ENTITY_POSTINGS_LAYOUT_VERSION {
            return Err(PostingLayoutError::VersionMismatch {
                expected: ENTITY_POSTINGS_LAYOUT_VERSION,
                actual: self.version.clone(),
            });
        }
        validate_dictionary(&self.dictionary)?;

        let expected_hash = dictionary_hash(&self.dictionary)?;
        if self.dictionary_hash != expected_hash {
            return Err(PostingLayoutError::DictionaryHashMismatch {
                expected: expected_hash,
                actual: self.dictionary_hash.clone(),
            });
        }
        if self.term_offsets.len() != self.dictionary.len() + 1 {
            return Err(PostingLayoutError::OffsetLengthMismatch {
                expected: self.dictionary.len() + 1,
                actual: self.term_offsets.len(),
            });
        }
        if self.term_offsets.first().copied() != Some(0) {
            return Err(PostingLayoutError::OffsetStartMismatch);
        }
        if self.term_offsets.last().copied() != Some(self.postings.len()) {
            return Err(PostingLayoutError::OffsetEndMismatch {
                expected: self.postings.len(),
                actual: self.term_offsets.last().copied().unwrap_or_default(),
            });
        }
        for offsets in self.term_offsets.windows(2) {
            if offsets[0] > offsets[1] {
                return Err(PostingLayoutError::OffsetsNotMonotonic);
            }
        }
        for (term_index, entry) in self.dictionary.iter().enumerate() {
            let start = self.term_offsets[term_index];
            let end = self.term_offsets[term_index + 1];
            let slice = &self.postings[start..end];
            reject_duplicate_surface(entry.term_id, slice)?;
            if slice
                .iter()
                .any(|posting| posting.surface_ordinal >= self.surface_count)
            {
                return Err(PostingLayoutError::SurfaceOrdinalOutOfRange {
                    term_id: entry.term_id,
                    surface_ordinal: slice
                        .iter()
                        .find(|posting| posting.surface_ordinal >= self.surface_count)
                        .map(|posting| posting.surface_ordinal)
                        .unwrap_or_default(),
                    surface_count: self.surface_count,
                });
            }
            if !slice
                .windows(2)
                .all(|pair| pair[0].surface_ordinal < pair[1].surface_ordinal)
            {
                return Err(PostingLayoutError::PostingsNotSorted {
                    term_id: entry.term_id,
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostingLayoutError {
    DictionaryHashMismatch {
        expected: String,
        actual: String,
    },
    DictionaryNotCompact {
        expected: u32,
        actual: u32,
    },
    DictionaryNotSorted,
    DuplicateDictionaryTermId(u32),
    DuplicateSurfaceForTerm {
        term_id: u32,
        surface_ordinal: u32,
    },
    OffsetEndMismatch {
        expected: usize,
        actual: usize,
    },
    OffsetLengthMismatch {
        expected: usize,
        actual: usize,
    },
    OffsetStartMismatch,
    OffsetsNotMonotonic,
    PostingsNotSorted {
        term_id: u32,
    },
    SurfaceOrdinalOutOfRange {
        term_id: u32,
        surface_ordinal: u32,
        surface_count: u32,
    },
    TermIdOverflow(u32),
    UnknownTermId(u32),
    VersionMismatch {
        expected: &'static str,
        actual: String,
    },
}

impl fmt::Display for PostingLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PostingLayoutError {}

fn normalize_dictionary(
    mut dictionary: Vec<PostingDictionaryEntry>,
) -> Result<Vec<PostingDictionaryEntry>, PostingLayoutError> {
    dictionary.sort_by_key(|entry| (entry.term_id, entry.kind, entry.key.clone()));
    validate_dictionary(&dictionary)?;
    Ok(dictionary)
}

fn validate_dictionary(dictionary: &[PostingDictionaryEntry]) -> Result<(), PostingLayoutError> {
    let mut previous_key: Option<(u32, PostingFeatureKind, &str)> = None;
    for (index, entry) in dictionary.iter().enumerate() {
        let expected = u32::try_from(index).map_err(|_| PostingLayoutError::TermIdOverflow(0))?;
        if entry.term_id != expected {
            return Err(PostingLayoutError::DictionaryNotCompact {
                expected,
                actual: entry.term_id,
            });
        }
        let key = (entry.term_id, entry.kind, entry.key.as_str());
        if let Some(previous) = previous_key
            && previous >= key
        {
            if previous.0 == entry.term_id {
                return Err(PostingLayoutError::DuplicateDictionaryTermId(entry.term_id));
            }
            return Err(PostingLayoutError::DictionaryNotSorted);
        }
        previous_key = Some(key);
    }
    Ok(())
}

fn reject_duplicate_surface(
    term_id: u32,
    postings: &[PostingRecord],
) -> Result<(), PostingLayoutError> {
    for pair in postings.windows(2) {
        if pair[0].surface_ordinal == pair[1].surface_ordinal {
            return Err(PostingLayoutError::DuplicateSurfaceForTerm {
                term_id,
                surface_ordinal: pair[0].surface_ordinal,
            });
        }
    }
    Ok(())
}

fn dictionary_hash(dictionary: &[PostingDictionaryEntry]) -> Result<String, PostingLayoutError> {
    validate_dictionary(dictionary)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(ENTITY_POSTINGS_LAYOUT_VERSION.as_bytes());
    for entry in dictionary {
        hasher.update(entry.term_id.to_string().as_bytes());
        hasher.update(&[0]);
        hasher.update(format!("{:?}", entry.kind).as_bytes());
        hasher.update(&[0]);
        hasher.update(entry.key.len().to_string().as_bytes());
        hasher.update(&[0]);
        hasher.update(entry.key.as_bytes());
        hasher.update(&[0xff]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}
