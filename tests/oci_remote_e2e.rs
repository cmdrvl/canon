#![forbid(unsafe_code)]

use canon::distribution::{
    cache::{ContentCache, sha256_digest},
    package::{inspect_local_package, pack_local_package},
    remote::{
        OciPublishReceipt, OciPullReceipt, OciRemote, OciRemoteErrorKind, OciRemotePolicy,
        publish_package_by_immutable_digest, pull_package_by_immutable_digest,
        pull_resolved_package, resolve_tag_once,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tempfile::TempDir;
use tiny_http::{Header, Method, Response, Server, StatusCode};

#[test]
fn publish_then_pull_by_immutable_digest_materializes_external_cache() -> Result<(), Box<dyn Error>>
{
    let registry = MockOciRegistry::spawn();
    let archive = registry_archive("people", "1.0.0", "alpha")?;
    let remote = OciRemote::new(registry.base_url(), "canon/registry");
    let cache_dir = TempDir::new()?;
    let cache = ContentCache::new(cache_dir.path());

    let published =
        publish_package_by_immutable_digest(&remote, &archive, None, OciRemotePolicy::online())?;
    let pulled = pull_package_by_immutable_digest(
        &remote,
        &published.manifest_digest,
        &cache,
        OciRemotePolicy::online(),
    )?;

    assert_eq!(
        published.package_content_digest,
        pulled.package_content_digest
    );
    assert_eq!(
        published.package_archive_digest,
        pulled.package_archive_digest
    );
    assert_eq!(published.manifest_digest, pulled.manifest_digest);
    assert!(cache.blob_path(&published.manifest_digest)?.exists());
    assert!(std::path::Path::new(&pulled.package_cache_path).exists());
    assert_eq!(
        fs::read(&pulled.package_cache_path)?,
        archive,
        "cache materializes the verified package archive bytes"
    );
    assert!(pulled.verified_files >= 1);

    let state = registry.snapshot();
    assert_eq!(state.blob_puts, 2);
    assert_eq!(state.manifest_puts, 1);
    assert_eq!(state.blob_gets, 2);
    Ok(())
}

#[test]
fn tag_resolution_is_recorded_once_and_pull_does_not_follow_tag_drift() -> Result<(), Box<dyn Error>>
{
    let registry = MockOciRegistry::spawn();
    let remote = OciRemote::new(registry.base_url(), "canon/registry");
    let cache_dir = TempDir::new()?;
    let cache = ContentCache::new(cache_dir.path());

    let first = registry_archive("people", "1.0.0", "alpha")?;
    let first_publish = publish_package_by_immutable_digest(
        &remote,
        &first,
        Some("latest"),
        OciRemotePolicy::online(),
    )?;
    let resolved = resolve_tag_once(&remote, "latest", OciRemotePolicy::online())?;
    assert_eq!(resolved.manifest_digest, first_publish.manifest_digest);

    let second = registry_archive("people", "2.0.0", "beta")?;
    let second_publish = publish_package_by_immutable_digest(
        &remote,
        &second,
        Some("latest"),
        OciRemotePolicy::online(),
    )?;
    assert_ne!(
        first_publish.manifest_digest,
        second_publish.manifest_digest
    );

    let pulled = pull_resolved_package(&remote, &resolved, &cache, OciRemotePolicy::online())?;
    assert_eq!(pulled.manifest_digest, first_publish.manifest_digest);
    assert_eq!(
        pulled.resolved_from_tag.as_ref().unwrap().manifest_digest,
        first_publish.manifest_digest
    );
    assert_eq!(
        inspect_local_package(&fs::read(&pulled.package_cache_path)?)?
            .package
            .package_version,
        "1.0.0"
    );
    Ok(())
}

#[test]
fn offline_read_only_policy_fails_before_network_or_cache_writes() -> Result<(), Box<dyn Error>> {
    let registry = MockOciRegistry::spawn();
    let remote = OciRemote::new(registry.base_url(), "canon/registry");
    let cache_dir = TempDir::new()?;
    let cache = ContentCache::read_only(cache_dir.path());

    let error = pull_package_by_immutable_digest(
        &remote,
        &format!("sha256:{}", "a".repeat(64)),
        &cache,
        OciRemotePolicy::offline_read_only(),
    )
    .expect_err("offline pull must fail closed");

    assert_eq!(error.kind, OciRemoteErrorKind::NetworkDisabled);
    assert_eq!(registry.snapshot().total_requests, 0);
    assert!(!cache_dir.path().join("oci").exists());
    Ok(())
}

#[test]
fn repeated_push_and_pull_are_idempotent() -> Result<(), Box<dyn Error>> {
    let registry = MockOciRegistry::spawn();
    let archive = registry_archive("people", "1.0.0", "alpha")?;
    let remote = OciRemote::new(registry.base_url(), "canon/registry");
    let cache_dir = TempDir::new()?;
    let cache = ContentCache::new(cache_dir.path());

    let first =
        publish_package_by_immutable_digest(&remote, &archive, None, OciRemotePolicy::online())?;
    let after_first_push = registry.snapshot();
    let second =
        publish_package_by_immutable_digest(&remote, &archive, None, OciRemotePolicy::online())?;
    let after_second_push = registry.snapshot();
    assert_eq!(first.manifest_digest, second.manifest_digest);
    assert_eq!(after_first_push.blob_puts, after_second_push.blob_puts);
    assert_eq!(
        after_first_push.manifest_puts,
        after_second_push.manifest_puts
    );
    assert!(second.reused_blobs.len() >= 2);
    assert!(!second.manifest_uploaded);

    let pulled_once = pull_package_by_immutable_digest(
        &remote,
        &first.manifest_digest,
        &cache,
        OciRemotePolicy::online(),
    )?;
    let pulled_twice = pull_package_by_immutable_digest(
        &remote,
        &first.manifest_digest,
        &cache,
        OciRemotePolicy::online(),
    )?;

    assert_eq!(
        pulled_once.package_cache_path,
        pulled_twice.package_cache_path
    );
    assert_eq!(fs::read(&pulled_twice.package_cache_path)?, archive);
    Ok(())
}

#[test]
fn cli_push_and_pull_reaches_oci_transport() -> Result<(), Box<dyn Error>> {
    let registry = MockOciRegistry::spawn();
    let archive = registry_archive("people", "1.0.0", "cli")?;
    let temp = TempDir::new()?;
    let archive_path = temp.path().join("people.canonpkg");
    let cache_path = temp.path().join("cache");
    fs::write(&archive_path, &archive)?;

    let push_output = Command::new(assert_cmd::cargo::cargo_bin!("canon"))
        .args([
            "package",
            "push",
            "--archive",
            archive_path.to_str().unwrap(),
            "--registry",
            &registry.base_url(),
            "--repository",
            "canon/registry",
            "--tag",
            "cli",
            "--emit",
            "json",
        ])
        .output()?;
    assert!(
        push_output.status.success(),
        "push failed: {}",
        String::from_utf8_lossy(&push_output.stderr)
    );
    let pushed: OciPublishReceipt = serde_json::from_slice(&push_output.stdout)?;

    let pull_output = Command::new(assert_cmd::cargo::cargo_bin!("canon"))
        .args([
            "package",
            "pull",
            "--registry",
            &registry.base_url(),
            "--repository",
            "canon/registry",
            "--cache",
            cache_path.to_str().unwrap(),
            "--digest",
            &pushed.manifest_digest,
            "--emit",
            "json",
        ])
        .output()?;
    assert!(
        pull_output.status.success(),
        "pull failed: {}",
        String::from_utf8_lossy(&pull_output.stderr)
    );
    let pulled: OciPullReceipt = serde_json::from_slice(&pull_output.stdout)?;

    assert_eq!(pushed.manifest_digest, pulled.manifest_digest);
    assert_eq!(pushed.package_content_digest, pulled.package_content_digest);
    assert_eq!(fs::read(&pulled.package_cache_path)?, archive);
    assert!(registry.snapshot().total_requests > 0);
    Ok(())
}

fn registry_archive(id: &str, version: &str, payload: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("README.txt"),
        format!("{id}:{version}:{payload}"),
    )?;
    let package_bytes = package_bytes(id, version, payload);
    Ok(pack_local_package(root.path(), &package_bytes)?)
}

fn package_bytes(id: &str, version: &str, payload: &str) -> Vec<u8> {
    let mut value = json!({
        "schema_version": "canon.registry.package.v1",
        "registry": {
            "id": id,
            "version": version
        },
        "content_digest": "",
        "payload": payload,
        "licenses": ["Apache-2.0"],
        "capabilities": ["registry-package"]
    });
    let content_digest = {
        let mut digest_view = value.clone();
        digest_view["content_digest"] = Value::String(String::new());
        format!(
            "blake3:{}",
            blake3::hash(&serde_json::to_vec(&digest_view).unwrap()).to_hex()
        )
    };
    value["content_digest"] = Value::String(content_digest);
    serde_json::to_vec(&value).unwrap()
}

#[derive(Debug, Clone, Default)]
struct MockState {
    blobs: BTreeMap<String, Vec<u8>>,
    manifests: BTreeMap<String, Vec<u8>>,
    tags: BTreeMap<String, String>,
    total_requests: usize,
    blob_gets: usize,
    blob_puts: usize,
    manifest_gets: usize,
    manifest_puts: usize,
}

struct MockOciRegistry {
    base_url: String,
    state: Arc<Mutex<MockState>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockOciRegistry {
    fn spawn() -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        let state = Arc::new(Mutex::new(MockState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match server.recv_timeout(Duration::from_millis(50)) {
                    Ok(Some(request)) => handle_request(request, &thread_state),
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url,
            state,
            stop,
            thread: Some(thread),
        }
    }

    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    fn snapshot(&self) -> MockState {
        self.state.lock().unwrap().clone()
    }
}

impl Drop for MockOciRegistry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn handle_request(request: tiny_http::Request, state: &Arc<Mutex<MockState>>) {
    let method = request.method().clone();
    let url = request.url().to_string();
    state.lock().unwrap().total_requests += 1;

    if url.contains("/blobs/uploads/") {
        handle_upload(method, url, request, state);
    } else if url.contains("/blobs/") {
        handle_blob(method, url, request, state);
    } else if url.contains("/manifests/") {
        handle_manifest(method, url, request, state);
    } else {
        let _ = request.respond(Response::empty(StatusCode(404)));
    }
}

fn handle_upload(
    method: Method,
    url: String,
    mut request: tiny_http::Request,
    state: &Arc<Mutex<MockState>>,
) {
    if method == Method::Post {
        let location = format!("{}upload-1", url.split('?').next().unwrap_or(&url));
        let _ = request
            .respond(Response::empty(StatusCode(202)).with_header(header("Location", &location)));
        return;
    }
    if method == Method::Put {
        let Some(digest) = query_value(&url, "digest") else {
            let _ = request.respond(Response::empty(StatusCode(400)));
            return;
        };
        let mut bytes = Vec::new();
        request.as_reader().read_to_end(&mut bytes).unwrap();
        if sha256_digest(&bytes) != digest {
            let _ = request.respond(Response::empty(StatusCode(400)));
            return;
        }
        let mut state = state.lock().unwrap();
        state.blob_puts += 1;
        state.blobs.insert(digest.clone(), bytes);
        let _ = request.respond(
            Response::empty(StatusCode(201)).with_header(header("Docker-Content-Digest", &digest)),
        );
        return;
    }
    let _ = request.respond(Response::empty(StatusCode(405)));
}

fn handle_blob(
    method: Method,
    url: String,
    request: tiny_http::Request,
    state: &Arc<Mutex<MockState>>,
) {
    let digest = last_path_segment(&url);
    let blob = state.lock().unwrap().blobs.get(&digest).cloned();
    match (method, blob) {
        (Method::Head, Some(_)) => {
            let _ = request.respond(
                Response::empty(StatusCode(200))
                    .with_header(header("Docker-Content-Digest", &digest)),
            );
        }
        (Method::Head, None) => {
            let _ = request.respond(Response::empty(StatusCode(404)));
        }
        (Method::Get, Some(bytes)) => {
            state.lock().unwrap().blob_gets += 1;
            let _ = request.respond(
                Response::from_data(bytes).with_header(header("Docker-Content-Digest", &digest)),
            );
        }
        (Method::Get, None) => {
            let _ = request.respond(Response::empty(StatusCode(404)));
        }
        _ => {
            let _ = request.respond(Response::empty(StatusCode(405)));
        }
    }
}

fn handle_manifest(
    method: Method,
    url: String,
    mut request: tiny_http::Request,
    state: &Arc<Mutex<MockState>>,
) {
    let reference = last_path_segment(&url);
    match method {
        Method::Put => {
            let mut bytes = Vec::new();
            request.as_reader().read_to_end(&mut bytes).unwrap();
            let digest = sha256_digest(&bytes);
            let mut state = state.lock().unwrap();
            state.manifest_puts += 1;
            state.manifests.insert(digest.clone(), bytes);
            if reference != digest {
                state.tags.insert(reference, digest.clone());
            }
            let _ = request.respond(
                Response::empty(StatusCode(201))
                    .with_header(header("Docker-Content-Digest", &digest)),
            );
        }
        Method::Head | Method::Get => {
            let resolved = {
                let state = state.lock().unwrap();
                if reference.starts_with("sha256:") {
                    Some(reference.clone())
                } else {
                    state.tags.get(&reference).cloned()
                }
            };
            let Some(digest) = resolved else {
                let _ = request.respond(Response::empty(StatusCode(404)));
                return;
            };
            let manifest = state.lock().unwrap().manifests.get(&digest).cloned();
            let Some(manifest) = manifest else {
                let _ = request.respond(Response::empty(StatusCode(404)));
                return;
            };
            if method == Method::Get {
                state.lock().unwrap().manifest_gets += 1;
                let _ = request.respond(
                    Response::from_data(manifest)
                        .with_header(header("Docker-Content-Digest", &digest)),
                );
            } else {
                let _ = request.respond(
                    Response::empty(StatusCode(200))
                        .with_header(header("Docker-Content-Digest", &digest)),
                );
            }
        }
        _ => {
            let _ = request.respond(Response::empty(StatusCode(405)));
        }
    }
}

fn last_path_segment(url: &str) -> String {
    url.split('?')
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap()
        .to_string()
}

fn query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for part in query.split('&') {
        let (candidate, value) = part.split_once('=')?;
        if candidate == key {
            return Some(value.to_string());
        }
    }
    None
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}
