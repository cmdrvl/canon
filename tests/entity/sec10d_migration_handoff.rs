#![forbid(unsafe_code)]

use canon::entity::prepare::{PrepareRunRequest, run_prepare};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const HANDOFF: &str =
    include_str!("../fixtures/entity/regab/sec10d_contract/migration_handoff.json");

#[derive(Debug, Deserialize)]
struct HandoffPacket {
    schema_version: String,
    profile_id: String,
    fixture_paths: HandoffFixturePaths,
    commands: Vec<HandoffCommand>,
    expected_work_dir_files: Vec<String>,
    regression_checklist: Vec<HandoffChecklistItem>,
}

#[derive(Debug, Deserialize)]
struct HandoffFixturePaths {
    org_mentions_csv: String,
    profile_strategy_yaml: String,
    registry_snapshot: String,
}

#[derive(Debug, Deserialize)]
struct HandoffCommand {
    stage: String,
    command: String,
}

#[derive(Debug, Deserialize)]
struct HandoffChecklistItem {
    id: String,
    invariant: String,
}

#[test]
fn sec10d_handoff_commands_use_entity_namespace_and_fixture_paths() {
    let packet = handoff_packet();
    assert_eq!(
        packet.schema_version,
        "canon.entity.sec10d_migration_handoff.v0"
    );

    let stages = packet
        .commands
        .iter()
        .map(|command| command.stage.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        [
            "prepare",
            "run",
            "review_export",
            "audit",
            "promote",
            "apply_exact_replay"
        ]
    );

    for command in &packet.commands {
        assert!(
            !command.command.contains("canon org"),
            "{} handoff command must not use the retired org namespace",
            command.stage
        );
        assert!(
            !command.command.contains('<'),
            "{} handoff command must not contain placeholders",
            command.stage
        );
        assert!(
            command.command.starts_with("canon "),
            "{} handoff command is copy/paste oriented",
            command.stage
        );
    }

    for path in [
        &packet.fixture_paths.org_mentions_csv,
        &packet.fixture_paths.profile_strategy_yaml,
        &packet.fixture_paths.registry_snapshot,
    ] {
        assert!(
            repo_path(path).exists(),
            "handoff fixture path must exist: {path}"
        );
    }

    assert!(
        packet.commands[0]
            .command
            .contains(&packet.fixture_paths.org_mentions_csv)
    );
    assert!(
        packet.commands[1]
            .command
            .contains(&packet.fixture_paths.profile_strategy_yaml)
    );
    assert!(packet.commands.iter().all(|command| {
        command
            .command
            .contains(&packet.fixture_paths.registry_snapshot)
            || matches!(command.stage.as_str(), "review_export" | "audit")
    }));
}

#[test]
fn sec10d_handoff_checklist_names_parser_boundary_and_snowflake_fields() {
    let packet = handoff_packet();
    let checklist_ids = packet
        .regression_checklist
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();

    for required in [
        "parser_boundary",
        "raw_field_preservation",
        "snowflake_append_only_fields",
        "unresolved_reviewable_status",
        "core_exact_lookup_firewall",
        "no_network_or_model_runtime",
    ] {
        assert!(
            checklist_ids.contains(required),
            "missing handoff checklist item {required}"
        );
    }

    let snowflake = checklist_item(&packet, "snowflake_append_only_fields");
    for suffix in [
        "_org_canon_id",
        "_org_canonical_name",
        "_org_resolution_status",
        "_org_registry_id",
        "_org_registry_version",
        "_org_rule_id",
    ] {
        assert!(
            snowflake.invariant.contains(suffix),
            "Snowflake checklist must name {suffix}"
        );
    }

    let parser = checklist_item(&packet, "parser_boundary");
    assert!(parser.invariant.contains("parser"));
    assert!(parser.invariant.contains("unchanged"));
}

#[test]
fn sec10d_handoff_prepare_command_runs_against_canon_fixtures() {
    let packet = handoff_packet();
    let temp = tempfile::tempdir().expect("tempdir");
    let work_dir = temp.path().join("sec10d-regab-firms");

    let artifact = run_prepare(PrepareRunRequest {
        rows: &repo_path(&packet.fixture_paths.org_mentions_csv),
        profile: &packet.profile_id,
        registry: &repo_path(&packet.fixture_paths.registry_snapshot),
        work_dir: &work_dir,
    })
    .expect("handoff prepare command equivalent runs");

    assert_eq!(artifact.profile.id, "regab_firm_identity");
    assert_eq!(artifact.registry_snapshot.id, "firms");
    assert_eq!(artifact.summary["row_count"], 8);
    assert_eq!(artifact.summary["prepared_surfaces"], 8);
    assert!(work_dir.join("prepare/prepare.json").exists());
    assert!(work_dir.join("prepare/surfaces.jsonl").exists());

    for expected in packet
        .expected_work_dir_files
        .iter()
        .filter(|path| path.starts_with("prepare/"))
    {
        assert!(
            work_dir.join(expected).exists(),
            "prepare work-dir file should be emitted: {expected}"
        );
    }
}

fn handoff_packet() -> HandoffPacket {
    serde_json::from_str(HANDOFF).expect("sec10d handoff packet parses")
}

fn checklist_item<'a>(packet: &'a HandoffPacket, id: &str) -> &'a HandoffChecklistItem {
    packet
        .regression_checklist
        .iter()
        .find(|item| item.id == id)
        .unwrap_or_else(|| panic!("missing checklist item {id}"))
}

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}
