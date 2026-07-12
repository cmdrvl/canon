#![allow(dead_code)]

use assert_cmd::prelude::*;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as StdCommand,
};

pub fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

pub fn fixture_path(relative: &str) -> PathBuf {
    manifest_dir().join(relative)
}

pub fn canon_bin() -> assert_cmd::Command {
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
}

pub fn canon_std_command_in_manifest() -> StdCommand {
    let mut command = StdCommand::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

pub fn run_canon_json(args: &[&str], exit_code: i32) -> Value {
    let assert = canon_std_command_in_manifest()
        .args(args)
        .assert()
        .code(exit_code);
    serde_json::from_slice(&assert.get_output().stdout).expect("canon stdout should be JSON")
}

pub fn write_registry_metadata(path: &Path, id: &str, version: &str, entry_count: usize) {
    write_registry_metadata_full(
        path,
        id,
        version,
        entry_count,
        "Test registry",
        "2026-01-01",
    );
}

pub fn write_registry_metadata_with_description(
    path: &Path,
    id: &str,
    version: &str,
    entry_count: usize,
    description: &str,
) {
    write_registry_metadata_full(path, id, version, entry_count, description, "2026-01-01");
}

pub fn write_registry_metadata_full(
    path: &Path,
    id: &str,
    version: &str,
    entry_count: usize,
    description: &str,
    updated: &str,
) {
    let registry_json = serde_json::json!({
        "id": id,
        "version": version,
        "description": description,
        "updated": updated,
        "entry_count": entry_count,
    });
    fs::write(
        path.join("registry.json"),
        serde_json::to_string_pretty(&registry_json).unwrap(),
    )
    .unwrap();
}

pub fn write_seed_csv(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
}

pub fn copy_json_registry_fixture(relative: &str, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    let source = fixture_path(relative);
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            fs::copy(path, destination.join(entry.file_name())).unwrap();
        }
    }
}
