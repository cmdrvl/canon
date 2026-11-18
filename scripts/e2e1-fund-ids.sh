#!/usr/bin/env bash
set -euo pipefail

# bd-z4eu: Canon E2E-1, the real BDC election-notice index through the public
# entity-session CLI. The work directory is deliberately retained. This
# script never deletes it and never redirects a command's stderr: every command
# has a retained stdout and command/status record while stderr remains visible
# to the operator. The export namespace below is a stable E2E projection
# namespace, not a registry namespace.

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
repo_dir="$(cd "$script_dir/.." && pwd -P)"
input_csv="$repo_dir/artifacts/e2e1-bdc-election/bdc_election_cik_names.csv"
receipt_json="$repo_dir/artifacts/e2e1-bdc-election/receipt.json"
canon_bin="${CANON_BIN:-}"
work_dir=""
proof_revision="${PROOF_REVISION:-}"

usage() {
  cat <<'EOF'
usage: scripts/e2e1-fund-ids.sh [options]

Runs the BDC election-notice index proof in a new retained work directory.
The directory must be empty if supplied and is never deleted by this script.

  --canon-bin PATH   canon executable (default: CANON_BIN, target/debug/canon,
                     target/release/canon, or canon on PATH)
  --input CSV        source CSV (default: artifacts/e2e1-bdc-election/...csv)
  --receipt JSON     source receipt (default: artifacts/e2e1-bdc-election/receipt.json)
  --work-dir DIR     retained, initially empty output directory
  PROOF_REVISION=... optional revision label recorded in the proof output
  --help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --canon-bin) canon_bin="${2:?missing canon executable}"; shift 2 ;;
    --input) input_csv="${2:?missing input CSV}"; shift 2 ;;
    --receipt) receipt_json="${2:?missing receipt JSON}"; shift 2 ;;
    --work-dir) work_dir="${2:?missing work directory}"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$canon_bin" ]]; then
  if [[ -x "$repo_dir/target/debug/canon" ]]; then
    canon_bin="$repo_dir/target/debug/canon"
  elif [[ -x "$repo_dir/target/release/canon" ]]; then
    canon_bin="$repo_dir/target/release/canon"
  else
    canon_bin="$(command -v canon || true)"
  fi
