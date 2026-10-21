#![allow(dead_code)]
//! Digest-bound retained development inputs. Loading these fixtures never solves.

#[path = "retained_archive.rs"]
mod retained_archive;

use canon::geo::*;
use canon::project::{
    ProjectExtensionDagNode, ProjectExtensionDagOutput, ProjectExtensionDagRequest,
    ProjectPlanNodeKind, ProjectPlanOutputMaterialization, compile_extension_project_plan,
};
use retained_archive::{RetainedDigest, extract_tar_gz, read_json_gz_files, verify_archive};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

#[derive(Clone)]
pub struct RetainedAnswerFixture {
    pub case_id: String,
    pub catalog_row: Value,
    pub question: GeoQuestion,
    pub config: GeoAnswerConfig,
    pub section: GeoTileWorkUnitArtifact,
    pub compilation: GeoEvidenceCompilationArtifact,
    pub composition: GeoCompositionArtifact,
    pub explanation: GeoExplanationArtifact,
    pub next_evidence: Option<GeoNextEvidenceArtifact>,
    pub plan: GeoPlan,
    pub retained_run: GeoRun,
    pub source_files: BTreeMap<String, Vec<u8>>,
}

impl RetainedAnswerFixture {
    pub fn inputs(&self) -> GeoAnswerInputs<'_> {
        GeoAnswerInputs {
            section: &self.section,
            compilation: &self.compilation,
            composition: &self.composition,
            explanation: &self.explanation,
            next_evidence: self.next_evidence.as_ref(),
        }
    }

    pub fn claim_inputs(&self) -> GeoAnswerClaimInputs<'_> {
        GeoAnswerClaimInputs {
            section: &self.section,
            compilation: &self.compilation,
            composition: &self.composition,
            explanation: &self.explanation,
        }
    }

    pub fn answer(&self) -> GeoAnswerArtifact {
        project_answer(&self.inputs(), &self.config, &GeoAnswerLimits::default())
            .unwrap_or_else(|e| panic!("retained projection {}: {e}", self.case_id))
    }

    pub fn answer_with_execution(&self) -> GeoAnswerArtifact {
        project_answer_with_execution(
            &self.inputs(),
            &self.config,
            &GeoAnswerLimits::default(),
            &self.retained_run,
        )
        .unwrap_or_else(|e| panic!("retained execution projection {}: {e}", self.case_id))
    }

    /// Exercise registered stages before automatic question-to-answer planning
    /// lands. Reuse the shared public DAG compiler; never reproduce its hashes.
    pub fn registered_answer_plan(&self) -> GeoPlan {
        let mut plan = self.plan.clone();
        let prefix = match self.config.entity_level {
            GeoControlEntityLevel::Building => "geo.building",
            _ => "geo.parcel",
        };
        let answer_id = format!("{prefix}.answer");
        let template = plan
            .project_plan
            .nodes
            .iter()
            .find(|n| n.node_id == format!("{prefix}.explain"))
            .unwrap()
            .clone();
        let mut nodes = plan
            .project_plan
            .nodes
            .iter()
            .map(|node| ProjectExtensionDagNode {
                node_id: node.node_id.clone(),
                kind: node.kind,
                class: node.class,
                command: node.command.clone(),
                dependencies: node.dependencies.clone(),
                content_hash_inputs: node
                    .content_hash_inputs
                    .iter()
                    .filter(|r| !r.ref_id.starts_with("node."))
                    .cloned()
                    .collect(),
                outputs: node
                    .outputs
                    .iter()
                    .map(|o| ProjectExtensionDagOutput {
                        output_id: o.output_id.clone(),
                        path: o.path.clone(),
                        materialization: o.materialization,
                    })
                    .collect(),
                limits: node.limits.clone(),
                cache_eligible: node.cache.eligible,
                side_effects: node.side_effects.clone(),
                refusal_conditions: node.refusal_conditions.clone(),
            })
            .collect::<Vec<_>>();
        let mut dependencies = ["section", "compile_evidence", "solve", "explain"]
            .iter()
            .map(|suffix| format!("{prefix}.{suffix}"))
            .collect::<Vec<_>>();
        if self.config.answer_scope == GeoAnswerScope::FullWorkflow {
            dependencies.push(format!("{prefix}.next_evidence"));
        }
        nodes.push(ProjectExtensionDagNode {
            node_id: answer_id.clone(),
            kind: ProjectPlanNodeKind::Solve,
            class: template.class,
            command: "canon.geo.stage.answer.v0".into(),
            dependencies,
            content_hash_inputs: template
                .content_hash_inputs
                .iter()
                .filter(|r| !r.ref_id.starts_with("node."))
                .cloned()
                .collect(),
            outputs: vec![ProjectExtensionDagOutput {
                output_id: "answer".into(),
                path: format!(
                    "geo/{}/answer.json",
                    if prefix.ends_with("building") {
                        "building"
                    } else {
                        "parcel"
                    }
                ),
                materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
            }],
            limits: template.limits.clone(),
            cache_eligible: true,
            side_effects: template.side_effects,
            refusal_conditions: template.refusal_conditions,
        });
        let mut request = ProjectExtensionDagRequest::offline_read_only(
            plan.project_plan.project_id.clone(),
            plan.project_plan.manifest_digest.clone(),
            plan.project_plan.lock_digest.clone(),
            nodes,
        );
        request.plan_artifact_path = plan.project_plan.plan_artifact_path.clone();
        plan.project_plan = compile_extension_project_plan(request).unwrap();
        let mut overlay = plan
            .geo_nodes
            .iter()
            .find(|n| n.stage == GeoPlanStage::ExplainResidual)
            .unwrap()
            .clone();
        overlay.project_node_id = answer_id.clone();
        overlay.stage = GeoPlanStage::ProjectCurrentAnswer;
        overlay.entity_level = Some(self.config.entity_level);
        overlay.expected_output_contract = CANON_GEO_ANSWER_VERSION.into();
        overlay.answer_config = Some(self.config.clone());
        overlay.incidence_factorization_required = false;
        overlay.exact_solve_scope = None;
        plan.geo_nodes.push(overlay);
        plan.geo_nodes
            .sort_by(|a, b| a.project_node_id.cmp(&b.project_node_id));
        for grain in &mut plan.grain_outcomes {
            if grain.entity_level == self.config.entity_level {
                grain.project_node_ids.push(answer_id.clone());
                grain.project_node_ids.sort();
            }
        }
        plan.semantic_hash = geo_plan_semantic_hash(&plan).unwrap();
        plan.plan_id = format!(
            "{}:{}",
            plan.version,
            plan.semantic_hash.trim_start_matches("blake3:")
        );
        validate_geo_plan(&plan).unwrap();
        plan
    }

    pub fn write_caller_inputs(&self, root: &Path) -> Vec<String> {
        let mut arguments = Vec::new();
        for binding in self.catalog_row["run_bindings"]
            .as_array()
            .expect("retained bindings")
        {
            let member = binding["path"].as_str().unwrap();
            let destination = root.join(member);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(&destination, &self.source_files[member]).unwrap();
            arguments.push("--input".into());
            arguments.push(format!(
                "{}:{}={}",
                binding["node_id"].as_str().unwrap(),
                binding["binding_id"].as_str().unwrap(),
                destination.display()
            ));
        }
        arguments
    }

    /// Synthetic test transformations must rebind declared digests rather than
    /// bypass the projector's input-consistency checks.
    pub fn rebind_changed_evidence(&mut self) {
        let mut normalized = self.compilation.clone();
        normalized.composition_request =
            canonicalize_composition_request(&normalized.composition_request).unwrap();
        normalized
            .admissions
            .sort_by(|a, b| a.observation_id.cmp(&b.observation_id));
        for admission in &mut normalized.admissions {
            admission.source_records.sort();
            admission.generated_ids.sort();
            admission.contract.source_lineage_ids.sort();
        }
        let bytes = canonical_evidence_compilation_bytes(&normalized).unwrap();
        let digest = blake3::hash(&bytes).to_hex().to_string();
        self.composition
            .evidence_compilation
            .as_mut()
            .unwrap()
            .blake3 = digest.clone();
        self.explanation.evidence_blake3 = format!("blake3:{digest}");
        let request =
            canonicalize_composition_request(&self.compilation.composition_request).unwrap();
        self.explanation.request_blake3 = format!(
            "blake3:{}",
            blake3::hash(&serde_json::to_vec(&request).unwrap())
        );
        if let Some(next) = &mut self.next_evidence {
            next.composition_blake3 = format!(
                "blake3:{}",
                blake3::hash(&canonical_composition_bytes(&self.composition).unwrap())
            );
        }
    }
}

