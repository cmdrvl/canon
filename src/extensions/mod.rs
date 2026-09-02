#![forbid(unsafe_code)]

#[allow(dead_code)]
pub mod identifier;
#[allow(dead_code)]
pub mod normalization;
#[allow(dead_code)]
pub mod ontology;
#[allow(dead_code)]
pub mod profile;
#[allow(dead_code)]
pub mod relation_policy;
pub mod review_policy;
#[allow(dead_code)]
pub mod vocabulary;

use std::{fs, io, path::Path};

pub const DOMAIN_NEUTRAL_EXTENSION_RUNTIME_RULE: &str = "domain_neutral_runtime_extension";
pub const DOMAIN_NEUTRAL_EXTENSION_DOC_RULE: &str = "domain_neutral_extension_docs";

pub const DOMAIN_NEUTRAL_EXTENSION_SOURCE_FILES: &[&str] = &[
    "src/extensions/identifier.rs",
    "src/extensions/normalization.rs",
    "src/extensions/ontology.rs",
    "src/extensions/profile.rs",
    "src/extensions/relation_policy.rs",
    "src/extensions/review_policy.rs",
    "src/extensions/vocabulary.rs",
];

pub const FORBIDDEN_EXTENSION_RUNTIME_TERMS: &[&str] = &[
    "cmbs",
    "regab",
    "loan",
    "loans",
    "servicer",
    "tranche",
    "issuer",
    "borrower",
    "collateral",
    "openfigi",
    "cusip",
    "isin",
    "sedol",
];

pub const FORBIDDEN_EXTENSION_DOC_REFERENCES: &[&str] = &[
    "cmbs",
    "regab",
    "src/entity/profiles/",
    "tests/fixtures/entity/",
    "cmbs_tenant_label",
    "regab_firm_identity",
    "strip_regab_noise",
];

