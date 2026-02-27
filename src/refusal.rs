use crate::{CanonOutput, Outcome, Refusal, RefusalCode};

pub fn create_refusal(
    code: RefusalCode,
    message: String,
    detail: serde_json::Value,
    next_command: Option<String>,
) -> CanonOutput {
    CanonOutput {
        version: "canon.v0".to_string(),
        outcome: Outcome::Refusal,
        registry: None,
        summary: None,
        mappings: vec![],
        unresolved: vec![],
        refusal: Some(Refusal {
            code,
            message,
            detail,
            next_command,
        }),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn refusal_placeholder() {
        // TODO: Add refusal tests
    }
}
