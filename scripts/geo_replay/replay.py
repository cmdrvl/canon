#!/usr/bin/env python3
"""Offline public-command replay. Retained integration is never live or accuracy proof."""
import argparse
import datetime
import gzip
import hashlib
import json
import math
import os
import pathlib
import platform
import signal
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
CATALOG = ROOT / "tests/fixtures/geo/replay/catalog.json"


class ReplayError(Exception):
    def __init__(self, code, **detail):
        super().__init__(json.dumps(dict(code=code, **detail), sort_keys=True))
        self.code = code


def require(condition, code, **detail):
    if not condition:
        raise ReplayError(code, **detail)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def pointer(value, path):
    for part in path.strip("/").split("/") if path else []:
        part = part.replace("~1", "/").replace("~0", "~")
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def safe(name):
    p = pathlib.PurePosixPath(name)
    require(bool(name) and not p.is_absolute() and ".." not in p.parts,
            "unsafe_member", member=name)


class _ResolvedCurrentAnswer(dict):
    """In-process marker; a catalog JSON field cannot bypass source validation."""


def resolve_case(case, catalog=None):
    if case.get("mode") in ("retained_resume", "evaluation_solve", "cost_observation") and not isinstance(case, _ResolvedCurrentAnswer):
        catalog = catalog or json.loads(CATALOG.read_bytes())
        sources = [row for row in catalog["cases"] if row["case_id"] == case["source_case_id"]]
        require(len(sources) == 1 and sources[0]["mode"] == "legacy_full_workflow", "invalid_full_source")
        permitted = {"case_id", "source_case_id", "mode", "tier", "proof_class", "role", "invariants", "expected_resume", "request", "retained_stdout"}
        require(case["proof_class"] == "retained" and not case.keys()-permitted, "invalid_full_source")
        require(case["role"] == {"retained_resume":"integrity_only", "evaluation_solve":"evaluation_only", "cost_observation":"cost_observation"}[case["mode"]], "invalid_full_source_role")
        return _ResolvedCurrentAnswer(dict(sources[0], **case, golden_changes=[]))
    if case.get("mode") != "current_answer" or isinstance(case, _ResolvedCurrentAnswer):
        return case
    require(case["proof_class"] == "retained", "proof_class_invalid", case_id=case["case_id"])
    catalog = catalog or json.loads(CATALOG.read_bytes())
    sources = [row for row in catalog["cases"] if row["case_id"] == case["source_case_id"]]
    require(len(sources) == 1 and sources[0]["mode"] == "legacy_full_workflow",
            "invalid_current_answer_source", case_id=case["case_id"])
    source = sources[0]
    permitted = {"case_id", "source_case_id", "mode", "tier", "proof_class", "invariants", "expected_current_answer"}
    require(not case.keys() - permitted, "invalid_current_answer_override", case_id=case["case_id"])
    merged = _ResolvedCurrentAnswer(dict(source, **case, golden_changes=[]))
    # Analysis stages are deliberately absent in current-answer mode. Preserve
    # every source kernel assertion; historical terminal status is read from
    # the immutable run, not from later approved legacy execution changes.
    kernel_checks = [check for check in source["invariants"]
                     if (check.get("artifact_id") or "").rsplit("/", 1)[-1] in CURRENT_UPSTREAM]
    merged["historical_invariants"] = kernel_checks+[dict(kind="run_status", value=source["retained"]["run_status"])]
    answer_id = source["run_bindings"][0]["node_id"].rsplit(".", 1)[0]+".answer/answer"
    standard = [dict(kind="run_status", value="ABSTAINED"),
                dict(kind="pointer_equals", pointer="/phase", value="SOLVED")]
    for path, value in [("/further_work/source", "not_in_scope"),
                        ("/further_work/action_discovery", "not_attempted"),
                        ("/acceptance/status", "no_policy_declared"),
                        ("/extent_and_reach/complete_extent_claimed", False)]:
        standard.append(dict(kind="pointer_equals", artifact_id=answer_id, pointer=path, value=value))
    merged["invariants"] = kernel_checks+standard+case["invariants"]
    return merged


def load_archive(case):
    case = resolve_case(case)
    require(case["proof_class"] == "retained", "proof_class_invalid",
            case_id=case["case_id"], field="proof_class")
    archive = case["archive"]
    path = ROOT / archive["path"]
    require(path.is_file(), "missing_member", path=str(path))
    data = path.read_bytes()
    require(sha(data) == archive["sha256"], "digest_mismatch", path=str(path))
    source = archive.get("measurement")
    if source:
        manifest = ROOT / source
        require(manifest.is_file(), "missing_member", path=str(manifest))
        expected = pointer(json.loads(manifest.read_bytes()), archive["source_pointer"])
        require(expected == archive.get("measurement_digest", archive["sha256"]),
                "digest_mismatch", path=str(manifest))
    if archive["format"] == "json_gz_files":
        document = json.loads(gzip.decompress(data))
        files = {}
        for name, contents in document["files"].items():
            safe(name)
            contents = contents.encode()
            require(sha(contents) == document["file_sha256"][name],
                    "digest_mismatch", member=name)
            files[name] = contents
        return files
    if archive["format"] == "loose_files":
        files = {}
        for name, expected in archive["members"].items():
            safe(name)
            member = path.parent / name
            require(member.is_file(), "missing_member", path=str(member))
            contents = member.read_bytes()
            require(sha(contents) == expected, "digest_mismatch", member=name)
            files[name] = contents
        return files
    if archive["format"] == "loose":
        return {path.name: data}
    files = {}
    with tarfile.open(path, "r:gz") as tar:
        for member in tar:
            safe(member.name)
            require(member.isfile() or member.isdir(), "unsafe_member", member=member.name)
            if member.isfile():
                require(member.size < 100_000_000, "unsafe_member", member=member.name)
                files[member.name] = tar.extractfile(member).read()
    return files


def get(files, name):
    require(name in files, "missing_member", member=name)
    return files[name]


def retained(case, files):
    case = resolve_case(case)
    if "before" in case:
        verify_preparation(case)
    if case["mode"] in ("leaf_replay", "evidence_only", "measurement_replay"):
        for name, expected in case["member_sha256"].items():
            require(sha(get(files, name)) == expected, "digest_mismatch", member=name)
        return None
    if case.get("expected_refusal"):
        for name, expected in case["member_sha256"].items():
            require(sha(get(files, name)) == expected, "digest_mismatch", member=name)
        require(sha(get(files, case["retained_run"])) == case["retained_stdout_sha256"], "digest_mismatch", field="retained_refusal")
        command = json.loads(get(files, case["retained_argv_source"]))["argv"]
        recorded = [command[i + 1] for i, arg in enumerate(command) if arg == "--input"]
        configured = [b["node_id"] + ":" + b["binding_id"] + "=" + b["retained_path"] for b in case["run_bindings"]]
        require(recorded == configured, "argv_binding_drift", case_id=case["case_id"])
        return None
    run = json.loads(get(files, case["retained_run"]))
    original = dict(run_semantic_hash=run["semantic_hash"], run_status=run["status"],
                    output_digests={r["artifact_id"]: r["content_digest"] for r in run["output_refs"]})
    require(case["retained"] == original, "digest_mismatch", case_id=case["case_id"], field="retained")
    require(sha(get(files, case["retained_plan"])) == case["expected_plan_sha256"],
            "digest_mismatch", case_id=case["case_id"], field="retained_plan")
    commands = json.loads(get(files, case["retained_argv_source"]))
    if isinstance(commands, list):
        commands = next(c for c in commands if c.get("output") == case["retained_run"])
    argv = commands["argv"]
    original_bindings = [argv[i + 1] for i, arg in enumerate(argv) if arg == "--input"]
    require(len(original_bindings) == len(case["run_bindings"]), "argv_binding_drift", case_id=case["case_id"])
    for actual, configured in zip(original_bindings, case["run_bindings"]):
        left, path = actual.split("=", 1)
        require(left == configured["node_id"] + ":" + configured["binding_id"]
                and path == configured["retained_path"]
                and pathlib.PurePosixPath(path).name == pathlib.PurePosixPath(configured["path"]).name,
                "argv_binding_drift", binding=actual)
        require(sha(get(files, configured["path"])) == configured["sha256"],
                "digest_mismatch", member=configured["path"])
    for name, expected in case["member_sha256"].items():
        require(sha(get(files, name)) == expected, "digest_mismatch", member=name)
    return run


