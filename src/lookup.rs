use crate::{InputValues, Registry, ResolveResult};
use std::error::Error;

pub fn resolve_values(
    _registry: &Registry,
    _input_values: &InputValues,
) -> Result<ResolveResult, Box<dyn Error>> {
    // TODO: Implement lookup logic
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn lookup_placeholder() {
        // TODO: Add lookup tests
    }
}
