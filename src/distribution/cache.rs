use super::package;
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentCache {
    root: PathBuf,
    writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedBlob {
    pub digest: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub already_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPackage {
    pub archive_digest: String,
    pub package_content_digest: String,
    pub package_bytes_digest: String,
    pub path: PathBuf,
    pub verified_files: usize,
    pub verified_bytes: u64,
    pub already_present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentCacheErrorKind {
    InvalidDigest,
    DigestMismatch,
    ReadOnly,
    Io,
    Package,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentCacheError {
    pub kind: ContentCacheErrorKind,
    pub message: String,
}

impl ContentCacheError {
    fn new(kind: ContentCacheErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for ContentCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ContentCacheError {}

impl ContentCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            writable: true,
        }
    }

    pub fn read_only(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            writable: false,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_writable(&self) -> bool {
        self.writable
    }

    pub fn blob_path(&self, digest: &str) -> Result<PathBuf, ContentCacheError> {
        let (algorithm, hex) = parse_digest(digest)?;
        Ok(self.root.join("oci/blobs").join(algorithm).join(hex))
    }

    pub fn package_archive_path(&self, archive_digest: &str) -> Result<PathBuf, ContentCacheError> {
        let (algorithm, hex) = parse_digest(archive_digest)?;
        Ok(self
            .root
            .join("packages")
            .join(algorithm)
            .join(hex)
            .join("archive.json"))
    }

    pub fn get_blob(&self, digest: &str) -> Result<Option<Vec<u8>>, ContentCacheError> {
        let path = self.blob_path(digest)?;
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_file(&path)?;
        verify_digest(digest, &bytes)?;
        Ok(Some(bytes))
    }

    pub fn put_blob(&self, digest: &str, bytes: &[u8]) -> Result<CachedBlob, ContentCacheError> {
        verify_digest(digest, bytes)?;
        let path = self.blob_path(digest)?;
        let already_present = put_verified_bytes(self, &path, digest, bytes)?;
        Ok(CachedBlob {
            digest: digest.to_string(),
            path,
            bytes: bytes.len() as u64,
            already_present,
        })
    }

    pub fn materialize_package_archive(
        &self,
        archive_bytes: &[u8],
    ) -> Result<CachedPackage, ContentCacheError> {
        let verification = package::verify_local_package(archive_bytes).map_err(|error| {
            ContentCacheError::new(
                ContentCacheErrorKind::Package,
                format!("local package verification failed before cache materialization: {error}"),
            )
        })?;
        let path = self.package_archive_path(&verification.archive_digest)?;
        let already_present =
            put_package_archive_bytes(self, &path, &verification.archive_digest, archive_bytes)?;
        Ok(CachedPackage {
            archive_digest: verification.archive_digest,
            package_content_digest: verification.package_content_digest,
            package_bytes_digest: verification.package_bytes_digest,
            path,
            verified_files: verification.verified_files,
            verified_bytes: verification.verified_bytes,
            already_present,
        })
    }
}

fn put_package_archive_bytes(
    cache: &ContentCache,
    path: &Path,
    archive_digest: &str,
    bytes: &[u8],
) -> Result<bool, ContentCacheError> {
    if path.exists() {
        let existing = read_file(path)?;
        let existing_verification = package::verify_local_package(&existing).map_err(|error| {
            ContentCacheError::new(
                ContentCacheErrorKind::Package,
                format!(
                    "cached package archive {} is invalid: {error}",
                    path.display()
                ),
            )
        })?;
        if existing_verification.archive_digest != archive_digest {
            return Err(ContentCacheError::new(
                ContentCacheErrorKind::DigestMismatch,
                format!(
                    "cache path {} has archive digest {}, expected {archive_digest}",
                    path.display(),
                    existing_verification.archive_digest
                ),
            ));
        }
        if existing == bytes {
            return Ok(true);
        }
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::DigestMismatch,
            format!(
                "cache path {} already contains different package archive bytes",
                path.display()
            ),
        ));
    }

    if !cache.writable {
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::ReadOnly,
            format!(
                "cache is read-only and {} is not materialized",
                path.display()
            ),
        ));
    }

    let parent = path.parent().ok_or_else(|| {
        ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!("cache path has no parent: {}", path.display()),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!(
                "failed to create cache directory {}: {error}",
                parent.display()
            ),
        )
    })?;

    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(ContentCacheError::new(
                ContentCacheErrorKind::Io,
                format!(
                    "failed to remove stale temp cache file {}: {error}",
                    tmp.display()
                ),
            ));
        }
    }
    fs::write(&tmp, bytes).map_err(|error| {
        ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!("failed to write temp cache file {}: {error}", tmp.display()),
        )
    })?;
    let written = read_file(&tmp)?;
    let written_verification = package::verify_local_package(&written).map_err(|error| {
        ContentCacheError::new(
            ContentCacheErrorKind::Package,
            format!("temp package archive {} is invalid: {error}", tmp.display()),
        )
    })?;
    if written_verification.archive_digest != archive_digest {
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::DigestMismatch,
            format!(
                "temp package archive {} has digest {}, expected {archive_digest}",
                tmp.display(),
                written_verification.archive_digest
            ),
        ));
    }

    match fs::rename(&tmp, path) {
        Ok(()) => Ok(false),
        Err(_error) if path.exists() => {
            let existing = read_file(path)?;
            let existing_verification =
                package::verify_local_package(&existing).map_err(|error| {
                    ContentCacheError::new(
                        ContentCacheErrorKind::Package,
                        format!(
                            "cached package archive {} is invalid: {error}",
                            path.display()
                        ),
                    )
                })?;
            if existing_verification.archive_digest == archive_digest && existing == bytes {
                let _ = fs::remove_file(&tmp);
                Ok(true)
            } else {
                Err(ContentCacheError::new(
                    ContentCacheErrorKind::DigestMismatch,
                    format!(
                        "cache path {} won a race with different bytes",
                        path.display()
                    ),
                ))
            }
        }
        Err(error) => Err(ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!(
                "failed to move temp cache file {} to {}: {error}",
                tmp.display(),
                path.display()
            ),
        )),
    }
}

