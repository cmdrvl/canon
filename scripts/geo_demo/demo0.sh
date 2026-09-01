#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s [--work-dir DIR]\n' "${0##*/}" >&2
  printf 'Runs Demo 0 over retained fixture input using public canon geo CLI commands.\n' >&2
}

work_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      if [[ $# -lt 2 ]]; then
        usage
        exit 64
      fi
      work_dir="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 64
      ;;
  esac
done

if ! command -v jq >/dev/null 2>&1; then
  printf 'demo0 requires jq on PATH for deterministic JSON assembly\n' >&2
  exit 69
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"

if [[ -n "${CANON_BIN:-}" ]]; then
  canon_bin="$CANON_BIN"
else
  cargo_target_dir="$(cargo metadata --quiet --no-deps --format-version 1 --manifest-path "$repo_root/Cargo.toml" | jq -r '.target_directory')"
  if [[ -z "$cargo_target_dir" || "$cargo_target_dir" == "null" ]]; then
    printf 'demo0 could not resolve cargo target directory for %s\n' "$repo_root/Cargo.toml" >&2
    exit 70
  fi
  cargo build --quiet --manifest-path "$repo_root/Cargo.toml" --bin canon
  canon_bin="$cargo_target_dir/debug/canon"
  if [[ ! -x "$canon_bin" ]]; then
    printf 'demo0 cargo build did not produce executable canon binary at %s\n' "$canon_bin" >&2
    exit 70
  fi
fi

if [[ -z "$work_dir" ]]; then
  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/canon-geo-demo0.XXXXXX")"
else
  mkdir -p "$work_dir"
fi

canon_cli() {
  "$canon_bin" "$@"
}

run_json() {
  local out="$1"
  shift
  canon_cli "$@" > "$out"
  jq -e . "$out" >/dev/null
}

case4_rows="$work_dir/case4-warehouse-rows.json"
evidence_request="$work_dir/evidence-request.json"
evidence_compilation="$work_dir/evidence-compilation.json"
solve_artifact="$work_dir/solve.json"
population_request="$work_dir/population.json"
evaluation_artifact="$work_dir/evaluation.json"
capabilities_artifact="$work_dir/capabilities.json"
plan_question="$work_dir/question.json"
plan_inventory="$work_dir/inventory.json"
plan_profile="$work_dir/profile.json"
plan_budget="$work_dir/budget.json"
plan_artifact="$work_dir/plan.json"
run_home_cell_rows="$work_dir/run-home-cell-rows.json"
run_tile_request="$work_dir/run-tile-request.json"
run_artifact="$work_dir/run.json"
tile_discovery_request="$work_dir/tile-discovery-request.json"
tile_discovery_artifact="$work_dir/tile-discovery.json"
tile_owner_request="$work_dir/tile-owner-request.json"
tile_observer_request="$work_dir/tile-observer-request.json"
tile_owner_artifact="$work_dir/tile-owner.json"
tile_observer_artifact="$work_dir/tile-observer.json"
reconciliation_request="$work_dir/tile-reconciliation-request.json"
reconciliation_artifact="$work_dir/tile-reconciliation.json"
negative_request="$work_dir/negative-hard-chimera-composition.json"
negative_artifact="$work_dir/negative-hard-chimera.json"

cat > "$case4_rows" <<'JSON'
{
  "version": "canon_geo_warehouse_rows.v0",
  "profile": {
    "version": "canon_geo_composition_profile.v0",
    "selection_level": "parcel"
  },
  "parcel_rows": [
    { "parcel_id": "1004540041" },
    { "parcel_id": "1004540042" },
    { "parcel_id": "1004540043" },
    { "parcel_id": "1004540044" },
    { "parcel_id": "1004540045" },
    { "parcel_id": "1004540046" },
    { "parcel_id": "1004540047" }
  ],
  "building_parcel_rows": [
    { "building_id": "1006494", "parcel_id": "1004540041" },
    { "building_id": "1006495", "parcel_id": "1004540042" },
    { "building_id": "1006496", "parcel_id": "1004540043" },
    { "building_id": "1006497", "parcel_id": "1004540044" },
    { "building_id": "1006498", "parcel_id": "1004540045" },
    { "building_id": "1006499", "parcel_id": "1004540046" },
    { "building_id": "1006500", "parcel_id": "1004540047" }
  ],
  "contracts": [
    {
      "id": "rho.case4.address-core",
      "version": "1.0.0",
      "source_dataset": "fixture.case4.mappluto_address_members",
      "source_release": "26v1/2026-05-01",
      "source_lineage_ids": ["EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT:26v1"],
      "method_id": "retained_case4_asserted_member_address_probe",
      "method_version": "1.0.0",
      "claim_role": "stable_identity_anchor",
      "basis": {
        "kind": "logical_relaxation",
        "invariant_id": "case4_fixture_asserted_members_are_candidate_superset"
      }
    },
    {
      "id": "rho.case4.area-majority-buildings",
      "version": "1.0.0",
      "source_dataset": "fixture.case4.nyc_building_footprints",
      "source_release": "2026-08-09",
      "source_lineage_ids": ["EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT:2026-08-09"],
      "method_id": "intersection_area_over_computed_footprint_area_gt_0_5",
      "method_version": "1.0.0",
      "claim_role": "stable_identity_anchor",
      "basis": {
        "kind": "logical_relaxation",
        "invariant_id": "case4_fixture_majority_buildings_are_candidate_superset"
      }
    },
    {
      "id": "rho.case4.fixture-address-probe",
      "version": "1.0.0",
      "source_dataset": "fixture.case4.fixture_address_probe",
      "source_release": "26v1/2026-05-01",
      "source_lineage_ids": ["EDGAR_DB.SOURCE.NYC_DCP_MAPPLUTO_HOT:26v1"],
      "method_id": "fixture_address_probe_zero_match",
      "method_version": "1.0.0",
      "claim_role": "attribute_observation",
      "basis": {
        "kind": "empirical_calibration",
        "population_id": "case4_fixture_address_probe",
        "calibration_blake3": "97e7e532ba98fb5ce35769f30b61b738d906c6686f17c7d8bbbf61bf3f8b910c",
        "falsification_rule_id": "do_not_admit_fixture_address_probe_as_membership"
      }
    }
  ],
  "evidence_rows": [
    {
      "observation_id": "obs.case4.address-core",
      "contract_id": "rho.case4.address-core",
      "source_record": {
        "source_record_id": "case4.asserted_members.mappluto_26v1",
        "source_vintage": "26v1/2026-05-01",
        "record_blake3": "8c7db293f7195e1a3c4d397c2bcf2f59a4fd289f9a302b295669bcccb938d333"
      },
      "observation": {
        "kind": "exact_sets",
        "level": "parcel",
        "sets": [[
          "1004540041",
          "1004540042",
          "1004540043",
          "1004540044",
          "1004540045",
          "1004540046"
        ]]
      }
    },
    {
      "observation_id": "obs.case4.area-majority-buildings",
      "contract_id": "rho.case4.area-majority-buildings",
      "source_record": {
        "source_record_id": "case4.nyc_footprints.majority_2026_08_09",
        "source_vintage": "2026-08-09",
        "record_blake3": "6ee7136102b255723487ec7a5d9f0a8ac0efc6fdf1972830c25eda91072ee151"
      },
      "observation": {
        "kind": "exact_sets",
        "level": "building",
        "sets": [[
          "1006494",
          "1006495",
          "1006496",
          "1006497",
          "1006498",
          "1006499"
        ]]
      }
    },
    {
      "observation_id": "obs.case4.fixture-address-probe-199-e-12-street",
      "contract_id": "rho.case4.fixture-address-probe",
      "source_record": {
        "source_record_id": "case4.fixture_address_probe.199_east_12_street.zero_match",
        "source_vintage": "26v1/2026-05-01",
        "record_blake3": "c54e12755a1240376324a828921506c5090b4d653d67dad129713a1856f766cc"
      },
      "observation": {
        "kind": "exact_sets",
        "level": "parcel",
        "sets": [[]]
      }
    }
  ],
  "max_assignments": 20000,
  "max_materialized_models": 16
}
JSON

cat > "$negative_request" <<'JSON'
{
  "version": "canon_geo_composition_request.v0",
  "profile": {
    "version": "canon_geo_composition_profile.v0",
    "selection_level": "parcel"
  },
  "universe": {
    "parcels": [
      "1004540041",
      "1004540042",
      "1004540043",
      "1004540044",
      "1004540045",
      "1004540046",
      "1004540047"
    ],
    "buildings": [
      { "id": "1006494", "parcel_ids": ["1004540041"] },
      { "id": "1006495", "parcel_ids": ["1004540042"] },
      { "id": "1006496", "parcel_ids": ["1004540043"] },
      { "id": "1006497", "parcel_ids": ["1004540044"] },
      { "id": "1006498", "parcel_ids": ["1004540045"] },
      { "id": "1006499", "parcel_ids": ["1004540046"] },
      { "id": "1006500", "parcel_ids": ["1004540047"] }
    ]
  },
  "hard_constraints": [
    {
      "id": "asserted_address_core",
      "constraint": {
        "kind": "allowed_sets",
        "level": "parcel",
        "sets": [[
          "1004540041",
          "1004540042",
          "1004540043",
          "1004540044",
          "1004540045",
          "1004540046"
        ]]
      }
    },
    {
      "id": "area_majority_buildings",
      "constraint": {
        "kind": "allowed_sets",
        "level": "building",
        "sets": [[
          "1006494",
          "1006495",
          "1006496",
          "1006497",
          "1006498",
          "1006499"
        ]]
      }
    },
    {
      "id": "chimera_wrongly_admitted",
      "constraint": {
        "kind": "allowed_sets",
        "level": "parcel",
        "sets": [["1004540047"]]
      }
    }
  ],
  "soft_preferences": [],
  "max_assignments": 20000,
  "max_materialized_models": 16
}
JSON

run_output_path() {
  local node_id="$1"
  local output_id="$2"
  local relative
  relative="$(
    jq -er \
      --arg node "$node_id" \
      --arg output "$output_id" \
      '.project_run_report.receipt.node_receipts[]
        | select(.node_id == $node)
        | .outputs[]
        | select(.output_id == $output)
        | .path' \
      "$run_artifact"
  )"
  printf '%s/%s\n' "$work_dir" "$relative"
}

run_json "$capabilities_artifact" geo capabilities --emit json
fixture_digest="$(jq -r '.semantic_hash' "$capabilities_artifact")"
run_center_cell="892a100d62bffff"

jq -n \
  '{
    version: "canon_geo_question.v0",
    question_id: "question.demo0.case4.fixture_replay",
    subject_bindings: [
      {
        role: "target",
        binding_class: "operator_label",
        value: "Demo 0 retained case 4 chimera fixture"
      },
      {
        role: "input_address",
        binding_class: "address_text",
        value: "199 EAST 12 STREET"
      }
    ],
    bounded_geography: {
      geography_id: "region.demo0.case4.fixture_tile",
      geography_kind: "bounded_fixture",
      description: "Demo 0 retained Case 4 bounded parcel fixture"
    },
    requested_grains: [{
      entity_level: "parcel",
      required_evidence_classes: ["address_set", "building_footprint"],
      optional_evidence_classes: []
    }],
    query_as_of: {
      utc_day: "2026-08-31",
      semantic_id: "demo0.query_as_of.utc_day",
      unit: "utc_day",
      origin: "caller_declared"
    },
    requested_claim_classes: ["candidate_reach", "collateral_composition", "stable_identity"],
    presentation_limits: [
      {
        semantic_id: "presentation.max_models",
        counter: "models",
        value: 16,
        unit: "model",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "presentation.max_candidates",
        counter: "candidates",
        value: 32,
        unit: "candidate",
        origin: "caller_declared",
        action: "report_budget_fallback"
      }
    ],
    abstention_policy: {
      unsupported_grain: "report_unsupported",
      unresolved_residual: "report_residual",
      budget_fallback: "report_residual"
    },
    decision_policy: null,
    resource_budget_ref: "budget.demo0.case4.fixture_replay"
  }' > "$plan_question"

