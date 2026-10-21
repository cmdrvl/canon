#![forbid(unsafe_code)]

//! Installed, offline instructions. This envelope is deliberately separate from
//! the content-addressed capabilities used as a semantic planning input.

use serde_json::{Value, json};
use std::{collections::BTreeMap, error::Error};

macro_rules! schemas {
    ($($name:literal),+ $(,)?) => {
        [$(($name, include_str!(concat!("../../schemas/", $name)))),+]
    };
}

const SCHEMAS: &[(&str, &str)] = &schemas![
    "canon.geo.acquisition_request.v0.schema.json",
    "canon.geo.capabilities.v0.schema.json",
    "canon.geo.composition_request.v0.schema.json",
    "canon.geo.descriptive_asset_request.v0.schema.json",
    "canon.geo.evidence_request.v0.schema.json",
    "canon.geo.home_cell_rows.v1.schema.json",
    "canon.geo.next_evidence_inputs.v0.schema.json",
    "canon.geo.next_evidence_request.v0.schema.json",
    "canon.geo.question.v0.schema.json",
    "canon.geo.regional_inventory.v1.schema.json",
    "canon.geo.resource_budget.v0.schema.json",
    "canon.geo.separation_inputs.v0.schema.json",
    "canon.geo.separation_request.v0.schema.json",
    "canon.geo.tile_work_request.v1.schema.json",
    "canon.geo.warehouse_rows.v0.schema.json",
];

