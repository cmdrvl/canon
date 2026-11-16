#![forbid(unsafe_code)]

//! Deterministic provenance material attached to evidence-session registry
//! writes.  The registry writer stores the small `session_authority` object;
//! the complete closure is retained by the session commit publication.

use crate::{entity::session::SessionSolveCluster, witness::hash_bytes};
use serde_json::{Value, json};

pub const SESSION_AUTHORITY_VERSION: &str = "canon.entity.session.authority.v1";
pub const SESSION_CLOSURE_VERSION: &str = "canon.evidence_closure.v0";
pub const SESSION_CLOSURE_SET_VERSION: &str = "canon.evidence_closure_set.v0";
pub const DEFAULT_SENSITIVITY: &str = "not_public";

/// The per-outcome closure that is written beside the commit receipt.  The
/// manifest is a JSON value intentionally: later closure versions may add
/// typed source handles without changing the registry entry schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionClosure {
    pub outcome_ref: String,
    pub closure_hash: String,
    pub manifest: Value,
}

/// A registry-entry authority and its matching closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAuthorityMaterial {
    pub authority: Value,
    pub closure: SessionClosure,
}

/// Stable inputs shared by every provenance outcome emitted for a session
/// cluster.  The registry version is retained as context even though it is
/// intentionally excluded from closure bytes, so copying a registry does not
/// change an outcome's evidence hash.
#[derive(Debug, Clone, Copy)]
pub struct ProvenanceContext<'a> {
    pub session_id: &'a str,
    pub solve_hash: &'a str,
    pub rules_hash: &'a str,
    pub registry_id: &'a str,
    pub registry_version: &'a str,
    pub cluster: &'a SessionSolveCluster,
    pub canonical_id: Option<&'a str>,
    pub sensitivity: Option<&'a str>,
}

/// The outcome-specific subject and its retained observations.
#[derive(Debug, Clone, Copy)]
pub struct ProvenanceSubject<'a> {
    pub kind: &'a str,
    pub subject: &'a str,
    pub observation_values: &'a [Value],
}

/// Missing sensitivity is deliberately conservative.  We preserve a supplied
/// value verbatim, because the source system owns its sensitivity vocabulary;
/// Canon must never silently turn an unstated source into public evidence.
pub fn effective_sensitivity(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| DEFAULT_SENSITIVITY.to_string())
}

/// Assemble the stable closure and the small authority object used by every
/// alias emitted for one accepted session cluster.
pub fn build_authority_material(
    context: ProvenanceContext<'_>,
) -> Result<SessionAuthorityMaterial, String> {
    let kind = if context.canonical_id.is_some() {
        "attached_alias"
    } else {
        "minted_id"
    };
    let subject = outcome_subject(context.cluster);
    build_authority_material_for_subject_with_observations(
        context,
        ProvenanceSubject {
            kind,
            subject: &subject,
            observation_values: &[],
        },
    )
}

/// Assemble authority for a particular registry outcome subject.  Attached and
/// key aliases must use their exact input as the subject; a minted outcome
/// uses the cluster's stable subject instead.
pub fn build_authority_material_for_subject(
    context: ProvenanceContext<'_>,
    subject: ProvenanceSubject<'_>,
) -> Result<SessionAuthorityMaterial, String> {
    build_authority_material_for_subject_with_observations(
        context,
        ProvenanceSubject {
            observation_values: &[],
            ..subject
        },
    )
}

pub fn build_authority_material_for_subject_with_observations(
    context: ProvenanceContext<'_>,
    subject: ProvenanceSubject<'_>,
) -> Result<SessionAuthorityMaterial, String> {
    let ProvenanceContext {
        session_id,
        solve_hash,
        rules_hash,
        registry_id,
        registry_version: _,
        cluster,
        canonical_id,
        sensitivity,
    } = context;
    let ProvenanceSubject {
        kind,
        subject,
        observation_values,
    } = subject;
    let tier = cluster
        .tier
        .as_deref()
        .ok_or_else(|| "accepted session cluster has no tier".to_string())?;
    if !matches!(tier, "A" | "B") {
        return Err(format!("session authority cannot commit tier {tier}"));
    }
    if !matches!(kind, "minted_id" | "attached_alias" | "key_alias") {
        return Err(format!("unsupported session authority kind {kind}"));
    }
    if subject.trim().is_empty() {
        return Err("session authority subject cannot be empty".to_string());
    }
    let subject_hash = hash_bytes(subject.as_bytes());
    let subject_hash = subject_hash
        .strip_prefix("blake3:")
        .unwrap_or(subject_hash.as_str());
    let outcome_ref = format!("cmdrvl://canon/outcome/{registry_id}/{kind}/{subject_hash}");
    let evidence_ids = sorted_evidence_ids(&cluster.accepting_evidence_ids);
    let evidence_set_bytes = serde_json::to_vec(&evidence_ids)
        .map_err(|error| format!("serialize evidence set: {error}"))?;
    let evidence_set_hash = hash_bytes(&evidence_set_bytes);
    let sensitivity = effective_sensitivity(sensitivity);

    let observations = if observation_values.is_empty() {
        cluster
            .surfaces
            .iter()
            .map(|surface| json!({"surface": surface}))
            .collect::<Vec<_>>()
    } else {
        observation_values.to_vec()
    };
    let manifest = json!({
        "version": SESSION_CLOSURE_VERSION,
        "outcome_ref": outcome_ref,
        "kind": kind,
        "subject": subject,
        "decision": {
            "tier": tier,
            "rule": cluster.reason,
            "cluster_id": cluster.cluster_id,
        },
        "inputs": {
            "rules_hash": rules_hash,
            "registry": {"id": registry_id},
            "incumbents": canonical_id.map(|id| vec![json!({
                "canonical_id": id,
                "via": if kind == "key_alias" { "key" } else { "alias" },
                "input": subject,
                "closure_hash": Value::Null,
            })]).unwrap_or_default(),
        },
        "observations": observations,
        "evidence_ids": evidence_ids,
        "upstream": [],
        "sensitivity": sensitivity,
    });
    let closure_bytes = serde_json::to_vec(&manifest)
        .map_err(|error| format!("serialize evidence closure: {error}"))?;
    let closure_hash = hash_bytes(&closure_bytes);
    let authority = json!({
        "version": SESSION_AUTHORITY_VERSION,
        "session_id": session_id,
        "solve_hash": solve_hash,
        "rules_hash": rules_hash,
        "cluster_id": cluster.cluster_id,
        "tier": tier,
        "evidence_set_hash": evidence_set_hash,
        "evidence_ids": evidence_ids.iter().take(16).cloned().collect::<Vec<_>>(),
        "outcome_ref": outcome_ref,
        "closure_hash": closure_hash,
        "sensitivity": sensitivity,
    });
    Ok(SessionAuthorityMaterial {
        authority,
        closure: SessionClosure {
            outcome_ref,
            closure_hash,
            manifest,
        },
    })
}