jq -n \
  --arg digest "$fixture_digest" \
  '{
    version: "canon_geo_regional_inventory.v1",
    inventory_id: "inventory.demo0.case4.fixture_replay",
    region: {
      geography_id: "region.demo0.case4.fixture_tile",
      geography_kind: "bounded_fixture",
      description: "Demo 0 retained Case 4 bounded parcel fixture"
    },
    sources: [{
      source_instance_id: "demo0_bounded_parcel_evidence",
      release: {
        release_id: "demo0.case4.fixture_release",
        release_digest: $digest
      },
      temporal_scope: {
        valid_time: {
          start_utc_day: "2026-01-01",
          end_utc_day: "2026-12-31"
        },
        release_time: {
          utc_day: "2026-08-31",
          semantic_id: "demo0.fixture_release.utc_day",
          unit: "utc_day",
          origin: "caller_declared"
        }
      },
      lineage_ids: ["demo0.retained.case4.fixture_replay"],
      native_scope: {
        kind: "native_entity",
        entity_level: "parcel",
        identity_participation: "stable_alias"
      },
      evidence_classes: ["address_set", "building_footprint"],
      coverage: {
        coverage_id: "coverage.demo0.case4.fixture_tile",
        region: {
          geography_id: "region.demo0.case4.fixture_tile",
          geography_kind: "bounded_fixture",
          description: "Demo 0 retained Case 4 bounded parcel fixture"
        },
        predicate: "declared retained Case 4 parcel fixture candidates"
      },
      local_state: {
        state: "available",
        local_ref: {
          artifact_id: "artifact.demo0.case4.warehouse_rows",
          contract_version: "canon_geo_warehouse_rows.v0",
          content_hash: $digest,
          media_type: "application/json"
        }
      },
      geometry: {
        geometry_contract_version: "demo0.fixture.geometry.v1",
        coordinate_reference_system: "EPSG:4326",
        transform_id: "demo0.fixture.identity_transform",
        transform_digest: $digest,
        numeric_error_bounds: [{
          semantic_id: "demo0.fixture.transform_error",
          value: 0,
          unit: "millimetre",
          origin: "adapter_contract"
        }]
      },
      license_class: "public_redistributable",
      egress_class: "shareable",
      estimates: [{
        semantic_id: "demo0.fixture.rows",
        value: 7,
        unit: "row",
        origin: "caller_declared"
      }]
    }],
    discovery_gaps: []
  }' > "$plan_inventory"