def verify_preparation(case):
    before = case["before"]
    cache = {}
    for step in before["glue_steps"]:
        source = step["source"]
        if source["kind"] == "archive_member":
            key = json.dumps(source["archive"], sort_keys=True)
            if key not in cache:
                cache[key] = load_archive(dict(case_id=case["case_id"], proof_class="retained", archive=source["archive"]))
            require(sha(get(cache[key], source["member"])) == source["sha256"], "digest_mismatch", member=source["member"])
        elif source["kind"] == "loose_retained_script":
            path = ROOT / source["path"]
            require(path.is_file(), "missing_member", path=str(path))
            require(sha(path.read_bytes()) == source["sha256"], "digest_mismatch", path=str(path))
        elif source["kind"] == "retained_script_digest_observation":
            path = ROOT / source["record"]
            require(path.is_file(), "missing_member", path=str(path))
            data = path.read_bytes()
            require(sha(data) == source["record_sha256"], "digest_mismatch", path=str(path))
            require(pointer(json.loads(data), source["source_pointer"]) == source["digest"], "digest_mismatch", path=str(path))
            require(source["source_bytes_retained"] is False, "invariant_failed", field="source_bytes_retained")
        else:
            raise ReplayError("unknown_preparation_source", source=source)
    require(before["glue_invocations"] == len(before["glue_steps"])
            and before["pre_step_invocations"] == len(before["pre_steps"]), "burden_drift", case_id=case["case_id"])


def expected_digest(case, artifact, original):
    expected = original
    for change in case["golden_changes"]:
        if change.get("kind") == "invariant_change":
            continue
        if artifact not in change["artifact_ids"]:
            continue
        require(change.get("bead", "").startswith("bd-") and change.get("semantic_note")
                and change.get("date") and len(change.get("changed_paths", [])) <= 50,
                "invalid_golden_change", artifact_id=artifact)
        require(change["old"] == expected, "invalid_golden_change", artifact_id=artifact)
        expected = change["new"]
    return expected


def parity(case, artifact, original, actual, version="unknown"):
    expected = expected_digest(case, artifact, original)
    require(actual == expected, "digest_drift", case_id=case["case_id"],
            artifact_id=artifact, old=expected, new=actual, contract_version=version)


def paths_changed(old, new, prefix=""):
    if type(old) is not type(new):
        return [prefix or "/"]
    if isinstance(old, dict):
        result = []
        for key in sorted(old.keys() | new.keys()):
            path = prefix + "/" + key.replace("~", "~0").replace("/", "~1")
            result += [path] if key not in old or key not in new else paths_changed(old[key], new[key], path)
        return result
    if isinstance(old, list):
        if len(old) != len(new):
            return [prefix]
        return [p for i, (a, b) in enumerate(zip(old, new)) for p in paths_changed(a, b, prefix + "/" + str(i))]
    return [] if old == new else [prefix or "/"]


def subject_values(document):
    """Read subjects from geographic/claim contracts, excluding operational IDs."""
    subjects = set()

    def add(value):
        if isinstance(value, (str, int)) and not isinstance(value, bool) and str(value):
            subjects.add(str(value))

    def walk(value, key="", parent=""):
        if isinstance(value, dict):
            if value.get("level") in ("parcel", "building", "point"):
                add(value.get("id"))
            if key in ("parcels", "buildings", "candidates", "points"):
                add(value.get("id"))
            if key == "subject_bindings" and value.get("role") in ("target", "supplied-address"):
                add(value.get("value"))
            for child, item in value.items():
                walk(item, child, key)
        elif isinstance(value, list):
            for item in value:
                walk(item, key, parent)
        elif key in ("parcels", "buildings", "parcel_ids", "building_ids", "parcel_id", "building_id", "bbl", "bin", "point_id",
                     "property_name", "address", "street_address", "matched_address", "input_address", "site_address", "site"):
            add(value)
        elif key == "name" and parent in ("subject", "property"):
            add(value)

    walk(document)
    return subjects


def development_subjects(catalog):
    """Source-derived exclusion export, shared by later held-out gates."""
    subjects = set()
    cache = {}

    for case in catalog["cases"]:
        case = resolve_case(case, catalog)
        key = json.dumps(case["archive"], sort_keys=True)
        if key not in cache:
            cache[key] = load_archive(case)
        files = cache[key]
        for name in case["member_sha256"]:
            if name.endswith(".json"):
                subjects.update(subject_values(json.loads(get(files, name))))
    return subjects


def count_subject_facts(attributes):
    """Unknown slots are not assertions; explicit zero and false remain facts."""
    return sum(value is not None and (not isinstance(value, str) or bool(value.strip()))
               for value in attributes.values())


class Log:
    def __init__(self, out, case):
        self.out = out
        self.case = case
        self.executed = []
        out.mkdir(parents=True, exist_ok=True)
        self.file = (out / "run.log").open("a")

    def event(self, event, **details):
        if event in ("command", "USAGE"):
            commands = [details["argv"]] if event == "command" else details["argv"]
            relative = [[arg.replace(str(self.out.resolve()) + os.sep, "") for arg in argv] for argv in commands]
            details["argv_relative"] = relative[0] if event == "command" else relative
        if event == "USAGE":
            caller = set()
            bindings = []
            generated = set()
            path_flags = {"--question", "--capabilities", "--inventory", "--profile", "--budget", "--request", "--population", "--loops", "--runs", "--receipts"}
            for argv in self.executed:
                for index, arg in enumerate(argv[:-1]):
                    if arg not in path_flags and arg != "--input":
                        continue
                    value = argv[index + 1]
                    if arg == "--input":
                        binding, value = value.split("=", 1)
                        bindings.append(binding)
                    path = pathlib.Path(value)
                    if not path.is_absolute():
                        path = ROOT / path
                    paths = {p for p in path.rglob("*") if p.is_file()} if path.is_dir() else {path}
                    caller.update(paths)
                    if arg == "--capabilities":
                        generated.update(paths)
            burden = details.setdefault("burden", {})
            burden.update(version="canon_geo_replay_burden.v0", canon_invocations=len(self.executed),
                          caller_files=len(caller), caller_bytes=sum(p.stat().st_size for p in caller),
                          caller_files_machine_generated=len(generated), hand_written_bindings=len(bindings),
                          hand_written_node_ids=len({b.split(":", 1)[0] for b in bindings}),
                          glue_invocations=0, pre_step_invocations=0, acquisition_steps=0,
                          adapter_invocations=0, policy_files=0,
                          retained_preparation=getattr(self, "preparation", None))
        row = dict(event=event, ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                   case_id=self.case, proof_class="retained", **details)
        self.file.write(json.dumps(row, sort_keys=True) + "\n")
        self.file.flush()

    def command(self, label, binary, args, cwd, exit_code=0):
        started = time.monotonic()
        process = subprocess.run([str(binary)] + args, cwd=cwd, capture_output=True, timeout=120 if self.case_tier == "fast" else None)
        self.last_exit = process.returncode
        sys.stderr.buffer.write(process.stderr)
        stdout = self.out / (label + ".json")
        stderr = self.out / (label + ".stderr")
        stdout.write_bytes(process.stdout)
        stderr.write_bytes(process.stderr)
        argv = [str(binary)] + args
        self.executed.append(argv)
        self.event("command", argv=argv, cwd=str(cwd), exit=process.returncode,
                   elapsed_ms=round((time.monotonic() - started) * 1000),
                   stdout=str(stdout), stdout_sha256=sha(process.stdout), stderr=str(stderr),
                   stderr_tail=process.stderr.decode(errors="replace")[-4000:])
        require(exit_code is None or process.returncode == exit_code, "command_failed", argv=argv, exit=process.returncode)
        return process.stdout


