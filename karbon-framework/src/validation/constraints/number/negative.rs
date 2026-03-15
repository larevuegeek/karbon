use crate::validation::constraints::{ConstraintResult, ConstraintViolation, NumericConstraint};

/// Validates that a value is a negative number (strictly less than zero).
///
/// Equivalent to Symfony's `Negative` constraint.
pub struct Negative {
    pub message: String,
}

impl Default for Negative {
    fn default() -> Self {
        Self {
            message: "This value should be negative.".to_string(),
        }
    }
}

impl Negative {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl NumericConstraint for Negative {
    fn validate_f64(&self, value: f64) -> ConstraintResult {
        if value >= 0.0 {
            return Err(ConstraintViolation::new(
                self.name(),
                &self.message,
                value.to_string(),
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Negative"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negative_values() {
        let constraint = Negative::new();
        assert!(constraint.validate_f64(-1.0).is_ok());
        assert!(constraint.validate_f64(-0.001).is_ok());
        assert!(constraint.validate_f64(-1000.0).is_ok());
    }

    #[test]
    fn test_non_negative_values() {
        let constraint = Negative::new();
        assert!(constraint.validate_f64(0.0).is_err());
        assert!(constraint.validate_f64(1.0).is_err());
        assert!(constraint.validate_f64(0.001).is_err());
    }
}
