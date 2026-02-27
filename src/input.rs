use crate::InputValues;
use std::error::Error;

pub fn parse_input(_input_path: &str, _column: &str) -> Result<InputValues, Box<dyn Error>> {
    // TODO: Implement input parsing
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn input_placeholder() {
        // TODO: Add input parsing tests
    }
}