jq -n \
  '{
    version: "canon_geo_composition_profile.v0",
    selection_level: "parcel"
  }' > "$plan_profile"

jq -n \
  '{
    version: "canon_geo_resource_budget.v0",
    budget_id: "budget.demo0.case4.fixture_replay",
    deterministic_bounds: [
      {
        semantic_id: "budget.max_bytes",
        counter: "bytes",
        value: 1000000,
        unit: "byte",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "budget.max_rows",
        counter: "rows",
        value: 10000,
        unit: "row",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "budget.max_cells",
        counter: "cells",
        value: 64,
        unit: "cell",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "budget.max_candidates",
        counter: "candidates",
        value: 500,
        unit: "candidate",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "budget.max_variables",
        counter: "variables",
        value: 128,
        unit: "variable",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "budget.max_states",
        counter: "states",
        value: 100000,
        unit: "state",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "budget.max_models",
        counter: "models",
        value: 10000,
        unit: "model",
        origin: "caller_declared",
        action: "report_budget_fallback"
      },
      {
        semantic_id: "budget.max_operations",
        counter: "operations",
        value: 1000000,
        unit: "operation",
        origin: "caller_declared",
        action: "report_budget_fallback"
      }
    ],
    telemetry: [{
      metric: "wall_time",
      unit: "millisecond",
      origin: "operator_policy",
      semantic_effect: "none"
    }]
  }' > "$plan_budget"

