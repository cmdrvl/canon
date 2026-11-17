#!/usr/bin/env bash
set -euo pipefail

# bd-vdcw.34: real BDC cross-fund proof through Canon public commands.
# All writes remain under the retained work directory. This script never
# deletes it and never redirects a command's stderr away from the operator.

registry_source="/Users/zac/Source/cmdrvl/cmdrvl-context/artifacts/firms/registry"
gold_dir="/Users/zac/Source/cmdrvl/cmdrvl-soi/gold_sets"
work_dir=""
canon_bin="${CANON_BIN:-}"
seed="bd-vdcw.34-bdc-v1"
sample_count=200
labels_file=""
decision_review=""

usage() {
  cat <<'EOF'
usage: scripts/entity-session-bdc-proof.sh [options]
  --registry-source DIR   read-only served organisations registry to copy
  --gold-dir DIR          BDC gold-set directory
  --work-dir DIR          retained, initially empty output directory
  --canon-bin PATH        canon executable (or CANON_BIN)
  --seed TEXT             runtime holdout/sample seed
  --sample-count N        blind sample count per tier (default: 200)
  --labels JSONL          operator labels for session sample --score
  --decision-review JSON  operator review decisions for different import
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --registry-source) registry_source="${2:?missing registry source}"; shift 2 ;;
    --gold-dir) gold_dir="${2:?missing gold directory}"; shift 2 ;;
    --work-dir) work_dir="${2:?missing work directory}"; shift 2 ;;
    --canon-bin) canon_bin="${2:?missing canon executable}"; shift 2 ;;
    --seed) seed="${2:?missing seed}"; shift 2 ;;
    --sample-count) sample_count="${2:?missing sample count}"; shift 2 ;;
    --labels) labels_file="${2:?missing labels file}"; shift 2 ;;
    --decision-review) decision_review="${2:?missing review file}"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$canon_bin" ]]; then
  if [[ -x target/debug/canon ]]; then canon_bin="target/debug/canon"
  elif [[ -x target/release/canon ]]; then canon_bin="target/release/canon"
  else canon_bin="$(command -v canon || true)"
  fi
fi
[[ -n "$canon_bin" && -x "$canon_bin" ]] || { printf 'set CANON_BIN or --canon-bin\n' >&2; exit 2; }
for path in "$registry_source" "$gold_dir"; do
  [[ -d "$path" ]] || { printf 'missing proof directory: %s\n' "$path" >&2; exit 2; }
done
[[ "$sample_count" =~ ^[0-9]+$ && "$sample_count" -gt 0 ]] || { printf '%s\n' '--sample-count must be positive' >&2; exit 2; }
[[ -z "$labels_file" || -f "$labels_file" ]] || { printf 'missing labels: %s\n' "$labels_file" >&2; exit 2; }
[[ -z "$decision_review" || -f "$decision_review" ]] || { printf 'missing decision file: %s\n' "$decision_review" >&2; exit 2; }
command -v b3sum >/dev/null 2>&1 || { printf 'b3sum is required\n' >&2; exit 2; }

if [[ -z "$work_dir" ]]; then work_dir="$(mktemp -d "${TMPDIR:-/tmp}/canon-bdc-proof.XXXXXX")"
else mkdir -p "$work_dir"; fi
work_dir="$(cd "$work_dir" && pwd -P)"
if [[ -n "$(find "$work_dir" -mindepth 1 -print -quit)" ]]; then
  printf 'refusing non-empty retained work directory: %s\n' "$work_dir" >&2
  exit 2
fi
mkdir -p "$work_dir/commands"
printf 'work_dir=%s\nserved_registry_untouched=true\nselection_seed=%s\nrevision=%s\n' "$work_dir" "$seed" "$(git rev-parse HEAD 2>/dev/null || printf unknown)"

