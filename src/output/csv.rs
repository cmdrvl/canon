use crate::{InputValues, ResolveResult};
use std::error::Error;

pub fn emit_csv(
    _input_path: &str,
    _input_values: &InputValues,
    _result: &ResolveResult,
    _canon_column: &str,
) -> Result<String, Box<dyn Error>> {
    // TODO: Implement CSV emission (bd-37k agent's target)
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn csv_emission_placeholder() {
        // TODO: Add CSV emission tests
    }
}
