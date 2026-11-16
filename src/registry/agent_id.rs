//! Agent-friendly entity ids: `<kind prefix>_<10 body symbols><2 check symbols>`.
//!
//! The body is derived from the founding outcome (so a replay, or a second
//! agent minting the same entity from the same key, gets the same id) and the
//! two check symbols make every single-symbol substitution and every adjacent
//! transposition detectable before any lookup.

use std::fmt;

/// 32 symbols, values 0..=31 in this order: no `i`, `l`, `o`, `u`.
pub const AGENT_ID_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
pub const AGENT_ID_BODY_LEN: usize = 10;
pub const AGENT_ID_CHECK_LEN: usize = 2;
pub const AGENT_ID_SYMBOL_LEN: usize = AGENT_ID_BODY_LEN + AGENT_ID_CHECK_LEN;
const CHECK_MODULUS: u32 = 1021;
const BODY_DOMAIN: &str = "canon.id.v1";

/// 3^j mod 1021 for j = 1..=10. 1021 is prime and the weights are distinct and
/// non-zero.
const WEIGHTS: [u32; AGENT_ID_BODY_LEN] = [3, 9, 27, 81, 243, 729, 145, 435, 284, 852];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentIdRejection {
    BadLength,
    BadSymbol,
    UnknownKind,
    BadCheck,
}

impl AgentIdRejection {
    pub fn reason(self) -> &'static str {
        match self {
            Self::BadLength => "bad_length",
            Self::BadSymbol => "bad_symbol",
            Self::UnknownKind => "unknown_kind",
            Self::BadCheck => "bad_check",
        }
    }
}

impl fmt::Display for AgentIdRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

/// A well-formed agent id split into its parts. `id` is the lower-case id
/// without any label; `label` is the advisory text after `~`, when present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAgentId {
    pub prefix: String,
    pub id: String,
    pub label: Option<String>,
}

/// A kind prefix is lower-case ASCII letters followed by one underscore.
pub fn kind_prefix_is_valid(prefix: &str) -> bool {
    let Some(letters) = prefix.strip_suffix('_') else {
        return false;
    };
    !letters.is_empty() && letters.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn symbol_value(symbol: u8) -> Option<u32> {
    AGENT_ID_ALPHABET
        .iter()
        .position(|candidate| *candidate == symbol)
        .map(|position| position as u32)
}

/// The two check symbols for a 10-symbol body, or `None` when the body has the
/// wrong length or a symbol outside the alphabet. The body is lower-cased
/// before checking.
pub fn agent_id_check_symbols(body: &str) -> Option<String> {
    let bytes = body.to_ascii_lowercase().into_bytes();
    if bytes.len() != AGENT_ID_BODY_LEN {
        return None;
    }
    let mut sum = 0u32;
    for (weight, symbol) in WEIGHTS.iter().zip(bytes.iter()) {
        sum += weight * symbol_value(*symbol)?;
    }
    let check = sum % CHECK_MODULUS;
    let high = AGENT_ID_ALPHABET[(check / 32) as usize] as char;
    let low = AGENT_ID_ALPHABET[(check % 32) as usize] as char;
    Some(format!("{high}{low}"))
}

/// Parse and verify an agent id, optionally carrying `~label`. `known_prefixes`
/// are the kind prefixes (with their trailing underscore) the caller accepts.
/// Refusals come in the order length, symbol, kind, check.
pub fn parse_agent_id(
    input: &str,
    known_prefixes: &[&str],
) -> Result<ParsedAgentId, AgentIdRejection> {
    let (id_part, label) = match input.split_once('~') {
        Some((id_part, label)) => (id_part, Some(label.to_string())),
        None => (input, None),
    };
    let lowered = id_part.to_ascii_lowercase();
    let Some(split) = lowered.find('_').map(|index| index + 1) else {
        // No kind prefix at all: nothing to measure the symbols against.
        return Err(if lowered.is_empty() {
            AgentIdRejection::BadLength
        } else {
            AgentIdRejection::UnknownKind
        });
    };
    let (prefix, symbols) = lowered.split_at(split);
    if symbols.len() != AGENT_ID_SYMBOL_LEN {
        return Err(AgentIdRejection::BadLength);
    }
    if symbols.bytes().any(|symbol| symbol_value(symbol).is_none()) {
        return Err(AgentIdRejection::BadSymbol);
    }
    if !kind_prefix_is_valid(prefix) || !known_prefixes.contains(&prefix) {
        return Err(AgentIdRejection::UnknownKind);
    }
    let (body, check) = symbols.split_at(AGENT_ID_BODY_LEN);
    match agent_id_check_symbols(body) {
        Some(expected) if expected == check => Ok(ParsedAgentId {
            prefix: prefix.to_string(),
            id: lowered,
            label,
        }),
        _ => Err(AgentIdRejection::BadCheck),
    }
}

/// The 10-symbol body for `(prefix, outcome_ref, n)`: the first 50 bits of
/// blake3(`canon.id.v1` | prefix | outcome_ref | n), most significant first.
pub fn agent_id_body(prefix: &str, outcome_ref: &str, n: u32) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in [BODY_DOMAIN, prefix, outcome_ref, &n.to_string()] {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    let digest = hasher.finalize();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest.as_bytes()[..8]);
    let bits = u64::from_be_bytes(head) >> 14;
    (0..AGENT_ID_BODY_LEN)
        .map(|position| {
            let shift = 5 * (AGENT_ID_BODY_LEN - 1 - position);
            AGENT_ID_ALPHABET[((bits >> shift) & 31) as usize] as char
        })
        .collect()
}

