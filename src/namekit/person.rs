//! Person and trust names (bd-vdcw.26).
//!
//! Deterministic and table-driven: no nickname tables, no phonetic matching, no
//! gender or ethnicity inference, and no merge on a family name plus an initial.
//! Everything here is a pure function of the surface text.

use serde::Serialize;

/// What a surface looks like, from table-driven cues only. `Unknown` stays
/// unknown: a name is never guessed into `Person`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameClass {
    Org,
    Person,
    Trust,
    Fragment,
    Unknown,
}

impl NameClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::Person => "person",
            Self::Trust => "trust",
            Self::Fragment => "fragment",
            Self::Unknown => "unknown",
        }
    }
}

/// Tokens that make a name an organisation.
const LEGAL_FORMS: &[&str] = &[
    "llc",
    "lp",
    "llp",
    "lllp",
    "lc",
    "pllc",
    "inc",
    "incorporated",
    "corp",
    "corporation",
    "co",
    "company",
    "ltd",
    "limited",
    "plc",
    "gmbh",
    "nv",
    "ag",
];

const HONORIFICS: &[&str] = &["mr", "mrs", "ms", "miss", "dr", "prof", "hon", "rev", "sir"];

/// Post-nominal credentials: not part of the name. Generational suffixes are
/// not among them; they tell people apart.
const CREDENTIALS: &[&str] = &[
    "md", "do", "dds", "dmd", "jd", "esq", "phd", "cpa", "cfa", "mba", "pe", "rn", "dvm",
];

/// Credentials that are also family names ("Nguyen Van Do"), so are dropped
/// only where a comma sets them apart.
const CREDENTIALS_AFTER_A_COMMA_ONLY: &[&str] = &["do"];

/// Drop the credentials that end `parts`: whole comma parts, then trailing
/// words of the last part. Two name words are always left. Returns whether
/// anything was dropped.
fn drop_credentials(parts: &mut Vec<Vec<String>>) -> bool {
    let name_words = |parts: &[Vec<String>]| parts.iter().map(Vec::len).sum::<usize>();
    let mut dropped = false;

    while let Some((last, before)) = parts.split_last()
        && !before.is_empty()
        && last.iter().all(|w| CREDENTIALS.contains(&w.as_str()))
        && name_words(before) >= 2
    {
        parts.pop();
        dropped = true;
    }
    while name_words(parts) > 2
        && let Some(last) = parts.last_mut()
        && last.len() > 1
        && last.last().is_some_and(|w| {
            CREDENTIALS.contains(&w.as_str())
                && !CREDENTIALS_AFTER_A_COMMA_ONLY.contains(&w.as_str())
        })
    {
        last.pop();
        dropped = true;
    }
    dropped
}

/// Family-name particles that stay with the family name.
const PARTICLES: &[&str] = &[
    "van", "von", "der", "den", "de", "da", "del", "di", "la", "le", "st",
];

/// Words that mark a trust.
const TRUST_WORDS: &[&str] = &["trust", "revocable", "irrevocable", "u/a/d", "u/a", "u/d/t"];

/// Words that end the holder's name inside a trust name.
const TRUST_CUTS: &[&str] = &["dated", "u/a/d", "u/a", "u/d/t", "under"];

/// Trailing words of a trust name that are not part of the holder.
const TRUST_TAIL: &[&str] = &[
    "trust",
    "revocable",
    "irrevocable",
    "living",
    "family",
    "declaration",
    "of",
    "the",
];

/// A token reduced to lowercase letters, digits, `/` and the inner `-` and `'`.
fn word(token: &str) -> String {
    token
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '/' | '-' | '\''))
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim_matches(['-', '\''])
        .to_string()
}

fn words(surface: &str) -> Vec<String> {
    surface
        .split_whitespace()
        .map(|token| word(&token.replace(['.', ','], "")))
        .filter(|word| !word.is_empty())
        .collect()
}

fn suffix_form(word: &str) -> Option<&'static str> {
    match word {
        "jr" | "junior" => Some("jr"),
        "sr" | "senior" => Some("sr"),
        "ii" | "2nd" => Some("ii"),
        "iii" | "3rd" => Some("iii"),
        "iv" | "4th" => Some("iv"),
        _ => None,
    }
}

