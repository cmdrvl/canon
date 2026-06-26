use canon::{
    RefusalCode,
    entity::prepare::{
        PrepareRunRequest, PreparedExactLookupStatus, PreparedSurfaceRecord, run_prepare,
    },
};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn entity_prepare_exact_lookup_marks_known_aliases_before_blocking() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_rows(&rows);
    write_registry(
        &registry,
        "cmbs-tenants",
        "2026.06.25",
        &[("Sears", "TNT-SEARS", "tenant_label", "TENANT_ALIAS")],
    );

    let artifact = run_prepare(PrepareRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("prepare run");
    let surfaces = read_surfaces(work_dir.join(&artifact.surfaces_path));

    let sears = surface_with_raw(&surfaces, "SEARS LLC");
    assert_eq!(
        sears.exact_lookup.status,
        PreparedExactLookupStatus::Resolved
    );
    assert_eq!(
        sears.exact_lookup.canonical_id.as_deref(),
        Some("TNT-SEARS")
    );
    assert_eq!(
        sears.exact_lookup.canonical_type.as_deref(),
        Some("tenant_label")
    );
    assert_eq!(sears.exact_lookup.rule_id.as_deref(), Some("TENANT_ALIAS"));
    assert_eq!(sears.exact_lookup.matched_input.as_deref(), Some("Sears"));
    assert_eq!(
        sears.exact_lookup.lookup_inputs,
        ["SEARS LLC".to_string(), "Sears".to_string()]
    );
    assert_eq!(
        sears
            .exact_lookup
            .registry_snapshot
            .as_ref()
            .expect("registry snapshot")
            .lookup_snapshot_hash,
        artifact.registry_snapshot.lookup_snapshot_hash
    );

    let unknown = surface_with_raw(&surfaces, "Unknown Shop");
    assert_eq!(
        unknown.exact_lookup.status,
        PreparedExactLookupStatus::Unresolved
    );
    assert_eq!(artifact.summary["exact_resolved_surfaces"], 1);
    assert_eq!(artifact.summary["unresolved_surfaces"], 1);
}