safe() { printf '%s' "$1" | tr -c 'A-Za-z0-9_.-' '_'; }
run_json() {
  local label="$1"; shift; local name; name="$(safe "$label")"; local out="$work_dir/commands/$name.stdout"
  printf '%q ' "$@">"$work_dir/commands/$name.command"; printf '\n'>>"$work_dir/commands/$name.command"
  printf '\n[%s] command:' "$label"; printf ' %q' "$@"; printf '\n'
  local status; set +e; "$@" >"$out"; status="$?"; set -e
  printf '%s\n' "$status" >"$work_dir/commands/$name.status"; cat "$out"
  RUN_STATUS="$status"; RUN_STDOUT_PATH="$out"
}
run_ok() {
  run_json "$@"
  [[ "$RUN_STATUS" -eq 0 ]] || { printf '[%s] unexpected exit=%s\n' "$1" "$RUN_STATUS" >&2; exit "$RUN_STATUS"; }
}

snapshot_closures() {
  python3 - "$1" "$2" <<'PY'
import json, pathlib, sys
root, out = map(pathlib.Path, sys.argv[1:])
found = set()
def walk(value):
    if isinstance(value, dict):
        if isinstance(value.get("closure_hash"), str): found.add(value["closure_hash"])
        for child in value.values(): walk(child)
    elif isinstance(value, list):
        for child in value: walk(child)
for path in sorted(root.rglob("*")):
    if not path.is_file() or path.suffix not in {".json", ".jsonl"}: continue
    values = [json.loads(line) for line in path.read_text().splitlines() if line.strip()] if path.suffix == ".jsonl" else [json.loads(path.read_text())]
    for value in values: walk(value)
out.write_text(json.dumps(sorted(found), indent=2) + "\n")
PY
}

