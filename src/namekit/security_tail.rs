//! Table-driven cleanup for source footnote marks and security descriptions.
//!
//! These transforms are deliberately narrower than general punctuation
//! normalization. A suffix is removed only when it is a recognized terminal
//! fragment after a declared separator; text in the issuer name remains intact.

use serde::Serialize;

pub const NAMEKIT_FOOTNOTE_MARKS_VERSION: &str = "canon_namekit_footnote_marks.v0";
pub const NAMEKIT_SECURITY_TAIL_VERSION: &str = "canon_namekit_security_tail.v0";

const FOOTNOTE_MARKS: &[char] = &['#', '^', '~', '*', '†', '‡', '§'];
const SUPERSCRIPT_DIGITS: &[char] = &['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

/// The result of one listed, lossy name transform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StripNormalization {
    pub raw: String,
    pub normalized: String,
    pub removed: Option<String>,
    pub step: Option<String>,
}

impl StripNormalization {
    fn unchanged(raw: &str) -> Self {
        Self {
            raw: raw.to_string(),
            normalized: raw.to_string(),
            removed: None,
            step: None,
        }
    }
}

/// Remove terminal source footnote marks, including numeric/lowercase letter
/// markers in parentheses and Unicode superscript digits.
pub fn strip_footnote_marks(input: &str) -> StripNormalization {
    let mut end = input.len();
    let mut changed = false;

    loop {
        let trimmed_end = input[..end].trim_end().len();
        if trimmed_end != end {
            end = trimmed_end;
        }

        let mut removed_mark = false;
        while let Some((start, character)) = input[..end].char_indices().next_back() {
            if FOOTNOTE_MARKS.contains(&character) || SUPERSCRIPT_DIGITS.contains(&character) {
                end = start;
                changed = true;
                removed_mark = true;
            } else {
                break;
            }
        }
        if removed_mark {
            continue;
        }

        if input[..end].ends_with(')') {
            let Some(open) = input[..end].rfind('(') else {
                break;
            };
            let content = &input[open + 1..end - 1];
            if is_parenthesized_footnote(content) {
                end = open;
                changed = true;
                continue;
            }
        }
        break;
    }

    if !changed {
        return StripNormalization::unchanged(input);
    }
    StripNormalization {
        raw: input.to_string(),
        normalized: input[..end].trim_end().to_string(),
        removed: Some(input[end..].trim().to_string()),
        step: Some("footnote_marks".to_string()),
    }
}

fn is_parenthesized_footnote(content: &str) -> bool {
    let trimmed = content.trim();
    !trimmed.is_empty()
        && trimmed.chars().all(|character| {
            character.is_ascii_digit()
                || character.is_ascii_lowercase()
                || character.is_ascii_whitespace()
                || matches!(character, ',' | ';')
        })
}

/// Remove one recognized terminal security/instrument description after a
/// separator. The table intentionally requires a complete instrument phrase:
/// `Class A` and `Series B` alone remain meaningful issuer text.
pub fn strip_security_tail(input: &str) -> StripNormalization {
    let mut candidates = Vec::new();
    for separator in [" - ", ", "] {
        if let Some(index) = input.rfind(separator) {
            let suffix_start = index + separator.len();
            if let Some(step) = security_tail_step(&input[suffix_start..]) {
                candidates.push((index, suffix_start, step));
            }
        }
    }
    if input.ends_with(')')
        && let Some(index) = input.rfind(" (")
    {
        let suffix_start = index + 2;
        if let Some(step) = security_tail_step(&input[suffix_start..input.len() - 1]) {
            candidates.push((index, suffix_start, step));
        }
    }

    let Some((index, suffix_start, step)) = candidates.into_iter().max_by_key(|item| item.0) else {
        return StripNormalization::unchanged(input);
    };
    StripNormalization {
        raw: input.to_string(),
        normalized: input[..index].trim_end().to_string(),
        removed: Some(
            input[suffix_start..]
                .trim_end_matches(')')
                .trim()
                .to_string(),
        ),
        step: Some(step),
    }
}