pub fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", encode_hex(&sha256(bytes)))
}

pub fn verify_digest(digest: &str, bytes: &[u8]) -> Result<(), ContentCacheError> {
    let (algorithm, _) = parse_digest(digest)?;
    let actual = match algorithm {
        "sha256" => sha256_digest(bytes),
        "blake3" => format!("blake3:{}", blake3::hash(bytes).to_hex()),
        _ => {
            return Err(ContentCacheError::new(
                ContentCacheErrorKind::InvalidDigest,
                format!("unsupported content digest algorithm {algorithm}"),
            ));
        }
    };
    if digest == actual {
        Ok(())
    } else {
        Err(ContentCacheError::new(
            ContentCacheErrorKind::DigestMismatch,
            format!("content digest mismatch: expected {digest}, found {actual}"),
        ))
    }
}

pub fn parse_digest(digest: &str) -> Result<(&str, &str), ContentCacheError> {
    let Some((algorithm, hex)) = digest.split_once(':') else {
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::InvalidDigest,
            format!("content digest must include algorithm prefix: {digest}"),
        ));
    };
    if !matches!(algorithm, "sha256" | "blake3") {
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::InvalidDigest,
            format!("unsupported content digest algorithm {algorithm}"),
        ));
    }
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::InvalidDigest,
            format!("content digest must contain 64 lowercase hex characters: {digest}"),
        ));
    }
    Ok((algorithm, hex))
}

fn put_verified_bytes(
    cache: &ContentCache,
    path: &Path,
    digest: &str,
    bytes: &[u8],
) -> Result<bool, ContentCacheError> {
    if path.exists() {
        let existing = read_file(path)?;
        verify_digest(digest, &existing)?;
        if existing == bytes {
            return Ok(true);
        }
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::DigestMismatch,
            format!(
                "cache path {} already contains different bytes",
                path.display()
            ),
        ));
    }

    if !cache.writable {
        return Err(ContentCacheError::new(
            ContentCacheErrorKind::ReadOnly,
            format!(
                "cache is read-only and {} is not materialized",
                path.display()
            ),
        ));
    }

    let parent = path.parent().ok_or_else(|| {
        ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!("cache path has no parent: {}", path.display()),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!(
                "failed to create cache directory {}: {error}",
                parent.display()
            ),
        )
    })?;

    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(ContentCacheError::new(
                ContentCacheErrorKind::Io,
                format!(
                    "failed to remove stale temp cache file {}: {error}",
                    tmp.display()
                ),
            ));
        }
    }
    fs::write(&tmp, bytes).map_err(|error| {
        ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!("failed to write temp cache file {}: {error}", tmp.display()),
        )
    })?;
    verify_digest(digest, &read_file(&tmp)?)?;

    match fs::rename(&tmp, path) {
        Ok(()) => Ok(false),
        Err(_error) if path.exists() => {
            let existing = read_file(path)?;
            verify_digest(digest, &existing)?;
            if existing == bytes {
                let _ = fs::remove_file(&tmp);
                Ok(true)
            } else {
                Err(ContentCacheError::new(
                    ContentCacheErrorKind::DigestMismatch,
                    format!(
                        "cache path {} won a race with different bytes",
                        path.display()
                    ),
                ))
            }
        }
        Err(error) => Err(ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!(
                "failed to move temp cache file {} to {}: {error}",
                tmp.display(),
                path.display()
            ),
        )),
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, ContentCacheError> {
    fs::read(path).map_err(|error| {
        ContentCacheError::new(
            ContentCacheErrorKind::Io,
            format!("failed to read cache file {}: {error}", path.display()),
        )
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (input.len() as u64) * 8;
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk_index in 0..(message.len() / 64) {
        let chunk_start = chunk_index * 64;
        let chunk = &message[chunk_start..chunk_start + 64];
        let mut w = [0u32; 64];
        for (index, slot) in w.iter_mut().take(16).enumerate() {
            let word_start = index * 4;
            *slot = u32::from_be_bytes([
                chunk[word_start],
                chunk[word_start + 1],
                chunk[word_start + 2],
                chunk[word_start + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        for (target, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *target = target.wrapping_add(value);
        }
    }

    let mut output = [0u8; 32];
    for (index, word) in h.into_iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}