pub fn sorted_evidence_ids(ids: &[String]) -> Vec<String> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    ids
}

pub fn outcome_subject(cluster: &SessionSolveCluster) -> String {
    for (namespace, values) in &cluster.keys {
        if let Some(value) = values.first() {
            return format!("{namespace}:{value}");
        }
    }
    let mut surfaces = cluster.surfaces.clone();
    surfaces.sort();
    surfaces
        .first()
        .cloned()
        .unwrap_or_else(|| cluster.cluster_id.clone())
}

pub fn closure_set_value(
    session_id: &str,
    solve_hash: &str,
    closures: &[SessionClosure],
) -> Result<(Value, String), String> {
    let mut refs = closures
        .iter()
        .map(|closure| {
            json!({
                "outcome_ref": closure.outcome_ref,
                "closure_hash": closure.closure_hash,
            })
        })
        .collect::<Vec<_>>();
    refs.sort_by(|left, right| {
        let left_key = left
            .get("outcome_ref")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_key = right
            .get("outcome_ref")
            .and_then(Value::as_str)
            .unwrap_or_default();
        left_key.cmp(right_key)
    });
    let value = json!({
        "version": SESSION_CLOSURE_SET_VERSION,
        "session_id": session_id,
        "solve_hash": solve_hash,
        "closures": refs,
    });
    let bytes =
        serde_json::to_vec(&value).map_err(|error| format!("serialize closure set: {error}"))?;
    Ok((value, hash_bytes(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn cluster(ids: &[&str]) -> SessionSolveCluster {
        SessionSolveCluster {
            cluster_id: "cluster-1".to_string(),
            outcome: "accepted".to_string(),
            tier: Some("B".to_string()),
            surfaces: vec!["Beta".to_string(), "Alpha".to_string()],
            observation_count: 2,
            keys: BTreeMap::new(),
            incumbent_canonical_id: None,
            accepting_evidence_ids: ids.iter().map(|id| (*id).to_string()).collect(),
            reason: "tier_b".to_string(),
            conflict_reasons: Vec::new(),
        }
    }

    #[test]
    fn missing_sensitivity_is_not_public() {
        assert_eq!(effective_sensitivity(None), DEFAULT_SENSITIVITY);
        assert_eq!(effective_sensitivity(Some("public")), "public");
    }

    #[test]
    fn evidence_order_changes_closure_material_but_not_surface_order() {
        let left_cluster = cluster(&["b", "a"]);
        let left = build_authority_material(ProvenanceContext {
            session_id: "session",
            solve_hash: "solve",
            rules_hash: "rules",
            registry_id: "registry",
            registry_version: "0.1.0",
            cluster: &left_cluster,
            canonical_id: None,
            sensitivity: None,
        })
        .expect("left material");
        let right_cluster = cluster(&["a"]);
        let right = build_authority_material(ProvenanceContext {
            session_id: "session",
            solve_hash: "solve",
            rules_hash: "rules",
            registry_id: "registry",
            registry_version: "0.1.0",
            cluster: &right_cluster,
            canonical_id: None,
            sensitivity: None,
        })
        .expect("right material");
        assert_ne!(left.closure.closure_hash, right.closure.closure_hash);
        assert_eq!(left.closure.manifest["evidence_ids"], json!(["a", "b"]));
    }

    #[test]
    fn closure_set_is_sorted_by_outcome_ref() {
        let first_cluster = cluster(&["a"]);
        let mut first = build_authority_material(ProvenanceContext {
            session_id: "session",
            solve_hash: "solve",
            rules_hash: "rules",
            registry_id: "registry",
            registry_version: "0.1.0",
            cluster: &first_cluster,
            canonical_id: None,
            sensitivity: None,
        })
        .expect("material");
        first.closure.outcome_ref = "b".to_string();
        let mut second = first.clone();
        second.closure.outcome_ref = "a".to_string();
        let (set, _) = closure_set_value("session", "solve", &[first.closure, second.closure])
            .expect("closure set");
        assert_eq!(set["closures"][0]["outcome_ref"], "a");
    }
}