def artifact_bytes(files, run, ref):
    suffix = "artifacts/cas/" + ref["content_digest"].split(":")[1] + ".bin"
    candidates = [v for k, v in files.items() if k.endswith(suffix)]
    if not candidates:
        # Some immutable archives predate CAS publication and retain the
        # receipt-owned workspace path alongside each original run.json.
        roots = [k.removesuffix("run.json") for k, v in files.items()
                 if k.endswith("/run.json") and json.loads(v) == run]
        outputs = [o for n in run["project_run_report"]["receipt"]["node_receipts"]
                   for o in n["outputs"] if n["node_id"] == ref["project_node_id"]
                   and o["output_id"] == ref["output_id"] and o["content_digest"] == ref["content_digest"]]
        candidates = [files[root+"workspace/"+o["path"]] for root in roots for o in outputs
                      if root+"workspace/"+o["path"] in files]
    require(bool(candidates), "missing_member", artifact_id=ref["artifact_id"])
    require(all(v == candidates[0] for v in candidates), "digest_mismatch", artifact_id=ref["artifact_id"])
    return candidates[0]


CURRENT_UPSTREAM = {"home_cells", "section", "materialize_evidence", "compile_evidence", "propagation", "solve", "explanation"}


def validate_current_answer_upstream(case, run):
    refs = {ref["artifact_id"]: ref for ref in run["output_refs"]}
    upstream = [ref for ref in refs.values() if ref["output_id"] in CURRENT_UPSTREAM]
    require(len(upstream) == 7 and len(refs) == len(run["output_refs"]), "invariant_failed", field="current_answer_upstream_set")
    for ref in upstream:
        require(ref["content_digest"] == case["retained"]["output_digests"].get(ref["artifact_id"]),
                "digest_drift", artifact_id=ref["artifact_id"], field="immutable_upstream")
    return refs


def validate_current_answer_capture(case, plan_bytes, run, propose=None):
    require(run.get("blockers", []) == [], "invariant_failed", field="current_answer_blockers")
    refs = validate_current_answer_upstream(case, run)
    require(len(refs) == 8 and sum(ref["output_id"] == "answer" for ref in refs.values()) == 1,
            "invariant_failed", field="current_answer_output_set")
    actual = dict(plan_sha256=sha(plan_bytes), run_semantic_hash=run["semantic_hash"],
                  run_status=run["status"], output_digests={key: ref["content_digest"] for key, ref in refs.items()})
    expected = case.get("expected_current_answer")
    if propose and expected is None:
        return dict(kind="current_answer_capture", case_id=case["case_id"], source_case_id=case["source_case_id"],
                    bead=propose, expected_current_answer=actual, semantic_note="REVIEW REQUIRED: initial current-answer capture; seven upstream bytes unchanged")
    require(expected is not None, "missing_expected_capture", case_id=case["case_id"])
    require(expected == actual, "digest_drift", case_id=case["case_id"], field="expected_current_answer", old=expected, new=actual)
    return None


def preflight_invariants(run):
    require(run["status"] == "WAITING_FOR_INPUT" and run["phase"] == "PREFLIGHTED"
            and run.get("project_run_report") is None and run.get("output_refs", []) == []
            and any(b["kind"] == "INVALID_INPUT" for b in run.get("blockers", [])),
            "invalid_preflight_transition")
    return [dict(kind="pointer_equals", pointer=path, value=pointer(run, path))
            for path in ["/status", "/phase", "/blockers"]]


def execution_invariants(case):
    checks = case["invariants"]
    for change in case["golden_changes"]:
        if change.get("kind") != "invariant_change" or change.get("target") != "execution_invariants":
            continue
        require(change["old"] == checks and change.get("bead", "").startswith("bd-")
                and change.get("semantic_note") and change.get("date"), "invalid_golden_change", target="execution_invariants")
        checks = change["new"]
    return checks


def expected_resume_outcome(case, original):
    expected = original
    for change in case["golden_changes"]:
        if change.get("kind") != "invariant_change" or change.get("target") != "resume_current_outcome":
            continue
        require(change.get("old") == expected and change.get("bead", "").startswith("bd-")
                and change.get("date") and change.get("semantic_note"), "invalid_golden_change", target="resume_current_outcome")
        expected = change["new"]
    return expected


def workspace_refusal_contract(envelope, work):
    refusal = envelope.get("refusal", {})
    detail = refusal.get("detail", {})
    fields = detail.get("detail", {})
    require(envelope.get("outcome") == "REFUSAL" and refusal.get("code") == "E_ENTITY_ARTIFACT_CONTRACT"
            and detail.get("geo_run_error_code") == "INVALID_INPUT"
            and fields.get("field") == "work_dir" and fields.get("path") == str(work)
            and "project_node_id" not in fields and not work.exists() and not work.parent.exists(),
            "invariant_failed", field="workspace_refusal_contract", observed=fields, requested=str(work))
    return dict(process_exit=2, outcome="REFUSAL", code=refusal["code"],
                geo_run_error_code=detail["geo_run_error_code"], field="work_dir", path_binding="requested_work_dir")


def failed_baseline_check(case, run, files):
    if case.get("source_case_id", case["case_id"]).removeprefix("resume.") != "nxrt.experiment":
        return None
    require(case.get("experiment_class") == "failed_baseline"
            and run["status"] not in ("RESOLVED", "COMPLETED")
            and not case.get("claims_resolution") and case.get("result_class") not in ("resolved", "success"),
            "failed_baseline_reported_as_success", case_id=case["case_id"])
    truth = json.loads(get(files, "experiment/truth-feasibility.json"))
    require(truth["status"] == "conflict" and truth["summary"]["residual_model_count"] == 0,
            "invariant_failed", field="retained_nxrt_truth_conflict")
    return dict(experiment_class="failed_baseline", retained_status=case["retained"]["run_status"],
                current_status=run["status"], retained_truth_status=truth["status"],
                retained_catalog_classification=case.get("experiment_class"),
                truth_probe_role="evaluation_only", identity_success=False)


