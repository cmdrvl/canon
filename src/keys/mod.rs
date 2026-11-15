//! Key namespaces: format and check-digit validation for identifier keys.
//!
//! Keys (CIK, LEI, CUSIP, ISIN, ticker) are checked before they are trusted: a
//! mistyped or placeholder key must neither merge two entities nor veto a true
//! match. This module is domain-neutral and pure: no network lookup of whether
//! a key exists, no fuzzy repair (a failed check digit is never "corrected"),
//! and the CUSIP/ISIN arithmetic lives only here.

/// Values that mean "no key" in source tables. Compared ASCII
/// case-insensitively after trimming; treated as null, never as a value.
pub const PLACEHOLDERS: &[&str] = &["N/A", "NA", "NONE", "NULL", "UNKNOWN", "TBD", "-", "0"];

/// Why a key value was refused. `reason()` is the stable machine string used in
/// row warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRejection {
    /// Empty after trimming.
    Empty,
    /// A declared placeholder (or an all-zero CIK): null, not a value.
    Placeholder,
    /// Wrong length, alphabet or shape for the namespace.
    Format,
    /// Well-formed but the check digit does not verify.
    CheckDigit,
}

impl KeyRejection {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Placeholder => "placeholder",
            Self::Format => "format",
            Self::CheckDigit => "check_digit",
        }
    }
}

/// A key namespace with a validation rule. Unknown names are `Opaque`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyNamespace {
    Cik,
    Lei,
    Cusip,
    Isin,
    Ticker,
    /// Trimmed string, no format check (`format: opaque`).
    Opaque,
}

impl KeyNamespace {
    /// Map a namespace name (rule-table spelling, ASCII case-insensitive).
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "cik" => Self::Cik,
            "lei" => Self::Lei,
            "cusip" => Self::Cusip,
            "isin" => Self::Isin,
            "ticker" => Self::Ticker,
            _ => Self::Opaque,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Cik => "cik",
            Self::Lei => "lei",
            Self::Cusip => "cusip",
            Self::Isin => "isin",
            Self::Ticker => "ticker",
            Self::Opaque => "opaque",
        }
    }

    /// False for `Opaque`: status warns when such a namespace is declared
    /// authoritative.
    pub fn is_validated(self) -> bool {
        !matches!(self, Self::Opaque)
    }

    /// Whether the namespace is authoritative when the rule table says nothing.
    pub fn default_authoritative(self) -> bool {
        matches!(self, Self::Cik | Self::Lei | Self::Cusip | Self::Isin)
    }

    /// The shape a value must have; used as the `fix` text of a rejected key.
    pub fn expected_form(self) -> &'static str {
        match self {
            Self::Cik => "1-10 digits, not all zero",
            Self::Lei => "20 upper-case alphanumerics with a valid ISO 17442 check",
            Self::Cusip => "9 upper-case alphanumerics with a valid check digit",
            Self::Isin => "12 characters: 2 letters, 9 alphanumerics, Luhn check digit",
            Self::Ticker => "1-10 of A-Z 0-9 . -",
            Self::Opaque => "any non-empty string",
        }
    }

    /// Canonical form of `value` or the reason it is refused.
    pub fn canonicalise(self, value: &str) -> Result<String, KeyRejection> {
        let value = value.trim();
        if value.is_empty() {
            return Err(KeyRejection::Empty);
        }
        if is_placeholder(value) {
            return Err(KeyRejection::Placeholder);
        }
        match self {
            Self::Opaque => Ok(value.to_string()),
            Self::Cik => canonical_cik(value),
            Self::Lei => {
                if !is_upper_alnum(value, 20) {
                    return Err(KeyRejection::Format);
                }
                if lei_is_valid(value) {
                    Ok(value.to_string())
                } else {
                    Err(KeyRejection::CheckDigit)
                }
            }
            Self::Cusip => {
                let bytes = value.as_bytes();
                if bytes.len() != 9
                    || !bytes[..8].iter().all(|b| cusip_char_value(*b).is_some())
                    || !bytes[8].is_ascii_digit()
                {
                    return Err(KeyRejection::Format);
                }
                if cusip_is_valid(value) {
                    Ok(value.to_string())
                } else {
                    Err(KeyRejection::CheckDigit)
                }
            }
            Self::Isin => {
                let bytes = value.as_bytes();
                if bytes.len() != 12
                    || !bytes[..2].iter().all(u8::is_ascii_uppercase)
                    || !bytes[..11]
                        .iter()
                        .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
                    || !bytes[11].is_ascii_digit()
                {
                    return Err(KeyRejection::Format);
                }
                if isin_is_valid(value) {
                    Ok(value.to_string())
                } else {
                    Err(KeyRejection::CheckDigit)
                }
            }
            Self::Ticker => {
                let upper = value.to_ascii_uppercase();
                let ok = (1..=10).contains(&upper.len())
                    && upper.bytes().all(|b| {
                        b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'.' || b == b'-'
                    });
                if ok {
                    Ok(upper)
                } else {
                    Err(KeyRejection::Format)
                }
            }
        }
    }
}

