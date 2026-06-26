use serde_json::Value;
use std::{fs, path::Path};

fn manifest() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/cmbs/tenant_sample_benchmark_manifest.json");
    serde_json::from_str(&fs::read_to_string(path).expect("CMBS benchmark manifest opens"))
        .expect("CMBS benchmark manifest parses")
}

#[test]
fn cmbs_tenant_sample_benchmark_manifest_pins_sears_and_hard_negatives() {
    let manifest = manifest();

    assert_eq!(
        manifest["version"],
        "canon_entity_cmbs_tenant_benchmark_manifest.v0"
    );
    assert_eq!(manifest["source"]["data_rows"], 6000);
    assert_eq!(manifest["source"]["tenant_observations"], 10143);
    assert_eq!(manifest["source"]["unique_raw_tenant_names"], 431);

    let clusters = manifest["must_link_clusters"]
        .as_array()
        .expect("must_link_clusters array");
    let sears = clusters
        .iter()
        .find(|cluster| cluster["id"] == "TNT-SEARS")
        .expect("Sears cluster present");
    assert_eq!(sears["label"], "Sears");
    assert_eq!(sears["observations"], 4);
    assert_eq!(
        sears["variants"]
            .as_array()
            .expect("sears variants")
            .iter()
            .map(|value| value.as_str().expect("sears variant"))
            .collect::<Vec<_>>(),
        ["SEARS LLC", "Sears", "Sears Roebuck & Co.", "Sears #1234"]
    );

    let hard_negative_pairs = manifest["hard_negative_pairs"]
        .as_array()
        .expect("hard_negative_pairs array")
        .iter()
        .map(|pair| {
            let pair = pair.as_array().expect("pair array");
            (
                pair[0].as_str().expect("left"),
                pair[1].as_str().expect("right"),
            )
        })
        .collect::<Vec<_>>();

    for expected in [
        ("Sears", "Sears Auto Center"),
        ("Sears", "Kmart"),
        ("Sears", "Transform SR LLC"),
        ("Sears", "Sears Holdings"),
    ] {
        assert!(
            hard_negative_pairs.contains(&expected),
            "missing hard-negative pair {expected:?}"
        );
    }
}
