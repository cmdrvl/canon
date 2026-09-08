#!/usr/bin/env bash
set -euo pipefail

work_dir=""
canon_bin="${CANON_BIN:-target/debug/canon}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      work_dir="${2:?missing --work-dir value}"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$work_dir" ]]; then
  echo "usage: $0 --work-dir <dir>" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "e2e_accretion requires jq on PATH" >&2
  exit 2
fi

if [[ ! -x "$canon_bin" ]]; then
  echo "missing executable $canon_bin; build target/debug/canon before running this harness" >&2
  exit 2
fi

mkdir -p "$work_dir"
log="$work_dir/run.log"
: > "$log"

run_json() {
  local output="$1"
  shift
  {
    printf '+'
    printf ' %q' "$@"
    printf ' > %q\n' "$output"
  } | tee -a "$log"
  set +e
  "$@" >"$output" 2>>"$log"
  local status=$?
  set -e
  echo "exit_code=$status" | tee -a "$log"
  return "$status"
}

assert_jq() {
  local filter="$1"
  local file="$2"
  jq -e "$filter" "$file" >/dev/null
}

log_run_digests() {
  local label="$1"
  local file="$2"
  jq -r --arg label "$label" '
    "run \($label) semantic_hash=\(.semantic_hash)",
    (.output_refs[]? | "output_ref \($label) \(.artifact_id) \(.content_digest)")
  ' "$file" >>"$log"
}

inputs_dir="$work_dir/inputs"
mkdir -p "$inputs_dir"

cat >"$inputs_dir/question.json" <<'JSON'
{
  "version": "canon_geo_question.v0",
  "question_id": "question.fixture.accretion.public",
  "subject_bindings": [
    {
      "role": "target",
      "binding_class": "operator_label",
      "value": "fixture public accretion subject"
    }
  ],
  "bounded_geography": {
    "geography_id": "region.fixture.accretion",
    "geography_kind": "bounded_fixture",
    "description": "One explicitly bounded accretion fixture"
  },
  "requested_grains": [
    {
      "entity_level": "building",
      "required_evidence_classes": ["building_footprint"],
      "optional_evidence_classes": []
    }
  ],
  "query_as_of": {
    "utc_day": "2026-08-31",
    "semantic_id": "query.as_of",
    "unit": "utc_day",
    "origin": "caller_declared"
  },
  "requested_claim_classes": ["collateral_composition"],
  "presentation_limits": [
    {
      "semantic_id": "presentation.max_models",
      "counter": "models",
      "value": 16,
      "unit": "model",
      "origin": "caller_declared",
      "action": "truncate_presentation_only"
    }
  ],
  "abstention_policy": {
    "unsupported_grain": "report_unsupported",
    "unresolved_residual": "report_residual",
    "budget_fallback": "report_residual"
  },
  "decision_policy": null,
  "resource_budget_ref": "budget.fixture.accretion"
}
JSON

cat >"$inputs_dir/inventory.json" <<'JSON'
{
  "version": "canon_geo_regional_inventory.v1",
  "inventory_id": "inventory.fixture.accretion",
  "region": {
    "geography_id": "region.fixture.accretion",
    "geography_kind": "bounded_fixture",
    "description": "One explicitly bounded accretion fixture"
  },
  "sources": [
    {
      "source_instance_id": "source.fixture.buildings",
      "release": {
        "release_id": "release.fixture.one",
        "release_digest": "blake3:1111111111111111111111111111111111111111111111111111111111111111"
      },
      "temporal_scope": {
        "valid_time": {
          "start_utc_day": "2026-01-01",
          "end_utc_day": "2026-12-31"
        },
        "transaction_time": null,
        "release_time": null
      },
      "lineage_ids": ["lineage.fixture.accretion"],
      "native_scope": {
        "kind": "native_entity",
        "entity_level": "building",
        "identity_participation": "stable_alias"
      },
      "evidence_classes": ["building_footprint"],
      "coverage": {
        "coverage_id": "coverage.fixture.accretion",
        "region": {
          "geography_id": "region.fixture.accretion",
          "geography_kind": "bounded_fixture",
          "description": "One explicitly bounded accretion fixture"
        },
        "predicate": "all declared fixture records"
      },
      "local_state": {
        "state": "available",
        "local_ref": {
          "artifact_id": "artifact.fixture.buildings",
          "contract_version": "canon_geo_warehouse_rows.v0",
          "content_hash": "blake3:2222222222222222222222222222222222222222222222222222222222222222",
          "media_type": "application/json"
        }
      },
      "geometry": {
        "geometry_contract_version": "geometry.fixture.v1",
        "coordinate_reference_system": "EPSG:4326",
        "transform_id": "identity.fixture",
        "transform_digest": "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        "numeric_error_bounds": [
          {
            "semantic_id": "transform.error",
            "value": 0,
            "unit": "millimeter",
            "origin": "adapter_contract"
          }
        ]
      },
      "license_class": "public_redistributable",
      "egress_class": "shareable",
      "estimates": [
        {
          "semantic_id": "source.rows",
          "value": 100,
          "unit": "row",
          "origin": "source_release"
        }
      ]
    }
  ],
  "discovery_gaps": []
}
JSON