pub fn real_answer_instance() -> GeoAnswerArtifact {
    retained_answer_fixture("cornerstone.national_building_supported").answer_with_execution()
}

pub fn retained_answer_fixture(case_id: &str) -> RetainedAnswerFixture {
    static CACHE: OnceLock<Mutex<BTreeMap<String, RetainedAnswerFixture>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap();
    cache
        .entry(case_id.into())
        .or_insert_with(|| load_fixture(case_id))
        .clone()
}

fn decode<T: DeserializeOwned>(files: &BTreeMap<String, Vec<u8>>, member: &str) -> T {
    serde_json::from_slice(
        files
            .get(member)
            .unwrap_or_else(|| panic!("missing_member: {member}")),
    )
    .unwrap_or_else(|e| panic!("retained member {member}: {e}"))
}

fn load_fixture(case_id: &str) -> RetainedAnswerFixture {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog: Value = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/geo/replay/catalog.json")).unwrap(),
    )
    .unwrap();
    let row = catalog["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case_id"] == case_id)
        .unwrap_or_else(|| panic!("missing retained case {case_id}"))
        .clone();
    assert_eq!(row["proof_class"], "retained");
    let archive = root.join(row["archive"]["path"].as_str().unwrap());
    let digest = RetainedDigest::Sha256(row["archive"]["sha256"].as_str().unwrap().into());
    verify_archive(&archive, &digest).unwrap();
    let files = if row["archive"]["format"] == "json_gz_files" {
        read_json_gz_files(&archive, &digest).unwrap()
    } else {
        // Archive member names come from the retained run's references. The first
        // extraction reads only the immutable run descriptor, then its four inputs.
        let directory = tempfile::Builder::new()
            .prefix("canon-answer-retained-")
            .tempdir()
            .unwrap()
            .keep();
        let run_member = row["retained_run"].as_str().unwrap();
        extract_tar_gz(&archive, &digest, &[run_member], &directory).unwrap();
        let run: Value =
            serde_json::from_slice(&fs::read(directory.join(run_member)).unwrap()).unwrap();
        let mut members = row["member_sha256"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for reference in run["output_refs"].as_array().unwrap() {
            if !matches!(
                reference["output_id"].as_str().unwrap(),
                "section" | "compile_evidence" | "solve" | "explanation" | "next_evidence"
            ) {
                continue;
            }
            let hex = reference["content_digest"]
                .as_str()
                .unwrap()
                .strip_prefix("blake3:")
                .unwrap();
            members.push(format!(
                "{}run/.canon/geo-run/artifacts/cas/{hex}.bin",
                row["case_root"].as_str().unwrap()
            ));
        }
        members.sort();
        members.dedup();
        let member_refs = members.iter().map(String::as_str).collect::<Vec<_>>();
        extract_tar_gz(&archive, &digest, &member_refs, &directory).unwrap();
        members
            .into_iter()
            .map(|name| {
                let bytes = fs::read(directory.join(&name)).unwrap();
                (name, bytes)
            })
            .collect()
    };
    for (member, expected) in row["member_sha256"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(&files[member])),
            expected.as_str().unwrap(),
            "retained member hash {member}"
        );
    }
    let run: Value = decode(&files, row["retained_run"].as_str().unwrap());
    let artifact = |output: &str| -> Vec<u8> {
        let reference = run["output_refs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["output_id"] == output)
            .unwrap();
        let bytes = if row["archive"]["format"] == "json_gz_files" {
            files[&format!(
                "{}workspace/geo/parcel/{output}.json",
                row["case_root"].as_str().unwrap()
            )]
                .clone()
        } else {
            let hex = reference["content_digest"]
                .as_str()
                .unwrap()
                .strip_prefix("blake3:")
                .unwrap();
            files[&format!(
                "{}run/.canon/geo-run/artifacts/cas/{hex}.bin",
                row["case_root"].as_str().unwrap()
            )]
                .clone()
        };
        assert_eq!(
            format!("blake3:{}", blake3::hash(&bytes)),
            reference["content_digest"].as_str().unwrap(),
            "retained artifact {output}"
        );
        bytes
    };
    let question: GeoQuestion = decode(&files, row["plan_inputs"]["question"].as_str().unwrap());
    let compilation: GeoEvidenceCompilationArtifact =
        serde_json::from_slice(&artifact("compile_evidence")).unwrap();
    let config = GeoAnswerConfig {
        entity_level: match compilation.composition_request.profile.selection_level {
            GeoEntityLevel::Parcel => GeoControlEntityLevel::Parcel,
            GeoEntityLevel::Building => GeoControlEntityLevel::Building,
            GeoEntityLevel::Property => GeoControlEntityLevel::Property,
            GeoEntityLevel::PoiUnit => GeoControlEntityLevel::Poi,
        },
        answer_scope: GeoAnswerScope::CurrentAnswer,
        requested_claim_classes: question.requested_claim_classes.clone(),
        decision_policy: question.decision_policy.clone(),
        question_hash: question_semantic_hash(&question).unwrap(),
        question_id: question.question_id.clone(),
        query_as_of: question.query_as_of.clone(),
    };
    let plan = decode(&files, row["retained_plan"].as_str().unwrap());
    let retained_run = serde_json::from_value(run.clone()).unwrap();
    RetainedAnswerFixture {
        case_id: case_id.into(),
        catalog_row: row.clone(),
        question,
        config,
        section: serde_json::from_slice(&artifact("section")).unwrap(),
        compilation,
        composition: serde_json::from_slice(&artifact("solve")).unwrap(),
        explanation: serde_json::from_slice(&artifact("explanation")).unwrap(),
        next_evidence: None,
        plan,
        retained_run,
        source_files: files,
    }
}
