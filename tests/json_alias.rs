//! `--json` is shorthand for `--emit json` on the lookup command (bd-ko97).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(rel)
}

fn lookup(extra: &[&str]) -> Output {
    let input = fixture_path("inputs/partial.csv");
    let registry = fixture_path("registries/cusip-isin");
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(input)
        .args(["--registry"])
        .arg(registry)
        .args(["--column", "cusip", "--no-witness"])
        .args(extra)
        .output()
        .expect("failed to execute canon binary")
}

#[test]
fn json_flag_output_is_byte_identical_to_the_default_and_to_emit_json() {
    let default = lookup(&[]);
    let alias = lookup(&["--json"]);
    let explicit = lookup(&["--emit", "json"]);

    assert!(
        !default.stdout.is_empty(),
        "lookup must print the mapping artifact: stderr={}",
        String::from_utf8_lossy(&default.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&alias.stdout)
        .expect("--json stdout must be one JSON document");

    assert_eq!(alias.stdout, default.stdout, "--json == default output");
    assert_eq!(
        alias.stdout, explicit.stdout,
        "--json == --emit json output"
    );
    assert_eq!(alias.status.code(), default.status.code());
    assert_eq!(alias.status.code(), explicit.status.code());
}

#[test]
fn json_flag_with_emit_is_refused_before_any_lookup_runs() {
    for emit in ["csv", "json"] {
        let output = lookup(&["--json", "--emit", emit]);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert_eq!(
            output.status.code(),
            Some(2),
            "--json --emit {emit} is a usage error: stderr={stderr}"
        );
        assert!(
            output.stdout.is_empty(),
            "a refused invocation prints nothing on stdout"
        );
        assert!(
            stderr.contains("--json") && stderr.contains("--emit"),
            "the refusal must name both flags: {stderr}"
        );
    }
}