pub const REQUIRED_NEUTRAL_DOC_REFERENCES: &[&str] = &["pkg.alpha", "pkg.beta"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceScanViolation {
    pub rule_id: &'static str,
    pub path: String,
    pub line: usize,
    pub term: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocScanViolation {
    pub rule_id: &'static str,
    pub path: String,
    pub reference: String,
}

pub fn scan_domain_neutral_extension_sources(
    repo_root: &Path,
) -> io::Result<Vec<SourceScanViolation>> {
    let mut violations = Vec::new();
    for relative_path in DOMAIN_NEUTRAL_EXTENSION_SOURCE_FILES {
        let source = fs::read_to_string(repo_root.join(relative_path))?;
        violations.extend(scan_stripped_rust_source(
            DOMAIN_NEUTRAL_EXTENSION_RUNTIME_RULE,
            relative_path,
            &source,
            FORBIDDEN_EXTENSION_RUNTIME_TERMS,
        ));
    }
    Ok(violations)
}

pub fn scan_stripped_rust_source(
    rule_id: &'static str,
    virtual_path: &str,
    source: &str,
    forbidden_terms: &[&str],
) -> Vec<SourceScanViolation> {
    let stripped = strip_rust_comments(source);
    stripped
        .lines()
        .enumerate()
        .flat_map(|(line_index, line)| {
            scan_line_for_terms(rule_id, virtual_path, line_index + 1, line, forbidden_terms)
        })
        .collect()
}

pub fn scan_extension_docs(
    path: &str,
    markdown: &str,
    forbidden_references: &[&str],
) -> Vec<DocScanViolation> {
    let lower_markdown = markdown.to_ascii_lowercase();
    forbidden_references
        .iter()
        .filter_map(|reference| {
            let lower_reference = reference.to_ascii_lowercase();
            lower_markdown
                .contains(&lower_reference)
                .then(|| DocScanViolation {
                    rule_id: DOMAIN_NEUTRAL_EXTENSION_DOC_RULE,
                    path: path.to_string(),
                    reference: (*reference).to_string(),
                })
        })
        .collect()
}

pub fn render_source_scan_report(violations: &[SourceScanViolation]) -> String {
    violations
        .iter()
        .map(|violation| {
            format!(
                "{}:{} [{}] term={} :: {}",
                violation.path,
                violation.line,
                violation.rule_id,
                violation.term,
                violation.excerpt
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_doc_scan_report(violations: &[DocScanViolation]) -> String {
    violations
        .iter()
        .map(|violation| {
            format!(
                "{} [{}] forbidden_reference={}",
                violation.path, violation.rule_id, violation.reference
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn strip_rust_comments(source: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Code,
        LineComment,
        BlockComment(usize),
        String,
        RawString(usize),
    }

    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut state = State::Code;
    let mut index = 0usize;

    while index < bytes.len() {
        match state {
            State::Code => {
                if let Some(prefix_len) = raw_string_prefix_len(bytes, index) {
                    output.push_str(&source[index..index + prefix_len]);
                    let hash_count = prefix_len.saturating_sub(2);
                    index += prefix_len;
                    state = State::RawString(hash_count);
                    continue;
                }
                if bytes[index] == b'"' {
                    output.push('"');
                    index += 1;
                    state = State::String;
                    continue;
                }
                if matches_bytes(bytes, index, b"//") {
                    output.push_str("  ");
                    index += 2;
                    state = State::LineComment;
                    continue;
                }
                if matches_bytes(bytes, index, b"/*") {
                    output.push_str("  ");
                    index += 2;
                    state = State::BlockComment(1);
                    continue;
                }
                output.push(bytes[index] as char);
                index += 1;
            }
            State::LineComment => {
                if bytes[index] == b'\n' {
                    output.push('\n');
                    index += 1;
                    state = State::Code;
                } else {
                    output.push(' ');
                    index += 1;
                }
            }
            State::BlockComment(mut depth) => {
                if matches_bytes(bytes, index, b"/*") {
                    output.push_str("  ");
                    index += 2;
                    depth += 1;
                    state = State::BlockComment(depth);
                } else if matches_bytes(bytes, index, b"*/") {
                    output.push_str("  ");
                    index += 2;
                    if depth == 1 {
                        state = State::Code;
                    } else {
                        depth -= 1;
                        state = State::BlockComment(depth);
                    }
                } else if bytes[index] == b'\n' {
                    output.push('\n');
                    index += 1;
                    state = State::BlockComment(depth);
                } else {
                    output.push(' ');
                    index += 1;
                    state = State::BlockComment(depth);
                }
            }
            State::String => {
                output.push(bytes[index] as char);
                if bytes[index] == b'\\' && index + 1 < bytes.len() {
                    index += 1;
                    output.push(bytes[index] as char);
                } else if bytes[index] == b'"' {
                    state = State::Code;
                }
                index += 1;
            }
            State::RawString(hash_count) => {
                if is_raw_string_terminator(bytes, index, hash_count) {
                    output.push('"');
                    for _ in 0..hash_count {
                        output.push('#');
                    }
                    index += 1 + hash_count;
                    state = State::Code;
                } else {
                    output.push(bytes[index] as char);
                    index += 1;
                }
            }
        }
    }

    output
}

fn scan_line_for_terms(
    rule_id: &'static str,
    virtual_path: &str,
    line_number: usize,
    line: &str,
    forbidden_terms: &[&str],
) -> Vec<SourceScanViolation> {
    let lower_line = line.to_ascii_lowercase();
    forbidden_terms
        .iter()
        .filter_map(|term| {
            find_term_boundary(&lower_line, term).map(|_| SourceScanViolation {
                rule_id,
                path: virtual_path.to_string(),
                line: line_number,
                term: (*term).to_string(),
                excerpt: normalize_excerpt(line),
            })
        })
        .collect()
}

fn normalize_excerpt(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_term_boundary(line: &str, term: &str) -> Option<usize> {
    let mut start = 0usize;
    while let Some(found_at) = line[start..].find(term) {
        let absolute = start + found_at;
        if has_identifier_boundary(line, absolute, term.len()) {
            return Some(absolute);
        }
        start = absolute + term.len();
    }
    None
}

fn has_identifier_boundary(line: &str, start: usize, len: usize) -> bool {
    let previous = line[..start].chars().next_back();
    let next = line[start + len..].chars().next();
    !previous.is_some_and(|character| character.is_ascii_alphanumeric())
        && !next.is_some_and(|character| character.is_ascii_alphanumeric())
}

fn matches_bytes(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes.get(index..index + needle.len()) == Some(needle)
}

fn raw_string_prefix_len(bytes: &[u8], index: usize) -> Option<usize> {
    if bytes.get(index) != Some(&b'r') {
        return None;
    }
    let mut cursor = index + 1;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'"')).then_some(cursor - index + 1)
}

fn is_raw_string_terminator(bytes: &[u8], index: usize, hash_count: usize) -> bool {
    if bytes.get(index) != Some(&b'"') {
        return false;
    }
    (0..hash_count).all(|offset| bytes.get(index + 1 + offset) == Some(&b'#'))
}