snapshot_tree() {
  python3 - "$1" "$2" <<'PY'
import hashlib, json, pathlib, sys
root, out = map(pathlib.Path, sys.argv[1:])
value = {}
for path in sorted(root.rglob("*")):
    if path.is_file() and path.name != "_index.sqlite":
        value[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
out.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
PY
}

cat >"$work_dir/verify-closures.py" <<'PY'
import collections, json, pathlib, subprocess, sys
label, registry, commit, exported, baseline, summary = map(pathlib.Path, sys.argv[1:])
def walk(value):
    out = set()
    if isinstance(value, dict):
        if isinstance(value.get("closure_hash"), str): out.add(value["closure_hash"])
        for child in value.values(): out |= walk(child)
    elif isinstance(value, list):
        for child in value: out |= walk(child)
    return out
def b3(path): return subprocess.check_output(["b3sum", str(path)], text=True).split()[0]
commit_value = json.loads(commit.read_text()); export = json.loads(exported.read_text())
closures = export.get("closures")
if not isinstance(closures, list) or not closures: raise SystemExit(f"{label}: no closures")
emitted = {row.get("closure_hash") for row in closures}
if None in emitted or not walk(commit_value) <= emitted: raise SystemExit(f"{label}: commit closure mismatch")
root = pathlib.Path(export["out"]); levels = collections.Counter()
for row in closures:
    digest = row["closure_hash"]; directory = row["directory"]
    if directory != digest.removeprefix("blake3:"): raise SystemExit(f"{label}: directory mismatch")
    sensitivity = row.get("sensitivity", {})
    if sensitivity.get("level") not in {"public", "restricted"}: raise SystemExit(f"{label}: sensitivity missing")
    levels[sensitivity["level"]] += 1
    closure = root / directory / "closure.json"
    if not closure.is_file() or b3(closure) != directory: raise SystemExit(f"{label}: closure hash mismatch")
    listed = {entry.get("path"): entry.get("blake3") for entry in row.get("files", [])}
    if listed.get(f"{directory}/closure.json") != digest: raise SystemExit(f"{label}: self hash mismatch")
    for relative, listed_hash in listed.items():
        path = root / relative
        if not path.is_file() or b3(path) != str(listed_hash).removeprefix("blake3:"):
            raise SystemExit(f"{label}: file hash mismatch: {path}")
registry_hashes = set()
for path in sorted(registry.rglob("*")):
    if not path.is_file() or path.suffix not in {".json", ".jsonl"}: continue
    values = [json.loads(line) for line in path.read_text().splitlines() if line.strip()] if path.suffix == ".jsonl" else [json.loads(path.read_text())]
    for value in values: registry_hashes |= walk(value)
old = set(json.loads(baseline.read_text()))
if not registry_hashes - old <= emitted: raise SystemExit(f"{label}: registry closure missing from export")
result = {"label": str(label), "commit_closure_count": len(walk(commit_value)), "emitted_closure_count": len(emitted), "new_registry_closure_count": len(registry_hashes - old), "sensitivity_counts": dict(sorted(levels.items()))}
summary.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
print(json.dumps(result, sort_keys=True))
PY

python3 - "$registry_source" "$work_dir/staging-registry" "$work_dir/preexisting-aliases.json" <<'PY'
import json, pathlib, shutil, sys
source, target, baseline = map(pathlib.Path, sys.argv[1:])
shutil.copytree(source, target)
value = {}
for path in sorted(source.glob("*.json")):
    if path.name in {"registry.json", "20-firm-keys.json"}: continue
    data = json.loads(path.read_text())
    if isinstance(data, list):
        value[path.name] = sorted((row.get("input"), row.get("canonical_id"), row.get("canonical_type")) for row in data if isinstance(row, dict) and "input" in row and "canonical_id" in row)
baseline.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
PY
registry="$work_dir/staging-registry"

python3 - "$gold_dir" "$work_dir/day-one-ciks.txt" "$work_dir/heldout-a-ciks.txt" "$work_dir/heldout-b-ciks.txt" "$work_dir/selection.json" "$seed" <<'PY'
import hashlib, json, pathlib, sys
gold, day_one, held_a, held_b, receipt = map(pathlib.Path, sys.argv[1:6])
seed = sys.argv[6]
ciks = sorted({json.loads(p.read_text())["source"]["cik"] for p in gold.glob("*.goldset.v*.json")})
ranked = sorted(ciks, key=lambda cik: hashlib.sha256(f"{seed}|{cik}".encode()).hexdigest())
if len(ciks) != 54: raise SystemExit(f"expected 54 funds, found {len(ciks)}")
held = ranked[:5]
for path, values in ((day_one, sorted(ranked[5:])), (held_a, sorted(held[:2])), (held_b, sorted(held[2:]))):
    path.write_text("".join(f"{v}\n" for v in values))
receipt.write_text(json.dumps({"version":"canon.entity.session.bdc.selection.v1","selection_seed":seed,"fund_count":len(ciks),"day_one_ciks":sorted(ranked[5:]),"day_two_same_session_ciks":sorted(held[:2]),"day_two_new_session_ciks":sorted(held[2:]),"ranked_ciks":ranked,"selection_method":"sha256(seed|fund_cik) ascending; first five held out"}, indent=2, sort_keys=True) + "\n")
PY

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
converter="$script_dir/entity_sessions/bdc_goldsets_to_session.py"
python3 "$converter" --mode borrowers --gold-dir "$gold_dir" --ciks-file "$work_dir/day-one-ciks.txt" --pick all --out "$work_dir/day-one.jsonl"
python3 "$converter" --mode borrowers --gold-dir "$gold_dir" --ciks-file "$work_dir/heldout-a-ciks.txt" --pick all --out "$work_dir/heldout-a.jsonl"
python3 "$converter" --mode borrowers --gold-dir "$gold_dir" --ciks-file "$work_dir/heldout-b-ciks.txt" --pick all --out "$work_dir/heldout-b.jsonl"
cat "$work_dir/day-one.jsonl" "$work_dir/heldout-a.jsonl" >"$work_dir/day-two-same-session.jsonl"

cat >"$work_dir/registries.yaml" <<EOF
version: canon.session.registries.v1
kinds:
  organisation:
    registry: $registry
    registry_id: firms
    prefix: ORG
    zero_pad: 3
    canonical_type: firm
    alias_file: 30-session-aliases.json
    key_file: 20-firm-keys.json
EOF
cat >"$work_dir/rules.yaml" <<'EOF'
version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
EOF
cat >"$work_dir/strategy.yaml" <<'EOF'
block:
  preflight: false
  max_candidates_per_run: 5000000
EOF
cat >"$work_dir/profile.yaml" <<'EOF'
profile: bdc_cross_fund_public_proof
version: 0.1.0
entity_type: organisation
identity_semantics: same_normalized_name
canonical_type: firm
required_fields: [source_row_id, surface]
normalized_views:
  core:
    operators: [strip_footnote_marks, strip_security_tail, name_structure, preserve_legal_form_with_bare]
  row_key:
    operators: [ascii_trim_upper]
  investment_type:
    operators: [identity]
  maturity_date:
    operators: [identity]
  spread:
    operators: [identity]
  reference_rate:
    operators: [identity]
prepare:
  primary_surface_fields: [surface]
  canonical_surface_normalized_view: core
  normalized_view_fields:
    core: surface
    row_key: source_row_id
    investment_type: investment_type
    maturity_date: maturity_date
    spread: spread
    reference_rate: reference_rate
  surface_key_fields: [row_key]
  nullable_fields: [investment_type, maturity_date, spread, reference_rate]
  provenance_fields: [source_row_id, origin]
blocking:
  operators:
    - op: exact_identity
      view: core
    - op: composite_key
      views: [maturity_date, investment_type]
evidence:
  support:
    - op: exact_view
      view: core
    - op: categorical_match
      params:
        field: investment_type
    - op: categorical_match
      params:
        field: reference_rate
    - op: date_exact
      params:
        field: maturity_date
    - op: numeric_within_tolerance
      params:
        field: spread
        absolute: "0.05"
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
  aliases: bdc_cross_fund_public_proof.aliases
  distinct: bdc_cross_fund_public_proof.distinct
  relations: bdc_cross_fund_public_proof.relations
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
EOF

python3 - "$work_dir/day-one.jsonl" "$work_dir/heldout-a.jsonl" "$work_dir/heldout-b.jsonl" "$work_dir/source-manifest.json" <<'PY'
import json, pathlib, sys
out = {}
for text in sys.argv[1:4]:
    path = pathlib.Path(text)
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    out[path.name] = {"rows":len(rows),"distinct_surfaces":len({r["surface"] for r in rows}),"lineages":len({r["source"]["lineage"] for r in rows})}
pathlib.Path(sys.argv[4]).write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")
print(json.dumps(out, sort_keys=True))
PY

session="$work_dir/day-one-session"
snapshot_closures "$registry" "$work_dir/closures-before-day-one.json"
run_ok "day-one-init" "$canon_bin" entity session init --goal "BDC cross-fund Canon evidence proof day one" --registries "$work_dir/registries.yaml" --work-dir "$session" --emit json
run_ok "day-one-ingest" "$canon_bin" entity session ingest "$session" "$work_dir/day-one.jsonl" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-one-ingest.json"
run_ok "day-one-derive" "$canon_bin" entity session derive "$session" --profile "$work_dir/profile.yaml" --strategy "$work_dir/strategy.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-one-derive.json"
run_ok "day-one-solve" "$canon_bin" entity session solve "$session" --rules "$work_dir/rules.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-one-solve.json"
for stratum in tier_b tier_c standalone; do
  run_ok "day-one-sample-$stratum" "$canon_bin" entity session sample "$session" --stratum "$stratum" --count "$sample_count" --seed "$seed:$stratum" --blind --emit jsonl
  cp "$RUN_STDOUT_PATH" "$work_dir/day-one-sample-$stratum.jsonl"
done
python3 - "$work_dir" "$sample_count" <<'PY'
import json, pathlib, sys
root, count = pathlib.Path(sys.argv[1]), int(sys.argv[2])
rows=[]; headers=[]
for name in ("tier_b","tier_c","standalone"):
    lines=[line for line in (root/f"day-one-sample-{name}.jsonl").read_text().splitlines() if line.strip()]
    if not lines: raise SystemExit(f"empty sample for {name}")
    headers.append(json.loads(lines[0])); rows.extend(json.loads(line) for line in lines[1:])
header=dict(headers[0]); header.update(operator_labeling_sheet=True,sample_strata=["tier_b","tier_c","standalone"],sample_count_requested_per_stratum=count)
(root/"operator-label-sheet.jsonl").write_text(json.dumps(header,sort_keys=True)+"\n"+"\n".join(json.dumps(row,sort_keys=True) for row in rows)+"\n")
(root/"sample-manifest.json").write_text(json.dumps({"headers":headers,"row_count":len(rows),"rows_by_source":{name:sum(1 for line in (root/f"day-one-sample-{name}.jsonl").read_text().splitlines() if line.strip())-1 for name in ("tier_b","tier_c","standalone")}},indent=2,sort_keys=True)+"\n")
print(json.dumps({"sample_rows":len(rows),"operator_label_sheet":str(root/"operator-label-sheet.jsonl")},sort_keys=True))
PY
if [[ -n "$labels_file" ]]; then
  run_ok "operator-label-score" "$canon_bin" entity session sample "$session" --score "$labels_file" --emit json
  cp "$RUN_STDOUT_PATH" "$work_dir/operator-label-score.json"
else
  printf 'operator_label_status=BLOCKED (labels absent; no score or precision claim)\n'
fi

run_ok "day-one-commit" "$canon_bin" entity session commit "$session" --next-version 2026-11-17-bdc-day-one --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-one-commit.json"
day_one_hash="$(python3 - "$work_dir/day-one-commit.json" <<'PY'
import json, pathlib, sys
value=json.loads(pathlib.Path(sys.argv[1]).read_text())
if value.get("outcome")!="committed" or not value.get("minted") and not value.get("attached"): raise SystemExit(f"day-one commit wrote no outcomes: {value}")
print(value["registry"]["new"]["content_hash"])
PY
)"
printf 'day_one_registry_content_hash=%s\n' "$day_one_hash"
run_ok "day-one-evidence-closure-export" "$canon_bin" entity session export "$session" --format evidence-closure --out "$work_dir/day-one-evidence-closures" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-one-evidence-closure.json"
python3 "$work_dir/verify-closures.py" day-one "$registry" "$work_dir/day-one-commit.json" "$work_dir/day-one-evidence-closure.json" "$work_dir/closures-before-day-one.json" "$work_dir/day-one-closure-summary.json"
run_ok "day-one-registry-lint" "$canon_bin" registry lint "$registry" --emit json