run_json "$plan_artifact" geo plan \
  --question "$plan_question" \
  --capabilities "$capabilities_artifact" \
  --inventory "$plan_inventory" \
  --profile "$plan_profile" \
  --budget "$plan_budget"

jq -n \
  --slurpfile rows "$case4_rows" \
  --slurpfile plan "$plan_artifact" \
  --arg digest "$fixture_digest" \
  --arg center "$run_center_cell" \
  'def source($plan; $digest): {
      source_instance_id: "demo0_bounded_parcel_evidence",
      release: {
        release_id: "demo0.case4.fixture_release",
        release_digest: $digest
      },
      native_scope: {
        kind: "native_entity",
        entity_level: "parcel",
        identity_participation: "stable_alias"
      },
      inventory_ref: $plan.inventory_ref
    };
    {
      version: "canon_geo_home_cell_rows.v1",
      coordinate_crs: "EPSG:4326",
      coordinate_decimal_places: 9,
      h3_resolution: 9,
      stability_radius_fixed: 1000,
      rows: ($rows[0].parcel_rows
        | sort_by(.parcel_id)
        | map({
          source: source($plan[0]; $digest),
          feature_id: .parcel_id,
          source_record_id: ("demo0.case4.parcel_candidate." + .parcel_id),
          geometry_sha256: "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed",
          representative_point_method: "centroid_of_derived_wgs84_geometry",
          longitude: "-73.977264000",
          latitude: "40.753429000",
          transform_execution_id: "demo0.fixture.transform_execution",
          transform_definition_id: "demo0.fixture.transform_definition",
          claimed_home_cell: $center
        })),
      max_rows: 32
    }' > "$run_home_cell_rows"