cat >"$inputs_dir/profile.json" <<'JSON'
{
  "version": "canon_geo_composition_profile.v0",
  "selection_level": "building"
}
JSON

cat >"$inputs_dir/budget.json" <<'JSON'
{
  "version": "canon_geo_resource_budget.v0",
  "budget_id": "budget.fixture.accretion",
  "deterministic_bounds": [
    {
      "semantic_id": "budget.max_bytes",
      "counter": "bytes",
      "value": 1000000,
      "unit": "byte",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    },
    {
      "semantic_id": "budget.max_rows",
      "counter": "rows",
      "value": 10000,
      "unit": "row",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    },
    {
      "semantic_id": "budget.max_cells",
      "counter": "cells",
      "value": 64,
      "unit": "cell",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    },
    {
      "semantic_id": "budget.max_candidates",
      "counter": "candidates",
      "value": 500,
      "unit": "candidate",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    },
    {
      "semantic_id": "budget.max_variables",
      "counter": "variables",
      "value": 128,
      "unit": "variable",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    },
    {
      "semantic_id": "budget.max_states",
      "counter": "states",
      "value": 100000,
      "unit": "state",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    },
    {
      "semantic_id": "budget.max_models",
      "counter": "models",
      "value": 10000,
      "unit": "model",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    },
    {
      "semantic_id": "budget.max_operations",
      "counter": "operations",
      "value": 1000000,
      "unit": "operation",
      "origin": "caller_declared",
      "action": "report_budget_fallback"
    }
  ],
  "telemetry": [
    {
      "metric": "wall_time",
      "unit": "millisecond",
      "origin": "operator_policy",
      "semantic_effect": "none"
    }
  ]
}
JSON

"$canon_bin" geo capabilities --emit json >"$inputs_dir/capabilities.json"

cat >"$inputs_dir/home-cell-rows.json" <<'JSON'
{
  "version": "canon_geo_home_cell_rows.v1",
  "coordinate_crs": "EPSG:4326",
  "coordinate_decimal_places": 9,
  "h3_resolution": 9,
  "stability_radius_fixed": 1000,
  "rows": [
    {
      "source": {
        "source_instance_id": "source.fixture.buildings",
        "release": {
          "release_id": "fixture-release-2026-08-31",
          "release_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444"
        },
        "native_scope": {
          "kind": "native_entity",
          "entity_level": "building",
          "identity_participation": "stable_alias"
        },
        "inventory_ref": {
          "inventory_id": "inventory.fixture.accretion",
          "semantic_hash": "blake3:5555555555555555555555555555555555555555555555555555555555555555",
          "planning_hash": "blake3:6666666666666666666666666666666666666666666666666666666666666666"
        }
      },
      "feature_id": "building-a",
      "source_record_id": "rec-building-a",
      "geometry_sha256": "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed",
      "representative_point_method": "centroid_of_derived_wgs84_geometry",
      "longitude": "-73.977264000",
      "latitude": "40.753429000",
      "transform_execution_id": "fixture-transform-execution",
      "transform_definition_id": "fixture-transform-definition",
      "claimed_home_cell": "892a100d62bffff"
    },
    {
      "source": {
        "source_instance_id": "source.fixture.buildings",
        "release": {
          "release_id": "fixture-release-2026-08-31",
          "release_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444"
        },
        "native_scope": {
          "kind": "native_entity",
          "entity_level": "building",
          "identity_participation": "stable_alias"
        },
        "inventory_ref": {
          "inventory_id": "inventory.fixture.accretion",
          "semantic_hash": "blake3:5555555555555555555555555555555555555555555555555555555555555555",
          "planning_hash": "blake3:6666666666666666666666666666666666666666666666666666666666666666"
        }
      },
      "feature_id": "building-b",
      "source_record_id": "rec-building-b",
      "geometry_sha256": "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed",
      "representative_point_method": "centroid_of_derived_wgs84_geometry",
      "longitude": "-73.977264000",
      "latitude": "40.753429000",
      "transform_execution_id": "fixture-transform-execution",
      "transform_definition_id": "fixture-transform-definition",
      "claimed_home_cell": "892a100d62bffff"
    }
  ],
  "max_rows": 16
}
JSON

