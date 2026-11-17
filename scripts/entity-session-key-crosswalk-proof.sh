#!/usr/bin/env bash
set -euo pipefail

# bd-vdcw.35: replay a real CIK/LEI key crosswalk through the public CLI.
# The script never writes to the served registry. It copies that registry to a
# retained staging directory, records source hashes, runs entity session
# init/ingest/solve/commit, exports the crosswalk, and replays both namespaces
# through plain exact lookup. Sources are arguments: identifiers are never
# manufactured or inferred from memory.

registry_source="/Users/zac/Source/cmdrvl/cmdrvl-context/artifacts/firms/registry"
gold_dir="/Users/zac/Source/cmdrvl/cmdrvl-soi/gold_sets"
cik_source=""
lei_source=""
work_dir=""
canon_bin="$(printenv CANON_BIN || true)"
seed="bd-vdcw.35-crosswalk-v1"
sample_size=100
allow_partial_coverage=0

usage() {
  cat >&2 <<'EOF'
usage: entity-session-key-crosswalk-proof.sh \
  --cik-source JSONL --lei-source JSONL [options]

JSONL source records are {"name":...,"cik":...} for the CIK source and
{"name":...,"cik":...,"lei":...} for the independent LEI source. The record
names for day one must be exact registered surfaces; day two joins independent
LEI rows to gold-set CIK by CIK. No source is fetched by this script.

options:
  --registry-source DIR  served organisations registry to copy
  --gold-dir DIR         BDC gold sets
  --work-dir DIR         retained output directory; default is retained mktemp
  --canon-bin PATH       canon executable (or CANON_BIN)
  --seed TEXT            recorded crosswalk sample seed
  --sample-size N        maximum rows in operator sample (default: 100)
  --allow-partial-coverage
                         run with joined BDC CIKs only; record the missing
                         CIK list and coverage denominator instead of claiming
                         full population coverage

Evidence-closure verification requires the `b3sum` command available from the
local Rust toolchain. All output directories are retained; this script never
deletes a work directory.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --registry-source) registry_source="$2"; shift 2 ;;
    --gold-dir) gold_dir="$2"; shift 2 ;;
    --cik-source) cik_source="$2"; shift 2 ;;
    --lei-source) lei_source="$2"; shift 2 ;;
    --work-dir) work_dir="$2"; shift 2 ;;
    --canon-bin) canon_bin="$2"; shift 2 ;;
    --seed) seed="$2"; shift 2 ;;
    --sample-size) sample_size="$2"; shift 2 ;;
    --allow-partial-coverage) allow_partial_coverage=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage; exit 2 ;;
  esac
done

if [[ -z "$cik_source" || -z "$lei_source" ]]; then
  printf '%s\n' '--cik-source and --lei-source are required; sources must be named before the run' >&2
  usage
  exit 2
fi
if [[ -z "$canon_bin" ]]; then
  if [[ -x target/debug/canon ]]; then
    canon_bin="target/debug/canon"
  elif [[ -x target/release/canon ]]; then
    canon_bin="target/release/canon"
  else
    canon_bin="$(command -v canon || true)"
  fi
fi
for required in "$registry_source" "$gold_dir" "$cik_source" "$lei_source"; do
  if [[ ! -e "$required" ]]; then
    printf 'missing proof input: %s\n' "$required" >&2
    exit 2
  fi
done
if [[ -z "$canon_bin" || ! -x "$canon_bin" ]]; then
  printf 'set CANON_BIN or --canon-bin to an executable canon binary\n' >&2
  exit 2
fi
if ! command -v b3sum >/dev/null 2>&1; then
  printf '%s\n' 'evidence-closure verification requires b3sum on PATH' >&2
  exit 2
fi
if ! [[ "$sample_size" =~ ^[0-9]+$ ]]; then
  printf '%s\n' '--sample-size must be a non-negative integer' >&2
  exit 2
fi

if [[ -z "$work_dir" ]]; then
  work_dir="$(mktemp -d /tmp/canon-key-crosswalk.XXXXXX)"
else
  mkdir -p "$work_dir"
fi
work_dir="$(cd "$work_dir" && pwd -P)"
printf 'work_dir=%s\n' "$work_dir"
printf 'served_registry_untouched=true\n'
mkdir -p "$work_dir/commands"
if [[ -e "$work_dir/staging-registry" ]]; then
  printf 'refusing to overwrite existing staging registry: %s\n' "$work_dir/staging-registry" >&2
  printf '%s\n' 'choose a new --work-dir; retained proof output is never deleted' >&2
  exit 2