jq -n \
  --slurpfile rows "$run_home_cell_rows" \
  --arg center "$run_center_cell" \
  '{
    version: "canon_geo_tile_work_request.v1",
    center_cell: $center,
    halo_k: 1,
    features: ($rows[0].rows
      | map({
        source,
        feature_id,
        home_cell: $center
      })),
    max_features: 32,
    max_work_cells: 7
  }' > "$run_tile_request"

run_json "$run_artifact" geo run \
  --plan "$plan_artifact" \
  --work-dir "$work_dir" \
  --input "geo.parcel.home_cells:rows=$run_home_cell_rows" \
  --input "geo.parcel.section:request=$run_tile_request" \
  --input "geo.parcel.materialize_evidence:rows=$case4_rows"

evidence_request="$(run_output_path "geo.parcel.materialize_evidence" "materialize_evidence")"
evidence_compilation="$(run_output_path "geo.parcel.compile_evidence" "compile_evidence")"
solve_artifact="$(run_output_path "geo.parcel.solve" "solve")"

jq -e . "$evidence_request" >/dev/null
jq -e . "$evidence_compilation" >/dev/null
jq -e . "$solve_artifact" >/dev/null

jq -e '
  .status == "resolved"
  and .summary.residual_model_count == 1
  and .summary.residual_model_count_complete == true
  and .backbone_complete == true
  and .hard_forced.parcels == [
    "1004540041",
    "1004540042",
    "1004540043",
    "1004540044",
    "1004540045",
    "1004540046"
  ]
  and .hard_forced.buildings == [
    "1006494",
    "1006495",
    "1006496",
    "1006497",
    "1006498",
    "1006499"
  ]
' "$solve_artifact" >/dev/null

jq -e '
  ([.admissions[].disposition] | sort) == [
    "diagnostic_only",
    "hard_constraint",
    "hard_constraint"
  ]
' "$evidence_compilation" >/dev/null

jq -n \
  --slurpfile evidence "$evidence_request" \
  '{
    version: "canon_geo_population_request.v0",
    max_cases: 1,
    cases: [{
      id: "demo0.case4_chimera_multi_street.fixture_replay",
      evidence: $evidence[0],
      truth_plane: "address_derived_control",
      truth: {
        parcels: [
          "1004540041",
          "1004540042",
          "1004540043",
          "1004540044",
          "1004540045",
          "1004540046"
        ],
        buildings: [
          "1006494",
          "1006495",
          "1006496",
          "1006497",
          "1006498",
          "1006499"
        ]
      }
    }]
  }' > "$population_request"

run_json "$evaluation_artifact" geo evaluate --population "$population_request"
jq -e '
  .summary.cases == 1
  and .summary.resolved_cases == 1
  and .summary.solver_truth_exclusion_cases == 0
  and .cases[0].truth_model_in_residual == true
' "$evaluation_artifact" >/dev/null

run_json "$negative_artifact" geo solve --request "$negative_request"
jq -e '
  .status == "conflict"
  and .summary.residual_model_count == 0
  and (.conflict_constraint_ids | index("chimera_wrongly_admitted") != null)
' "$negative_artifact" >/dev/null