fi
[[ -n "$canon_bin" && -x "$canon_bin" ]] || {
  printf 'set CANON_BIN or --canon-bin to an executable canon binary\n' >&2
  exit 2
}
if [[ "$canon_bin" != /* ]]; then
  if [[ -x "$repo_dir/$canon_bin" ]]; then
    canon_bin="$repo_dir/$canon_bin"
  else
    canon_bin="$(command -v "$canon_bin" || true)"
  fi
fi
canon_bin="$(cd "$(dirname "$canon_bin")" && pwd -P)/$(basename "$canon_bin")"
[[ -f "$input_csv" ]] || { printf 'missing input CSV: %s\n' "$input_csv" >&2; exit 2; }
[[ -f "$receipt_json" ]] || { printf 'missing receipt JSON: %s\n' "$receipt_json" >&2; exit 2; }
command -v b3sum >/dev/null 2>&1 || { printf 'b3sum is required\n' >&2; exit 2; }
command -v sqlite3 >/dev/null 2>&1 || { printf 'sqlite3 is required\n' >&2; exit 2; }

if [[ -z "$work_dir" ]]; then
  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/canon-e2e1-fund-ids.XXXXXX")"
else
  mkdir -p "$work_dir"
fi
work_dir="$(cd "$work_dir" && pwd -P)"
if [[ -n "$(find "$work_dir" -mindepth 1 -print -quit)" ]]; then
  printf 'refusing non-empty retained work directory: %s\n' "$work_dir" >&2
  exit 2
fi
mkdir -p "$work_dir/commands"

safe_name() { printf '%s' "$1" | tr -c 'A-Za-z0-9_.-' '_'; }
run_json() {
  local label="$1"
  shift
  local name
  name="$(safe_name "$label")"
  local stdout_path="$work_dir/commands/$name.stdout"
  printf '%q ' "$@" >"$work_dir/commands/$name.command"
  printf '\n' >>"$work_dir/commands/$name.command"
  if [[ "$label" != check-id-* ]]; then
    printf '\n[%s] command:' "$label"
    printf ' %q' "$@"
    printf '\n'
  fi
  local status
  set +e
  "$@" >"$stdout_path"
  status="$?"
  set -e
  printf '%s\n' "$status" >"$work_dir/commands/$name.status"
  if [[ "$label" != check-id-* ]]; then
    printf '[%s] exit=%s stdout=%s\n' "$label" "$status" "$stdout_path"
  fi
  RUN_STATUS="$status"
  RUN_STDOUT_PATH="$stdout_path"
}
run_ok() {
  local label="$1"
  shift
  run_json "$label" "$@"
  [[ "$RUN_STATUS" -eq 0 ]] || {
    printf '[%s] unexpected exit=%s\n' "$label" "$RUN_STATUS" >&2
    exit "$RUN_STATUS"
  }
}
run_maybe() {
  local label="$1"
  shift
  run_json "$label" "$@"
  [[ "$RUN_STATUS" -eq 0 || "$RUN_STATUS" -eq 1 ]] || {
    printf '[%s] unexpected exit=%s (allowed statuses: 0 1)\n' "$label" "$RUN_STATUS" >&2
    exit "$RUN_STATUS"
  }
}

printf 'proof_class=real_data_public_command_replay\n'
if [[ -z "$proof_revision" ]]; then
  proof_revision="$(git -C "$repo_dir" rev-parse HEAD 2>/dev/null || printf unknown)"
fi
printf 'revision=%s\n' "$proof_revision"
printf 'work_dir=%s\n' "$work_dir"
printf 'served_registry_untouched=true\n'
printf 'operator_decision=entity_is_CIK; mint_all_non_conflicting_CIKs; flags_are_informational\n'

python3 - "$input_csv" "$receipt_json" "$work_dir/receipt-verification.json" <<'PY'
import csv
import hashlib
import json
import pathlib
import sys

csv_path, receipt_path, out_path = map(pathlib.Path, sys.argv[1:])
receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
with csv_path.open(newline="", encoding="utf-8") as handle:
    rows = list(csv.DictReader(handle))
required = {"cik", "company_name", "filing_types", "first_filed", "last_filed", "submissions"}
if not rows or set(rows[0]) != required:
    raise SystemExit(f"unexpected BDC CSV columns: {sorted(rows[0]) if rows else []}")
if any(set(row) != required for row in rows):
    raise SystemExit("BDC CSV row columns are not uniform")
sha256 = hashlib.sha256(csv_path.read_bytes()).hexdigest()
counts = {
    "rows": len(rows),
    "distinct_ciks": len({row["cik"] for row in rows}),
    "sum_submissions": sum(int(row["submissions"]) for row in rows),
    "cik_name_pairs": len({(row["cik"], row["company_name"]) for row in rows}),
    "csv_sha256": sha256,
}
expected = {**receipt.get("file_counts", {}), "csv_sha256": receipt.get("csv_sha256")}
actual = {
    "rows": counts["rows"],
    "distinct_ciks": counts["distinct_ciks"],
    "sum_submissions": counts["sum_submissions"],
    "csv_sha256": counts["csv_sha256"],
}
if actual != expected:
    raise SystemExit(json.dumps({"expected": expected, "actual": actual}, sort_keys=True))
if any(len(row["cik"]) != 10 or not row["cik"].isdigit() or set(row["cik"]) == {"0"} for row in rows):
    raise SystemExit("source CSV contains an invalid or placeholder CIK")
result = {
    "receipt": str(receipt_path),
    "source": str(csv_path),
    "expected": expected,
    "actual": counts,
    "warehouse_counts": receipt.get("warehouse_counts", {}),
    "verified": True,
}
out_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps({"receipt_verified": True, **counts}, sort_keys=True))
PY


python3 - "$input_csv" "$work_dir/observations.jsonl" "$work_dir/flags.jsonl" "$work_dir/source-manifest.json" <<'PY'
import csv
import hashlib
import json
import pathlib
import sys
from collections import Counter, defaultdict

input_path, observation_path, flags_path, manifest_path = map(pathlib.Path, sys.argv[1:])
rows = list(csv.DictReader(input_path.open(newline="", encoding="utf-8")))
individual_ciks = {"0001131059", "0001589829"}
observations = []
flag_rows = []
by_cik = defaultdict(list)
for index, row in enumerate(rows, start=1):
    cik = row["cik"]
    name = row["company_name"]
    types = row["filing_types"].split("+")
    flags = []
    if all(form in {"N-6F", "N-6F/A"} for form in types):
        flags.append("n6f_only")
    if all(form in {"N-54C", "N-54C/A"} for form in types):
        flags.append("withdrawn_only")
    if "TEST" in name.upper():
        flags.append("test_row")
    if cik in individual_ciks:
        flags.append("individual_filer")
    obs_id = "bdc-election-index:{}:{}".format(
        cik, hashlib.sha256(f"{cik}\0{name}".encode()).hexdigest()[:16]
    )
    observations.append({
        "obs_id": obs_id,
        "entity_type": "fund",
        "surface": name,
        "keys": {"cik": cik},
        "attrs": {
            "filing_types": row["filing_types"],
            "first_filed": row["first_filed"],
            "last_filed": row["last_filed"],
        },
        "source": {
            "channel": "file",
            "id": "bdc-election-index",
            "file": input_path.name,
            "lineage": f"bdc-election-index:cik:{cik}",
        },
    })
    flag_rows.append({
        "row": index,
        "cik": cik,
        "company_name": name,
        "filing_types": types,
        "flags": flags,
    })
    by_cik[cik].append(row)
observation_path.write_text(
    "".join(json.dumps(row, sort_keys=True) + "\n" for row in observations), encoding="utf-8"
)
flags_path.write_text(
    "".join(json.dumps(row, sort_keys=True) + "\n" for row in flag_rows), encoding="utf-8"
)
manifest = {
    "source_rows": len(rows),
    "observation_rows": len(observations),
    "distinct_ciks": len(by_cik),
    "ciks_with_multiple_names": sum(
        len({row["company_name"] for row in values}) > 1 for values in by_cik.values()
    ),
    "names_with_multiple_ciks": len({
        name for name in {row["company_name"] for row in rows}
        if len({row["cik"] for row in rows if row["company_name"] == name}) > 1
    }),
    "flag_counts": dict(sorted(Counter(flag for row in flag_rows for flag in row["flags"]).items())),
    "flags_path": str(flags_path),
    "observation_schema": "canon.entity.session.observation.v1",
}
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(manifest, sort_keys=True))
PY

registry="$work_dir/registry"
cat >"$work_dir/registries.yaml" <<EOF
version: canon.session.registries.v1
kinds:
  fund:
    registry: $registry
    registry_id: funds
    id_scheme: agent
    kind_prefix: fund
    canonical_type: fund
    alias_file: aliases.json
EOF
cat >"$work_dir/rules.yaml" <<'EOF'
version: canon.entity.session.rules.v1
keys:
  authoritative: [cik]
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
  split_shared_names: true
incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
EOF
cat >"$work_dir/profile.yaml" <<'EOF'
profile: e2e1_bdc_election_fund
version: 0.1.0
entity_type: fund
identity_semantics: same_normalized_name
canonical_type: fund
required_fields: [source_row_id, surface]
normalized_views:
  core:
    operators: [ascii_trim_upper]
  row_key:
    operators: [ascii_trim_upper]
prepare:
  primary_surface_fields: [surface]
  canonical_surface_normalized_view: core
  normalized_view_fields:
    core: surface
    row_key: source_row_id
  anchor_fields:
    cik: cik
  surface_key_fields: [row_key]
  nullable_fields: [cik]
  provenance_fields: [source_row_id, origin]
blocking:
  operators:
    - op: exact_identity
      view: core
evidence:
  support:
    - op: exact_view
      view: core
  cannot_link:
    - op: protected_token_conflict
      view: core
      params:
        score_units: "0"
  relation_hints:
    - op: cross_profile_alignment
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: e2e1_bdc_election_fund.aliases
  distinct: e2e1_bdc_election_fund.distinct
  relations: e2e1_bdc_election_fund.relations
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
EOF

cat >"$work_dir/strategy.yaml" <<'EOF'
block:
  preflight: false
  max_candidates_per_run: 5000000
EOF

obs_digest="blake3:$(b3sum "$work_dir/observations.jsonl" | awk '{print $1}')"
session="$work_dir/session"
run_ok "session-init" "$canon_bin" entity session init \
  --goal "E2E-1 BDC election notice fund ids" \
  --registries "$work_dir/registries.yaml" \
  --work-dir "$session" --emit json
run_ok "session-ingest" "$canon_bin" entity session ingest "$session" "$work_dir/observations.jsonl" \
  --expect-digest "$obs_digest" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/ingest.json"
python3 - "$work_dir/ingest.json" "$work_dir/observations.jsonl" <<'PY'
import json
import pathlib
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text())
rows = [line for line in pathlib.Path(sys.argv[2]).read_text().splitlines() if line.strip()]
if value.get("accepted") != len(rows) or value.get("rejected"):
    raise SystemExit(f"BDC observations were not all accepted: {value}")
PY
run_ok "session-derive" "$canon_bin" entity session derive "$session" \
  --profile "$work_dir/profile.yaml" --strategy "$work_dir/strategy.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/derive.json"
run_ok "session-solve" "$canon_bin" entity session solve "$session" --rules "$work_dir/rules.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/solve.json"

run_ok "session-commit" "$canon_bin" entity session commit "$session" \
  --next-version 1.0.0 --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/commit.json"
python3 - "$work_dir/commit.json" "$work_dir/holdbacks.json" "$work_dir/observations.jsonl" "$work_dir/solve.json" <<'PY'
import json
import pathlib
import sys

commit_path, holdback_path, observations_path, solve_path = map(pathlib.Path, sys.argv[1:])
commit = json.loads(commit_path.read_text())
solve = json.loads(solve_path.read_text())
if commit.get("outcome") != "committed":
    raise SystemExit(f"first commit did not commit: {commit}")
observations = {}
for line in observations_path.read_text().splitlines():
    if line.strip():
        row = json.loads(line)
        observations[row["obs_id"]] = row
rows = []
clusters = {cluster["cluster_id"]: cluster for cluster in solve.get("clusters", [])}
for skipped in commit.get("skipped", []):
    cluster = clusters.get(skipped.get("cluster_id"), {})
    cluster_ciks = {
        str(value).lstrip("0") or "0"
        for values in cluster.get("keys", {}).values()
        for value in values
    }
    ids = skipped.get("obs_ids") or skipped.get("observation_ids") or []
    if not ids and skipped.get("obs_id"):
        ids = [skipped["obs_id"]]
    mapped = [
        row for row in observations.values()
        if row["keys"].get("cik", "").lstrip("0") in cluster_ciks
    ]
    rows.append({
        "reason": skipped.get("reason"),
        "detail": skipped,
        "obs_ids": ids,
        "cluster_keys": cluster.get("keys", {}),
        "cluster_surfaces": cluster.get("surfaces", []),
        "rows": [
            {"obs_id": row["obs_id"], "cik": row["keys"].get("cik"), "surface": row["surface"]}
            for row in mapped
        ],
    })
holdback_path.write_text(json.dumps(rows, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps({
    "minted": len(commit.get("minted", [])),
    "attached": len(commit.get("attached", [])),
    "skipped": len(rows),
    "skip_reasons": sorted({row.get("reason") for row in rows}),
}, sort_keys=True))
PY

run_ok "registry-lint" "$canon_bin" registry lint "$registry" --emit json
run_ok "registry-key-crosswalk" "$canon_bin" registry export --registry "$registry" \
  --format key-crosswalk --out "$work_dir/key-crosswalk.csv" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/key-crosswalk.json"
run_ok "evidence-closure-export" "$canon_bin" entity session export "$session" \
  --format evidence-closure --out "$work_dir/evidence-closures" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/evidence-closure.json"

python3 - "$registry" "$work_dir/commit.json" "$work_dir/evidence-closure.json" "$work_dir/evidence-closure-summary.json" <<'PY'
import json
import pathlib
import subprocess
import sys

registry, commit_path, export_path, summary_path = map(pathlib.Path, sys.argv[1:])
commit = json.loads(commit_path.read_text())
export = json.loads(export_path.read_text())
closures = export.get("closures")
if not isinstance(closures, list) or not closures:
    raise SystemExit("evidence-closure export emitted no closures")
emitted = {row.get("closure_hash") for row in closures}
if None in emitted:
    raise SystemExit("evidence-closure export has a closure without closure_hash")

def walk(value):
    found = set()
    if isinstance(value, dict):
        if isinstance(value.get("closure_hash"), str):
            found.add(value["closure_hash"])
        for child in value.values():
            found |= walk(child)
    elif isinstance(value, list):
        for child in value:
            found |= walk(child)
    return found

commit_hashes = walk(commit)
if not commit_hashes <= emitted:
    raise SystemExit(f"commit closure hashes not emitted: {sorted(commit_hashes - emitted)}")
levels = {}
closure_root = pathlib.Path(export["out"])
for row in closures:
    level = row.get("sensitivity", {}).get("level")
    if level not in {"public", "restricted"}:
        raise SystemExit(f"closure has no valid sensitivity level: {row}")
    levels[level] = levels.get(level, 0) + 1
    digest = row["closure_hash"].removeprefix("blake3:")
    directory = row.get("directory")
    if directory != digest:
        raise SystemExit(f"closure directory is not its hash: {row}")
    closure_path = closure_root / directory / "closure.json"
    if not closure_path.is_file():
        raise SystemExit(f"missing emitted closure: {closure_path}")
    actual = subprocess.check_output(["b3sum", str(closure_path)], text=True).split()[0]
    if actual != digest:
        raise SystemExit(f"closure hash mismatch: {closure_path}")
registry_hashes = set()
for path in sorted(registry.rglob("*.json")):
    try:
        values = [json.loads(path.read_text())]
    except json.JSONDecodeError:
        continue
    for value in values:
        registry_hashes |= walk(value)
missing = registry_hashes - emitted
if missing:
    raise SystemExit(f"registry closure hashes not emitted: {sorted(missing)}")
summary = {
    "commit_closure_hashes": len(commit_hashes),
    "emitted_closures": len(emitted),
    "registry_closure_hashes": len(registry_hashes),
    "all_registry_closure_hashes_emitted": True,
    "sensitivity_levels": dict(sorted(levels.items())),
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(summary, sort_keys=True))
PY

python3 - "$work_dir/commit.json" "$work_dir/padded-key-aliases.jsonl" <<'PY'
import json
import pathlib
import sys

commit_path, output_path = map(pathlib.Path, sys.argv[1:])
commit = json.loads(commit_path.read_text())
rows = []
for minted in commit.get("minted", []):
    for alias in minted.get("aliases", []):
        if alias.get("alias_file") != "keys.cik.json":
            continue
        value = alias.get("input", "")
        if not value.isdigit() or value == "0" * 10:
            continue
        padded = value.zfill(10)
        if padded == value:
            continue
        rows.append({
            "canonical_id": minted["canonical_id"],
            "alias_file": "keys.cik.json",
            "input": padded,
            "rule_id": "SESSION_KEY_CIK_PADDED",
        })
output_path.write_text(
    "".join(json.dumps(row, sort_keys=True) + "\n" for row in sorted(rows, key=lambda row: row["input"])),
    encoding="utf-8",
)
print(json.dumps({"padded_key_aliases": len(rows)}, sort_keys=True))
PY
run_ok "registry-add-padded-cik-aliases" "$canon_bin" registry add-entries \
  --registry "$registry" --from "$work_dir/padded-key-aliases.jsonl" \
  --next-version 1.0.1 --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/padded-key-aliases.json"
run_ok "registry-key-crosswalk-padded" "$canon_bin" registry export --registry "$registry" \
  --format key-crosswalk --out "$work_dir/key-crosswalk.csv" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/key-crosswalk-padded.json"

python3 - "$input_csv" "$work_dir/cik-lookup.jsonl" "$work_dir/name-lookup.jsonl" <<'PY'
import csv
import json
import pathlib
import sys

input_path, cik_path, name_path = map(pathlib.Path, sys.argv[1:])
rows = list(csv.DictReader(input_path.open(newline="", encoding="utf-8")))
cik_path.write_text(
    "".join(
        json.dumps({"cik": value}, sort_keys=True) + "\n"
        for value in sorted({row["cik"] for row in rows})
    ),
    encoding="utf-8",
)
name_path.write_text(
    "".join(
        json.dumps({"name": row["company_name"], "cik": row["cik"]}, sort_keys=True) + "\n"
        for row in rows
    ),
    encoding="utf-8",
)
PY
run_maybe "plain-cik-lookup" "$canon_bin" "$work_dir/cik-lookup.jsonl" --registry "$registry" \
  --column cik --explicit --no-witness --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/cik-lookup.json"
run_maybe "plain-name-lookup" "$canon_bin" "$work_dir/name-lookup.jsonl" --registry "$registry" \
  --column name --explicit --no-witness --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/name-lookup.json"

python3 - "$input_csv" "$work_dir/commit.json" "$work_dir/solve.json" "$work_dir/cik-lookup.json" "$work_dir/name-lookup.json" "$work_dir/key-crosswalk.csv" "$work_dir/lookup-summary.json" <<'PY'
import csv
import json
import pathlib
import sys

csv_path, commit_path, solve_path, cik_lookup_path, name_lookup_path, crosswalk_path, summary_path = map(pathlib.Path, sys.argv[1:])
source = list(csv.DictReader(csv_path.open(newline="", encoding="utf-8")))
commit = json.loads(commit_path.read_text())
solve = json.loads(solve_path.read_text())
cik_lookup = json.loads(cik_lookup_path.read_text())
name_lookup = json.loads(name_lookup_path.read_text())

def raw(value):
    return value.removeprefix("u8:") if isinstance(value, str) else value

minted = {row["canonical_id"] for row in commit.get("minted", [])}
holdbacks = json.loads((commit_path.parent / "holdbacks.json").read_text())
skipped_ciks = {row["cik"] for group in holdbacks for row in group.get("rows", [])}
expected_ciks = {row["cik"] for row in source} - skipped_ciks
cik_map = {raw(row["input"]): raw(row["canonical_id"]) for row in cik_lookup.get("mappings", [])}
unresolved_ciks = {raw(row["input"]) for row in cik_lookup.get("unresolved", [])}
name_map = {raw(row["input"]): raw(row["canonical_id"]) for row in name_lookup.get("mappings", [])}
shared_groups = solve.get("shared_names")
if not isinstance(shared_groups, list) or not shared_groups:
    raise SystemExit("solve did not report shared_names for the split_shared_names proof")
shared_names = {group.get("surface") for group in shared_groups}
if None in shared_names or not all(isinstance(name, str) and name for name in shared_names):
    raise SystemExit(f"solve shared_names have invalid surfaces: {shared_groups}")
expected_shared_names = {"ALLIED CAPITAL CORP", "INFINITY CAPITAL GROUP INC"}
if shared_names != expected_shared_names:
    raise SystemExit(f"unexpected shared names: {sorted(shared_names)}")
commit_shared_groups = commit.get("shared_names")
if not isinstance(commit_shared_groups, list):
    raise SystemExit("commit did not report shared_names")
if {group.get("surface") for group in commit_shared_groups} != shared_names:
    raise SystemExit(f"commit shared_names differ from solve: {commit_shared_groups}")
for group in commit_shared_groups:
    for member in group.get("members", []):
        if not isinstance(member.get("canonical_id"), str) or member["canonical_id"] not in minted:
            raise SystemExit(f"shared-name member was not minted: {group}")
if holdbacks:
    raise SystemExit(f"split_shared_names unexpectedly held back clusters: {holdbacks}")
for cik in sorted(expected_ciks):
    if cik not in cik_map or cik_map[cik] not in minted:
        raise SystemExit(f"CIK did not resolve to a minted id: {cik}: {cik_map.get(cik)}")
for cik in sorted(skipped_ciks):
    if cik in cik_map:
        raise SystemExit(f"held-back CIK unexpectedly resolved: {cik}")
for row in source:
    name = row["company_name"]
    if name in shared_names:
        if raw(name) in name_map:
            raise SystemExit(f"shared name unexpectedly resolved: {name!r}: {name_map[raw(name)]}")
        continue
    if raw(name) not in name_map or name_map[raw(name)] != cik_map.get(row["cik"]):
        raise SystemExit(f"historical name did not resolve to its CIK id: {row}")
if len(minted) != len(expected_ciks):
    raise SystemExit(f"minted {len(minted)} ids for {len(expected_ciks)} non-conflicting CIKs")
if len(cik_map) != len(expected_ciks):
    raise SystemExit(f"CIK lookup resolved {len(cik_map)} values, expected {len(expected_ciks)}")
if unresolved_ciks:
    raise SystemExit(f"unexpected unresolved CIKs: {sorted(unresolved_ciks)}")
crosswalk = list(csv.DictReader(crosswalk_path.open(newline="", encoding="utf-8")))
crosswalk_ids = {}
for row in crosswalk:
    for value in row.get("cik", "").split(";") + row.get("cik_padded", "").split(";"):
        if value:
            if value in crosswalk_ids and crosswalk_ids[value] != row["canonical_id"]:
                raise SystemExit(f"CIK appears under multiple ids: {value}")
            crosswalk_ids[value] = row["canonical_id"]
expected_unpadded = {value.lstrip("0") for value in expected_ciks}
expected_padded = set(expected_ciks)
expected_crosswalk_values = expected_unpadded | expected_padded
if set(crosswalk_ids) != expected_crosswalk_values:
    raise SystemExit("key crosswalk does not contain both canonical and zero-padded non-conflicting CIKs")
for value in expected_ciks:
    if crosswalk_ids[value] != crosswalk_ids[value.lstrip("0")]:
        raise SystemExit(f"padded and canonical CIKs resolve to different ids: {value}")
summary = {
    "source_ciks": len({row["cik"] for row in source}),
    "held_back_ciks": len(skipped_ciks),
    "minted_ids": len(minted),
    "plain_cik_resolved": len(cik_map),
    "plain_cik_unresolved": len(unresolved_ciks),
    "historical_name_rows": len(source),
    "shared_names_unresolved": sorted(shared_names),
    "historical_name_unresolved_expected": len([row for row in source if row["company_name"] in shared_names]),
    "one_id_per_cik": True,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(summary, sort_keys=True))
PY

export_namespace="e2e1_bdc_election_fund"
exports_dir="$work_dir/exports"
exports_repeat_dir="$work_dir/exports-repeat"

run_export_set() {
  local label="$1"
  local output_dir="$2"
  mkdir -p "$output_dir/entity-cards"
  (
    cd "$work_dir"
    run_ok "${label}-dbt-seed" "$canon_bin" registry export \
      --format dbt-seed --registry registry --namespace "$export_namespace" \
      --canonical-type fund --out "$output_dir/dbt-seed.csv" \
      --schema-out "$output_dir/schema.yml" \
      --anti-collapse-test-out "$output_dir/anti-collapse.sql" --emit json
  )
  cp "$work_dir/commands/${label}-dbt-seed.stdout" "$output_dir/dbt-seed.json"
  (
    cd "$work_dir"
    run_ok "${label}-search-index" "$canon_bin" registry export \
      --format search-index --registry registry --namespace "$export_namespace" \
      --out "$output_dir/search-index.sqlite" --emit json
  )
  cp "$work_dir/commands/${label}-search-index.stdout" "$output_dir/search-index.json"
  (
    cd "$work_dir"
    run_ok "${label}-search-index-sqlite-open" sqlite3 "$output_dir/search-index.sqlite" ".tables"
  )
  (
    cd "$work_dir"
    run_ok "${label}-entity-card" "$canon_bin" registry export \
      --format entity-card --registry registry --out "$output_dir/entity-cards" --emit json
  )
  cp "$work_dir/commands/${label}-entity-card.stdout" "$output_dir/entity-card.json"
}

run_export_set "export-1" "$exports_dir"
run_export_set "export-2" "$exports_repeat_dir"
python3 - "$registry" "$work_dir/commit.json" "$input_csv" "$exports_dir" "$exports_repeat_dir" "$work_dir/exports-summary.json" <<'PY'
import csv
import hashlib
import json
import pathlib
import sqlite3
import sys

registry, commit_path, source_path, first_root, second_root, summary_path = map(
    pathlib.Path, sys.argv[1:]
)
commit = json.loads(commit_path.read_text(encoding="utf-8"))
minted = {row["canonical_id"] for row in commit.get("minted", [])}
registry_version = json.loads((registry / "registry.json").read_text(encoding="utf-8"))["version"]
source_rows = list(csv.DictReader(source_path.open(newline="", encoding="utf-8")))
renamed_names = sorted(
    {row["company_name"] for row in source_rows if row["cik"] == "0000748268"}
)
if len(renamed_names) < 2:
    raise SystemExit(f"CIK 748268 is not a renamed CIK in the source: {renamed_names}")

mapping_counts = {}
for path in sorted(registry.glob("*.json")):
    if path.name == "registry.json":
        continue
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, list):
        raise SystemExit(f"mapping file is not a JSON array: {path}")
    mapping_counts[path.name] = len(value)
expected_entries = sum(mapping_counts.values())

def artifact_snapshot(root):
    paths = [
        root / "dbt-seed.csv",
        root / "schema.yml",
        root / "anti-collapse.sql",
        root / "search-index.sqlite",
        *sorted((root / "entity-cards").glob("*.json")),
    ]
    if any(not path.is_file() for path in paths):
        missing = [str(path) for path in paths if not path.is_file()]
        raise SystemExit(f"export artifact is missing: {missing}")
    return {
        path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in paths
    }

def verify_export(root):
    payloads = {
        name: json.loads((root / name).read_text(encoding="utf-8"))
        for name in ("dbt-seed.json", "search-index.json", "entity-card.json")
    }
    versions = {payload["registry"]["version"] for payload in payloads.values()}
    if versions != {registry_version}:
        raise SystemExit(f"export registry versions drifted: {versions} vs {registry_version}")
    seed_rows = list(csv.DictReader((root / "dbt-seed.csv").open(newline="", encoding="utf-8")))
    if len(seed_rows) != expected_entries:
        raise SystemExit(f"dbt-seed rows {len(seed_rows)} != expected alias+key entries {expected_entries}")
    if payloads["dbt-seed.json"]["summary"]["exported_alias_count"] != expected_entries:
        raise SystemExit("dbt-seed summary count disagrees with alias+key entries")
    if any(row["canonical_id"] not in minted for row in seed_rows):
        raise SystemExit("dbt-seed contains a canonical id that was not minted")
    if any(not row["canonical_id"].startswith("fund_") for row in seed_rows):
        raise SystemExit("dbt-seed contains a non-fund canonical id")
    if any(row["namespace"] != "e2e1_bdc_election_fund" for row in seed_rows):
        raise SystemExit("dbt-seed namespace is not the declared stable namespace")

    by_key = {}
    for row in seed_rows:
        key = (row["namespace"], row["normalized_key"])
        by_key.setdefault(key, set()).add(row["canonical_id"])
    collapsed = {key: sorted(ids) for key, ids in by_key.items() if len(ids) > 1}
    if collapsed:
        raise SystemExit(f"dbt anti-collapse check found multiple ids: {collapsed}")
    anti_sql = (root / "anti-collapse.sql").read_text(encoding="utf-8").lower()
    if "count(distinct canonical_id) > 1" not in anti_sql:
        raise SystemExit("generated anti-collapse SQL does not assert distinct canonical ids")

    search_path = root / "search-index.sqlite"
    with sqlite3.connect(search_path) as connection:
        tables = {
            row[0]
            for row in connection.execute("select name from sqlite_master where type = 'table'")
        }
        required_tables = {"metadata", "entities", "aliases", "external_keys"}
        if not required_tables <= tables:
            raise SystemExit(f"search index is missing tables: {sorted(required_tables - tables)}")
        search_version = connection.execute(
            "select value from metadata where key = 'registry_version'"
        ).fetchone()[0]
        if search_version != registry_version:
            raise SystemExit(f"search index version {search_version} != {registry_version}")
        cik_ids = connection.execute(
            "select distinct canonical_id from external_keys where key_namespace = 'key' and key_value = ?",
            ("0000748268",),
        ).fetchall()
        if len(cik_ids) != 1:
            raise SystemExit(f"search index CIK lookup was not unique: {cik_ids}")
        renamed_id = cik_ids[0][0]
        name_ids = {}
        for name in renamed_names:
            ids = connection.execute(
                "select distinct canonical_id from aliases where alias = ?", (name,)
            ).fetchall()
            if len(ids) != 1 or ids[0][0] != renamed_id:
                raise SystemExit(f"search index name lookup failed for {name!r}: {ids}")
            name_ids[name] = ids[0][0]

    card_paths = sorted((root / "entity-cards").glob("*.json"))
    if len(card_paths) != len(minted):
        raise SystemExit(f"entity cards {len(card_paths)} != minted ids {len(minted)}")
    sizes = []
    renamed_card = None
    for path in card_paths:
        card = json.loads(path.read_text(encoding="utf-8"))
        if card.get("id") != path.stem or card.get("version") != "canon.entity_card.v0":
            raise SystemExit(f"entity card identity/version mismatch: {path}")
        sizes.append(path.stat().st_size)
        if card["id"] == renamed_id:
            renamed_card = card
    if renamed_card is None or not set(renamed_names) <= set(renamed_card.get("names", [])):
        raise SystemExit(f"renamed CIK card does not list both names: {renamed_card}")
    return {
        "registry_version": registry_version,
        "dbt_seed_rows": len(seed_rows),
        "expected_alias_key_entries": expected_entries,
        "mapping_file_counts": mapping_counts,
        "dbt_seed_count_matches_expected": True,
        "dbt_seed_canonical_ids": len({row["canonical_id"] for row in seed_rows}),
        "anti_collapse_keys_checked": len(by_key),
        "anti_collapse_collisions": 0,
        "search_index_tables": sorted(tables),
        "search_index_cik": {"cik": "0000748268", "canonical_id": renamed_id},
        "search_index_renamed_names": name_ids,
        "entity_card_count": len(card_paths),
        "entity_card_bytes": {
            "min": min(sizes),
            "max": max(sizes),
            "total": sum(sizes),
        },
        "renamed_cik_card": {"cik": "0000748268", "canonical_id": renamed_id, "names": renamed_names},
    }

first_summary = verify_export(first_root)
second_summary = verify_export(second_root)
first_files = artifact_snapshot(first_root)
second_files = artifact_snapshot(second_root)
if first_files != second_files:
    differences = sorted(set(first_files) ^ set(second_files))
    differences += sorted(
        name for name in set(first_files) & set(second_files) if first_files[name] != second_files[name]
    )
    raise SystemExit(f"two exports from the same registry version differ: {differences}")
summary = {
    "namespace": "e2e1_bdc_election_fund",
    "registry_version": registry_version,
    "first": first_summary,
    "second": second_summary,
    "two_exports_byte_identical": True,
    "artifact_files_compared": len(first_files),
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(summary, sort_keys=True))
PY

python3 - "$registry" "$work_dir/commit.json" "$work_dir/check-id" <<'PY'
import json
import pathlib
import sys

_, commit_path, out_dir = map(pathlib.Path, sys.argv[1:])
out_dir.mkdir()
ids = sorted({row["canonical_id"] for row in json.loads(commit_path.read_text()).get("minted", [])})
(out_dir / "ids.txt").write_text("".join(f"{value}\n" for value in ids), encoding="utf-8")
print(json.dumps({"ids_to_check": len(ids)}, sort_keys=True))
PY
while IFS= read -r canonical_id; do
  run_ok "check-id-$canonical_id" "$canon_bin" registry check-id --registry "$registry" "$canonical_id" --emit json
  cp "$RUN_STDOUT_PATH" "$work_dir/check-id/$canonical_id.json"
done <"$work_dir/check-id/ids.txt"

run_ok "same-session-solve-refresh" "$canon_bin" entity session solve "$session" --rules "$work_dir/rules.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/same-session-solve.json"

python3 - "$registry" "$work_dir/registry-before-noop.json" <<'PY'
import hashlib
import json
import pathlib
import sys
root, out = map(pathlib.Path, sys.argv[1:])
value = {}
for path in sorted(root.rglob("*")):
    if path.is_file() and path.name != "_index.sqlite":
        value[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
out.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
run_maybe "same-session-no-op-commit" "$canon_bin" entity session commit "$session" \
  --bump patch --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/same-session-no-op.json"
same_session_check_status=0
if ! python3 - "$work_dir/same-session-no-op.json" "$work_dir/registry-before-noop.json" "$registry" "$work_dir/same-session-repeat-defect.json" <<'PY'
import hashlib
import json
import pathlib
import sys
receipt, before_path, root, defect_path = map(pathlib.Path, sys.argv[1:])
value = json.loads(receipt.read_text())
before = json.loads(before_path.read_text())
after = {}
for path in sorted(root.rglob("*")):
    if path.is_file() and path.name != "_index.sqlite":
        after[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
failures = []
if value.get("outcome") != "nothing_to_commit":
    failures.append({"field": "outcome", "expected": "nothing_to_commit", "actual": value.get("outcome")})
if value.get("minted"):
    failures.append({"field": "minted", "expected": [], "actual_count": len(value["minted"])})
if value.get("attached"):
    failures.append({"field": "attached", "expected": [], "actual_count": len(value["attached"])})
if before != after:
    failures.append({"field": "registry_bytes", "expected": "unchanged", "actual": "changed"})
if failures:
    defect = {
        "repeat": "same_session",
        "failures": failures,
        "observed": {
            "outcome": value.get("outcome"),
            "attached": len(value.get("attached", [])),
            "minted": len(value.get("minted", [])),
            "changed_files": value.get("registry", {}).get("changed_files"),
        },
    }
    defect_path.write_text(json.dumps(defect, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(defect, sort_keys=True))
    raise SystemExit(1)
print(json.dumps({
    "same_session_no_op": True,
    "outcome": "nothing_to_commit",
    "attached": 0,
    "minted": 0,
}, sort_keys=True))
PY
then
  same_session_check_status=1
  printf '[same-session] strict no-write assertion failed; continuing to complete the retained proof\n' >&2
fi

second_session="$work_dir/second-session"
cp "$registry/aliases.json" "$work_dir/aliases-before-second-session.json"
python3 - "$registry" "$work_dir/registry-before-second-session.json" <<'PY'
import hashlib
import json
import pathlib
import sys
root, out = map(pathlib.Path, sys.argv[1:])
value = {}
for path in sorted(root.rglob("*")):
    if path.is_file() and path.name != "_index.sqlite":
        value[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
out.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
run_ok "second-session-init" "$canon_bin" entity session init --goal "E2E-1 BDC repeat session" \
  --registries "$work_dir/registries.yaml" --work-dir "$second_session" --emit json
run_ok "second-session-ingest" "$canon_bin" entity session ingest "$second_session" "$work_dir/observations.jsonl" \
  --expect-digest "$obs_digest" --emit json
run_ok "second-session-derive" "$canon_bin" entity session derive "$second_session" \
  --profile "$work_dir/profile.yaml" --strategy "$work_dir/strategy.yaml" --emit json
run_ok "second-session-solve" "$canon_bin" entity session solve "$second_session" --rules "$work_dir/rules.yaml" --emit json
run_maybe "second-session-commit" "$canon_bin" entity session commit "$second_session" --bump patch --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/second-session-commit.json"
second_session_check_status=0
if ! python3 - "$work_dir/second-session-commit.json" "$work_dir/aliases-before-second-session.json" "$registry/aliases.json" "$work_dir/registry-before-second-session.json" "$registry" "$work_dir/second-session-repeat-defect.json" <<'PY'
import json
import hashlib
import pathlib
import sys
value, before_path, aliases, before_registry_path, registry_root, defect_path = map(pathlib.Path, sys.argv[1:])
commit = json.loads(value.read_text())
before = before_path.read_bytes()
after = aliases.read_bytes()
before_registry = json.loads(before_registry_path.read_text())
after_registry = {}
for path in sorted(registry_root.rglob("*")):
    if path.is_file() and path.name != "_index.sqlite":
        after_registry[path.relative_to(registry_root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
alias_rows = json.loads(after)
alias_inputs = [row.get("input") for row in alias_rows]
failures = []
if commit.get("outcome") != "nothing_to_commit":
    failures.append({"field": "outcome", "expected": "nothing_to_commit", "actual": commit.get("outcome")})
if commit.get("minted"):
    failures.append({"field": "minted", "expected": [], "actual_count": len(commit["minted"])})
if commit.get("attached"):
    failures.append({"field": "attached", "expected": [], "actual_count": len(commit["attached"])})
if before != after:
    failures.append({"field": "aliases.json", "expected": "unchanged", "actual": "changed"})
if before_registry != after_registry:
    failures.append({"field": "registry_bytes", "expected": "unchanged", "actual": "changed"})
if len(alias_inputs) != len(set(alias_inputs)):
    failures.append({"field": "alias_inputs", "expected": "unique", "actual": "duplicates"})
if failures:
    defect = {
        "repeat": "second_session",
        "failures": failures,
        "observed": {
            "outcome": commit.get("outcome"),
            "attached": len(commit.get("attached", [])),
            "minted": len(commit.get("minted", [])),
            "changed_files": commit.get("registry", {}).get("changed_files"),
        },
    }
    defect_path.write_text(json.dumps(defect, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(defect, sort_keys=True))
    raise SystemExit(1)
print(json.dumps({
    "second_session_no_op": True,
    "outcome": "nothing_to_commit",
    "attached": 0,
    "minted": 0,
    "alias_count": len(alias_rows),
    "alias_inputs_unique": True,
}, sort_keys=True))
PY
then
  second_session_check_status=1
  printf '[second-session] strict no-write assertion failed; continuing to complete the retained proof\n' >&2
fi

cp -R "$registry" "$work_dir/negative-registry"
python3 - "$work_dir/registries.yaml" "$registry" "$work_dir/negative-registries.yaml" "$work_dir/negative-registry" <<'PY'
import pathlib
import sys
source, old_registry, destination, new_registry = map(pathlib.Path, sys.argv[1:])
destination.write_text(source.read_text().replace(str(old_registry), str(new_registry)), encoding="utf-8")
PY

python3 - "$input_csv" "$work_dir/commit.json" "$work_dir/negative-alias.jsonl" <<'PY'
import csv
import json
import pathlib
import sys
input_path, commit_path, output_path = map(pathlib.Path, sys.argv[1:])
commit = json.loads(commit_path.read_text())
key_alias = next(
    alias
    for minted in sorted(commit.get("minted", []), key=lambda row: row["canonical_id"])
    for alias in minted.get("aliases", [])
    if alias.get("alias_file") == "keys.cik.json"
)
canonical_cik = key_alias["input"]
rows = list(csv.DictReader(input_path.open(newline="", encoding="utf-8")))
row = next(row for row in rows if row["cik"].lstrip("0") == canonical_cik)
output_path.write_text(json.dumps({
    "obs_id": "planted-existing-cik-new-name",
    "entity_type": "fund",
    "surface": row["company_name"] + " PLANTED ALIAS",
    "keys": {"cik": row["cik"]},
    "source": {"channel": "file", "id": "planted-negative", "lineage": "planted:existing-cik"},
}) + "\n", encoding="utf-8")
PY
negative_alias_session="$work_dir/negative-alias-session"
run_ok "negative-alias-init" "$canon_bin" entity session init --goal "E2E-1 planted existing CIK alias" \
  --registries "$work_dir/negative-registries.yaml" --work-dir "$negative_alias_session" --emit json
run_ok "negative-alias-ingest" "$canon_bin" entity session ingest "$negative_alias_session" "$work_dir/negative-alias.jsonl" --emit json
run_ok "negative-alias-solve" "$canon_bin" entity session solve "$negative_alias_session" --rules "$work_dir/rules.yaml" --emit json
run_maybe "negative-alias-commit" "$canon_bin" entity session commit "$negative_alias_session" --bump patch --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/negative-alias-commit.json"
python3 - "$work_dir/negative-alias-commit.json" "$work_dir/negative-registry/aliases.json" <<'PY'
import json
import pathlib
import sys
commit, aliases = map(pathlib.Path, sys.argv[1:])
value = json.loads(commit.read_text())
if value.get("minted"):
    raise SystemExit(f"existing CIK alias minted an id: {value}")
if not any("PLANTED ALIAS" in row.get("input", "") for row in json.loads(aliases.read_text())):
    raise SystemExit("existing CIK alias did not attach")
print(json.dumps({"existing_cik_alias_attached": True}, sort_keys=True))
PY

cat >"$work_dir/malformed-cik.jsonl" <<'EOF'
{"obs_id":"planted-cik-letters","entity_type":"fund","surface":"Planted Letters CIK","keys":{"cik":"ABC"},"source":{"channel":"file","id":"planted-negative","lineage":"planted:bad-cik"}}
{"obs_id":"planted-cik-empty","entity_type":"fund","surface":"Planted Empty CIK","keys":{"cik":""},"source":{"channel":"file","id":"planted-negative","lineage":"planted:bad-cik"}}
{"obs_id":"planted-cik-zero","entity_type":"fund","surface":"Planted Zero CIK","keys":{"cik":"0000000000"},"source":{"channel":"file","id":"planted-negative","lineage":"planted:bad-cik"}}
EOF
malformed_session="$work_dir/malformed-cik-session"
run_ok "malformed-cik-init" "$canon_bin" entity session init --goal "E2E-1 planted malformed CIKs" \
  --registries "$work_dir/negative-registries.yaml" --work-dir "$malformed_session" --emit json
run_maybe "malformed-cik-ingest" "$canon_bin" entity session ingest "$malformed_session" "$work_dir/malformed-cik.jsonl" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/malformed-cik-ingest.json"
run_ok "malformed-cik-solve" "$canon_bin" entity session solve "$malformed_session" --rules "$work_dir/rules.yaml" --emit json
run_maybe "malformed-cik-commit" "$canon_bin" entity session commit "$malformed_session" --bump patch --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/malformed-cik-commit.json"
python3 - "$work_dir/malformed-cik-ingest.json" "$work_dir/malformed-cik-commit.json" <<'PY'
import json
import pathlib
import sys
ingest, commit = (json.loads(pathlib.Path(path).read_text()) for path in sys.argv[1:])
if not ingest.get("key_warnings") and not ingest.get("rejected"):
    raise SystemExit(f"malformed CIKs were not reported: {ingest}")
if commit.get("minted"):
    raise SystemExit(f"malformed CIK minted ids: {commit}")
print(json.dumps({"malformed_cik_reported": True, "malformed_cik_minted": 0}, sort_keys=True))
PY

repeat_check_status=0
if ! python3 - "$work_dir" <<'PY'
import json
import pathlib
import sys
root = pathlib.Path(sys.argv[1])
commit = json.loads((root / "commit.json").read_text())
holdbacks = json.loads((root / "holdbacks.json").read_text())
source = json.loads((root / "receipt-verification.json").read_text())["actual"]
manifest = json.loads((root / "source-manifest.json").read_text())
summary = {
    "source": source,
    "observations": manifest,
    "flags": manifest["flag_counts"],
    "day_one": {
        "minted": len(commit.get("minted", [])),
        "attached": len(commit.get("attached", [])),
        "held_back": len(holdbacks),
        "holdback_reasons": sorted({row.get("reason") for row in holdbacks}),
    },
    "evidence_closure": json.loads((root / "evidence-closure-summary.json").read_text()),
    "plain_lookup": json.loads((root / "lookup-summary.json").read_text()),
    "exports": json.loads((root / "exports-summary.json").read_text()),
    "same_session_no_op": not (root / "same-session-repeat-defect.json").exists(),
    "second_session_no_op": not (root / "second-session-repeat-defect.json").exists(),
    "repeat_defects": [
        json.loads(path.read_text())
        for path in sorted(root.glob("*-session-repeat-defect.json"))
    ],
    "planted_existing_cik_alias": True,
    "malformed_cik_reported": True,
    "held_back_rows": holdbacks,
    "retained_work_dir": True,
}
(root / "proof-summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(summary, sort_keys=True))
PY
then
  repeat_check_status=1
fi
printf 'proof_summary=%s\nflags=%s\nholdbacks=%s\nretained_work_dir=true\n' \
  "$work_dir/proof-summary.json" "$work_dir/flags.jsonl" "$work_dir/holdbacks.json"
if [[ "$same_session_check_status" -ne 0 || "$second_session_check_status" -ne 0 || "$repeat_check_status" -ne 0 ]]; then
  printf 'strict_repeat_assertion=failed\n' >&2
  exit 1
fi

