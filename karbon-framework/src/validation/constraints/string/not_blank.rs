use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is not blank (not empty and not only whitespace).
///
/// Equivalent to Symfony's `NotBlank` constraint.
pub struct NotBlank {
    pub message: String,
}

impl Default for NotBlank {
    fn default() -> Self {
        Self {
            message: "This value should not be blank.".to_string(),
        }
    }
}

impl NotBlank {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl Constraint for NotBlank {
    fn validate(&self, value: &str) -> ConstraintResult {
        if value.trim().is_empty() {
            return Err(ConstraintViolation::new(
                self.name(),
                &self.message,
                value,
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "NotBlank"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_values() {
        let constraint = NotBlank::new();
        assert!(constraint.validate("hello").is_ok());
        assert!(constraint.validate("0").is_ok());
        assert!(constraint.validate("false").is_ok());
        assert!(constraint.validate(" a ").is_ok());
    }

    #[test]
    fn test_blank_values() {
        let constraint = NotBlank::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("   ").is_err());
        assert!(constraint.validate("\t").is_err());
        assert!(constraint.validate("\n").is_err());
    }

    #[test]
    fn test_custom_message() {
        let constraint = NotBlank::new().with_message("Le champ est requis.");
        let err = constraint.validate("").unwrap_err();
        assert_eq!(err.message, "Le champ est requis.");
    }
}