center_cell="892a100d26bffff"
cat > "$tile_discovery_request" <<JSON
{
  "version": "canon_geo_tile_work_request.v1",
  "center_cell": "$center_cell",
  "halo_k": 1,
  "features": [
    {
      "source": {
        "source_instance_id": "demo0_case4",
        "release": {
          "release_id": "demo0_case4.release",
          "release_digest": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        },
        "native_scope": {
          "kind": "native_entity",
          "entity_level": "parcel",
          "identity_participation": "stable_alias"
        },
        "inventory_ref": {
          "inventory_id": "inventory.demo0",
          "semantic_hash": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
          "planning_hash": "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        }
      },
      "feature_id": "owner_probe",
      "home_cell": "$center_cell"
    }
  ],
  "max_features": 8,
  "max_work_cells": 7
}
JSON
run_json "$tile_discovery_artifact" geo tile-work --request "$tile_discovery_request"
neighbor_cell="$(jq -r --arg center "$center_cell" '[.work_cells[] | select(. != $center)] | sort | .[0]' "$tile_discovery_artifact")"
if [[ -z "$neighbor_cell" || "$neighbor_cell" == "null" ]]; then
  printf 'demo0 could not derive a deterministic k1 neighbor cell\n' >&2
  exit 70
fi

jq -n \
  --arg center "$center_cell" \
  --arg neighbor "$neighbor_cell" \
  '{
    version: "canon_geo_tile_work_request.v1",
    center_cell: $center,
    halo_k: 1,
    features: [
      {
        source: {
          source_instance_id: "mappluto_parcel",
          release: {
            release_id: "mappluto_parcel.release",
            release_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          },
          native_scope: {
            kind: "native_entity",
            entity_level: "parcel",
            identity_participation: "stable_alias"
          },
          inventory_ref: {
            inventory_id: "inventory.demo0",
            semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
          }
        },
        feature_id: "case4_core_six_parcels",
        home_cell: $center
      },
      {
        source: {
          source_instance_id: "nyc_building_footprints",
          release: {
            release_id: "nyc_building_footprints.release",
            release_digest: "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
          },
          native_scope: {
            kind: "native_entity",
            entity_level: "building",
            identity_participation: "evidence_only"
          },
          inventory_ref: {
            inventory_id: "inventory.demo0",
            semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
          }
        },
        feature_id: "case4_six_majority_footprints",
        home_cell: $neighbor
      }
    ],
    max_features: 8,
    max_work_cells: 7
  }' > "$tile_owner_request"

jq -n \
  --arg center "$center_cell" \
  --arg neighbor "$neighbor_cell" \
  '{
    version: "canon_geo_tile_work_request.v1",
    center_cell: $neighbor,
    halo_k: 1,
    features: [
      {
        source: {
          source_instance_id: "mappluto_parcel",
          release: {
            release_id: "mappluto_parcel.release",
            release_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          },
          native_scope: {
            kind: "native_entity",
            entity_level: "parcel",
            identity_participation: "stable_alias"
          },
          inventory_ref: {
            inventory_id: "inventory.demo0",
            semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
          }
        },
        feature_id: "case4_core_six_parcels",
        home_cell: $center
      },
      {
        source: {
          source_instance_id: "nyc_building_footprints",
          release: {
            release_id: "nyc_building_footprints.release",
            release_digest: "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
          },
          native_scope: {
            kind: "native_entity",
            entity_level: "building",
            identity_participation: "evidence_only"
          },
          inventory_ref: {
            inventory_id: "inventory.demo0",
            semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
          }
        },
        feature_id: "case4_six_majority_footprints",
        home_cell: $neighbor
      }
    ],
    max_features: 8,
    max_work_cells: 7
  }' > "$tile_observer_request"

run_json "$tile_owner_artifact" geo tile-work --request "$tile_owner_request"
run_json "$tile_observer_artifact" geo tile-work --request "$tile_observer_request"

payload_blake3="$(jq -r '.evidence_compilation.blake3' "$solve_artifact")"
if [[ ! "$payload_blake3" =~ ^[0-9a-f]{64}$ ]]; then
  printf 'demo0 solve output did not include a valid evidence compilation digest\n' >&2
  exit 70
fi