python3 - "$registry" "$work_dir/seeded-variants.jsonl" <<'PY'
import hashlib, json, pathlib, sys
registry, out=map(pathlib.Path,sys.argv[1:]); rows=[]
path = registry / "30-session-aliases.json"
if path.is_file():
    value=json.loads(path.read_text())
    if isinstance(value,list): rows.extend(r for r in value if isinstance(r,dict) and isinstance(r.get("input"),str) and isinstance(r.get("canonical_id"),str))
rows=sorted(rows,key=lambda r:hashlib.sha256(f"bd-vdcw.34|{r['input']}".encode()).hexdigest())
unique=[]; seen=set()
for row in rows:
    if row["input"] in seen: continue
    seen.add(row["input"]); unique.append(row)
if len(unique)<50: raise SystemExit(f"only {len(unique)} unique aliases available")
selected=unique[:50]
out.write_text("".join(json.dumps({"name":r["input"]},sort_keys=True)+"\n" for r in selected))
(out.parent/"seeded-variants-manifest.json").write_text(json.dumps({"seed":"bd-vdcw.34","count":50,"canonical_ids":[r["canonical_id"] for r in selected]},indent=2,sort_keys=True)+"\n")
PY
run_ok "seeded-variant-plain-lookup" "$canon_bin" "$work_dir/seeded-variants.jsonl" --registry "$registry" --column name --no-witness --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/seeded-variant-lookup.json"
python3 - "$work_dir/seeded-variant-lookup.json" <<'PY'
import json, pathlib, sys
value=json.loads(pathlib.Path(sys.argv[1]).read_text()); summary=value.get("summary",{})
if summary.get("resolved")!=50 or summary.get("unresolved")!=0: raise SystemExit(f"seeded lookup incomplete: {summary}")
print(json.dumps(summary,sort_keys=True))
PY

