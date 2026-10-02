#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'TEXT'
Usage: bash scripts/geo_demo/cornerstone.sh [--auto] [--recompute] [--step 1|2|3] [--work-dir DIR]

Walk through the retained Cornerstone property experiment, pausing between steps.
  --auto       Print every step without waiting for Enter.
  --recompute  Run Canon materialize/compile/solve on the retained adapter rows.
               Parcel solves can take several minutes each; no live data is fetched.
  --step       Show just one evidence step (default: all three).
  --work-dir   Keep artifacts in this new or empty directory (default: temporary).

Default mode reads hash-verified retained results and needs bash, jq, tar, and
sha256sum or shasum. Recompute also needs Cargo, or an explicit CANON_BIN.
TEXT
}

fail() { printf 'cornerstone: %s\n' "$*" >&2; exit 64; }
automatic=0
recompute=0
selected_step=all
work_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --auto) automatic=1; shift ;;
    --recompute) recompute=1; shift ;;
    --step)
      [[ $# -ge 2 ]] || fail '--step requires 1, 2, or 3'
      case "$2" in 1|2|3) selected_step="$2" ;; *) fail '--step requires 1, 2, or 3' ;; esac
      shift 2 ;;
    --work-dir)
      [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || fail '--work-dir requires a directory'
      work_dir="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

if [[ "$automatic" == 0 && ! -t 0 ]]; then
  fail 'interactive mode needs a terminal; use --auto for captured output'
fi
for dependency in jq tar; do
  command -v "$dependency" >/dev/null 2>&1 || fail "requires $dependency on PATH"
done
if command -v sha256sum >/dev/null 2>&1; then
  sha_command=(sha256sum)
elif command -v shasum >/dev/null 2>&1; then
  sha_command=(shasum -a 256)
else
  fail 'requires sha256sum or shasum on PATH'
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
experiment="$repo_root/scripts/geo_measurements/fixtures/cornerstone_national_2026-09-17"
archive="$experiment/run.tar.gz"
expected_hash="$(jq -er '.archives["run.tar.gz"].sha256' "$experiment/measurement.json")"
actual_hash="$("${sha_command[@]}" "$archive")"
[[ "${actual_hash%% *}" == "$expected_hash" ]] || fail 'retained archive SHA-256 mismatch'

if [[ -n "$work_dir" && -e "$work_dir" ]]; then
  [[ -d "$work_dir" ]] || fail '--work-dir must be a directory'
  shopt -s nullglob dotglob
  existing=("$work_dir"/*)
  [[ ${#existing[@]} == 0 ]] || fail '--work-dir must be empty; existing artifacts are preserved'
fi

canon_bin="${CANON_BIN:-}"
if [[ "$recompute" == 1 ]]; then
  if [[ -n "$canon_bin" ]]; then
    [[ -f "$canon_bin" && -x "$canon_bin" ]] || fail "invalid CANON_BIN: $canon_bin"
  else
    command -v cargo >/dev/null 2>&1 || fail 'recompute requires Cargo or CANON_BIN'
    target_dir="$(cargo metadata --quiet --no-deps --format-version 1 --manifest-path "$repo_root/Cargo.toml" | jq -er '.target_directory')"
    cargo build --quiet --locked --manifest-path "$repo_root/Cargo.toml" --bin canon
    canon_bin="$target_dir/debug/canon"
  fi
fi

if [[ -z "$work_dir" ]]; then
  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/canon-geo-cornerstone.XXXXXX")"
else
  mkdir -p -- "$work_dir"
fi
work_dir="$(cd -- "$work_dir" && pwd -P)"
mkdir "$work_dir/retained"

pause() {
  if [[ "$automatic" == 0 ]]; then
    printf '\nEnter to continue, or q to stop: '
    if ! IFS= read -r answer || [[ "$answer" == q || "$answer" == Q ]]; then
      printf '\nArtifacts kept at %s\n' "$work_dir"
      exit 0
    fi
  fi
}

run_json() {
  local output="$1"
  shift
  printf '  $ %q' "$canon_bin"
  printf ' %q' "$@"
  printf '\n'
  "$canon_bin" "$@" > "$output"
  jq -e . "$output" >/dev/null
}

show_step() {
  local number="$1" title="$2" variant="$3" level="$4"
  if [[ "$selected_step" != all && "$selected_step" != "$number" ]]; then
    return 0
  fi
  local retained="$work_dir/retained/$variant"
  local output="$work_dir/$variant"
  pause
  printf '\n%s. %s\n' "$number" "$title"
  tar -xzf "$archive" -C "$work_dir/retained" \
    "$variant/rows.json" \
    "$variant/run/geo/$level/compile_evidence.json" \
    "$variant/run/geo/$level/solve.json"
  mkdir "$output"
  local solve="$retained/run/geo/$level/solve.json"
  if [[ "$recompute" == 1 ]]; then
    run_json "$output/evidence.json" geo materialize-evidence --rows "$retained/rows.json"
    run_json "$output/compilation.json" geo compile-evidence --request "$output/evidence.json"
    run_json "$output/solve.json" geo solve --request "$output/compilation.json"
    solve="$output/solve.json"
  fi
  jq -e --arg level "$level" '
    if .version != "canon_geo_composition.v0" or .profile.selection_level != $level
    then error("unexpected solve artifact") else . end |
    (.soft_ranked | map(.cost) | min) as $best_cost |
    {
      grain: $level,
      solver_status: .status,
      candidates: .summary[($level + "_candidates")],
      hard_feasible_models: .summary.residual_model_count,
      exact_count: (.summary.residual_model_count_complete and
                    (.summary.residual_model_count_saturated | not)),
      best_cost: $best_cost,
      other_costs: ([.soft_ranked[] | select(.cost != $best_cost) | .cost] | unique),
      preferred_models: [.soft_ranked[] | select(.cost == $best_cost) | .model],
      hard_forced: .hard_forced
    }
  ' "$solve" > "$output/summary.json"
  jq -r '
    "  Candidates: \(.candidates) \(.grain)s",
    "  Hard-feasible alternatives: \(.hard_feasible_models) (exact count: \(.exact_count))",
    "  Equally preferred answers: \(.preferred_models | length)",
    (.preferred_models[] | "    " + ((.parcels + .buildings) | join(", "))),
    "  Preference cost: best \(.best_cost), other answers \(.other_costs | map(tostring) | join(", ")) (lower is better)",
    "  Native solver status: \(.solver_status)",
    "  Hard-forced members: \((.hard_forced.parcels + .hard_forced.buildings) | length)"
  ' "$output/summary.json"
  printf '  Inspect: %s\n' "$solve"
  case "$number" in
    1) printf '  Foursquare supplies the name, address, and point-to-parcel connection.\n' ;;
    2)
      printf '  Overture adds support; the county-derived address mirror earns no extra vote.\n'
      printf '  Preference costs are not probabilities or independent source counts.\n' ;;
    3)
      printf '  The retained Foursquare and county address points fall on different roofs.\n'
      printf '  Parcel support alone does not identify a unique building or complete site.\n' ;;
  esac
}

cat <<'TEXT'
CANON GEO — Which property does this evidence identify?

Subject: Cornerstone, 2409 S Conway Road, Orlando, Florida.
This known-subject experiment combines county parcels, Foursquare places,
and Overture places, addresses, and building footprints retained in September 2026.

The question is which single parcel or building is best supported. That premise
does not say the complete property consists of only one parcel or building.
TEXT
if [[ "$recompute" == 1 ]]; then
  printf '\nMode: fresh native stage execution over retained adapter rows.\n'
  "$canon_bin" --version
  printf 'Geometry adaptation and the full nine-stage workflow are not rerun here.\n'
else
  printf '\nMode: retained result readback; no solver execution or network acquisition.\n'
fi
printf 'Input archive SHA-256 verified: %s\nArtifacts: %s\n' "$expected_hash" "$work_dir"

show_step 1 'A place record supports a parcel' foursquare-parcel-supported parcel
show_step 2 'A second place record corroborates that parcel' national-parcel-supported parcel
show_step 3 'Ask the harder question: which building?' national-building-supported building

cat <<'TEXT'

What this demonstrates: preserve alternatives, combine traceable evidence,
and expose the difference between parcel support and building uncertainty.
These are soft preferences, not accepted identity or complete collateral extent.

The live version needs an agent with cmdrvl-data MCP access to acquire bounded
source rows and retain release pins, query receipts, and complete geometry bytes.
The experimental adapter then prepares Canon's offline inputs. This script
does not implement that acquisition loop or claim a fresh live measurement.
TEXT
printf '\nEvidence map: %s/footprints.svg\nFull replay instructions: %s/README.md\n' "$experiment" "$experiment"
printf 'Artifacts kept at %s\n' "$work_dir"