cat >"$inputs_dir/tile-work-request.json" <<'JSON'
{
  "version": "canon_geo_tile_work_request.v1",
  "center_cell": "892a100d62bffff",
  "halo_k": 1,
  "features": [
    {
      "source": {
        "source_instance_id": "source.fixture.buildings",
        "release": {
          "release_id": "fixture-release-2026-08-31",
          "release_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444"
        },
        "native_scope": {
          "kind": "native_entity",
          "entity_level": "building",
          "identity_participation": "stable_alias"
        },
        "inventory_ref": {
          "inventory_id": "inventory.fixture.accretion",
          "semantic_hash": "blake3:5555555555555555555555555555555555555555555555555555555555555555",
          "planning_hash": "blake3:6666666666666666666666666666666666666666666666666666666666666666"
        }
      },
      "feature_id": "building-a",
      "home_cell": "892a100d62bffff"
    },
    {
      "source": {
        "source_instance_id": "source.fixture.buildings",
        "release": {
          "release_id": "fixture-release-2026-08-31",
          "release_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444"
        },
        "native_scope": {
          "kind": "native_entity",
          "entity_level": "building",
          "identity_participation": "stable_alias"
        },
        "inventory_ref": {
          "inventory_id": "inventory.fixture.accretion",
          "semantic_hash": "blake3:5555555555555555555555555555555555555555555555555555555555555555",
          "planning_hash": "blake3:6666666666666666666666666666666666666666666666666666666666666666"
        }
      },
      "feature_id": "building-b",
      "home_cell": "892a100d62bffff"
    }
  ],
  "candidate_reach_reference": null,
  "max_features": 16,
  "max_work_cells": 7
}
JSON

cat >"$inputs_dir/warehouse-rows.json" <<'JSON'
{
  "version": "canon_geo_warehouse_rows.v0",
  "profile": {
    "version": "canon_geo_composition_profile.v0",
    "selection_level": "building"
  },
  "parcel_rows": [],
  "building_parcel_rows": [
    {
      "building_id": "building-b",
      "parcel_id": null
    },
    {
      "building_id": "building-a",
      "parcel_id": null
    }
  ],
  "contracts": [
    {
      "id": "rho.building-set",
      "version": "1.0.0",
      "source_dataset": "fixture.buildings",
      "source_release": "2026-08-31",
      "source_lineage_ids": ["fixture.buildings.release.rho.building-set"],
      "method_id": "fixture-building-candidate-set",
      "method_version": "1.0.0",
      "claim_role": "stable_identity_anchor",
      "basis": {
        "kind": "logical_relaxation",
        "invariant_id": "candidate-set-is-a-superset"
      }
    }
  ],
  "evidence_rows": [
    {
      "observation_id": "obs.building-set.b",
      "contract_id": "rho.building-set",
      "source_record": {
        "source_record_id": "row-b",
        "source_vintage": "2026-08-31",
        "record_blake3": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
      },
      "valid_time": null,
      "observation": {
        "kind": "exact_sets",
        "level": "building",
        "sets": [["building-a", "building-b"]]
      }
    },
    {
      "observation_id": "obs.building-set.a",
      "contract_id": "rho.building-set",
      "source_record": {
        "source_record_id": "row-a",
        "source_vintage": "2026-08-31",
        "record_blake3": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      },
      "valid_time": null,
      "observation": {
        "kind": "exact_sets",
        "level": "building",
        "sets": [["building-a", "building-b"]]
      }
    }
  ],
  "max_assignments": 128,
  "max_materialized_models": 64
}
JSON

jq '.evidence_rows += [{
  "observation_id": "obs.building-set.c",
  "contract_id": "rho.building-set",
  "source_record": {
    "source_record_id": "row-c",
    "source_vintage": "2026-08-31",
    "record_blake3": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  },
  "valid_time": null,
  "observation": {
    "kind": "exact_sets",
    "level": "building",
    "sets": [["building-a", "building-b"]]
  }
}]' "$inputs_dir/warehouse-rows.json" >"$inputs_dir/warehouse-rows-plus-one.json"

cat >"$inputs_dir/separation-inputs.json" <<'JSON'
{
  "version": "canon_geo_separation_inputs.v0",
  "subject_ref": null,
  "prospective": [
    {
      "id": "obs.prospective.binary-a",
      "contract_id": "rho.prospective.binary-a",
      "cost_units": 1,
      "outcomes": [
        {
          "outcome_id": "outcome.forbid-a",
          "induced": [
            {
              "kind": "forbid",
              "member": {
                "level": "building",
                "id": "building-a"
              }
            }
          ]
        },
        {
          "outcome_id": "outcome.require-a",
          "induced": [
            {
              "kind": "require",
              "member": {
                "level": "building",
                "id": "building-a"
              }
            }
          ]
        }
      ]
    }
  ]
}
JSON

