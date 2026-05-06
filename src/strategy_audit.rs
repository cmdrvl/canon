use crate::{Refusal, strategy_registry};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

type StrategyAuditResult<T> = Result<T, Refusal>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyAuditSchema {
    pub path: String,
    pub schema_fingerprint: String,
    pub column_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyAuditScript {
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyAuditSuite {
    pub path: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub content_hash: String,
    pub fixture_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyAuditSummary {
    pub fixtures: usize,
    pub passed: usize,
    pub failed: usize,
    pub repeatability_runs: usize,
    pub repeatability_checks: usize,
    pub repeatability_failures: usize,
    pub decision: StrategyAuditDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrategyAuditDecision {
    Proceed,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyAuditFixtureResult {
    pub id: String,
    pub input: String,
    pub expected_stdout: String,
    pub expected_exit_code: i32,
    pub actual_exit_code: i32,
    pub stdout_hash: String,
    pub stderr_hash: String,
    pub output_hash: String,
    pub passed: bool,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyAuditOutput {
    pub version: String,
    pub schema: StrategyAuditSchema,
    pub script: StrategyAuditScript,
    pub suite: StrategyAuditSuite,
    pub summary: StrategyAuditSummary,
    pub deterministic_output_hash: String,
    pub fixtures: Vec<StrategyAuditFixtureResult>,
    pub passed: bool,
    pub decision: String,
    pub sealed: bool,
    pub status: String,
    pub result: String,
}

impl StrategyAuditOutput {
    pub fn exit_code(&self) -> u8 {
        if self.passed { 0 } else { 1 }
    }

    pub fn render_summary(&self) -> String {
        format!(
            "{} audit {} | {}/{} passed, decision={:?}, output={}",
            self.suite.id,
            self.script.content_hash,
            self.summary.passed,
            self.summary.fixtures,
            self.summary.decision,
            self.deterministic_output_hash,
        )
    }
}

#[derive(Debug, Deserialize)]
struct SuiteManifest {
    suite_id: String,
    #[serde(default)]
    version: Option<String>,
    fixtures: Vec<SuiteFixture>,
    #[serde(default = "default_repeatability_runs")]
    repeatability_runs: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct SuiteFixture {
    id: String,
    input: String,
    #[serde(alias = "expected_output")]
    expected_stdout: String,
    #[serde(default)]
    expected_exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RunResultForHash {
    fixture_id: String,
    exit_code: i32,
    stdout_hash: String,
    stderr_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScriptRun {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl ScriptRun {
    fn stdout_hash(&self) -> String {
        hash_bytes(&self.stdout)
    }

    fn stderr_hash(&self) -> String {
        hash_bytes(&self.stderr)
    }

    fn output_hash(&self, fixture_id: &str) -> String {
        hash_json(&RunResultForHash {
            fixture_id: fixture_id.to_string(),
            exit_code: self.exit_code,
            stdout_hash: self.stdout_hash(),
            stderr_hash: self.stderr_hash(),
        })
    }
}

pub fn audit(
    schema_path: &Path,
    script_path: &Path,
    suite_dir: &Path,
) -> StrategyAuditResult<StrategyAuditOutput> {
    let schema = strategy_registry::load_schema_shape(schema_path)?;
    let schema_fingerprint = strategy_registry::fingerprint_schema(&schema)?;
    let script_hash = hash_file(script_path)?;
    let manifest = load_suite_manifest(suite_dir)?;
    let suite_hash = compute_suite_hash(suite_dir, &manifest)?;
    let repeatability_runs = manifest.repeatability_runs.max(2);

    let mut fixtures = Vec::new();
    let mut hash_inputs = Vec::new();
    let mut repeatability_checks = 0usize;

    for fixture in &manifest.fixtures {
        let fixture_result = run_fixture(
            script_path,
            suite_dir,
            fixture,
            repeatability_runs,
            &mut repeatability_checks,
        )?;
        hash_inputs.push(RunResultForHash {
            fixture_id: fixture_result.id.clone(),
            exit_code: fixture_result.actual_exit_code,
            stdout_hash: fixture_result.stdout_hash.clone(),
            stderr_hash: fixture_result.stderr_hash.clone(),
        });
        fixtures.push(fixture_result);
    }

    let passed_count = fixtures.iter().filter(|fixture| fixture.passed).count();
    let failed_count = fixtures.len() - passed_count;
    let passed = failed_count == 0;
    let decision = if passed {
        StrategyAuditDecision::Proceed
    } else {
        StrategyAuditDecision::Reject
    };

    Ok(StrategyAuditOutput {
        version: "canon_strategy_audit.v0".to_string(),
        schema: StrategyAuditSchema {
            path: schema_path.display().to_string(),
            schema_fingerprint,
            column_count: schema.columns.len(),
        },
        script: StrategyAuditScript {
            path: script_path.display().to_string(),
            content_hash: script_hash,
        },
        suite: StrategyAuditSuite {
            path: suite_dir.display().to_string(),
            id: manifest.suite_id,
            version: manifest.version,
            content_hash: suite_hash,
            fixture_count: fixtures.len(),
        },
        summary: StrategyAuditSummary {
            fixtures: fixtures.len(),
            passed: passed_count,
            failed: failed_count,
            repeatability_runs,
            repeatability_checks,
            repeatability_failures: 0,
            decision,
        },
        deterministic_output_hash: hash_json(&hash_inputs),
        fixtures,
        passed,
        decision: if passed { "PROCEED" } else { "REJECT" }.to_string(),
        sealed: passed,
        status: if passed { "PASS" } else { "FAIL" }.to_string(),
        result: if passed { "SUCCESS" } else { "FAILURE" }.to_string(),
    })
}

fn run_fixture(
    script_path: &Path,
    suite_dir: &Path,
    fixture: &SuiteFixture,
    repeatability_runs: usize,
    repeatability_checks: &mut usize,
) -> StrategyAuditResult<StrategyAuditFixtureResult> {
    validate_fixture(fixture)?;
    let input_path = suite_dir.join(&fixture.input);
    let expected_path = suite_dir.join(&fixture.expected_stdout);
    let input = fs::read(&input_path).map_err(|error| {
        Refusal::io_error(&input_path.display().to_string(), &error.to_string())
    })?;
    let expected_stdout = fs::read(&expected_path).map_err(|error| {
        Refusal::io_error(&expected_path.display().to_string(), &error.to_string())
    })?;
    let expected_exit_code = fixture.expected_exit_code.unwrap_or(0);
    let first_run = run_script(script_path, &input)?;

    for run_index in 1..repeatability_runs {
        *repeatability_checks += 1;
        let repeated = run_script(script_path, &input)?;
        if repeated != first_run {
            return Err(Refusal::strategy_proof_invalid(
                "Strategy audit refused nondeterministic script output",
                json!({
                    "fixture_id": fixture.id,
                    "run_index": run_index,
                    "first": script_run_detail(&first_run, &fixture.id),
                    "repeated": script_run_detail(&repeated, &fixture.id),
                }),
            ));
        }
    }

    let mut failures = Vec::new();
    if first_run.exit_code != expected_exit_code {
        failures.push(format!(
            "exit_code expected {} got {}",
            expected_exit_code, first_run.exit_code
        ));
    }
    if first_run.stdout != expected_stdout {
        failures.push("stdout differed from expected output".to_string());
    }

    Ok(StrategyAuditFixtureResult {
        id: fixture.id.clone(),
        input: fixture.input.clone(),
        expected_stdout: fixture.expected_stdout.clone(),
        expected_exit_code,
        actual_exit_code: first_run.exit_code,
        stdout_hash: first_run.stdout_hash(),
        stderr_hash: first_run.stderr_hash(),
        output_hash: first_run.output_hash(&fixture.id),
        passed: failures.is_empty(),
        failures,
    })
}

fn run_script(script_path: &Path, input: &[u8]) -> StrategyAuditResult<ScriptRun> {
    let mut child = Command::new(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            Refusal::io_error(&script_path.display().to_string(), &error.to_string())
        })?;
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(input)
        .map_err(|error| {
            Refusal::io_error(&script_path.display().to_string(), &error.to_string())
        })?;
    let output = child.wait_with_output().map_err(|error| {
        Refusal::io_error(&script_path.display().to_string(), &error.to_string())
    })?;

    Ok(ScriptRun {
        exit_code: output.status.code().unwrap_or(127),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn load_suite_manifest(suite_dir: &Path) -> StrategyAuditResult<SuiteManifest> {
    if !suite_dir.is_dir() {
        return Err(Refusal::strategy_input_contract(
            "Strategy audit suite directory not found",
            json!({ "suite": suite_dir.display().to_string() }),
        ));
    }
    let manifest_path = suite_dir.join("manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|error| {
        Refusal::strategy_input_contract(
            "Strategy audit suite manifest is missing or unreadable",
            json!({
                "suite": suite_dir.display().to_string(),
                "manifest": manifest_path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;
    let manifest: SuiteManifest = serde_json::from_slice(&bytes).map_err(|error| {
        Refusal::strategy_input_contract(
            "Strategy audit suite manifest is malformed",
            json!({
                "suite": suite_dir.display().to_string(),
                "manifest": manifest_path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;
    validate_manifest(suite_dir, &manifest)?;
    Ok(manifest)
}

fn validate_manifest(suite_dir: &Path, manifest: &SuiteManifest) -> StrategyAuditResult<()> {
    if manifest.suite_id.trim().is_empty() {
        return Err(malformed_suite(
            suite_dir,
            "suite_id must be non-empty",
            json!({ "field": "suite_id" }),
        ));
    }
    if manifest.fixtures.is_empty() {
        return Err(malformed_suite(
            suite_dir,
            "fixtures must contain at least one fixture",
            json!({ "field": "fixtures" }),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for fixture in &manifest.fixtures {
        validate_fixture(fixture)?;
        if !seen.insert(fixture.id.clone()) {
            return Err(malformed_suite(
                suite_dir,
                "fixture ids must be unique",
                json!({ "fixture_id": fixture.id }),
            ));
        }
    }
    Ok(())
}

fn validate_fixture(fixture: &SuiteFixture) -> StrategyAuditResult<()> {
    for (field, value) in [
        ("id", fixture.id.as_str()),
        ("input", fixture.input.as_str()),
        ("expected_stdout", fixture.expected_stdout.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(Refusal::strategy_input_contract(
                "Strategy audit suite fixture is malformed",
                json!({ "fixture_id": fixture.id, "field": field }),
            ));
        }
    }
    Ok(())
}

fn malformed_suite(suite_dir: &Path, message: &str, detail: Value) -> Refusal {
    Refusal::strategy_input_contract(
        format!(
            "Strategy audit suite '{}' is malformed: {message}",
            suite_dir.display()
        ),
        detail,
    )
}

fn compute_suite_hash(suite_dir: &Path, manifest: &SuiteManifest) -> StrategyAuditResult<String> {
    let mut paths = vec![suite_dir.join("manifest.json")];
    for fixture in &manifest.fixtures {
        paths.push(suite_dir.join(&fixture.input));
        paths.push(suite_dir.join(&fixture.expected_stdout));
    }
    paths.sort();
    paths.dedup();

    let mut manifest_bytes = Vec::new();
    for path in paths {
        let bytes = fs::read(&path)
            .map_err(|error| Refusal::io_error(&path.display().to_string(), &error.to_string()))?;
        let relative = path
            .strip_prefix(suite_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        manifest_bytes.extend_from_slice(relative.as_bytes());
        manifest_bytes.push(b'\t');
        manifest_bytes.extend_from_slice(bytes.len().to_string().as_bytes());
        manifest_bytes.push(b'\t');
        manifest_bytes.extend_from_slice(hash_bytes(&bytes).as_bytes());
        manifest_bytes.push(b'\n');
    }
    Ok(hash_bytes(&manifest_bytes))
}

fn script_run_detail(run: &ScriptRun, fixture_id: &str) -> Value {
    json!({
        "fixture_id": fixture_id,
        "exit_code": run.exit_code,
        "stdout_hash": run.stdout_hash(),
        "stderr_hash": run.stderr_hash(),
        "output_hash": run.output_hash(fixture_id),
    })
}

fn hash_file(path: &Path) -> StrategyAuditResult<String> {
    let bytes = fs::read(path)
        .map_err(|error| Refusal::io_error(&path.display().to_string(), &error.to_string()))?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes =
        serde_json::to_vec(value).expect("serializing strategy audit hash input is infallible");
    hash_bytes(&bytes)
}

fn default_repeatability_runs() -> usize {
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RefusalCode;
    use std::{error::Error, fs, path::PathBuf};
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn write_schema(dir: &Path) -> PathBuf {
        let path = dir.join("profile.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "columns": [
                    {"name": "name", "type": "string", "cardinality": 2}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        path
    }

    fn write_suite(dir: &Path, expected: &str) -> PathBuf {
        let suite = dir.join("suite");
        fs::create_dir(&suite).unwrap();
        fs::create_dir(suite.join("inputs")).unwrap();
        fs::create_dir(suite.join("expected")).unwrap();
        fs::write(suite.join("inputs/case1.txt"), "Acme\n").unwrap();
        fs::write(suite.join("expected/case1.out"), expected).unwrap();
        fs::write(
            suite.join("manifest.json"),
            serde_json::to_string_pretty(&json!({
                "suite_id": "strategy_suite.v1",
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
    fn write_script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("script.sh");
        fs::write(&path, body).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn audit_passes_fixture_and_repeatability_checks() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        let schema = write_schema(temp.path());
        let suite = write_suite(temp.path(), "Acme\n");
        let script = write_script(temp.path(), "#!/bin/sh\ncat\n");

        let output = audit(&schema, &script, &suite).expect("audit passes");

        assert_eq!(output.version, "canon_strategy_audit.v0");
        assert!(output.passed);
        assert_eq!(output.decision, "PROCEED");
        assert_eq!(output.status, "PASS");
        assert!(output.sealed);
        assert_eq!(output.summary.passed, 1);
        assert_eq!(output.summary.failed, 0);
        assert_eq!(output.summary.repeatability_checks, 1);
        assert!(output.deterministic_output_hash.starts_with("blake3:"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn audit_reports_fixture_failure_without_refusal() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        let schema = write_schema(temp.path());
        let suite = write_suite(temp.path(), "Different\n");
        let script = write_script(temp.path(), "#!/bin/sh\ncat\n");

        let output = audit(&schema, &script, &suite).expect("audit emits failure artifact");

        assert!(!output.passed);
        assert_eq!(output.decision, "REJECT");
        assert_eq!(output.status, "FAIL");
        assert_eq!(output.exit_code(), 1);
        assert_eq!(output.summary.failed, 1);
        assert_eq!(
            output.fixtures[0].failures,
            vec!["stdout differed from expected output"]
        );
        Ok(())
    }

    #[test]
    fn audit_refuses_malformed_suite() {
        let temp = TempDir::new().unwrap();
        let schema = write_schema(temp.path());
        let script = temp.path().join("missing-script");
        fs::write(&script, "").unwrap();
        let suite = temp.path().join("missing-suite");

        let refusal = audit(&schema, &script, &suite).unwrap_err();

        assert_eq!(refusal.code, RefusalCode::EStrategyInputContract);
    }

    #[cfg(unix)]
    #[test]
    fn audit_refuses_nondeterministic_output() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new()?;
        let schema = write_schema(temp.path());
        let suite = write_suite(temp.path(), "Acme 1\n");
        let script = write_script(
            temp.path(),
            "#!/bin/sh\nCOUNT_FILE=\"$0.count\"\nif [ -f \"$COUNT_FILE\" ]; then n=$(cat \"$COUNT_FILE\"); else n=0; fi\nn=$((n + 1))\necho \"$n\" > \"$COUNT_FILE\"\nwhile IFS= read -r line; do printf '%s %s\\n' \"$line\" \"$n\"; done\n",
        );

        let refusal = audit(&schema, &script, &suite).unwrap_err();

        assert_eq!(refusal.code, RefusalCode::EStrategyProofInvalid);
        assert!(refusal.message.contains("nondeterministic"));
        Ok(())
    }
}
