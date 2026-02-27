use crate::{RegistryMeta, ResolveResult};
use std::error::Error;

pub fn emit_json(
    _registry_meta: &RegistryMeta,
    _result: &ResolveResult,
) -> Result<String, Box<dyn Error>> {
    // TODO: Implement JSON emission (bd-2p4 agent's target)
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_emission_placeholder() {
        // TODO: Add JSON emission tests
    }
}