jq -n --slurpfile budget "$inputs_dir/budget.json" '{
  version: "canon_geo_next_evidence_inputs.v0",
  candidates: [
    {
      action_id: "action.binary-a",
      class: "separate_residual",
      kind: {
        kind: "observe",
        payload: "obs.prospective.binary-a"
      },
      observation_id: "obs.prospective.binary-a",
      cost_units: 1,
      redundant: false,
      lineage_ids: [],
      stop_reason: null
    }
  ],
  policy: null,
  budget: $budget[0],
  budget_spent: {}
}' >"$inputs_dir/next-evidence-inputs.json"

for file in "$inputs_dir"/*.json; do
  jq -e . "$file" >/dev/null
done

plan="$work_dir/plan.json"
run1="$work_dir/run-1.json"
run2="$work_dir/run-2.json"
inspection="$work_dir/inspection-compare.json"
run1_snapshot="$work_dir/work-run-1"
mkdir -p "$work_dir/work"

run_json "$plan" "$canon_bin" geo plan \
  --question "$inputs_dir/question.json" \
  --capabilities "$inputs_dir/capabilities.json" \
  --inventory "$inputs_dir/inventory.json" \
  --profile "$inputs_dir/profile.json" \
  --budget "$inputs_dir/budget.json"

run_json "$run1" "$canon_bin" geo run \
  --plan "$plan" \
  --work-dir "$work_dir/work" \
  --input "geo.building.home_cells:rows=$inputs_dir/home-cell-rows.json" \
  --input "geo.building.section:request=$inputs_dir/tile-work-request.json" \
  --input "geo.building.materialize_evidence:rows=$inputs_dir/warehouse-rows.json" \
  --input "geo.building.separation:request=$inputs_dir/separation-inputs.json" \
  --input "geo.building.next_evidence:request=$inputs_dir/next-evidence-inputs.json"

cp -R "$work_dir/work" "$run1_snapshot"

run_json "$run2" "$canon_bin" geo run \
  --plan "$plan" \
  --work-dir "$work_dir/work" \
  --input "geo.building.home_cells:rows=$inputs_dir/home-cell-rows.json" \
  --input "geo.building.section:request=$inputs_dir/tile-work-request.json" \
  --input "geo.building.materialize_evidence:rows=$inputs_dir/warehouse-rows-plus-one.json" \
  --input "geo.building.separation:request=$inputs_dir/separation-inputs.json" \
  --input "geo.building.next_evidence:request=$inputs_dir/next-evidence-inputs.json"

assert_jq '
  .project_run_report.resumed_nodes == [
    "geo.building.home_cells",
    "geo.building.section"
  ]
' "$run2"

assert_jq '
  (.project_run_report.executed_nodes | sort) == [
    "geo.building.compile_evidence",
    "geo.building.explain",
    "geo.building.materialize_evidence",
    "geo.building.next_evidence",
    "geo.building.propagate",
    "geo.building.separation",
    "geo.building.solve"
  ]
' "$run2"

assert_jq '
  .project_run_report.resource_reuse.saved_nodes == 2
  and (.project_run_report.resource_reuse.estimated_national_extrapolation == null)
' "$run2"

revision_count="$(find "$work_dir/work/.canon/geo-run/geo-run-manifest/revisions" -type f | wc -l | tr -d '[:space:]')"
if [[ "$revision_count" != "2" ]]; then
  echo "expected 2 immutable manifest revisions, got $revision_count" >&2
  exit 1
fi

run_json "$inspection" "$canon_bin" geo inspect \
  --run "$run1_snapshot" \
  --compare "$work_dir/work" \
  --emit json

assert_jq '
  any(.compare.evidence_added[]; startswith("geo.building.materialize_evidence/materialize_evidence@"))
  and .compare.model_count_before == .compare.model_count_after
' "$inspection"

log_run_digests "run-1" "$run1"
log_run_digests "run-2" "$run2"
jq -r '
  "resumed_nodes=" + (.project_run_report.resumed_nodes | join(",")),
  "executed_nodes=" + (.project_run_report.executed_nodes | join(",")),
  "resource_reuse.saved_nodes=" + (.project_run_report.resource_reuse.saved_nodes | tostring),
  "resource_reuse.estimated_national_extrapolation=null"
' "$run2" >>"$log"
jq -r '
  "inspect_compare.evidence_added=" + (.compare.evidence_added | join(",")),
  "inspect_compare.model_count_before=" + (.compare.model_count_before | tostring),
  "inspect_compare.model_count_after=" + (.compare.model_count_after | tostring)
' "$inspection" >>"$log"

echo "e2e_accretion=GREEN work_dir=$work_dir" | tee -a "$log"
