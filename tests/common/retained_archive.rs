#![allow(dead_code)]

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Component, Path},
    process::Command,
};

#[derive(Debug, Clone)]
pub enum RetainedDigest {
    Sha256(String),
    Blake3(String),
}

#[derive(Debug)]
pub struct RetainedArchiveError {
    pub code: &'static str,
    pub detail: BTreeMap<String, String>,
}

fn error(code: &'static str, message: impl ToString) -> RetainedArchiveError {
    RetainedArchiveError {
        code,
        detail: BTreeMap::from([("message".into(), message.to_string())]),
    }
}

pub fn verify_archive(path: &Path, expected: &RetainedDigest) -> Result<(), RetainedArchiveError> {
    let mut file = File::open(path).map_err(|e| error("missing_member", e))?;
    let mut sha = Sha256::new();
    let mut blake = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| error("parse", e))?;
        if count == 0 {
            break;
        }
        sha.update(&buffer[..count]);
        blake.update(&buffer[..count]);
    }
    let (actual, wanted) = match expected {
        RetainedDigest::Sha256(wanted) => (format!("{:x}", sha.finalize()), wanted),
        RetainedDigest::Blake3(wanted) => (blake.finalize().to_hex().to_string(), wanted),
    };
    if actual != *wanted {
        return Err(RetainedArchiveError {
            code: "digest_mismatch",
            detail: BTreeMap::from([
                ("path".into(), path.display().to_string()),
                ("expected".into(), wanted.clone()),
                ("actual".into(), actual),
            ]),
        });
    }
    Ok(())
}

fn safe_member(member: &str) -> bool {
    !member.is_empty()
        && Path::new(member)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

pub fn extract_tar_gz(
    path: &Path,
    expected: &RetainedDigest,
    members: &[&str],
    into: &Path,
) -> Result<(), RetainedArchiveError> {
    verify_archive(path, expected)?;
    for member in members {
        if !safe_member(member) {
            return Err(error("unsafe_member", member));
        }
    }
    std::fs::create_dir_all(into).map_err(|e| error("parse", e))?;
    // Read only named regular-file contents. Never delegate symlink/path traversal to tar extraction.
    for member in members {
        let output = Command::new("tar")
            .args(["-xOzf"])
            .arg(path)
            .arg("--")
            .arg(member)
            .output()
            .map_err(|e| error("tar_failed", e))?;
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            return Err(error("missing_member", member));
        }
        let destination = into.join(member);
        std::fs::create_dir_all(destination.parent().expect("member parent"))
            .map_err(|e| error("parse", e))?;
        std::fs::write(destination, output.stdout).map_err(|e| error("parse", e))?;
    }
    Ok(())
}

pub fn read_json_gz_files(
    path: &Path,
    expected: &RetainedDigest,
) -> Result<BTreeMap<String, Vec<u8>>, RetainedArchiveError> {
    verify_archive(path, expected)?;
    let file = File::open(path).map_err(|e| error("missing_member", e))?;
    let value: serde_json::Value =
        serde_json::from_reader(GzDecoder::new(file)).map_err(|e| error("parse", e))?;
    let files = value["files"]
        .as_object()
        .ok_or_else(|| error("parse", "files object missing"))?;
    let mut result = BTreeMap::new();
    for (name, contents) in files {
        if !safe_member(name) {
            return Err(error("unsafe_member", name));
        }
        let bytes = contents
            .as_str()
            .ok_or_else(|| error("parse", name))?
            .as_bytes()
            .to_vec();
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if value["file_sha256"][name].as_str() != Some(actual.as_str()) {
            return Err(error("digest_mismatch", name));
        }
        result.insert(name.clone(), bytes);
    }
    Ok(result)
}