cp -R "$registry" "$work_dir/registry-before-day-two"
snapshot_closures "$registry" "$work_dir/closures-before-day-two.json"
run_ok "day-two-same-session-ingest" "$canon_bin" entity session ingest "$session" "$work_dir/heldout-a.jsonl" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-two-ingest.json"
run_ok "day-two-same-session-derive" "$canon_bin" entity session derive "$session" --profile "$work_dir/profile.yaml" --strategy "$work_dir/strategy.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-two-derive.json"
run_ok "day-two-same-session-solve" "$canon_bin" entity session solve "$session" --rules "$work_dir/rules.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-two-solve.json"
run_ok "day-two-same-session-commit" "$canon_bin" entity session commit "$session" --next-version 2026-11-17-bdc-day-two --expect-base "$day_one_hash" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-two-commit.json"
day_two_hash="$(python3 - "$work_dir/day-two-commit.json" <<'PY'
import json, pathlib, sys
value=json.loads(pathlib.Path(sys.argv[1]).read_text())
if value.get("outcome")!="committed": raise SystemExit(f"day-two commit failed: {value}")
print(value["registry"]["new"]["content_hash"])
PY
)"
printf 'day_two_registry_content_hash=%s\n' "$day_two_hash"
run_ok "day-two-evidence-closure-export" "$canon_bin" entity session export "$session" --format evidence-closure --out "$work_dir/day-two-evidence-closures" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-two-evidence-closure.json"
python3 "$work_dir/verify-closures.py" day-two "$registry" "$work_dir/day-two-commit.json" "$work_dir/day-two-evidence-closure.json" "$work_dir/closures-before-day-two.json" "$work_dir/day-two-closure-summary.json"
run_ok "day-two-registry-lint" "$canon_bin" registry lint "$registry" --emit json
snapshot_tree "$registry" "$work_dir/registry-before-noop.json"
run_ok "day-two-no-op-commit" "$canon_bin" entity session commit "$session" --next-version 2026-11-17-bdc-day-two-noop --expect-base "$day_two_hash" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/day-two-no-op.json"
python3 - "$work_dir/day-two-no-op.json" "$work_dir/registry-before-noop.json" "$registry" <<'PY'
import hashlib, json, pathlib, sys
receipt=json.loads(pathlib.Path(sys.argv[1]).read_text())
if receipt.get("outcome") not in {"nothing_to_commit", "already_committed"}:
    raise SystemExit(f"repeat commit was not no-op: {receipt}")