/// Build instructions using only compiled source schemas and deterministic data.
pub fn agent_guide() -> Result<Value, Box<dyn Error>> {
    let documents: BTreeMap<_, Value> = SCHEMAS
        .iter()
        .map(|(name, bytes)| Ok((*name, serde_json::from_str(bytes)?)))
        .collect::<Result<_, serde_json::Error>>()?;
    // Keep the handbook as data rather than a large recursive json! expansion.
    // Only the compiled schemas and executable example need dynamic assembly.
    let mut guide: Value = serde_json::from_str(
        r#"{
        "version": "canon_geo_agent_guide.v0",
        "scope": "Offline evidence-backed geographic association. Acquisition and source interpretation happen outside Canon; normal registry lookup remains exact.",
        "tasks": {
            "discover_from_clues": {
                "status": "experimental_bounded_descriptive_intake",
                "command": "canon geo plan --question question.json --capabilities capabilities.json --inventory inventory.json --profile profile.json --budget budget.json",
                "required_local_inputs": ["bounded regional candidate inventory", "retained claim and candidate source records", "explicit profile and budget"],
                "outputs": ["plan", "bounded candidate/evidence artifacts", "residual and explanation"],
                "supported_attributes": ["property_name", "address", "property_type", "unit_count", "year_built"],
                "missing": "A facts-only inquiry is not implemented: do not use --inquiry. This profile does not derive an area from prose or acquire datasets. bd-2s32/bd-3s20 own the complete discovery journey."
            },
            "check_supplied_address": {
                "status": "limited",
                "command": "canon geo capabilities --guide",
                "required_local_inputs": ["address assertion", "regional candidates", "retained source evidence"],
                "outputs": ["descriptive soft address agreement, not inferred building membership"],
                "missing": "Source-neutral address-to-building inquiry remains bd-3mft/bd-33hh. The separate native membership bridge is NYC/PAD-specific. Geocoder success is not Canon identity proof."
            },
            "add_evidence_resume": {
                "status": "implemented_explicit_bindings",
                "command": "canon geo run --plan plan.json --work-dir work --input NODE_ID:BINDING_ID=updated.json [other declared bindings]",
                "required_local_inputs": ["same validated plan", "all required external bindings", "updated release-bound evidence"],
                "outputs": ["new run receipt", "recomputed affected artifacts", "reused valid receipts"],
                "missing": "Read node and binding IDs from the generated plan. Reuse is validated, not guaranteed across source releases. For acquired inventory changes use geo replan-from-acquisition."
            },
            "inspect_why": {
                "status": "implemented_stored_artifacts",
                "command": "canon geo inspect --run work --recommend-next",
                "required_local_inputs": ["stored run workspace"],
                "outputs": ["artifact handles", "residual state", "stored next-evidence state"],
                "missing": "Current-answer projection and the complete one-call agent answer remain bd-1uo5/bd-1g18. Read output_refs and receipt paths; do not guess filenames."
            }
        },
        "input_contracts": {
            "question": {"schema": "canon.geo.question.v0.schema.json", "meaning": "Question, bounded geography, grain and as-of date; subject labels are assertions, not evidence constraints."},
            "capabilities": {"schema": "canon.geo.capabilities.v0.schema.json", "command": "canon geo capabilities > capabilities.json", "meaning": "Use default capabilities, never this guide envelope, as the planning input."},
            "inventory": {"schema": "canon.geo.regional_inventory.v1.schema.json", "meaning": "Available source releases, coverage and local acquisition state; availability is not truth reach."},
            "profile": {"schema": "canon.geo.composition_request.v0.schema.json", "pointer": "/$defs/composition_profile"},
            "budget": {"schema": "canon.geo.resource_budget.v0.schema.json"},
            "descriptive": {"schema": "canon.geo.descriptive_asset_request.v0.schema.json", "binding_example": "geo.building.materialize_evidence:rows", "meaning": "Claim attributes and source-backed candidate attributes. Supports one parcel or one building, not campus extent. Run executor accepts this family; the warehouse-only materialize-evidence leaf does not."},
            "warehouse": {"schema": "canon.geo.warehouse_rows.v0.schema.json", "meaning": "Alternative materialize_evidence:rows family. Candidate incidence and typed observations, not arbitrary article prose."},
            "source_reference": {"schema": "canon.geo.evidence_request.v0.schema.json", "pointer": "/$defs/evidence_record_ref", "meaning": "Record identifier, vintage and raw record BLAKE3; retain actual source bytes externally. Repeated copies do not establish independent support."},
            "rho_contract": {"schema": "canon.geo.evidence_request.v0.schema.json", "pointer": "/$defs/rho_contract", "meaning": "Versioned observation semantics and admission authority. Uncalibrated source claims must remain diagnostic or soft; source count is not confidence."},
            "home_cells": {"schema": "canon.geo.home_cell_rows.v1.schema.json", "meaning": "Source-bound representative points for blocking, not geometric truth."},
            "tile_work": {"schema": "canon.geo.tile_work_request.v1.schema.json", "meaning": "Bounded candidate section plus controlled halo."},
            "separation": {"schema": "canon.geo.separation_inputs.v0.schema.json", "meaning": "Prospective outcomes, not observations already acquired."},
            "next_evidence": {"schema": "canon.geo.next_evidence_inputs.v0.schema.json", "meaning": "Prospective action candidates and cost budget, not acceptance authority."}
        },
        "schema_documents": {},
        "schema_resolution": "Keys are schema basenames. Resolve local $ref document names within schema_documents and fragments as JSON pointers. $id URLs identify schemas; no network lookup or repository checkout is required.",
        "examples": [],
        "limitations": [
            {"feature": "area, acreage, square feet", "status": "unavailable_in_descriptive_v0", "owner": "bd-gixj", "next_step": "Retain source value, units and footprint-versus-floor-area grain; do not put square feet in unit_count."},
            {"feature": "stories and near-road/relative-location clues", "status": "unavailable_in_descriptive_v0", "owner": "bd-gixj", "next_step": "Retain raw clue and bounded search uncertainty; collect candidate geometry and source-backed measurements externally."},
            {"feature": "complete agent inquiry", "status": "unfinished", "owner": "bd-pd2y", "next_step": "Use supported typed inputs; this guide does not complete the fresh-agent discovery acceptance."},
            {"feature": "truth and acceptance", "status": "not_established_by_example", "next_step": "Evaluate independent truth reach, grain and calibrated acceptance separately from exact representation-relative solving."}
        ],
        "recovery": {
            "wrong_input_family": "Use each input_contracts entry and the generated plan's bindings. Existing E_ENTITY_ARTIFACT_CONTRACT details identify geo_control_error_code, geo_plan_error_code or geo_run_error_code as applicable.",
            "unsupported_flag": "Unknown flags such as --inquiry remain CLI errors; start with canon geo capabilities --guide.",
            "release_mismatch": "Use the same source release/digest binding in home-cell rows, section features and descriptive inventory_source; acquire and replan for an actual release change. Never edit a digest to disguise changed bytes."
        },
        "next_command": "canon geo capabilities > capabilities.json"
    }"#,
    )?;
    guide["schema_documents"] = serde_json::to_value(documents)?;
    guide["examples"] = Value::Array(vec![synthetic_example()?]);
    Ok(guide)
}

