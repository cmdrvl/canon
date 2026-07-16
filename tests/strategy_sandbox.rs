#[cfg(all(unix, not(target_os = "macos")))]
use canon::RefusalCode;
use canon::strategy_audit::audit;
#[cfg(target_os = "macos")]
use canon::strategy_audit::{StrategyAuditOutput, StrategyAuditTerminationCategory};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn write_schema(dir: &Path) -> PathBuf {
    let path = dir.join("profile.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&json!({
            "columns": [
                {"name": "name", "type": "string", "cardinality": 1}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn write_suite(dir: &Path, expected: &[u8]) -> PathBuf {
    let suite = dir.join("suite");
    fs::create_dir(&suite).unwrap();
    fs::create_dir(suite.join("inputs")).unwrap();
    fs::create_dir(suite.join("expected")).unwrap();
    fs::write(suite.join("inputs/case1.txt"), "Acme\n").unwrap();
    fs::write(suite.join("expected/case1.out"), expected).unwrap();
    fs::write(
        suite.join("manifest.json"),
        serde_json::to_string_pretty(&json!({
            "suite_id": "strategy_sandbox_suite.v1",
            "version": "1.0.0",
            "repeatability_runs": 2,
            "fixtures": [
                {
                    "id": "case1",
                    "input": "inputs/case1.txt",
                    "expected_stdout": "expected/case1.out",
                    "expected_exit_code": 0
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    suite
}

#[cfg(unix)]
fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(target_os = "macos")]
fn log_output(label: &str, output: &StrategyAuditOutput) {
    println!(
        "runner {label}: {}",
        serde_json::to_string_pretty(&output.runner).unwrap()
    );
    println!(
        "summary {label}: {}",
        serde_json::to_string_pretty(&output.summary).unwrap()
    );
    for fixture in &output.fixtures {
        println!(
            "fixture {label}: id={} termination={} reason={} exit={:?} signal={:?} stdout_hash={} stderr_hash={} command={:?}",
            fixture.id,
            serde_json::to_string(&fixture.termination.category).unwrap(),
            fixture.termination.reason,
            fixture.termination.exit_code,
            fixture.termination.signal,
            fixture.stdout_hash,
            fixture.stderr_hash,
            fixture.termination.command,
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn strategy_audit_exposes_runner_contract_and_clears_environment() {
    let temp = TempDir::new().unwrap();
    let schema = write_schema(temp.path());
    let suite = write_suite(temp.path(), b"unset\n");
    let script = write_script(
        temp.path(),
        "env.sh",
        "#!/bin/sh\nprintf '%s\\n' \"${HOME-unset}\"\n",
    );

    let output = audit(&schema, &script, &suite).unwrap();

    log_output("env", &output);
    assert!(output.passed);
    assert_eq!(output.runner.platform, "darwin-sandbox-exec");
    assert_eq!(output.runner.network_policy, "deny_all");
    assert_eq!(
        output.fixtures[0].termination.category,
        StrategyAuditTerminationCategory::Completed
    );
}

#[cfg(target_os = "macos")]
#[test]
fn strategy_audit_times_out_infinite_loop() {
    let temp = TempDir::new().unwrap();
    let schema = write_schema(temp.path());
    let suite = write_suite(temp.path(), b"");
    let script = write_script(temp.path(), "loop.sh", "#!/bin/sh\nsleep 10\n");

    let output = audit(&schema, &script, &suite).unwrap();

    log_output("timeout", &output);
    assert!(!output.passed);
    assert_eq!(output.summary.timeout_failures, 1);
    assert_eq!(
        output.fixtures[0].termination.category,
        StrategyAuditTerminationCategory::Timeout
    );
    assert!(
        output.fixtures[0].failures[0].contains("timeout"),
        "expected timeout failure, got {:?}",
        output.fixtures[0].failures
    );
}

#[cfg(target_os = "macos")]
#[test]
fn strategy_audit_denies_filesystem_escape() {
    let temp = TempDir::new().unwrap();
    let schema = write_schema(temp.path());
    let suite = write_suite(temp.path(), b"safe\n");
    let outside = temp.path().join("outside.txt");
    let body = format!(
        "#!/bin/sh\nprintf 'escape\\n' > \"{}\" || true\nprintf 'safe\\n'\n",
        outside.display()
    );
    let script = write_script(temp.path(), "escape.sh", &body);

    let output = audit(&schema, &script, &suite).unwrap();

    log_output("filesystem", &output);
    assert!(!output.passed);
    assert_eq!(output.summary.policy_denials, 1);
    assert_eq!(
        output.fixtures[0].termination.category,
        StrategyAuditTerminationCategory::PolicyDenied
    );
    assert!(
        !outside.exists(),
        "sandbox must block writes outside scratch"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn strategy_audit_denies_network_access() {
    let temp = TempDir::new().unwrap();
    let schema = write_schema(temp.path());
    let suite = write_suite(temp.path(), b"offline\n");
    let script = write_script(
        temp.path(),
        "network.sh",
        "#!/bin/bash\nexec 3<>/dev/tcp/1.1.1.1/80 || true\nprintf 'offline\\n'\n",
    );

    let output = audit(&schema, &script, &suite).unwrap();

    log_output("network", &output);
    assert!(!output.passed);
    assert_eq!(output.summary.policy_denials, 1);
    assert_eq!(
        output.fixtures[0].termination.category,
        StrategyAuditTerminationCategory::PolicyDenied
    );
}

#[cfg(target_os = "macos")]
#[test]
fn strategy_audit_bounds_output_and_marks_resource_limit() {
    let temp = TempDir::new().unwrap();
    let schema = write_schema(temp.path());
    let suite = write_suite(temp.path(), b"");
    let script = write_script(temp.path(), "spam.sh", "#!/bin/sh\nyes 0123456789abcdef\n");

    let output = audit(&schema, &script, &suite).unwrap();

    log_output("output-limit", &output);
    assert!(!output.passed);
    assert_eq!(output.summary.resource_limit_failures, 1);
    assert_eq!(
        output.fixtures[0].termination.category,
        StrategyAuditTerminationCategory::ResourceLimit
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn strategy_audit_refuses_unsupported_platform() {
    let temp = TempDir::new().unwrap();
    let schema = write_schema(temp.path());
    let suite = write_suite(temp.path(), b"Acme\n");
    let script = write_script(temp.path(), "script.sh", "#!/bin/sh\ncat\n");

    let refusal = audit(&schema, &script, &suite).unwrap_err();

    assert_eq!(refusal.code, RefusalCode::EStrategyInputContract);
    assert!(refusal.message.contains("isolated runner"));
}
