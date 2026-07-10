#![no_main]

use canon::registry::{canonical_package_bytes, parse_registry_package};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 128 * 1024 {
        return;
    }

    if let Ok(package) = parse_registry_package(data) {
        let canonical = canonical_package_bytes(&package).expect("canonical bytes");
        let reparsed = parse_registry_package(&canonical).expect("canonical package reparses");
        assert_eq!(package, reparsed);
    }
});
