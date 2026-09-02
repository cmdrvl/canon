#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s\n' "${0##*/}" >&2
  printf 'Runs focused Canon Geo propagation checks without adding a public geo verb.\n' >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
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

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"

cargo test --manifest-path "$repo_root/Cargo.toml" \
  --test geo_propagate \
  t03_propagation_preserves_fixture_and_seeded_random_model_sets \
  -- --nocapture

cargo test --manifest-path "$repo_root/Cargo.toml" \
  --test geo_executor \
  geo_project_node_executor_runs_the_six_planner_leaf_chain \
  -- --nocapture
