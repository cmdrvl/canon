use crate::Refusal;
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const SANDBOX_EXECUTABLE: &str = "/usr/bin/sandbox-exec";
const LIMIT_PROBE_PYTHON_SOURCE: &str = r#"
import resource
import subprocess
import sys

def set_soft_limit(name, desired_soft):
    value = getattr(resource, name, None)
    if value is None:
        return
    current_soft, current_hard = resource.getrlimit(value)
    next_soft = desired_soft if current_hard == resource.RLIM_INFINITY else min(desired_soft, current_hard)
    resource.setrlimit(value, (next_soft, current_hard))

set_soft_limit("RLIMIT_CPU", 1)
set_soft_limit("RLIMIT_NOFILE", 64)
set_soft_limit("RLIMIT_FSIZE", 1_048_576)
subprocess.check_output(["ps", "-o", "pgid=,pid=,rss=", "-ax"], text=True)
print(sys.executable)
"#;
const HELPER_PYTHON_SOURCE: &str = r#"
import json
import os
import resource
import selectors
import signal
import subprocess
import sys
import time

profile_path = sys.argv[1]
runtime_exec = sys.argv[2]
script_path = sys.argv[3]
input_path = sys.argv[4]
stdout_path = sys.argv[5]
stderr_path = sys.argv[6]
scratch_dir = sys.argv[7]
wall_timeout_ms = int(sys.argv[8])
output_limit_bytes = int(sys.argv[9])
cpu_limit_secs = int(sys.argv[10])
memory_limit_bytes = int(sys.argv[11])
process_limit = int(sys.argv[12])
file_limit = int(sys.argv[13])
file_size_limit_bytes = int(sys.argv[14])

def _set_soft_limit(name, desired_soft):
    value = getattr(resource, name, None)
    if value is None:
        return
    current_soft, current_hard = resource.getrlimit(value)
    if current_hard == resource.RLIM_INFINITY:
        next_soft = desired_soft
    else:
        next_soft = min(desired_soft, current_hard)
    resource.setrlimit(value, (next_soft, current_hard))

def _preexec():
    _set_soft_limit("RLIMIT_CPU", cpu_limit_secs)
    _set_soft_limit("RLIMIT_NOFILE", file_limit)
    _set_soft_limit("RLIMIT_FSIZE", file_size_limit_bytes)

def _kill_group(proc):
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass

def _lower_preview(path):
    try:
        with open(path, "rb") as fh:
            return fh.read(512).decode("utf-8", "replace").lower()
    except OSError:
        return ""

def _group_stats(pgid):
    try:
        output = subprocess.check_output(["ps", "-o", "pgid=,pid=,rss=", "-ax"], text=True)
    except Exception:
        return None

    process_count = 0
    total_kib = 0
    for line in output.splitlines():
        parts = line.split()
        if len(parts) != 3:
            continue
        try:
            row_pgid = int(parts[0])
            row_rss = int(parts[2])
        except ValueError:
            continue
        if row_pgid == pgid:
            process_count += 1
            total_kib += row_rss
    return process_count, total_kib * 1024

result = {
    "category": "runner_failure",
    "reason": "runner did not complete",
    "exit_code": None,
    "signal": None,
}

cmd = ["/usr/bin/sandbox-exec", "-f", profile_path, runtime_exec, script_path]
env = {
    "PATH": "/bin:/usr/bin",
    "LC_ALL": "C",
    "LANG": "C",
    "TMPDIR": scratch_dir,
}

try:
    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=scratch_dir,
        env=env,
        start_new_session=True,
        preexec_fn=_preexec,
    )
except Exception as exc:
    result["reason"] = f"failed to launch sandboxed runtime: {exc}"
    print(json.dumps(result))
    sys.exit(0)

with open(input_path, "rb") as input_fh:
    input_bytes = input_fh.read()

try:
    if proc.stdin is not None:
        proc.stdin.write(input_bytes)
        proc.stdin.close()
except BrokenPipeError:
    pass

selector = selectors.DefaultSelector()
streams = {}
for name, pipe in (("stdout", proc.stdout), ("stderr", proc.stderr)):
    if pipe is None:
        continue
    os.set_blocking(pipe.fileno(), False)
    selector.register(pipe, selectors.EVENT_READ, data=name)
    streams[name] = pipe

output_bytes = 0
timed_out = False
output_limited = False
process_limited = False
memory_limited = False
group_probe_failed = False
deadline = time.monotonic() + (wall_timeout_ms / 1000.0)