const FIXED_SECURITY_TAILS: &[(&str, &str)] = &[
    ("first lien term loan", "security_tail:first_lien_term_loan"),
    (
        "second lien term loan",
        "security_tail:second_lien_term_loan",
    ),
    ("lp interests", "security_tail:lp_interests"),
    ("delayed draw", "security_tail:delayed_draw"),
    ("term loan", "security_tail:term_loan"),
    ("first lien", "security_tail:first_lien"),
    ("second lien", "security_tail:second_lien"),
    ("common units", "security_tail:common_units"),
    ("common shares", "security_tail:common_shares"),
    ("preferred units", "security_tail:preferred_units"),
    ("preferred shares", "security_tail:preferred_shares"),
    ("warrants", "security_tail:warrants"),
    ("revolver", "security_tail:revolver"),
    ("preferred", "security_tail:preferred"),
    ("common", "security_tail:common"),
    ("units", "security_tail:units"),
    ("shares", "security_tail:shares"),
    ("notes", "security_tail:notes"),
];

fn security_tail_step(raw: &str) -> Option<String> {
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if let Some((_, step)) = FIXED_SECURITY_TAILS
        .iter()
        .find(|(tail, _)| *tail == normalized)
    {
        return Some((*step).to_string());
    }

    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    if tokens.len() >= 3
        && matches!(tokens[0], "class" | "series")
        && tokens[1].len() == 1
        && tokens[1]
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        let instrument = tokens[2..].join("_");
        if matches!(
            instrument.as_str(),
            "common"
                | "preferred"
                | "units"
                | "shares"
                | "common_units"
                | "common_shares"
                | "preferred_units"
                | "preferred_shares"
                | "warrants"
        ) {
            return Some(format!(
                "security_tail:{}_{}_{}",
                tokens[0], tokens[1], instrument
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{strip_footnote_marks, strip_security_tail};

    #[test]
    fn strips_table_driven_footnotes_without_touching_name_tokens() {
        for (raw, expected) in [
            ("ACP Avenu Midco LLC (14)", "ACP Avenu Midco LLC"),
            ("ACP Avenu Midco LLC (a)(b)", "ACP Avenu Midco LLC"),
            ("Acme Holdings LLC#^~", "Acme Holdings LLC"),
            ("Acme Holdings LLC¹²", "Acme Holdings LLC"),
        ] {
            let result = strip_footnote_marks(raw);
            assert_eq!(result.normalized, expected, "{raw}");
            assert_eq!(result.step.as_deref(), Some("footnote_marks"));
            assert!(result.removed.is_some(), "{raw}");
        }
        for raw in [
            "Acme (USA) Inc",
            "Acme (Holdings) Inc",
            "Acme (Delaware) Inc",
            "Acme (f/k/a Old Name) Inc",
            "Series 2021-1 Trust",
        ] {
            assert_eq!(strip_footnote_marks(raw).normalized, raw, "{raw}");
        }
    }

    #[test]
    fn strips_complete_security_tails_only_after_declared_separators() {
        let cases = [
            (
                "Acme Holdings, Inc. - Class A Common Units",
                "Acme Holdings, Inc.",
                "security_tail:class_a_common_units",
            ),
            (
                "Acme Holdings Inc - Series B Preferred",
                "Acme Holdings Inc",
                "security_tail:series_b_preferred",
            ),
            (
                "Acme Holdings, Inc., Warrants",
                "Acme Holdings, Inc.",
                "security_tail:warrants",
            ),
            (
                "Acme Holdings (Revolver)",
                "Acme Holdings",
                "security_tail:revolver",
            ),
            (
                "Acme Holdings - First Lien Term Loan",
                "Acme Holdings",
                "security_tail:first_lien_term_loan",
            ),
        ];
        for (raw, expected, step) in cases {
            let result = strip_security_tail(raw);
            assert_eq!(result.normalized, expected, "{raw}");
            assert_eq!(result.step.as_deref(), Some(step));
            assert!(result.removed.is_some(), "{raw}");
        }
    }

    #[test]
    fn leaves_meaningful_name_tails_and_unseparated_words_alone() {
        for raw in [
            "LD II Sub III, LLC",
            "Series 2021-1 Trust",
            "Warrants Capital Partners LLC",
            "Coca-Cola - Refreshments USA Inc",
            "Acme Fund III",
            "Acme - Class A",
            "Acme - Series B",
        ] {
            let result = strip_security_tail(raw);
            assert_eq!(result.normalized, raw, "{raw}");
            assert!(result.step.is_none(), "{raw}");
        }
    }
}