if receipt.get("registry", {}).get("changed_files"):
    raise SystemExit("no-op commit reported changed registry files")
before=json.loads(pathlib.Path(sys.argv[2]).read_text())
root=pathlib.Path(sys.argv[3]); after={}
for path in sorted(root.rglob("*")):
    if path.is_file() and path.name!="_index.sqlite": after[path.relative_to(root).as_posix()]=hashlib.sha256(path.read_bytes()).hexdigest()
if before != after: raise SystemExit("no-op commit changed registry files")
pathlib.Path(sys.argv[1]).with_name("registry-after-noop.json").write_text(json.dumps(after,sort_keys=True))
PY

python3 - "$work_dir/day-one-commit.json" "$work_dir/day-two-commit.json" <<'PY'
import json, pathlib, sys
first, second = (json.loads(pathlib.Path(path).read_text()) for path in sys.argv[1:])
def rows(value):
    return [row for kind in ("minted", "attached") for row in value.get(kind, [])]
first_rows, second_rows = rows(first), rows(second)
first_clusters = {row.get("cluster_id") for row in first_rows}
second_clusters = {row.get("cluster_id") for row in second_rows}
if first_clusters & second_clusters:
    raise SystemExit(f"day two re-committed day-one clusters: {sorted(first_clusters & second_clusters)}")
first_aliases = {alias.get("input") for row in first_rows for alias in row.get("aliases", [])}
second_aliases = [alias.get("input") for row in second_rows for alias in row.get("aliases", [])]
if first_aliases & set(second_aliases):
    raise SystemExit(f"day two repeated aliases: {sorted(first_aliases & set(second_aliases))}")
