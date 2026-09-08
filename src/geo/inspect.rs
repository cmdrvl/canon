#![forbid(unsafe_code)]

//! Read-only inspection over a published Geo run work directory.
//!
//! Inspection is a projection over stored run manifests, project receipts, and
//! content-addressed output bytes. It does not execute Geo plan nodes.

use super::{
    CANON_GEO_COMPOSITION_VERSION, CANON_GEO_EVIDENCE_COMPILATION_VERSION,
    CANON_GEO_EXPLANATION_VERSION, CANON_GEO_NEXT_EVIDENCE_VERSION, CANON_GEO_PROPAGATION_VERSION,
    CANON_GEO_RUN_VERSION, CANON_GEO_SEPARATION_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION,
    GeoClaimClass, GeoEvidenceDisposition, GeoRun, GeoRunArtifactRef, GeoRunOutputRef,
    GeoRunPlanRef, canonical_geo_run_bytes, geo_run_manifest_head_path, read_geo_run_manifest_head,
};
use crate::{
    project::receipt::digest_bytes,
    project::{ProjectRunOutputReceipt, ProjectRunPolicy},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

pub const CANON_GEO_INSPECTION_VERSION: &str = "canon_geo_inspection.v0";
const GEO_RUN_JSON_MEDIA_TYPE: &str = "application/json";
const GEO_RUN_MANIFEST_ARTIFACT_ID: &str = "geo-run-manifest/head.json";
const DEFAULT_MAX_COMPONENT_REFS: u64 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeoInspectOptions {
    pub component_id: Option<String>,
    pub recommend_next: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GeoInspectionQuestion {
    #[serde(rename = "Q1")]
    Q1,
    #[serde(rename = "Q2")]
    Q2,
    #[serde(rename = "Q3")]
    Q3,
    #[serde(rename = "Q4")]
    Q4,
    #[serde(rename = "Q5")]
    Q5,
    #[serde(rename = "Q6")]
    Q6,
    #[serde(rename = "Q7")]
    Q7,
    #[serde(rename = "Q8")]
    Q8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoInspectErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    InspectArtifactMissing,
    InspectQuestionUnanswerable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspectError {
    pub code: GeoInspectErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoInspectError {
    fn new(
        code: GeoInspectErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    fn invalid(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoInspectErrorCode::InvalidInput, message, detail)
    }

    fn artifact_missing(
        artifact: impl Into<String>,
        expected_digest: impl Into<String>,
        actual_digest: impl Into<String>,
        message: impl Into<String>,
        extra: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        let mut detail = BTreeMap::from([
            ("artifact".to_string(), artifact.into()),
            ("expected_digest".to_string(), expected_digest.into()),
            ("actual_digest".to_string(), actual_digest.into()),
        ]);
        detail.extend(
            extra
                .into_iter()
                .map(|(key, value)| (key.into(), value.into())),
        );
        Self::new(GeoInspectErrorCode::InspectArtifactMissing, message, detail)
    }
}

impl fmt::Display for GeoInspectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoInspectError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspectionAbstention {
    pub code: GeoInspectErrorCode,
    pub message: String,
    pub missing_contract: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspectionAnswer {
    pub question: GeoInspectionQuestion,
    pub answer: String,
    pub artifact_refs: Vec<GeoRunArtifactRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstention: Option<GeoInspectionAbstention>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspectionPlanes {
    pub availability: String,
    pub coverage: String,
    pub candidate_reach: String,
    pub admission: String,
    pub solver_exactness: String,
    pub reconciliation: String,
    pub truth_quality: String,
    pub cost: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspectionBounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    pub max_component_refs: u64,
    pub component_ref_count: u64,
    pub component_refs_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspectionMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual_model_count: Option<u64>,
    pub artifact_digests: BTreeMap<String, String>,
    pub backbone_members: Vec<String>,
    pub contradictions: Vec<String>,
    pub component_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_class: Option<GeoClaimClass>,
    pub proof_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspectionDelta {
    pub evidence_added: Vec<String>,
    pub evidence_removed: Vec<String>,
    pub components_invalidated: Vec<String>,
    pub model_count_before: u64,
    pub model_count_after: u64,
    pub backbone_gained: Vec<String>,
    pub backbone_lost: Vec<String>,
    pub contradictions_introduced: Vec<String>,
    pub contradictions_resolved: Vec<String>,
    pub claim_class_changes: Vec<(GeoClaimClass, GeoClaimClass)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoInspection {
    pub version: String,
    pub run_id: String,
    pub semantic_hash: String,
    pub plan_ref: GeoRunPlanRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    pub recommend_next: bool,
    pub planes: GeoInspectionPlanes,
    pub bounds: GeoInspectionBounds,
    pub metrics: GeoInspectionMetrics,
    pub answers: Vec<GeoInspectionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<GeoInspectionDelta>,
}

struct StoredArtifact {
    reference: GeoRunArtifactRef,
    value: Value,
}

struct StoredRun {
    run: GeoRun,
    manifest_ref: GeoRunArtifactRef,
    artifacts: BTreeMap<String, StoredArtifact>,
    artifacts_by_contract: BTreeMap<String, Vec<String>>,
}

pub fn inspect(work_dir: &Path) -> Result<GeoInspection, GeoInspectError> {
    inspect_with_options(work_dir, GeoInspectOptions::default())
}

pub fn inspect_with_options(
    work_dir: &Path,
    options: GeoInspectOptions,
) -> Result<GeoInspection, GeoInspectError> {
    if let Some(component_id) = &options.component_id
        && component_id.trim().is_empty()
    {
        return Err(GeoInspectError::invalid(
            "Geo inspect component id must be nonempty",
            [("field", "component_id")],
        ));
    }
    let stored = read_stored_run(work_dir)?;
    build_inspection(&stored, options)
}

pub fn inspect_with_compare(
    work_dir: &Path,
    other_work_dir: &Path,
    options: GeoInspectOptions,
) -> Result<GeoInspection, GeoInspectError> {
    let mut base = inspect_with_options(work_dir, options)?;
    let other = inspect(other_work_dir)?;
    base.compare = Some(compare(&base, &other)?);
    stamp_inspection(base)
}

pub fn compare(
    base: &GeoInspection,
    other: &GeoInspection,
) -> Result<GeoInspectionDelta, GeoInspectError> {
    if base.plan_ref.question_hash != other.plan_ref.question_hash {
        return Err(GeoInspectError::invalid(
            "Geo inspect comparison requires runs with the same plan_ref.question_hash",
            [
                ("plan_ref", "question_hash"),
                ("base", base.plan_ref.question_hash.as_str()),
                ("other", other.plan_ref.question_hash.as_str()),
            ],
        ));
    }

    let base_artifacts = &base.metrics.artifact_digests;
    let other_artifacts = &other.metrics.artifact_digests;
    let evidence_added = other_artifacts
        .iter()
        .filter(|(artifact_id, digest)| base_artifacts.get(*artifact_id) != Some(*digest))
        .map(|(artifact_id, digest)| format!("{artifact_id}@{digest}"))
        .collect::<Vec<_>>();
    let evidence_removed = base_artifacts
        .iter()
        .filter(|(artifact_id, digest)| other_artifacts.get(*artifact_id) != Some(*digest))
        .map(|(artifact_id, digest)| format!("{artifact_id}@{digest}"))
        .collect::<Vec<_>>();

    let components_invalidated = other
        .metrics
        .component_keys
        .iter()
        .filter(|component| !base.metrics.component_keys.contains(component))
        .cloned()
        .collect::<Vec<_>>();
    let backbone_gained = other
        .metrics
        .backbone_members
        .iter()
        .filter(|member| !base.metrics.backbone_members.contains(member))
        .cloned()
        .collect::<Vec<_>>();
    let backbone_lost = base
        .metrics
        .backbone_members
        .iter()
        .filter(|member| !other.metrics.backbone_members.contains(member))
        .cloned()
        .collect::<Vec<_>>();
    let contradictions_introduced = other
        .metrics
        .contradictions
        .iter()
        .filter(|constraint| !base.metrics.contradictions.contains(constraint))
        .cloned()
        .collect::<Vec<_>>();
    let contradictions_resolved = base
        .metrics
        .contradictions
        .iter()
        .filter(|constraint| !other.metrics.contradictions.contains(constraint))
        .cloned()
        .collect::<Vec<_>>();
    let claim_class_changes = match (base.metrics.claim_class, other.metrics.claim_class) {
        (Some(left), Some(right)) if left != right => vec![(left, right)],
        _ => Vec::new(),
    };

    Ok(GeoInspectionDelta {
        evidence_added,
        evidence_removed,
        components_invalidated,
        model_count_before: base.metrics.residual_model_count.unwrap_or(0),
        model_count_after: other.metrics.residual_model_count.unwrap_or(0),
        backbone_gained,
        backbone_lost,
        contradictions_introduced,
        contradictions_resolved,
        claim_class_changes,
    })
}

pub fn validate_inspection_artifact(inspection: &GeoInspection) -> Result<(), GeoInspectError> {
    if inspection.version != CANON_GEO_INSPECTION_VERSION {
        return Err(GeoInspectError::new(
            GeoInspectErrorCode::UnsupportedVersion,
            "Geo inspection artifact has unsupported version",
            [
                ("expected", CANON_GEO_INSPECTION_VERSION),
                ("actual", inspection.version.as_str()),
            ],
        ));
    }
    validate_nonempty("run_id", &inspection.run_id)?;
    validate_digest("semantic_hash", &inspection.semantic_hash)?;
    validate_plan_ref(&inspection.plan_ref)?;
    validate_nonempty("planes.availability", &inspection.planes.availability)?;
    validate_nonempty("planes.coverage", &inspection.planes.coverage)?;
    validate_nonempty("planes.candidate_reach", &inspection.planes.candidate_reach)?;
    validate_nonempty("planes.admission", &inspection.planes.admission)?;
    validate_nonempty(
        "planes.solver_exactness",
        &inspection.planes.solver_exactness,
    )?;
    validate_nonempty("planes.reconciliation", &inspection.planes.reconciliation)?;
    validate_nonempty("planes.truth_quality", &inspection.planes.truth_quality)?;
    validate_nonempty("planes.cost", &inspection.planes.cost)?;
    if inspection.bounds.max_component_refs == 0 {
        return Err(GeoInspectError::invalid(
            "Geo inspection max_component_refs must be positive",
            [("field", "bounds.max_component_refs")],
        ));
    }
    if inspection.component_id != inspection.bounds.component_id {
        return Err(GeoInspectError::invalid(
            "Geo inspection component_id must match bounds.component_id",
            [("field", "bounds.component_id")],
        ));
    }
    validate_nonempty("metrics.proof_class", &inspection.metrics.proof_class)?;
    if inspection.metrics.component_keys.len() as u64 > inspection.bounds.max_component_refs {
        return Err(GeoInspectError::invalid(
            "Geo inspection component detail exceeds max_component_refs",
            [
                ("field", "metrics.component_keys".to_string()),
                (
                    "max_component_refs",
                    inspection.bounds.max_component_refs.to_string(),
                ),
                (
                    "component_keys",
                    inspection.metrics.component_keys.len().to_string(),
                ),
            ],
        ));
    }
    for (artifact_id, digest) in &inspection.metrics.artifact_digests {
        validate_nonempty("metrics.artifact_digests.key", artifact_id)?;
        validate_digest("metrics.artifact_digests.value", digest)?;
    }
    let questions = inspection_questions();
    if inspection.answers.len() != questions.len() {
        return Err(GeoInspectError::invalid(
            "Geo inspection must answer exactly the eight architecture questions",
            [
                ("expected", questions.len().to_string()),
                ("actual", inspection.answers.len().to_string()),
            ],
        ));
    }
    for (index, answer) in inspection.answers.iter().enumerate() {
        if answer.question != questions[index] {
            return Err(GeoInspectError::invalid(
                "Geo inspection questions are not in architecture order",
                [
                    ("index", index.to_string()),
                    ("expected", question_id(questions[index])),
                    ("actual", question_id(answer.question)),
                ],
            ));
        }
        validate_nonempty("answers.answer", &answer.answer)?;
        if answer.artifact_refs.is_empty() {
            return Err(GeoInspectError::invalid(
                "Geo inspection answers must carry at least one artifact ref",
                [("question", question_id(answer.question))],
            ));
        }
        let mut prior = None::<String>;
        for reference in &answer.artifact_refs {
            validate_artifact_ref(reference)?;
            if prior
                .as_ref()
                .is_some_and(|left| left >= &reference.artifact_id)
            {
                return Err(GeoInspectError::invalid(
                    "Geo inspection answer artifact refs must be sorted and distinct",
                    [
                        ("question", question_id(answer.question)),
                        ("artifact_id", reference.artifact_id.clone()),
                    ],
                ));
            }
            prior = Some(reference.artifact_id.clone());
        }
        if let Some(abstention) = &answer.abstention {
            if abstention.code != GeoInspectErrorCode::InspectQuestionUnanswerable {
                return Err(GeoInspectError::invalid(
                    "Geo inspection abstention must use inspect_question_unanswerable",
                    [("question", question_id(answer.question))],
                ));
            }
            validate_nonempty("answers.abstention.message", &abstention.message)?;
            validate_nonempty(
                "answers.abstention.missing_contract",
                &abstention.missing_contract,
            )?;
        }
    }
    let expected_hash = inspection_semantic_hash(inspection)?;
    if inspection.semantic_hash != expected_hash {
        return Err(GeoInspectError::invalid(
            "Geo inspection semantic_hash does not match its semantic projection",
            [
                ("expected", expected_hash),
                ("actual", inspection.semantic_hash.clone()),
            ],
        ));
    }
    Ok(())
}

pub fn canonical_inspection_bytes(inspection: &GeoInspection) -> Result<Vec<u8>, GeoInspectError> {
    validate_inspection_artifact(inspection)?;
    serde_json::to_vec(inspection).map_err(|error| {
        GeoInspectError::invalid(
            "Geo inspection artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn inspection_semantic_hash(inspection: &GeoInspection) -> Result<String, GeoInspectError> {
    #[derive(Serialize)]
    struct Projection<'a> {
        version: &'a str,
        run_id: &'a str,
        plan_ref: &'a GeoRunPlanRef,
        component_id: &'a Option<String>,
        recommend_next: bool,
        planes: &'a GeoInspectionPlanes,
        bounds: &'a GeoInspectionBounds,
        metrics: &'a GeoInspectionMetrics,
        answers: &'a [GeoInspectionAnswer],
        compare: &'a Option<GeoInspectionDelta>,
    }

    let projection = Projection {
        version: &inspection.version,
        run_id: &inspection.run_id,
        plan_ref: &inspection.plan_ref,
        component_id: &inspection.component_id,
        recommend_next: inspection.recommend_next,
        planes: &inspection.planes,
        bounds: &inspection.bounds,
        metrics: &inspection.metrics,
        answers: &inspection.answers,
        compare: &inspection.compare,
    };
    serde_json::to_vec(&projection)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| {
            GeoInspectError::invalid(
                "Geo inspection semantic projection could not be serialized",
                [("error", error.to_string())],
            )
        })
}

pub fn inspection_summary(inspection: &GeoInspection) -> String {
    let mut lines = Vec::with_capacity(12);
    lines.push(format!("version: {}", inspection.version));
    lines.push(format!("run_id: {}", inspection.run_id));
    lines.push(format!("semantic_hash: {}", inspection.semantic_hash));
    lines.push(format!("status: {}", inspection.planes.solver_exactness));
    lines.push(format!(
        "candidate_reach: {}",
        inspection.planes.candidate_reach
    ));
    lines.push(format!(
        "truth_quality: {}",
        inspection.planes.truth_quality
    ));
    lines.push(format!("cost: {}", inspection.planes.cost));
    for answer in &inspection.answers {
        lines.push(format!(
            "{}: {}",
            question_id(answer.question),
            answer.answer
        ));
    }
    lines.join("\n")
}

fn build_inspection(
    stored: &StoredRun,
    options: GeoInspectOptions,
) -> Result<GeoInspection, GeoInspectError> {
    let composition = stored.first_by_contract(CANON_GEO_COMPOSITION_VERSION);
    let evidence = stored.first_by_contract(CANON_GEO_EVIDENCE_COMPILATION_VERSION);
    let section = stored.first_by_contract(CANON_GEO_TILE_WORK_UNIT_VERSION);
    let explanation = stored.first_by_contract(CANON_GEO_EXPLANATION_VERSION);
    let separation = stored.first_by_contract(CANON_GEO_SEPARATION_VERSION);
    let next_evidence = stored.first_by_contract(CANON_GEO_NEXT_EVIDENCE_VERSION);
    let propagation = stored.first_by_contract(CANON_GEO_PROPAGATION_VERSION);

    let planes = inspection_planes(stored, section, evidence, composition, explanation);
    let bounds = inspection_bounds(options.component_id.clone(), composition);
    let metrics = inspection_metrics(stored, composition, explanation, &bounds);
    let answers = vec![
        answer_q1(stored),
        answer_q2(stored, evidence),
        answer_q3(stored, evidence),
        answer_q4(stored, section),
        answer_q5(stored, composition, &bounds),
        answer_q6(stored, composition, explanation),
        answer_q7(stored),
        answer_q8(
            stored,
            next_evidence,
            separation,
            propagation,
            options.recommend_next,
        ),
    ];
    stamp_inspection(GeoInspection {
        version: CANON_GEO_INSPECTION_VERSION.to_string(),
        run_id: stored.run.run_id.clone(),
        semantic_hash: String::new(),
        plan_ref: stored.run.plan_ref.clone(),
        component_id: options.component_id,
        recommend_next: options.recommend_next,
        planes,
        bounds,
        metrics,
        answers,
        compare: None,
    })
}

fn stamp_inspection(mut inspection: GeoInspection) -> Result<GeoInspection, GeoInspectError> {
    inspection.semantic_hash.clear();
    inspection.semantic_hash = inspection_semantic_hash(&inspection)?;
    validate_inspection_artifact(&inspection)?;
    Ok(inspection)
}

fn read_stored_run(work_dir: &Path) -> Result<StoredRun, GeoInspectError> {
    let policy = ProjectRunPolicy::new(work_dir, ".canon/geo-run");
    let head_path = geo_run_manifest_head_path(&policy).map_err(|error| {
        GeoInspectError::artifact_missing(
            GEO_RUN_MANIFEST_ARTIFACT_ID,
            "present",
            "missing",
            "Geo inspect could not resolve the Geo run manifest head path",
            [
                ("geo_run_error_code", format!("{:?}", error.code)),
                ("message", error.message),
            ],
        )
    })?;
    let run = read_geo_run_manifest_head(&policy)
        .map_err(|error| {
            GeoInspectError::artifact_missing(
                GEO_RUN_MANIFEST_ARTIFACT_ID,
                "canonical",
                "invalid",
                "Geo inspect could not validate the stored Geo run manifest head",
                [
                    ("geo_run_error_code", format!("{:?}", error.code)),
                    ("message", error.message),
                ],
            )
        })?
        .ok_or_else(|| {
            GeoInspectError::artifact_missing(
                GEO_RUN_MANIFEST_ARTIFACT_ID,
                "present",
                "missing",
                "Geo inspect requires a published Geo run manifest head",
                [(
                    "recovery_command",
                    "canon geo run --plan <PLAN.json> --work-dir <DIR> --input <NODE_ID:BINDING_ID=PATH>",
                )],
            )
        })?;
    let head_bytes = read_non_symlink(&head_path, GEO_RUN_MANIFEST_ARTIFACT_ID, "present")?;
    let canonical = canonical_geo_run_bytes(&run).map_err(|error| {
        GeoInspectError::artifact_missing(
            GEO_RUN_MANIFEST_ARTIFACT_ID,
            "canonical",
            "invalid",
            "Geo inspect could not canonicalize the stored Geo run manifest head",
            [
                ("geo_run_error_code", format!("{:?}", error.code)),
                ("message", error.message),
            ],
        )
    })?;
    if head_bytes != canonical {
        return Err(GeoInspectError::artifact_missing(
            GEO_RUN_MANIFEST_ARTIFACT_ID,
            digest_bytes(&canonical),
            digest_bytes(&head_bytes),
            "Geo inspect found noncanonical Geo run manifest head bytes",
            [("path", head_path.display().to_string())],
        ));
    }
    let manifest_ref = GeoRunArtifactRef {
        node_id: "geo.run".to_string(),
        binding_id: "manifest_head".to_string(),
        artifact_id: GEO_RUN_MANIFEST_ARTIFACT_ID.to_string(),
        content_digest: digest_bytes(&head_bytes),
        media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
        contract_version: CANON_GEO_RUN_VERSION.to_string(),
        byte_count: head_bytes.len() as u64,
    };

    let receipts_by_output = output_receipts_by_ref(&run)?;
    let mut artifacts = BTreeMap::new();
    let mut artifacts_by_contract = BTreeMap::<String, Vec<String>>::new();
    for output_ref in &run.output_refs {
        if let Some(receipt) = receipts_by_output.get(&(
            output_ref.project_node_id.clone(),
            output_ref.output_id.clone(),
        )) {
            validate_receipt_matches_output_ref(receipt, output_ref)?;
        }
        let stored = read_output_ref_from_cas(work_dir, output_ref)?;
        artifacts_by_contract
            .entry(output_ref.contract_version.clone())
            .or_default()
            .push(output_ref.artifact_id.clone());
        artifacts.insert(output_ref.artifact_id.clone(), stored);
    }
    for values in artifacts_by_contract.values_mut() {
        values.sort();
        values.dedup();
    }

    Ok(StoredRun {
        run,
        manifest_ref,
        artifacts,
        artifacts_by_contract,
    })
}

fn output_receipts_by_ref(
    run: &GeoRun,
) -> Result<BTreeMap<(String, String), ProjectRunOutputReceipt>, GeoInspectError> {
    let Some(report) = &run.project_run_report else {
        if run.output_refs.is_empty() {
            return Ok(BTreeMap::new());
        }
        return Err(GeoInspectError::artifact_missing(
            "project_run_report",
            "canon.project.run.v2",
            "missing",
            "Geo inspect cannot validate output refs without stored project run receipts",
            [(
                "recovery_command",
                "canon geo run --plan <PLAN.json> --work-dir <DIR> --input <NODE_ID:BINDING_ID=PATH>",
            )],
        ));
    };
    let mut receipts = BTreeMap::new();
    for receipt in &report.receipt.node_receipts {
        for output in &receipt.outputs {
            receipts.insert(
                (receipt.node_id.clone(), output.output_id.clone()),
                output.clone(),
            );
        }
    }
    for output_ref in &run.output_refs {
        let key = (
            output_ref.project_node_id.clone(),
            output_ref.output_id.clone(),
        );
        if !receipts.contains_key(&key) {
            return Err(GeoInspectError::artifact_missing(
                output_ref.artifact_id.clone(),
                output_ref.content_digest.clone(),
                "receipt_missing",
                "Geo inspect output ref has no matching project receipt output",
                [
                    ("project_node_id", output_ref.project_node_id.as_str()),
                    ("output_id", output_ref.output_id.as_str()),
                ],
            ));
        }
    }
    Ok(receipts)
}

fn validate_receipt_matches_output_ref(
    receipt: &ProjectRunOutputReceipt,
    output_ref: &GeoRunOutputRef,
) -> Result<(), GeoInspectError> {
    if receipt.content_digest != output_ref.content_digest
        || receipt.byte_count != output_ref.byte_count
    {
        return Err(GeoInspectError::artifact_missing(
            output_ref.artifact_id.clone(),
            output_ref.content_digest.clone(),
            receipt.content_digest.clone(),
            "Geo inspect found output ref and project receipt digest/count drift",
            [
                ("project_node_id", output_ref.project_node_id.as_str()),
                ("output_id", output_ref.output_id.as_str()),
            ],
        ));
    }
    Ok(())
}

fn read_output_ref_from_cas(
    work_dir: &Path,
    output_ref: &GeoRunOutputRef,
) -> Result<StoredArtifact, GeoInspectError> {
    let path = artifact_cas_path(work_dir, &output_ref.content_digest)?;
    let bytes = read_non_symlink(
        &path,
        &output_ref.artifact_id,
        output_ref.content_digest.as_str(),
    )?;
    let actual_digest = digest_bytes(&bytes);
    if actual_digest != output_ref.content_digest || bytes.len() as u64 != output_ref.byte_count {
        return Err(GeoInspectError::artifact_missing(
            output_ref.artifact_id.clone(),
            output_ref.content_digest.clone(),
            actual_digest,
            "Geo inspect found content-addressed artifact digest/count drift",
            [
                ("path", path.display().to_string()),
                ("expected_byte_count", output_ref.byte_count.to_string()),
                ("actual_byte_count", bytes.len().to_string()),
            ],
        ));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        GeoInspectError::artifact_missing(
            output_ref.artifact_id.clone(),
            output_ref.content_digest.clone(),
            actual_digest.clone(),
            "Geo inspect could not parse content-addressed artifact JSON",
            [
                ("path", path.display().to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    let actual_version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    if actual_version != output_ref.contract_version {
        return Err(GeoInspectError::artifact_missing(
            output_ref.artifact_id.clone(),
            output_ref.content_digest.clone(),
            actual_digest,
            "Geo inspect found content-addressed artifact with the wrong contract version",
            [
                ("expected_contract", output_ref.contract_version.as_str()),
                ("actual_contract", actual_version),
            ],
        ));
    }
    Ok(StoredArtifact {
        reference: output_ref_as_artifact_ref(output_ref),
        value,
    })
}

fn artifact_cas_path(work_dir: &Path, content_digest: &str) -> Result<PathBuf, GeoInspectError> {
    let digest_hex = digest_hex("content_digest", content_digest)?;
    Ok(work_dir
        .join(".canon")
        .join("geo-run")
        .join("artifacts")
        .join("cas")
        .join(format!("{digest_hex}.bin")))
}

fn read_non_symlink(
    path: &Path,
    artifact: &str,
    expected_digest: &str,
) -> Result<Vec<u8>, GeoInspectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(GeoInspectError::artifact_missing(
                artifact,
                expected_digest,
                "symlink",
                "Geo inspect refuses symlinked stored artifacts",
                [("path", path.display().to_string())],
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(GeoInspectError::artifact_missing(
                artifact,
                expected_digest,
                "missing",
                "Geo inspect could not find the stored artifact",
                [
                    ("path", path.display().to_string()),
                    (
                        "recovery_command",
                        "canon geo run --plan <PLAN.json> --work-dir <DIR> --input <NODE_ID:BINDING_ID=PATH>"
                            .to_string(),
                    ),
                ],
            ));
        }
        Err(error) => {
            return Err(GeoInspectError::artifact_missing(
                artifact,
                expected_digest,
                "unreadable",
                "Geo inspect could not stat the stored artifact",
                [
                    ("path", path.display().to_string()),
                    ("error", error.to_string()),
                ],
            ));
        }
    }
    fs::read(path).map_err(|error| {
        GeoInspectError::artifact_missing(
            artifact,
            expected_digest,
            "unreadable",
            "Geo inspect could not read the stored artifact",
            [
                ("path", path.display().to_string()),
                ("error", error.to_string()),
            ],
        )
    })
}

impl StoredRun {
    fn first_by_contract(&self, contract: &str) -> Option<&StoredArtifact> {
        self.artifacts_by_contract
            .get(contract)
            .and_then(|ids| ids.first())
            .and_then(|artifact_id| self.artifacts.get(artifact_id))
    }
}

fn output_ref_as_artifact_ref(output_ref: &GeoRunOutputRef) -> GeoRunArtifactRef {
    GeoRunArtifactRef {
        node_id: output_ref.project_node_id.clone(),
        binding_id: output_ref.output_id.clone(),
        artifact_id: output_ref.artifact_id.clone(),
        content_digest: output_ref.content_digest.clone(),
        media_type: output_ref.media_type.clone(),
        contract_version: output_ref.contract_version.clone(),
        byte_count: output_ref.byte_count,
    }
}

fn inspection_metrics(
    stored: &StoredRun,
    composition: Option<&StoredArtifact>,
    explanation: Option<&StoredArtifact>,
    bounds: &GeoInspectionBounds,
) -> GeoInspectionMetrics {
    let mut artifact_digests = BTreeMap::from([(
        stored.manifest_ref.artifact_id.clone(),
        stored.manifest_ref.content_digest.clone(),
    )]);
    artifact_digests.extend(stored.artifacts.values().map(|artifact| {
        (
            artifact.reference.artifact_id.clone(),
            artifact.reference.content_digest.clone(),
        )
    }));
    let residual_model_count = composition
        .and_then(|artifact| artifact.value.pointer("/summary/residual_model_count"))
        .and_then(Value::as_u64);
    let mut component_keys = composition
        .and_then(|artifact| artifact.value.get("factorization"))
        .and_then(Value::as_array)
        .map(|components| {
            components
                .iter()
                .filter_map(|component| component.get("key").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    component_keys.sort();
    component_keys.dedup();
    if let Some(component_id) = &bounds.component_id {
        component_keys.retain(|key| key == component_id);
    }
    component_keys.truncate(bounds.max_component_refs as usize);
    let backbone_members = composition
        .and_then(|artifact| artifact.value.get("hard_forced"))
        .map(string_leaves)
        .unwrap_or_default();
    let mut contradictions = composition
        .and_then(|artifact| artifact.value.get("conflict_constraint_ids"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(explanation) = explanation {
        contradictions.extend(
            explanation
                .value
                .get("cores")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|core| {
                    core.get("constraint_ids")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
    }
    contradictions.sort();
    contradictions.dedup();

    GeoInspectionMetrics {
        residual_model_count,
        artifact_digests,
        backbone_members,
        contradictions,
        component_keys,
        claim_class: composition.map(|_| GeoClaimClass::CollateralComposition),
        proof_class: proof_class(stored),
    }
}

fn inspection_planes(
    stored: &StoredRun,
    section: Option<&StoredArtifact>,
    evidence: Option<&StoredArtifact>,
    composition: Option<&StoredArtifact>,
    explanation: Option<&StoredArtifact>,
) -> GeoInspectionPlanes {
    GeoInspectionPlanes {
        availability: availability_plane(stored),
        coverage: coverage_plane(stored, section),
        candidate_reach: candidate_reach_plane(section),
        admission: admission_plane(evidence),
        solver_exactness: solver_exactness_plane(composition),
        reconciliation: reconciliation_plane(stored, explanation),
        truth_quality: truth_quality_plane(section),
        cost: cost_plane(stored),
    }
}

fn answer_q1(stored: &StoredRun) -> GeoInspectionAnswer {
    answer(
        GeoInspectionQuestion::Q1,
        format!(
            "Run {} answers question hash {} for project {} at graph {}.",
            stored.run.run_id,
            stored.run.plan_ref.question_hash,
            stored.run.plan_ref.project_id,
            stored.run.plan_ref.project_graph_hash
        ),
        manifest_and_first_output_ref(stored),
    )
}

fn answer_q2(stored: &StoredRun, evidence: Option<&StoredArtifact>) -> GeoInspectionAnswer {
    if let Some(evidence) = evidence {
        let contracts = evidence
            .value
            .get("admissions")
            .and_then(Value::as_array)
            .map(|admissions| {
                admissions
                    .iter()
                    .filter_map(|admission| {
                        admission
                            .pointer("/contract/source_release")
                            .and_then(Value::as_str)
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        return answer(
            GeoInspectionQuestion::Q2,
            format!(
                "Available source releases from stored evidence: {}; proof_class={}.",
                join_or_none(contracts),
                proof_class(stored)
            ),
            vec![evidence.reference.clone()],
        );
    }
    unanswerable(
        GeoInspectionQuestion::Q2,
        CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        &stored.manifest_ref,
    )
}

fn answer_q3(stored: &StoredRun, evidence: Option<&StoredArtifact>) -> GeoInspectionAnswer {
    if let Some(evidence) = evidence {
        let mut counts = BTreeMap::<String, u64>::new();
        for disposition in evidence
            .value
            .get("admissions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|admission| admission.get("disposition").and_then(Value::as_str))
        {
            *counts.entry(disposition.to_string()).or_insert(0) += 1;
        }
        return answer(
            GeoInspectionQuestion::Q3,
            format!(
                "Rho admission dispositions from stored compilation: hard_constraint={}, soft_preference={}, diagnostic_only={}; rejected evidence is absent from the compiled artifact.",
                counts
                    .get(disposition_name(GeoEvidenceDisposition::HardConstraint))
                    .copied()
                    .unwrap_or(0),
                counts
                    .get(disposition_name(GeoEvidenceDisposition::SoftPreference))
                    .copied()
                    .unwrap_or(0),
                counts
                    .get(disposition_name(GeoEvidenceDisposition::DiagnosticOnly))
                    .copied()
                    .unwrap_or(0),
            ),
            vec![evidence.reference.clone()],
        );
    }
    unanswerable(
        GeoInspectionQuestion::Q3,
        CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        &stored.manifest_ref,
    )
}

fn answer_q4(stored: &StoredRun, section: Option<&StoredArtifact>) -> GeoInspectionAnswer {
    if let Some(section) = section {
        return answer(
            GeoInspectionQuestion::Q4,
            format!(
                "{} Solver exactness does not prove empirical accuracy.",
                candidate_reach_plane(Some(section))
            ),
            vec![section.reference.clone()],
        );
    }
    unanswerable(
        GeoInspectionQuestion::Q4,
        CANON_GEO_TILE_WORK_UNIT_VERSION,
        &stored.manifest_ref,
    )
}

fn answer_q5(
    stored: &StoredRun,
    composition: Option<&StoredArtifact>,
    bounds: &GeoInspectionBounds,
) -> GeoInspectionAnswer {
    if let Some(composition) = composition {
        return answer(
            GeoInspectionQuestion::Q5,
            format!(
                "{} Component detail is bounded to {} refs; truncated={}.",
                solver_exactness_plane(Some(composition)),
                bounds.max_component_refs,
                bounds.component_refs_truncated
            ),
            vec![composition.reference.clone()],
        );
    }
    unanswerable(
        GeoInspectionQuestion::Q5,
        CANON_GEO_COMPOSITION_VERSION,
        &stored.manifest_ref,
    )
}

fn answer_q6(
    stored: &StoredRun,
    composition: Option<&StoredArtifact>,
    explanation: Option<&StoredArtifact>,
) -> GeoInspectionAnswer {
    let Some(explanation) = explanation else {
        return unanswerable(
            GeoInspectionQuestion::Q6,
            CANON_GEO_EXPLANATION_VERSION,
            &stored.manifest_ref,
        );
    };
    let status = composition
        .and_then(|artifact| artifact.value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let residual = composition
        .and_then(|artifact| artifact.value.pointer("/summary/residual_model_count"))
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let cores = explanation
        .value
        .get("cores")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut refs = vec![explanation.reference.clone()];
    if let Some(composition) = composition {
        refs.push(composition.reference.clone());
    }
    answer(
        GeoInspectionQuestion::Q6,
        format!(
            "Stored status={status}, residual_model_count={residual}, explanation_cores={cores}. Matching payload digests are byte identity only; semantic reconciliation requires its own artifact/verdict."
        ),
        refs,
    )
}

fn answer_q7(stored: &StoredRun) -> GeoInspectionAnswer {
    let reused = stored
        .run
        .project_run_report
        .as_ref()
        .map(|report| report.resumed_nodes.len())
        .unwrap_or(0);
    let invalidated = stored
        .run
        .project_run_report
        .as_ref()
        .map(|report| report.invalidated_nodes.len())
        .unwrap_or(0);
    answer(
        GeoInspectionQuestion::Q7,
        format!(
            "Reusable prior work is stored in run receipts: resumed_nodes={reused}, invalidated_nodes={invalidated}."
        ),
        manifest_and_first_output_ref(stored),
    )
}

fn manifest_and_first_output_ref(stored: &StoredRun) -> Vec<GeoRunArtifactRef> {
    let mut refs = vec![stored.manifest_ref.clone()];
    if let Some(artifact) = stored.artifacts.values().next() {
        refs.push(artifact.reference.clone());
    }
    refs
}

fn answer_q8(
    stored: &StoredRun,
    next_evidence: Option<&StoredArtifact>,
    separation: Option<&StoredArtifact>,
    propagation: Option<&StoredArtifact>,
    recommend_next: bool,
) -> GeoInspectionAnswer {
    if let Some(next_evidence) = next_evidence {
        let stop = next_evidence
            .value
            .get("stop")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let frontier = next_evidence
            .value
            .get("frontier")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let mut refs = vec![next_evidence.reference.clone()];
        if let Some(separation) = separation {
            refs.push(separation.reference.clone());
        }
        if let Some(propagation) = propagation {
            refs.push(propagation.reference.clone());
        }
        return answer(
            GeoInspectionQuestion::Q8,
            format!(
                "Stored next-evidence frontier has {frontier} nondominated actions and stop={stop}; recommend_next={recommend_next} reads this artifact only."
            ),
            refs,
        );
    }
    unanswerable(
        GeoInspectionQuestion::Q8,
        CANON_GEO_NEXT_EVIDENCE_VERSION,
        &stored.manifest_ref,
    )
}

fn answer(
    question: GeoInspectionQuestion,
    answer: String,
    mut artifact_refs: Vec<GeoRunArtifactRef>,
) -> GeoInspectionAnswer {
    artifact_refs.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    artifact_refs.dedup_by(|left, right| left.artifact_id == right.artifact_id);
    GeoInspectionAnswer {
        question,
        answer,
        artifact_refs,
        abstention: None,
    }
}

fn unanswerable(
    question: GeoInspectionQuestion,
    missing_contract: &str,
    manifest_ref: &GeoRunArtifactRef,
) -> GeoInspectionAnswer {
    GeoInspectionAnswer {
        question,
        answer: format!(
            "inspect_question_unanswerable: stored artifact for {missing_contract} is absent; rerun canon geo run with the generated stage that publishes it."
        ),
        artifact_refs: vec![manifest_ref.clone()],
        abstention: Some(GeoInspectionAbstention {
            code: GeoInspectErrorCode::InspectQuestionUnanswerable,
            message: "stored artifact is absent from this run".to_string(),
            missing_contract: missing_contract.to_string(),
        }),
    }
}

fn inspection_bounds(
    component_id: Option<String>,
    composition: Option<&StoredArtifact>,
) -> GeoInspectionBounds {
    let component_ref_count = composition
        .and_then(|artifact| artifact.value.get("factorization"))
        .and_then(Value::as_array)
        .map_or(0, |components| components.len() as u64);
    GeoInspectionBounds {
        component_id,
        max_component_refs: DEFAULT_MAX_COMPONENT_REFS,
        component_ref_count,
        component_refs_truncated: component_ref_count > DEFAULT_MAX_COMPONENT_REFS,
    }
}

fn availability_plane(stored: &StoredRun) -> String {
    format!(
        "status={:?}; completed_outputs={}; blockers={}",
        stored.run.status,
        stored.run.output_refs.len(),
        stored.run.blockers.len()
    )
}

fn coverage_plane(stored: &StoredRun, section: Option<&StoredArtifact>) -> String {
    let candidate_count = section
        .and_then(|artifact| artifact.value.pointer("/candidate_reach/candidate_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    format!(
        "grain_states={}; bounded_candidate_count={candidate_count}",
        stored.run.grain_states.len()
    )
}

fn candidate_reach_plane(section: Option<&StoredArtifact>) -> String {
    if let Some(section) = section {
        let status = section
            .value
            .pointer("/candidate_reach/status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let truth = section
            .value
            .pointer("/candidate_reach/truth_reach_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let reason = section
            .value
            .pointer("/candidate_reach/reason")
            .and_then(Value::as_str)
            .unwrap_or("missing_reason");
        return format!("candidate_reach={status}; truth_reach={truth}; reason={reason}");
    }
    format!("missing stored {CANON_GEO_TILE_WORK_UNIT_VERSION}")
}

fn admission_plane(evidence: Option<&StoredArtifact>) -> String {
    if let Some(evidence) = evidence {
        let mut counts = BTreeMap::<String, u64>::new();
        for disposition in evidence
            .value
            .get("admissions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|admission| admission.get("disposition").and_then(Value::as_str))
        {
            *counts.entry(disposition.to_string()).or_insert(0) += 1;
        }
        return format!(
            "hard_constraint={}; soft_preference={}; diagnostic_only={}",
            counts
                .get(disposition_name(GeoEvidenceDisposition::HardConstraint))
                .copied()
                .unwrap_or(0),
            counts
                .get(disposition_name(GeoEvidenceDisposition::SoftPreference))
                .copied()
                .unwrap_or(0),
            counts
                .get(disposition_name(GeoEvidenceDisposition::DiagnosticOnly))
                .copied()
                .unwrap_or(0)
        );
    }
    format!("missing stored {CANON_GEO_EVIDENCE_COMPILATION_VERSION}")
}

fn solver_exactness_plane(composition: Option<&StoredArtifact>) -> String {
    if let Some(composition) = composition {
        let status = composition
            .value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let residual = composition
            .value
            .pointer("/summary/residual_model_count")
            .and_then(Value::as_u64)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let residual_complete = composition
            .value
            .pointer("/summary/residual_model_count_complete")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let residual_saturated = composition
            .value
            .pointer("/summary/residual_model_count_saturated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let exact_components = composition
            .value
            .get("factorization")
            .and_then(Value::as_array)
            .map(|components| {
                components
                    .iter()
                    .filter(|component| {
                        component
                            .get("exact")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);
        return format!(
            "status={status}; residual_model_count={residual}; residual_complete={residual_complete}; residual_saturated={residual_saturated}; exact_components={exact_components}"
        );
    }
    format!("missing stored {CANON_GEO_COMPOSITION_VERSION}")
}

fn reconciliation_plane(stored: &StoredRun, explanation: Option<&StoredArtifact>) -> String {
    let explanation_state = if let Some(explanation) = explanation {
        let cores = explanation
            .value
            .get("cores")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        format!("explanation_cores={cores}")
    } else {
        format!("missing {CANON_GEO_EXPLANATION_VERSION}")
    };
    format!(
        "{explanation_state}; artifact_digest_match_is_not_semantic_reconciliation; output_refs={}",
        stored.run.output_refs.len()
    )
}

fn truth_quality_plane(section: Option<&StoredArtifact>) -> String {
    if let Some(section) = section {
        let truth = section
            .value
            .pointer("/candidate_reach/truth_reach_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return format!("{truth}; exact solver output is representation-relative and not accuracy");
    }
    "unverified; no stored reach artifact".to_string()
}

fn cost_plane(stored: &StoredRun) -> String {
    let counters = stored
        .run
        .deterministic_usage
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    if counters.is_empty() {
        "deterministic_usage=none".to_string()
    } else {
        format!("deterministic_usage={}", counters.join(","))
    }
}

fn proof_class(stored: &StoredRun) -> String {
    let acquisition_classes = stored
        .run
        .acquisition_satisfactions
        .iter()
        .map(|satisfaction| serde_scalar_name(&satisfaction.proof_class))
        .collect::<BTreeSet<_>>();
    if !acquisition_classes.is_empty() {
        return acquisition_classes
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
    }
    let source_datasets = stored
        .artifacts
        .values()
        .flat_map(|artifact| collect_source_datasets(&artifact.value))
        .collect::<BTreeSet<_>>();
    if source_datasets.iter().any(|source| {
        source.starts_with("fixture.") || source.contains("fixture") || source.contains("not_live")
    }) {
        "fixture".to_string()
    } else if source_datasets.is_empty() {
        "unverified_retained".to_string()
    } else {
        "retained".to_string()
    }
}

fn serde_scalar_name<T>(value: &T) -> String
where
    T: Serialize + fmt::Debug,
{
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{value:?}"))
}

fn collect_source_datasets(value: &Value) -> Vec<String> {
    match value {
        Value::Object(map) => {
            let mut out = Vec::new();
            if let Some(source) = map.get("source_dataset").and_then(Value::as_str) {
                out.push(source.to_string());
            }
            for child in map.values() {
                out.extend(collect_source_datasets(child));
            }
            out
        }
        Value::Array(values) => values.iter().flat_map(collect_source_datasets).collect(),
        _ => Vec::new(),
    }
}

fn string_leaves(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_string_leaves(value, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_string_leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(value) => out.push(value.clone()),
        Value::Array(values) => {
            for value in values {
                collect_string_leaves(value, out);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                collect_string_leaves(value, out);
            }
        }
        _ => {}
    }
}

fn disposition_name(disposition: GeoEvidenceDisposition) -> &'static str {
    match disposition {
        GeoEvidenceDisposition::HardConstraint => "hard_constraint",
        GeoEvidenceDisposition::SoftPreference => "soft_preference",
        GeoEvidenceDisposition::DiagnosticOnly => "diagnostic_only",
    }
}

fn join_or_none(values: Vec<&str>) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}

fn question_id(question: GeoInspectionQuestion) -> String {
    match question {
        GeoInspectionQuestion::Q1 => "Q1",
        GeoInspectionQuestion::Q2 => "Q2",
        GeoInspectionQuestion::Q3 => "Q3",
        GeoInspectionQuestion::Q4 => "Q4",
        GeoInspectionQuestion::Q5 => "Q5",
        GeoInspectionQuestion::Q6 => "Q6",
        GeoInspectionQuestion::Q7 => "Q7",
        GeoInspectionQuestion::Q8 => "Q8",
    }
    .to_string()
}

fn inspection_questions() -> [GeoInspectionQuestion; 8] {
    [
        GeoInspectionQuestion::Q1,
        GeoInspectionQuestion::Q2,
        GeoInspectionQuestion::Q3,
        GeoInspectionQuestion::Q4,
        GeoInspectionQuestion::Q5,
        GeoInspectionQuestion::Q6,
        GeoInspectionQuestion::Q7,
        GeoInspectionQuestion::Q8,
    ]
}

fn validate_plan_ref(plan_ref: &GeoRunPlanRef) -> Result<(), GeoInspectError> {
    validate_nonempty("plan_ref.plan_id", &plan_ref.plan_id)?;
    validate_digest("plan_ref.semantic_hash", &plan_ref.semantic_hash)?;
    validate_nonempty("plan_ref.project_id", &plan_ref.project_id)?;
    validate_digest("plan_ref.project_graph_hash", &plan_ref.project_graph_hash)?;
    validate_digest("plan_ref.question_hash", &plan_ref.question_hash)?;
    validate_digest("plan_ref.capabilities_hash", &plan_ref.capabilities_hash)?;
    validate_digest(
        "plan_ref.inventory_planning_hash",
        &plan_ref.inventory_planning_hash,
    )?;
    validate_digest("plan_ref.profile_hash", &plan_ref.profile_hash)?;
    validate_digest(
        "plan_ref.budget_planning_hash",
        &plan_ref.budget_planning_hash,
    )
}

fn validate_artifact_ref(reference: &GeoRunArtifactRef) -> Result<(), GeoInspectError> {
    validate_nonempty("artifact_refs.node_id", &reference.node_id)?;
    validate_nonempty("artifact_refs.binding_id", &reference.binding_id)?;
    validate_nonempty("artifact_refs.artifact_id", &reference.artifact_id)?;
    validate_digest("artifact_refs.content_digest", &reference.content_digest)?;
    if reference.media_type != GEO_RUN_JSON_MEDIA_TYPE {
        return Err(GeoInspectError::invalid(
            "Geo inspection artifact ref must point to JSON bytes",
            [
                ("artifact_id", reference.artifact_id.as_str()),
                ("media_type", reference.media_type.as_str()),
            ],
        ));
    }
    validate_nonempty(
        "artifact_refs.contract_version",
        &reference.contract_version,
    )?;
    if reference.byte_count == 0 {
        return Err(GeoInspectError::invalid(
            "Geo inspection artifact ref byte_count must be positive",
            [("artifact_id", reference.artifact_id.as_str())],
        ));
    }
    Ok(())
}

fn validate_nonempty(field: &str, value: &str) -> Result<(), GeoInspectError> {
    if value.trim().is_empty() {
        return Err(GeoInspectError::invalid(
            "Geo inspection field must be nonempty",
            [("field", field)],
        ));
    }
    Ok(())
}

fn validate_digest(field: &str, value: &str) -> Result<(), GeoInspectError> {
    digest_hex(field, value).map(|_| ())
}

fn digest_hex<'a>(field: &str, value: &'a str) -> Result<&'a str, GeoInspectError> {
    value
        .strip_prefix("blake3:")
        .filter(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .ok_or_else(|| {
            GeoInspectError::invalid(
                "Geo inspection digest must be lowercase BLAKE3",
                [("field", field), ("value", value)],
            )
        })
}