with open(stdout_path, "wb") as stdout_fh, open(stderr_path, "wb") as stderr_fh:
    writers = {"stdout": stdout_fh, "stderr": stderr_fh}
    while True:
        if not selector.get_map() and proc.poll() is not None:
            break

        remaining = deadline - time.monotonic()
        if remaining <= 0:
            timed_out = True
            _kill_group(proc)
            break

        group_stats = _group_stats(proc.pid)
        if group_stats is None:
            group_probe_failed = True
            _kill_group(proc)
            break
        process_count, rss_bytes = group_stats
        if process_count > process_limit:
            process_limited = True
            _kill_group(proc)
            break
        if rss_bytes > memory_limit_bytes:
            memory_limited = True
            _kill_group(proc)
            break

        events = selector.select(min(remaining, 0.05))
        if not events and proc.poll() is not None and not selector.get_map():
            break

        for key, _ in events:
            name = key.data
            chunk = os.read(key.fileobj.fileno(), 8192)
            if not chunk:
                selector.unregister(key.fileobj)
                key.fileobj.close()
                continue

            if output_bytes < output_limit_bytes:
                allowed = min(len(chunk), output_limit_bytes - output_bytes)
                writers[name].write(chunk[:allowed])
            output_bytes += len(chunk)

            if output_bytes > output_limit_bytes and not output_limited:
                output_limited = True
                _kill_group(proc)
                break

        if output_limited:
            break

for pipe in streams.values():
    try:
        pipe.close()
    except OSError:
        pass

try:
    proc.wait(timeout=1.0)
except Exception:
    _kill_group(proc)
    proc.wait()

preview = _lower_preview(stderr_path)
returncode = proc.returncode
signal_code = -returncode if returncode is not None and returncode < 0 else None

if timed_out:
    result["category"] = "timeout"
    result["reason"] = f"wall_timeout_ms_exceeded:{wall_timeout_ms}"
elif output_limited:
    result["category"] = "resource_limit"
    result["reason"] = f"output_limit_bytes_exceeded:{output_limit_bytes}"
elif group_probe_failed:
    result["category"] = "runner_failure"
    result["reason"] = "failed_to_measure_process_group_state"
elif process_limited:
    result["category"] = "resource_limit"
    result["reason"] = f"process_limit_exceeded:{process_limit}"
elif memory_limited:
    result["category"] = "resource_limit"
    result["reason"] = f"memory_limit_bytes_exceeded:{memory_limit_bytes}"
elif "resource temporarily unavailable" in preview or "file size limit exceeded" in preview or "cannot allocate memory" in preview:
    result["category"] = "resource_limit"
    result["reason"] = "sandboxed_runtime_hit_resource_limit"
elif "operation not permitted" in preview or "permission denied" in preview:
    result["category"] = "policy_denied"
    result["reason"] = "sandbox_policy_denied"
elif signal_code is not None:
    signal_limited = (
        getattr(signal, "SIGXCPU", 24),
        getattr(signal, "SIGXFSZ", 25),
        signal.SIGKILL,
    )
    result["category"] = "resource_limit" if signal_code in signal_limited else "runner_failure"
    result["reason"] = f"terminated_by_signal:{signal_code}"
else:
    result["category"] = "completed"
    result["reason"] = "completed"

result["exit_code"] = None if returncode is None or returncode < 0 else returncode
result["signal"] = signal_code
print(json.dumps(result))
"#;

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunnerContract {
    pub(crate) contract_id: String,
    pub(crate) platform: String,
    pub(crate) runtime_allowlist: Vec<String>,
    pub(crate) wall_timeout_ms: u64,
    pub(crate) cpu_timeout_secs: u64,
    pub(crate) memory_limit_bytes: u64,
    pub(crate) output_limit_bytes: u64,
    pub(crate) process_limit: u64,
    pub(crate) file_limit: u64,
    pub(crate) file_size_limit_bytes: u64,
    pub(crate) env_policy: String,
    pub(crate) filesystem_policy: String,
    pub(crate) scratch_policy: String,
    pub(crate) stdin_policy: String,
    pub(crate) network_policy: String,
    pub(crate) supervisor: String,
}