if len(second_aliases) != len(set(second_aliases)):
    raise SystemExit("day two contains a duplicate alias")
print(json.dumps({"day_one_rows":len(first_rows),"day_two_rows":len(second_rows),"day_two_new_aliases":len(second_aliases)},sort_keys=True))
PY

python3 - "$work_dir/heldout-b.jsonl" "$work_dir/step-eight-input.jsonl" <<'PY'
import json, pathlib, sys
source,out=map(pathlib.Path,sys.argv[1:]); rows=[json.loads(line) for line in source.read_text().splitlines() if line.strip()]
out.write_text("".join(json.dumps({"name":r["surface"]},sort_keys=True)+"\n" for r in rows))
PY
run_ok "step-eight-exact-prelookup" "$canon_bin" "$work_dir/step-eight-input.jsonl" --registry "$registry" --column name --no-witness --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/step-eight-prelookup.json"
python3 - "$work_dir/heldout-b.jsonl" "$work_dir/step-eight-prelookup.json" "$work_dir/step-eight-unresolved.jsonl" <<'PY'
import json, pathlib, sys
source,lookup,out=map(pathlib.Path,sys.argv[1:]); value=json.loads(lookup.read_text())
unresolved={r.get("input") for r in value.get("unresolved",[]) if r.get("input") is not None}
rows=[json.loads(line) for line in source.read_text().splitlines() if line.strip()]; selected=[r for r in rows if r["surface"] in unresolved]
out.write_text("".join(json.dumps(r,sort_keys=True)+"\n" for r in selected))
print(json.dumps({"input_rows":len(rows),"exactly_resolved_rows":len(rows)-len(selected),"unresolved_rows":len(selected),"unresolved_surfaces":len({r["surface"] for r in selected})},sort_keys=True))
PY
step_session="$work_dir/step-eight-session"
run_ok "step-eight-init" "$canon_bin" entity session init --goal "BDC cross-fund Canon evidence proof new session" --registries "$work_dir/registries.yaml" --work-dir "$step_session" --emit json
run_ok "step-eight-ingest" "$canon_bin" entity session ingest "$step_session" "$work_dir/step-eight-unresolved.jsonl" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/step-eight-ingest.json"
run_ok "step-eight-derive" "$canon_bin" entity session derive "$step_session" --profile "$work_dir/profile.yaml" --strategy "$work_dir/strategy.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/step-eight-derive.json"
run_ok "step-eight-solve" "$canon_bin" entity session solve "$step_session" --rules "$work_dir/rules.yaml" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/step-eight-solve.json"
run_ok "step-eight-commit" "$canon_bin" entity session commit "$step_session" --next-version 2026-11-17-bdc-day-three --expect-base "$day_two_hash" --emit json
cp "$RUN_STDOUT_PATH" "$work_dir/step-eight-commit.json"
python3 - "$work_dir/step-eight-commit.json" <<'PY'
import json, pathlib, sys
value=json.loads(pathlib.Path(sys.argv[1]).read_text())
if value.get("outcome")!="committed": raise SystemExit(f"step eight failed: {value}")
if value.get("minted"): raise SystemExit(f"step eight minted IDs: {value['minted']}")
if not value.get("attached"): raise SystemExit("step eight attached no spelling to an earlier ID")
print(json.dumps({"minted":0,"attached":len(value.get("attached",[])),"skipped":len(value.get("skipped",[]))},sort_keys=True))
PY
run_ok "step-eight-registry-lint" "$canon_bin" registry lint "$registry" --emit json

if [[ -n "$decision_review" ]]; then
  run_ok "operator-different-review-import" "$canon_bin" entity review import "$decision_review" --session "$step_session" --reviewer Zac --emit json
  cp "$RUN_STDOUT_PATH" "$work_dir/operator-different-review-import.json"
  [[ -s "$registry/_escrow/cannot_link.jsonl" ]] || { printf 'decision import wrote no cannot_link.jsonl\n' >&2; exit 1; }
  printf 'different_veto_status=imported\n'