/// Classify a surface. A single token that is a legal form or a generational
/// suffix is a fragment of upstream name splitting, whatever else is true.
pub fn classify_name(surface: &str) -> NameClass {
    let tokens = words(surface);
    match tokens.as_slice() {
        [] => return NameClass::Unknown,
        [only] if LEGAL_FORMS.contains(&only.as_str()) || suffix_form(only).is_some() => {
            return NameClass::Fragment;
        }
        _ => {}
    }
    if tokens
        .iter()
        .any(|token| LEGAL_FORMS.contains(&token.as_str()))
    {
        return NameClass::Org;
    }
    let starts_estate = {
        let rest = tokens.strip_prefix(&["the".to_string()]).unwrap_or(&tokens);
        matches!(rest, [first, second, ..] if first == "estate" && second == "of")
    };
    if starts_estate
        || tokens
            .iter()
            .any(|token| TRUST_WORDS.contains(&token.as_str()))
    {
        return NameClass::Trust;
    }
    // "dated" alone is a trust cue only after another word ("Smith Family dated 1/1/01").
    if tokens.len() > 1 && tokens.iter().skip(1).any(|token| token == "dated") {
        return NameClass::Trust;
    }
    NameClass::Unknown
}

/// The components of a person name, lowercase, with the steps that produced
/// them. `suffix` is its own protected component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonName {
    pub given: String,
    pub middle: String,
    pub family: String,
    pub suffix: String,
    pub steps: Vec<String>,
}

impl PersonName {
    /// The exact-comparable form: every component, suffix included.
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.family, self.given, self.middle, self.suffix
        )
    }
}

/// Parse "Last, First Middle Suffix", "First Middle Last Suffix" and
/// "First Middle Last, Suffix". `None` when the text has fewer than two name
/// words or a shape this parser does not take (no guessing).
pub fn parse_person_name(surface: &str) -> Option<PersonName> {
    let mut steps = vec!["person:case_fold".to_string()];
    if surface.contains('.') {
        steps.push("person:strip_periods".to_string());
    }
    let mut parts = surface
        .split(',')
        .map(words)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if drop_credentials(&mut parts) {
        steps.push("person:drop_credential".to_string());
    }
    let is_suffix_part = |part: &[String]| part.iter().all(|w| suffix_form(w).is_some());
    let (mut family, mut rest, mut suffix_words): (Vec<String>, Vec<String>, Vec<String>) =
        match parts.as_slice() {
            [single] => {
                let (name, suffix) = split_trailing_suffix(single);
                (Vec::new(), name, suffix)
            }
            [first, second] if is_suffix_part(second) => {
                let (name, mut suffix) = split_trailing_suffix(first);
                suffix.extend(second.iter().cloned());
                (Vec::new(), name, suffix)
            }
            [first, second] => {
                steps.push("person:last_first_reordered".to_string());
                let (name, suffix) = split_trailing_suffix(second);
                (first.clone(), name, suffix)
            }
            [first, second, third] if is_suffix_part(third) => {
                steps.push("person:last_first_reordered".to_string());
                (first.clone(), second.clone(), third.clone())
            }
            _ => return None,
        };
    if suffix_words.len() > 1 {
        return None;
    }
    // Honorifics are dropped from the front.
    let before = rest.len();
    while rest
        .first()
        .is_some_and(|w| HONORIFICS.contains(&w.as_str()))
        && rest.len() > 1
    {
        rest.remove(0);
    }
    if rest.len() != before {
        steps.push("person:drop_honorific".to_string());
    }
    if family.is_empty() {
        // First Middle Last: the family name is the last word plus its particles.
        if rest.len() < 2 {
            return None;
        }
        let mut cut = rest.len() - 1;
        while cut > 1 && PARTICLES.contains(&rest[cut - 1].as_str()) {
            cut -= 1;
        }
        family = rest.split_off(cut);
    }
    if rest.is_empty() || family.is_empty() {
        return None;
    }
    let given = rest.remove(0);
    let suffix = suffix_words
        .pop()
        .and_then(|w| suffix_form(&w))
        .unwrap_or_default()
        .to_string();
    if !suffix.is_empty() {
        steps.push("person:suffix_split".to_string());
    }
    Some(PersonName {
        given,
        middle: rest.join(" "),
        family: family.join(" "),
        suffix,
        steps,
    })
}

fn split_trailing_suffix(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    match tokens.split_last() {
        Some((last, rest)) if suffix_form(last).is_some() && !rest.is_empty() => {
            (rest.to_vec(), vec![last.clone()])
        }
        _ => (tokens.to_vec(), Vec::new()),
    }
}

/// How two parsed people relate. Equivalence is exact after the listed steps;
/// similarity is for review only; a conflicting suffix is an authoritative
/// incompatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersonRelation {
    Equivalent {
        steps: Vec<String>,
    },
    /// One middle is an initial of the other's middle name ("A" and "Andrew").
    MiddleInitialVsName,
    /// Both suffixes are present and differ.
    SuffixConflict {
        left: String,
        right: String,
    },
    Unrelated,
}

