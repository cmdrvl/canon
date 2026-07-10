#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/project/manifest.rs"]
mod project_manifest;

fuzz_target!(|data: &[u8]| {
    if data.len() > 32 * 1024 {
        return;
    }

    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    if let Ok(manifest) = project_manifest::load_project_manifest_toml(text) {
        let canonical =
            project_manifest::canonical_project_manifest_bytes(&manifest).expect("canonical bytes");
        let reparsed: project_manifest::ProjectManifest =
            serde_json::from_slice(&canonical).expect("canonical manifest json");
        let reparsed_bytes = project_manifest::canonical_project_manifest_bytes(&reparsed)
            .expect("reparsed canonical bytes");
        assert_eq!(canonical, reparsed_bytes);

        let digest = project_manifest::project_manifest_digest(&manifest).expect("digest");
        let reparsed_digest =
            project_manifest::project_manifest_digest(&reparsed).expect("reparsed digest");
        assert_eq!(digest, reparsed_digest);
    }
});
