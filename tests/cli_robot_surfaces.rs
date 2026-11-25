//! Agent discovery surfaces (bd-c98n): `canon capabilities --json`,
//! `canon robot-docs guide`, `canon doctor triage --json` and its deprecated
//! spellings `canon --robot-triage` and `canon doctor --robot-triage`.
//!
//! Every test runs the real `canon` binary. `capabilities --json` must be the
//! one operator.v1 document `--describe` prints, the deprecated spellings must
//! be the same read-only doctor report as the canonical command, and none of
//! them may be a second report that can drift.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::tempdir;

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Run canon in `dir` with a witness path that must stay untouched and the
/// given extra environment.
fn run_canon(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let witness = dir.join("witness").join("canon-witness.jsonl");
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command
        .current_dir(dir)
        .env("HOME", dir)
        .env("EPISTEMIC_WITNESS", &witness)
        .env_remove("NO_COLOR")
        .env_remove("TERM")
        .args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("canon runs");
    Run {
        code: output.status.code().expect("canon exits normally"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}

fn assert_nothing_written(dir: &Path) {
    let leftovers: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(
        leftovers.is_empty(),
        "discovery commands must not create files, found {leftovers:?}"
    );
}

/// True when this binary has the `canon geo` commands. Read from the binary's
/// own `--describe` contract, which lists only the commands it was compiled
/// with, so the answer is right in every build profile.
fn geo_compiled(dir: &Path) -> bool {
    let run = run_canon(dir, &["--describe"], &[]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    let manifest: Value = serde_json::from_str(&run.stdout).expect("--describe emits JSON");
    manifest["commands"]
        .as_array()
        .expect("--describe lists commands")
        .iter()
        .any(|entry| {
            entry["name"]
                .as_str()
                .is_some_and(|name| name == "geo" || name.starts_with("geo "))
        })
}

#[test]
fn robot_triage_is_the_doctor_triage_report() {
    let dir = tempdir().unwrap();
    let canonical = run_canon(dir.path(), &["doctor", "triage", "--json"], &[]);
    let top = run_canon(dir.path(), &["--robot-triage"], &[]);
    let doctor = run_canon(dir.path(), &["doctor", "--robot-triage"], &[]);

    assert_eq!(canonical.code, 0, "stderr: {}", canonical.stderr);
    assert_eq!(top.code, 0, "stderr: {}", top.stderr);
    assert_eq!(top.stderr, "");
    assert_eq!(
        top.stdout, canonical.stdout,
        "deprecated top-level triage must be byte-identical to `doctor triage --json`"
    );
    assert_eq!(
        doctor.stdout, canonical.stdout,
        "deprecated `doctor --robot-triage` must be byte-identical to `doctor triage --json`"
    );
    let payload: Value = serde_json::from_str(&top.stdout).expect("one JSON object");
    assert_eq!(payload["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(payload["mode"], "triage");
    assert_eq!(payload["healthy"], true);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["fixers"], serde_json::json!([]));
    assert_eq!(top.stdout.trim_end().lines().count(), 1, "single-line JSON");
    assert_nothing_written(dir.path());
}

#[test]
fn robot_triage_needs_no_registry_or_column() {
    // A lookup-shaped invocation requires --registry and --column; the triage
    // flag must satisfy those requirements the way --describe does.
    let dir = tempdir().unwrap();
    let run = run_canon(dir.path(), &["--robot-triage"], &[]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    assert!(!run.stderr.contains("--registry"), "stderr: {}", run.stderr);
}

#[test]
fn capabilities_json_is_the_describe_document() {
    let dir = tempdir().unwrap();
    let caps = run_canon(dir.path(), &["capabilities", "--json"], &[]);
    let describe = run_canon(dir.path(), &["--describe"], &[]);

    assert_eq!(caps.code, 0, "stderr: {}", caps.stderr);
    assert_eq!(caps.stderr, "");
    assert_eq!(
        caps.stdout, describe.stdout,
        "`capabilities --json` must print the same operator.v1 bytes as `--describe`"
    );
    let manifest: Value = serde_json::from_str(&caps.stdout).expect("one JSON object");
    assert_eq!(manifest["schema_version"], "operator.v1");
    assert_eq!(manifest["name"], "canon");
    assert_eq!(manifest["tool_version"], env!("CARGO_PKG_VERSION"));
    assert_nothing_written(dir.path());
}

#[test]
fn doctor_capabilities_report_embeds_the_capabilities_document() {
    let dir = tempdir().unwrap();
    let caps = run_canon(dir.path(), &["capabilities", "--json"], &[]);
    let doctor = run_canon(dir.path(), &["doctor", "capabilities", "--json"], &[]);

    assert_eq!(doctor.code, 0, "stderr: {}", doctor.stderr);
    assert_eq!(doctor.stderr, "");
    let report: Value = serde_json::from_str(&doctor.stdout).expect("one JSON object");
    let contract: Value = serde_json::from_str(&caps.stdout).expect("capabilities is JSON");
    assert_eq!(report["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(report["mode"], "capabilities");
    assert_eq!(report["healthy"], true);
    assert_eq!(
        report["contract"], contract,
        "`contract` must be the document `capabilities --json` prints"
    );
    assert_eq!(report["read_only"], true);
    assert_eq!(report["fixers"], serde_json::json!([]));
    assert_eq!(
        report["top_level_commands"]["capabilities"]["command"],
        "canon capabilities --json"
    );
    assert_eq!(
        report["top_level_commands"]["robot_docs"]["command"],
        "canon robot-docs guide"
    );
    let deprecated: Vec<&str> = report["deprecated_aliases"]
        .as_array()
        .expect("deprecated_aliases is an array")
        .iter()
        .filter_map(|entry| entry["command"].as_str())
        .collect();
    for alias in [
        "canon doctor health [--json]",
        "canon doctor --robot-triage",
        "canon --robot-triage",
    ] {
        assert!(deprecated.contains(&alias), "missing alias {alias}");
    }
    assert_nothing_written(dir.path());
}

#[test]
fn doctor_health_alias_is_the_canonical_health_report() {
    let dir = tempdir().unwrap();
    let canonical = run_canon(dir.path(), &["doctor", "--json"], &[]);
    let alias = run_canon(dir.path(), &["doctor", "health", "--json"], &[]);
    let json_first = run_canon(dir.path(), &["doctor", "--json", "triage"], &[]);
    let triage = run_canon(dir.path(), &["doctor", "triage", "--json"], &[]);

    assert_eq!(canonical.code, 0, "stderr: {}", canonical.stderr);
    assert_eq!(alias.stdout, canonical.stdout);
    assert_eq!(
        json_first.stdout, triage.stdout,
        "--json before or after the subcommand is the same request"
    );
    let payload: Value = serde_json::from_str(&canonical.stdout).expect("one JSON object");
    assert_eq!(payload["mode"], "health");
    assert_nothing_written(dir.path());
}

#[test]
fn capabilities_without_json_prints_text_not_json() {
    let dir = tempdir().unwrap();
    let run = run_canon(dir.path(), &["capabilities"], &[]);
    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    assert!(run.stdout.starts_with("canon capabilities\n"));
    assert!(serde_json::from_str::<Value>(&run.stdout).is_err());
}

#[test]
fn robot_docs_guide_is_read_only_markdown() {
    let dir = tempdir().unwrap();
    let run = run_canon(dir.path(), &["robot-docs", "guide"], &[]);

    assert_eq!(run.code, 0, "stderr: {}", run.stderr);
    assert_eq!(run.stderr, "");
    assert!(run.stdout.starts_with("# canon robot-docs guide\n"));
    for needle in [
        "canon --robot-triage",
        "canon capabilities --json",
        "canon robot-docs guide",
        "canon doctor --robot-triage",
        "canon doctor triage --json",
        "canon doctor --json",
        "Exit codes:",
        "no --fix surface",
        "ENTITY_WORKBENCH_AGENT_GUIDE.md",
    ] {
        assert!(
            run.stdout.contains(needle),
            "guide must contain `{needle}`:\n{}",
            run.stdout
        );
    }
    assert_nothing_written(dir.path());

    // The guide must advertise Geo commands exactly when this binary has them.
    assert_eq!(
        run.stdout.contains("canon geo capabilities --guide"),
        geo_compiled(dir.path())
    );
}

#[test]
fn discovery_output_ignores_no_color_and_dumb_terminals() {
    // canon emits no color at all, so NO_COLOR, TERM=dumb, and the non-tty
    // stdout of a pipe must all produce identical, escape-free bytes.
    let dir = tempdir().unwrap();
    for args in [
        &["--robot-triage"][..],
        &["doctor", "triage", "--json"][..],
        &["capabilities", "--json"][..],
        &["capabilities"][..],
        &["robot-docs", "guide"][..],
    ] {
        let plain = run_canon(dir.path(), args, &[]);
        let no_color = run_canon(dir.path(), args, &[("NO_COLOR", "1")]);
        let dumb = run_canon(dir.path(), args, &[("TERM", "dumb")]);
        let forced = run_canon(
            dir.path(),
            args,
            &[("CLICOLOR_FORCE", "1"), ("TERM", "xterm-256color")],
        );
        for (label, run) in [
            ("NO_COLOR", &no_color),
            ("TERM=dumb", &dumb),
            ("CLICOLOR_FORCE", &forced),
        ] {
            assert_eq!(run.code, plain.code, "{args:?} under {label}");
            assert_eq!(run.stdout, plain.stdout, "{args:?} under {label}");
            assert_eq!(run.stderr, plain.stderr, "{args:?} under {label}");
        }
        assert!(
            !plain.stdout.contains('\u{1b}') && !plain.stderr.contains('\u{1b}'),
            "{args:?} must emit no ANSI escapes"
        );
    }
}

#[test]
fn robot_docs_requires_the_guide_subcommand() {
    let dir = tempdir().unwrap();

    let bare = run_canon(dir.path(), &["robot-docs"], &[]);
    assert_eq!(bare.code, 2, "bare robot-docs is a usage error");
    assert_eq!(bare.stdout, "", "usage errors never write stdout");
    assert!(bare.stderr.contains("guide"), "stderr: {}", bare.stderr);

    let typo = run_canon(dir.path(), &["robot-docs", "gide"], &[]);
    assert_eq!(typo.code, 2);
    assert_eq!(typo.stdout, "");
    assert_nothing_written(dir.path());
}

#[test]
fn discovery_commands_reject_a_fix_surface() {
    let dir = tempdir().unwrap();
    for args in [
        &["capabilities", "--fix"][..],
        &["robot-docs", "guide", "--fix"][..],
        &["--robot-triage", "--fix"][..],
    ] {
        let run = run_canon(dir.path(), args, &[]);
        assert_eq!(run.code, 2, "{args:?} must be a usage error");
        assert_eq!(run.stdout, "", "{args:?} must not write stdout");
    }
    assert_nothing_written(dir.path());
}

#[test]
fn a_misspelled_top_level_command_still_gets_a_did_you_mean() {
    // The new command names join the did-you-mean table; a typo must be
    // refused with a suggestion, not read as an input file path.
    let dir = tempdir().unwrap();
    let run = run_canon(
        dir.path(),
        &["capabilites", "--registry", "r", "--column", "c"],
        &[],
    );
    assert_eq!(run.code, 2);
    let refusal: Value = serde_json::from_str(&run.stdout).expect("refusal envelope on stdout");
    assert_eq!(
        refusal["refusal"]["detail"]["suggested_subcommand"],
        "capabilities"
    );
}