fi
cp -R "$registry_source" "$work_dir/staging-registry"
registry="$work_dir/staging-registry"
python3 - "$registry" "$work_dir/preexisting-aliases.json" <<'PY'
import json
import pathlib
import sys

registry, output = map(pathlib.Path, sys.argv[1:])
snapshot = {}
for path in sorted(registry.glob("*.json")):
    if path.name in {"registry.json", "20-firm-keys.json"}:
        continue
    value = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(value, list):
        snapshot[path.name] = sorted(
            (row.get("input"), row.get("canonical_id"), row.get("canonical_type"))
            for row in value
            if isinstance(row, dict) and "input" in row and "canonical_id" in row
        )
output.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

sha256_file() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}
printf 'source.cik.path=%s\n' "$cik_source"
printf 'source.cik.sha256=%s\n' "$(sha256_file "$cik_source")"
printf 'source.lei.path=%s\n' "$lei_source"
printf 'source.lei.sha256=%s\n' "$(sha256_file "$lei_source")"
printf 'source.gold_sets.path=%s\n' "$gold_dir"

rules="$work_dir/rules.yaml"
cat >"$rules" <<'EOF'
version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
  lei: {authoritative: true, single_valued: true}
  ticker: {authoritative: false, single_valued: false}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
EOF

# Build observations from supplied source rows without a name normalizer. Day
# one uses exact registry names; day two joins the independent LEI source by
# CIK. The gold-set ticker is retained in source metadata but not ingested as a
# key: these values are lower-case filing slugs, not declared exchange tickers.
python3 - "$registry" "$gold_dir" "$cik_source" "$lei_source" "$work_dir" "$allow_partial_coverage" <<'PY'
import json
import pathlib
import sys

registry, gold_dir, cik_path, lei_path, work = map(pathlib.Path, sys.argv[1:6])
allow_partial = sys.argv[6] == "1"

def jsonl(path):
    rows = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise SystemExit(f"{path}:{number}: source row must be an object")
        value["_line"] = number
        rows.append(value)
    return rows

cik_rows = jsonl(cik_path)
lei_rows = jsonl(lei_path)
name_rows = json.loads((registry / "00-name-to-firm.json").read_text(encoding="utf-8"))
registered = {row["input"] for row in name_rows}

cik_by_name = {}
for row in cik_rows:
    name, cik = row.get("name"), row.get("cik")
    if isinstance(name, str) and isinstance(cik, str) and name in registered:
        prior = cik_by_name.setdefault(name, (cik, row["_line"]))
        if prior[0] != cik:
            raise SystemExit(f"conflicting CIK source rows for exact name {name!r}")

lei_by_name = {}
lei_by_cik = {}
for row in lei_rows:
    name, cik, lei = row.get("name"), row.get("cik"), row.get("lei")
    if not all(isinstance(value, str) for value in (name, cik, lei)):
        continue
    if name in registered:
        prior = lei_by_name.setdefault(name, (cik, lei, row["_line"]))
        if prior[:2] != (cik, lei):
            raise SystemExit(f"conflicting LEI source rows for exact name {name!r}")
    prior = lei_by_cik.setdefault(cik, (name, lei, row["_line"]))
    if prior[1] != lei:
        raise SystemExit(f"conflicting LEI source rows for CIK {cik!r}")

day_one = []
day_one_missing = []
for name in sorted(registered):
    cik_record = cik_by_name.get(name)
    lei_record = lei_by_name.get(name)
    if cik_record is None or lei_record is None:
        day_one_missing.append({
            "name": name,
            "missing": [key for key, value in (("cik", cik_record), ("lei", lei_record)) if value is None],
        })
        continue
    cik, cik_line = cik_record
    lei_cik, lei, lei_line = lei_record
    if cik != lei_cik:
        raise SystemExit(f"day-one exact sources disagree on CIK for {name!r}")
    day_one.extend([
        {
            "obs_id": f"day-one-cik:{cik}",
            "entity_type": "organisation",
            "surface": name,
            "keys": {"cik": cik},
            "source": {"channel": "file", "id": cik_path.name, "lineage": f"cik-source:{cik_path.name}:{cik_line}"},
        },
        {
            "obs_id": f"day-one-lei:{lei}",
            "entity_type": "organisation",
            "surface": name,
            "keys": {"lei": lei},
            "source": {"channel": "file", "id": lei_path.name, "lineage": f"lei-source:{lei_path.name}:{lei_line}"},
        },
    ])

gold_by_cik = {}
for path in sorted(gold_dir.glob("*.goldset.v*.json")):
    source = json.loads(path.read_text(encoding="utf-8"))["source"]
    cik = str(source["cik"])
    gold_by_cik.setdefault(cik, {
        "company": str(source["company"]),
        "ticker": str(source.get("ticker", "")),
        "path": path.name,
    })