#[test]
#[allow(non_snake_case)]
fn EN_P005_existing_registry_aliases_carry_registry_snapshot_hash() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows =
        Path::new("tests/fixtures/entity/regab/sec10d_baseline_public/org_mentions_selected.csv");
    let registry = temp.path().join("firms");
    let work_dir = temp.path().join("work");
    copy_registry_json_files(
        Path::new("tests/fixtures/entity/regab/sec10d_baseline_public/registry_snapshot/firms"),
        &registry,
    );

    let artifact = run_prepare(PrepareRunRequest {
        rows,
        profile: "regab_firm_identity",
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("prepare run");
    let surfaces = read_surfaces(work_dir.join(&artifact.surfaces_path));

    let resolved_count = surfaces
        .iter()
        .filter(|surface| surface.exact_lookup.status == PreparedExactLookupStatus::Resolved)
        .count() as u64;
    assert!(resolved_count > 0);
    assert_eq!(artifact.summary["exact_resolved_surfaces"], resolved_count);
    assert_eq!(
        artifact.summary["unresolved_surfaces"],
        surfaces.len() as u64 - resolved_count
    );

    let computershare = surface_with_raw(&surfaces, "Computershare");
    assert_eq!(
        computershare.exact_lookup.status,
        PreparedExactLookupStatus::Resolved
    );
    assert_eq!(
        computershare.exact_lookup.canonical_id.as_deref(),
        Some("ORG-022")
    );
    assert_eq!(
        computershare.exact_lookup.canonical_type.as_deref(),
        Some("firm")
    );
    assert_eq!(
        computershare.exact_lookup.rule_id.as_deref(),
        Some("REGAB_EXACT_ALIAS")
    );
    assert_eq!(
        computershare.exact_lookup.matched_input.as_deref(),
        Some("Computershare")
    );
    assert_eq!(
        computershare
            .exact_lookup
            .registry_snapshot
            .as_ref()
            .expect("registry snapshot")
            .lookup_snapshot_hash,
        artifact.registry_snapshot.lookup_snapshot_hash
    );
}

#[test]
fn entity_prepare_exact_lookup_does_not_resolve_from_side_surface_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    fs::write(
        &rows,
        concat!(
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
            "row-1,deal-1,loan-1,prop-1,Unknown Shop,[],\"[\"\"Sears\"\"]\"\n",
        ),
    )
    .expect("rows");
    write_registry(
        &registry,
        "cmbs-tenants",
        "2026.06.25",
        &[("Sears", "TNT-SEARS", "tenant_label", "TENANT_ALIAS")],
    );

    let artifact = run_prepare(PrepareRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("prepare run");
    let surfaces = read_surfaces(work_dir.join(&artifact.surfaces_path));

    assert_eq!(surfaces.len(), 1);
    assert_eq!(
        surfaces[0].exact_lookup.status,
        PreparedExactLookupStatus::Unresolved
    );
    assert_eq!(artifact.summary["exact_resolved_surfaces"], 0);
    assert_eq!(artifact.summary["unresolved_surfaces"], 1);
}

#[test]
fn entity_prepare_exact_lookup_refuses_conflicting_alias_targets_without_surface_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    fs::write(
        &rows,
        concat!(
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
            "row-1,deal-1,loan-1,prop-1,Sears LLC,[],[]\n",
            "row-2,deal-2,loan-2,prop-2,Sears,[],[]\n",
        ),
    )
    .expect("rows");
    write_registry(
        &registry,
        "cmbs-tenants",
        "2026.06.25",
        &[
            ("Sears", "TNT-SEARS", "tenant_label", "TENANT_ALIAS"),
            (
                "Sears LLC",
                "TNT-SEARS-ROEBUCK",
                "tenant_label",
                "TENANT_ALIAS",
            ),
        ],
    );

    let refusal = run_prepare(PrepareRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect_err("conflicting exact aliases refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityPatchConflict);
    assert!(!work_dir.join("prepare").join("surfaces.jsonl").exists());
}

fn write_cmbs_rows(path: &Path) {
    fs::write(
        path,
        concat!(
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
            "row-1,deal-1,loan-1,prop-1,SEARS LLC,\"[\"\"Sears\"\"]\",[]\n",
            "row-2,deal-1,loan-2,prop-1,Sears,[],[]\n",
            "row-3,deal-2,loan-3,prop-2,Unknown Shop,[],[]\n",
        ),
    )
    .expect("rows");
}

fn write_registry(registry: &Path, id: &str, version: &str, entries: &[(&str, &str, &str, &str)]) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "id": id,
            "version": version,
            "description": "prepare exact lookup test registry",
            "updated": "2026-06-25",
            "entry_count": entries.len()
        }))
        .expect("registry json"),
    )
    .expect("registry metadata");
    let mappings = entries
        .iter()
        .map(|(input, canonical_id, canonical_type, rule_id)| {
            serde_json::json!({
                "input": input,
                "canonical_id": canonical_id,
                "canonical_type": canonical_type,
                "rule_id": rule_id
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&mappings).expect("mappings json"),
    )
    .expect("registry mappings");
}

fn copy_registry_json_files(source: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("registry dest");
    for entry in fs::read_dir(source).expect("registry source") {
        let path = entry.expect("registry entry").path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            fs::copy(&path, dest.join(path.file_name().expect("file name")))
                .expect("copy registry json");
        }
    }
}

fn read_surfaces(path: PathBuf) -> Vec<PreparedSurfaceRecord> {
    fs::read_to_string(path)
        .expect("surfaces jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("surface row"))
        .collect()
}

fn surface_with_raw<'a>(
    surfaces: &'a [PreparedSurfaceRecord],
    raw: &str,
) -> &'a PreparedSurfaceRecord {
    surfaces
        .iter()
        .find(|surface| surface.raw_variants.iter().any(|value| value == raw))
        .expect("surface with raw variant")
}
