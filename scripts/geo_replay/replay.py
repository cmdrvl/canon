#!/usr/bin/env python3
"""Offline public-command replay. Retained integration is never live or accuracy proof."""
import argparse
import datetime
import gzip
import hashlib
import json
import os
import pathlib
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


def load_archive(case):
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
        process = subprocess.run([str(binary)] + args, cwd=cwd, capture_output=True, timeout=120 if self.case_tier == "fast" else 3600)
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
    require(bool(candidates), "missing_member", artifact_id=ref["artifact_id"])
    require(all(v == candidates[0] for v in candidates), "digest_mismatch", artifact_id=ref["artifact_id"])
    return candidates[0]


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
    started = time.monotonic()
    files = load_archive(case)  # No process may run before evidence verification.
    old_run = retained(case, files)
    log = Log(out, case["case_id"])
    log.case_tier = case["tier"]
    log.preparation = case.get("before")
    log.event("start", archive=case["archive"], mode=case["mode"], tier=case["tier"])
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
    for name in case["member_sha256"]:
        destination = inputs / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(get(files, name))
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
    for binding in case["run_bindings"]:
        args += ["--input", binding["node_id"] + ":" + binding["binding_id"] + "=" + str(inputs / binding["path"])]
    if case["tier"] == "resume":
        # Historical solve facts remain tested as retained facts, even when current code
        # correctly refuses the old executor's noncanonical next-evidence bytes.
        old_artifacts = {ref["artifact_id"]: json.loads(artifact_bytes(files, old_run, ref)) for ref in old_run["output_refs"]}
        for check in case["invariants"]:
            passed = invariant(check, old_run, old_artifacts)
            log.event("retained_invariant", check=check, passed=passed, current_execution=False)
            require(passed, "invariant_failed", check=check)
        change = next((g for g in reversed(case["golden_changes"]) if g.get("kind") == "invariant_change" and g.get("target") == "resume_current_outcome"), None)
        if change or propose:
            data = log.command("run", binary, args, ROOT, exit_code=change["new"]["process_exit"] if change else None)
            envelope = json.loads(data)
            if envelope.get("outcome") == "REFUSAL":
                refusal = envelope["refusal"]
                actual = dict(process_exit=2, outcome="REFUSAL", code=refusal["code"],
                              geo_run_error_code=refusal["detail"]["geo_run_error_code"],
                              node_id=refusal["detail"]["detail"]["node_id"],
                              message=refusal["detail"]["message"])
                original = dict(process_exit=0, status=old_run["status"], semantic_hash=old_run["semantic_hash"])
                require(actual["code"] == "E_ENTITY_ARTIFACT_CONTRACT" and actual["geo_run_error_code"] == "PROJECT_RUN_FAILED"
                        and actual["node_id"].endswith(".next_evidence") and "bytes are not canonical executor output" in actual["message"], "invariant_failed", outcome=actual)
                if propose:
                    proposal = dict(kind="invariant_change", target="resume_current_outcome", bead=propose,
                                    date=datetime.datetime.now(datetime.timezone.utc).date().isoformat(), commit=None,
                                    old=original, new=actual, contract_versions=[old_run["version"], envelope["version"]],
                                    changed_paths=paths_changed(original, actual), semantic_note="REVIEW REQUIRED: legacy deserialization does not preserve canonical executor bytes; no current answer was produced")
                    log.event("golden_proposal", proposal=proposal)
                    print(json.dumps(proposal, sort_keys=True))
                else:
                    require(change["old"] == original and change["new"] == actual and change["bead"].startswith("bd-") and change["semantic_note"], "invariant_failed", change=change, observed=actual)
                log.event("current_resume_refusal", **actual, previous_stored_status=old_run["status"], current_answer_produced=False)
                log.event("USAGE", argv=log.executed, burden=dict(caller_files=len(case["member_sha256"]), hand_written_bindings=len(case["run_bindings"])), invariants=["retained facts preserved", "current typed canonical-byte refusal", "no current answer produced"])
                log.event("finish", status="REFUSAL", current_answer_produced=False)
                return
            require(not change, "invariant_failed", check="expected approved resume refusal", actual=envelope)
            raise ReplayError("unexpected_resume_proposal", case_id=case["case_id"], outcome=envelope)
    if case.get("expected_refusal"):
        data = log.command("run", binary, args, ROOT, exit_code=2)
        require(not work.exists() and not work.parent.exists(), "invariant_failed", check="workspace_refusal_creates_no_directory")
        require(sha(data) == case["retained_stdout_sha256"], "digest_drift", case_id=case["case_id"], artifact_id="refusal_envelope")
        log.event("USAGE", argv=log.executed, burden=dict(caller_files=len(case["member_sha256"]), hand_written_bindings=len(case["run_bindings"])), invariants=["REFUSAL exit2", "workspace absent", "retained envelope bytes equal"])
        log.event("finish", status="REFUSAL", code=json.loads(data)["refusal"]["code"])
        return
    run = json.loads(log.command("run", binary, args, ROOT))
    log.command("inspect", binary, ["geo", "inspect", "--run", str(work)], ROOT)
    current = {}
    for ref in run["output_refs"]:
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
    for check in case["invariants"]:
        passed = invariant(check, run, current)
        log.event("invariant", check=check, passed=passed)
        require(passed, "invariant_failed", case_id=case["case_id"], check=check)
    actual_digests = {r["artifact_id"]: r["content_digest"] for r in run["output_refs"]}
    comparisons = [("run_semantic_hash", old_run["semantic_hash"], run["semantic_hash"], old_run, run),
                   ("run_status", old_run["status"], run["status"], old_run["status"], run["status"])]
    old_refs = {r["artifact_id"]: r for r in old_run["output_refs"]}
    for key in sorted(old_refs.keys() | actual_digests.keys()):
        old_ref = old_refs.get(key)
        old_data = json.loads(artifact_bytes(files, old_run, old_ref)) if old_ref and propose and old_ref["content_digest"] != actual_digests.get(key) else None
        comparisons.append((key, old_ref["content_digest"] if old_ref else None, actual_digests.get(key), old_data, current.get(key)))
    for key, old, new, old_json, new_json in comparisons:
        if propose and old != new:
            proposal = dict(case_id=case["case_id"], bead=propose, date=datetime.datetime.now(datetime.timezone.utc).date().isoformat(),
                                  commit=None, artifact_ids=[key], old=old, new=new,
                                  contract_versions=[(new_json or {}).get("version", "canon_geo_run.v0") if isinstance(new_json, dict) else old_refs.get(key, {}).get("contract_version", "canon_geo_run.v0")],
                                  semantic_note="REVIEW REQUIRED", changed_paths=paths_changed(old_json, new_json)[:50])
            log.event("golden_proposal", proposal=proposal)
            print(json.dumps(proposal, sort_keys=True))
        else:
            parity(case, key, old, new, current.get(key, {}).get("version", run["version"]))
    caller = {str(inputs / p) for p in case["plan_inputs"].values()} | {str(inputs / b["path"]) for b in case["run_bindings"]}
    burden = dict(version="canon_geo_replay_burden.v0", canon_invocations=len(log.executed),
                  caller_files=len(caller), caller_bytes=sum(pathlib.Path(p).stat().st_size for p in caller),
                  caller_files_machine_generated=1, hand_written_bindings=len(case["run_bindings"]),
                  hand_written_node_ids=len({b["node_id"] for b in case["run_bindings"]}))
    burden.update(hand_authored_constraints=0, subject_facts=0, hand_authored_partitions=0,
                  policy_files=0, adapter_invocations=0, acquisition_steps=0,
                  glue_invocations=0, pre_step_invocations=0,
                  retained_preparation=case.get("before"))
    for binding in case["run_bindings"]:
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
    require(burden["caller_bytes"] == case["agent_burden_baseline"]["bytes"], "burden_drift", actual=burden)
    log.event("burden", **burden)
    log.event("USAGE", argv=log.executed, burden=burden, invariants=case["invariants"])
    elapsed = time.monotonic() - started
    require(case["tier"] != "fast" or elapsed < 120, "runtime_tier_exceeded", elapsed=elapsed)
    load_archive(case)  # Reverify immutable source after all public commands.
    log.event("finish", elapsed_seconds=elapsed, status=run["status"], semantic_hash=run["semantic_hash"])
    print(json.dumps(dict(case_id=case["case_id"], elapsed_seconds=elapsed, out=str(out))), file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case")
    parser.add_argument("--tier", default="fast")
    parser.add_argument("--binary", default=os.environ.get("CANON_BIN", str(ROOT / "target/debug/canon")))
    parser.add_argument("--out")
    parser.add_argument("--usage-only", action="store_true")
    parser.add_argument("--propose-golden-change")
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    catalog = json.loads(CATALOG.read_bytes())
    out = pathlib.Path(args.out or tempfile.mkdtemp(prefix="canon-retained-replay-")).resolve()
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