day_two = []
day_two_missing_lei = []
for cik in sorted(gold_by_cik):
    gold = gold_by_cik[cik]
    lei_record = lei_by_cik.get(cik)
    if lei_record is None:
        day_two_missing_lei.append({"cik": cik, **gold})
        continue
    source_name, lei, lei_line = lei_record
    day_two.extend([
        {
            "obs_id": f"day-two-cik:{cik}",
            "entity_type": "organisation",
            "surface": gold["company"],
            "keys": {"cik": cik},
            "source": {
                "channel": "file", "id": gold["path"],
                "lineage": f"goldset-cik:{gold['path']}", "ticker_slug": gold["ticker"],
            },
        },
        {
            "obs_id": f"day-two-lei:{lei}",
            "entity_type": "organisation",
            "surface": source_name,
            "keys": {"cik": cik, "lei": lei},
            "source": {
                "channel": "file", "id": lei_path.name,
                "lineage": f"lei-source:{lei_path.name}:{lei_line}",
                "joined_cik": cik, "ticker_slug": gold["ticker"],
            },
        },
    ])

def write_jsonl(path, rows):
    path.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in rows), encoding="utf-8")

write_jsonl(work / "day-one.jsonl", day_one)
write_jsonl(work / "day-two.jsonl", day_two)
(work / "day-one-missing.json").write_text(json.dumps(day_one_missing, indent=2, sort_keys=True) + "\n", encoding="utf-8")
(work / "day-two-missing-lei.json").write_text(json.dumps(day_two_missing_lei, indent=2, sort_keys=True) + "\n", encoding="utf-8")
coverage = {
    "joined_ciks": len(day_two) // 2,
    "gold_ciks": len(gold_by_cik),
    "coverage_numerator": len(day_two) // 2,
    "coverage_denominator": len(gold_by_cik),
    "missing_ciks": len(day_two_missing_lei),
    "missing_cik_list": str(work / "day-two-missing-lei.json"),
}
(work / "source-manifest.json").write_text(json.dumps({
    "registry_exact_name_count": len(registered),
    "day_one_exact_name_count": len(day_one) // 2,
    "day_one_missing_count": len(day_one_missing),
    "bdc_distinct_cik_count": len(gold_by_cik),
    "day_two_joined_cik_count": len(day_two) // 2,
    "day_two_missing_lei_count": len(day_two_missing_lei),
    "ticker_slug_rows_not_ingested_as_keys": len(day_two) // 2,
    "day_one_observation_count": len(day_one),
    "day_two_observation_count": len(day_two),
    "coverage_mode": "partial" if allow_partial else "complete",
    "partial_coverage_allowed": allow_partial,
    "day_two_coverage": coverage,
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")

with (work / "day-two-replay.csv").open("w", encoding="utf-8") as output:
    output.write("cik,lei\n")
    for cik in sorted(gold_by_cik):
        lei_record = lei_by_cik.get(cik)
        if lei_record is not None:
            output.write(f"{cik},{lei_record[1]}\n")

print(json.dumps({
    "registry_exact_name_count": len(registered),
    "day_one_exact_name_count": len(day_one) // 2,
    "day_one_missing_count": len(day_one_missing),
    "bdc_distinct_cik_count": len(gold_by_cik),
    "day_two_joined_cik_count": len(day_two) // 2,
    "day_two_missing_lei_count": len(day_two_missing_lei),
    "ticker_slug_rows_not_ingested_as_keys": len(day_two) // 2,
    "coverage_mode": "partial" if allow_partial else "complete",
    "day_two_coverage": coverage,
}, sort_keys=True))
if not day_one:
    raise SystemExit("no exact day-one source pair matched the registry")
if day_two_missing_lei and not allow_partial:
    raise SystemExit(
        "independent LEI source covers "
        f"{len(day_two) // 2}/{len(gold_by_cik)} BDC CIKs; "
        "missing list is day-two-missing-lei.json; "
        "rerun with --allow-partial-coverage for a declared partial proof"
    )
PY
printf 'source_manifest=%s\n' "$(cat "$work_dir/source-manifest.json")"
python3 - "$work_dir/source-manifest.json" <<'PY'
import json
import pathlib
import sys

manifest = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
coverage = manifest["day_two_coverage"]
print(f"day_two_coverage={coverage['coverage_numerator']}/{coverage['coverage_denominator']}")
print(f"day_two_missing_cik_list={coverage['missing_cik_list']}")
PY

run_json() {
  local label="$1"
  shift
  local safe_label
  safe_label="$(printf '%s' "$label" | tr -c 'A-Za-z0-9_.-' '_')"
  local stdout_path="$work_dir/commands/$safe_label.stdout"
  local stderr_path="$work_dir/commands/$safe_label.stderr"
  printf '\n[%s] command:' "$label"
  printf ' %q' "$@"
  printf '\n'
  local output status
  set +e
  output=$("$@" 2> >(tee "$stderr_path" >&2))
  status=$?
  set -e
  printf '%s\n' "$output" | tee "$stdout_path"
  RUN_JSON="$output"
  RUN_STATUS="$status"
}

run_expected_json() {
  local label="$1"
  shift
  run_json "$label" "$@"
  if [[ "$RUN_STATUS" -ne 0 ]]; then
    printf '[%s] unexpected exit=%s\n' "$label" "$RUN_STATUS" >&2
    exit "$RUN_STATUS"
  fi
}

run_expected_failure_json() {
  local label="$1"
  shift
  run_json "$label" "$@"
  if [[ "$RUN_STATUS" -eq 0 ]]; then
    printf '[%s] expected a refusal/domain failure, got exit=0\n' "$label" >&2
    exit 1
  fi
}

record_registry_closure_hashes() {
  local registry_path="$1"
  local output_path="$2"
  python3 - "$registry_path" "$output_path" <<'PY'
import json
import pathlib
import sys

registry, output = map(pathlib.Path, sys.argv[1:])
hashes = set()

def collect(value):
    if isinstance(value, dict):
        closure_hash = value.get("closure_hash")
        if isinstance(closure_hash, str):
            hashes.add(closure_hash)
        for child in value.values():
            collect(child)
    elif isinstance(value, list):
        for child in value:
            collect(child)

for path in sorted(registry.rglob("*")):
    if not path.is_file() or path.suffix not in {".json", ".jsonl"}:
        continue
    if path.suffix == ".jsonl":
        values = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
    else:
        values = [json.loads(path.read_text(encoding="utf-8"))]
    for value in values:
        collect(value)
output.write_text(json.dumps(sorted(hashes), indent=2) + "\n", encoding="utf-8")
PY
}

verify_evidence_closure() {
  local label="$1"
  local registry_path="$2"
  local commit_path="$3"
  local export_path="$4"
  local baseline_path="$5"
  local summary_path="$6"
  python3 - "$label" "$registry_path" "$commit_path" "$export_path" "$baseline_path" "$summary_path" <<'PY'
import collections
import json
import pathlib
import subprocess
import sys

label, registry_path, commit_path, export_path, baseline_path, summary_path = map(pathlib.Path, sys.argv[1:])

def collect_closure_hashes(value):
    hashes = set()

    def collect(node):
        if isinstance(node, dict):
            closure_hash = node.get("closure_hash")
            if isinstance(closure_hash, str):
                hashes.add(closure_hash)
            for child in node.values():
                collect(child)
        elif isinstance(node, list):
            for child in node:
                collect(child)

    collect(value)
    return hashes

def b3(path):
    output = subprocess.check_output(["b3sum", str(path)], text=True)
    return output.split()[0]

commit = json.loads(commit_path.read_text(encoding="utf-8"))
export = json.loads(export_path.read_text(encoding="utf-8"))
closures = export.get("closures")
if not isinstance(closures, list) or not closures:
    raise SystemExit(f"{label}: evidence-closure export emitted no closures")
emitted = {row.get("closure_hash") for row in closures}
if None in emitted or not all(isinstance(value, str) for value in emitted):
    raise SystemExit(f"{label}: evidence-closure export has a missing closure_hash")
commit_hashes = collect_closure_hashes(commit)
if not commit_hashes <= emitted:
    raise SystemExit(f"{label}: commit closure hashes not emitted: {sorted(commit_hashes - emitted)}")

closure_root = pathlib.Path(export["out"])
for closure in closures:
    closure_hash = closure["closure_hash"]
    directory = closure.get("directory")
    expected_directory = closure_hash.removeprefix("blake3:")
    if directory != expected_directory:
        raise SystemExit(f"{label}: closure directory does not match hash: {closure}")
    sensitivity = closure.get("sensitivity")
    if not isinstance(sensitivity, dict) or sensitivity.get("level") not in {"public", "restricted"}:
        raise SystemExit(f"{label}: closure has no recorded sensitivity level: {closure}")
    if not isinstance(sensitivity.get("unstated_inputs"), int):
        raise SystemExit(f"{label}: closure has no sensitivity input count: {closure}")
    closure_file = closure_root / directory / "closure.json"
    if not closure_file.is_file() or b3(closure_file) != expected_directory:
        raise SystemExit(f"{label}: emitted closure does not hash to {closure_hash}: {closure_file}")
    listed = {entry.get("path"): entry.get("blake3") for entry in closure.get("files", [])}
    closure_entry = listed.get(f"{directory}/closure.json")
    if closure_entry != closure_hash:
        raise SystemExit(f"{label}: export file record disagrees with closure hash: {closure}")
    for relative, listed_hash in listed.items():
        file_path = closure_root / relative
        if not file_path.is_file() or b3(file_path) != str(listed_hash).removeprefix("blake3:"):
            raise SystemExit(f"{label}: exported file hash mismatch: {file_path}")

registry_hashes = set()
for path in sorted(registry_path.rglob("*")):
    if not path.is_file() or path.suffix not in {".json", ".jsonl"}:
        continue
    if path.suffix == ".jsonl":
        values = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
    else:
        values = [json.loads(path.read_text(encoding="utf-8"))]
    for value in values:
        registry_hashes |= collect_closure_hashes(value)
baseline = set(json.loads(baseline_path.read_text(encoding="utf-8")))
new_registry_hashes = registry_hashes - baseline
if not new_registry_hashes <= emitted:
    raise SystemExit(
        f"{label}: registry closure hashes not emitted: {sorted(new_registry_hashes - emitted)}"
    )

sensitivity_counts = collections.Counter(row["sensitivity"]["level"] for row in closures)
summary = {
    "label": str(label),
    "commit_closure_hash_count": len(commit_hashes),
    "emitted_closure_count": len(emitted),
    "new_registry_closure_hash_count": len(new_registry_hashes),
    "sensitivity_counts": dict(sorted(sensitivity_counts.items())),
    "sensitivity_levels": {
        row["closure_hash"]: row["sensitivity"]["level"] for row in closures
    },
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(summary, sort_keys=True))
PY
}

run_positive() {
  local label="$1"
  local target_registry="$2"
  local crosswalk_path="$3"
  local session_root="$work_dir/$label-sessions"
  mkdir -p "$session_root"
  local day_one_session="$session_root/day-one"
  local day_two_session="$session_root/day-two"
  local registries_map="$work_dir/$label-registries.yaml"
  local day_one_closure_dir="$work_dir/$label-evidence-closures-day-one"
  local day_two_closure_dir="$work_dir/$label-evidence-closures-day-two"
  local registry_closure_baseline="$work_dir/$label-registry-closures-before-day-one.json"
  local day_two_registry_closure_baseline="$work_dir/$label-registry-closures-before-day-two.json"
  record_registry_closure_hashes "$target_registry" "$registry_closure_baseline"
  cat >"$registries_map" <<EOF
version: canon.session.registries.v1
kinds:
  organisation:
    registry: $target_registry
    registry_id: firms
    prefix: ORG
    zero_pad: 3
    canonical_type: firm
    alias_file: 30-session-aliases.json
    key_file: 20-firm-keys.json
EOF

  run_expected_json "$label session init day one" "$canon_bin" entity session init \
    --goal "BDC key crosswalk proof day one" \
    --registries "$registries_map" \
    --work-dir "$day_one_session" \
    --emit json
  run_expected_json "$label ingest day one" "$canon_bin" entity session ingest \
    "$day_one_session" "$work_dir/day-one.jsonl" --emit json
  python3 - "$RUN_JSON" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
if value.get("rejected"):
    raise SystemExit(f"day-one ingest rejected rows: {value['rejected']}")
PY
  run_expected_json "$label solve day one" "$canon_bin" entity session solve \
    "$day_one_session" --rules "$rules" --emit json
  run_expected_json "$label commit day one" "$canon_bin" entity session commit \
    "$day_one_session" --next-version 2026-09-30-bd35-day-one --emit json
  local day_one_commit="$RUN_JSON"
  printf '%s\n' "$day_one_commit" >"$work_dir/$label-day-one-commit.json"
  python3 - "$day_one_commit" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
if value.get("outcome") != "committed":
    raise SystemExit(f"day-one outcome was not committed: {value}")
if value.get("minted"):
    raise SystemExit(f"day-one minted an id for an existing registry name: {value['minted']}")
if not value.get("attached"):
    raise SystemExit("day-one did not attach any exact existing registry name")
PY
  local day_one_new_hash
  day_one_new_hash="$(python3 - "$day_one_commit" <<'PY'
import json
import sys
print(json.loads(sys.argv[1])["registry"]["new"]["content_hash"])
PY
)"
  printf '%s.day_one_new_hash=%s\n' "$label" "$day_one_new_hash"
  run_expected_json "$label export evidence closure day one" "$canon_bin" entity session export \
    "$day_one_session" --format evidence-closure --out "$day_one_closure_dir" --emit json
  printf '%s\n' "$RUN_JSON" >"$work_dir/$label-day-one-evidence-closure.json"
  verify_evidence_closure "$label day one" "$target_registry" \
    "$work_dir/$label-day-one-commit.json" "$work_dir/$label-day-one-evidence-closure.json" \
    "$registry_closure_baseline" "$work_dir/$label-day-one-evidence-closure-summary.json"
  run_expected_json "$label registry lint after day one" "$canon_bin" registry lint \
    "$target_registry" --emit json

  cp -R "$target_registry" "$work_dir/$label-registry-before-day-two"
  record_registry_closure_hashes "$target_registry" "$day_two_registry_closure_baseline"
  run_expected_json "$label session init day two" "$canon_bin" entity session init \
    --goal "BDC key crosswalk proof day two" \
    --registries "$registries_map" \
    --work-dir "$day_two_session" \
    --emit json
  run_expected_json "$label ingest day two" "$canon_bin" entity session ingest \
    "$day_two_session" "$work_dir/day-two.jsonl" --emit json
  python3 - "$RUN_JSON" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
if value.get("rejected"):
    raise SystemExit(f"day-two ingest rejected rows: {value['rejected']}")
PY
  run_expected_json "$label solve day two" "$canon_bin" entity session solve \
    "$day_two_session" --rules "$rules" --emit json
  run_expected_json "$label commit day two" "$canon_bin" entity session commit \
    "$day_two_session" --next-version 2026-09-30-bd35-day-two \
    --expect-base "$day_one_new_hash" --emit json
  local day_two_commit="$RUN_JSON"
  printf '%s\n' "$day_two_commit" >"$work_dir/$label-day-two-commit.json"
  python3 - "$day_two_commit" "$day_one_new_hash" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
expected_base = sys.argv[2]
if value.get("outcome") != "committed":
    raise SystemExit(f"day-two outcome was not committed: {value}")
if not value.get("minted") and not value.get("attached"):
    raise SystemExit("day-two committed no BDC entities")
if value["registry"]["base"]["content_hash"] != expected_base:
    raise SystemExit("day-two receipt did not record the day-one base")
PY
  run_expected_json "$label export evidence closure day two" "$canon_bin" entity session export \
    "$day_two_session" --format evidence-closure --out "$day_two_closure_dir" --emit json
  printf '%s\n' "$RUN_JSON" >"$work_dir/$label-day-two-evidence-closure.json"
  verify_evidence_closure "$label day two" "$target_registry" \
    "$work_dir/$label-day-two-commit.json" "$work_dir/$label-day-two-evidence-closure.json" \
    "$day_two_registry_closure_baseline" "$work_dir/$label-day-two-evidence-closure-summary.json"
  run_expected_json "$label registry lint after day two" "$canon_bin" registry lint \
    "$target_registry" --emit json
  run_expected_json "$label export key crosswalk" "$canon_bin" registry export \
    --registry "$target_registry" --format key-crosswalk --out "$crosswalk_path" --emit json

  local replay="$work_dir/$label-replay.csv"
  cp "$work_dir/day-two-replay.csv" "$replay"
  run_expected_json "$label plain CIK lookup" "$canon_bin" "$replay" \
    --registry "$target_registry" --column cik --explicit --plain-json-values --no-witness
  printf '%s\n' "$RUN_JSON" >"$work_dir/$label-cik-lookup.json"
  run_expected_json "$label plain LEI lookup" "$canon_bin" "$replay" \
    --registry "$target_registry" --column lei --explicit --plain-json-values --no-witness
  printf '%s\n' "$RUN_JSON" >"$work_dir/$label-lei-lookup.json"
  python3 - "$crosswalk_path" "$replay" \
    "$work_dir/$label-cik-lookup.json" "$work_dir/$label-lei-lookup.json" <<'PY'
import csv
import json
import pathlib
import sys

crosswalk_path, replay_path, cik_path, lei_path = map(pathlib.Path, sys.argv[1:])
with crosswalk_path.open(newline="", encoding="utf-8") as stream:
    rows = list(csv.DictReader(stream))
by_key = {}
for row in rows:
    for namespace in ("cik", "lei"):
        for value in filter(None, row.get(namespace, "").split(";")):
            by_key[(namespace, value)] = row["canonical_id"]

def lookup_ids(path):
    value = json.loads(path.read_text(encoding="utf-8"))
    return {
        str(mapping["input"]).removeprefix("u8:"): str(mapping["canonical_id"]).removeprefix("u8:")
        for mapping in value.get("mappings", [])
    }

cik_lookup = lookup_ids(cik_path)
lei_lookup = lookup_ids(lei_path)
expected = list(csv.DictReader(replay_path.open(newline="", encoding="utf-8")))
if not rows:
    raise SystemExit("key crosswalk export has no keyed entities")
if len(cik_lookup) != len(expected) or len(lei_lookup) != len(expected):
    raise SystemExit(f"plain lookup was partial: CIK={len(cik_lookup)} LEI={len(lei_lookup)} expected={len(expected)}")
for source in expected:
    cik, lei = source["cik"], source["lei"]
    expected_id = by_key.get(("cik", cik))
    if expected_id is None or by_key.get(("lei", lei)) != expected_id:
        raise SystemExit(f"crosswalk did not join CIK/LEI: {source}")
    if cik_lookup.get(cik) != expected_id:
        raise SystemExit(f"plain CIK lookup mismatch for {cik}")
    if lei_lookup.get(lei) != expected_id:
        raise SystemExit(f"plain LEI lookup mismatch for {lei}")
print(json.dumps({
    "crosswalk_rows": len(rows),
    "source_cik_rows": len(expected),
    "cik_exact_lookup_resolved": len(cik_lookup),
    "lei_exact_lookup_resolved": len(lei_lookup),
    "cik_to_lei_same_id": len(expected),
}, sort_keys=True))
PY
}

run_positive "positive" "$registry" "$work_dir/key-crosswalk.csv"
python3 - "$registry" "$work_dir/preexisting-aliases.json" <<'PY'
import json
import pathlib
import sys

registry, baseline_path = map(pathlib.Path, sys.argv[1:])
baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
for name, expected in baseline.items():
    path = registry / name
    value = json.loads(path.read_text(encoding="utf-8"))
    actual = [list(row) for row in sorted(
        (row.get("input"), row.get("canonical_id"), row.get("canonical_type"))
        for row in value
        if isinstance(row, dict) and "input" in row and "canonical_id" in row
    )]
    if actual != expected:
        raise SystemExit(f"pre-existing aliases changed in {name}")
print(json.dumps({"preexisting_aliases_unchanged": True, "files_checked": len(baseline)}, sort_keys=True))
PY

# Re-run from an untouched copy and compare the actual exported bytes. This is
# a clean public-command replay, not a regenerated golden artifact.
cp -R "$registry_source" "$work_dir/replay-registry"
run_positive "replay" "$work_dir/replay-registry" "$work_dir/key-crosswalk-replay.csv"
if cmp -s "$work_dir/key-crosswalk.csv" "$work_dir/key-crosswalk-replay.csv"; then
  printf 'crosswalk_byte_identical=true\n'
else
  printf 'crosswalk_byte_identical=false\n' >&2
  exit 1
fi

# Select the operator sample by row position using only the recorded seed and
# row count. No crosswalk value is inspected until after the positions exist.
python3 - "$work_dir/key-crosswalk.csv" "$work_dir/operator-sample.csv" \
  "$work_dir/sample-selection.json" "$seed" "$sample_size" <<'PY'
import csv
import hashlib
import json
import pathlib
import sys

crosswalk, sample, receipt = map(pathlib.Path, sys.argv[1:4])
seed, limit = sys.argv[4], int(sys.argv[5])
with crosswalk.open("r", encoding="utf-8") as stream:
    row_count = max(0, sum(1 for _ in stream) - 1)
positions = sorted(
    range(row_count),
    key=lambda position: hashlib.sha256(f"{seed}|{position}".encode()).hexdigest(),
)[: min(limit, row_count)]
selected = set(positions)
with crosswalk.open(newline="", encoding="utf-8") as source, sample.open("w", newline="", encoding="utf-8") as output:
    reader = csv.reader(source)
    writer = csv.writer(output)
    writer.writerow(next(reader))
    for position, row in enumerate(reader):
        if position in selected:
            writer.writerow(row)
receipt.write_text(json.dumps({
    "version": "canon.entity.session.key_crosswalk_sample.v1",
    "selection_seed": seed,
    "candidate_row_count": row_count,
    "selected_zero_based_rows": positions,
    "sample_size": len(positions),
    "operator_check": "pending_named_authoritative_lookup",
}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(json.loads(receipt.read_text(encoding="utf-8")), sort_keys=True))
PY
printf 'operator_sample=%s\n' "$work_dir/operator-sample.csv"
printf 'operator_sample_receipt=%s\n' "$(cat "$work_dir/sample-selection.json")"
printf 'operator_sample_result=pending_named_authoritative_lookup\n'

# Plant a conflict using runtime-selected real rows. The first entity keeps its
# real CIK but receives another entity's real LEI, while its real primary name
# supplies the incumbent endpoint. The commit must refuse without changing the
# copied registry.
cp -R "$registry" "$work_dir/negative-key-conflict-registry"
python3 - "$work_dir/key-crosswalk.csv" "$work_dir/negative-key-conflict.jsonl" <<'PY'
import csv
import json
import pathlib
import sys

crosswalk, output = map(pathlib.Path, sys.argv[1:])
rows = [row for row in csv.DictReader(crosswalk.open(newline="", encoding="utf-8"))
        if row.get("primary_name") and row.get("cik") and row.get("lei")]
if len(rows) < 2:
    raise SystemExit("need two keyed crosswalk rows for planted conflict")
left, right = rows[0], rows[1]
record = {
    "obs_id": "planted-runtime-selected-key-conflict",
    "entity_type": "organisation",
    "surface": left["primary_name"],
    "keys": {"cik": left["cik"].split(";")[0], "lei": right["lei"].split(";")[0]},
    "source": {"channel": "file", "id": output.name, "lineage": "planted:real-key-conflict"},
}
output.write_text(json.dumps(record, sort_keys=True) + "\n", encoding="utf-8")
PY

python3 - "$work_dir/negative-key-conflict-registry" "$work_dir/negative-registry-before.json" <<'PY'
import hashlib
import json
import pathlib
import sys

root, output = map(pathlib.Path, sys.argv[1:])
def digest(path):
    value = hashlib.sha256()
    for file in sorted(path.rglob("*")):
        if file.is_file():
            value.update(str(file.relative_to(path)).encode())
            value.update(file.read_bytes())
    return value.hexdigest()
output.write_text(json.dumps({"sha256": digest(root)}) + "\n", encoding="utf-8")
PY

negative_session="$work_dir/negative-key-conflict-session"
negative_map="$work_dir/negative-registries.yaml"
cat >"$negative_map" <<EOF
version: canon.session.registries.v1
kinds:
  organisation:
    registry: $work_dir/negative-key-conflict-registry
    registry_id: firms
    prefix: ORG
    zero_pad: 3
    canonical_type: firm
    alias_file: 30-session-aliases.json
    key_file: 20-firm-keys.json
EOF
run_expected_json "negative key-conflict init" "$canon_bin" entity session init \
  --goal "BDC planted real key conflict" \
  --registries "$negative_map" \
  --work-dir "$negative_session" \
  --emit json
run_expected_json "negative key-conflict ingest" "$canon_bin" entity session ingest \
  "$negative_session" "$work_dir/negative-key-conflict.jsonl" --emit json
run_expected_json "negative key-conflict solve" "$canon_bin" entity session solve \
  "$negative_session" --rules "$rules" --emit json
run_expected_failure_json "negative key-conflict commit" "$canon_bin" entity session commit \
  "$negative_session" --next-version 2026-09-30-bd35-negative --emit json

python3 - "$RUN_JSON" "$work_dir/negative-key-conflict-registry" "$work_dir/negative-registry-before.json" <<'PY'
import hashlib
import json
import pathlib
import sys

value = json.loads(sys.argv[1])
if value.get("outcome") != "nothing_to_commit":
    raise SystemExit(f"planted conflict unexpectedly committed: {value}")
skipped = value.get("skipped", [])
if not any(
    row.get("reason") == "key_conflict"
    or "key_conflict" in row.get("conflict_reasons", [])
    or row.get("detail", {}).get("reason") == "key_conflict"
    for row in skipped
):
    raise SystemExit(f"planted conflict lacked key_conflict reason: {value}")

root, before = pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
def digest(path):
    result = hashlib.sha256()
    for file in sorted(path.rglob("*")):
        if file.is_file():
            result.update(str(file.relative_to(path)).encode())
            result.update(file.read_bytes())
    return result.hexdigest()
expected = json.loads(before.read_text(encoding="utf-8"))["sha256"]
actual = digest(root)
if actual != expected:
    raise SystemExit(f"refused conflict mutated the registry: {expected} != {actual}")
print(json.dumps({
    "outcome": value["outcome"],
    "registry_unchanged": True,
    "skipped": skipped,
}, sort_keys=True))
PY

printf '\nproof_complete=true\n'
printf 'countermetric=source rows dropped or unresolved are recorded in source-manifest.json and command logs; no precision claim\n'
printf 'ticker_handling=gold-set ticker slugs were retained in source metadata and not committed as keys\n'
printf 'served_registry=%s (untouched)\n' "$registry_source"
printf 'retained_artifacts=%s\n' "$work_dir"