fn bound(counter: &str, value: u64, unit: &str) -> Value {
    json!({"semantic_id": format!("guide.synthetic.{counter}"), "counter": counter, "value": value, "unit": unit, "origin": "caller_declared", "action": "report_budget_fallback"})
}

fn synthetic_example() -> Result<Value, Box<dyn Error>> {
    let region = json!({"geography_id": "guide.synthetic.region", "geography_kind": "bounded_fixture", "description": "Synthetic two-building inventory, not live data"});
    // A separate raw candidate inventory avoids a self-referential hash: the
    // descriptive request subsequently binds this inventory's semantic hash.
    let inventory_rows = json!({
        "version": "canon_geo_warehouse_rows.v0",
        "profile": {"version": "canon_geo_composition_profile.v0", "selection_level": "building"},
        "parcel_rows": [],
        "building_parcel_rows": [{"building_id": "building-a", "parcel_id": null}, {"building_id": "building-b", "parcel_id": null}],
        "contracts": [], "evidence_rows": [], "max_assignments": 128, "max_materialized_models": 64
    });
    let inventory_bytes = serde_json::to_vec(&inventory_rows)?;
    let inventory_digest = format!("blake3:{}", blake3::hash(&inventory_bytes).to_hex());
    let release = json!({"release_id": "guide.synthetic.v1", "release_digest": inventory_digest});
    let scope = json!({"kind": "native_entity", "entity_level": "building", "identity_participation": "stable_alias"});
    let budget = json!({
        "version": "canon_geo_resource_budget.v0", "budget_id": "guide.synthetic.budget",
        "deterministic_bounds": [bound("bytes", 1_000_000, "byte"), bound("rows", 100, "row"), bound("cells", 64, "cell"), bound("candidates", 16, "candidate"), bound("variables", 16, "variable"), bound("states", 10_000, "state"), bound("models", 64, "model"), bound("operations", 1_000_000, "operation")],
        "telemetry": []
    });
    let inventory = json!({
        "version": "canon_geo_regional_inventory.v1", "inventory_id": "guide.synthetic.inventory", "region": region,
        "sources": [{
            "source_instance_id": "guide.synthetic.buildings", "release": release, "temporal_scope": {},
            "lineage_ids": ["guide.synthetic.lineage"], "native_scope": scope,
            "evidence_classes": ["building_footprint", "asserted_attribute"],
            "coverage": {"coverage_id": "guide.synthetic.coverage", "region": region, "predicate": "two declared synthetic records; empirical truth reach unverified"},
            "local_state": {"state": "available", "local_ref": {"artifact_id": "guide.synthetic.inventory-rows", "contract_version": "canon_geo_warehouse_rows.v0", "content_hash": inventory_digest, "media_type": "application/json"}},
            "license_class": "public_redistributable", "egress_class": "shareable", "estimates": []
        }], "discovery_gaps": []
    });
    let typed_inventory = serde_json::from_value(inventory.clone())?;
    let source = json!({
        "source_instance_id": "guide.synthetic.buildings", "release": release, "native_scope": scope,
        "inventory_ref": {"inventory_id": "guide.synthetic.inventory", "semantic_hash": super::control::regional_inventory_semantic_hash(&typed_inventory)?, "planning_hash": super::control::regional_inventory_planning_hash(&typed_inventory)?}
    });
    let record = |id: &str| json!({"source_record_id": id, "source_vintage": "guide.synthetic.v1", "record_blake3": blake3::hash(id.as_bytes()).to_hex().to_string()});
    let channels: Vec<_> = ["property_name", "address", "property_type", "unit_count", "year_built"].into_iter().map(|channel| json!({
        "channel": channel, "tolerance": 0,
        "contract": {"id": format!("guide.synthetic.{channel}"), "version": "1", "source_dataset": "guide.synthetic.buildings", "source_release": "guide.synthetic.v1", "source_lineage_ids": ["guide.synthetic.lineage"], "method_id": "synthetic-attribute-comparison-not-calibrated", "method_version": "1", "claim_role": "attribute_observation", "basis": {"kind": "uncalibrated", "reason": "Synthetic parser/workflow demonstration, not an empirical error model", "admission_policy": {"kind": "soft_with_weight", "cost_if_absent": 1}}}
    })).collect();
    let descriptive = json!({
        "version": "canon_geo_descriptive_asset_request.v0",
        "profile": {"profile_id": "descriptive_asset_single_member_v0", "selection_level": "building", "channels": channels},
        "bounded_geography": region, "inventory_source": source,
        "claim": {"claim_id": "guide.synthetic.claim", "as_of": "2026-08-31", "attributes": {"property_type": "office"}, "source_records": [record("guide.synthetic.claim.v1")]},
        "candidates": [
            {"id": "building-a", "attributes": {"property_name": "Example North", "address": "10 North Example Street", "property_type": "office"}, "source_records": [record("guide.synthetic.building-a")]},
            {"id": "building-b", "attributes": {"property_name": "Example South", "address": "10 South Example Street", "property_type": null}, "source_records": [record("guide.synthetic.building-b")]}
        ], "max_candidates": 16, "max_assignments": 128, "max_materialized_models": 64
    });
    let mut updated = descriptive.clone();
    updated["claim"]["attributes"]["property_name"] = json!("Example North");
    updated["claim"]["source_records"] = json!([record("guide.synthetic.claim.v2")]);
    let ids = ["building-a", "building-b"];
    let home_rows: Vec<_> = ids.iter().map(|id| json!({
        "source": source, "feature_id": id, "source_record_id": format!("guide.synthetic.{id}"),
        "geometry_sha256": "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed",
        "representative_point_method": "synthetic_centroid_not_live",
        "longitude": "-73.977264000", "latitude": "40.753429000", "transform_execution_id": "guide.synthetic.transform", "transform_definition_id": "guide.synthetic.definition", "claimed_home_cell": "892a100d62bffff"
    })).collect();
    let features: Vec<_> = ids
        .iter()
        .map(|id| json!({"source": source, "feature_id": id, "home_cell": "892a100d62bffff"}))
        .collect();
    let capabilities: Value = serde_json::from_slice(
        &super::control::canonical_capabilities_bytes(&super::control::default_geo_capabilities()?)?,
    )?;
    let files = json!({
            "question.json": {"version": "canon_geo_question.v0", "question_id": "guide.synthetic.question", "subject_bindings": [{"role": "target", "binding_class": "operator_label", "value": "An office in the synthetic bounded region"}], "bounded_geography": region, "requested_grains": [{"entity_level": "building", "required_evidence_classes": ["building_footprint"], "optional_evidence_classes": ["asserted_attribute"]}], "query_as_of": {"utc_day": "2026-08-31", "semantic_id": "guide.synthetic.as_of", "unit": "utc_day", "origin": "caller_declared"}, "requested_claim_classes": ["candidate_reach", "stable_identity"], "presentation_limits": [bound("models", 16, "model"), bound("candidates", 16, "candidate")], "abstention_policy": {"unsupported_grain": "report_unsupported", "unresolved_residual": "report_residual", "budget_fallback": "report_residual"}, "decision_policy": null, "resource_budget_ref": "guide.synthetic.budget"},
            "capabilities.json": capabilities,
            "inventory.json": inventory,
            "inventory-rows.json": inventory_rows,
            "profile.json": {"version": "canon_geo_composition_profile.v0", "selection_level": "building"},
            "budget.json": budget,
            "home-cells.json": {"version": "canon_geo_home_cell_rows.v1", "coordinate_crs": "EPSG:4326", "coordinate_decimal_places": 9, "h3_resolution": 9, "stability_radius_fixed": 1000, "rows": home_rows, "max_rows": 16},
            "tile-work.json": {"version": "canon_geo_tile_work_request.v1", "center_cell": "892a100d62bffff", "halo_k": 1, "features": features, "max_features": 16, "max_work_cells": 7},
            "descriptive.json": descriptive,
            "updated-descriptive.json": updated,
            "separation.json": {"version": "canon_geo_separation_inputs.v0", "prospective": [{"id": "guide.synthetic.prospective", "contract_id": "guide.synthetic.prospective", "cost_units": 1, "outcomes": [{"outcome_id": "a", "induced": [{"kind": "require", "member": {"level": "building", "id": "building-a"}}]}, {"outcome_id": "b", "induced": [{"kind": "forbid", "member": {"level": "building", "id": "building-a"}}]}]}]},
            "next-evidence.json": {"version": "canon_geo_next_evidence_inputs.v0", "candidates": [{"action_id": "guide.synthetic.action", "class": "separate_residual", "kind": {"kind": "observe", "payload": "guide.synthetic.prospective"}, "observation_id": "guide.synthetic.prospective", "cost_units": 1}], "budget": budget}
    });
    let mut example: Value = serde_json::from_str(
        r#"{
        "id": "synthetic_descriptive_not_live",
        "proof_class": "synthetic_contract_demonstration",
        "warning": "Not a real property, calibrated identity result or complete campus. The single-member cardinality is a declared question profile; soft clues do not establish truth. Two candidates remain feasible. Prospective actions below are tutorial inputs, not collected evidence.",
        "create_directories": ["work"],
        "file_encoding": "Serialize write_files values as compact UTF-8 JSON with no trailing newline. inventory-rows.json is retained source data and its exact bytes are pinned by inventory.json local_ref.content_hash and source release_digest.",
        "write_files": {},
        "steps": [
            {"argv": ["geo", "plan", "--question", "question.json", "--capabilities", "capabilities.json", "--inventory", "inventory.json", "--profile", "profile.json", "--budget", "budget.json"], "stdout_file": "plan.json"},
            {"argv": ["geo", "run", "--plan", "plan.json", "--work-dir", "work", "--input", "geo.building.home_cells:rows=home-cells.json", "--input", "geo.building.section:request=tile-work.json", "--input", "geo.building.materialize_evidence:rows=descriptive.json", "--input", "geo.building.separation:request=separation.json", "--input", "geo.building.next_evidence:request=next-evidence.json"], "stdout_file": "run.json"},
            {"argv": ["geo", "inspect", "--run", "work", "--recommend-next"], "stdout_file": "inspection.json"},
            {"argv": ["geo", "run", "--plan", "plan.json", "--work-dir", "work", "--input", "geo.building.home_cells:rows=home-cells.json", "--input", "geo.building.section:request=tile-work.json", "--input", "geo.building.materialize_evidence:rows=updated-descriptive.json", "--input", "geo.building.separation:request=separation.json", "--input", "geo.building.next_evidence:request=next-evidence.json"], "stdout_file": "updated-run.json"},
            {"argv": ["geo", "inspect", "--run", "work", "--recommend-next"], "stdout_file": "updated-inspection.json"}
        ],
        "execution": "Write each write_files value using file_encoding under its filename in one empty directory. Create create_directories first (mkdir work). For each step, invoke canon with argv in that directory, retain stdout as stdout_file and check exit status and domain status before the next step. Runs finish all nine stages with ABSTAINED because two candidates remain feasible; ABSTAINED is a legitimate result, not a failed executor. Read output_refs and bound receipt paths for candidate/evidence/solve artifacts.",
        "expected": {"run_status": "ABSTAINED", "executed_stages": 9, "candidate_count": 2, "residual_model_count": 2, "hard_forced_buildings": [], "added_evidence": "one additional soft name preference for building-a; no candidate eliminated"}
    }"#,
    )?;
    example["write_files"] = files;
    Ok(example)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_guide_embeds_source_schema_closure_and_valid_entry_pointers() {
        let guide = agent_guide().unwrap();
        let docs = &guide["schema_documents"];
        fn refs(value: &Value, name: &str, docs: &Value) {
            match value {
                Value::Object(map) => {
                    if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                        let (document, fragment) =
                            reference.split_once('#').unwrap_or((reference, ""));
                        let target = &docs[if document.is_empty() { name } else { document }];
                        assert!(
                            target.is_object(),
                            "missing local document {reference} in {name}"
                        );
                        assert!(
                            fragment.is_empty() || target.pointer(fragment).is_some(),
                            "unresolved {reference} in {name}"
                        );
                    }
                    for child in map.values() {
                        refs(child, name, docs);
                    }
                }
                Value::Array(items) => {
                    for child in items {
                        refs(child, name, docs);
                    }
                }
                _ => {}
            }
        }
        for (name, source) in SCHEMAS {
            assert_eq!(docs[*name], serde_json::from_str::<Value>(source).unwrap());
            refs(&docs[*name], name, docs);
        }
        for entry in guide["input_contracts"].as_object().unwrap().values() {
            let doc = &docs[entry["schema"].as_str().unwrap()];
            assert!(doc.is_object());
            if let Some(pointer) = entry["pointer"].as_str() {
                assert!(doc.pointer(pointer).is_some(), "missing entry {pointer}");
            }
        }
    }
}
