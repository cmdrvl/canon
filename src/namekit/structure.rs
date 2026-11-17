//! Name structure: alternates (dba / fka / aka) and member lists in one surface
//! (bd-vdcw.24).
//!
//! Deterministic and table-driven. Nothing here guesses: no alias without a
//! marker in the text, no split on a bare comma, no split on "and" / "&" unless
//! every side independently ends in a legal form, and no choice of which list
//! member is the "real" entity.

use serde::Serialize;

use crate::namekit::legal_suffix::normalize_legal_forms;

/// The class of an alternate-name marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Dba,
    Fka,
    Aka,
    Formerly,
}

impl MarkerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dba => "dba",
            Self::Fka => "fka",
            Self::Aka => "aka",
            Self::Formerly => "formerly",
        }
    }
}

/// (marker text lowercased, class, allowed bare). A marker that is not bare is
/// recognised only inside parentheses or after a comma.
const MARKERS: &[(&str, MarkerKind, bool)] = &[
    ("doing business as", MarkerKind::Dba, true),
    ("formerly known as", MarkerKind::Fka, true),
    ("trading as", MarkerKind::Dba, true),
    ("d.b.a.", MarkerKind::Dba, true),
    ("d.b.a", MarkerKind::Dba, true),
    ("d/b/a", MarkerKind::Dba, true),
    ("dba", MarkerKind::Dba, true),
    ("f.k.a.", MarkerKind::Fka, true),
    ("f.k.a", MarkerKind::Fka, true),
    ("f/k/a", MarkerKind::Fka, true),
    ("fka", MarkerKind::Fka, true),
    ("a.k.a.", MarkerKind::Aka, true),
    ("a.k.a", MarkerKind::Aka, true),
    ("a/k/a", MarkerKind::Aka, true),
    ("aka", MarkerKind::Aka, true),
    ("formerly", MarkerKind::Formerly, false),
    ("t/a", MarkerKind::Dba, false),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Alternate {
    pub name: String,
    pub marker: MarkerKind,
    /// The marker as written in the source string.
    pub marker_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NameStructure {
    pub primary: String,
    pub alternates: Vec<Alternate>,
    /// The members of a co-borrower list; `None` for a single name.
    pub members: Option<Vec<String>>,
    pub steps: Vec<String>,
}

impl NameStructure {
    pub fn is_plain(&self) -> bool {
        self.alternates.is_empty() && self.members.is_none()
    }
}

struct Found {
    start: usize,
    end: usize,
    kind: MarkerKind,
    text: String,
    parenthesised: bool,
}

fn is_boundary_before(byte: Option<u8>) -> bool {
    byte.is_none_or(|b| b.is_ascii_whitespace() || b == b'(' || b == b',')
}

fn is_boundary_after(byte: Option<u8>) -> bool {
    byte.is_none_or(|b| b.is_ascii_whitespace() || b == b')' || b == b',')
}

fn find_marker(text: &str, allow_empty_primary: bool) -> Option<Found> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut best: Option<Found> = None;
    for &(marker, kind, bare) in MARKERS {
        let mut from = 0;
        while let Some(offset) = lower[from..].find(marker) {
            let start = from + offset;
            let end = start + marker.len();
            from = start + 1;
            let before = start.checked_sub(1).map(|index| bytes[index]);
            if !is_boundary_before(before) || !is_boundary_after(bytes.get(end).copied()) {
                continue;
            }
            // The text before the marker, ignoring spaces.
            let prefix = text[..start].trim_end();
            let parenthesised = prefix.ends_with('(');
            let comma = prefix.ends_with(',');
            if !(bare || parenthesised || comma) {
                continue;
            }
            // A marker needs something before it to be an alias of.
            if !allow_empty_primary
                && prefix
                    .trim_matches(|c: char| c == '(' || c == ',' || c.is_whitespace())
                    .is_empty()
            {
                continue;
            }
            let found = Found {
                start,
                end,
                kind,
                text: text[start..end].to_string(),
                parenthesised,
            };
            if best
                .as_ref()
                .is_none_or(|current| found.start < current.start)
            {
                best = Some(found);
            }
            break;
        }
    }
    best
}

fn trim_name(text: &str) -> String {
    text.trim_matches(|c: char| c == '(' || c == ')' || c == ',' || c == ';' || c.is_whitespace())
        .to_string()
}

/// The primary name and the alternates, recursively. `None` when no marker.
fn parse_alternates(
    text: &str,
    allow_empty_primary: bool,
    steps: &mut Vec<String>,
) -> Option<(String, Vec<Alternate>)> {
    let found = find_marker(text, allow_empty_primary)?;
    let mut primary = trim_name(&text[..found.start]);
    let rest = &text[found.end..];
    let mut alternates = Vec::new();
    steps.push(format!("name_structure:marker:{}", found.kind.as_str()));
    let (inside, tail) = if found.parenthesised {
        steps.push("name_structure:parenthesised".to_string());
        let mut depth = 1_i32;
        let mut close = rest.len();
        for (index, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        (&rest[..close], rest.get(close + 1..).unwrap_or(""))
    } else {
        (rest, "")
    };
    match parse_alternates(inside, false, steps) {
        Some((name, nested)) => {
            alternates.push(Alternate {
                name,
                marker: found.kind,
                marker_text: found.text.clone(),
            });
            alternates.extend(nested);
        }
        None => alternates.push(Alternate {
            name: trim_name(inside),
            marker: found.kind,
            marker_text: found.text.clone(),
        }),
    }
    if let Some((tail_primary, tail_alternates)) = parse_alternates(tail, true, steps) {
        if !tail_primary.is_empty() {
            primary = format!("{primary} {tail_primary}").trim().to_string();
        }
        alternates.extend(tail_alternates);
    } else if !trim_name(tail).is_empty() {
        primary = format!("{primary} {}", trim_name(tail)).trim().to_string();
    }
    Some((primary, alternates))
}

fn ends_in_legal_form(part: &str) -> bool {
    !normalize_legal_forms(part).steps.is_empty()
}

/// Split on a conjunction (" and " / " & ", any case) at word boundaries.
fn split_conjunction(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut parts = Vec::new();
    let mut last = 0;
    let mut index = 0;
    while index < lower.len() {
        let rest = &lower[index..];
        let skip = if rest.starts_with(" and ") {
            5
        } else if rest.starts_with(" & ") {
            3
        } else {
            0
        };
        if skip > 0 {
            parts.push(text[last..index].to_string());
            index += skip;
            last = index;
        } else {
            index += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    parts.push(text[last..].to_string());
    parts
}

fn primary_is_list(primary: &str) -> bool {
    if split_explicit(primary).is_some() {
        return true;
    }
    let parts = split_conjunction(primary);
    parts.len() > 1 && parts.iter().all(|part| ends_in_legal_form(part.trim()))
}

fn split_explicit(text: &str) -> Option<Vec<String>> {
    for separator in [";", " / ", "\n"] {
        if text.contains(separator) {
            let parts = text
                .split(separator)
                .map(trim_name)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            if parts.len() >= 2 {
                return Some(parts);
            }
        }
    }
    None
}

/// Phrases that make the name on their left a unit of the name on their right.
/// A table, not a guess: a name without one of these is never read as a unit.
const UNIT_PATTERNS: &[&str] = &[
    "an operating division of",
    "a business unit of",
    "a division of",
    "a unit of",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartOf {
    pub unit: String,
    pub parent: String,
    pub pattern: &'static str,
}

/// "X, a division of Y": X is a unit of Y. Only the listed phrases; a surface
/// that also states a dba/fka alternate or a member list is not parsed.
pub fn parse_part_of(raw: &str) -> Option<PartOf> {
    let text = raw.trim();
    let lower = text.to_ascii_lowercase();
    for pattern in UNIT_PATTERNS {
        let mut from = 0;
        while let Some(found) = lower[from..].find(pattern) {
            let at = from + found;
            let end = at + pattern.len();
            from = end;
            let before = text[..at].trim_end();
            let separated = at == 0 || text[..at].ends_with([' ', ',']);
            let followed = text[end..].starts_with(' ');
            let unit = trim_name(before.trim_end_matches(',').trim());
            let parent = trim_name(&text[end..]);
            if !separated || !followed || unit.is_empty() || parent.is_empty() {
                continue;
            }
            let parsed = parse_name_structure(text);
            if parsed.members.is_some() || !parsed.alternates.is_empty() {
                return None;
            }
            return Some(PartOf {
                unit,
                parent,
                pattern,
            });
        }
    }
    None
}

/// A segment that is nothing but legal-form words ("Inc.", "L.P."): never a
/// party of its own.
fn is_legal_form_only(part: &str) -> bool {
    let normalized = normalize_legal_forms(part);
    if normalized.steps.is_empty() {
        return false;
    }
    let form_words = normalized
        .steps
        .iter()
        .filter_map(|step| step.strip_prefix("legal_form:"))
        .map(|form| form.split_whitespace().count())
        .sum::<usize>();
    normalized.normalized.split_whitespace().count() <= form_words
}

/// Merge every member that is only a legal-form word into the member before it.
fn rejoin_legal_form_only(members: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for member in members {
        match out.last_mut() {
            Some(previous) if is_legal_form_only(&member) => {
                previous.push_str(", ");
                previous.push_str(&member);
            }
            _ => out.push(member),
        }
    }
    out
}

/// An Oxford-style comma list inside one part: split on ", " only when, after
/// a legal-form-only segment is rejoined to its name, every group ends in a
/// legal form on its own.
fn split_comma_members(part: &str) -> Vec<String> {
    let segments = part
        .split(", ")
        .map(trim_name)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let groups = rejoin_legal_form_only(segments);
    if groups.len() > 1 && groups.iter().all(|group| ends_in_legal_form(group)) {
        groups
    } else {
        vec![trim_name(part)]
    }
}

/// Parse one surface into its primary name, alternates and members.
pub fn parse_name_structure(raw: &str) -> NameStructure {
    let text = raw.trim();
    let mut steps = Vec::new();
    let plain = |steps: Vec<String>| NameStructure {
        primary: text.to_string(),
        alternates: Vec::new(),
        members: None,
        steps,
    };

    let alternates = parse_alternates(text, false, &mut steps);
    let list = split_explicit(text);
    let conjunction = {
        let parts = split_conjunction(text);
        if parts.len() < 2 {
            None
        } else if parts.iter().all(|part| ends_in_legal_form(part.trim())) {
            Some(parts)
        } else {
            steps.push("no_split:ambiguous_conjunction".to_string());
            None
        }
    };
    let members = list.or(conjunction);

    match (alternates, members) {
        (Some(_), Some(_)) => {
            // Both an alias marker and a list: which name the marker belongs
            // to is a guess, so nothing is parsed.
            steps.retain(|step| step.starts_with("no_split:"));
            steps.push("name_structure:marker_inside_list".to_string());
            plain(steps)
        }
        (Some((primary, _)), None) if primary_is_list(&primary) => {
            steps.retain(|step| step.starts_with("no_split:"));
            steps.push("name_structure:marker_inside_list".to_string());
            plain(steps)
        }
        (Some((primary, alternates)), None) if !primary.is_empty() => {
            if alternates.iter().any(|alternate| alternate.name.is_empty()) {
                steps.retain(|step| step.starts_with("no_split:"));
                steps.push("name_structure:empty_alternate".to_string());
                return plain(steps);
            }
            NameStructure {
                primary,
                alternates,
                members: None,
                steps,
            }
        }
        (None, Some(parts)) => {
            // An Oxford-style comma list: only when every comma segment ends in a
            // legal form on its own.
            let mut members = Vec::new();
            for part in parts {
                members.extend(split_comma_members(&part));
            }
            let members = rejoin_legal_form_only(members);
            steps.push("name_structure:list".to_string());
            NameStructure {
                primary: text.to_string(),
                alternates: Vec::new(),
                members: Some(members),
                steps,
            }
        }
        _ => {
            steps.retain(|step| step.starts_with("no_split:"));
            plain(steps)
        }
    }
}

/// The key two names are compared by: normalised, legal forms canonical.
pub fn structure_key(name: &str) -> (String, Vec<String>) {
    let normalized = normalize_legal_forms(name);
    (normalized.normalized, normalized.steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alt(text: &str) -> (String, Vec<(String, MarkerKind)>) {
        let parsed = parse_name_structure(text);
        (
            parsed.primary,
            parsed
                .alternates
                .into_iter()
                .map(|a| (a.name, a.marker))
                .collect(),
        )
    }

    #[test]
    fn every_marker_parses_parenthesised_comma_introduced_and_bare() {
        for (marker, kind) in [
            ("dba", MarkerKind::Dba),
            ("d/b/a", MarkerKind::Dba),
            ("d.b.a.", MarkerKind::Dba),
            ("doing business as", MarkerKind::Dba),
            ("trading as", MarkerKind::Dba),
            ("fka", MarkerKind::Fka),
            ("f/k/a", MarkerKind::Fka),
            ("f.k.a.", MarkerKind::Fka),
            ("formerly known as", MarkerKind::Fka),
            ("aka", MarkerKind::Aka),
            ("a/k/a", MarkerKind::Aka),
        ] {
            let want = (
                "Quill Holdings LP".to_string(),
                vec![("Harbor".to_string(), kind)],
            );
            assert_eq!(
                alt(&format!("Quill Holdings LP ({marker} Harbor)")),
                want,
                "{marker}"
            );
            assert_eq!(
                alt(&format!("Quill Holdings LP, {marker} Harbor")),
                want,
                "{marker}"
            );
            assert_eq!(
                alt(&format!("Quill Holdings LP {marker} Harbor")),
                want,
                "{marker}"
            );
            assert_eq!(
                alt(&format!(
                    "Quill Holdings LP {} Harbor",
                    marker.to_uppercase()
                )),
                want
            );
        }
        // "formerly" and "t/a" are loose words: parenthesised or comma only.
        assert_eq!(
            alt("Quill Holdings LP (formerly Harbor)"),
            (
                "Quill Holdings LP".to_string(),
                vec![("Harbor".to_string(), MarkerKind::Formerly)]
            )
        );
        assert_eq!(
            alt("Quill Holdings LP, t/a Harbor"),
            (
                "Quill Holdings LP".to_string(),
                vec![("Harbor".to_string(), MarkerKind::Dba)]
            )
        );
        assert!(parse_name_structure("Quill formerly Harbor LP").is_plain());
    }

    #[test]
    fn nested_and_repeated_markers_give_every_alternate_in_order() {
        let nested = parse_name_structure("Quill Co (dba Harbor (fka Dock))");
        assert_eq!(nested.primary, "Quill Co");
        let names = nested
            .alternates
            .iter()
            .map(|a| (a.name.as_str(), a.marker))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![("Harbor", MarkerKind::Dba), ("Dock", MarkerKind::Fka)]
        );
        let repeated = parse_name_structure("Quill Co (dba Harbor) (aka Dock)");
        assert_eq!(repeated.alternates.len(), 2);
        assert_eq!(repeated.primary, "Quill Co");
    }

    #[test]
    fn a_marker_inside_a_word_or_without_a_primary_is_not_a_marker() {
        assert!(parse_name_structure("Madbaker Foods LLC").is_plain());
        assert!(parse_name_structure("Akamai Group").is_plain());
        assert!(parse_name_structure("dba Harbor").is_plain());
        assert!(parse_name_structure("Quill Co (dba )").is_plain());
    }

    #[test]
    fn explicit_separators_and_legal_form_conjunctions_split_into_members() {
        let list = |text: &str| parse_name_structure(text).members;
        let want = Some(vec![
            "Alpha Holdings LLC".to_string(),
            "Beta Capital LP".to_string(),
        ]);
        assert_eq!(list("Alpha Holdings LLC and Beta Capital LP"), want);
        assert_eq!(list("Alpha Holdings LLC & Beta Capital LP"), want);
        assert_eq!(list("Alpha Holdings LLC; Beta Capital LP"), want);
        assert_eq!(list("Alpha Holdings LLC / Beta Capital LP"), want);
        assert_eq!(list("Alpha Holdings LLC\nBeta Capital LP"), want);
        assert_eq!(
            list("Alpha Holdings LLC, Beta Capital LP and Gamma Inc"),
            Some(vec![
                "Alpha Holdings LLC".to_string(),
                "Beta Capital LP".to_string(),
                "Gamma Inc".to_string()
            ])
        );
        // Explicit separators need no legal form.
        assert_eq!(list("Ann Reed; Bo Hart").map(|m| m.len()), Some(2));
    }

    #[test]
    fn ambiguous_conjunctions_and_bare_commas_are_not_split() {
        for text in [
            "Johnson and Johnson",
            "Procter & Gamble",
            "Smith and Sons LLC",
            "A&B Holdings",
            "Smith, Jones and Partners LLP",
            "Medline Borrower, LP",
            "Alpha Holdings LLC and Beta",
        ] {
            let parsed = parse_name_structure(text);
            assert!(parsed.is_plain(), "{text}");
            assert_eq!(parsed.primary, text);
        }
        assert!(
            parse_name_structure("Johnson and Johnson")
                .steps
                .contains(&"no_split:ambiguous_conjunction".to_string())
        );
    }

    #[test]
    fn a_marker_together_with_a_list_is_left_unparsed() {
        let parsed = parse_name_structure("Alpha Holdings LLC and Beta Capital LP (dba Quill)");
        assert!(parsed.is_plain());
        assert!(
            parsed
                .steps
                .contains(&"name_structure:marker_inside_list".to_string())
        );
    }

    #[test]
    fn a_legal_form_word_is_never_a_member_of_its_own() {
        let list = |text: &str| parse_name_structure(text).members;
        assert_eq!(
            list("Alder Works Company, Inc., Birch Fund, L.P. and Cedar Trading LLC"),
            Some(vec![
                "Alder Works Company, Inc.".to_string(),
                "Birch Fund, L.P.".to_string(),
                "Cedar Trading LLC".to_string()
            ])
        );
        assert_eq!(
            list("Alder Build, LLC and Birch Stone Company, LLC"),
            Some(vec![
                "Alder Build, LLC".to_string(),
                "Birch Stone Company, LLC".to_string()
            ])
        );
        assert_eq!(
            list("Alder Build; LLC"),
            Some(vec!["Alder Build, LLC".to_string()])
        );
        for members in [
            list("Alder Works, Inc., Birch Fund, L.P., Cedar Co., Ltd; Dune Corp"),
            list("Alder Build, LLC / Birch Stone, Inc."),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                members.iter().all(|m| !is_legal_form_only(m)),
                "{members:?}"
            );
        }
    }

    #[test]
    fn a_listed_phrase_makes_a_unit_and_nothing_else_does() {
        let part = parse_part_of("Alder Servicing, a division of Alder Trust Bank, N.A.").unwrap();
        assert_eq!(part.unit, "Alder Servicing");
        assert_eq!(part.parent, "Alder Trust Bank, N.A.");
        assert_eq!(part.pattern, "a division of");
        assert!(parse_part_of("Alder Servicing a business unit of Alder Holdings LLC").is_some());
        for text in [
            "Alder Trust Bank, N.A.",
            "Alder Division of Trust",
            "Alder Servicing, a divisionof Alder Bank",
            ", a division of Alder Bank",
            "Alder Servicing, a division of",
            "Alder Servicing, a division of Alder LLC and Birch Corp",
            "Alder Servicing, a division of Alder Bank (dba Alder Home Loans)",
            "Black Rock Coffee Bar",
        ] {
            assert!(parse_part_of(text).is_none(), "{text}");
        }
    }
}
