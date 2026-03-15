use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is equal to a given value.
///
/// Equivalent to Symfony's `EqualTo` constraint.
pub struct EqualTo {
    pub value: String,
    pub message: String,
}

impl EqualTo {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            message: "This value should be equal to {{ compared_value }}.".to_string(),
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl Constraint for EqualTo {
    fn validate(&self, value: &str) -> ConstraintResult {
        if value != self.value {
            return Err(ConstraintViolation::new(
                self.name(),
                self.message.replace("{{ compared_value }}", &self.value),
                value,
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "EqualTo"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_values() {
        let constraint = EqualTo::new("hello");
        assert!(constraint.validate("hello").is_ok());
    }

    #[test]
    fn test_not_equal_values() {
        let constraint = EqualTo::new("hello");
        assert!(constraint.validate("world").is_err());
        assert!(constraint.validate("Hello").is_err()); // case sensitive
        assert!(constraint.validate("").is_err());
    }

    #[test]
    fn test_error_message() {
        let constraint = EqualTo::new("expected");
        let err = constraint.validate("actual").unwrap_err();
        assert!(err.message.contains("expected"));
    }
}
