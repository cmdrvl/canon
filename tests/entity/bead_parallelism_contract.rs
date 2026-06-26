#![forbid(unsafe_code)]

use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
struct BeadIssue {
    id: String,
    title: String,
    status: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    design: String,
}

#[test]
fn bead_parallelism_contract_flags_broad_runtime_source_reservations() {
    let violations = active_entity_workbench_beads()
        .into_iter()
        .filter_map(|issue| {
            let sources = named_runtime_sources(&issue);
            if sources.len() <= 2 || has_multi_file_justification(&issue) {
                return None;
            }

            Some(format!(
                "{} names {} runtime source files without multi-file justification: {}",
                issue.id,
                sources.len(),
                sources.into_iter().collect::<Vec<_>>().join(", ")
            ))
        })
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "broad entity-workbench runtime reservations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn bead_parallelism_contract_requires_parallel_split_verification_commands() {
    let parallel_split = active_entity_workbench_beads()
        .into_iter()
        .filter(|issue| has_label(issue, "parallel-split"))
        .collect::<Vec<_>>();
    assert!(
        !parallel_split.is_empty(),
        "expected active parallel-split entity-workbench beads"
    );

    let missing = parallel_split
        .into_iter()
        .filter(|issue| !has_quick_verification_command(issue))
        .map(|issue| format!("{} {}", issue.id, issue.title))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "parallel-split beads missing quick verification commands:\n{}",
        missing.join("\n")
    );
}

fn active_entity_workbench_beads() -> Vec<BeadIssue> {
    std::fs::read_to_string(".beads/issues.jsonl")
        .expect("read .beads/issues.jsonl")
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str::<BeadIssue>(line)
                .unwrap_or_else(|error| panic!("parse bead JSONL line {}: {error}", index + 1))
        })
        .filter(|issue| issue.status != "closed")
        .filter(|issue| has_label(issue, "entity-workbench"))
        .collect()
}

fn named_runtime_sources(issue: &BeadIssue) -> BTreeSet<String> {
    let text = issue_text(issue);
    let mut sources = BTreeSet::new();
    let mut offset = 0;

    while let Some(relative_start) = text[offset..].find("src/") {
        let start = offset + relative_start;
        let mut end = start;
        for (relative_index, ch) in text[start..].char_indices() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '.') {
                end = start + relative_index + ch.len_utf8();
            } else {
                break;
            }
        }

        let candidate = &text[start..end];
        if candidate.ends_with(".rs") {
            sources.insert(candidate.to_string());
        }
        offset = end.max(start + "src/".len());
    }

    sources
}

fn has_multi_file_justification(issue: &BeadIssue) -> bool {
    if has_label(issue, "multi-file-orchestration") {
        return true;
    }

    let text = issue_text(issue).to_ascii_lowercase();
    text.contains("multi-file-orchestration")
        || text.contains("explicit justification:")
        || text.contains("reservation justification:")
}

fn has_quick_verification_command(issue: &BeadIssue) -> bool {
    let text = issue_text(issue);
    text.contains("Verification commands:")
        && (text.contains("cargo test")
            || text.contains("cargo run")
            || text.contains("cargo clippy")
            || text.contains("cargo fmt"))
}

fn has_label(issue: &BeadIssue, label: &str) -> bool {
    issue.labels.iter().any(|candidate| candidate == label)
}

fn issue_text(issue: &BeadIssue) -> String {
    [
        issue.title.as_str(),
        issue.description.as_str(),
        issue.acceptance_criteria.as_str(),
        issue.notes.as_str(),
        issue.design.as_str(),
    ]
    .join("\n")
}