else
  printf 'different_veto_status=BLOCKED (operator decision absent)\n'
fi

python3 - "$work_dir/preexisting-aliases.json" "$registry" "$session" "$step_session" "$work_dir/evidence-provenance.json" <<'PY'
import json, pathlib, sys
baseline,registry,session,step,out=map(pathlib.Path,sys.argv[1:])
old=json.loads(baseline.read_text()); current={}
for path in sorted(registry.glob("*.json")):
    if path.name in {"registry.json","20-firm-keys.json"}: continue
    value=json.loads(path.read_text())
    if isinstance(value,list): current[path.name]=sorted((r.get("input"),r.get("canonical_id"),r.get("canonical_type")) for r in value if isinstance(r,dict) and "input" in r and "canonical_id" in r)
for name, rows in old.items():
    if current.get(name,[])[:len(rows)] != rows: raise SystemExit(f"pre-existing aliases changed in {name}")
allowed={"canon","human"}; asserted=[]
for root in (session,step):
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.suffix not in {".json",".jsonl"}: continue
        lines=path.read_text().splitlines() if path.suffix==".jsonl" else [path.read_text()]
        for line in lines:
            if not line.strip(): continue
            try: value=json.loads(line)
            except json.JSONDecodeError: continue
            stack=[value]
            while stack:
                node=stack.pop()
                if isinstance(node,dict):
                    if isinstance(node.get("asserted_by"),str):
                        asserted.append(node["asserted_by"])
                        if node["asserted_by"].split(":",1)[0] not in allowed: raise SystemExit(f"unexpected asserted_by {node['asserted_by']}")
                    stack.extend(node.values())
                elif isinstance(node,list): stack.extend(node)
summary={"asserted_by_count":len(asserted),"asserted_by_prefixes":sorted({v.split(":",1)[0] for v in asserted}),"unexpected_count":0}
out.write_text(json.dumps(summary,indent=2,sort_keys=True)+"\n"); print(json.dumps(summary,sort_keys=True))
PY

python3 - "$work_dir" "$seed" <<'PY'
import json, pathlib, sys
root,seed=pathlib.Path(sys.argv[1]),sys.argv[2]
def load(name): return json.loads((root/name).read_text())
one=load("day-one-solve.json"); d2=load("day-two-commit.json"); d3=load("step-eight-commit.json")
summary={"proof_class":"real_data_public_command_replay","selection_seed":seed,"source_manifest":load("source-manifest.json"),"sample_manifest":load("sample-manifest.json"),"day_one_solve_hash":one.get("content_hash"),"day_one_counts":one.get("summary",one.get("counts",{})),"day_one_minted":len(load("day-one-commit.json").get("minted",[])),"day_one_attached":len(load("day-one-commit.json").get("attached",[])),"day_one_closure":load("day-one-closure-summary.json"),"day_two_minted":len(d2.get("minted",[])),"day_two_attached":len(d2.get("attached",[])),"day_two_noop":load("day-two-no-op.json").get("outcome"),"day_two_closure":load("day-two-closure-summary.json"),"step_eight_prelookup":load("step-eight-prelookup.json").get("summary",{}),"step_eight_minted":len(d3.get("minted",[])),"step_eight_attached":len(d3.get("attached",[])),"seeded_variant_lookup":load("seeded-variant-lookup.json").get("summary",{}),"evidence_provenance":load("evidence-provenance.json"),"operator_labels":"supplied" if (root/"operator-label-score.json").is_file() else "blocked","different_veto":"imported" if (root/"operator-different-review-import.json").is_file() else "blocked"}
(root/"proof-summary.json").write_text(json.dumps(summary,indent=2,sort_keys=True)+"\n"); print(json.dumps(summary,sort_keys=True))
PY
printf 'proof_complete=true\noperator_label_sheet=%s\nproof_summary=%s\nretained_work_dir=true\n' "$work_dir/operator-label-sheet.jsonl" "$work_dir/proof-summary.json"