def validate_duplicate(case, files, catalog):
    target = next(row for row in catalog["cases"] if row["case_id"] == case["duplicate_of"])
    original = retained(case, files); target_files = load_archive(target); target_run = retained(target, target_files)
    require(original["semantic_hash"] == target_run["semantic_hash"], "digest_drift", field="duplicate_semantic_hash")
    require(case["plan_inputs"].keys() == target["plan_inputs"].keys() and len(case["run_bindings"]) == len(target["run_bindings"]), "argv_binding_drift", field="duplicate_input_shape")
    for flag, name in case["plan_inputs"].items():
        require(get(files,name)==get(target_files,target["plan_inputs"][flag]), "digest_drift", field="duplicate_plan_input")
    for a,b in zip(case["run_bindings"],target["run_bindings"]):
        require(a["node_id"]==b["node_id"] and a["binding_id"]==b["binding_id"] and get(files,a["path"])==get(target_files,b["path"]), "digest_drift", field="duplicate_run_input")
    return original


def invariant(check, run, artifacts):
    kind = check["kind"]
    value = artifacts.get(check.get("artifact_id"), {})
    if kind == "run_status":
        return run["status"] == check["value"]
    if kind == "composition_status":
        return value["status"] == check["value"]
    if kind == "residual_saturated":
        return value["summary"]["residual_model_count_saturated"] is True
    if kind == "residual_count":
        return value["summary"]["residual_model_count"] == check["value"]
    if kind == "forced_members":
        return value["hard_forced"] == dict(parcels=check["parcels"], buildings=check["buildings"])
    if kind == "no_forced_member":
        return value["hard_forced"] == dict(parcels=[], buildings=[])
    if kind == "component_hard_feasible":
        return next(c for c in value["factorization"] if c["key"] == check["key"])["hard_feasible_assignments"] == check["count"]
    if kind in ("soft_top_tie", "soft_all_tied"):
        rows = value["soft_ranked"]
        minimum = min(row["cost"] for row in rows)
        tied = [row for row in rows if row["cost"] == minimum]
        if kind == "soft_all_tied":
            return len(tied) == len(rows) == check["count"]
        members = sorted(m for row in tied for m in row["model"]["buildings"] + row["model"]["parcels"])
        return minimum == check["cost"] and members == sorted(check["members"])
    if kind == "failed_node":
        return any(n["node_id"] == check["node_id"] and check["message_contains"] in json.dumps(n)
                   for n in run["project_run_report"]["node_reports"])
    if kind == "pointer_equals":
        actual = pointer(run if not check.get("artifact_id") else value, check["pointer"])
        if check.get("projection") == "cost_histogram":
            histogram = {}
            for row in actual:
                cost = str(row["cost"])
                histogram[cost] = histogram.get(cost, 0) + 1
            actual = histogram
        elif check.get("projection") == "lineage_components":
            selected = [set(row["contract"]["source_lineage_ids"]) for row in actual if row["observation_id"] in check["observation_ids"]]
            groups = []
            for group in selected:
                while any(group & other for other in groups):
                    overlapping = [other for other in groups if group & other]
                    groups = [other for other in groups if not group & other]
                    group = group.union(*overlapping)
                groups.append(group)
            actual = len(groups)
        elif check.get("projection") == "admission_summary":
            actual = [dict(observation_id=row["observation_id"], disposition=row["disposition"],
                           source_lineage_ids=row["contract"]["source_lineage_ids"],
                           admission_reason=row.get("admission_reason")) for row in actual]
        else:
            require(not check.get("projection"), "unknown_invariant", projection=check.get("projection"))
        return actual == check["value"]
    raise ReplayError("unknown_invariant", kind=kind)


