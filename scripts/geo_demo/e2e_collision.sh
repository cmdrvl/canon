#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s --work-dir DIR\n' "${0##*/}" >&2
  printf 'Runs the Canon Geo cross-deal collision report over two fixture ledgers.\n' >&2
}

work_dir=""
canon_bin="${CANON_BIN:-target/debug/canon}"

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

if [[ -z "$work_dir" ]]; then
  usage
  exit 64
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'e2e_collision requires jq on PATH\n' >&2
  exit 69
fi

if [[ ! -x "$canon_bin" ]]; then
  printf 'missing executable %s; build target/debug/canon or set CANON_BIN before running this harness\n' "$canon_bin" >&2
  exit 70
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"

mkdir -p "$work_dir"
log="$work_dir/run.log"
: >"$log"

blake3_file() {
  local path="$1"
  if command -v b3sum >/dev/null 2>&1; then
    b3sum "$path" | awk '{print $1}'
    return
  fi
  if [[ -n "${CANON_BLAKE3_BIN:-}" && -x "${CANON_BLAKE3_BIN:-}" ]]; then
    "$CANON_BLAKE3_BIN" "$path" | awk '{print $1}'
    return
  fi
  local rlib
  rlib="$(find "$repo_root/target/debug/deps" -maxdepth 1 -name 'libblake3-*.rlib' -print -quit 2>/dev/null || true)"
  if [[ -n "$rlib" ]] && command -v rustc >/dev/null 2>&1; then
    local helper="$work_dir/blake3_file"
    cat >"$work_dir/blake3_file.rs" <<'RS'
use std::{env, fs, process};

fn main() {
    let Some(path) = env::args().nth(1) else {
        process::exit(2);
    };
    let Ok(bytes) = fs::read(path) else {
        process::exit(2);
    };
    println!("{}", blake3::hash(&bytes).to_hex());
}
RS
    rustc --edition 2024 -L dependency="$repo_root/target/debug/deps" --extern blake3="$rlib" "$work_dir/blake3_file.rs" -o "$helper"
    "$helper" "$path"
    return
  fi
  printf 'no BLAKE3 helper found; set CANON_BLAKE3_BIN or run a Rust test once\n' >&2
  exit 2
}

log_digest() {
  local label="$1"
  local file="$2"
  local digest
  digest="$(blake3_file "$file")"
  printf '%s blake3:%s %s\n' "$label" "$digest" "$file" >>"$log"
}

run_json() {
  local out="$1"
  shift
  {
    printf '+'
    printf ' %q' "$@"
    printf ' > %q\n' "$out"
  } | tee -a "$log"
  set +e
  "$@" >"$out" 2>>"$log"
  local status=$?
  set -e
  printf 'exit_code=%s\n' "$status" | tee -a "$log"
  return "$status"
}

ledger_a="$work_dir/ledger-a.json"
ledger_b="$work_dir/ledger-b.json"
pari_passu="$work_dir/pari-passu.json"
adjacency="$work_dir/adjacency.json"
cross_deal="$work_dir/cross-deal.json"

jq -n --arg version "canon_geo_collateral_ledger.v0" '
{
  version: $version,
  proof_class: "fixture",
  rows: [
    {
      version: $version,
      accession: "fixture.d3.collision.a",
      deal_id: "fixture.d3.collision.deal.a",
      loan_id: "fixture.d3.collision.loan.a",
      reach: "full",
      parcel_set: ["1004540041", "1004540042"],
      building_set: ["1006494"],
      deed_ids: [],
      truth_plane: "gate_v2_historical",
      claim_class: "collateral_composition",
      residual_model_count: 1,
      count_exact: true,
      backbone_complete: true,
      source_release_pins: [
        {
          source_dataset: "fixture.d3.collision",
          source_release: "2026-09-02",
          blake3: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }
      ],
      composition_blake3: "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      evidence_blake3: "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      ambiguous_parcel_set: [],
      ambiguous_building_set: [],
      property_refs: [],
      composition_status: "resolved"
    }
  ],
  rollups: [
    {
      deal_id: "fixture.d3.collision.deal.a",
      accession: "fixture.d3.collision.a",
      rows: 1,
      truth_planes: {
        gate_v2_historical: {
          resolved: 1,
          ambiguous: 0,
          conflict: 0,
          reach_none: 0,
          budget_fallback: 0
        }
      }
    }
  ]
}' >"$ledger_a"