jq -n \
  --slurpfile owner "$tile_owner_artifact" \
  --slurpfile observer "$tile_observer_artifact" \
  --arg center "$center_cell" \
  --arg neighbor "$neighbor_cell" \
  --arg payload "blake3:$payload_blake3" \
  '{
    version: "canon_geo_tile_reconciliation_request.v1",
    halo_k: 1,
    batches: [
      {
        work_unit: $owner[0],
        proposals: [{
          semantics: { kind: "composition" },
          work_unit_blake3: $owner[0].work_unit_blake3,
          payload_blake3: $payload,
          members: [
            {
              source: {
                source_instance_id: "mappluto_parcel",
                release: {
                  release_id: "mappluto_parcel.release",
                  release_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                },
                native_scope: {
                  kind: "native_entity",
                  entity_level: "parcel",
                  identity_participation: "stable_alias"
                },
                inventory_ref: {
                  inventory_id: "inventory.demo0",
                  semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                  planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                }
              },
              feature_id: "case4_core_six_parcels",
              candidate_entity_level: "parcel",
              home_cell: $center
            },
            {
              source: {
                source_instance_id: "nyc_building_footprints",
                release: {
                  release_id: "nyc_building_footprints.release",
                  release_digest: "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                },
                native_scope: {
                  kind: "native_entity",
                  entity_level: "building",
                  identity_participation: "evidence_only"
                },
                inventory_ref: {
                  inventory_id: "inventory.demo0",
                  semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                  planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                }
              },
              feature_id: "case4_six_majority_footprints",
              candidate_entity_level: "building",
              home_cell: $neighbor
            }
          ]
        }]
      },
      {
        work_unit: $observer[0],
        proposals: [{
          semantics: { kind: "composition" },
          work_unit_blake3: $observer[0].work_unit_blake3,
          payload_blake3: $payload,
          members: [
            {
              source: {
                source_instance_id: "nyc_building_footprints",
                release: {
                  release_id: "nyc_building_footprints.release",
                  release_digest: "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                },
                native_scope: {
                  kind: "native_entity",
                  entity_level: "building",
                  identity_participation: "evidence_only"
                },
                inventory_ref: {
                  inventory_id: "inventory.demo0",
                  semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                  planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                }
              },
              feature_id: "case4_six_majority_footprints",
              candidate_entity_level: "building",
              home_cell: $neighbor
            },
            {
              source: {
                source_instance_id: "mappluto_parcel",
                release: {
                  release_id: "mappluto_parcel.release",
                  release_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                },
                native_scope: {
                  kind: "native_entity",
                  entity_level: "parcel",
                  identity_participation: "stable_alias"
                },
                inventory_ref: {
                  inventory_id: "inventory.demo0",
                  semantic_hash: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                  planning_hash: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                }
              },
              feature_id: "case4_core_six_parcels",
              candidate_entity_level: "parcel",
              home_cell: $center
            }
          ]
        }]
      }
    ],
    max_batches: 4,
    max_proposals: 8,
    max_members_per_decision: 8,
    max_features_per_batch: 8,
    max_work_cells_per_batch: 7
  }' > "$reconciliation_request"

run_json "$reconciliation_artifact" geo reconcile-tiles --request "$reconciliation_request"
jq -e '
  .owned_decisions == 1
  and .discarded_halo_proposals == 1
  and .input_proposals == 2
  and .decisions[0].semantics.kind == "composition"
  and .decisions[0].proposal_copies == 2
' "$reconciliation_artifact" >/dev/null

jq -S -c -n \
  --slurpfile capabilities "$capabilities_artifact" \
  --slurpfile plan "$plan_artifact" \
  --slurpfile run "$run_artifact" \
  --slurpfile evidence "$evidence_request" \
  --slurpfile compilation "$evidence_compilation" \
  --slurpfile solve "$solve_artifact" \
  --slurpfile evaluation "$evaluation_artifact" \
  --slurpfile owner_tile "$tile_owner_artifact" \
  --slurpfile observer_tile "$tile_observer_artifact" \
  --slurpfile reconciliation "$reconciliation_artifact" \
  --slurpfile negative "$negative_artifact" \
  '{
    artifact_versions: {
      capabilities: $capabilities[0].version,
      plan: $plan[0].version,
      run: $run[0].version,
      evidence_request: $evidence[0].version,
      evidence_compilation: $compilation[0].version,
      composition: $solve[0].version,
      population_evaluation: $evaluation[0].version,
      owner_tile_work: $owner_tile[0].version,
      observer_tile_work: $observer_tile[0].version,
      tile_reconciliation: $reconciliation[0].version,
      negative_composition: $negative[0].version
    },
    capabilities: {
      implemented_geo_commands: ($capabilities[0].commands.implemented | length),
      unavailable_control_plane: ($capabilities[0].commands.unavailable | map(.command))
    },
    commands_exercised: [
      "canon geo capabilities --emit json",
      "canon geo plan --question question.json --capabilities capabilities.json --inventory inventory.json --profile profile.json --budget budget.json",
      "canon geo run --plan plan.json --work-dir DIR --input geo.parcel.home_cells:rows=run-home-cell-rows.json --input geo.parcel.section:request=run-tile-request.json --input geo.parcel.materialize_evidence:rows=case4-warehouse-rows.json",
      "canon geo evaluate --population population.json",
      "canon geo solve --request negative-hard-chimera-composition.json",
      "canon geo tile-work --request tile-discovery-request.json",
      "canon geo tile-work --request tile-owner-request.json",
      "canon geo tile-work --request tile-observer-request.json",
      "canon geo reconcile-tiles --request tile-reconciliation-request.json"
    ],
    composition: {
      backbone_complete: $solve[0].backbone_complete,
      hard_forced: $solve[0].hard_forced,
      bounded_universe: {
        parcel_candidates: $solve[0].summary.parcel_candidates,
        building_candidates: $solve[0].summary.building_candidates,
        solution_parcels: ($solve[0].hard_forced.parcels | length),
        solution_buildings: ($solve[0].hard_forced.buildings | length)
      },
      residual_model_count: $solve[0].summary.residual_model_count,
      residual_model_count_complete: $solve[0].summary.residual_model_count_complete,
      status: $solve[0].status
    },
    shared_run: {
      status: $run[0].status,
      phase: $run[0].phase,
      output_contracts: ($run[0].output_refs | map(.contract_version) | sort),
      executed_nodes: $run[0].project_run_report.executed_nodes,
      blocked_nodes: $run[0].project_run_report.blocked_nodes,
      proof_boundary: "offline_contract_replay"
    },
    demo_id: "canon_geo_demo0_case4_chimera_fixture",
    evaluation: {
      candidate_reach: $evaluation[0].cases[0].candidate_reach,
      cases: $evaluation[0].summary.cases,
      evaluation_role: "contract_replay_not_accuracy",
      false_merge_cases: $evaluation[0].summary.false_merge_cases,
      resolved_cases: $evaluation[0].summary.resolved_cases,
      solver_truth_exclusion_cases: $evaluation[0].summary.solver_truth_exclusion_cases,
      truth_independent: false,
      truth_model_in_residual: $evaluation[0].cases[0].truth_model_in_residual,
      truth_plane: $evaluation[0].cases[0].truth_plane
    },
    evidence: {
      admissions_total: ($compilation[0].admissions | length),
      diagnostic_admissions: ($compilation[0].admissions | map(select(.disposition == "diagnostic_only")) | length),
      hard_constraint_admissions: ($compilation[0].admissions | map(select(.disposition == "hard_constraint")) | length),
      source_record_hash_scope: "fixture_row_blake3_values_not_original_warehouse_byte_receipts"
    },
    fixture_address_probe: {
      admitted_as_hard_evidence: false,
      diagnostic_disposition: ($compilation[0].admissions[] | select(.observation_id == "obs.case4.fixture-address-probe-199-e-12-street") | .disposition),
      excluded_candidate_forced: (($solve[0].hard_forced.parcels | index("1004540047")) != null),
      parser_exercised: false,
      probe_address: "199 EAST 12 STREET",
      retained_mappluto_match_count: 0
    },
    negative: {
      conflict_constraint_ids: $negative[0].conflict_constraint_ids,
      residual_model_count: $negative[0].summary.residual_model_count,
      status: $negative[0].status,
      test: "wrongly_admit_fixture_address_probe_as_hard_evidence"
    },
    proof: {
      bounded_scope: "seven parcel candidates with a six-member solution plus one two-cell H3 ownership fixture",
      case_id: "case_4_chimera_multi_street",
      fresh_live_receipt: false,
      not_claimed: [
        "live_source_acquisition",
        "national_solve",
        "citywide_recall",
        "source_truth",
        "fresh_live_receipt"
      ],
      proof_class: "fixture",
      retained_source_basis: [
        "docs/geo_design_session/CASE_4_CHIMERA_MULTI_STREET.md",
        "tests/fixtures/geo/e4_worked_cases.json"
      ]
    },
    tile_ownership: {
      discarded_halo_proposals: $reconciliation[0].discarded_halo_proposals,
      halo_k: $reconciliation[0].halo_k,
      input_proposals: $reconciliation[0].input_proposals,
      owned_decisions: $reconciliation[0].owned_decisions,
      owner_cell: $reconciliation[0].decisions[0].owner_cell,
      decision_semantics: $reconciliation[0].decisions[0].semantics.kind,
      owner_tile_center_features: $owner_tile[0].center_feature_count,
      owner_tile_halo_features: $owner_tile[0].halo_feature_count,
      payload_blake3_source: "solve.evidence_compilation.blake3",
      work_cells_per_tile: ($owner_tile[0].work_cells | length)
    }
  }'