pub fn compare_persons(left: &PersonName, right: &PersonName) -> PersonRelation {
    if left.family != right.family || left.given != right.given {
        return PersonRelation::Unrelated;
    }
    match (left.suffix.is_empty(), right.suffix.is_empty()) {
        (false, false) if left.suffix != right.suffix => {
            return PersonRelation::SuffixConflict {
                left: left.suffix.clone(),
                right: right.suffix.clone(),
            };
        }
        (true, true) => {}
        (false, false) => {}
        // One suffix absent: nothing is claimed either way.
        _ => return PersonRelation::Unrelated,
    }
    let mut steps = left.steps.clone();
    steps.extend(right.steps.iter().cloned());
    steps.sort();
    steps.dedup();
    if left.middle == right.middle {
        return PersonRelation::Equivalent { steps };
    }
    if left.middle.is_empty() || right.middle.is_empty() {
        steps.push("person:middle_absent".to_string());
        steps.sort();
        return PersonRelation::Equivalent { steps };
    }
    let (short, long) = if left.middle.chars().count() <= right.middle.chars().count() {
        (&left.middle, &right.middle)
    } else {
        (&right.middle, &left.middle)
    };
    if short.chars().count() == 1 && long.starts_with(short.as_str()) {
        return PersonRelation::MiddleInitialVsName;
    }
    PersonRelation::Unrelated
}