/// True when the trimmed value is a declared placeholder.
pub fn is_placeholder(value: &str) -> bool {
    let value = value.trim();
    PLACEHOLDERS
        .iter()
        .any(|placeholder| placeholder.eq_ignore_ascii_case(value))
}

fn canonical_cik(value: &str) -> Result<String, KeyRejection> {
    if value.len() > 10 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(KeyRejection::Format);
    }
    let stripped = value.trim_start_matches('0');
    if stripped.is_empty() {
        return Err(KeyRejection::Placeholder);
    }
    Ok(stripped.to_string())
}

fn is_upper_alnum(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
}

/// ISO 17442 / ISO 7064 MOD 97-10: letters expand to 10..35; the whole number
/// mod 97 must equal 1. Expects 20 upper-case alphanumerics.
pub fn lei_is_valid(value: &str) -> bool {
    if !is_upper_alnum(value, 20) {
        return false;
    }
    let mut remainder = 0u32;
    for byte in value.bytes() {
        if byte.is_ascii_digit() {
            remainder = (remainder * 10 + u32::from(byte - b'0')) % 97;
        } else {
            let expanded = u32::from(byte - b'A') + 10;
            remainder = (remainder * 100 + expanded) % 97;
        }
    }
    remainder == 1
}

/// CUSIP: 9 bytes, 8 body characters plus a numeric check digit. Value must
/// already be trimmed.
pub fn cusip_is_valid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 9 {
        return false;
    }
    let check_digit = bytes[8];
    if !check_digit.is_ascii_digit() {
        return false;
    }
    match cusip_check_digit(&bytes[..8]) {
        Some(expected) => check_digit - b'0' == expected,
        None => false,
    }
}

/// Expected check digit for an 8-byte CUSIP body.
pub fn cusip_check_digit(body: &[u8]) -> Option<u8> {
    if body.len() != 8 {
        return None;
    }
    let mut sum = 0u32;
    for (index, byte) in body.iter().enumerate() {
        let mut value = cusip_char_value(*byte)?;
        if index % 2 == 1 {
            value *= 2;
        }
        sum += value / 10 + value % 10;
    }
    u8::try_from((10 - (sum % 10)) % 10).ok()
}

fn cusip_char_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'A'..=b'Z' => Some(u32::from(byte - b'A') + 10),
        b'*' => Some(36),
        b'@' => Some(37),
        b'#' => Some(38),
        _ => None,
    }
}

/// ISIN Luhn check over the digit expansion (letters -> 10..35). Callers that
/// need the 12-character shape check the length themselves.
pub fn isin_is_valid(value: &str) -> bool {
    let mut digits = Vec::new();
    for byte in value.bytes() {
        match byte {
            b'0'..=b'9' => digits.push(byte - b'0'),
            b'A'..=b'Z' => {
                let expanded = byte - b'A' + 10;
                digits.push(expanded / 10);
                digits.push(expanded % 10);
            }
            _ => return false,
        }
    }
    luhn_digits_valid(&digits)
}