impl RunnerContract {
    fn macos() -> Self {
        Self {
            contract_id: "canon_strategy_runner.darwin.v1".to_string(),
            platform: "darwin-sandbox-exec".to_string(),
            runtime_allowlist: vec!["/bin/sh".to_string(), "/bin/bash".to_string()],
            wall_timeout_ms: 2_000,
            cpu_timeout_secs: 1,
            memory_limit_bytes: 128 * 1024 * 1024,
            output_limit_bytes: 65_536,
            process_limit: 32,
            file_limit: 64,
            file_size_limit_bytes: 1_048_576,
            env_policy: "inherit_nothing; set PATH=/bin:/usr/bin, LANG=C, LC_ALL=C, TMPDIR=<scratch>".to_string(),
            filesystem_policy: "deny all writes outside owned scratch; package, suite, registry, and project paths are read-only".to_string(),
            scratch_policy: "fresh owned scratch directory per run; removed after capture".to_string(),
            stdin_policy: "fixture bytes only via closed-after-write pipe".to_string(),
            network_policy: "deny_all".to_string(),
            supervisor: "python3 + sandbox-exec".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerTerminationCategory {
    Completed,
    Timeout,
    ResourceLimit,
    PolicyDenied,
    RunnerFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunnerTermination {
    pub(crate) category: RunnerTerminationCategory,
    pub(crate) reason: String,
    pub(crate) runtime: String,
    pub(crate) command: Vec<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) signal: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunnerOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) termination: RunnerTermination,
}

#[derive(Debug, Clone)]
struct RuntimeInvocation {
    label: String,
    executable: String,
    command: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct HelperReport {
    category: HelperCategory,
    reason: String,
    exit_code: Option<i32>,
    signal: Option<i32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HelperCategory {
    Completed,
    Timeout,
    ResourceLimit,
    PolicyDenied,
    RunnerFailure,
}

struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    fn new() -> Result<Self, Refusal> {
        let base = std::env::temp_dir().join("canon-strategy-runner");
        fs::create_dir_all(&base)
            .map_err(|error| Refusal::io_error(&base.display().to_string(), &error.to_string()))?;

        for _ in 0..32 {
            let suffix = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let candidate = base.join(format!("run-{}-{}-{}", std::process::id(), nanos, suffix));
            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self { path: candidate }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(Refusal::io_error(
                        &candidate.display().to_string(),
                        &error.to_string(),
                    ));
                }
            }
        }

        Err(Refusal::strategy_input_contract(
            "Strategy audit runner could not allocate a fresh scratch directory",
            json!({
                "base": base.display().to_string(),
            }),
        ))
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn contract() -> Result<RunnerContract, Refusal> {
    if !cfg!(target_os = "macos") {
        return Err(unsupported_platform_refusal(
            "unsupported target_os",
            json!({
                "target_os": std::env::consts::OS,
                "required_platform": "macos",
            }),
        ));
    }

    if !Path::new(SANDBOX_EXECUTABLE).is_file() {
        return Err(unsupported_platform_refusal(
            "sandbox-exec is unavailable",
            json!({
                "sandbox_exec": SANDBOX_EXECUTABLE,
            }),
        ));
    }

    let python_probe = Command::new("python3")
        .arg("-c")
        .arg(LIMIT_PROBE_PYTHON_SOURCE)
        .output()
        .map_err(|error| {
            unsupported_platform_refusal(
                "python3 supervisor is unavailable",
                json!({
                    "error": error.to_string(),
                }),
            )
        })?;

    if !python_probe.status.success() {
        return Err(unsupported_platform_refusal(
            "python3 supervisor probe failed",
            json!({
                "status": python_probe.status.code(),
                "stderr": String::from_utf8_lossy(&python_probe.stderr),
            }),
        ));
    }

    Ok(RunnerContract::macos())
}

pub(crate) fn run(
    script_path: &Path,
    input: &[u8],
    contract: &RunnerContract,
) -> Result<RunnerOutput, Refusal> {
    let script_path = script_path.canonicalize().map_err(|error| {
        Refusal::io_error(&script_path.display().to_string(), &error.to_string())
    })?;
    let runtime = resolve_runtime(&script_path, contract)?;
    let scratch = ScratchDir::new()?;

    let input_path = scratch.path.join("fixture.stdin");
    let stdout_path = scratch.path.join("stdout.bin");
    let stderr_path = scratch.path.join("stderr.bin");
    let profile_path = scratch.path.join("sandbox.sb");

    fs::write(&input_path, input).map_err(|error| {
        Refusal::io_error(&input_path.display().to_string(), &error.to_string())
    })?;
    fs::write(&stdout_path, []).map_err(|error| {
        Refusal::io_error(&stdout_path.display().to_string(), &error.to_string())
    })?;
    fs::write(&stderr_path, []).map_err(|error| {
        Refusal::io_error(&stderr_path.display().to_string(), &error.to_string())
    })?;
    fs::write(&profile_path, sandbox_profile(&scratch.path)).map_err(|error| {
        Refusal::io_error(&profile_path.display().to_string(), &error.to_string())
    })?;

    let helper = Command::new("python3")
        .arg("-c")
        .arg(HELPER_PYTHON_SOURCE)
        .arg(&profile_path)
        .arg(&runtime.executable)
        .arg(&script_path)
        .arg(&input_path)
        .arg(&stdout_path)
        .arg(&stderr_path)
        .arg(&scratch.path)
        .arg(contract.wall_timeout_ms.to_string())
        .arg(contract.output_limit_bytes.to_string())
        .arg(contract.cpu_timeout_secs.to_string())
        .arg(contract.memory_limit_bytes.to_string())
        .arg(contract.process_limit.to_string())
        .arg(contract.file_limit.to_string())
        .arg(contract.file_size_limit_bytes.to_string())
        .output();

    let helper = match helper {
        Ok(output) => output,
        Err(error) => {
            return Ok(runner_failure_output(
                &runtime,
                format!("failed to launch python3 runner supervisor: {error}"),
            ));
        }
    };

    let stdout = fs::read(&stdout_path).unwrap_or_default();
    let stderr = fs::read(&stderr_path).unwrap_or_default();

    let report = serde_json::from_slice::<HelperReport>(&helper.stdout).map_err(|error| {
        Refusal::strategy_input_contract(
            "Strategy audit runner emitted malformed supervisor metadata",
            json!({
                "script": script_path.display().to_string(),
                "error": error.to_string(),
                "stdout": String::from_utf8_lossy(&helper.stdout),
                "stderr": String::from_utf8_lossy(&helper.stderr),
            }),
        )
    })?;

    let termination = RunnerTermination {
        category: map_helper_category(report.category),
        reason: report.reason,
        runtime: runtime.label,
        command: runtime.command,
        exit_code: report.exit_code,
        signal: report.signal,
    };
    let exit_code = report
        .exit_code
        .unwrap_or_else(|| report.signal.map_or(125, |signal| 128 + signal));

    Ok(RunnerOutput {
        exit_code,
        stdout,
        stderr,
        termination,
    })
}

fn map_helper_category(category: HelperCategory) -> RunnerTerminationCategory {
    match category {
        HelperCategory::Completed => RunnerTerminationCategory::Completed,
        HelperCategory::Timeout => RunnerTerminationCategory::Timeout,
        HelperCategory::ResourceLimit => RunnerTerminationCategory::ResourceLimit,
        HelperCategory::PolicyDenied => RunnerTerminationCategory::PolicyDenied,
        HelperCategory::RunnerFailure => RunnerTerminationCategory::RunnerFailure,
    }
}

fn runner_failure_output(runtime: &RuntimeInvocation, reason: String) -> RunnerOutput {
    let termination = RunnerTermination {
        category: RunnerTerminationCategory::RunnerFailure,
        reason,
        runtime: runtime.label.clone(),
        command: runtime.command.clone(),
        exit_code: None,
        signal: None,
    };
    RunnerOutput {
        exit_code: 125,
        stdout: Vec::new(),
        stderr: Vec::new(),
        termination,
    }
}

fn resolve_runtime(
    script_path: &Path,
    contract: &RunnerContract,
) -> Result<RuntimeInvocation, Refusal> {
    let bytes = fs::read(script_path).map_err(|error| {
        Refusal::io_error(&script_path.display().to_string(), &error.to_string())
    })?;
    let first_line = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .map(|line| {
            String::from_utf8_lossy(line)
                .trim_end_matches('\r')
                .to_string()
        })
        .unwrap_or_default();

    let executable = match first_line.as_str() {
        "#!/bin/sh" => Some("/bin/sh"),
        "#!/usr/bin/env sh" => Some("/bin/sh"),
        "#!/bin/bash" => Some("/bin/bash"),
        "#!/usr/bin/env bash" => Some("/bin/bash"),
        _ => None,
    };

    if let Some(executable) = executable {
        return Ok(RuntimeInvocation {
            label: executable.to_string(),
            executable: executable.to_string(),
            command: vec![executable.to_string(), script_path.display().to_string()],
        });
    }

    Err(Refusal::strategy_input_contract(
        "Strategy audit runner only supports explicit /bin/sh or /bin/bash shebangs on this platform",
        json!({
            "script": script_path.display().to_string(),
            "shebang": first_line,
            "runtime_allowlist": contract.runtime_allowlist,
        }),
    ))
}

fn sandbox_profile(scratch_dir: &Path) -> String {
    format!(
        "(version 1)\n(allow default)\n(deny network*)\n(deny file-write*)\n(allow file-write* (subpath \"{}\"))\n",
        escape_sbpl_path(scratch_dir)
    )
}

fn escape_sbpl_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn unsupported_platform_refusal(reason: &str, detail: serde_json::Value) -> Refusal {
    Refusal::strategy_input_contract(
        format!("Strategy audit isolated runner is unavailable: {reason}"),
        detail,
    )
}