/// The holder named inside a trust or estate name, when it is more than one word:
/// "The John A. Smith Revocable Trust dated 1/1/2001" gives "John A. Smith".
pub fn trust_holder(surface: &str) -> Option<String> {
    let tokens = surface.split_whitespace().collect::<Vec<_>>();
    let lowered = tokens
        .iter()
        .map(|t| word(&t.replace(['.', ','], "")))
        .collect::<Vec<_>>();
    let mut start = 0;
    if lowered.first().is_some_and(|w| w == "the") {
        start = 1;
    }
    if lowered.get(start).is_some_and(|w| w == "estate")
        && lowered.get(start + 1).is_some_and(|w| w == "of")
    {
        start += 2;
    }
    let mut end = (start..tokens.len())
        .find(|&i| TRUST_CUTS.contains(&lowered[i].as_str()))
        .unwrap_or(tokens.len());
    while end > start && TRUST_TAIL.contains(&lowered[end - 1].as_str()) {
        end -= 1;
    }
    (end > start + 1).then(|| {
        tokens[start..end]
            .join(" ")
            .trim_end_matches(',')
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(text: &str) -> PersonName {
        parse_person_name(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    #[test]
    fn the_same_person_in_three_spellings_has_one_key_and_listed_steps() {
        let a = person("Smith, John A. Jr.");
        let b = person("John A Smith Jr");
        let c = person("JOHN A. SMITH, JR.");
        assert_eq!(a.key(), "smith|john|a|jr");
        assert_eq!(a.key(), b.key());
        assert_eq!(a.key(), c.key());
        assert!(a.steps.contains(&"person:last_first_reordered".to_string()));
        assert!(!b.steps.contains(&"person:last_first_reordered".to_string()));
        assert!(
            matches!(compare_persons(&a, &c), PersonRelation::Equivalent { steps } if steps.contains(&"person:suffix_split".to_string()))
        );
        let jane = person("Jane Doe");
        assert!(compare_persons(&jane, &person("Doe, Jane")).eq_equiv());
    }

    impl PersonRelation {
        fn eq_equiv(&self) -> bool {
            matches!(self, PersonRelation::Equivalent { .. })
        }
    }

    #[test]
    fn honorifics_particles_and_the_comma_suffix_form() {
        assert_eq!(person("Dr. Jane Doe").key(), "doe|jane||");
        assert_eq!(person("Ludwig van Beethoven").family, "van beethoven");
        assert_eq!(person("John Smith, Jr").suffix, "jr");
        assert_eq!(person("Smith, John, III").key(), "smith|john||iii");
        assert_eq!(person("Smith, Mary Ann").middle, "ann");
    }

    #[test]
    fn credentials_are_dropped_and_are_not_read_as_a_given_name() {
        let jane = person("Jane Doe");
        for text in [
            "Dr. Jane Doe, MD",
            "Jane Doe, M.D.",
            "Jane Doe MD",
            "Jane Doe, Esq.",
            "Doe, Jane, CPA",
            "Doe, Jane JD",
            "Jane Doe, DO",
            "Jane Doe, MD, PhD",
        ] {
            let parsed = person(text);
            assert_eq!(parsed.key(), "doe|jane||", "{text}");
            assert!(
                parsed.steps.contains(&"person:drop_credential".to_string()),
                "{text}"
            );
            assert!(compare_persons(&parsed, &jane).eq_equiv(), "{text}");
        }
        let junior = person("John A. Smith Jr., MD");
        assert_eq!(junior.suffix, "jr");
        assert!(matches!(
            compare_persons(&junior, &person("John A. Smith Sr.")),
            PersonRelation::SuffixConflict { .. }
        ));
    }

    #[test]
    fn a_name_that_only_looks_like_a_credential_is_kept() {
        for (text, key) in [
            ("Smith, Jones", "smith|jones||"),
            // a family name, with no comma before it
            ("Nguyen Van Do", "van do|nguyen||"),
            ("Anh Do", "do|anh||"),
            // dropping it would leave one name word
            ("Smith, Do", "smith|do||"),
            ("Jane Pe", "pe|jane||"),
        ] {
            let parsed = person(text);
            assert_eq!(parsed.key(), key, "{text}");
            assert!(
                !parsed.steps.contains(&"person:drop_credential".to_string()),
                "{text}"
            );
        }
    }

    #[test]
    fn a_single_word_or_odd_shape_is_not_a_person_name() {
        for text in ["Smith", "", "Jr", "A, B, C, D", "Smith, Jones, Brown"] {
            assert!(parse_person_name(text).is_none(), "{text}");
        }
    }

    #[test]
    fn middle_absent_is_equivalent_with_a_step_and_initial_versus_name_is_only_similar() {
        let plain = person("John Smith");
        let with_middle = person("John A. Smith");
        match compare_persons(&plain, &with_middle) {
            PersonRelation::Equivalent { steps } => {
                assert!(steps.contains(&"person:middle_absent".to_string()))
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            compare_persons(&with_middle, &person("John Andrew Smith")),
            PersonRelation::MiddleInitialVsName
        );
        // Different middle initials are not equivalent and not even similar.
        assert_eq!(
            compare_persons(&with_middle, &person("John B. Smith")),
            PersonRelation::Unrelated
        );
        // Two different full middle names are not similar.
        assert_eq!(
            compare_persons(&person("John Andrew Smith"), &person("John Alan Smith")),
            PersonRelation::Unrelated
        );
    }

    #[test]
    fn suffixes_conflict_only_when_both_are_present() {
        let junior = person("John A. Smith Jr.");
        assert_eq!(
            compare_persons(&junior, &person("John A. Smith Sr.")),
            PersonRelation::SuffixConflict {
                left: "jr".into(),
                right: "sr".into()
            }
        );
        // One side without a suffix: nothing claimed.
        assert_eq!(
            compare_persons(&junior, &person("John A. Smith")),
            PersonRelation::Unrelated
        );
        // Different given or family names are unrelated, not conflicting.
        assert_eq!(
            compare_persons(&junior, &person("Jon A. Smith Sr.")),
            PersonRelation::Unrelated
        );
    }

    #[test]
    fn a_family_name_and_an_initial_alone_never_match() {
        assert_eq!(
            compare_persons(&person("J. Smith"), &person("John Smith")),
            PersonRelation::Unrelated
        );
        // No nickname table: Bob and Robert are different given names.
        assert_eq!(
            compare_persons(&person("Bob Smith"), &person("Robert Smith")),
            PersonRelation::Unrelated
        );
    }

    #[test]
    fn classification_is_table_driven_and_unknown_stays_unknown() {
        let class = |text: &str| classify_name(text);
        assert_eq!(class("Jr"), NameClass::Fragment);
        assert_eq!(class("LP"), NameClass::Fragment);
        assert_eq!(class("Sr."), NameClass::Fragment);
        assert_eq!(class("L.L.C."), NameClass::Fragment);
        assert_eq!(class("John Smith LLC"), NameClass::Org);
        assert_eq!(class("Smith Holdings, L.P."), NameClass::Org);
        assert_eq!(
            class("The John A. Smith Revocable Trust dated 1/1/2001"),
            NameClass::Trust
        );
        assert_eq!(class("Estate of John A. Smith"), NameClass::Trust);
        assert_eq!(class("Jane Doe Family Trust"), NameClass::Trust);
        assert_eq!(class("Smith Family u/a/d 3/3/99"), NameClass::Trust);
        // A plausible person is NOT guessed into person.
        assert_eq!(class("John A. Smith"), NameClass::Unknown);
        assert_eq!(class("Smith, Jones"), NameClass::Unknown);
        assert_eq!(class(""), NameClass::Unknown);
        // Real-estate words are not a trust cue.
        assert_eq!(class("Smith Real Estate Holdings"), NameClass::Unknown);
    }

    #[test]
    fn a_trust_names_its_holder() {
        assert_eq!(
            trust_holder("The John A. Smith Revocable Trust dated 1/1/2001").as_deref(),
            Some("John A. Smith")
        );
        assert_eq!(
            trust_holder("Estate of John A. Smith").as_deref(),
            Some("John A. Smith")
        );
        assert_eq!(
            trust_holder("Jane Doe Revocable Living Trust").as_deref(),
            Some("Jane Doe")
        );
        // A one-word holder is not enough to name a person.
        assert_eq!(trust_holder("Smith Family Trust"), None);
    }
}