fn luhn_digits_valid(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    for (index, digit) in digits.iter().rev().enumerate() {
        let mut value = u32::from(*digit);
        if index % 2 == 1 {
            value *= 2;
        }
        sum += value / 10 + value % 10;
    }
    sum.is_multiple_of(10)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent LEI builder: compute the two check digits for an 18-char body.
    fn lei_with_check(body: &str) -> String {
        let mut remainder = 0u32;
        for ch in body.chars().chain("00".chars()) {
            let expanded = ch.to_digit(36).expect("alphanumeric");
            remainder = if expanded >= 10 {
                (remainder * 100 + expanded) % 97
            } else {
                (remainder * 10 + expanded) % 97
            };
        }
        format!("{body}{:02}", 98 - remainder)
    }

    #[test]
    fn cik_padded_and_unpadded_are_one_key() {
        let cik = KeyNamespace::Cik;
        assert_eq!(cik.canonicalise("0001287750").as_deref(), Ok("1287750"));
        assert_eq!(cik.canonicalise("1287750").as_deref(), Ok("1287750"));
        assert_eq!(cik.canonicalise(" 1287750 ").as_deref(), Ok("1287750"));
    }

    #[test]
    fn cik_placeholders_and_bad_shapes_are_rejected() {
        let cik = KeyNamespace::Cik;
        assert_eq!(cik.canonicalise("0"), Err(KeyRejection::Placeholder));
        assert_eq!(
            cik.canonicalise("0000000000"),
            Err(KeyRejection::Placeholder)
        );
        assert_eq!(cik.canonicalise("N/A"), Err(KeyRejection::Placeholder));
        assert_eq!(cik.canonicalise("  "), Err(KeyRejection::Empty));
        assert_eq!(cik.canonicalise("12345678901"), Err(KeyRejection::Format));
        assert_eq!(cik.canonicalise("12A4"), Err(KeyRejection::Format));
    }

    #[test]
    fn placeholder_list_applies_to_every_namespace_case_insensitively() {
        for name in ["cik", "lei", "cusip", "isin", "ticker", "whatever"] {
            let ns = KeyNamespace::from_name(name);
            for placeholder in [
                "N/A", "n/a", "NA", "None", "NULL", "unknown", "TBD", "-", "0",
            ] {
                assert_eq!(
                    ns.canonicalise(placeholder),
                    Err(KeyRejection::Placeholder),
                    "{name} {placeholder}"
                );
            }
        }
    }

    #[test]
    fn lei_published_example_validates() {
        assert_eq!(
            KeyNamespace::Lei
                .canonicalise("5493001KJTIIGC8Y1R12")
                .as_deref(),
            Ok("5493001KJTIIGC8Y1R12")
        );
    }

    #[test]
    fn lei_computed_check_digits_validate_and_transposition_fails() {
        let lei = lei_with_check("ABCD1234EFGH5678IJ");
        assert_eq!(lei.len(), 20);
        assert_eq!(KeyNamespace::Lei.canonicalise(&lei), Ok(lei.clone()));

        // Transpose two adjacent digits of the published LEI: detected.
        let swapped = "5490301KJTIIGC8Y1R12";
        assert_eq!(
            KeyNamespace::Lei.canonicalise(swapped),
            Err(KeyRejection::CheckDigit)
        );
        // The invented fixture style is refused, not accepted.
        assert_eq!(
            KeyNamespace::Lei.canonicalise("LEI0000000000000001"),
            Err(KeyRejection::Format)
        );
        assert_eq!(
            KeyNamespace::Lei.canonicalise("lei00000000000000001"),
            Err(KeyRejection::Format)
        );
    }

    #[test]
    fn cusip_published_examples() {
        // Apple, Microsoft.
        assert!(cusip_is_valid("037833100"));
        assert!(cusip_is_valid("594918104"));
        assert!(!cusip_is_valid("037833101"));
        assert_eq!(cusip_check_digit(b"03783310"), Some(0));
        assert_eq!(
            KeyNamespace::Cusip.canonicalise("037833100").as_deref(),
            Ok("037833100")
        );
        assert_eq!(
            KeyNamespace::Cusip.canonicalise("037833101"),
            Err(KeyRejection::CheckDigit)
        );
        assert_eq!(
            KeyNamespace::Cusip.canonicalise("03783310"),
            Err(KeyRejection::Format)
        );
    }

    #[test]
    fn isin_published_examples() {
        // Apple, Treasury-style US ISIN.
        assert!(isin_is_valid("US0378331005"));
        assert!(!isin_is_valid("US0378331006"));
        assert_eq!(
            KeyNamespace::Isin.canonicalise("US0378331005").as_deref(),
            Ok("US0378331005")
        );
        assert_eq!(
            KeyNamespace::Isin.canonicalise("US0378331006"),
            Err(KeyRejection::CheckDigit)
        );
        assert_eq!(
            KeyNamespace::Isin.canonicalise("us0378331005"),
            Err(KeyRejection::Format)
        );
        assert_eq!(
            KeyNamespace::Isin.canonicalise("US03783310"),
            Err(KeyRejection::Format)
        );
    }

    #[test]
    fn ticker_is_uppercased_and_shape_checked() {
        let ticker = KeyNamespace::Ticker;
        assert_eq!(ticker.canonicalise("brk.b").as_deref(), Ok("BRK.B"));
        assert_eq!(
            ticker.canonicalise("ABCDEFGHIJK"),
            Err(KeyRejection::Format)
        );
        assert_eq!(ticker.canonicalise("AB CD"), Err(KeyRejection::Format));
        assert!(!KeyNamespace::Ticker.default_authoritative());
    }

    #[test]
    fn unknown_namespaces_are_opaque_trimmed_strings() {
        let ns = KeyNamespace::from_name("duns");
        assert_eq!(ns, KeyNamespace::Opaque);
        assert!(!ns.is_validated());
        assert_eq!(ns.canonicalise("  abc-123 ").as_deref(), Ok("abc-123"));
        assert_eq!(KeyNamespace::from_name(" CIK "), KeyNamespace::Cik);
    }

    #[test]
    fn rejection_reasons_are_stable_strings() {
        assert_eq!(KeyRejection::CheckDigit.reason(), "check_digit");
        assert_eq!(KeyRejection::Placeholder.reason(), "placeholder");
        assert_eq!(KeyRejection::Format.reason(), "format");
        assert_eq!(KeyRejection::Empty.reason(), "empty");
    }
}