/// Mint the id for a founding outcome. `taken` answers whether an id already
/// belongs to a different entity; the counter `n` rises until the id is free
/// and the body is not all digits. Returns the id and the `n` that produced it.
pub fn mint_agent_id(
    prefix: &str,
    outcome_ref: &str,
    taken: &dyn Fn(&str) -> bool,
) -> Result<(String, u32), AgentIdRejection> {
    if !kind_prefix_is_valid(prefix) {
        return Err(AgentIdRejection::UnknownKind);
    }
    let mut n = 0u32;
    loop {
        let body = agent_id_body(prefix, outcome_ref, n);
        let check = agent_id_check_symbols(&body).ok_or(AgentIdRejection::BadSymbol)?;
        let id = format!("{prefix}{body}{check}");
        if !body.bytes().all(|symbol| symbol.is_ascii_digit()) && !taken(&id) {
            return Ok((id, n));
        }
        n += 1;
    }
}

/// Lower-case, runs of non-alphanumerics as one `-`, edges trimmed.
pub fn label_slug(text: &str) -> String {
    let mut slug = String::new();
    let mut pending_separator = false;
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            pending_separator = false;
            slug.push(character);
        } else {
            pending_separator = true;
        }
    }
    slug
}

/// True when `label` names one of the entity's aliases after slugging both.
pub fn label_matches<'a>(label: &str, aliases: impl IntoIterator<Item = &'a str>) -> bool {
    let wanted = label_slug(label);
    !wanted.is_empty() && aliases.into_iter().any(|alias| label_slug(alias) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: &[&str] = &["org_", "per_"];

    #[test]
    fn published_vectors_hold() {
        assert_eq!(agent_id_check_symbols("7k3m9q2xh4").as_deref(), Some("3z"));
        assert_eq!(agent_id_check_symbols("4f9k2m7q8x").as_deref(), Some("vj"));
        assert!(parse_agent_id("org_7k3m9q2xh43z", KINDS).is_ok());
        assert!(parse_agent_id("per_4f9k2m7q8xvj", KINDS).is_ok());
    }

    #[test]
    fn every_single_symbol_substitution_is_refused() {
        let id = "org_7k3m9q2xh43z";
        let start = "org_".len();
        for position in start..id.len() {
            for replacement in AGENT_ID_ALPHABET {
                let replacement = *replacement as char;
                if id.as_bytes()[position] as char == replacement {
                    continue;
                }
                let mut mutated = id.to_string();
                mutated.replace_range(position..=position, &replacement.to_string());
                assert_eq!(
                    parse_agent_id(&mutated, KINDS),
                    Err(AgentIdRejection::BadCheck),
                    "substitution at {position} with {replacement} slipped through: {mutated}"
                );
            }
        }
    }

    #[test]
    fn every_adjacent_transposition_is_refused() {
        for id in ["org_7k3m9q2xh43z", "per_4f9k2m7q8xvj"] {
            let start = 4;
            for position in start..id.len() - 1 {
                let bytes = id.as_bytes();
                if bytes[position] == bytes[position + 1] {
                    continue;
                }
                let mut swapped = bytes.to_vec();
                swapped.swap(position, position + 1);
                let swapped = String::from_utf8(swapped).unwrap();
                assert_eq!(
                    parse_agent_id(&swapped, KINDS),
                    Err(AgentIdRejection::BadCheck),
                    "transposition at {position} slipped through: {swapped}"
                );
            }
        }
    }

    #[test]
    fn minted_ids_survive_the_exhaustive_mutation_check() {
        for outcome in ["a", "registry|key:cik:0000320193", "registry|surface:acme"] {
            let (id, _) = mint_agent_id("org_", outcome, &|_| false).unwrap();
            let start = 4;
            for position in start..id.len() {
                for replacement in AGENT_ID_ALPHABET {
                    let replacement = *replacement as char;
                    if id.as_bytes()[position] as char == replacement {
                        continue;
                    }
                    let mut mutated = id.clone();
                    mutated.replace_range(position..=position, &replacement.to_string());
                    assert!(parse_agent_id(&mutated, KINDS).is_err());
                }
            }
            for position in start..id.len() - 1 {
                let bytes = id.as_bytes();
                if bytes[position] == bytes[position + 1] {
                    continue;
                }
                let mut swapped = bytes.to_vec();
                swapped.swap(position, position + 1);
                assert!(parse_agent_id(&String::from_utf8(swapped).unwrap(), KINDS).is_err());
            }
        }
    }

    #[test]
    fn upper_case_checks_as_lower_case() {
        let parsed = parse_agent_id("ORG_7K3M9Q2XH43Z", KINDS).unwrap();
        assert_eq!(parsed.id, "org_7k3m9q2xh43z");
        assert_eq!(parsed.prefix, "org_");
    }

    #[test]
    fn refusal_reasons_follow_the_documented_order() {
        assert_eq!(
            parse_agent_id("org_7k3m9q2xh43", KINDS),
            Err(AgentIdRejection::BadLength)
        );
        assert_eq!(
            parse_agent_id("org_7k3m9q2xh43zz", KINDS),
            Err(AgentIdRejection::BadLength)
        );
        assert_eq!(parse_agent_id("", KINDS), Err(AgentIdRejection::BadLength));
        assert_eq!(
            parse_agent_id("org_7k3m9q2xh4iz", KINDS),
            Err(AgentIdRejection::BadSymbol)
        );
        assert_eq!(
            parse_agent_id("org_7k3m9q2xh4uz", KINDS),
            Err(AgentIdRejection::BadSymbol)
        );
        assert_eq!(
            parse_agent_id("fund_7k3m9q2xh43z", KINDS),
            Err(AgentIdRejection::UnknownKind)
        );
        assert_eq!(
            parse_agent_id("7k3m9q2xh43zabcd", KINDS),
            Err(AgentIdRejection::UnknownKind)
        );
        assert_eq!(
            parse_agent_id("org_7k3m9q2xh43y", KINDS),
            Err(AgentIdRejection::BadCheck)
        );
    }

    #[test]
    fn label_is_advisory_and_split_off() {
        let parsed = parse_agent_id("org_7k3m9q2xh43z~ares-capital", KINDS).unwrap();
        assert_eq!(parsed.id, "org_7k3m9q2xh43z");
        assert_eq!(parsed.label.as_deref(), Some("ares-capital"));
        assert!(parse_agent_id("org_7k3m9q2xh43y~ares-capital", KINDS).is_err());
    }

    #[test]
    fn label_comparison_slugs_both_sides() {
        assert_eq!(label_slug("Ares Capital, Corp."), "ares-capital-corp");
        assert!(label_matches(
            "ares-capital-corp",
            ["ARES CAPITAL CORP", "Other"]
        ));
        assert!(!label_matches("blue-owl", ["ARES CAPITAL CORP"]));
        assert!(!label_matches("", ["ARES CAPITAL CORP"]));
    }

    #[test]
    fn mint_is_deterministic_and_kind_separated() {
        let (first, n) = mint_agent_id("org_", "founding-outcome", &|_| false).unwrap();
        let (second, _) = mint_agent_id("org_", "founding-outcome", &|_| false).unwrap();
        let (person, _) = mint_agent_id("per_", "founding-outcome", &|_| false).unwrap();
        assert_eq!(n, 0);
        assert_eq!(first, second);
        assert_ne!(&first[4..], &person[4..]);
        assert!(first.starts_with("org_") && person.starts_with("per_"));
        assert!(parse_agent_id(&first, KINDS).is_ok());
        assert!(parse_agent_id(&person, KINDS).is_ok());
    }

    #[test]
    fn forced_collision_moves_to_the_next_counter_and_both_ids_check() {
        let (first, _) = mint_agent_id("org_", "founding-outcome", &|_| false).unwrap();
        let (second, n) =
            mint_agent_id("org_", "founding-outcome", &|candidate| candidate == first).unwrap();
        assert_eq!(n, 1);
        assert_ne!(first, second);
        assert!(parse_agent_id(&first, KINDS).is_ok());
        assert!(parse_agent_id(&second, KINDS).is_ok());
    }

    #[test]
    fn mint_rejects_a_malformed_kind_prefix() {
        for prefix in ["org", "Org_", "org__", "_", "o1g_", ""] {
            assert_eq!(
                mint_agent_id(prefix, "x", &|_| false),
                Err(AgentIdRejection::UnknownKind),
                "{prefix:?}"
            );
        }
    }

    #[test]
    fn body_is_ten_alphabet_symbols() {
        let body = agent_id_body("org_", "x", 0);
        assert_eq!(body.len(), AGENT_ID_BODY_LEN);
        assert!(body.bytes().all(|symbol| symbol_value(symbol).is_some()));
    }
}