def replay(case, binary, out, propose=None):
    case = resolve_case(case)
    if case["mode"] in ("retained_resume", "evaluation_solve", "cost_observation"):
        return replay_full_source(case, binary, out, propose)
    started = time.monotonic()
    files = load_archive(case)  # No process may run before evidence verification.
    old_run = retained(case, files)
    if case["mode"] == "current_answer":
        require(propose or case.get("expected_current_answer") is not None,
                "missing_expected_capture", case_id=case["case_id"])
        old_artifacts = {ref["artifact_id"]: json.loads(artifact_bytes(files, old_run, ref)) for ref in old_run["output_refs"]}
        for check in case["historical_invariants"]:
            require(invariant(check, old_run, old_artifacts), "invariant_failed", check=check, scope="immutable_historical_source")
    log = Log(out, case["case_id"])
    log.case_tier = case["tier"]
    log.preparation = case.get("before")
    baseline = failed_baseline_check(case, old_run, files) if old_run else None
    log.event("start", archive=case["archive"], mode=case["mode"], tier=case["tier"], experiment_class=case.get("experiment_class", "integration"))
    if baseline:
        log.event("failed_baseline", **baseline, current_execution=False)
    if case.get("duplicate_of"):
        validate_duplicate(case, files, json.loads(CATALOG.read_bytes()))
        log.event("duplicate_validation", duplicate_of=case["duplicate_of"], original_role=case["role"],
                  canonical_processes_executed=0, semantic_hash=old_run["semantic_hash"], current_recompute=False)
        log.event("USAGE", argv=[], burden={}, invariants=["retained inputs and semantic hash equal named duplicate source; not a recompute"])
        log.event("finish", status=old_run["status"], classification="retained_duplicate_validation_not_recompute")
        return
    if case["mode"] == "evidence_only":
        for check in case["evidence_invariants"]:
            value = pointer(json.loads(get(files, check["member"])), check["pointer"])
            require(value == check["value"], "invariant_failed", check=check)
            log.event("invariant", check=check, passed=True)
        log.event("finish", status="evidence_only_no_native_association_claim")
        return
    if case["mode"] == "measurement_replay":
        data = log.command("measurement", binary.with_name("canon_geo_measurements"), case["argv"], ROOT)
        require(sha(data) == case["retained_stdout_sha256"], "digest_drift", case_id=case["case_id"], artifact_id="measurement_stdout")
        result = json.loads(data)
        for check in case["evidence_invariants"]:
            passed = pointer(result, check["pointer"]) == check["value"]
            log.event("invariant", check=check, passed=passed)
            require(passed, "invariant_failed", check=check)
        log.event("USAGE", argv=log.executed, burden=dict(caller_files=len(case["member_sha256"]), hand_written_bindings=0), invariants=case["evidence_invariants"])
        log.event("finish", status="retained_recovery_measurement_no_precision_claim")
        return
    inputs = out / "inputs"
    inputs.mkdir()
    bindings = [] if case["mode"] == "leaf_replay" else case["run_bindings"]
    if case["mode"] == "current_answer":
        bindings = [binding for binding in bindings if binding["node_id"].rsplit(".", 1)[-1] in {"home_cells", "section", "materialize_evidence"}]
        require(len(bindings) == 3, "invariant_failed", field="current_answer_bindings")
    selected_members = set(case["plan_inputs"].values()) | {binding["path"] for binding in bindings} if case["mode"] == "current_answer" else case["member_sha256"]
    for name in selected_members:
        destination = inputs / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(get(files, name))
    if case["mode"] == "current_answer":
        question = inputs / case["plan_inputs"]["question"]
        value = json.loads(question.read_bytes())
        value["answer_scope"] = "current_answer"
        question.write_text(json.dumps(value, separators=(",", ":"))+"\n")
        capabilities = log.command("capabilities", binary, ["geo", "capabilities", "--emit", "json"], ROOT)
        (inputs / case["plan_inputs"]["capabilities"]).write_bytes(capabilities)
    if case["mode"] == "leaf_replay":
        data = log.command("leaf", binary, ["geo", "materialize-address-evidence", "--request", str(inputs / case["request"])], ROOT)
        parity(case, "stdout_sha256", case["retained_stdout_sha256"], sha(data), "canon_geo_address_bundle.v0")
        value = json.loads(data)
        for check in case["evidence_invariants"]:
            passed = pointer(value, check["pointer"]) == check["value"]
            log.event("invariant", check=check, passed=passed)
            require(passed, "invariant_failed", check=check)
        log.event("USAGE", argv=log.executed, burden=dict(caller_files=1, caller_bytes=len(get(files, case["request"])), hand_written_bindings=0), invariants=case["evidence_invariants"])
        log.event("finish", status="retained_address_evidence_not_accepted_identity")
        return
    plan_args = ["geo", "plan"]
    for flag, path in case["plan_inputs"].items():
        plan_args += ["--" + flag, str(inputs / path)]
    plan = log.command("plan", binary, plan_args, ROOT)
    if case["mode"] != "current_answer":
        parity(case, "plan_sha256", case["expected_plan_sha256"], sha(plan), "canon_geo_plan.v0")
    (out / "plan.json").write_bytes(plan)
    work = out / "missing-parent/work" if case.get("expected_refusal") else out / "work"
    if not case.get("expected_refusal"):
        work.mkdir()
    if case["tier"] == "resume":
        prefix = case["retained_workspace"]
        for name, data in files.items():
            if name.startswith(prefix):
                destination = work / name[len(prefix):]
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
    args = ["geo", "run", "--plan", str(out / "plan.json"), "--work-dir", str(work)]
    for binding in bindings:
        args += ["--input", binding["node_id"] + ":" + binding["binding_id"] + "=" + str(inputs / binding["path"])]
    if case["tier"] == "resume":
        # Historical solve facts remain tested as retained facts, even when current code
        # correctly refuses the old executor's noncanonical next-evidence bytes.
        old_artifacts = {ref["artifact_id"]: json.loads(artifact_bytes(files, old_run, ref)) for ref in old_run["output_refs"]}
        for check in case["invariants"]:
            passed = invariant(check, old_run, old_artifacts)
            log.event("retained_invariant", check=check, passed=passed, current_execution=False)
            require(passed, "invariant_failed", check=check)
        changes = [g for g in case["golden_changes"] if g.get("kind") == "invariant_change" and g.get("target") == "resume_current_outcome"]
        original = dict(process_exit=0, status=old_run["status"], semantic_hash=old_run["semantic_hash"])
        expected = expected_resume_outcome(case, original)
        if changes or propose:
            data = log.command("run", binary, args, ROOT, exit_code=None if propose else expected["process_exit"])
            envelope = json.loads(data)
            if envelope.get("outcome") == "REFUSAL":
                refusal = envelope["refusal"]
                actual = dict(process_exit=log.last_exit, outcome="REFUSAL", code=refusal["code"],
                              geo_run_error_code=refusal["detail"]["geo_run_error_code"],
                              node_id=refusal["detail"]["detail"]["node_id"],
                              message=refusal["detail"]["message"])
                require(actual["process_exit"] == 2 and actual["code"] == "E_ENTITY_ARTIFACT_CONTRACT" and actual["geo_run_error_code"] == "PROJECT_RUN_FAILED"
                        and actual["node_id"].endswith(".next_evidence") and "bytes are not canonical executor output" in actual["message"], "invariant_failed", outcome=actual)
                label = "current_resume_refusal"
                status = "REFUSAL"
            else:
                preflight_invariants(envelope)
                require(log.last_exit == 0, "invariant_failed", field="resume_preflight_process_exit")
                actual = dict(process_exit=log.last_exit, status=envelope["status"], phase=envelope["phase"],
                              semantic_hash=envelope["semantic_hash"], blockers=envelope["blockers"], current_cached_success=False)
                label = "current_resume_preflight"
                status = envelope["status"]
            if propose:
                proposal = dict(case_id=case["case_id"], kind="invariant_change", target="resume_current_outcome", bead=propose,
                                date=datetime.datetime.now(datetime.timezone.utc).date().isoformat(), commit=None,
                                old=expected, new=actual, contract_versions=[old_run["version"], envelope["version"]],
                                changed_paths=paths_changed(expected, actual), semantic_note="REVIEW REQUIRED: separately retained current resume outcome; preflight wait/refusal is not current cached success or a new answer")
                log.event("golden_proposal", proposal=proposal)
                print(json.dumps(proposal, sort_keys=True))
            else:
                require(expected == actual, "invariant_failed", expected=expected, observed=actual)
            log.event(label, **actual, previous_stored_status=old_run["status"], current_answer_produced=False)
            log.event("USAGE", argv=log.executed, burden=dict(caller_files=len(case["member_sha256"]), hand_written_bindings=len(case["run_bindings"])), invariants=["retained facts preserved", "strict current canonical-byte refusal or INVALID zero-work preflight", "no current cached success or answer produced"])
            log.event("finish", status=status, current_answer_produced=False, current_cached_success=False)
            return
    if case.get("expected_refusal"):
        data = log.command("run", binary, args, ROOT, exit_code=2)
        require(not work.exists() and not work.parent.exists(), "invariant_failed", check="workspace_refusal_creates_no_directory")
        change = next((entry for entry in reversed(case["golden_changes"]) if entry.get("kind") == "invariant_change" and entry.get("target") == "workspace_refusal_contract"), None)
        if propose or change:
            observed = workspace_refusal_contract(json.loads(data), work)
            if propose and not change:
                proposal = dict(case_id=case["case_id"], kind="invariant_change", target="workspace_refusal_contract", bead=propose,
                                date=datetime.datetime.now(datetime.timezone.utc).date().isoformat(), commit=None,
                                old=case["retained_stdout_sha256"], new=observed,
                                semantic_note="REVIEW REQUIRED: operational work_dir path must equal requested workspace, never a golden random path hash")
                log.event("golden_proposal", proposal=proposal)
                print(json.dumps(proposal, sort_keys=True))
            else:
                require(change["old"] == case["retained_stdout_sha256"] and change["new"] == observed
                        and change.get("bead", "").startswith("bd-") and change.get("date") and change.get("semantic_note"),
                        "invalid_golden_change", target="workspace_refusal_contract")
        else:
            require(sha(data) == case["retained_stdout_sha256"], "digest_drift", case_id=case["case_id"], artifact_id="refusal_envelope")
        log.event("USAGE", argv=log.executed, burden=dict(caller_files=len(case["member_sha256"]), hand_written_bindings=len(case["run_bindings"])), invariants=["REFUSAL exit2", "workspace absent", "retained envelope bytes equal or named reviewed operational-path contract"])
        log.event("finish", status="REFUSAL", code=json.loads(data)["refusal"]["code"])
        return
    run = json.loads(log.command("run", binary, args, ROOT))
    baseline = failed_baseline_check(case, run, files)
    if baseline:
        log.event("failed_baseline", **baseline, current_execution=True)
    log.command("inspect", binary, ["geo", "inspect", "--run", str(work)], ROOT)
    current = {}
    for ref in run.get("output_refs", []):
        path = work / ".canon/geo-run/artifacts/cas" / (ref["content_digest"].split(":")[1] + ".bin")
        require(path.is_file(), "missing_member", path=str(path))
        data = path.read_bytes()
        current[ref["artifact_id"]] = json.loads(data)
        if shutil.which("b3sum"):
            hashed = subprocess.run(["b3sum", str(path)], capture_output=True, check=True)
            sys.stderr.buffer.write(hashed.stderr)
            require("blake3:" + hashed.stdout.decode().split()[0] == ref["content_digest"], "digest_mismatch", path=str(path))
        else:
            log.event("b3sum_unavailable", artifact_id=ref["artifact_id"])
        log.event("artifact", **ref, path=str(path))
    checks = execution_invariants(case)
    if case["mode"] != "current_answer" and run["status"] == "WAITING_FOR_INPUT" and any(b["kind"] == "INVALID_INPUT" for b in run["blockers"]):
        observed_checks = preflight_invariants(run)
        if propose and checks != observed_checks:
            proposal = dict(case_id=case["case_id"], kind="invariant_change", target="execution_invariants",
                            bead=propose, date=datetime.datetime.now(datetime.timezone.utc).date().isoformat(),
                            commit=None, old=checks, new=observed_checks,
                            semantic_note="REVIEW REQUIRED: detectable input fault now blocks before execution; immutable historical outputs remain retained")
            log.event("golden_proposal", proposal=proposal)
            print(json.dumps(proposal, sort_keys=True))
            checks = observed_checks
        require(checks == observed_checks, "invariant_failed", field="preflight_transition_requires_named_review")
    for check in checks:
        passed = invariant(check, run, current)
        log.event("invariant", check=check, passed=passed)
        require(passed, "invariant_failed", case_id=case["case_id"], check=check)
    actual_digests = {r["artifact_id"]: r["content_digest"] for r in run.get("output_refs", [])}
    if case["mode"] == "current_answer":
        proposal = validate_current_answer_capture(case, plan, run, propose)
        if proposal:
            log.event("golden_proposal", proposal=proposal)
            print(json.dumps(proposal, sort_keys=True))
    comparisons = [] if case["mode"] == "current_answer" else [("run_semantic_hash", old_run["semantic_hash"], run["semantic_hash"], old_run, run),
                   ("run_status", old_run["status"], run["status"], old_run["status"], run["status"])]
    old_refs = {r["artifact_id"]: r for r in old_run["output_refs"]}
    for key in sorted(old_refs.keys() | actual_digests.keys()):
        if case["mode"] == "current_answer":
            continue
        old_ref = old_refs.get(key)
        old_data = json.loads(artifact_bytes(files, old_run, old_ref)) if old_ref and propose and old_ref["content_digest"] != actual_digests.get(key) else None
        comparisons.append((key, old_ref["content_digest"] if old_ref else None, actual_digests.get(key), old_data, current.get(key)))
    for key, old, new, old_json, new_json in comparisons:
        expected = expected_digest(case, key, old)
        if propose and expected != new:
            proposal = dict(case_id=case["case_id"], bead=propose, date=datetime.datetime.now(datetime.timezone.utc).date().isoformat(),
                                  commit=None, artifact_ids=[key], old=expected, new=new,
                                  contract_versions=[(new_json or {}).get("version", "canon_geo_run.v0") if isinstance(new_json, dict) else old_refs.get(key, {}).get("contract_version", "canon_geo_run.v0")],
                                  semantic_note="REVIEW REQUIRED", changed_paths=paths_changed(old_json, new_json)[:50])
            if expected != old:
                proposal["semantic_diff_base"] = "immutable_archive_json; old digest is prior approved golden; compare prior retained runtime before approval"
            log.event("golden_proposal", proposal=proposal)
            print(json.dumps(proposal, sort_keys=True))
        else:
            parity(case, key, old, new, current.get(key, {}).get("version", run["version"]))
    caller = {str(inputs / p) for p in case["plan_inputs"].values()} | {str(inputs / b["path"]) for b in bindings}
    burden = dict(version="canon_geo_replay_burden.v0", canon_invocations=len(log.executed),
                  caller_files=len(caller), caller_bytes=sum(pathlib.Path(p).stat().st_size for p in caller),
                  caller_files_machine_generated=1, hand_written_bindings=len(bindings),
                  hand_written_node_ids=len({b["node_id"] for b in bindings}))
    burden.update(hand_authored_constraints=0, subject_facts=0, hand_authored_partitions=0,
                  policy_files=0, adapter_invocations=0, acquisition_steps=0,
                  glue_invocations=0, pre_step_invocations=0,
                  retained_preparation=case.get("before"))
    for binding in bindings:
        value = json.loads(get(files, binding["path"]))
        if value.get("version") == "canon_geo_warehouse_rows.v0":
            burden["hand_authored_constraints"] += len(value.get("contracts", [])) + len(value.get("evidence_rows", []))
        elif value.get("version") == "canon_geo_descriptive_asset_request.v0":
            burden["hand_authored_constraints"] += len(value.get("profile", {}).get("channels", []))
            burden["subject_facts"] += count_subject_facts(value.get("claim", {}).get("attributes", {}))
        if "separation" in binding["node_id"]:
            prospective = value.get("prospective", [])
            burden["hand_authored_partitions"] += len(prospective) + sum(len(p.get("outcomes", [])) for p in prospective)
        if "next_evidence" in binding["node_id"]:
            burden["hand_authored_partitions"] += len(value.get("candidates", []))
    if case["mode"] == "current_answer":
        require(burden["caller_files"] == 8 and burden["hand_written_bindings"] == 3 and burden["hand_authored_partitions"] == 0, "burden_drift", actual=burden)
    else:
        require(burden["caller_bytes"] == case["agent_burden_baseline"]["bytes"], "burden_drift", actual=burden)
    log.event("burden", **burden)
    log.event("USAGE", argv=log.executed, burden=burden, invariants=checks)
    elapsed = time.monotonic() - started
    require(case["tier"] != "fast" or elapsed < 120, "runtime_tier_exceeded", elapsed=elapsed)
    load_archive(case)  # Reverify immutable source after all public commands.
    log.event("finish", elapsed_seconds=elapsed, status=run["status"], semantic_hash=run["semantic_hash"])
    print(json.dumps(dict(case_id=case["case_id"], elapsed_seconds=elapsed, out=str(out))), file=sys.stderr)