jq -n --arg version "canon_geo_collateral_ledger.v0" '
{
  version: $version,
  proof_class: "fixture",
  rows: [
    {
      version: $version,
      accession: "fixture.d3.collision.b",
      deal_id: "fixture.d3.collision.deal.b",
      loan_id: "fixture.d3.collision.loan.b",
      reach: "full",
      parcel_set: ["1004540041", "1004540042"],
      building_set: ["1006495"],
      deed_ids: [],
      truth_plane: "gate_v2_historical",
      claim_class: "collateral_composition",
      residual_model_count: 1,
      count_exact: true,
      backbone_complete: true,
      source_release_pins: [
        {
          source_dataset: "fixture.d3.collision",
          source_release: "2026-09-02",
          blake3: "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
        }
      ],
      composition_blake3: "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
      evidence_blake3: "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
      ambiguous_parcel_set: [],
      ambiguous_building_set: [],
      property_refs: [],
      composition_status: "resolved"
    }
  ],
  rollups: [
    {
      deal_id: "fixture.d3.collision.deal.b",
      accession: "fixture.d3.collision.b",
      rows: 1,
      truth_planes: {
        gate_v2_historical: {
          resolved: 1,
          ambiguous: 0,
          conflict: 0,
          reach_none: 0,
          budget_fallback: 0
        }
      }
    }
  ]
}' >"$ledger_b"

jq -n '
[
  {
    entity: {
      level: "parcel",
      id: "1004540041"
    },
    accessions: ["fixture.d3.collision.a", "fixture.d3.collision.b"],
    source_record: {
      source_record_id: "fixture.pari_passu.1004540041",
      source_vintage: "2026-09-02",
      record_blake3: "1111111111111111111111111111111111111111111111111111111111111111"
    }
  }
]' >"$pari_passu"

jq -n '{
  "1004540041": "1004540",
  "1004540042": "1004540"
}' >"$adjacency"

log_digest ledger_a "$ledger_a"
log_digest ledger_b "$ledger_b"
log_digest pari_passu "$pari_passu"
log_digest adjacency "$adjacency"

run_json "$cross_deal" "$canon_bin" geo ledger collision \
  --ledgers "$ledger_a" "$ledger_b" \
  --pari-passu "$pari_passu" \
  --adjacency "$adjacency"

jq -e '
  (.version == "canon_geo_cross_deal.v0")
  and (([.collisions[] | select(.kind == "shared_parcel")] | length) == 2)
  and ((.collisions[] | select(.entity.id == "1004540041") | .pari_passu) == true)
  and ((.collisions[] | select(.entity.id == "1004540042") | .pari_passu) == false)
  and ((.collisions[] | select(.entity.id == "1004540041") | .source_record.source_record_id) == "fixture.pari_passu.1004540041")
  and (.adjacency_concentration == [{
    block_id: "1004540",
    deal_count: 2,
    accessions: ["fixture.d3.collision.a", "fixture.d3.collision.b"],
    loan_ids: ["fixture.d3.collision.loan.a", "fixture.d3.collision.loan.b"],
    parcel_count: 2
  }])
' "$cross_deal" >/dev/null

log_digest cross_deal "$cross_deal"
jq -r '.collisions[] | [.kind, .entity.id, (.accessions | join(",")), .pari_passu, .explanation] | @tsv' "$cross_deal" >>"$log"
jq -r '.adjacency_concentration[] | [.block_id, .deal_count, (.accessions | join(",")), .parcel_count] | @tsv' "$cross_deal" >>"$log"

printf 'cross-deal artifact: %s\n' "$cross_deal"
