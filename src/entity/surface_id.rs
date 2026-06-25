//! Deterministic prepared-surface identifier derivation.
//!
//! Surface IDs are stage join keys. They are derived only from profile-scoped
//! normalized surface material and the deduped raw primary surface set; row
//! provenance is deliberately excluded.

use crate::Refusal;
use crate::entity::error::EntityRefusalKind;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_ENTITY_SURFACE_ID_VERSION: &str = "canon_entity_surface_id.v0";
pub const SURFACE_ID_HASH_ALGORITHM: &str = "blake3";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceIdMaterial {
    pub version: String,
    pub profile_id: String,
    pub normalized_view: SurfaceIdNormalizedView,
    pub raw_surfaces: Vec<String>,
}

impl SurfaceIdMaterial {
    pub fn new(
        profile_id: impl Into<String>,
        normalized_view_name: impl Into<String>,
        normalized_view_value: impl Into<String>,
        raw_surfaces: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            version: CANON_ENTITY_SURFACE_ID_VERSION.to_string(),
            profile_id: profile_id.into(),
            normalized_view: SurfaceIdNormalizedView {
                name: normalized_view_name.into(),
                value: normalized_view_value.into(),
            },
            raw_surfaces: raw_surfaces
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }

    fn validate(&self) -> Result<(), Refusal> {
        let mut missing = Vec::new();
        if self.profile_id.trim().is_empty() {
            missing.push("profile_id");
        }
        if self.normalized_view.name.trim().is_empty() {
            missing.push("normalized_view.name");
        }
        if self.normalized_view.value.trim().is_empty() {
            missing.push("normalized_view.value");
        }
        if self.raw_surfaces.is_empty() {
            missing.push("raw_surfaces");
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(EntityRefusalKind::ArtifactContract.to_refusal(
                "Prepared surface is missing required surface_id material",
                json!({ "missing": missing, "material": self }),
                None,
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceIdNormalizedView {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedSurfaceId {
    pub surface_id: String,
    pub material: SurfaceIdMaterial,
}

trait SurfaceIdHasher {
    fn algorithm(&self) -> &'static str;
    fn hash(&self, bytes: &[u8]) -> String;
}

struct Blake3SurfaceIdHasher;

impl SurfaceIdHasher for Blake3SurfaceIdHasher {
    fn algorithm(&self) -> &'static str {
        SURFACE_ID_HASH_ALGORITHM
    }

    fn hash(&self, bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }
}

pub fn derive_surface_ids(
    materials: &[SurfaceIdMaterial],
) -> Result<Vec<DerivedSurfaceId>, Refusal> {
    derive_surface_ids_with_hasher(materials, &Blake3SurfaceIdHasher)
}

fn derive_surface_ids_with_hasher(
    materials: &[SurfaceIdMaterial],
    hasher: &impl SurfaceIdHasher,
) -> Result<Vec<DerivedSurfaceId>, Refusal> {
    let mut seen: BTreeMap<String, SurfaceIdMaterial> = BTreeMap::new();
    let mut derived = Vec::with_capacity(materials.len());

    for material in materials {
        material.validate()?;
        let bytes = serde_json::to_vec(material).map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Failed to serialize surface_id material",
                json!({ "error": error.to_string() }),
                None,
            )
        })?;
        let digest = hasher.hash(&bytes);
        let surface_id = format!(
            "surf:{}:{}:{}",
            material.profile_id,
            hasher.algorithm(),
            digest
        );

        if let Some(existing) = seen.get(&surface_id) {
            if existing != material {
                return Err(EntityRefusalKind::SurfaceIdCollision.to_refusal(
                    "Prepared surfaces produced the same surface_id",
                    json!({
                        "surface_id": surface_id,
                        "hash_algorithm": hasher.algorithm(),
                        "existing_material": existing,
                        "colliding_material": material,
                        "collision_policy": "refuse_without_silent_rekey"
                    }),
                    None,
                ));
            }
        } else {
            seen.insert(surface_id.clone(), material.clone());
        }

        derived.push(DerivedSurfaceId {
            surface_id,
            material: material.clone(),
        });
    }

    Ok(derived)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RefusalCode;

    struct ConstantHasher;

    impl SurfaceIdHasher for ConstantHasher {
        fn algorithm(&self) -> &'static str {
            "constant"
        }

        fn hash(&self, _bytes: &[u8]) -> String {
            "collision".to_string()
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn EN_P004_synthetic_surface_hash_collision_refuses() {
        let materials = vec![
            SurfaceIdMaterial::new(
                "cmbs_tenant_label",
                "tenant_core",
                "sears",
                ["Sears LLC".to_string()],
            ),
            SurfaceIdMaterial::new(
                "cmbs_tenant_label",
                "tenant_core",
                "sears auto center",
                ["Sears Auto Center".to_string()],
            ),
        ];

        let refusal = derive_surface_ids_with_hasher(&materials, &ConstantHasher)
            .expect_err("distinct material with same hash refuses");

        assert_eq!(refusal.code, RefusalCode::EEntitySurfaceIdCollision);
        assert_eq!(
            refusal.detail["collision_policy"],
            "refuse_without_silent_rekey"
        );
        assert_eq!(
            refusal.detail["surface_id"],
            "surf:cmbs_tenant_label:constant:collision"
        );
    }
}
