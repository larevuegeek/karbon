use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is not equal to a given value.
///
/// Equivalent to Symfony's `NotEqualTo` constraint.
pub struct NotEqualTo {
    pub value: String,
    pub message: String,
}

impl NotEqualTo {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            message: "This value should not be equal to {{ compared_value }}.".to_string(),
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl Constraint for NotEqualTo {
    fn validate(&self, value: &str) -> ConstraintResult {
        if value == self.value {
            return Err(ConstraintViolation::new(
                self.name(),
                self.message.replace("{{ compared_value }}", &self.value),
                value,
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "NotEqualTo"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_different_values() {
        let constraint = NotEqualTo::new("forbidden");
        assert!(constraint.validate("allowed").is_ok());
        assert!(constraint.validate("").is_ok());
    }

    #[test]
    fn test_same_value() {
        let constraint = NotEqualTo::new("forbidden");
        assert!(constraint.validate("forbidden").is_err());
    }
}