def timing_observation(case_id, elapsed_ms, retained_ms=None, classification="semantic_replay"):
    # This record feeds bd-1ssh / bd-259l. No timing comparison can refuse an
    # otherwise correct semantic replay.
    return dict(case_id=case_id, elapsed_ms=elapsed_ms, retained_elapsed_ms=retained_ms,
                ratio_to_retained=elapsed_ms/retained_ms if retained_ms else None,
                classification=classification, timing_affects_semantics=False)


def bounded_cost_command(log, binary, args, ceiling):
    require(ceiling > 0 and math.isfinite(ceiling), "invalid_cost_ceiling", ceiling=ceiling)
    argv = [str(binary)]+args
    started = time.monotonic()
    process = subprocess.Popen(argv, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    hit_ceiling = False
    try:
        stdout, stderr = process.communicate(timeout=ceiling)
    except subprocess.TimeoutExpired:
        hit_ceiling = True
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
    sys.stderr.buffer.write(stderr)
    (log.out/"cost-run.json").write_bytes(stdout)
    (log.out/"cost-run.stderr").write_bytes(stderr)
    log.executed.append(argv)
    elapsed_ms = round((time.monotonic()-started)*1000)
    log.event("command", argv=argv, cwd=str(ROOT), exit=process.returncode, elapsed_ms=elapsed_ms,
              ceiling_seconds=ceiling, stdout_sha256=sha(stdout), stderr_tail=stderr.decode(errors="replace")[-4000:])
    log.event("cost_observation_ceiling_reached" if hit_ceiling else "cost_observation_completed",
              ceiling_seconds=ceiling, exit=process.returncode, elapsed_ms=elapsed_ms,
              semantic_case=False, semantic_success_claimed=False)
    return elapsed_ms


def inspect_resume_twice(log, binary, work):
    first = log.command("inspect-1", binary, ["geo", "inspect", "--run", str(work)], ROOT, exit_code=None)
    second = log.command("inspect-2", binary, ["geo", "inspect", "--run", str(work)], ROOT, exit_code=None)
    require(first == second, "digest_drift", field="fresh_inspection_determinism")
    value = json.loads(first)
    if value.get("outcome") == "REFUSAL":
        require(value["refusal"]["code"] == "E_ENTITY_ARTIFACT_CONTRACT"
                and value["refusal"]["detail"].get("geo_inspect_error_code"), "invariant_failed", field="typed_resume_inspection")
        contract = dict(outcome="REFUSAL", code=value["refusal"]["code"], geo_inspect_error_code=value["refusal"]["detail"]["geo_inspect_error_code"])
    else:
        require(value["version"] == "canon_geo_inspection.v0", "invariant_failed", field="typed_resume_inspection")
        contract = dict(version=value["version"], run_id=value["run_id"])
    log.event("inspection_determinism", stdout_sha256=sha(first), equal_bytes=True, contract=contract,
              proof_scope="stored integrity/projection or explicit typed refusal; not recompute or current cached-success proof")
    return contract


def replay_full_source(case, binary, out, propose=None, ceiling=1200):
    started = time.monotonic()
    source = next(row for row in json.loads(CATALOG.read_bytes())["cases"] if row["case_id"] == case["source_case_id"])
    files = load_archive(source)
    old_run = retained(source, files)
    if case["mode"] == "retained_resume":
        require(propose or case.get("expected_resume") is not None, "missing_expected_capture", case_id=case["case_id"])
    log = Log(out, case["case_id"]); log.case_tier = case["tier"]; log.preparation = source.get("before")
    log.event("start", archive=source["archive"], mode=case["mode"], tier=case["tier"], source_case_id=source["case_id"])
    if case["mode"] == "evaluation_solve":
        require(case["role"] == "evaluation_only", "evaluation_input_leak")
        name = case["request"]
        request = out/"evaluation-only-request.json"; request.write_bytes(get(files, name))
        data = log.command("truth-probe", binary, ["geo", "solve", "--request", str(request)], ROOT)
        expected = get(files, case["retained_stdout"])
        require(sha(data) == sha(expected), "digest_drift", artifact_id="nxrt.truth_probe")
        value = json.loads(data)
        require(value["status"] == "conflict" and value["summary"]["residual_model_count"] == 0, "invariant_failed", field="truth_probe_conflict")
        hashed = subprocess.run(["b3sum", str(out/"truth-probe.json")], capture_output=True, check=True)
        sys.stderr.buffer.write(hashed.stderr)
        log.event("artifact", artifact_id=case["case_id"], path=str(out/"truth-probe.json"),
                  content_digest="blake3:"+hashed.stdout.decode().split()[0], byte_count=len(data),
                  contract_version=value["version"], role="evaluation_only")
        log.event("evaluation_only", request_sha256=sha(request.read_bytes()), stdout_sha256=sha(data), identity_success=False)
        log.event("USAGE", argv=log.executed, burden={}, invariants=["evaluation_only", "exact retained conflict bytes", "residual_count_zero"])
        log.event("finish", status="conflict", role="evaluation_only", elapsed_seconds=time.monotonic()-started)
        return
    inputs = out/"inputs"; inputs.mkdir()
    for name in source["member_sha256"]:
        destination = inputs/name; destination.parent.mkdir(parents=True, exist_ok=True); destination.write_bytes(get(files, name))
    plan_args = ["geo", "plan"]
    for flag, name in source["plan_inputs"].items():plan_args += ["--"+flag, str(inputs/name)]
    plan = log.command("plan", binary, plan_args, ROOT)
    require(sha(plan) == source["expected_plan_sha256"], "digest_drift", artifact_id="retained_source_plan")
    plan_path = out/"plan.json"; plan_path.write_bytes(plan)
    work = out/"work"; work.mkdir()
    bindings = source["run_bindings"]
    if case["mode"] == "retained_resume":
        prefix = source["retained_run"].removesuffix("run.json")+"run/"
        head = json.loads(get(files, prefix+".canon/geo-run/geo-run-manifest/head.json"))
        report = head["project_run_report"]
        require(len(report.get("resumed_nodes", [])) == 9 and not report.get("executed_nodes", [])
                and head["semantic_hash"] == old_run["semantic_hash"], "invariant_failed", field="historical_resume_nine")
        log.event("historical_resume", resumed_nodes=report["resumed_nodes"], executed_nodes=[],
                  semantic_hash=head["semantic_hash"], current_execution=False, proof_scope="retained integrity/projection, not recompute or upgraded-cache proof")
        for name, data in files.items():
            if name.startswith(prefix):
                destination = work/name[len(prefix):]; destination.parent.mkdir(parents=True, exist_ok=True); destination.write_bytes(data)
    else:
        require(case["mode"] == "cost_observation", "unknown_full_source_mode")
        prefix = source["retained_run"].removesuffix("run.json")+"numeric-mask-interrupted/"
        evidence = json.loads(get(files, prefix+"execution.json"))
        require(evidence["exit_code"] == -15, "invariant_failed", field="retained_numeric_interruption")
        numeric = inputs/"numeric-mask-separation.json"; numeric.write_bytes(get(files, prefix+"separation.json"))
        log.event("retained_cost", elapsed_seconds=evidence["elapsed_seconds"], exit=evidence["exit_code"],
                  execution_member=prefix+"execution.json", execution_sha256=sha(get(files, prefix+"execution.json")),
                  separation_sha256=sha(numeric.read_bytes()), total_inquiry_cost_claimed=False)
    args = ["geo", "run", "--plan", str(plan_path), "--work-dir", str(work)]
    for binding in bindings:
        path = numeric if case["mode"] == "cost_observation" and binding["node_id"].endswith(".separation") else inputs/binding["path"]
        args += ["--input", binding["node_id"]+":"+binding["binding_id"]+"="+str(path)]
    if case["mode"] == "cost_observation":
        bounded_cost_command(log, binary, args, ceiling)
        log.event("USAGE", argv=log.executed, burden={}, invariants=["bounded cost observation only; timeout is not a semantic pass"])
        log.event("finish", status="cost_observation_only", elapsed_seconds=time.monotonic()-started)
        return
    data = log.command("run", binary, args, ROOT, exit_code=None)
    value = json.loads(data)
    if value.get("outcome") == "REFUSAL":
        refusal = value["refusal"]; detail = refusal["detail"]
        observed = dict(outcome="REFUSAL", code=refusal["code"], geo_run_error_code=detail["geo_run_error_code"],
                        message=detail["message"], detail=detail["detail"], current_cached_success=False)
    else:
        observed = dict(status=value["status"], phase=value["phase"], semantic_hash=value["semantic_hash"],
                        resumed_nodes=(value.get("project_run_report") or {}).get("resumed_nodes", []),
                        executed_nodes=(value.get("project_run_report") or {}).get("executed_nodes", []),
                        output_digests={r["artifact_id"]:r["content_digest"] for r in value.get("output_refs", [])})
    observed["inspect_contract"] = inspect_resume_twice(log, binary, work)
    observed["inspect_deterministic"] = True
    if propose and case.get("expected_resume") is None:
        proposal = dict(kind="resume_capture", case_id=case["case_id"], bead=propose, expected_resume=observed,
                        semantic_note="REVIEW REQUIRED: actual current resume outcome; a refusal does not establish current cached success")
        log.event("golden_proposal", proposal=proposal); print(json.dumps(proposal, sort_keys=True))
    else:
        require(case["expected_resume"] == observed, "digest_drift", field="expected_resume", actual=observed)
    log.event("USAGE", argv=log.executed, burden={}, invariants=["historical nine-node resume preserved", "current outcome separately frozen", "two deterministic inspections"])
    log.event("finish", **observed, elapsed_seconds=time.monotonic()-started)


def full_suite(args, catalog, out):
    out.mkdir(parents=True, exist_ok=True)
    suite_log = Log(out, "retained_full_suite")
    suite_log.event("suite_start", tier=args.tier, build_profile=args.build_profile, host=platform.platform())
    merged_logs = set()
    def merge_log(case_id):
        path = out/case_id/"run.log"
        if path.is_file() and case_id not in merged_logs:
            with (out/"run.log").open("a") as destination:
                destination.write(path.read_text())
            merged_logs.add(case_id)
    selected = [row for row in catalog["cases"] if row["tier"] in (("fast", "full") if args.tier == "all" else (args.tier,))]
    full = args.tier in ("full", "all")
    semantic = [row for row in selected if not row.get("duplicate_of")]
    duplicates = [row for row in selected if row.get("duplicate_of")]
    resumes = [row for row in catalog["cases"] if row["tier"] == "resume"] if full else []
    costs = [row for row in catalog["cases"] if row["mode"] == "cost_observation"] if full else []
    rows = []; executed = 0; failed = 0; duplicate_count = 0; resume_count = 0; cost_count = 0
    try:
        for row in semantic+duplicates+resumes+costs:
            case = resolve_case(row, catalog); started = time.monotonic()
            files = load_archive(case)
            if row.get("duplicate_of"):
                replay(case, pathlib.Path(args.binary).resolve(), out/case["case_id"])
                duplicate_count += 1
                classification = "retained_duplicate_validation_not_recompute"
            else:
                replay_full_source(case, pathlib.Path(args.binary).resolve(), out/case["case_id"], ceiling=args.cost_ceiling_seconds) if case["mode"] in ("retained_resume", "evaluation_solve", "cost_observation") else replay(case, pathlib.Path(args.binary).resolve(), out/case["case_id"])
                if row in resumes and case["mode"] != "retained_resume":
                    extra = Log(out/case["case_id"], case["case_id"]); extra.case_tier = "resume"
                    extra.executed = [json.loads(line)["argv"] for line in (extra.out/"run.log").read_text().splitlines() if json.loads(line)["event"] == "command"]
                    inspect_resume_twice(extra, pathlib.Path(args.binary).resolve(), extra.out/"work")
                    extra.event("USAGE", argv=extra.executed, burden={}, invariants=["current resume outcome already checked", "two actual deterministic stored inspections; no upgraded-cache success claim"])
                classification = "cost_observation" if row in costs else "resume_integrity_not_recompute" if row in resumes else "semantic_replay"
                if row in costs:cost_count += 1
                elif row in resumes:resume_count += 1
                else:executed += 1
            original = resolve_case(row, catalog)
            source_execution = original.get("retained_argv_source")
            if case["mode"] == "cost_observation":
                source_execution = original["retained_run"].removesuffix("run.json")+"numeric-mask-interrupted/execution.json"
            captured = json.loads(get(files, source_execution)) if source_execution else {}
            retained_ms = captured.get("elapsed_seconds",0)*1000 if isinstance(captured,dict) else None
            observation = timing_observation(row["case_id"],round((time.monotonic()-started)*1000),retained_ms,classification)
            observation["retained_timing_scope"] = "retained_interrupted_attempt_only" if case["mode"] == "cost_observation" else "retained_completion_attempt_only"
            rows.append(observation)
            merge_log(case["case_id"])
    except Exception:
        failed += 1
        raise
    finally:
        if 'case' in locals():merge_log(case["case_id"])
        report = dict(version="canon_geo_retained_timings.v0", measured_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                      consumer_beads=["bd-1ssh","bd-259l"], proof_class="retained", build_profile=args.build_profile,
                      host=platform.platform(), semantic_cases_expected=len(semantic), semantic_cases_executed=executed,
                      duplicate_validations_expected=len(duplicates), duplicate_validations_executed=duplicate_count,
                      resume_cases_expected=len(resumes), resume_cases_executed=resume_count,
                      cost_observations_expected=len(costs), cost_observations_executed=cost_count,
                      invariant_failures=failed, rows=rows, elapsed_never_semantic=True)
        (out/"timings.json").write_text(json.dumps(report,sort_keys=True,indent=2)+"\n")
        suite_log.event("suite_finish", summary=report)
    require(executed==len(semantic) and duplicate_count==len(duplicates) and resume_count==len(resumes) and cost_count==len(costs), "suite_incomplete")
    print(json.dumps(report,sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case")
    parser.add_argument("--tier", default="fast")
    parser.add_argument("--binary", default=os.environ.get("CANON_BIN", str(ROOT / "target/debug/canon")))
    parser.add_argument("--out")
    parser.add_argument("--usage-only", action="store_true")
    parser.add_argument("--propose-golden-change")
    parser.add_argument("--validate-only", action="store_true")
    parser.add_argument("--suite", action="store_true")
    parser.add_argument("--cost-ceiling-seconds", type=float, default=1200)
    parser.add_argument("--build-profile", default="provided_binary")
    args = parser.parse_args()
    catalog = json.loads(CATALOG.read_bytes())
    out = pathlib.Path(args.out or tempfile.mkdtemp(prefix="canon-retained-replay-")).resolve()
    if args.suite:
        require(args.cost_ceiling_seconds > 0 and math.isfinite(args.cost_ceiling_seconds), "invalid_cost_ceiling")
        full_suite(args, catalog, out)
        return
    if args.usage_only:
        for log in out.glob("*/run.log"):
            for line in log.read_text().splitlines():
                row = json.loads(line)
                if row["event"] == "USAGE":
                    print(json.dumps(row))
        return
    selected = [c for c in catalog["cases"] if c["case_id"] == args.case] if args.case else [c for c in catalog["cases"] if c["tier"] == args.tier]
    require(bool(selected), "missing_member", case_id=args.case, tier=args.tier)
    for case in selected:
        if args.validate_only:
            files = load_archive(case)
            retained(case, files)
        else:
            replay(case, pathlib.Path(args.binary).resolve(), out / case["case_id"], args.propose_golden_change)
    print(json.dumps(dict(event="summary", executed=0 if args.validate_only else len(selected), expected=len(selected), out=str(out))), file=sys.stderr)


if __name__ == "__main__":
    try:
        main()
    except (ReplayError, OSError, KeyError, ValueError, StopIteration, subprocess.TimeoutExpired) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
