"""Step 0 recorder for the data-center recovery protocol (docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md).

Pins and reproduces the Pryor failure before anything is changed, and writes one immutable baseline record.
It runs, from a clean worktree at a frozen revision:
  A1  the response's own standard-library objective check (extracted verbatim from the response, Appendix A1)
  A2  `canon geo solve --request <retained Pryor compilation>` with the worktree's binary, compared with the historical manifest
and records stdout/stderr digests, exit codes, versions, budgets and the input digest. It changes nothing.

Usage: python3 step0_baseline.py <worktree> <canon-binary> [<extra-canon-binary-label=path> ...] > step0_baseline_record.json
"""
import hashlib
import json
import platform
import re
import subprocess
import sys
from pathlib import Path

wt = Path(sys.argv[1]).resolve()
binary = sys.argv[2]
extras = dict(a.split("=", 1) for a in sys.argv[3:])


def run(cmd, cwd=None, stdin=None):
    p = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, input=stdin)
    return {"cmd": " ".join(map(str, cmd)), "exit_code": p.returncode, "stdout_sha256": hashlib.sha256(p.stdout.encode()).hexdigest(),
            "stdout_bytes": len(p.stdout.encode()), "stderr": p.stderr.strip()[:500], "_stdout": p.stdout}


test = wt / "docs/geo_datacenter_site_test_2026-09-28"
inp = test / "runs/02_pryor_arm_d/pryor.evidence_compilation.json"
raw_in = inp.read_bytes()
compilation = json.loads(raw_in)
req = compilation["composition_request"]
manifest = json.loads((test / "data/solve_manifest.json").read_text())
hist = next(x for x in manifest if x["file"] == "02_pryor_arm_d/pryor.solve.json")

record = {
    "purpose": "Step 0 baseline: pin and reproduce the Pryor superset regression before any change. Not a site-accuracy measurement.",
    "repository_revision": run(["git", "rev-parse", "HEAD"], cwd=wt)["_stdout"].strip(),
    "worktree_status_lines": len([l for l in run(["git", "status", "--short"], cwd=wt)["_stdout"].splitlines() if l.strip()]),
    "versions": {"rustc": run(["rustc", "--version"])["_stdout"].strip(), "cargo": run(["cargo", "--version"])["_stdout"].strip(),
                 "python": platform.python_version(), "platform": platform.platform()},
    "canon_binary": {"path": binary, "version": run([binary, "--version"])["_stdout"].strip()},
    "input": {"path": str(inp.relative_to(wt)), "sha256": hashlib.sha256(raw_in).hexdigest(), "bytes": len(raw_in),
              "budgets": {"max_assignments": req["max_assignments"], "max_materialized_models": req["max_materialized_models"]},
              "candidates": len(req["universe"]["buildings"]), "hard_constraints": len(req.get("hard_constraints", [])),
              "soft_preferences": len(req["soft_preferences"])},
    "historical_manifest_entry": hist,
}

# A1: extract the response's own script verbatim and run it on the retained input
doc = (wt / "docs/RESPONSE_CANON_GEO_DATACENTER_SITE_TEST.md").read_text()
m = re.search(r"python3 - \"\$INPUT\" <<'PY' \| tee \"\$EVIDENCE/pryor_objective_check.json\"\n(.*?)\nPY\n", doc, re.S)
a1 = run([sys.executable, "-", str(inp)], stdin=m.group(1))
record["A1_objective_check"] = {k: v for k, v in a1.items() if k != "_stdout"}
try:
    record["A1_objective_check"]["result"] = json.loads(a1["_stdout"])
except ValueError:
    record["A1_objective_check"]["result"] = None


def replay(label, path):
    s = run([path, "geo", "solve", "--request", str(inp)])
    out = {k: v for k, v in s.items() if k != "_stdout"}
    out["historical_byte_match"] = s["stdout_sha256"] == hist["sha256"]
    try:
        solve = json.loads(s["_stdout"])
        ranked = solve.get("soft_ranked", [])
        out["status"] = solve.get("status")
        out["residual_model_count"] = solve.get("summary", {}).get("residual_model_count")
        out["rank1_cost"] = ranked[0]["cost"] if ranked else None
        out["rank1_size"] = len(ranked[0]["model"]["buildings"]) if ranked else None
        out["rank1_is_full_universe"] = bool(ranked) and len(ranked[0]["model"]["buildings"]) == record["input"]["candidates"]
        out["models_at_rank1_cost"] = sum(1 for r in ranked if r["cost"] == ranked[0]["cost"]) if ranked else None
    except (ValueError, KeyError):
        out["parse_error"] = "not the expected solve object"
    return out


record["A2_replay_pinned_binary"] = replay("pinned", binary)
for label, path in extras.items():
    record[f"A2_replay_{label}"] = replay(label, path)

pin = record["A2_replay_pinned_binary"]
record["verdict"] = {
    "superset_signature_reproduced": bool(record["A1_objective_check"].get("result", {}) and
                                          record["A1_objective_check"]["result"].get("full_universe_is_unique_optimum")) and
                                     bool(pin.get("rank1_is_full_universe")) and pin.get("models_at_rank1_cost") == 1,
    "historical_byte_match_on_pinned_binary": pin.get("historical_byte_match"),
    "drift_explained": "n/a" if pin.get("historical_byte_match") else "STOP: retain both identities and explain before proceeding",
}
print(json.dumps(record, indent=2))
