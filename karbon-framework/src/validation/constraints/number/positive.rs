use crate::validation::constraints::{ConstraintResult, ConstraintViolation, NumericConstraint};

/// Validates that a value is a positive number (strictly greater than zero).
///
/// Equivalent to Symfony's `Positive` constraint.
pub struct Positive {
    pub message: String,
}

impl Default for Positive {
    fn default() -> Self {
        Self {
            message: "This value should be positive.".to_string(),
        }
    }
}

impl Positive {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl NumericConstraint for Positive {
    fn validate_f64(&self, value: f64) -> ConstraintResult {
        if value <= 0.0 {
            return Err(ConstraintViolation::new(
                self.name(),
                &self.message,
                value.to_string(),
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Positive"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positive_values() {
        let constraint = Positive::new();
        assert!(constraint.validate_f64(1.0).is_ok());
        assert!(constraint.validate_f64(0.001).is_ok());
        assert!(constraint.validate_f64(1000.0).is_ok());
    }

    #[test]
    fn test_non_positive_values() {
        let constraint = Positive::new();
        assert!(constraint.validate_f64(0.0).is_err());
        assert!(constraint.validate_f64(-1.0).is_err());
        assert!(constraint.validate_f64(-0.001).is_err());
    }
}
