//! `--json` is shorthand for `--emit json` on the registry commands (bd-ko97).

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

fn canon(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .expect("failed to execute canon binary")
}

fn fixture_registry() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("registries")
        .join("cusip-isin")
}

fn empty_registry_with_scheme() -> TempDir {
    let temp = TempDir::new().unwrap();
    let metadata = json!({
        "id": "people",
        "version": "1.2.3",
        "description": "registry --json alias test fixture",
        "updated": "2026-05-27",
        "entry_count": 0,
        "owner": "test-suite",
        "default_id_scheme": {"prefix": "PPL", "zero_pad": 5}
    });
    fs::write(
        temp.path().join("registry.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    temp
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn next_id_json_flag_flips_the_plain_default_to_the_same_json_as_emit_json() {
    let registry = empty_registry_with_scheme();
    let registry = registry.path().to_str().unwrap();
    let base = ["registry", "next-id", "--registry", registry];
    let run = |extra: &[&str]| canon(&[&base[..], extra].concat());

    let default = run(&[]);
    let alias = run(&["--json"]);
    let explicit = run(&["--emit", "json"]);

    assert_eq!(default.status.code(), Some(0), "{}", stderr(&default));
    assert!(
        serde_json::from_slice::<Value>(&default.stdout).is_err(),
        "next-id defaults to plain text, not JSON"
    );
    let document: Value =
        serde_json::from_slice(&alias.stdout).expect("next-id --json must print one JSON document");
    assert!(document.is_object());
    assert_eq!(alias.stdout, explicit.stdout, "--json == --emit json");
    assert_eq!(alias.status.code(), explicit.status.code());
    assert_ne!(alias.stdout, default.stdout);
}

#[test]
fn json_flag_matches_the_json_default_on_registry_reports() {
    let registry = fixture_registry();
    let registry = registry.to_str().unwrap();
    let commands: [&[&str]; 3] = [
        &["registry", "providers"],
        &["registry", "identity", "--registry", registry],
        &["registry", "lint", registry],
    ];
    for command in commands {
        let default = canon(command);
        let alias = canon(&[command, &["--json"]].concat());
        let explicit = canon(&[command, &["--emit", "json"]].concat());

        let label = command[..2].join(" ");
        assert_eq!(
            default.status.code(),
            Some(0),
            "{label}: {}",
            stderr(&default)
        );
        assert!(!default.stdout.is_empty(), "{label} prints a report");
        assert_eq!(alias.stdout, default.stdout, "{label} --json == default");
        assert_eq!(
            alias.stdout, explicit.stdout,
            "{label} --json == --emit json"
        );
        assert_eq!(alias.status.code(), default.status.code(), "{label}");
    }
}

#[test]
fn json_flag_with_emit_is_refused_on_registry_commands_before_anything_runs() {
    let registry = empty_registry_with_scheme();
    let registry = registry.path().to_str().unwrap();
    let fixture = fixture_registry();
    let fixture = fixture.to_str().unwrap();
    let cases: [(&[&str], &str); 5] = [
        (&["registry", "next-id", "--registry", registry], "plain"),
        (&["registry", "next-id", "--registry", registry], "json"),
        (&["registry", "lint", fixture], "summary"),
        (&["registry", "lint", fixture], "json"),
        (&["registry", "providers"], "summary"),
    ];
    for (command, emit) in cases {
        let output = canon(&[command, &["--json", "--emit", emit]].concat());
        let label = format!("{} --json --emit {emit}", command[..2].join(" "));
        let message = stderr(&output);

        assert_eq!(output.status.code(), Some(2), "{label}: {message}");
        assert!(output.stdout.is_empty(), "{label} printed on stdout");
        assert!(
            message.contains("--json") && message.contains("--emit"),
            "{label}: the refusal must name both flags: {message}"
        );
    }
}
